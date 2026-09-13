//! Record 0032's gates: one driver for `run`, `eval` and the session, and
//! `.await` at each of them.
//!
//! Unix only and `test-support` only: the fixture that gives an input
//! something to await exists under that feature, and interruption is sent
//! as `SIGINT` the way a terminal sends it.
#![cfg(all(unix, feature = "test-support"))]
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: Option<i32>,
	elapsed: Duration,
}

fn rnx(args: &[&str], stdin: Option<&str>) -> Ran {
	let start = Instant::now();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(args)
		.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	if let Some(text) = stdin {
		child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
	}
	let done = child.wait_with_output().unwrap();
	Ran {
		stdout: String::from_utf8_lossy(&done.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&done.stderr).into_owned(),
		code: done.status.code(),
		elapsed: start.elapsed(),
	}
}

/// Start rnx, send it `SIGINT` after `after`, and collect what it did. The
/// wait is bounded: a run that ignores the interrupt is killed and reported
/// as such rather than hanging the suite.
fn interrupted(args: &[&str], stdin: Option<&str>, after: Duration) -> Ran {
	let start = Instant::now();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(args)
		.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	// Written and left open: a session reading a pipe must not see its end
	// before the interrupt lands, or it would exit for want of input.
	let mut input = child.stdin.take();
	if let (Some(text), Some(pipe)) = (stdin, input.as_mut()) {
		pipe.write_all(text.as_bytes()).unwrap();
		pipe.flush().unwrap();
	}
	std::thread::sleep(after);
	let sent = Command::new("kill")
		.arg("-INT")
		.arg(child.id().to_string())
		.status()
		.unwrap();
	assert!(sent.success(), "could not send SIGINT");
	// Once the interrupt has had time to land, the pipe is closed: a session
	// then reads to its end and exits, and a run has already exited.
	std::thread::sleep(Duration::from_millis(300));
	drop(input.take());
	let deadline = Instant::now() + Duration::from_secs(5);
	while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
		std::thread::sleep(Duration::from_millis(10));
	}
	if child.try_wait().unwrap().is_none() {
		child.kill().unwrap();
		let done = child.wait_with_output().unwrap();
		panic!(
			"rnx did not stop within five seconds of SIGINT; killed\nstdout: {}\nstderr: {}",
			String::from_utf8_lossy(&done.stdout),
			String::from_utf8_lossy(&done.stderr)
		);
	}
	let done = child.wait_with_output().unwrap();
	Ran {
		stdout: String::from_utf8_lossy(&done.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&done.stderr).into_owned(),
		code: done.status.code(),
		elapsed: start.elapsed(),
	}
}

fn script(name: &str, source: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-0032-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join(name);
	std::fs::write(&path, source).unwrap();
	path
}

// Gate 2: `.await` works at all three entry points, and a synchronous file
// still runs.

#[test]
fn a_file_may_await_in_main() {
	let path = script("await.rn", "pub async fn main(_) { host::test_pending(20).await }");
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "20\n");
}

#[test]
fn a_synchronous_file_still_runs() {
	let path = script("sync.rn", "pub fn main(_) { let n = 0; for i in 0..10 { n += i; } n }");
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "45\n");
}

#[test]
fn eval_may_await() {
	let ran = rnx(&["eval", "host::test_pending(20).await + 1"], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "21\n");
}

#[test]
fn a_session_input_may_await_at_the_top_level() {
	let ran = rnx(
		&["repl"],
		Some("let waited = host::test_pending(20).await;\nwaited + 1\n"),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains("21\n"), "{}", ran.stdout);
	assert!(ran.stderr.is_empty(), "{}", ran.stderr);
}

// Gate 3: the budget survives a pending poll.

fn completes_under(source: &str, budget: usize) -> bool {
	let path = script("budget.rn", source);
	let ran = rnx(
		&["run", "--budget", &budget.to_string(), path.to_str().unwrap()],
		None,
	);
	match ran.code {
		Some(0) => true,
		Some(1) if ran.stderr.starts_with("halted:") => false,
		_ => panic!("unexpected outcome: {:?} {}", ran.code, ran.stderr),
	}
}

/// The smallest budget, to a step of 50, under which the script completes.
fn smallest_budget(source: &str) -> usize {
	let mut budget = 50;
	while !completes_under(source, budget) {
		budget += 50;
		assert!(budget < 200_000, "never completed");
	}
	budget
}

#[test]
fn an_await_does_not_grant_a_fresh_budget() {
	// Three scripts with the same instructions, awaiting at the start, in
	// the middle, and at the end. An await mid-slice must resume with what
	// the slice had left, so all three need the same budget.
	let head = "pub async fn main(_) { let n = 0;";
	let loop_a = "for i in 0..300 { n += i; }";
	let loop_b = "for i in 0..300 { n += i; }";
	let wait = "host::test_pending(1).await;";
	let tail = "n }";
	let at_start = format!("{head} {wait} {loop_a} {loop_b} {tail}");
	let in_middle = format!("{head} {loop_a} {wait} {loop_b} {tail}");
	let at_end = format!("{head} {loop_a} {loop_b} {wait} {tail}");
	let needed = smallest_budget(&at_start);
	assert!(needed > 1_000, "the scripts should span slices: {needed}");
	assert!(completes_under(&in_middle, needed), "the middle await needed more than {needed}");
	assert!(completes_under(&at_end, needed), "the end await needed more than {needed}");
	assert!(!completes_under(&in_middle, needed - 50), "the middle await got a fresh budget");
	assert!(!completes_under(&at_end, needed - 50), "the end await got a fresh budget");
}

#[test]
fn awaiting_in_a_loop_halts_for_budget_and_nothing_else() {
	let path = script("await-loop.rn", "pub async fn main(_) { loop { host::test_pending(0).await; } }");
	let ran = rnx(&["run", "--budget", "5000", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(1), "{}", ran.stderr);
	assert!(ran.stderr.starts_with("halted: 5000 instructions exceeded"), "{}", ran.stderr);
	assert_eq!(ran.stdout, "");
}

// Gate 4: interruption reaches a pending future and a CPU loop.

#[test]
fn ctrl_c_ends_a_run_pending_on_a_future() {
	let path = script("pending.rn", "pub async fn main(_) { host::test_pending(10000).await }");
	let ran = interrupted(&["run", path.to_str().unwrap()], None, Duration::from_millis(300));
	assert_eq!(ran.code, Some(130), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.elapsed < Duration::from_secs(2), "{:?}", ran.elapsed);
	assert_eq!(ran.stdout, "");
}

#[test]
fn ctrl_c_ends_a_run_in_a_loop() {
	let path = script("loop.rn", "pub fn main(_) { loop { } }");
	let ran = interrupted(
		&["run", "--budget", "18446744073709551614", path.to_str().unwrap()],
		None,
		Duration::from_millis(300),
	);
	assert_eq!(ran.code, Some(130), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.elapsed < Duration::from_secs(2), "{:?}", ran.elapsed);
}

#[test]
fn ctrl_c_ends_an_eval_pending_on_a_future() {
	let ran = interrupted(&["eval", "host::test_pending(10000).await"], None, Duration::from_millis(300));
	assert_eq!(ran.code, Some(130), "{}", ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
}

#[test]
fn ctrl_c_ends_a_session_input_pending_on_a_future_and_the_next_input_runs() {
	let ran = interrupted(
		&["repl"],
		Some("host::test_pending(10000).await\n"),
		Duration::from_millis(400),
	);
	// The session stays up after the interrupt; it ended because its input
	// pipe was closed once the interrupt had landed. Nothing pending was
	// left: had the future outlived the input, the exit would have waited.
	assert_eq!(ran.code, Some(0), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.elapsed < Duration::from_secs(3), "{:?}", ran.elapsed);
}

#[test]
fn after_an_interrupted_input_the_next_input_runs_to_completion() {
	// Two inputs on the pipe before the interrupt: the first is pending when
	// SIGINT arrives, and the second must then run and answer.
	let ran = interrupted(
		&["repl"],
		Some("host::test_pending(10000).await\nhost::test_pending(10).await + 1\n"),
		Duration::from_millis(400),
	);
	assert_eq!(ran.code, Some(0), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("11\n"), "{}", ran.stdout);
}

// Gate 5: diagnostics keep their place after an await.

#[test]
fn a_runtime_error_after_an_await_is_still_placed() {
	let path = script(
		"placed.rn",
		"pub async fn main(_) {\n    host::test_pending(1).await;\n    let v = [];\n    v[3]\n}\n",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(1));
	assert!(ran.stderr.contains("runtime error"), "{}", ran.stderr);
	assert!(ran.stderr.contains("line 4"), "{}", ran.stderr);
	assert!(ran.stderr.contains("v[3]"), "{}", ran.stderr);
}

#[test]
fn a_session_error_after_an_await_names_its_input() {
	let ran = rnx(&["repl"], Some("1\nhost::test_pending(1).await; let v = []; v[3]\n"));
	assert_eq!(ran.code, Some(0));
	assert!(ran.stderr.contains("runtime error"), "{}", ran.stderr);
	assert!(ran.stderr.contains("input 2"), "{}", ran.stderr);
}
