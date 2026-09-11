//! Record 0025's Windows answer for a deadline that lands on a blocked
//! delivery, and the collection that has to follow it.
//!
//! The Unix fixture of the same name gives the read end to a descendant that
//! survives the group's termination. Decision 5 and gate 10 rule that
//! unreachable inside rnx's Windows job, and the record's amendment says what
//! survives the ruling: the escaped **lifetime** is the unreachable premise,
//! not the deadline contract. So the holder here is an ordinary in-job child
//! that simply never reads — `ping`, which has no reason to — and the
//! delivery is a mebibyte with nowhere to go.
//!
//! Three things this has to establish rather than assume, and each is a
//! failure of its own if it cannot be:
//!
//! - **The delivery really is blocked.** More bytes than a pipe holds is a
//!   reason to expect a block, not evidence of one. `GetThreadIOPendingFlag`
//!   on the writer's own thread is the evidence, and the writer's identity
//!   comes from rnx rather than from a search: a thread id guessed from a
//!   count would answer for whatever now wears it.
//! - **The deadline is what ended it.** The child outlives the call by half a
//!   minute and is asserted still alive when the delivery ends, so nothing the
//!   child did can be mistaken for the deadline. `cancelled` must be false: an
//!   interrupt would be a different contract wearing the same prompt return.
//! - **The writer is collected before the call reports.** The worker is held
//!   past the end of its delivery, and the call must stay silent for as long
//!   as it is held. Without that hold, a worker which happened to finish first
//!   would let a missing join look like a collection.
//!
//! Handles are opened and kept for every observation, so a thread or process
//! id cannot be reused underneath one.
#![cfg(all(windows, feature = "test-support"))]

mod harness;

use std::fs::File;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FALSE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
	GetProcessIdOfThread, GetThreadIOPendingFlag, OpenProcess, OpenThread, PROCESS_SYNCHRONIZE,
	THREAD_QUERY_INFORMATION, THREAD_SYNCHRONIZE, WaitForSingleObject,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The deadline the call is given. Short enough that the child, which lives
/// for about thirty seconds, cannot be what ends it.
const DEADLINE_MS: u64 = 200;

/// The input's size, which rnx announces back, so the gate can check it is
/// looking at the delivery it asked for. Fifty thousand twenty-byte lines.
const INPUT_BYTES: u32 = 1_050_000;

/// How long the worker is held after its delivery ends, for a call that never
/// joins it to expose itself.
const HELD: Duration = Duration::from_millis(300);

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-delivery-deadline-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

#[test]
fn a_deadline_ends_a_blocked_delivery_and_the_writer_is_collected() {
	let dir = scratch();
	let path = dir.join("blocked-input.rn");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let release = dir.join("release-worker");
	// `host::stdin()` at the end keeps rnx alive after the call, so the
	// writer's end and the child's can be observed from outside while it is
	// still running. The teardown closes that stdin to let it leave.
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{ let input = \"\"; for i in 0..50000 {{ input += \"0123456789abcdefghij\\n\" }} \
			 let r = host::process_bytes_input(\"ping\", [\"-n\", \"30\", \"127.0.0.1\"], input.as_bytes(), {DEADLINE_MS})?; \
			 println!(\"{{:?}}\", (r.timed_out, r.cancelled)); host::stdin()?; Ok(()) }}"
		),
	)
	.unwrap();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.args(["--budget", "40000000"])
		.arg(&path)
		.env("RNX_TEST_DELIVERY_CONTROL", &dir)
		.env("RNX_TEST_DELIVERY_HOLD", "deadline")
		.stdin(Stdio::piped())
		.stdout(File::create(&out_path).unwrap())
		.stderr(File::create(&err_path).unwrap())
		.spawn()
		.expect("rnx did not start");

	// Nothing is asserted until the teardown below has run: a failure here
	// must not also leave rnx, its child and a held thread behind.
	let observed = (|| -> Result<(), String> {
		if !harness::appeared(&dir.join("identity"), Duration::from_secs(5)) {
			return Err("the delivery identity was never published".into());
		}
		let identity = harness::wrote(&dir.join("identity"))?;
		let numbers: Vec<u32> = identity
			.split_whitespace()
			.map(|n| {
				n.parse()
					.map_err(|_| format!("invalid delivery identity: {identity:?}"))
			})
			.collect::<Result<_, _>>()?;
		if numbers.len() != 3 || numbers[2] != INPUT_BYTES {
			return Err(format!("unexpected delivery identity: {identity:?}"));
		}
		// SAFETY: both ids were published by this run, each open is checked
		// before it is adopted, and `OwnedHandle` closes each one once.
		let thread = unsafe {
			OpenThread(
				THREAD_QUERY_INFORMATION | THREAD_SYNCHRONIZE,
				FALSE,
				numbers[0],
			)
		};
		if thread.is_null() {
			return Err(format!(
				"cannot open writer: {}",
				std::io::Error::last_os_error()
			));
		}
		let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
		let holder = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, FALSE, numbers[1]) };
		if holder.is_null() {
			return Err(format!(
				"cannot open child: {}",
				std::io::Error::last_os_error()
			));
		}
		let holder = unsafe { OwnedHandle::from_raw_handle(holder) };
		// SAFETY: the handle is owned for the whole of this query.
		if unsafe { GetProcessIdOfThread(thread.as_raw_handle()) } != child.id() {
			return Err("the writer belongs to a different process".into());
		}

		// Blocked in a write, rather than merely large.
		let until = Instant::now() + Duration::from_secs(5);
		loop {
			let mut pending = FALSE;
			// SAFETY: the thread handle is owned and live for this call.
			if unsafe { GetThreadIOPendingFlag(thread.as_raw_handle(), &mut pending) } == FALSE {
				return Err(format!(
					"cannot query writer I/O: {}",
					std::io::Error::last_os_error()
				));
			}
			if pending != FALSE {
				break;
			}
			if Instant::now() >= until {
				return Err("the writer never had pending I/O".into());
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		if dir.join("finished-delivery").exists() {
			return Err("the delivery ended before it was seen blocked".into());
		}
		// Unfinished **while the child is alive**, which is what rules out a
		// child that left of its own accord. Asked here and not after: the
		// cleanup ends the job before it stops the writer, so by the time the
		// delivery ends the child is gone by rnx's own hand.
		// SAFETY: owned handle, and a zero wait does not block.
		if unsafe { WaitForSingleObject(holder.as_raw_handle(), 0) } != WAIT_TIMEOUT {
			return Err("the child was already gone while the writer was still blocked".into());
		}

		// The deadline ends it, promptly.
		let blocked_at = Instant::now();
		if !harness::appeared(&dir.join("finished-delivery"), Duration::from_secs(3)) {
			return Err("the deadline did not end the blocked delivery".into());
		}
		let ended_after = blocked_at.elapsed();

		// Held: the call cannot report while the writer is still alive.
		let held_until = Instant::now() + HELD;
		while Instant::now() < held_until {
			if !harness::wrote(&out_path)?.is_empty() {
				return Err("the call reported before collecting the held writer".into());
			}
			// SAFETY: as above.
			if unsafe { WaitForSingleObject(thread.as_raw_handle(), 0) } != WAIT_TIMEOUT {
				return Err("the writer did not remain held for the collection control".into());
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		std::fs::write(&release, "release").map_err(|e| e.to_string())?;
		let until = Instant::now() + Duration::from_secs(2);
		while harness::wrote(&out_path)?.is_empty() {
			if Instant::now() >= until {
				return Err("the call did not report after the writer was released".into());
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		// SAFETY: as above.
		if unsafe { WaitForSingleObject(thread.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
			return Err("the writer was still alive when the call reported".into());
		}
		if unsafe { WaitForSingleObject(holder.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
			return Err("the child was still alive when the call reported".into());
		}
		if child.try_wait().map_err(|e| e.to_string())?.is_some() {
			return Err("rnx exited before the collection could be observed".into());
		}
		if ended_after >= Duration::from_secs(3) {
			return Err(format!(
				"the deadline took {ended_after:?} to end the delivery"
			));
		}
		Ok(())
	})();

	let mut trouble: Vec<String> = observed.err().into_iter().collect();
	if let Err(error) = std::fs::write(&release, "release") {
		trouble.push(error.to_string());
	}
	drop(child.stdin.take());
	let status = harness::reaped_within(&mut child, Duration::from_secs(5));
	if status.is_none() {
		trouble.push("rnx failed to exit after teardown released stdin".into());
		if let Some(why) = harness::killed_within(&mut child, Duration::from_secs(5)) {
			trouble.push(why);
		}
	}
	match (harness::wrote(&out_path), harness::wrote(&err_path)) {
		(Ok(stdout), Ok(stderr)) => {
			if stderr.contains("never released") {
				trouble.push("the worker hold timed out".into());
			}
			if stdout.trim() != "(true, false)" {
				trouble.push(format!(
					"the call reported {:?} rather than a deadline: {stderr}",
					stdout.trim()
				));
			}
			if status.flatten() != Some(0) {
				trouble.push(format!("rnx exited {:?}: {stderr}", status.flatten()));
			}
		}
		(out, err) => trouble.push(format!("cannot read results: {out:?} {err:?}")),
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}
