//! Record 0044's launch settings, through a native child on each platform.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{
	OnceLock,
	atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-process-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}
fn fixture() -> &'static Path {
	static PATH: OnceLock<PathBuf> = OnceLock::new();
	PATH.get_or_init(|| {
		let dir = scratch();
		let path = dir.join(format!("child{}", std::env::consts::EXE_SUFFIX));
		let out = Command::new("rustc")
			.args(["--edition=2024", "tests/harness/process_child.rs", "-o"])
			.arg(&path)
			.output()
			.unwrap();
		assert!(
			out.status.success(),
			"{}",
			String::from_utf8_lossy(&out.stderr)
		);
		path
	})
}
fn q(s: impl AsRef<std::ffi::OsStr>) -> String {
	serde_json::to_string(s.as_ref().to_str().unwrap()).unwrap()
}
fn command(dir: &Path, body: &str) -> Command {
	let path = dir.join("test.rn");
	std::fs::write(&path, format!("pub fn main(_) {{ {body} }}")).unwrap();
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_rnx"));
	cmd.args(["--color=never", "run"])
		.arg(path)
		.current_dir(dir);
	cmd
}
fn run(dir: &Path, body: &str) -> Output {
	command(dir, body).output().unwrap()
}
fn ok(out: Output) -> String {
	assert!(
		out.status.success(),
		"{}",
		String::from_utf8_lossy(&out.stderr)
	);
	assert!(
		out.stderr.is_empty(),
		"{}",
		String::from_utf8_lossy(&out.stderr)
	);
	String::from_utf8(out.stdout).unwrap()
}
fn call(name: &str, mode: &str, opts: &str) -> String {
	format!("process::{name}({}, [{}], {opts})", q(fixture()), q(mode))
}
#[test]
fn defaults_explicit_timeout_nonzero_and_input_are_the_same_supervised_reply() {
	let dir = scratch();
	let p = q(fixture());
	ok(run(
		&dir,
		&format!(
			r#"
        let a = process::run({p}, ["fail"], #{{}})?;
        let b = process::run({p}, ["fail"], #{{timeout_ms: 30000}})?;
        assert_eq!(json::stringify(a)?, json::stringify(b)?);
        assert_eq!(a.code, 7); assert_eq!(a.stderr, "complaint");

        for name in ["timed_out", "cancelled", "truncated", "cut_short", "unreadable"] {{ assert_eq!(a[name], false); }}
        let c = process::run_bytes({p}, ["fail"], #{{}})?;
        let d = process::run_bytes({p}, ["fail"], #{{timeout_ms: 30000}})?;
        assert_eq!(c.stdout, d.stdout); assert_eq!(c.stderr, d.stderr); assert_eq!(c.code, d.code);
        let c = process::run({p}, ["echo"], #{{input: "hé\0"}})?;
        assert_eq!(c.stdout, "hé\0");
        let c = process::run_bytes({p}, ["echo"], #{{input: b"h\xff\0"}})?;
        assert_eq!(c.stdout, b"h\xff\0");
        let c = process::run({p}, ["echo"], #{{input: b"hello"}})?;
        assert_eq!(c.stdout, "hello");
        for opts in [#{{}}, #{{input: ""}}, #{{timeout_ms: 30000, env_clear: false, env: #{{}}}}] {{
            assert_eq!(process::run({p}, ["echo"], opts)?.stdout, "");
        }}
        assert!(process::run("not-an-existing-rnx-child", [], #{{}}).is_err());
    "#
		),
	));
}
#[test]
fn validation_refuses_before_launch_and_names_the_sibling() {
	let dir = scratch();
	let p = q(fixture());
	for (option, needle) in [
		("#{timout_ms: 1}", "timout_ms"),
		("#{timeout_ms: 0}", "between"),
		("#{timeout_ms: 90001}", "between"),
		("#{timeout_ms: -1}", "integer"),
		("#{timeout_ms: 1.5}", "integer"),
		("#{input: ()}", "input"),
		("#{env_clear: 1}", "boolean"),
		("#{cwd: 1}", "cwd"),
		("#{cwd: \"\"}", "cwd"),
		("#{env: 1}", "env"),
		("#{env: #{BAD: 1}}", "BAD"),
		("#{env: #{BAD: Some(\"x\")}}", "BAD"),
		("#{env: #{BAD: \"x\\0\"}}", "BAD"),
		("#{env: #{\"A=B\": \"secret\"}}", "A=B"),
		("#{env: #{\"\": None}}", "nonempty"),
		("#{env: #{\"A\\0\": None}}", "NUL"),
		("#{cwd: \"x\\0\"}", "NUL"),
		("()", "object"),
	] {
		let text = ok(run(
			&dir,
			&format!(
				"if let Err(e) = process::run({p}, [\"marker\"], {option}) {{ println!(\"{{e}}\"); }} else {{ panic!(\"accepted\"); }}"
			),
		));
		assert!(text.contains(needle), "{option}: {text}");
		assert!(!text.contains("secret"));
		assert!(!dir.join("launched").exists());
	}
	ok(run(
		&dir,
		&format!(
			r#"
        assert!(process::run({p}, [1], #{{}}).is_err());
        assert!(process::run({p}, ["x\0"], #{{}}).is_err());
        assert!(process::run("", [], #{{}}).is_err());
        assert!(process::run("x\0", [], #{{}}).is_err());
    "#
		),
	));
	let text = ok(run(
		&dir,
		&format!(
			"if let Err(e) = {} {{ println!(\"{{e}}\"); }}",
			call("run", "bad", "#{}")
		),
	));
	assert!(
		text.contains("process::run_bytes") && !text.contains("host::process_bytes"),
		"{text}"
	);
	ok(run(
		&dir,
		&format!(
			"assert_eq!({}?.stdout, b\"x\\xff\");",
			call("run_bytes", "bad", "#{}")
		),
	));
}
#[test]
fn explicit_program_is_parent_relative_and_cwd_changes_only_the_child() {
	let dir = scratch();
	let sub = dir.join("out é");
	std::fs::create_dir(&sub).unwrap();
	let name = format!("child{}", std::env::consts::EXE_SUFFIX);
	std::fs::copy(fixture(), dir.join(&name)).unwrap();
	std::fs::copy(fixture(), sub.join(&name)).unwrap();
	let p = q(format!("./{name}"));
	ok(run(
		&dir,
		&format!(
			r#"
        let before = fs::cwd()?;
        assert_eq!(process::run({p}, ["identity"], #{{cwd: "out é"}})?.stdout, {});
        assert_eq!(process::run({p}, ["cwd"], #{{cwd: {}}})?.stdout, {});
        assert_eq!(process::run({p}, ["identity"], #{{}})?.stdout, {});
        assert_eq!(fs::cwd()?, before);
        assert!(process::run({p}, [], #{{cwd: "missing"}}).is_err());
        assert!(process::run({p}, [], #{{cwd: {p}}}).is_err());
    "#,
			q(dir.join(&name)),
			q(&sub),
			q(&sub),
			q(dir.join(&name))
		),
	));
	let source = format!(
		"assert_eq!(process::run({}, [\"identity\"], #{{}})?.stdout, {});",
		q(&name),
		q(dir.join(&name))
	);
	ok(command(&dir, &source).env("PATH", &dir).output().unwrap());
}
#[test]
fn environment_overrides_clear_and_remove_without_changing_parent_or_next_child() {
	let dir = scratch();
	let p = q(fixture());
	let body = format!(
		r#"
        let before = env::var("RNX_CHILD_A")?;
        let a = process::run({p}, ["env", "RNX_CHILD_A", "RNX_CHILD_B", "RNX_CHILD_C"],
            #{{env: #{{RNX_CHILD_A: "override", RNX_CHILD_B: None, RNX_CHILD_C: ""}}}})?;
        print!("{{}}", a.stdout);
        let b = process::run({p}, ["env", "RNX_CHILD_A", "RNX_CHILD_B"], #{{}})?;
        print!("{{}}", b.stdout);
        let c = process::run({p}, ["env", "RNX_CHILD_A", "RNX_CHILD_B", "RNX_CHILD_C"],
            #{{env_clear: true, env: #{{RNX_CHILD_A: None, RNX_CHILD_C: "only"}}}})?;
        print!("{{}}", c.stdout);
        assert_eq!(env::var("RNX_CHILD_A")?, before);
    "#
	);
	let text = ok(command(&dir, &body)
		.env("RNX_CHILD_A", "parent")
		.env("RNX_CHILD_B", "inherited")
		.output()
		.unwrap());
	assert_eq!(
		text,
		"RNX_CHILD_A=Some(\"override\")\nRNX_CHILD_B=None\nRNX_CHILD_C=Some(\"\")\nRNX_CHILD_A=Some(\"parent\")\nRNX_CHILD_B=Some(\"inherited\")\nRNX_CHILD_A=None\nRNX_CHILD_B=None\nRNX_CHILD_C=Some(\"only\")\n"
	);
}
#[test]
fn capture_and_deadline_flags_reach_the_new_surface() {
	let dir = scratch();
	for name in ["run", "run_bytes"] {
		ok(run(
			&dir,
			&format!(
				"let r = {}?; assert!(r.truncated); assert_eq!(r.stdout.len(), 2097152);",
				call(name, "big", "#{}")
			),
		));
		ok(run(
			&dir,
			&format!(
				"let r = {}?; assert!(r.timed_out); assert!(!r.cancelled);",
				call(name, "sleep", "#{timeout_ms: 20}")
			),
		));
	}
}
#[cfg(unix)]
#[test]
fn native_environment_is_inherited_without_unicode_conversion() {
	use std::os::unix::ffi::OsStrExt;
	let dir = scratch();
	let source = format!(
		"print!(\"{{}}\", process::run({}, [\"env\", \"RNX_RAW\"], #{{}})?.stdout);",
		q(fixture())
	);
	let text = ok(command(&dir, &source)
		.env("RNX_RAW", std::ffi::OsStr::from_bytes(b"\xff"))
		.output()
		.unwrap());
	assert!(text.contains("\\xFF"), "{text}");
}
#[cfg(windows)]
#[test]
fn windows_keys_and_drive_relative_paths_are_refused() {
	let dir = scratch();
	let p = q(fixture());
	ok(run(
		&dir,
		&format!(
			r#"
        assert!(process::run({p}, [], #{{env: #{{Path: "a", PATH: "b"}}}}).is_err());
        assert!(process::run("C:tool.exe", [], #{{}}).is_err());
        assert!(process::run({p}, [], #{{cwd: "C:work"}}).is_err());
    "#
		),
	));
}

#[path = "harness/commands.rs"]
mod commands;
mod harness;

#[test]
fn input_backpressure_and_early_exit_use_the_existing_writer() {
	let dir = scratch();
	let body = commands::expand(
		r#"
        let s = ""; for n in 0..20000 { s += "0123456789"; }
        let r = process::run(@PRESSURE@, #{input: s, timeout_ms: 20000})?;
        assert_eq!(r.code, 0); assert!(!r.timed_out);
        assert_eq!(r.stderr.len(), 200000);
        assert!(r.stdout.ends_with("200000\n"));
        let r = process::run_bytes(@SUCCEED@, #{input: s, timeout_ms: 20000})?;
        assert_eq!(r.code, 0);
    "#,
	);
	ok(run(&dir, &body));
}

#[cfg(unix)]
#[test]
fn cancellation_during_a_child_call_reaches_both_facades() {
	use std::process::Stdio;
	use std::time::Duration;
	for name in ["run", "run_bytes"] {
		let dir = scratch();
		let body = format!(
			"let r = {}?; println!(\"cancelled={{}}\", r.cancelled);",
			call(name, "sleep", "#{}")
		);
		let child = command(&dir, &body)
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		let started = harness::appeared(&dir.join("started"), Duration::from_secs(5));
		unsafe {
			libc::kill(child.id() as i32, libc::SIGINT);
		}
		let ended = harness::teardown(child, &dir);
		assert!(started && ended.trouble.is_empty(), "{:?}", ended.trouble);
		assert_eq!(ended.code, Some(0), "{}", ended.err);
		assert!(
			ended.out.contains("cancelled=true"),
			"{} {}",
			ended.out,
			ended.err
		);
	}
}

#[cfg(all(unix, feature = "test-support"))]
#[test]
fn cleanup_interrupt_and_late_delivery_error_cross_the_facade() {
	use std::process::Stdio;
	use std::time::Duration;
	for name in ["run", "run_bytes"] {
		let dir = scratch();
		let body = format!(
			"let r = {}?; println!(\"cancelled={{}}\", r.cancelled);",
			call(name, "fail", "#{}")
		);
		let began = dir.join("cleanup");
		let release = dir.join("readers");
		let child = command(&dir, &body)
			.env("RNX_TEST_SIGNAL_CLEANUP_TO", &began)
			.env("RNX_TEST_READER_WAITS_FOR", &release)
			.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", "8000")
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		let ready = harness::appeared(&began, Duration::from_secs(5));
		unsafe {
			libc::kill(child.id() as i32, libc::SIGINT);
		}
		std::fs::write(&release, b"go").unwrap();
		let ended = harness::teardown(child, &dir);
		assert!(ready && ended.trouble.is_empty(), "{:?}", ended.trouble);
		assert!(
			ended.out.contains("cancelled=true"),
			"{} {}",
			ended.out,
			ended.err
		);

		let dir = scratch();
		let script = q(harness::holds_stdin_until_released(&dir));
		let body = format!(
			r#"
            let s = ""; for n in 0..20000 {{ s += "0123456789"; }}
            process::{name}("sh", ["-c", {script}], #{{input: s, timeout_ms: 30000}})?;
        "#
		);
		let child = command(&dir, &body)
			.env("RNX_TEST_DELIVERY_FAILS", "fixture failure")
			.env("RNX_TEST_SIGNAL_DELIVERY_FAILURE_TO", dir.join("injected"))
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));
		let injected = harness::appeared(&dir.join("injected"), Duration::from_secs(10));
		let ended = harness::teardown(child, &dir);
		assert!(
			held && injected && ended.trouble.is_empty(),
			"{} {:?}",
			ended.err,
			ended.trouble
		);
		assert_eq!(ended.code, Some(1));
		assert!(
			ended
				.err
				.contains("the input could not be delivered: fixture failure"),
			"{}",
			ended.err
		);
	}
}

#[test]
fn git_status_example_uses_an_isolated_checkout() {
	let dir = scratch();
	let checkout = dir.join("checkout");
	let mut git = Command::new("git");
	for (key, _) in std::env::vars_os() {
		if key.to_string_lossy().starts_with("GIT_") {
			git.env_remove(key);
		}
	}
	let result = git
		.args(["init", "--quiet"])
		.arg(&checkout)
		.env("GIT_CONFIG_NOSYSTEM", "1")
		.env("GIT_CONFIG_GLOBAL", dir.join("no-config"))
		.env_remove("GIT_DIR")
		.env_remove("GIT_WORK_TREE")
		.output()
		.unwrap();
	assert!(
		result.status.success(),
		"{}",
		String::from_utf8_lossy(&result.stderr)
	);
	std::fs::write(checkout.join("new-file"), b"hello").unwrap();
	let body = format!(
		r#"
        let r = process::run("git", ["status", "--porcelain"], #{{cwd: {}, env: #{{
            GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: {}, GIT_DIR: None, GIT_WORK_TREE: None
        }}}})?;
        assert_eq!(r.code, 0); assert_eq!(r.stdout, "?? new-file\n");
    "#,
		q(&checkout),
		q(dir.join("no-config"))
	);
	let mut launch = command(&dir, &body);
	for (key, _) in std::env::vars_os() {
		if key.to_string_lossy().starts_with("GIT_") {
			launch.env_remove(key);
		}
	}
	ok(launch.output().unwrap());
}

#[test]
fn help_survives_reset_and_the_ceiling_still_refuses_input() {
	use std::io::Write;
	use std::process::Stdio;
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["--color=never", "--no-splash", "repl"])
		.env("TERM", "xterm")
		.env("RNX_CONFIG", scratch().join("absent-config"))
		.env_remove("RNX_MEMORY_CEILING")
		.env("RNX_HISTORY", scratch().join("history"))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(b":help process::run\n:reset\n:help process::run\n:q\n")
		.unwrap();
	let text = ok(child.wait_with_output().unwrap());
	assert_eq!(
		text.matches("Explicit relative program paths").count(),
		2,
		"{text}"
	);
	#[cfg(feature = "count-allocations")]
	{
		let dir = scratch();
		let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
			.args(["--color=never", "--no-splash", "repl"])
			.env("RNX_CONFIG", dir.join("absent-config"))
			.env("RNX_HISTORY", dir.join("history"))
			.env("TERM", "xterm")
			.env("RNX_MEMORY_CEILING", "1")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		child
			.stdin
			.take()
			.unwrap()
			.write_all(b"process::run(\"missing\", [], #{})?\n:q\n")
			.unwrap();
		let out = child.wait_with_output().unwrap();
		assert!(out.status.success());
		let text = format!(
			"{}{}",
			String::from_utf8_lossy(&out.stdout),
			String::from_utf8_lossy(&out.stderr)
		);
		assert!(text.contains("are at or above the ceiling of 1"), "{text}");
	}
}

#[test]
fn build_fixture_combines_directory_and_environment() {
	let dir = scratch();
	let work = dir.join("build é");
	std::fs::create_dir(&work).unwrap();
	let source = format!(
		"let r = process::run({}, [\"build\"], #{{cwd: {}, env: #{{RNX_BUILD_MODE: \"release\"}}}})?; assert_eq!(r.stdout, {});",
		q(fixture()),
		q(&work),
		q(format!("{}\nrelease", work.display()))
	);
	ok(run(&dir, &source));
}
#[cfg(feature = "test-support")]
#[test]
fn unreadable_capture_is_reported_by_both_forms() {
	for name in ["run", "run_bytes"] {
		let dir = scratch();
		let source = format!(
			"let r = {}?; assert!(r.unreadable); assert!(!r.truncated);",
			call(name, "fail", "#{}")
		);
		ok(command(&dir, &source)
			.env("RNX_TEST_CAPTURE_FAILS", "1")
			.output()
			.unwrap());
	}
}
