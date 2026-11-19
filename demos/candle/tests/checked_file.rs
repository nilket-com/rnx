//! Exercise the synchronous file entry through the generated Polars + Candle CLI.
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
	fn new() -> Self {
		static NEXT: AtomicU64 = AtomicU64::new(0);
		let base = Path::new(env!("CARGO_TARGET_TMPDIR"));
		fs::create_dir_all(base).unwrap();
		loop {
			let path = base.join(format!(
				"checked-file-{}-{}",
				std::process::id(),
				NEXT.fetch_add(1, Ordering::Relaxed)
			));
			match fs::create_dir(&path) {
				Ok(()) => return Self(path),
				Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
				Err(error) => panic!("create test directory: {error}"),
			}
		}
	}

	fn script(&self, source: &str) -> PathBuf {
		let path = self.0.join("main.rn");
		fs::write(&path, source).unwrap();
		path
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		fs::remove_dir_all(&self.0).unwrap();
	}
}

fn cli(script: &Path, input: &Path) -> Command {
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_rnx-candle-demo"));
	cmd.args(["--no-splash", "--color=never", "run", "--debug-engine"])
		.arg(script)
		.arg(input)
		.env("RNX_CONFIG", "")
		.env("RNX_HISTORY", "/dev/null")
		.env_remove("RNX_MEMORY_CEILING")
		.env_remove("RNX_SESSION_STORE_BYTES")
		.env_remove("POLARS_BACKTRACE_IN_ERR")
		.env_remove("POLARS_PANIC_ON_ERR");
	cmd
}

fn check(out: Output, route: &str, status: i32) -> (String, String) {
	let stdout = String::from_utf8(out.stdout).unwrap();
	let stderr = String::from_utf8(out.stderr).unwrap();
	assert_eq!(out.status.code(), Some(status), "{stdout}\n{stderr}");
	let selected: Vec<_> = stderr
		.lines()
		.filter(|l| l.starts_with("rnx: engine="))
		.collect();
	assert!(
		matches!(selected.as_slice(), [line] if line.starts_with(route)),
		"{stderr}"
	);
	(stdout, stderr)
}

const FEATURES: &str = r#"
pub fn main(args) {
	println!("FILE-START");
	let frame = polars::read_csv(args[0], [("id","i64"),("a","f64"),("b","f64"),("c","f64")])?;
	let dense = frame.to_dense(["a","c"],"f32")?;
	let tensor = candle::Tensor::from_dense(dense)?;
	println!("shape=({}, {})", tensor.rank(), tensor.elem_count());
	Ok(())
}
"#;

#[test]
fn checked_prefix_runs_once_with_the_file_allowance() {
	let scratch = Scratch::new();
	let script = scratch.script(FEATURES);
	let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/features.csv");
	let mut command = cli(&script, &input);
	let (stdout, _) = check(
		command
			.env("RNX_SESSION_STORE_BYTES", "1")
			.output()
			.unwrap(),
		"rnx: engine=execution ",
		0,
	);
	assert_eq!(
		stdout.lines().collect::<Vec<_>>(),
		["FILE-START", "shape=(2, 128)"]
	);
}

#[test]
fn panic_setting_withholds_file_reader_before_effects() {
	let scratch = Scratch::new();
	let script = scratch.script(FEATURES);
	let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/features.csv");
	for trace in ["0", "1"] {
		let (stdout, _) = check(
			cli(&script, &input)
				.env("POLARS_PANIC_ON_ERR", "1")
				.env("POLARS_BACKTRACE_IN_ERR", trace)
				.output()
				.unwrap(),
			"rnx: engine=legacy ",
			0,
		);
		assert_eq!(
			stdout.lines().collect::<Vec<_>>(),
			["FILE-START", "shape=(2, 128)"]
		);
	}
}

const LARGE_ROWS: u32 = 8_388_610;
const HEADER: &[u8] = b"id,label\n";

fn generate(path: &Path) {
	let mut file = BufWriter::with_capacity(1 << 20, File::create_new(path).unwrap());
	file.write_all(HEADER).unwrap();
	let mut row = [b'x'; 256];
	row[7] = b',';
	row[255] = b'\n';
	for i in 0..LARGE_ROWS {
		let mut number = i;
		for digit in row[..7].iter_mut().rev() {
			*digit = b'0' + (number % 10) as u8;
			number /= 10;
		}
		row[8] = b'A' + (i % 26) as u8;
		row[254] = b'0' + (i % 10) as u8;
		file.write_all(&row).unwrap();
	}
	file.flush().unwrap();
}

fn verify_fixture(path: &Path) {
	let file = File::open(path).unwrap();
	let size = file.metadata().unwrap().len();
	assert_eq!(size, HEADER.len() as u64 + 256 * u64::from(LARGE_ROWS));
	assert!(size > 1 << 31);
	let mut reader = BufReader::with_capacity(1 << 20, file);
	let mut line = String::new();
	reader.read_line(&mut line).unwrap();
	assert_eq!(line, "id,label\n");
	for expected in 0..LARGE_ROWS {
		line.clear();
		assert_eq!(reader.read_line(&mut line).unwrap(), 256);
		let (id, label) = line.trim_end_matches('\n').split_once(',').unwrap();
		assert_eq!(id.parse::<u32>().unwrap(), expected);
		let bytes = label.as_bytes();
		assert_eq!(bytes.len(), 247);
		assert_eq!(
			bytes[0],
			b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"[(expected as usize) % 26]
		);
		assert_eq!(bytes[246], b"0123456789"[(expected as usize) % 10]);
		if expected == 0 || expected == LARGE_ROWS / 2 || expected == LARGE_ROWS - 1 {
			assert!(bytes[1..246].iter().all(|&b| b == b'x'));
		}
	}
	assert!(reader.fill_buf().unwrap().is_empty());
	eprintln!("verified {size} CSV bytes, both columns, {LARGE_ROWS} rows");
}

#[test]
#[ignore = "explicit large-file routing gate, generates a CSV above 2 GiB"]
fn above_two_gib_csv_enters_checked_main() {
	let scratch = Scratch::new();
	let input = scratch.0.join("large.csv");
	generate(&input);
	verify_fixture(&input);
	let script = scratch.script(
		r#"pub fn main(args){
		println!("FILE-START");
		let df=polars::read_csv(args[0],[("id","i64"),("label","string")])?;
		let d=df.to_dense(["id"],"f32")?;
		let t=candle::Tensor::from_dense(d)?;
		println!("columns={} rank={} elements={}",df.width(),t.rank(),t.elem_count());
		Ok(())
	}"#,
	);
	// The same input exercises the other registration in a separate
	// process, with more Store credit than the entire CSV.
	let mut session = Command::new(env!("CARGO_BIN_EXE_rnx-candle-demo"))
		.current_dir(&scratch.0)
		.args(["--no-splash", "--color=never", "repl", "--engine=execution"])
		.env("RNX_CONFIG", "")
		.env("RNX_HISTORY", "/dev/null")
		.env_remove("RNX_MEMORY_CEILING")
		.env_remove("POLARS_BACKTRACE_IN_ERR")
		.env_remove("POLARS_PANIC_ON_ERR")
		.env("RNX_SESSION_STORE_BYTES", "8589934592")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.unwrap();
	session.stdin.take().unwrap().write_all(b"let frame=polars::read_csv(\"large.csv\",[(\"id\",\"i64\"),(\"label\",\"string\")])?;\n:quit\n").unwrap();
	let session = session.wait_with_output().unwrap();
	let error = String::from_utf8(session.stderr).unwrap();
	assert!(
		session.status.success() && error.contains("StoreCapacity"),
		"{error}"
	);
	eprintln!("execution session with 8 GiB credit: {error}");
	let spec = cli(&script, &input);
	let mut timed = Command::new("/usr/bin/time");
	timed
		.args(["-f", "peak_rss_kib=%M", "-o"])
		.arg(scratch.0.join("rss"))
		.arg(spec.get_program())
		.args(spec.get_args());
	for (key, value) in spec.get_envs() {
		if let Some(v) = value {
			timed.env(key, v);
		} else {
			timed.env_remove(key);
		}
	}
	let (stdout, stderr) = check(timed.output().unwrap(), "rnx: engine=execution ", 0);
	assert_eq!(
		stdout.lines().collect::<Vec<_>>(),
		["FILE-START", "columns=2 rank=2 elements=8388610"]
	);
	eprintln!(
		"{stdout}{stderr}{}",
		fs::read_to_string(scratch.0.join("rss")).unwrap()
	);
}

const PREFIX: &str = r#"pub fn main(args) {
	println!("FILE-START");
	let df = polars::read_csv(args[0], [("id","i64"),("a","f64"),("b","f64"),("c","f64")])?;
"#;

fn features() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/features.csv")
}

#[test]
fn unsupported_later_call_falls_back_before_the_first_effect() {
	let scratch = Scratch::new();
	let script = scratch.script(&format!("{PREFIX} df.lazy(); Ok(()) }}"));
	let (stdout, _) = check(
		cli(&script, &features()).output().unwrap(),
		"rnx: engine=legacy ",
		0,
	);
	assert_eq!(stdout.lines().collect::<Vec<_>>(), ["FILE-START"]);
}

#[test]
fn native_error_cannot_leave_a_checked_file_through_question_mark() {
	let scratch = Scratch::new();
	let script = scratch.script(&format!(
		"{PREFIX} println!(\"{{}}\",df.height()?); Ok(()) }}"
	));
	let (stdout, _) = check(
		cli(&script, &features()).output().unwrap(),
		"rnx: engine=legacy ",
		0,
	);
	assert_eq!(stdout.lines().next(), Some("FILE-START"));
}

#[test]
fn file_runtime_failure_keeps_its_location_and_effect() {
	let scratch = Scratch::new();
	let script = scratch.script(&format!("{PREFIX} 1 / (df.width()-4) }}"));
	let (stdout, err) = check(
		cli(&script, &features()).output().unwrap(),
		"rnx: engine=execution ",
		1,
	);
	assert_eq!(stdout.lines().collect::<Vec<_>>(), ["FILE-START"]);
	assert!(
		err.contains("main.rn, line 4") && err.contains("Division by zero"),
		"{err}"
	);
}

#[test]
fn checked_parse_failure_preserves_parser_diagnostics() {
	let scratch = Scratch::new();
	let script = scratch.script(FEATURES);
	let bad = scratch.0.join("bad.csv");
	fs::write(&bad, b"id,a,b,c\nbad,1,2,3\n").unwrap();
	for trace in ["0", "1"] {
		let (stdout, err) = check(
			cli(&script, &bad)
				.env("POLARS_BACKTRACE_IN_ERR", trace)
				.output()
				.unwrap(),
			"rnx: engine=execution ",
			1,
		);
		assert_eq!(stdout.lines().collect::<Vec<_>>(), ["FILE-START"]);
		assert!(
			err.contains("could not parse `bad`") && err.contains("column 'id'"),
			"{err}"
		);
		assert_eq!(err.contains("Rust backtrace:"), trace == "1", "{err}");
	}
}

fn looping_file(script: &Path, budget: &str) -> Command {
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_rnx-candle-demo"));
	cmd.args([
		"--no-splash",
		"--color=never",
		"run",
		"--debug-engine",
		"--budget",
		budget,
	])
	.arg(script)
	.arg(features())
	.env("RNX_CONFIG", "")
	.env("RNX_HISTORY", "/dev/null")
	.env_remove("POLARS_PANIC_ON_ERR")
	.env_remove("RNX_MEMORY_CEILING");
	cmd
}

#[test]
fn hard_fuel_survives_slice_boundaries() {
	let scratch = Scratch::new();
	let script = scratch.script(&format!(
		"{PREFIX} println!(\"file ready {{}}\",df.width()); loop {{}} }}"
	));
	let (stdout, err) = check(
		looping_file(&script, "8000").output().unwrap(),
		"rnx: engine=execution ",
		1,
	);
	assert_eq!(
		stdout.lines().collect::<Vec<_>>(),
		["FILE-START", "file ready 4"]
	);
	assert!(err.contains("8000 work units exceeded"), "{err}");
}

#[cfg(unix)]
#[test]
fn interrupt_after_checked_file_read_is_terminal() {
	use std::process::Stdio;
	use std::sync::mpsc;
	use std::time::Duration;

	let scratch = Scratch::new();
	let script = scratch.script(&format!(
		"{PREFIX} println!(\"file ready {{}}\",df.width()); loop {{}} }}"
	));
	let mut child = looping_file(&script, "2000000000")
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let out = child.stdout.take().unwrap();
	let (send, recv) = mpsc::channel();
	let reader = std::thread::spawn(move || {
		let mut lines = Vec::new();
		for line in BufReader::new(out).lines() {
			let line = line.unwrap();
			if line == "file ready 4" {
				let _ = send.send(());
			}
			lines.push(line);
		}
		lines
	});
	let ready = recv.recv_timeout(Duration::from_secs(60));
	if ready.is_ok() {
		assert!(
			Command::new("kill")
				.args(["-INT", &child.id().to_string()])
				.status()
				.unwrap()
				.success()
		);
	} else {
		let _ = child.kill();
	}
	let output = child.wait_with_output().unwrap();
	assert!(
		ready.is_ok(),
		"{:?}",
		String::from_utf8_lossy(&output.stderr)
	);
	let (_, stderr) = check(output, "rnx: engine=execution ", 130);
	assert!(stderr.contains("interrupted"));
	assert_eq!(reader.join().unwrap(), ["FILE-START", "file ready 4"]);
}

#[test]
fn complete_model_and_sales_keep_pre_effect_fallback() {
	let demos = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
	for (script, input) in [
		("candle/model.rn", "data"),
		("polars/sales.rn", "data/sales.csv"),
	] {
		let out = cli(&demos.join(script), &demos.join(input))
			.output()
			.unwrap();
		check(out, "rnx: engine=legacy ", 0);
	}
}
