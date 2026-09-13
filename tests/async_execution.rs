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
	// Each loop is several times the synchronous slice, so the property is
	// tested across what would have been many slice boundaries.
	let loop_a = "for i in 0..4000 { n += i; }";
	let loop_b = "for i in 0..4000 { n += i; }";
	let wait = "host::test_pending(1).await;";
	let tail = "n }";
	let at_start = format!("{head} {wait} {loop_a} {loop_b} {tail}");
	let in_middle = format!("{head} {loop_a} {wait} {loop_b} {tail}");
	let at_end = format!("{head} {loop_a} {loop_b} {wait} {tail}");
	let needed = smallest_budget(&at_start);
	assert!(needed > 30_000, "the scripts should dwarf a slice of 10,000: {needed}");
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
fn a_run_in_a_loop_is_bounded_by_its_budget_as_it_always_was() {
	// A file run was never interruptible mid-loop (record 0021), and record
	// 0032 does not make it so: the budget is the bound, sync or async.
	let path = script("loop.rn", "pub async fn main(_) { loop { } }");
	let ran = rnx(&["run", "--budget", "100000", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(1), "{}", ran.stderr);
	assert!(ran.stderr.starts_with("halted: 100000 instructions exceeded"), "{}", ran.stderr);
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

#[test]
fn a_nested_async_fn_that_outruns_a_slice_still_completes() {
	// The defect the review found: a nested async function of more than one
	// slice's instructions, under a budget that allows it, must return its
	// value. Slicing would have returned `()`.
	let path = script(
		"nested.rn",
		"async fn work() { let n = 0; for i in 0..2000 { n += i; } n }\npub async fn main(_) { work().await }\n",
	);
	let ran = rnx(&["run", "--budget", "2000000", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "1999000\n");
	// And in the session, where the helper is a declaration of an earlier
	// input and the await comes later.
	let ran = rnx(
		&["repl"],
		Some("async fn work() { let n = 0; for i in 0..2000 { n += i; } n }\nwork().await\n"),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains("1999000\n"), "{}", ran.stdout);
}

#[test]
fn a_synchronous_session_input_in_a_loop_is_still_interrupted_within_a_slice() {
	// Record 0002's promise for an input that never awaits, kept: the
	// synchronous path and its slices are unchanged.
	let ran = interrupted(&["repl"], Some("loop { }\n1 + 1\n"), Duration::from_millis(400));
	assert_eq!(ran.code, Some(0), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("2\n"), "{}", ran.stdout);
	assert!(ran.elapsed < Duration::from_secs(3), "{:?}", ran.elapsed);
}

// Classification: which wrapper and which path an input gets is the
// compiler's answer, not a reading of the text.

#[test]
fn a_string_that_looks_like_an_await_keeps_the_synchronous_slices() {
	// `.await` inside a string literal is not an await. The input must stay
	// on the synchronous path, where Ctrl-C ends a loop within a slice.
	let ran = interrupted(
		&["repl"],
		Some("let text = \".await\"; loop { }\n1 + 1\n"),
		Duration::from_millis(400),
	);
	assert_eq!(ran.code, Some(0), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("2\n"), "{}", ran.stdout);
	assert!(ran.elapsed < Duration::from_secs(3), "{:?}", ran.elapsed);
}

#[test]
fn a_file_whose_string_mentions_await_is_still_synchronous() {
	let path = script("stringy.rn", "pub fn main(_) { let text = \".await\"; text }");
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains(".await"), "{}", ran.stdout);
}

#[test]
fn select_is_recognised_even_though_it_never_writes_await() {
	// `select` awaits without the word; the compiler refuses it outside an
	// async function, which is how it is recognised.
	let ran = rnx(
		&["eval", "let a = host::test_pending(5); select { r = a => r }"],
		None,
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "5\n");
}

#[test]
fn an_awaited_async_block_is_recognised() {
	let ran = rnx(&["eval", "(async { 42 }).await"], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "42\n");
}

#[test]
fn declaring_an_async_function_does_not_make_the_input_that_declares_it_async() {
	// The declaration is hoisted above the wrapper, so an input that only
	// declares an awaiting function keeps the synchronous path itself.
	let ran = interrupted(
		&["repl"],
		Some("async fn helper() { host::test_pending(1).await }\nloop { }\n1 + 1\n"),
		Duration::from_millis(500),
	);
	assert_eq!(ran.code, Some(0), "stdout: {}\nstderr: {}", ran.stdout, ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("2\n"), "{}", ran.stdout);
	assert!(ran.elapsed < Duration::from_secs(4), "{:?}", ran.elapsed);
}

#[test]
fn a_failure_on_the_last_permitted_instruction_keeps_its_diagnostic() {
	// A failure that lands exactly as the budget runs out leaves the guard
	// exhausted, as a halt does. The error is what the reader needs; the
	// budget is said beside it and never instead of it.
	let path = script("boom.rn", "pub async fn main(_) { panic!(\"boom\") }");
	let mut both = None;
	for budget in 1..=32u32 {
		let ran = rnx(
			&["run", "--budget", &budget.to_string(), path.to_str().unwrap()],
			None,
		);
		assert_eq!(ran.code, Some(1), "budget {budget}: {}", ran.stderr);
		if ran.stderr.contains("Panicked: boom") && ran.stderr.contains("was exhausted at that point")
		{
			both = Some((budget, ran.stderr));
			break;
		}
	}
	let (budget, stderr) = both.expect(
		"no budget reported the panic together with the exhaustion: a failure on the last \
		 permitted instruction is being reported as a budget halt",
	);
	assert!(stderr.contains("Panicked: boom"), "budget {budget}: {stderr}");
}

#[test]
fn an_entry_point_reached_through_an_alias_is_executed_as_what_it_is() {
	// `pub use ... as main` makes an async function the entry point with no
	// `async` token anywhere near `main`, and the compiled unit keeps its
	// calling convention private. Nothing reads the shape of a file, so this
	// runs; a version that read it halted here for `awaited`.
	let path = script(
		"alias.rn",
		"mod inner {\n    pub async fn work(_) { host::test_pending(5).await }\n}\npub use inner::work as main;\n",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "5\n");

	let path = script(
		"alias-sync.rn",
		"mod inner {\n    pub fn work(_) { 7 }\n}\npub use inner::work as main;\n",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "7\n");
}

#[test]
fn a_failure_inside_one_slice_does_not_claim_the_whole_budget() {
	// A synchronous input is resumed a slice at a time, and a failure that
	// lands as a slice runs out leaves that slice's guard exhausted. The
	// input's budget is two billion instructions and is nowhere near spent,
	// so nothing may say it was.
	let ran = rnx(
		&["repl"],
		Some("let n=0; for i in 0..1664 { n += i; } let z=0; panic!(\"boom\")\n"),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stderr.contains("Panicked: boom"), "{}", ran.stderr);
	assert!(
		!ran.stderr.contains("was exhausted"),
		"a spent slice was reported as a spent budget: {}",
		ran.stderr
	);
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
