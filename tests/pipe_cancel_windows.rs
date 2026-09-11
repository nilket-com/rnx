//! Record 0025's amendment to gate 5: that `CancelIoEx` reached the
//! operation, which the end-to-end cancellation gate cannot say.
//!
//! In `run_child`'s cleanup the job ends before the writer is stopped, and on
//! Windows the closing of the last reader when a job dies can end a blocked
//! write on its own. So `cancelled=true` from a call is the public result and
//! not the mechanism. These two cases are the mechanism, around the real
//! `Pipe::begin` and `Pipe::stop`: the fixture owns both ends of a pipe,
//! nothing closes the read end and nothing drains it, and the only thing that
//! can end the write is the stop.
//!
//! Each case runs as its own process, bounded here. A cancellation that does
//! not reach the operation leaves a write blocked in the kernel, and a gate
//! about that has to be able to fail rather than stop the suite it is in.
#![cfg(all(windows, feature = "test-support"))]
mod harness;

use std::process::{Command, Stdio};
use std::time::Duration;

/// How long a controlled pipe is given to report. Far longer than either half
/// takes, because what is being bounded is the failure: a stop that never
/// reaches the write does not return at all.
const TO_REPORT: Duration = Duration::from_secs(20);

/// Run one half of the controlled pipe and give back what it reported.
///
/// Nothing is judged until the helper has been taken down, so a case that
/// fails does not also leave a process behind holding a pipe.
fn controlled(mode: &str) -> String {
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("test-pipe-control")
		.arg(mode)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap_or_else(|why| panic!("the controlled pipe could not be started: {why}"));
	let line = harness::line_within(child.stdout.take().unwrap(), TO_REPORT);
	let survived = harness::killed_within(&mut child, Duration::from_secs(5));
	let err = harness::read_within(child.stderr.take().unwrap(), Duration::from_secs(5));
	assert!(survived.is_none(), "{}", survived.unwrap_or_default());
	let err = err.unwrap_or_else(|why| panic!("{why}"));
	line.unwrap_or_else(|_| panic!("the {mode} control never reported within {TO_REPORT:?}: {err}"))
}

#[test]
fn a_stop_ends_a_blocked_write_while_the_read_end_is_still_open() {
	// The gate. `ERROR_OPERATION_ABORTED` is the whole of the evidence: it is
	// what a write says when the operation it was inside was cancelled, and
	// it is not what a write says when a reader went away or when the bytes
	// were finally taken.
	let line = controlled("cancel");
	assert!(
		line.contains("blocked_before_stop=true"),
		"the write was not blocked when the stop arrived, so it had nothing to reach: {line}"
	);
	assert!(
		line.contains("stopped=Reached"),
		"the stop found no operation to reach, so nothing was cancelled: {line}"
	);
	assert!(
		line.contains("outcome=failed error=995"),
		"the write did not end as ERROR_OPERATION_ABORTED: {line}"
	);
}

#[test]
fn without_a_stop_the_write_stays_unfinished_until_the_pipe_is_released() {
	// The control for the control. Without it the gate above would pass for a
	// write that was never blocked in the first place — and the release is a
	// drain rather than a close, because an end that came from closing the
	// read end is the very thing decision 5's amendment says cannot be told
	// apart from a cancellation.
	let line = controlled("held");
	assert!(
		line.contains("finished_before_release=false"),
		"the write finished on its own, so it was never blocked: {line}"
	);
	assert!(
		line.contains("outcome=returned error=none"),
		"the write did not end by being taken: {line}"
	);
	assert!(
		line.contains("drained=4194304"),
		"the drain did not take the whole write: {line}"
	);
}
