//! Record 0022: a child that is told what to do. What `process_bytes_input`
//! delivers, what it reports, and what it does when delivery cannot finish.
//!
//! Every case here runs under a harness that bounds itself, because the
//! failures being tested are hangs: a regression must fail the suite rather
//! than stop it.
#[path = "harness/commands.rs"]
mod commands;
mod harness;

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A child that stays for a distinctive ninety-seven seconds: long enough that
/// a bounded reap has to report it did not end, and distinctive enough that a
/// check for a leaked one cannot match another test's child.
///
/// **Spawned directly, with no shell.** `sh -c "sleep 97"` forks rather than
/// execs here — measured — so killing the shell leaves the sleeper behind, and
/// a control built on one would have been asking whether a shell can be
/// collected while an orphan accumulated on every run.
#[cfg(unix)]
const SLEEPER: (&str, &[&str]) = ("sleep", &["97"]);

/// Windows has no `sleep`, and `timeout` refuses the redirected standard input
/// this child is given. `ping` waits a second between echoes, so ninety-eight
/// of them to the loopback address is the same ninety-seven second stay, and it
/// needs no shell either — the substitution record 0025 already made for the
/// self-check's deadline probe.
#[cfg(windows)]
const SLEEPER: (&str, &[&str]) = ("ping", &["-n", "98", "127.0.0.1"]);

struct Ran {
	stdout: String,
	stderr: String,
	code: Option<i32>,
}

/// Run a script, and give up on it ourselves if it outlasts `limit`. The
/// deadline here belongs to the test, not to the thing being tested, which is
/// what lets "it hung" be a failure.
fn run_bounded(source: &str, limit: Duration) -> Ran {
	let source = commands::expand(source);
	let dir = std::env::temp_dir().join(format!(
		"rnx-child-input-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		// Building a mebibyte of input in Rune costs instructions; the budget
		// is the operator's to set, and here the operator is this test.
		.args(["--budget", "40000000"])
		.arg(&path)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	// Bounded throughout: the wait, the kill and the reap after it, and the
	// reading of the output — a pipe something still holds would otherwise
	// never reach the end of the file.
	//
	// A run that outstays the bound is killed, and that it outstayed is
	// remembered: the kill succeeding is not a reason to forget why it was
	// needed, which is what reporting the second reap's result alone would do.
	let mut outstayed = false;
	let mut code = harness::reaped_within(&mut child, limit);
	if code.is_none() {
		outstayed = true;
		let _ = child.kill();
		code = harness::reaped_within(&mut child, Duration::from_secs(3));
	}
	let out = harness::read_within(child.stdout.take().unwrap(), Duration::from_secs(3));
	let err = harness::read_within(child.stderr.take().unwrap(), Duration::from_secs(3));
	let _ = std::fs::remove_dir_all(&dir);
	// Cleanup is done; now it can be judged. A stream that could not be read
	// is a failure of its own rather than empty output.
	let out = out.unwrap_or_else(|why| panic!("{why}"));
	let err = err.unwrap_or_else(|why| panic!("{why}"));
	assert!(
		!outstayed,
		"the call did not return within {limit:?}: it hung.\nstdout: {out}\nstderr: {err}"
	);
	Ran {
		stdout: out,
		stderr: err,
		code: code.flatten(),
	}
}

/// A little over a mebibyte of input: more than a pipe holds, which on this
/// machine is 65,536 bytes, measured.
const BUILD_INPUT: &str =
	"let s = \"\";\n\tfor i in 0..50000 { s += \"0123456789abcdefghij\\n\" }\n";

#[test]
fn all_three_streams_under_pressure_at_once() {
	// The case decision 2 exists for: more than a pipe's worth going in while
	// more than a pipe's worth comes back on both reply streams, so every
	// direction is backed up at the same time. The child reports how many
	// bytes it read, which is how delivery is confirmed rather than assumed.
	let ran = run_bounded(
		&format!(
			"pub fn main(_) {{\n\t{BUILD_INPUT}\tlet r = host::process_bytes_input(@PRESSURE@, s.as_bytes(), 30000)?;\n\tprintln!(\"code={{:?}} out={{}} err={{}} truncated={{}} read={{}}\", r.code, r.stdout.len(), r.stderr.len(), r.truncated, String::from_utf8(host::process_bytes(@SUCCEED@, 1000)?.stdout)?);\n\tprintln!(\"tail={{:?}}\", String::from_utf8(r.stdout[200000..r.stdout.len()])?);\n\tOk(())\n}}\n"
		),
		Duration::from_secs(20),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains("code=0"), "{}", ran.stdout);
	assert!(ran.stdout.contains("out=200008"), "{}", ran.stdout);
	assert!(ran.stdout.contains("err=200000"), "{}", ran.stdout);
	assert!(ran.stdout.contains("truncated=false"), "{}", ran.stdout);
	// `wc -c` counted every byte of the input, so it was all delivered while
	// both reply streams were backed up.
	assert!(ran.stdout.contains("1050000"), "{}", ran.stdout);
}

/// How many threads a process has, from the kernel rather than from anything
/// the process says about itself.
#[cfg(unix)]
fn threads(pid: u32) -> usize {
	let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
	status
		.lines()
		.find_map(|l| l.strip_prefix("Threads:"))
		.and_then(|n| n.trim().parse().ok())
		.unwrap_or(0)
}

/// Start a run whose child holds the read end through an escaped descendant
/// and stays until released, with the deadline the caller asks for. It
/// announces the outcome on standard output.
#[cfg(unix)]
fn start_blocked_run(dir: &std::path::Path, deadline_ms: u32) -> Command {
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		format!(
			"pub fn main(args) {{\n\t{BUILD_INPUT}\tlet r = host::process_bytes_input(\"sh\", [\"-c\", args[0]], s.as_bytes(), {deadline_ms})?;\n\tprintln!(\"timed_out={{}} cancelled={{}}\", r.timed_out, r.cancelled);\n\tOk(())\n}}\n"
		),
	)
	.unwrap();
	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command
		.arg("run")
		.args(["--budget", "40000000"])
		.arg(&path)
		.arg(harness::holds_stdin_and_stays(dir))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	command
}

#[test]
#[cfg(unix)]
fn a_deadline_ends_the_call_while_delivery_is_blocked() {
	// This fixture needs a descendant that survives group termination.
	// Record 0025 decision 5 / Gate 10 establish why it cannot exist inside
	// rnx's Windows job. The Windows contract is exercised under test-support
	// by delivery_deadline_windows, whose holder is an in-job child that
	// simply never reads: the escaped lifetime is the unreachable premise,
	// not the deadline the delivery is bounded by.
	// The descendant holds the read end through a preserved descriptor and is
	// released only by the teardown, so the deadline has to be what ends the
	// call: there is a mebibyte still to write and nobody reading it.
	let dir = harness::scratch("child-deadline");
	let mut child = start_blocked_run(&dir, 200).spawn().unwrap();
	let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));
	let started = Instant::now();
	let announced = child
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(2)));
	let took = started.elapsed();
	let ended = harness::teardown(child, &dir);
	let (trouble, err) = (ended.trouble, ended.err);

	assert!(trouble.is_empty(), "{trouble:?}");
	assert!(held, "the descendant never took the descriptor");
	assert_eq!(
		announced.expect("no reader").as_deref(),
		Ok("timed_out=true cancelled=false"),
		"the deadline did not end the call: {err}"
	);
	// The descendant was still holding when the call came back, which is what
	// makes this about the delivery giving up rather than a closed pipe.
	assert!(
		took < Duration::from_secs(2),
		"the call took {took:?}, so it waited for the descendant"
	);
}

#[test]
#[cfg(unix)]
fn a_cancellation_ends_the_call_while_delivery_is_blocked() {
	// This fixture needs a descendant that survives group termination.
	// Record 0025 decision 5 / Gate 10 establish why it cannot exist inside
	// rnx's Windows job. The Windows contract is exercised under test-support
	// by console_interrupt::cancellation_collects_a_writer_blocked_on_child_input,
	// with the real cancellation mechanism gated by pipe_cancel_windows.
	// The same child with a long deadline, interrupted from outside: this
	// exercises the flag the delivery reads rather than a time it computes.
	let dir = harness::scratch("child-cancel");
	let mut child = start_blocked_run(&dir, 30000).spawn().unwrap();
	let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));
	let interrupted = Command::new("kill")
		.args(["-INT", &child.id().to_string()])
		.status()
		.map(|s| s.success())
		.unwrap_or(false);
	let started = Instant::now();
	let announced = child
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(2)));
	let took = started.elapsed();
	let ended = harness::teardown(child, &dir);
	let (trouble, err) = (ended.trouble, ended.err);

	assert!(trouble.is_empty(), "{trouble:?}");
	assert!(held, "the descendant never took the descriptor");
	assert!(interrupted, "the run could not be interrupted");
	assert_eq!(
		announced.expect("no reader").as_deref(),
		Ok("timed_out=false cancelled=true"),
		"the interrupt did not end the call: {err}"
	);
	assert!(
		took < Duration::from_secs(2),
		"the call took {took:?} after the interrupt"
	);
}

/// Start a run whose child hands the read end to an escaped descendant and
/// then leaves at once, so the call finishes with a delivery still unfinished.
/// It announces on standard output when the call returns.
#[cfg(unix)]
fn start_held_run(dir: &std::path::Path) -> Command {
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		format!(
			"pub fn main(args) {{\n\t{BUILD_INPUT}\tlet r = host::process_bytes_input(\"sh\", [\"-c\", args[0]], s.as_bytes(), 30000)?;\n\tprintln!(\"returned\");\n\t// Blocks until the teardown closes this end, spawning nothing, so the\n\t// thread count is the delivery's alone.\n\thost::stdin()?;\n\tOk(())\n}}\n"
		),
	)
	.unwrap();
	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command
		.arg("run")
		.args(["--budget", "40000000"])
		.arg(&path)
		.arg(harness::holds_stdin_until_released(dir))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	command
}

#[test]
#[cfg(unix)]
fn the_delivery_thread_is_gone_before_the_call_returns() {
	// Gate 10 establishes why this escaped-holder lifetime cannot be built
	// inside rnx's Windows job. Under test-support, delivery_exit_windows::
	// a_normal_child_exit_collects_the_unfinished_writer asks the normal-exit
	// collection contract with retained thread/process handles and a join
	// control. Record 0025 decision 5 maps the two fixtures explicitly.
	// The finding this gate exists for: an unfinished delivery used to be
	// detached, so anything it had to say — a failure this function promises
	// to report — was discarded, and it went on holding its copy of the input.
	//
	// Observed from outside, because the call returning cannot tell the two
	// apart: a detached writer and a stopped one both let the call return.
	// What distinguishes them is whether the thread is still there while the
	// escaped descendant still holds the read end, which the handshakes make
	// a fact rather than a hope.
	let dir = harness::scratch("child-threads");
	let mut child = start_held_run(&dir).spawn().unwrap();
	let pid = child.id();

	// Every step bounded, nothing judged yet.
	let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));
	let announced = child
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(3)));
	let counted = threads(pid);

	let ended = harness::teardown(child, &dir);
	let (trouble, err) = (ended.trouble, ended.err);

	assert!(trouble.is_empty(), "{trouble:?}");
	assert!(held, "the descendant never took the descriptor");
	assert_eq!(
		announced.expect("no reader").as_deref(),
		Ok("returned"),
		"the call did not return while the read end was held: {err}"
	);
	// A mebibyte was left undelivered and the descendant still held the read
	// end when this was counted. If the delivery were still there, it would
	// have been counted.
	assert_eq!(
		counted, 1,
		"{counted} threads while the read end was still held: the delivery was detached rather than stopped"
	);
}

#[test]
#[should_panic(expected = "the call did not return")]
fn the_harness_fails_within_its_bounds_against_a_runner_that_never_returns() {
	// The harness is what stands between a hanging regression and a stopped
	// suite, so it is exercised against a run that cannot finish. What must
	// happen is a failure inside the bounds — this test panicking is the pass
	// — with the runner reaped on the way out and **nothing left behind**.
	//
	// The run spins in Rune rather than waiting on a long-lived child: a
	// child would be in its own process group, and killing the runner cannot
	// clean up a group the runner never got to kill. A non-returning runner
	// with no helpers is what this is testing, so it must not create one.
	let dir = harness::scratch("child-nonreturning");
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		"pub fn main(_) {\n\tlet n = 0;\n\tfor i in 0..2000000000 { n += i }\n\tprintln!(\"returned {}\", n);\n\tOk(())\n}\n",
	)
	.unwrap();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.args(["--budget", "4000000000"])
		.arg(&path)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let announced = child
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(2)));
	// No descendant here, so there is nothing to acknowledge; what matters is
	// that the teardown reaps the runner and comes back.
	let _ended = harness::teardown(child, &dir);
	let _ = std::fs::remove_dir_all(&dir);
	assert_eq!(
		announced.expect("no reader").as_deref(),
		Ok("returned"),
		"the call did not return"
	);
}

#[test]
fn a_child_that_stops_reading_is_reported_as_itself() {
	// `head -1` takes one line and leaves. The write then fails with a broken
	// pipe, which is the child's prerogative and not an error: what comes
	// back is what it said and the status it exited with.
	let ran = run_bounded(
		&format!(
			"pub fn main(_) {{\n\t{BUILD_INPUT}\tlet r = host::process_bytes_input(@FIRST_LINE@, s.as_bytes(), 20000)?;\n\tprintln!(\"code={{:?}} out={{:?}}\", r.code, String::from_utf8(r.stdout)?);\n\tOk(())\n}}\n"
		),
		Duration::from_secs(20),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains("code=0"), "{}", ran.stdout);
	assert!(
		ran.stdout.contains("out=\"0123456789abcdefghij\\n\""),
		"{}",
		ran.stdout
	);
	assert!(
		!ran.stderr.contains("broken pipe") && !ran.stderr.contains("Broken pipe"),
		"a broken pipe was reported as a failure: {}",
		ran.stderr
	);
}

#[test]
fn nothing_to_say_and_nothing_to_hear_are_ordinary() {
	let ran = run_bounded(
		"pub fn main(_) {\n\tlet a = host::process_bytes_input(@COPY_STDIN@, b\"\", 20000)?;\n\tlet b = host::process_bytes_input(@SUCCEED@, b\"ignored\", 20000)?;\n\tlet c = host::process_bytes_input(@COPY_STDIN@, b\"echoed\", 20000)?;\n\tprintln!(\"{:?} {:?} {:?}\", a.code, b.code, String::from_utf8(c.stdout)?);\n\tOk(())\n}\n",
		Duration::from_secs(10),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "0 0 \"echoed\"\n");
}

#[test]
fn the_guarantees_of_the_older_forms_hold_for_this_one() {
	// Tested through the new function rather than assumed from the old ones:
	// a deadline, a killed process group, and a truncated capture at the cap
	// `host.rs` sets, which is two mebibytes and not the eight an earlier
	// draft of record 0022 claimed.
	let ran = run_bounded(
		"pub fn main(_) {\n\tlet slow = host::process_bytes_input(@SLEEP_5@, b\"x\", 200)?;\n\tprintln!(\"timed_out={} code={:?}\", slow.timed_out, slow.code);\n\tlet big = host::process_bytes_input(@THREE_MILLION_ZEROS@, b\"x\", 20000)?;\n\tprintln!(\"truncated={} out={}\", big.truncated, big.stdout.len());\n\tOk(())\n}\n",
		Duration::from_secs(10),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert!(ran.stdout.contains("timed_out=true"), "{}", ran.stdout);
	assert!(ran.stdout.contains("truncated=true"), "{}", ran.stdout);
	assert!(
		ran.stdout.contains(&format!("out={}", 2 * 1024 * 1024)),
		"the cap is not where host.rs puts it: {}",
		ran.stdout
	);
}

#[test]
fn the_older_forms_are_untouched() {
	// Three arguments still, and the same shape of answer, so the other three
	// ports cannot notice record 0022 happened.
	let ran = run_bounded(
		"pub fn main(_) {\n\tlet a = host::process(@ECHO_TEXT@, 20000)?;\n\tlet b = host::process_bytes(@ECHO_BYTES@, 20000)?;\n\tprintln!(\"{:?} {:?} {:?}\", a.stdout, a.code, String::from_utf8(b.stdout)?);\n\tOk(())\n}\n",
		Duration::from_secs(10),
	);
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	assert_eq!(ran.stdout, "\"text\\n\" 0 \"bytes\\n\"\n");
}

#[test]
fn the_harness_reports_a_stream_it_could_not_read() {
	// A stuck stream must not read as empty output. A gate asserting that
	// standard error was empty would otherwise pass for the wrong reason —
	// the pipe unread rather than the stream silent.
	let (program, args) = SLEEPER;
	let mut child = Command::new(program)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::null())
		.spawn()
		.unwrap_or_else(|why| panic!("`{program}` could not be started: {why}"));
	let outcome = harness::read_within(child.stdout.take().unwrap(), Duration::from_millis(200));
	let survived = harness::killed_within(&mut child, Duration::from_secs(5));
	assert!(survived.is_none(), "{}", survived.unwrap_or_default());
	assert!(
		outcome.is_err(),
		"a stream still held open was read as {outcome:?}"
	);
}

#[test]
#[should_panic(expected = "it hung")]
fn the_harness_remembers_a_run_that_outstayed_its_bound() {
	// The kill succeeding must not erase the reason it was needed: reporting
	// the second reap's result alone would turn a hang into a clean exit.
	// This test panicking is the pass.
	// Killing rnx cannot clean up its Unix process group, so use a brief
	// child here. The long SLEEPER used by the direct-child tests would leave
	// an orphan for ninety-seven seconds when this harness kills the runner.
	#[cfg(unix)]
	let (program, args) = ("sleep", &["1"][..]);
	#[cfg(windows)]
	let (program, args) = ("ping", &["-n", "2", "127.0.0.1"][..]);
	run_bounded(
		&format!(
			"pub fn main(_) {{ host::process({program:?}, {}, 20000)?; Ok(()) }}",
			serde_json::to_string(args).unwrap()
		),
		Duration::from_millis(200),
	);
}

#[test]
fn the_harness_never_reports_a_child_that_would_not_end_as_ended() {
	// The defect in the shape it took in the Windows breakaway probe: a
	// bounded reap times out, the kill's result is discarded, and the caller
	// is told the child ran — so a probe that asked nothing supplied a
	// passing control. The pair has to answer both halves separately: the
	// reap says it did not end, and the kill says whether anything survived.
	//
	// Portable, so the arm that failed there is gated here. Only the child
	// differs on Windows, and `SLEEPER` is where that difference lives — along
	// with why neither platform's child is reached through a shell.
	//
	// A child that cannot be started is said so plainly. The first Windows run
	// of this test failed on a missing `sleep`, and `unwrap` reported that as
	// an `Os` error beside a line number rather than as the name of a program
	// this machine does not have.
	let (program, args) = SLEEPER;
	let mut child = Command::new(program)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.unwrap_or_else(|why| panic!("`{program}` could not be started: {why}"));

	// Both collected before either is asserted. An assertion between them
	// panics out of the test on exactly the runs where the reap misbehaves —
	// which is when the child most needs collecting — so the failing run would
	// leak the very process this is about.
	let began = Instant::now();
	let reaped = harness::reaped_within(&mut child, Duration::from_millis(200));
	let survived = harness::killed_within(&mut child, Duration::from_secs(5));
	let took = began.elapsed();

	assert!(
		reaped.is_none(),
		"`{program} {}` was reaped as {reaped:?}",
		args.join(" ")
	);
	assert!(survived.is_none(), "{}", survived.unwrap_or_default());
	assert!(
		took < Duration::from_secs(5),
		"the bounded pair took {took:?}, so something waited without a bound"
	);
}
