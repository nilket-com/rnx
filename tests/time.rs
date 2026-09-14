//! Record 0038's public contract; each child has an explicit local zone.
use std::{
	io::Write,
	path::PathBuf,
	process::{Command, Output, Stdio},
	sync::atomic::{AtomicUsize, Ordering},
	time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
	fn new() -> Self {
		let p = std::env::temp_dir().join(format!(
			"rnx-time-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::create_dir(&p).unwrap();
		Self(p)
	}
	fn command(&self) -> Command {
		let mut c = Command::new(env!("CARGO_BIN_EXE_rnx"));
		c.env_clear()
			.env("TZ", "Europe/Paris")
			.env("TERM", "dumb")
			.current_dir(&self.0);
		c
	}
	fn eval(&self, s: &str) -> Command {
		let mut c = self.command();
		c.args(["eval", s]);
		c
	}
	fn file(&self, s: &str) -> PathBuf {
		let p = self.0.join("script.rn");
		std::fs::write(&p, s).unwrap();
		p
	}
}
impl Drop for Fixture {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn out(c: &mut Command) -> String {
	let o = c.output().unwrap();
	assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
	String::from_utf8(o.stdout).unwrap().trim().into()
}
fn yes(f: &Fixture, s: &str) {
	assert_eq!(out(&mut f.eval(s)), "true", "{s}");
}
fn lit(s: &str) -> String {
	serde_json::to_string(s).unwrap()
}
fn refuses(f: &Fixture, expr: &str, word: &str) {
	yes(
		f,
		&format!(
			"match {expr} {{Ok(_) => false, Err(e) => e.contains({})}}",
			lit(word)
		),
	);
}
#[test]
fn clocks_and_the_log_stopwatch_and_file_examples() {
	let f = Fixture::new();
	let before = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_millis() as i64;
	let now: i64 = out(&mut f.eval("time::now_ms()")).parse().unwrap();
	let after = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_millis() as i64;
	assert!(before - 100 <= now && now <= after + 100);
	yes(
		&f,
		"let first=time::monotonic_ms(); let last=first; for _ in 0..10000 { let n=time::monotonic_ms(); if n < last { panic!(\"clock decreased\"); } last=n; } first < 100 && last >= first",
	);
	yes(
		&f,
		r#"let ms=time::now_ms(); time::parse(time::rfc3339(ms,"UTC")?)? == ms"#,
	);
	let p = f.0.join("old");
	let file = std::fs::File::create(&p).unwrap();
	// FileTimes is cross-platform. The separate nanosecond fixture is Unix:
	// Windows timestamps cannot represent the one-nanosecond fixture exactly.
	file.set_times(std::fs::FileTimes::new().set_modified(UNIX_EPOCH - Duration::from_secs(1)))
		.unwrap();
	yes(
		&f,
		&format!(
			"let m=fs::metadata({})?.modified_ms; m == -1000 && time::now_ms()-m > 0 && time::parts(m,\"UTC\")?.year == 1969",
			lit(p.to_str().unwrap())
		),
	);
	#[cfg(unix)]
	{
		file.set_times(
			std::fs::FileTimes::new().set_modified(UNIX_EPOCH - Duration::from_nanos(1)),
		)
		.unwrap();
		assert_eq!(
			file.metadata().unwrap().modified().unwrap(),
			UNIX_EPOCH - Duration::from_nanos(1)
		);
		yes(
			&f,
			&format!(
				"fs::metadata({})?.modified_ms == time::parse(\"1969-12-31T23:59:59.999999999Z\")? && fs::metadata({})?.modified_ms == -1",
				lit(p.to_str().unwrap()),
				lit(p.to_str().unwrap())
			),
		);
	}
}
#[test]
fn every_calendar_round_trip_and_zone_offset() {
	let f = Fixture::new();
	let moments = [
		-377705023201000i64,
		-377705023200999,
		-1,
		0,
		1,
		253402207200999,
		1774744200000,
		1792888200000,
		1792891800000,
	];
	for zone in [
		"UTC",
		"+02:00",
		"-23:59",
		"+23:59",
		"Europe/Paris",
		"America/Chicago",
		"local",
	] {
		for ms in moments {
			yes(
				&f,
				&format!(
					"let p=time::parts({ms},{0})?; time::from_parts(p,{0})? == {ms}",
					lit(zone)
				),
			);
			if ms >= -1 {
				yes(
					&f,
					&format!("time::parse(time::rfc3339({ms},{0})?)? == {ms}", lit(zone)),
				);
			}
		}
	}
	for (date, paris, chicago) in [
		("2026-01-15T12:00:00Z", 3600, -21600),
		("2026-07-15T12:00:00Z", 7200, -18000),
	] {
		yes(
			&f,
			&format!(
				"let ms=time::parse({})?; time::parts(ms,\"Europe/Paris\")?.offset_seconds == {paris} && time::parts(ms,\"America/Chicago\")?.offset_seconds == {chicago}",
				lit(date)
			),
		);
	}
	yes(
		&f,
		r#"let p=time::parts(0,"UTC")?; p.weekday==4 && p.millisecond==0 && p.zone=="UTC""#,
	);
	yes(
		&f,
		r#"time::rfc3339(253402207200999,"UTC")? == "9999-12-30T22:00:00.999Z""#,
	);
	refuses(&f, "time::rfc3339(-377705023201000,\"UTC\")", "year -9999");
	refuses(
		&f,
		"time::rfc3339(-2208988800000,\"Europe/Paris\")",
		"+00:09:21",
	);
	yes(
		&f,
		r#"time::format(-2208988800000,"%:z","Europe/Paris")? == "+00:09:21" && time::format(-377705023201000,"%Y","UTC")? == "-9999""#,
	);
	for ms in [-377705023201001i64, 253402207201000, i64::MAX, i64::MIN] {
		for expr in [
			format!("time::parts({ms},\"UTC\")"),
			format!("time::format({ms},\"%Y\",\"UTC\")"),
			format!("time::rfc3339({ms},\"UTC\")"),
		] {
			refuses(&f, &expr, &ms.to_string());
		}
	}
}
#[test]
fn parsing_validates_annotations_leaps_and_offset_spelling() {
	let f = Fixture::new();
	for text in ["1969-12-31T23:59:59.999Z", "1969-12-31T23:59:59.999999999Z"] {
		yes(&f, &format!("time::parse({})? == -1", lit(text)));
	}
	yes(
		&f,
		r#"time::parse("1970-01-01T00:00:00.999999Z")? == 999 && time::parse("1970-01-01T00:00:00.999Z")? == 999"#,
	);
	yes(
		&f,
		r#"time::parse("2026-07-01T12:00:00Z[Europe/Paris]")? == time::parse("2026-07-01T12:00:00Z")?"#,
	);
	yes(
		&f,
		r#"time::parse("2026-07-01T12:00:00+02:00[Europe/Paris]")? == time::parse("2026-07-01T10:00:00Z")?"#,
	);
	for (s, word) in [
		("2026-07-01T12:00:00", "offset"),
		("2016-12-31T23:59:60Z", "leap second"),
		("2016-12-31T235960.5Z", "leap second"),
		("2016-12-31t23:59:60z", "leap second"),
		("2016-12-31T23:59:60Z[UTC]", "leap second"),
		("2026-07-01T12:00:00Z[Made/Up]", "Made/Up"),
		("2026-07-01T12:00:00+00:00[Europe/Paris]", "offset"),
		("1900-01-01T00:09:00+00:09[Europe/Paris]", "offset"),
		("2026-07-01T12:00:00+05:00[Europe/Paris]", "offset"),
		("2026-07-01T12:00:00Z[u-ca=iso8601]", "annotation"),
		("2026-07-01T12:00:00Z[UTC][u-ca=iso8601]", "annotation"),
		("2026-07-01T12:00:00+24:00", "24:00"),
		("2026-07-01T12:00:00+00:00:00", "±HH:MM"),
		("2026-07-01T12:00:00+0000", "±HH:MM"),
		("2026-07-01T12:00:00+01:60", "01:60"),
		("+010000-01-01T00:00:00Z", "range"),
	] {
		refuses(&f, &format!("time::parse({})", lit(s)), word);
	}
}
#[test]
fn from_parts_refuses_gaps_and_selects_only_valid_fold_offsets() {
	let f = Fixture::new();
	for (m, d, h) in [(3, 29, 2), (10, 25, 2), (7, 1, 12)] {
		let base = format!("#{{year:2026,month:{m},day:{d},hour:{h},minute:30");
		for off in [None, Some(3600), Some(7200), Some(0)] {
			let obj = format!(
				"{base}{}}}",
				off.map(|n| format!(",offset_seconds:{n}"))
					.unwrap_or_default()
			);
			let expr = format!("time::from_parts({obj},\"Europe/Paris\")");
			if m == 3 {
				refuses(&f, &expr, "gap");
			} else if m == 10 && off.is_none() {
				refuses(&f, &expr, "fold");
			} else if off == Some(0) || (m == 7 && off == Some(3600)) {
				refuses(&f, &expr, "offset");
			} else {
				yes(
					&f,
					&format!("let ms={expr}?; time::parts(ms,\"Europe/Paris\")?.hour == {h}"),
				);
			}
		}
	}
	for (obj, word) in [
		("#{year:2026,month:13,day:1}", "month"),
		("#{year:2026,month:2,day:30}", "day"),
		("#{year:2026,month:1,day:1,second:60}", "second"),
		("#{year:2026,month:1}", "day"),
		("#{year:2026,month:1,day:1,hour:1.5}", "hour"),
	] {
		refuses(&f, &format!("time::from_parts({obj},\"UTC\")"), word);
	}
	yes(
		&f,
		"time::from_parts(#{year:1970,month:1,day:1},\"UTC\")? == 0",
	);
}
#[test]
fn zones_and_formatting_fail_catchably() {
	let f = Fixture::new();
	for zone in ["Made/Up", "+24:00", "+02", "+02:00:00", "-00:60"] {
		refuses(&f, &format!("time::parts(0,{})", lit(zone)), zone);
	}
	refuses(&f, "time::format(0,\"%J\",\"UTC\")", "%J");
	assert_eq!(
		out(f
			.eval("match time::parts(0,\"local\") {Ok(_) => false,Err(e) => e.contains(\"TZ\")}")
			.env("TZ", "Made/Up")),
		"true"
	);
	// Fresh child, valid UTC-only database: an empty directory makes Jiff
	// fall back to the host database and cannot prove a missing name.
	#[cfg(unix)]
	{
		let empty = f.0.join("empty");
		std::fs::create_dir(&empty).unwrap();
		let mut tzif = b"TZif".to_vec();
		tzif.extend([0u8; 16]);
		for count in [0u32, 0, 0, 0, 1, 4] {
			tzif.extend(count.to_be_bytes());
		}
		tzif.extend([0u8; 6]);
		tzif.extend(b"UTC\0");
		std::fs::write(empty.join("UTC"), tzif).unwrap();
		assert_eq!(
			out(f
				.eval(
					"match time::parts(0,\"local\") {Ok(_) => false,Err(e) => e.contains(\"TZ\")}"
				)
				.env("TZDIR", &empty)),
			"true"
		);
		assert_eq!(
			out(f
				.eval("time::parts(0,\"UTC\")?.hour == 0 && time::parts(0,\"+02:00\")?.hour == 2")
				.env("TZDIR", empty)),
			"true"
		);
	}
}
fn session(f: &Fixture, text: &str) -> Output {
	let mut c = f
		.command()
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	c.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
	c.wait_with_output().unwrap()
}
#[test]
fn sleep_runs_in_every_entry_and_futures_are_inert() {
	let f = Fixture::new();
	let start = Instant::now();
	yes(
		&f,
		"let start=time::monotonic_ms(); time::sleep(50).await?; time::monotonic_ms()-start >= 50",
	);
	assert!(start.elapsed() >= Duration::from_millis(50));
	let file=f.file("pub async fn main(_) { let start=time::monotonic_ms(); time::sleep(50).await?; time::monotonic_ms()-start >= 50 }");
	assert_eq!(out(f.command().arg("run").arg(file)), "true");
	let o = session(
		&f,
		"let start=time::monotonic_ms(); time::sleep(50).await?; time::monotonic_ms()-start >= 50\n:debug\n:help time::now_ms\n:help time::monotonic_ms\n:reset\ntime::parse(\"1970-01-01T00:00:00Z\")?\n:quit\n",
	);
	assert!(o.status.success());
	let s = String::from_utf8_lossy(&o.stdout);
	assert!(
		s.contains("true")
			&& s.contains("pub async fn main")
			&& s.contains("wall-clock")
			&& s.contains("not an epoch timestamp"),
		"{s}"
	);
	let start = Instant::now();
	out(&mut f.eval("let f=time::sleep(10000); 7"));
	assert!(start.elapsed() < Duration::from_secs(2));
	let o = f
		.eval("fn f() { time::sleep(1).await } f()")
		.output()
		.unwrap();
	assert!(!o.status.success());
	assert!(String::from_utf8_lossy(&o.stderr).contains("await"));
	for n in [-1, 68719476736i64] {
		refuses(&f, &format!("time::sleep({n}).await"), &n.to_string());
	}
}

#[cfg(unix)]
#[test]
fn cancellation_ends_pending_sleep_in_run_eval_and_session() {
	use std::{
		io::{BufRead, BufReader},
		sync::mpsc,
	};
	for mode in ["run", "eval", "session"] {
		let f = Fixture::new();
		let body = "host::eprint(\"ready\\n\"); time::sleep(10000).await?; 7";
		let file = f.file(&format!("pub async fn main(_) {{ {body} }}"));
		let mut command = f.command();
		if mode == "run" {
			command.arg("run").arg(file);
		} else if mode == "eval" {
			command.args(["eval", body]);
		}
		let mut child = command
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		let mut stdin = child.stdin.take().unwrap();
		let stderr = child.stderr.take().unwrap();
		let (tx, rx) = mpsc::channel();
		let reader = std::thread::spawn(move || {
			let mut text = String::new();
			for line in BufReader::new(stderr).lines() {
				let line = line.unwrap();
				if line.contains("ready") {
					let _ = tx.send(());
				}
				text.push_str(&line);
				text.push('\n');
			}
			text
		});
		if mode == "session" {
			writeln!(stdin, "{body}").unwrap();
		}
		if rx.recv_timeout(Duration::from_secs(5)).is_err() {
			child.kill().unwrap();
			let _ = child.wait();
			panic!("{mode}: never ready");
		}
		// Ready is emitted just before awaiting; allow the VM to enter sleep.
		std::thread::sleep(Duration::from_millis(20));
		let sent = Instant::now();
		assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
		if mode == "session" {
			writeln!(stdin, "42\n:quit").unwrap();
		}
		drop(stdin);
		while child.try_wait().unwrap().is_none() && sent.elapsed() < Duration::from_millis(500) {
			std::thread::sleep(Duration::from_millis(2));
		}
		if child.try_wait().unwrap().is_none() {
			child.kill().unwrap();
			let _ = child.wait();
			panic!("{mode}: failed 500 ms cancellation bound");
		}
		let o = child.wait_with_output().unwrap();
		let err = reader.join().unwrap();
		assert!(err.contains("interrupted"), "{mode}: {err}");
		if mode == "session" {
			assert!(o.status.success());
			assert!(String::from_utf8_lossy(&o.stdout).contains("42"));
		} else {
			assert_eq!(o.status.code(), Some(130));
		}
	}
}
