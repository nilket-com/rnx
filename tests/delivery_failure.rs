//! Record 0022 gate: a delivery failure that arrives **after** the call has
//! begun cleaning up is still collected and reported.
//!
//! The failure is injected, because it cannot be provoked: a non-blocking
//! pipe offers a full buffer, an interruption, or a closed reader, and a
//! closed reader is deliberately not a failure. So this file needs the
//! `test-support` feature, and says so rather than skipping quietly — without
//! it there is nothing here to run, and `cargo test --features test-support`
//! is what runs it.
#![cfg(feature = "test-support")]

#[path = "harness/commands.rs"]
mod commands;
mod harness;

use std::process::{Command, Stdio};
#[cfg(unix)]
use std::time::Duration;

#[test]
#[cfg(unix)]
fn a_failure_after_cleanup_begins_still_reaches_the_script() {
	// The holder is a descendant that survives group termination, which record
	// 0025 decision 5 and gate 10 rule out inside rnx's Windows job — and
	// substituting the child alone would not do: there a cancelled write, or
	// one that finds its reader gone, returns before the branch the injection
	// lives on is reached at all. Windows holds the writer still at that
	// branch instead, through
	// `delivery_failure_windows::a_failure_after_cleanup_begins_still_reaches_the_script_windows`.
	//
	// The delivery is left unfinished by an escaped descendant holding the
	// read end, so the injection lands where it matters: at the moment the
	// call observes its own stop flag and begins collecting.
	let dir = harness::scratch("delivery-failure");
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		"pub fn main(args) {\n\tlet s = \"\";\n\tfor i in 0..50000 { s += \"0123456789abcdefghij\\n\" }\n\t// The `?` is the point: a delivery failure must reach the script as an\n\t// error rather than being swallowed with the thread that found it.\n\tlet r = host::process_bytes_input(\"sh\", [\"-c\", args[0]], s.as_bytes(), 30000)?;\n\tprintln!(\"the call succeeded, which it should not have\");\n\tOk(())\n}\n",
	)
	.unwrap();
	let child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.args(["--budget", "40000000"])
		.arg(&path)
		.arg(harness::holds_stdin_until_released(&dir))
		.env("RNX_TEST_DELIVERY_FAILS", "an injected write failure")
		.env("RNX_TEST_SIGNAL_DELIVERY_FAILURE_TO", dir.join("injected"))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();

	// Wait for the descendant to hold the descriptor, so the injection lands
	// while the delivery is genuinely unfinished.
	let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));

	// And wait for the injection itself before letting go of the descriptor.
	// Releasing first is a race the test loses silently: the writer finishes
	// through a broken pipe, which is the child's prerogative rather than a
	// failure, and the call then exits 0 with nothing to report. Bounded, and
	// its absence is asserted below rather than being allowed to pass.
	let injected = harness::appeared(&dir.join("injected"), Duration::from_secs(10));

	// The teardown bounds everything from here: releasing the descendant and
	// waiting for it to acknowledge, reaping the runner, and reading what it
	// said. Nothing is judged until it has run.
	let ended = harness::teardown(child, &dir);
	assert!(ended.trouble.is_empty(), "{:?}", ended.trouble);
	assert!(held, "the descendant never took the descriptor");
	assert!(
		injected,
		"the delivery never reached the injected failure, so nothing was asked: {}",
		ended.err
	);
	assert_eq!(
		ended.code,
		Some(1),
		"a delivery failure was not a failure: {}",
		ended.err
	);
	assert!(
		ended
			.err
			.contains("the input could not be delivered: an injected write failure"),
		"the failure did not survive collection: {}",
		ended.err
	);
	assert!(
		!ended.out.contains("should not have"),
		"the call reported success: {}",
		ended.out
	);
}

#[test]
fn an_unreadable_stream_is_reported_and_excuses_nothing() {
	// Record 0023 gate 5's third case, injected because a read error on a
	// pipe cannot be provoked from a script. What it must show is both halves:
	// the flag reaches the caller, and being unreadable overrides the
	// exemption a truncated capture would have earned — a stream that could
	// not be read is not known to have stopped at a character boundary.
	let dir = harness::scratch("capture-unreadable");
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{ let r = host::process({}, 20000)?; println!(\"unreadable={{}} truncated={{}} cut_short={{}} out={{}}\", r.unreadable, r.truncated, r.cut_short, r.stdout.len()); Ok(()) }}",
			commands::echo("hello")
		),
	)
	.unwrap();
	let child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.env("RNX_TEST_CAPTURE_FAILS", "1")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let ended = harness::teardown(child, &dir);
	assert!(ended.trouble.is_empty(), "{:?}", ended.trouble);
	assert_eq!(ended.code, Some(0), "{}", ended.err);
	assert!(ended.out.contains("unreadable=true"), "{}", ended.out);
	// Nothing was read, so nothing was truncated: the flags are independent
	// and this one stands on its own.
	assert!(ended.out.contains("truncated=false"), "{}", ended.out);
	assert!(ended.out.contains("out=0"), "{}", ended.out);
}

/// How many threads a process has, from the kernel.
#[cfg(unix)]
fn threads(pid: u32) -> usize {
	let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
	status
		.lines()
		.find_map(|l| l.strip_prefix("Threads:"))
		.and_then(|n| n.trim().parse().ok())
		.unwrap_or(0)
}

#[test]
#[cfg(unix)] // Decision 5: reader_panic_windows holds and observes the actual stderr worker.
fn a_reader_that_panics_does_not_strand_the_other() {
	// The contract is that both readers are always collected. A `?` on the
	// first join returns before the second is joined, which strands a thread
	// — the same defect record 0022 fixed for the writer.
	//
	// Observing the call fail is not enough to tell the two apart: it fails
	// either way. What distinguishes them is whether the **other** reader is
	// still running afterwards, so the script keeps the error instead of
	// propagating it, stays alive with nothing of its own, and the thread
	// count is read from outside while a descendant still holds the pipe that
	// reader was on.
	let dir = harness::scratch("reader-panic");
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		"pub fn main(args) {\n\t// Kept rather than propagated, so this process is still here to be\n\t// counted.\n\tlet outcome = host::process(\"sh\", [\"-c\", args[0]], 30000);\n\tprintln!(\"returned {}\", outcome is Result);\n\thost::stdin()?;\n\tOk(())\n}\n",
	)
	.unwrap();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		// The child leaves at once; its descendant keeps **standard error**,
		// which is the reader that must still be collected when the standard
		// output reader panics.
		.arg(harness::holds_stderr_until_released(&dir))
		.env("RNX_TEST_READER_PANICS", "stdout")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let pid = child.id();

	let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));
	let announced = child
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(5)));
	let counted = threads(pid);
	let ended = harness::teardown(child, &dir);

	assert!(ended.trouble.is_empty(), "{:?}", ended.trouble);
	assert!(held, "the descendant never took the descriptor");
	assert!(
		announced.expect("no reader").is_ok(),
		"the call did not return: {}",
		ended.err
	);
	assert_eq!(
		counted, 1,
		"{counted} threads while the descendant still held standard error: \
		 the panicking reader took the other one's collection with it"
	);
}
