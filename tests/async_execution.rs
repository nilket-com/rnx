//! Record 0032's gates: one driver for `run`, `eval` and the session, and
//! `.await` at each of them.
//!
//! Unix only and `test-support` only: the fixture that gives an input
//! something to await exists under that feature, and interruption is sent
//! as `SIGINT` the way a terminal sends it.
#![cfg(all(unix, feature = "test-support"))]
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::fd::AsRawFd;
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
	/// From the moment SIGINT was sent to the process's exit; zero when no
	/// signal was sent.
	since_signal: Duration,
}

fn rnx(args: &[&str], stdin: Option<&str>) -> Ran {
	let start = Instant::now();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(args)
		.stdin(if stdin.is_some() {
			Stdio::piped()
		} else {
			Stdio::null()
		})
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	if let Some(text) = stdin {
		child
			.stdin
			.take()
			.unwrap()
			.write_all(text.as_bytes())
			.unwrap();
	}
	let done = child.wait_with_output().unwrap();
	Ran {
		stdout: String::from_utf8_lossy(&done.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&done.stderr).into_owned(),
		code: done.status.code(),
		elapsed: start.elapsed(),
		since_signal: Duration::ZERO,
	}
}

/// When a test sends `SIGINT`.
enum Signal {
	/// Once the first `rnx_test::test_pending` await is running, which it
	/// reports from inside the operation; startup has its own bound.
	WhenPending,
	/// The Rune loop signals readiness on stdout just before entering.
	WhenLoopRuns,
	/// A line equal to the marker appears on stdout (after any banner), so a
	/// session input is known to be executing; bounded by `within`. A piped
	/// REPL may print its prompt on the same line, so fixtures print an empty
	/// line first and the marker starts its own line.
	WhenLine(&'static str, Duration),
	/// Wait for the actual request and withhold a response.
	WhenRequest(TcpListener),
}

fn ready_to_read(fd: &impl AsRawFd) -> std::io::Result<()> {
	ready_within(fd, Duration::from_secs(60))
}

fn ready_within(fd: &impl AsRawFd, within: Duration) -> std::io::Result<()> {
	let mut input = libc::pollfd {
		fd: fd.as_raw_fd(),
		events: libc::POLLIN,
		revents: 0,
	};
	// SAFETY: poll borrows one initialized descriptor for the duration of the call.
	let ms = i32::try_from(within.as_millis()).unwrap_or(i32::MAX);
	match unsafe { libc::poll(&mut input, 1, ms) } {
		1 => Ok(()),
		0 => Err(std::io::Error::new(
			std::io::ErrorKind::TimedOut,
			"the fixture did not become ready to interrupt",
		)),
		_ => Err(std::io::Error::last_os_error()),
	}
}

/// [`interrupt`], signalling once the pending await is running.
fn interrupted_when_pending(args: &[&str], stdin: Option<&str>) -> Ran {
	interrupt(args, stdin, Signal::WhenPending)
}

/// Start rnx, send it `SIGINT` once `signal` shows the input is executing, and
/// collect what it did. The wait is bounded: a run that ignores the interrupt
/// is killed and reported as such rather than hanging the suite.
fn interrupt(args: &[&str], stdin: Option<&str>, signal: Signal) -> Ran {
	let start = Instant::now();
	let ready = std::env::temp_dir().join(format!(
		"rnx-0032-ready-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	let _ = std::fs::remove_file(&ready);
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(args)
		.env("RNX_TEST_PENDING_READY", &ready)
		.env("NO_PROXY", "127.0.0.1")
		.env("no_proxy", "127.0.0.1")
		.stdin(if stdin.is_some() {
			Stdio::piped()
		} else {
			Stdio::null()
		})
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
	let mut consumed = String::new();
	let mut pending_connection = None;
	let readiness = (|| -> std::io::Result<()> {
		match signal {
			Signal::WhenPending => {
				let deadline = Instant::now() + Duration::from_secs(60);
				while !ready.exists()
					&& Instant::now() < deadline
					&& child.try_wait().unwrap().is_none()
				{
					std::thread::sleep(Duration::from_millis(10));
				}
				if !ready.exists() {
					return Err(std::io::Error::other(
						"rnx did not reach its pending await within 60 seconds of starting",
					));
				}
				let _ = std::fs::remove_file(&ready);
			}
			Signal::WhenLoopRuns => {
				let pipe = child.stdout.as_mut().unwrap();
				ready_to_read(pipe)?;
				let mut line = [0; 6];
				pipe.read_exact(&mut line)?;
				if &line != b"ready\n" {
					return Err(std::io::Error::other("unexpected loop readiness"));
				}
				consumed.push_str("ready\n");
			}
			Signal::WhenLine(marker, within) => {
				let pipe = child.stdout.as_mut().unwrap();
				let deadline = Instant::now() + within;
				let mut line = Vec::new();
				loop {
					let left = deadline.saturating_duration_since(Instant::now());
					if left.is_zero() || ready_within(pipe, left).is_err() {
						return Err(std::io::Error::other(
							"the fixture never printed its readiness line",
						));
					}
					let mut byte = [0; 1];
					if pipe.read(&mut byte)? == 0 {
						return Err(std::io::Error::other(
							"the fixture never printed its readiness line",
						));
					}
					consumed.push(byte[0] as char);
					if byte[0] == b'\n' {
						if line == marker.as_bytes() {
							break;
						}
						line.clear();
					} else {
						line.push(byte[0]);
					}
				}
			}
			Signal::WhenRequest(listener) => {
				ready_to_read(&listener)?;
				let (stream, _) = listener.accept()?;
				stream.set_read_timeout(Some(Duration::from_secs(5)))?;
				let mut reader = BufReader::new(stream);
				let mut line = String::new();
				reader.read_line(&mut line)?;
				if line != "GET /wait HTTP/1.1\r\n" {
					return Err(std::io::Error::other("unexpected fixture request"));
				}
				pending_connection = Some(reader.into_inner());
			}
		}
		Ok(())
	})();
	if let Err(error) = readiness {
		let _ = child.kill();
		let done = child.wait_with_output().unwrap();
		panic!(
			"{error}; no SIGINT sent\nstdout: {}\nstderr: {}",
			String::from_utf8_lossy(&done.stdout),
			String::from_utf8_lossy(&done.stderr)
		);
	}
	// Started before the external kill: it can be descheduled between sending
	// the signal and returning.
	let signalled = Instant::now();
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
	drop(pending_connection);
	consumed.push_str(&String::from_utf8_lossy(&done.stdout));
	// A later pending await may have written it again.
	let _ = std::fs::remove_file(&ready);
	Ran {
		stdout: consumed,
		stderr: String::from_utf8_lossy(&done.stderr).into_owned(),
		code: done.status.code(),
		elapsed: start.elapsed(),
		since_signal: signalled.elapsed(),
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
	let path = script(
		"await.rn",
		"pub async fn main(_) { rnx_test::test_pending(20).await }",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "20\n");
}

#[test]
fn a_file_waits_on_the_execution_engine_and_keeps_argv_and_errors() {
	for (name, source, code, stdout) in [
		(
			"argv.rn",
			"pub async fn main(args) { time::sleep(1).await?; args.len() }",
			0,
			"2\n",
		),
		(
			"arity.rn",
			"pub async fn main() { time::sleep(1).await?; 7 }",
			1,
			"",
		),
		(
			"located.rn",
			"pub async fn main(_) {\n    time::sleep(1).await?;\n    println!(\"once\");\n    1 / 0\n}",
			1,
			"once\n",
		),
	] {
		let path = script(name, source);
		let path = path.to_str().unwrap();
		let old = rnx(&["run", "--engine=legacy", path, "one", "two"], None);
		let new = rnx(&["run", "--debug-engine", path, "one", "two"], None);
		let (engine, diagnostics) = new.stderr.split_once('\n').unwrap();
		assert!(
			engine.starts_with("rnx: engine=execution "),
			"{name}: {engine}"
		);
		assert_eq!(new.code, Some(code), "{name}: {diagnostics}");
		assert_eq!(new.code, old.code, "{name}");
		assert_eq!(new.stdout, stdout, "{name}");
		assert_eq!(new.stdout, old.stdout, "{name}");
		assert_eq!(diagnostics, old.stderr, "{name}");
		match name {
			"arity.rn" => assert_eq!(
				diagnostics,
				"runtime error (no source position is available for this fault): Wrong number of arguments 1, expected 0\n"
			),
			"located.rn" => {
				assert!(diagnostics.contains("line 4, column 5: Division by zero"));
				assert!(diagnostics.contains("1 / 0"));
			}
			_ => assert!(diagnostics.is_empty()),
		}
	}
}

#[test]
fn an_execution_file_halts_with_its_full_budget_before_or_after_a_wait() {
	for (body, stdout) in [
		("loop {}", ""),
		(
			"time::sleep(1).await?; println!(\"waited\"); loop {}",
			"waited\n",
		),
	] {
		let path = script("budget.rn", &format!("pub async fn main(_) {{ {body} }}"));
		let path = path.to_str().unwrap();
		let old = rnx(
			&["run", "--engine=legacy", "--budget", "100000", path],
			None,
		);
		let new = rnx(&["run", "--debug-engine", "--budget", "100000", path], None);
		let (engine, diagnostics) = new.stderr.split_once('\n').unwrap();
		assert!(engine.starts_with("rnx: engine=execution "), "{engine}");
		assert_eq!(new.code, Some(1));
		assert_eq!(new.code, old.code);
		assert_eq!(new.stdout, stdout);
		assert_eq!(new.stdout, old.stdout);
		assert_eq!(
			diagnostics,
			"halted: 100000 work units exceeded; --budget N raises it\n"
		);
		assert_eq!(diagnostics, old.stderr);
	}
}

#[test]
fn other_file_async_forms_decline_before_effects() {
	for source in [
		"async fn helper() { 7 } pub async fn main(_) { println!(\"once\"); helper().await }",
		"pub async fn main(_) { println!(\"once\"); (async { 7 }).await }",
		"pub async fn main(_) { println!(\"once\"); let _pending = time::sleep(1); 7 }",
		"pub async fn main(_) { println!(\"once\"); select { _ = time::sleep(1) => 7 } }",
	] {
		let path = script("decline.rn", source);
		let path = path.to_str().unwrap();
		let ran = rnx(&["run", "--debug-engine", path], None);
		assert!(
			ran.stderr.starts_with("rnx: engine=legacy refusal="),
			"{source}: {}",
			ran.stderr
		);
		assert_eq!(ran.code, Some(0), "{source}: {}", ran.stderr);
		assert_eq!(ran.stdout, "once\n7\n", "{source}");
	}
}

#[test]
fn an_execution_file_interrupts_a_pending_host_wait_without_retrying() {
	for (legacy, engine) in [
		(true, "rnx: engine=legacy"),
		(false, "rnx: engine=execution native-requested=on"),
	] {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let url = format!("http://{}/wait", listener.local_addr().unwrap());
		let path = script(
			"interrupt-wait.rn",
			&format!(
				"pub async fn main(_) {{ println!(\"once\"); http::get({url:?}).await?.body }}"
			),
		);
		let mut args = vec!["run", "--debug-engine"];
		if legacy {
			args.push("--engine=legacy");
		}
		args.push(path.to_str().unwrap());
		let ran = interrupt(&args, None, Signal::WhenRequest(listener));
		assert_eq!(ran.code, Some(130), "{engine}: {}", ran.stderr);
		assert_eq!(ran.stdout, "once\n", "{engine}");
		assert_eq!(ran.stderr, format!("{engine}\ninterrupted\n"), "{engine}");
	}
}

#[test]
fn an_execution_file_interrupts_a_running_loop_between_slices() {
	let path = script(
		"interrupt-loop.rn",
		"pub async fn main(_) { println!(\"ready\"); loop {} }",
	);
	let ran = interrupt(
		&[
			"run",
			"--debug-engine",
			"--budget",
			"2000000000",
			path.to_str().unwrap(),
		],
		None,
		Signal::WhenLoopRuns,
	);
	assert_eq!(ran.code, Some(130), "{}", ran.stderr);
	assert_eq!(ran.stdout, "ready\n");
	assert_eq!(
		ran.stderr,
		"rnx: engine=execution native-requested=on\ninterrupted\n"
	);
}

#[test]
fn a_synchronous_file_still_runs() {
	let path = script(
		"sync.rn",
		"pub fn main(_) { let n = 0; for i in 0..10 { n += i; } n }",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "45\n");
}

#[test]
fn eval_may_await() {
	let ran = rnx(&["eval", "rnx_test::test_pending(20).await + 1"], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "21\n");
}

#[test]
fn a_session_input_may_await_at_the_top_level() {
	let ran = rnx(
		&["repl"],
		Some("let waited = rnx_test::test_pending(20).await;\nwaited + 1\n"),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains("21\n"), "{}", ran.stdout);
	assert!(ran.stderr.is_empty(), "{}", ran.stderr);
}

// Gate 3: the budget survives a pending poll.

fn completes_under(source: &str, budget: usize) -> bool {
	let path = script("budget.rn", source);
	let ran = rnx(
		&[
			"run",
			"--budget",
			&budget.to_string(),
			path.to_str().unwrap(),
		],
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
	let wait = "rnx_test::test_pending(1).await;";
	let tail = "n }";
	let at_start = format!("{head} {wait} {loop_a} {loop_b} {tail}");
	let in_middle = format!("{head} {loop_a} {wait} {loop_b} {tail}");
	let at_end = format!("{head} {loop_a} {loop_b} {wait} {tail}");
	let needed = smallest_budget(&at_start);
	assert!(
		needed > 30_000,
		"the scripts should dwarf a slice of 10,000: {needed}"
	);
	assert!(
		completes_under(&in_middle, needed),
		"the middle await needed more than {needed}"
	);
	assert!(
		completes_under(&at_end, needed),
		"the end await needed more than {needed}"
	);
	assert!(
		!completes_under(&in_middle, needed - 50),
		"the middle await got a fresh budget"
	);
	assert!(
		!completes_under(&at_end, needed - 50),
		"the end await got a fresh budget"
	);
}

#[test]
fn awaiting_in_a_loop_halts_for_budget_and_nothing_else() {
	let path = script(
		"await-loop.rn",
		"pub async fn main(_) { loop { rnx_test::test_pending(0).await; } }",
	);
	let ran = rnx(&["run", "--budget", "5000", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(1), "{}", ran.stderr);
	assert!(
		ran.stderr.starts_with("halted: 5000 work units exceeded"),
		"{}",
		ran.stderr
	);
	assert_eq!(ran.stdout, "");
}

// Gate 4: interruption reaches a pending future and a CPU loop.

#[test]
fn ctrl_c_ends_a_run_pending_on_a_future() {
	let path = script(
		"pending.rn",
		"pub async fn main(_) { rnx_test::test_pending(10000).await }",
	);
	let ran = interrupted_when_pending(&["run", path.to_str().unwrap()], None);
	assert_eq!(
		ran.code,
		Some(130),
		"stdout: {}\nstderr: {}",
		ran.stdout,
		ran.stderr
	);
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
	assert!(
		ran.stderr.starts_with("halted: 100000 work units exceeded"),
		"{}",
		ran.stderr
	);
}

#[test]
fn ctrl_c_ends_an_eval_pending_on_a_future() {
	let ran = interrupted_when_pending(&["eval", "rnx_test::test_pending(10000).await"], None);
	assert_eq!(ran.code, Some(130), "{}", ran.stderr);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
}

#[test]
fn ctrl_c_ends_a_session_input_pending_on_a_future_and_the_next_input_runs() {
	let ran = interrupted_when_pending(&["repl"], Some("rnx_test::test_pending(10000).await\n"));
	// The session stays up after the interrupt; it ended because its input
	// pipe was closed once the interrupt had landed. Nothing pending was
	// left: had the future outlived the input, the exit would have waited.
	assert_eq!(
		ran.code,
		Some(0),
		"stdout: {}\nstderr: {}",
		ran.stdout,
		ran.stderr
	);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.elapsed < Duration::from_secs(3), "{:?}", ran.elapsed);
}

#[test]
#[should_panic(expected = "did not reach its pending await")]
fn an_input_that_never_awaits_the_fixture_is_never_signalled() {
	// Readiness comes from inside the pending operation, never from time or
	// the banner: an input that never reaches it fails before any SIGINT.
	interrupted_when_pending(&["eval", "1 + 1"], None);
}

#[test]
fn after_an_interrupted_input_the_next_input_runs_to_completion() {
	// Two inputs on the pipe before the interrupt: the first is pending when
	// SIGINT arrives, and the second must then run and answer.
	let ran = interrupted_when_pending(
		&["repl"],
		Some("rnx_test::test_pending(10000).await\nrnx_test::test_pending(10).await + 1\n"),
	);
	assert_eq!(
		ran.code,
		Some(0),
		"stdout: {}\nstderr: {}",
		ran.stdout,
		ran.stderr
	);
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
	let ran = rnx(
		&["run", "--budget", "2000000", path.to_str().unwrap()],
		None,
	);
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
	let ran = interrupt(
		&["repl"],
		Some("println!(\"\"); println!(\"ready\"); loop { }\n1 + 1\n"),
		Signal::WhenLine("ready", Duration::from_secs(60)),
	);
	assert_eq!(
		ran.code,
		Some(0),
		"stdout: {}\nstderr: {}",
		ran.stdout,
		ran.stderr
	);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("2\n"), "{}", ran.stdout);
	assert!(
		ran.since_signal < Duration::from_secs(3),
		"{:?}",
		ran.since_signal
	);
}

#[test]
fn native_heavy_session_work_is_interrupted_and_the_next_input_works() {
	let ran = interrupt(
		&["repl"],
		Some(
			"println!(\"\"); println!(\"ready\"); for _ in 0..100000 { (0..5000).iter().count(); }\n1 + 1\n",
		),
		Signal::WhenLine("ready", Duration::from_secs(60)),
	);
	assert_eq!(
		ran.code,
		Some(0),
		"stdout: {}\nstderr: {}",
		ran.stdout,
		ran.stderr
	);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("2\n"), "{}", ran.stdout);
	assert!(
		ran.since_signal < Duration::from_secs(3),
		"{:?}",
		ran.since_signal
	);
}

// Classification: which wrapper and which path an input gets is the
// compiler's answer, not a reading of the text.

#[test]
fn a_string_that_looks_like_an_await_keeps_the_synchronous_slices() {
	// Readiness comes from this input, after compilation and cancellation arming.
	// :debug checks that adding the print did not select the async wrapper.
	let script = r#"
import os,queue,re,signal,subprocess,sys,threading,time
p=subprocess.Popen([sys.argv[1],'--no-splash','repl','--engine=legacy'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=dict(os.environ,RNX_CONFIG='',RNX_HISTORY='/dev/null',NO_COLOR='1'))
def line(stream,seconds,message):
 q=queue.Queue()
 threading.Thread(target=lambda:q.put(stream.readline()),daemon=True).start()
 try:return q.get(timeout=seconds)
 except queue.Empty:raise AssertionError(message)
try:
 p.stdin.write(b'let text = ".await"; println!("ready"); loop { }\n');p.stdin.flush()
 ready=line(p.stdout,10,'input never became ready')
 assert ready.endswith(b'ready\n'),('input never became ready',ready)
 started=time.monotonic()
 p.send_signal(signal.SIGINT)
 diagnostic=line(p.stderr,3,'input became ready but was not interrupted')
 assert diagnostic==b'interrupted\n',diagnostic
 p.stdin.write(b':debug\n1 + 1\n:quit\n');p.stdin.flush()
 out,err=p.communicate(timeout=3)
 assert p.returncode==0,(p.returncode,out,err)
 assert err==b'',err
 assert b'pub fn main(__rnx_state)' in out and b'pub async fn main' not in out,out
 assert b'let text = ".await"' in out,out
 out=re.sub(rb'\[\d+\] (?:> )?',b'',out)
 assert b'2\n' in out,out
 assert time.monotonic()-started<3,'interrupt and next input exceeded three seconds'
finally:
 if p.poll() is None:p.kill();p.wait()
"#;
	let result = Command::new("timeout")
		.args([
			"--kill-after=2s",
			"20s",
			"python3",
			"-c",
			script,
			env!("CARGO_BIN_EXE_rnx"),
		])
		.output()
		.unwrap();
	assert!(result.status.success(), "{result:?}");
}

#[test]
fn a_file_whose_string_mentions_await_is_still_synchronous() {
	let path = script(
		"stringy.rn",
		"pub fn main(_) { let text = \".await\"; text }",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains(".await"), "{}", ran.stdout);
}

#[test]
fn select_is_recognised_even_though_it_never_writes_await() {
	// `select` awaits without the word; the compiler refuses it outside an
	// async function, which is how it is recognised.
	let ran = rnx(
		&[
			"eval",
			"let a = rnx_test::test_pending(5); select { r = a => r }",
		],
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
	let ran = interrupt(
		&["repl"],
		Some(
			"async fn helper() { rnx_test::test_pending(1).await }\nprintln!(\"\"); println!(\"ready\"); loop { }\n1 + 1\n",
		),
		Signal::WhenLine("ready", Duration::from_secs(60)),
	);
	assert_eq!(
		ran.code,
		Some(0),
		"stdout: {}\nstderr: {}",
		ran.stdout,
		ran.stderr
	);
	assert!(ran.stderr.contains("interrupted"), "{}", ran.stderr);
	assert!(ran.stdout.contains("2\n"), "{}", ran.stdout);
	assert!(
		ran.since_signal < Duration::from_secs(4),
		"{:?}",
		ran.since_signal
	);
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
			&[
				"run",
				"--budget",
				&budget.to_string(),
				path.to_str().unwrap(),
			],
			None,
		);
		assert_eq!(ran.code, Some(1), "budget {budget}: {}", ran.stderr);
		if ran.stderr.contains("Panicked: boom")
			&& ran.stderr.contains("was exhausted at that point")
		{
			both = Some((budget, ran.stderr));
			break;
		}
	}
	let (budget, stderr) = both.expect(
		"no budget reported the panic together with the exhaustion: a failure on the last \
		 permitted instruction is being reported as a budget halt",
	);
	assert!(
		stderr.contains("Panicked: boom"),
		"budget {budget}: {stderr}"
	);
}

#[test]
fn an_entry_point_reached_through_an_alias_is_executed_as_what_it_is() {
	// `pub use ... as main` makes an async function the entry point with no
	// `async` token anywhere near `main`, and the compiled unit keeps its
	// calling convention private. Nothing reads the shape of a file, so this
	// runs; a version that read it halted here for `awaited`.
	let path = script(
		"alias.rn",
		"mod inner {\n    pub async fn work(_) { rnx_test::test_pending(5).await }\n}\npub use inner::work as main;\n",
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
		"pub async fn main(_) {\n    rnx_test::test_pending(1).await;\n    let v = [];\n    v[3]\n}\n",
	);
	let ran = rnx(&["run", path.to_str().unwrap()], None);
	assert_eq!(ran.code, Some(1));
	assert!(ran.stderr.contains("runtime error"), "{}", ran.stderr);
	assert!(ran.stderr.contains("line 4"), "{}", ran.stderr);
	assert!(ran.stderr.contains("v[3]"), "{}", ran.stderr);
}

#[test]
fn a_session_error_after_an_await_names_its_input() {
	let ran = rnx(
		&["repl"],
		Some("1\nrnx_test::test_pending(1).await; let v = []; v[3]\n"),
	);
	assert_eq!(ran.code, Some(0));
	assert!(ran.stderr.contains("runtime error"), "{}", ran.stderr);
	assert!(ran.stderr.contains("input 2"), "{}", ran.stderr);
}

#[test]
#[should_panic(expected = "the fixture never printed its readiness line")]
fn a_session_that_never_signals_readiness_is_never_interrupted() {
	// The control for the readiness line: without it, the harness reports the
	// missing premise instead of sending SIGINT and judging the result.
	interrupt(
		&["repl"],
		Some("1 + 1\n"),
		Signal::WhenLine("ready", Duration::from_secs(2)),
	);
}
