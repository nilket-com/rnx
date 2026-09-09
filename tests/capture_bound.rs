//! Record 0023: a capture that ends when the call does.
//!
//! The bound is on how long rnx keeps asking for more, so the cases here are
//! about a call returning while something else still holds its pipes — and
//! about telling a caller which of three things went wrong.
mod harness;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run a script that reports every capture flag, bounded by the harness.
fn reported(child: &str, deadline_ms: u32, allowance_ms: Option<u32>) -> (String, Duration) {
	reported_with(child, deadline_ms, allowance_ms, None)
}

/// The same, with the readers held back before their first attempt.
fn reported_with(
	child: &str,
	deadline_ms: u32,
	allowance_ms: Option<u32>,
	late_ms: Option<u32>,
) -> (String, Duration) {
	let dir = harness::scratch("capture");
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		format!(
			"pub fn main(args) {{\n\tlet r = host::process(\"sh\", [\"-c\", args[0]], {deadline_ms})?;\n\tprintln!(\"timed_out={{}} cancelled={{}} truncated={{}} cut_short={{}} unreadable={{}} out={{}} err={{}}\", r.timed_out, r.cancelled, r.truncated, r.cut_short, r.unreadable, r.stdout.len(), r.stderr.len());\n\tOk(())\n}}\n"
		),
	)
	.unwrap();
	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command
		.arg("run")
		.arg(&path)
		.arg(child)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	if let Some(ms) = allowance_ms {
		command.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", ms.to_string());
	}
	if let Some(ms) = late_ms {
		command.env("RNX_TEST_READER_STARTS_LATE_MS", ms.to_string());
	}
	let mut spawned = command.spawn().unwrap();
	let started = Instant::now();
	let line = spawned
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(10)));
	let took = started.elapsed();
	let ended = harness::teardown(spawned, &dir);
	assert!(ended.trouble.is_empty(), "{:?}", ended.trouble);
	(
		line.expect("no reader")
			.unwrap_or_else(|_| panic!("the call did not return: {}", ended.err)),
		took,
	)
}

/// A descendant outside the process group that keeps the named pipes and
/// lives well past any deadline here.
fn holds(what: &str) -> String {
	match what {
		"stdout" => "setsid sleep 6 2>/dev/null & sleep 6".to_owned(),
		"stderr" => "setsid sleep 6 >/dev/null & sleep 6".to_owned(),
		"both" => "setsid sleep 6 & sleep 6".to_owned(),
		"neither" => "setsid sleep 6 >/dev/null 2>&1 <&- & sleep 6".to_owned(),
		other => panic!("no such case: {other}"),
	}
}

#[test]
fn a_held_reply_pipe_no_longer_holds_the_call() {
	// The table in record 0023's context: before this cut, every row but the
	// first returned after the descendant's whole life — about 3,100 ms
	// against a 200 ms deadline.
	for which in ["neither", "stdout", "stderr", "both"] {
		let (line, took) = reported(&holds(which), 200, None);
		assert!(line.contains("timed_out=true"), "{which}: {line}");
		assert!(
			took < Duration::from_secs(2),
			"{which}: the call took {took:?}, so it waited for the descendant"
		);
	}
}

#[test]
fn a_child_that_ends_its_streams_spends_none_of_the_allowance() {
	// Clock-controlled rather than timed against a long deadline: with three
	// seconds of allowance available, an unnecessary wait cannot hide. The
	// three seconds take effect only under `test-support`; without it this
	// still asserts a prompt return, which is the weaker half of the same
	// claim.
	let (line, took) = reported("echo hello", 30000, Some(3000));
	assert!(line.contains("cut_short=false"), "{line}");
	assert!(line.contains("out=6"), "{line}");
	assert!(
		took < Duration::from_millis(1500),
		"the call took {took:?} of a 3000 ms allowance it did not need"
	);
}

#[test]
fn a_child_that_exits_while_a_pipe_is_held_returns_after_the_cleanup() {
	// Thirty second deadline, child gone in milliseconds, descendant holding
	// standard output for six seconds: the call must come back after the
	// allowance rather than after the deadline or the descendant.
	let (line, took) = reported("setsid sleep 6 2>/dev/null & sleep 0.1", 30000, None);
	assert!(line.contains("timed_out=false"), "{line}");
	assert!(line.contains("cut_short=true"), "{line}");
	assert!(
		took < Duration::from_secs(2),
		"the call took {took:?}: it waited for the deadline or the descendant"
	);
}

#[test]
fn the_three_shortfalls_are_independent() {
	// A stream that passes the cap and is then cut short reports both, which
	// an exclusive ending would have got wrong.
	let (line, _) = reported(
		"setsid sh -c 'head -c 3000000 /dev/zero; sleep 6' & sleep 0.1",
		30000,
		None,
	);
	assert!(line.contains("truncated=true"), "{line}");
	assert!(line.contains("cut_short=true"), "{line}");
	assert!(line.contains("unreadable=false"), "{line}");
	assert!(line.contains("out=2097152"), "{line}");

	// And the cap alone, with nothing holding anything.
	let (line, _) = reported("head -c 3000000 /dev/zero", 30000, None);
	assert!(line.contains("truncated=true"), "{line}");
	assert!(line.contains("cut_short=false"), "{line}");
}

#[test]
fn what_was_captured_is_still_captured() {
	// The allowance must not cost bytes that were there to be read.
	let (line, _) = reported(
		"head -c 1048576 /dev/zero; head -c 1048576 /dev/zero >&2",
		30000,
		None,
	);
	assert!(line.contains("out=1048576"), "{line}");
	assert!(line.contains("err=1048576"), "{line}");
	assert!(line.contains("truncated=false"), "{line}");
	assert!(line.contains("cut_short=false"), "{line}");
	assert!(line.contains("unreadable=false"), "{line}");
}

#[test]
fn a_descendant_that_never_stops_writing_does_not_hold_the_call() {
	// The case the group kill cannot stop, and the one no reasoning about a
	// pipeful covers: it is bounded by reading the clock before every read.
	let (line, took) = reported(
		"setsid sh -c 'while :; do echo xxxxxxxxxxxxxxxx; done' 2>/dev/null & sleep 6",
		200,
		None,
	);
	assert!(line.contains("cut_short=true"), "{line}");
	assert!(
		took < Duration::from_secs(2),
		"the call took {took:?} against a writer that never stops"
	);
}

#[test]
fn a_completeness_check_can_see_a_prefix_that_truncation_cannot() {
	// The state the ports could not detect before this cut: a child exits 0,
	// a descendant keeps the pipe with bytes still in it, and the call
	// returns a prefix. `truncated` is false — nothing hit the size cap — so
	// a check written against it alone reports success from partial output,
	// which is what the ports did and what decision 3 corrects.
	let (line, _) = reported(
		"printf 'partial'; setsid sleep 6 2>/dev/null & sleep 0.1",
		30000,
		None,
	);
	assert!(line.contains("truncated=false"), "{line}");
	assert!(line.contains("cut_short=true"), "{line}");
	// And there really is output, so this is a prefix rather than nothing:
	// the case is only interesting because the bytes look like an answer.
	assert!(line.contains("out=7"), "{line}");
}

/// Needs the allowance stretched so an interrupt can land inside it: with the
/// ordinary hundred milliseconds the cleanup is over before a test could aim
/// at it, and the case would pass for the wrong reason. So it runs under
/// `test-support`, and says so rather than being quietly weaker by default.
#[cfg(feature = "test-support")]
#[test]
fn an_interrupt_during_the_cleanup_is_still_an_interrupt() {
	// Distinct from cancelling while the child is still being waited for.
	// Once the child has exited, the wait loop has stopped watching for an
	// interrupt, so before this cut a SIGINT arriving during the cleanup was
	// simply ignored: the call waited out the whole allowance and reported
	// `cancelled=false` with a capture cut short for no reason it could name.
	//
	// The allowance is set to a second so the interrupt lands inside it.
	let dir = harness::scratch("capture-cancel-cleanup");
	let path = dir.join("script.rn");
	std::fs::write(
		&path,
		"pub fn main(args) {\n\tlet r = host::process(\"sh\", [\"-c\", args[0]], 30000)?;\n\tprintln!(\"cancelled={} cut_short={}\", r.cancelled, r.cut_short);\n\tOk(())\n}\n",
	)
	.unwrap();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		// The child leaves at once; its descendant keeps standard output, so
		// the call is in its cleanup with a reader still going.
		.arg("setsid sleep 6 2>/dev/null & sleep 0.1")
		.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", "1000")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	// Long enough for the child to have exited and the cleanup to be under
	// way, short enough to be inside the second of allowance.
	std::thread::sleep(Duration::from_millis(350));
	let interrupted = Command::new("kill")
		.args(["-INT", &child.id().to_string()])
		.status()
		.map(|s| s.success())
		.unwrap_or(false);
	let started = Instant::now();
	let line = child
		.stdout
		.take()
		.map(|out| harness::line_within(out, Duration::from_secs(5)));
	let took = started.elapsed();
	let ended = harness::teardown(child, &dir);

	assert!(ended.trouble.is_empty(), "{:?}", ended.trouble);
	assert!(interrupted, "the run could not be interrupted");
	let line = line.expect("no reader").expect("the call did not return");
	assert!(
		line.contains("cancelled=true"),
		"an interrupt during cleanup was ignored: {line}"
	);
	assert!(line.contains("cut_short=true"), "{line}");
	assert!(
		took < Duration::from_millis(600),
		"the call took {took:?} of the second of allowance it was told to abandon"
	);
}

/// Record 0025's correction: the readers are given the cleanup allowance
/// before any of them is reached.
///
/// Held back by a known amount, with an allowance longer than it, the two
/// orderings answer differently. Draining first: the reader wakes, finds the
/// six bytes the child left in the pipe, and reads to the end. Reaching the
/// readers as soon as the wait ends: the reader wakes already stopped, and
/// reports a capture cut short that lost output nothing was going to reclaim.
///
/// The delay needs an injected hook because a child cannot arrange one: its
/// output is ordinarily read while it is still running, which is why the
/// defect was invisible to every other gate here.
#[cfg(feature = "test-support")]
#[test]
fn the_readers_are_drained_before_any_of_them_is_reached() {
	let (line, took) = reported_with("echo hello", 30000, Some(3000), Some(300));
	assert!(line.contains("out=6"), "output was lost: {line}");
	assert!(line.contains("cut_short=false"), "{line}");
	assert!(line.contains("truncated=false"), "{line}");
	assert!(line.contains("unreadable=false"), "{line}");
	assert!(
		took < Duration::from_millis(2000),
		"the call took {took:?}: it spent allowance it did not need"
	);
}
