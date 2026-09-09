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

mod harness;

use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn a_failure_after_cleanup_begins_still_reaches_the_script() {
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
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();

	// Wait for the descendant to hold the descriptor, so the injection lands
	// while the delivery is genuinely unfinished.
	let held = harness::appeared(&dir.join("ready"), Duration::from_secs(5));

	// The teardown bounds everything from here: releasing the descendant and
	// waiting for it to acknowledge, reaping the runner, and reading what it
	// said. Nothing is judged until it has run.
	let ended = harness::teardown(child, &dir);
	assert!(ended.trouble.is_empty(), "{:?}", ended.trouble);
	assert!(held, "the descendant never took the descriptor");
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
