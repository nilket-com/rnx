//! Record 0025's Windows answer for a delivery failure that arrives after the
//! call has begun cleaning up.
//!
//! The Unix fixture leaves the delivery unfinished with an escaped descendant
//! holding the read end, so the injected failure lands on the branch that
//! observes the stop. Decision 5 and gate 10 rule that holder out here, and
//! **substituting the child is not enough**: on Windows a write that is
//! cancelled, or that finds a reader gone, returns from the write itself and
//! never reaches the branch the injection lives on. A fixture built that way
//! would report `Abandoned` and pass for asking nothing.
//!
//! So what is arranged is not a blocked write but a still writer. It is held
//! at the top of its loop, before it looks at its stop flag, and released only
//! once the call has told it to stop — the one window in which a released
//! writer is certain to find the stop set. The failure it then reports has to
//! reach the script as an error rather than vanishing with the thread that
//! found it.
//!
//! Every wait here is bounded, and a hook released by its own timeout says so
//! on rnx's standard error, which this reads and fails on: a writer let go by
//! a timeout may have looked at a stop that was not yet set, which is the
//! ordinary case wearing this gate's name.
#![cfg(all(windows, feature = "test-support"))]

mod harness;

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// How long any one handshake is given. Generous, because what it bounds is a
/// failure: nothing here is expected to take it.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// The deadline the call is given. It is what ends the wait and starts the
/// cleanup, with the child still alive.
const DEADLINE_MS: u32 = 1_000;

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-delivery-failure-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

#[test]
fn a_failure_after_cleanup_begins_still_reaches_the_script_windows() {
	let dir = scratch();
	let path = dir.join("failure.rn");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let release = dir.join("release-writer");
	let held = dir.join("release-writer.held");
	let told = dir.join("writer-stopped");
	// The `?` is the point: a delivery failure must reach the script as an
	// error rather than being swallowed with the thread that found it. The
	// child never reads, so the delivery is unfinished however the writer is
	// scheduled — `ping` outlives the deadline by half a minute.
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{ let s = \"\"; for i in 0..50000 {{ s += \"0123456789abcdefghij\\n\" }} \
			 host::process_bytes_input(\"ping\", [\"-n\", \"30\", \"127.0.0.1\"], s.as_bytes(), {DEADLINE_MS})?; \
			 println!(\"the call succeeded, which it should not have\"); Ok(()) }}"
		),
	)
	.unwrap();
	let mut rnx = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.args(["--budget", "40000000"])
		.arg(&path)
		.env("RNX_TEST_DELIVERY_FAILS", "an injected write failure")
		.env("RNX_TEST_WRITER_HOLDS_BEFORE_STOP", &release)
		.env("RNX_TEST_SIGNAL_WRITER_STOP_TO", &told)
		.stdin(Stdio::null())
		.stdout(File::create(&out_path).unwrap())
		.stderr(File::create(&err_path).unwrap())
		.spawn()
		.expect("rnx did not start");

	// Nothing is judged until the teardown has run, and the release happens
	// whatever went wrong: a writer left held would keep the call from ever
	// returning, and this gate must fail rather than stop the suite.
	let mut trouble = Vec::new();
	let at_the_check = harness::appeared(&held, HANDSHAKE);
	if !at_the_check {
		trouble.push("the writer never reached its stop check".to_owned());
	}
	// Only now: before this the writer would look at a stop that had not been
	// set, take the ordinary path, and the injection would never be consulted.
	if at_the_check && !harness::appeared(&told, HANDSHAKE) {
		trouble.push("the call never told the writer to stop".to_owned());
	}
	if let Err(why) = std::fs::write(&release, "go") {
		trouble.push(format!("cannot release the writer: {why}"));
	}

	let status = harness::reaped_within(&mut rnx, Duration::from_secs(10));
	if status.is_none() {
		trouble.push("rnx did not return after the writer was released".to_owned());
		if let Some(why) = harness::killed_within(&mut rnx, Duration::from_secs(5)) {
			trouble.push(why);
		}
	}
	match (harness::wrote(&out_path), harness::wrote(&err_path)) {
		(Ok(said), Ok(complained)) => {
			if status.flatten() != Some(1) {
				trouble.push(format!(
					"a delivery failure was not a failure: {:?}: {complained}",
					status.flatten()
				));
			}
			// Not merely "an error": the injected one, by its own words.
			if !complained.contains("an injected write failure") {
				trouble.push(format!(
					"the injected failure did not reach the script: {complained:?}"
				));
			}
			// A hook released by its own timeout would have asked nothing.
			if complained.contains("never released") {
				trouble.push(format!("a hook timed out: {complained:?}"));
			}
			if !said.is_empty() {
				trouble.push(format!("the call reported success as well: {said:?}"));
			}
		}
		(out, err) => trouble.push(format!("cannot read results: {out:?} {err:?}")),
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}
