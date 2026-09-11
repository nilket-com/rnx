//! Whether a Ctrl-C on Windows reaches rnx and ends a call that is waiting.
//!
//! Record 0025 decision 4: `libc::signal(SIGINT, …)` becomes
//! `SetConsoleCtrlHandler`, the handler sets a flag, and every waiter reads
//! it. That had never run anywhere. This asks whether it does.
//!
//! Four things this fixture has to get right, each of which an earlier draft
//! got wrong:
//!
//! - **Isolation is watched for, and the watch is itself proved.** The child
//!   gets `CREATE_NEW_CONSOLE`, which is documented to give it a console of
//!   its own — but a documented flag is not a measurement. This process
//!   installs a handler and restores delivery to itself, so a leaked event
//!   would be *seen*; and a third case proves that detector can fire, by
//!   re-running this binary in a console of its own and raising an event
//!   there. Without that, "no leak" would be the answer a detector that could
//!   never fire also gives. A draft that used `CREATE_NO_WINDOW` and read its
//!   passing as isolation proved nothing at all: this runner ignores Ctrl-C,
//!   so a leaked event would have left it alive and looking isolated.
//! - **Readiness comes from the grandchild.** The interrupt must arrive while
//!   the call is genuinely waiting on a live child. A marker the script wrote
//!   *before* the call would say only that the script got that far, which an
//!   earlier draft did and which left the event able to arrive before any
//!   child existed. So the grandchild makes the marker and then stays alive,
//!   and rnx's hook waits for it.
//! - **Capture is file-backed.** Reader threads left blocked on a pipe are
//!   not collected by a timeout, so there are no reader threads: the child
//!   writes to files, and the files are read after it is gone.
//! - **The control is asserted to have taken effect.** A hook that failed to
//!   put the process into the state being controlled for would quietly
//!   measure the ordinary case twice.
//!
//! **Two moments, not one.** An interrupt while the call is **waiting** on a
//! live child is the first and easier case. Gate 6 is the second: the child
//! has already exited and the readers are still finishing. That window cannot
//! be timed from outside, so rnx says when it opens — the allowance's instant
//! is announced through `RNX_TEST_SIGNAL_CLEANUP_TO` — and the raise waits for
//! exactly that. A later delay would have landed somewhere unknowable and
//! could not have said where.

#![cfg(all(windows, feature = "test-support"))]

mod harness;

use std::fs::File;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FALSE, TRUE};
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};
use windows_sys::core::BOOL;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// How many console control events have reached *this* process. A count that
/// only ever rises, and is **never reset**: the cases run concurrently, and a
/// flag one of them cleared would erase what the other had already seen. Each
/// case reads it before and after its own run and compares.
static LEAKS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn noticed(event: u32) -> BOOL {
	if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
		LEAKS.fetch_add(1, Ordering::Relaxed);
		return TRUE;
	}
	FALSE
}

/// Make this process able to notice an event that escapes the child's
/// console, and survive it. Answers whether it can.
///
/// Handler first, then delivery, and delivery only if the handler took — the
/// same three-part shape `watch_for_interrupt` uses, for the same reasons.
/// A detector that failed to arm would otherwise report "no leak" for every
/// run, which is the one answer it must never give by default.
fn watch_for_leaks() -> Result<(), String> {
	static ARMED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
	ARMED
		.get_or_init(|| unsafe {
			if SetConsoleCtrlHandler(Some(noticed), TRUE) == FALSE {
				return Err(format!(
					"the leak detector's handler did not install: {}",
					std::io::Error::last_os_error()
				));
			}
			if SetConsoleCtrlHandler(None, FALSE) == FALSE {
				return Err(format!(
					"the leak detector could not restore delivery: {}",
					std::io::Error::last_os_error()
				));
			}
			Ok(())
		})
		.clone()
}

/// A console of the child's own. Documented to create one, which is why it is
/// used; `CREATE_NO_WINDOW` is not documented to.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Long enough that a call reaching it means the interrupt never arrived.
const CHILD_DEADLINE_MS: u64 = 15_000;

/// A bound on the whole run.
const LIMIT: Duration = Duration::from_secs(40);

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-interrupt-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

struct Ran {
	leaks_before: usize,
	stdout: String,
	stderr: String,
	took: Duration,
	code: Option<i32>,
}

/// Run a script in a console of its own, raising the interrupt once the
/// **grandchild** says it is running.
fn interrupted_run(dir: &Path, ignoring_first: bool) -> Result<Ran, String> {
	watch_for_leaks()?;
	let leaks_before = LEAKS.load(Ordering::Relaxed);

	// The child writes the marker itself and then blocks. So the event is
	// raised because a child exists and the call is waiting on it — not
	// because the script reached the line before the call.
	let ready = dir.join("ready");
	// A batch file rather than a command line. `&` and `>` have to survive
	// Rust's argument quoting and then `cmd`'s own parsing, and a first
	// version that tried lost them: the marker was never made, so the hook
	// waited for a readiness that never came and raised nothing.
	let batch = dir.join("ready-then-wait.bat");
	std::fs::write(
		&batch,
		format!(
			"@echo off\r\ntype nul > \"{}\"\r\nping -n 30 127.0.0.1\r\n",
			ready.display()
		),
	)
	.map_err(|e| format!("cannot write the batch file: {e}"))?;
	let script = format!(
		"pub fn main(_) {{ let r = host::process(\"cmd\", [\"/c\", {}], {CHILD_DEADLINE_MS})?; \
		 (r.cancelled, r.timed_out) }}",
		serde_escape(&batch.display().to_string())
	);
	let path = dir.join("waiting.rn");
	std::fs::write(&path, &script).map_err(|e| format!("cannot write the script: {e}"))?;

	// File-backed, so nothing is left blocked on a pipe if the run overruns.
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let out = File::create(&out_path).map_err(|e| format!("cannot make a stdout file: {e}"))?;
	let err = File::create(&err_path).map_err(|e| format!("cannot make a stderr file: {e}"))?;

	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command
		.arg("run")
		.arg(&path)
		.env("RNX_TEST_RAISE_INTERRUPT_WHEN", &ready)
		.creation_flags(CREATE_NEW_CONSOLE)
		.stdin(Stdio::null())
		.stdout(Stdio::from(out))
		.stderr(Stdio::from(err));
	// Set for one case and explicitly cleared for the other, so neither
	// inherits it from whatever ran before.
	if ignoring_first {
		command.env("RNX_TEST_IGNORE_CTRL_C_FIRST", "1");
	} else {
		command.env_remove("RNX_TEST_IGNORE_CTRL_C_FIRST");
	}

	let started = Instant::now();
	let mut child = command
		.spawn()
		.map_err(|e| format!("rnx did not start: {e}"))?;

	let deadline = Instant::now() + LIMIT;
	let status = loop {
		match child.try_wait() {
			Ok(Some(status)) => break status,
			Ok(None) => {}
			Err(e) => {
				let survived = harness::killed_within(&mut child, LIMIT);
				return Err(match survived {
					Some(survived) => format!("cannot wait for rnx: {e}; and {survived}"),
					None => format!("cannot wait for rnx: {e}"),
				});
			}
		}
		if Instant::now() >= deadline {
			let survived = harness::killed_within(&mut child, LIMIT);
			return Err(match survived {
				Some(survived) => format!("rnx did not end within {LIMIT:?}, and {survived}"),
				None => format!("rnx did not end within {LIMIT:?}"),
			});
		}
		std::thread::sleep(Duration::from_millis(10));
	};

	// A stream that could not be read is the run's failure, not empty output:
	// every caller below asks whether some line is present, and absence is
	// what an unreadable file would tell them.
	Ok(Ran {
		leaks_before,
		stdout: harness::wrote(&out_path)?,
		stderr: harness::wrote(&err_path)?,
		took: started.elapsed(),
		code: status.code(),
	})
}

/// A Rune string literal for `text`. The command carries backslashes and
/// quotes, and neither survives being pasted in raw.
fn serde_escape(text: &str) -> String {
	serde_json::to_string(text).expect("a string is serialisable")
}

/// Gate 5's public half. Unlike the waiting-child case, the event is gated
/// by pending I/O on the identified delivery thread. The child is ping,
/// which stays alive without reading stdin. The writer performs no file I/O
/// until delivery has ended, so its pending operation is the pipe write.
#[test]
fn cancellation_collects_a_writer_blocked_on_child_input() {
	use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
	use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
	use windows_sys::Win32::System::Threading::{
		GetProcessIdOfThread, GetThreadIOPendingFlag, OpenProcess, OpenThread, PROCESS_SYNCHRONIZE,
		THREAD_QUERY_INFORMATION, THREAD_SYNCHRONIZE, WaitForSingleObject,
	};

	let dir = scratch();
	let path = dir.join("blocked-input.rn");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let trigger = dir.join("interrupt-now");
	let received = dir.join("interrupt-received");
	let release = dir.join("release-worker");
	std::fs::write(&path, format!(
		"pub fn main(_) {{ let input = \"\"; for i in 0..50000 {{ input += \"0123456789abcdefghij\\n\" }} \
		 let r = host::process_bytes_input(\"ping\", [\"-n\", \"30\", \"127.0.0.1\"], input.as_bytes(), {CHILD_DEADLINE_MS})?; \
		 println!(\"{{:?}}\", (r.cancelled, r.timed_out)); host::stdin()?; Ok(()) }}"
	)).unwrap();
	watch_for_leaks().expect("the console leak detector must be armed");
	let leaks_before = LEAKS.load(Ordering::Relaxed);
	let started = Instant::now();
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.args(["--budget", "40000000"])
		.arg(&path)
		.env("RNX_TEST_DELIVERY_CONTROL", &dir)
		.env("RNX_TEST_RAISE_INTERRUPT_WHEN", &trigger)
		.env("RNX_TEST_SIGNAL_RAISED_TO", &received)
		.env_remove("RNX_TEST_IGNORE_CTRL_C_FIRST")
		.creation_flags(CREATE_NEW_CONSOLE)
		.stdin(Stdio::piped())
		.stdout(File::create(&out_path).unwrap())
		.stderr(File::create(&err_path).unwrap())
		.spawn()
		.expect("rnx did not start");

	// No assertion can bypass the teardown below. All waits and observations
	// are bounded, and retained handles prevent process/thread ID reuse.
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
		if numbers.len() != 3 || numbers[2] != 1_050_000 {
			return Err(format!("unexpected delivery identity: {identity:?}"));
		}
		// SAFETY: IDs are supplied by this run; failed opens are checked before
		// adoption. OwnedHandle closes each successful handle exactly once.
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
		let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, FALSE, numbers[1]) };
		if process.is_null() {
			return Err(format!(
				"cannot open child: {}",
				std::io::Error::last_os_error()
			));
		}
		let process = unsafe { OwnedHandle::from_raw_handle(process) };
		// SAFETY: both handles remain owned through every query and wait.
		if unsafe { GetProcessIdOfThread(thread.as_raw_handle()) } != child.id() {
			return Err("the writer belongs to a different process".into());
		}
		let until = Instant::now() + Duration::from_secs(5);
		loop {
			let mut pending = FALSE;
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
		if unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } != WAIT_TIMEOUT
			|| dir.join("finished-delivery").exists()
		{
			return Err("delivery or the child ended before the interrupt".into());
		}
		let interrupted_at = Instant::now();
		std::fs::write(&trigger, "interrupt").map_err(|e| e.to_string())?;
		if !harness::appeared(&received, Duration::from_secs(3))
			|| !harness::appeared(&dir.join("finished-delivery"), Duration::from_secs(3))
		{
			return Err("the interrupt did not end the pending delivery".into());
		}
		// Let an omitted join expose itself while the worker is held alive.
		// The normal call must remain unable to report throughout this window.
		let held_until = Instant::now() + Duration::from_millis(300);
		while Instant::now() < held_until {
			if !harness::wrote(&out_path)?.is_empty() {
				return Err("the call returned before collecting the held writer".into());
			}
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
		if unsafe { WaitForSingleObject(thread.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
			return Err("the writer was still alive when the call returned".into());
		}
		if unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
			return Err("the child was still alive when the call returned".into());
		}
		if child.try_wait().map_err(|e| e.to_string())?.is_some() {
			return Err("rnx exited before worker termination could be observed".into());
		}
		if interrupted_at.elapsed() >= Duration::from_secs(3) {
			return Err("cancellation did not return promptly".into());
		}
		Ok(())
	})();
	let mut trouble: Vec<String> = observed.err().into_iter().collect();
	if let Err(error) = std::fs::write(&release, "release") {
		trouble.push(error.to_string());
	}
	drop(child.stdin.take());
	let status = harness::reaped_within(&mut child, Duration::from_secs(3));
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
			trouble.extend(trouble_with(
				&Ran {
					leaks_before,
					stdout,
					stderr,
					took: started.elapsed(),
					code: status.flatten(),
				},
				false,
			));
		}
		(out, err) => trouble.push(format!("cannot read results: {out:?} {err:?}")),
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// What a run got wrong, if anything.
///
/// The result must be exactly `(true, false)` — cancelled, and **not** timed
/// out. `(true, true)` is a different outcome wearing the same word, and an
/// earlier draft accepted it.
fn trouble_with(ran: &Ran, ignoring_first: bool) -> Vec<String> {
	let mut trouble = Vec::new();
	let said = ran.stdout.trim();
	if said != "(true, false)" {
		trouble.push(format!("expected (true, false), got {said:?}"));
	}
	if ran.code != Some(0) {
		trouble.push(format!("expected exit 0, got {:?}", ran.code));
	}
	if !ran.stderr.contains("handler installed=true") {
		trouble.push(format!(
			"the handler did not install: {:?}",
			ran.stderr.trim()
		));
	}
	if !ran.stderr.contains("delivery restored=true") {
		trouble.push("delivery was not restored".to_owned());
	}
	if !ran.stderr.contains("raised a console interrupt: ok=true") {
		trouble.push(format!(
			"the event was not raised successfully: {:?}",
			ran.stderr.trim()
		));
	}
	// The control must have taken effect, or it is not a control.
	if ignoring_first && !ran.stderr.contains("now ignoring Ctrl-C: ok=true") {
		trouble.push(format!(
			"the process was not put into the ignoring state: {:?}",
			ran.stderr.trim()
		));
	}
	if !ignoring_first && ran.stderr.contains("now ignoring Ctrl-C") {
		trouble.push("the ordinary case was run with the ignore hook set".to_owned());
	}
	if ran.took >= Duration::from_millis(CHILD_DEADLINE_MS) {
		trouble.push(format!(
			"ran for {:?}, its whole deadline — the interrupt is not what ended it",
			ran.took
		));
	}
	if LEAKS.load(Ordering::Relaxed) > ran.leaks_before {
		trouble.push("the event escaped the child's console and reached this process".to_owned());
	}
	trouble
}

/// An interrupt ends a call that is waiting on a child, and the call reports
/// being cancelled rather than timed out.
#[test]
fn an_interrupt_ends_a_call_that_is_waiting() {
	let dir = scratch();
	let trouble = match interrupted_run(&dir, false) {
		Err(why) => vec![why],
		Ok(ran) => trouble_with(&ran, false),
	};
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// The same, from a process already in the "ignore Ctrl-C" state.
///
/// This is the control for the finding. Decision 4's first Windows draft
/// installed a handler and assumed delivery; a process in this state
/// registers the handler successfully and never runs it. Whether the test
/// runner happens to be ignoring Ctrl-C is not something a fixture
/// chooses — so without this case, the gate above establishes the fix only
/// under whatever state it inherited.
#[test]
fn an_interrupt_arrives_even_when_ctrl_c_was_being_ignored() {
	let dir = scratch();
	let trouble = match interrupted_run(&dir, true) {
		Err(why) => vec![why],
		Ok(ran) => trouble_with(&ran, true),
	};
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// The leak detector can actually fire.
///
/// Every other assertion about isolation rests on this one. A detector that
/// could never fire would report "no leak" for every run, including a run
/// that leaked — the one answer it must never give by default, and one no
/// amount of passing would expose.
///
/// Proving it cannot be done in this process: generating a console-wide event
/// here would reach the shell running the suite. So this test **re-runs
/// itself** in a console of its own, and in that copy raises an event and
/// reports whether the handler saw it. Nothing leaves that console.
#[test]
fn the_leak_detector_notices_an_event_in_its_own_console() {
	// The inner role: armed, raise, report. Reached only in the copy.
	if std::env::var_os("RNX_TEST_DETECTOR_SELF_CHECK").is_some() {
		watch_for_leaks().expect("the detector did not arm in the copy");
		let before = LEAKS.load(Ordering::Relaxed);
		// SAFETY: no arguments to get wrong. This copy has a console of its
		// own, so process group 0 reaches only itself.
		let raised = unsafe {
			windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)
		};
		assert!(raised != FALSE, "the copy could not raise an event");
		// The handler runs on a thread of the console's choosing, so this
		// waits for it rather than assuming it has already run.
		let until = Instant::now() + Duration::from_secs(10);
		while LEAKS.load(Ordering::Relaxed) == before && Instant::now() < until {
			std::thread::sleep(Duration::from_millis(10));
		}
		assert!(
			LEAKS.load(Ordering::Relaxed) > before,
			"the detector did not notice an event raised in its own console"
		);
		return;
	}

	// The outer role: run the copy, and require it to have noticed.
	let dir = scratch();
	let out_path = dir.join("stdout");
	let out = File::create(&out_path).expect("cannot make a stdout file");
	let err = out.try_clone().expect("cannot share the stdout file");

	let mut child = Command::new(std::env::current_exe().expect("no test binary"))
		.arg("--exact")
		.arg("the_leak_detector_notices_an_event_in_its_own_console")
		.arg("--nocapture")
		.env("RNX_TEST_DETECTOR_SELF_CHECK", "1")
		.creation_flags(CREATE_NEW_CONSOLE)
		.stdin(Stdio::null())
		.stdout(Stdio::from(out))
		.stderr(Stdio::from(err))
		.spawn()
		.expect("the copy did not start");

	let deadline = Instant::now() + LIMIT;
	let mut trouble = Vec::new();
	let status = loop {
		match child.try_wait() {
			Ok(Some(status)) => break Some(status),
			Ok(None) => {}
			Err(e) => {
				trouble.push(format!("cannot wait for the copy: {e}"));
				break None;
			}
		}
		if Instant::now() >= deadline {
			trouble.push(format!("the copy did not end within {LIMIT:?}"));
			break None;
		}
		std::thread::sleep(Duration::from_millis(10));
	};
	if status.is_none()
		&& let Some(survived) = harness::killed_within(&mut child, LIMIT)
	{
		trouble.push(survived);
	}
	let said = match harness::wrote(&out_path) {
		Ok(said) => said,
		Err(e) => {
			trouble.push(e);
			String::new()
		}
	};
	if let Some(status) = status
		&& !status.success()
	{
		trouble.push(format!("the copy failed ({status}): {}", said.trim()));
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// Gate 6: an interrupt during the **cleanup** is still reported.
///
/// Record 0023 promises `cancelled` whether the interrupt arrives while the
/// call waits or while it cleans up. The first is the case above. This is the
/// second, and it is a different moment rather than a later one: the child has
/// already exited, and the readers have been given the allowance but not yet
/// told to stop.
///
/// **Three handshakes, no delays.** An earlier draft held the readers back a
/// fixed 1,500 ms and called that "still finishing", which says only that they
/// started late — a reader that finished early on an idle machine would have
/// let this pass without asking anything. So:
///
/// 1. rnx writes `RNX_TEST_SIGNAL_CLEANUP_TO` at the instant the allowance is
///    published, which is the one point where the child is gone and the
///    readers are still going.
/// 2. The raise waits for that file, then raises, then writes
///    `RNX_TEST_SIGNAL_RAISED_TO`.
/// 3. The readers wait for **that** file before reading a byte.
///
/// So the readers are provably unfinished at the moment the event arrives:
/// they had not started. Each wait is bounded, so a handshake that never
/// completes fails the gate rather than hanging it.
///
/// The child exits normally, so `cancelled` is false when the cleanup begins.
/// Coming back true means it was seen during it.
#[test]
fn an_interrupt_during_the_cleanup_is_still_reported() {
	let dir = scratch();
	let mut trouble = Vec::new();

	if let Err(why) = watch_for_leaks() {
		trouble.push(why);
	}
	let leaks_before = LEAKS.load(Ordering::Relaxed);

	// A child that says something and leaves at once. Its output is what the
	// readers still have to drain when the interrupt arrives.
	let batch = dir.join("say-and-go.bat");
	std::fs::write(&batch, "@echo off\r\necho six!!\r\n").expect("cannot write the batch file");
	let script = format!(
		"pub fn main(_) {{ let r = host::process(\"cmd\", [\"/c\", {}], 10000)?; \
		 (r.cancelled, r.timed_out, r.cut_short) }}",
		serde_escape(&batch.display().to_string())
	);
	let path = dir.join("cleanup.rn");
	std::fs::write(&path, &script).expect("cannot write the script");

	let began = dir.join("cleanup-began");
	let raised = dir.join("interrupt-raised");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let out = File::create(&out_path).expect("cannot make a stdout file");
	let err = File::create(&err_path).expect("cannot make a stderr file");

	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		// The allowance is long so the phase cannot close before the event
		// lands. It is a bound, not the mechanism: the handshakes are.
		.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", "8000")
		.env("RNX_TEST_SIGNAL_CLEANUP_TO", &began)
		.env("RNX_TEST_RAISE_INTERRUPT_WHEN", &began)
		.env("RNX_TEST_SIGNAL_RAISED_TO", &raised)
		.env("RNX_TEST_READER_WAITS_FOR", &raised)
		.env_remove("RNX_TEST_READER_STARTS_LATE_MS")
		.env_remove("RNX_TEST_IGNORE_CTRL_C_FIRST")
		.creation_flags(CREATE_NEW_CONSOLE)
		.stdin(Stdio::null())
		.stdout(Stdio::from(out))
		.stderr(Stdio::from(err))
		.spawn()
		.expect("rnx did not start");

	let deadline = Instant::now() + LIMIT;
	let status = loop {
		match child.try_wait() {
			Ok(Some(status)) => break Some(status),
			Ok(None) => {}
			Err(e) => {
				trouble.push(format!("cannot wait for rnx: {e}"));
				break None;
			}
		}
		if Instant::now() >= deadline {
			trouble.push(format!("rnx did not end within {LIMIT:?}"));
			break None;
		}
		std::thread::sleep(Duration::from_millis(10));
	};
	if status.is_none()
		&& let Some(survived) = harness::killed_within(&mut child, LIMIT)
	{
		trouble.push(survived);
	}

	let mut unreadable = |e: String| {
		trouble.push(e);
		String::new()
	};
	let said = harness::wrote(&out_path).unwrap_or_else(&mut unreadable);
	let stderr = harness::wrote(&err_path).unwrap_or_else(&mut unreadable);
	if let Some(status) = status {
		if status.code() != Some(0) {
			trouble.push(format!("expected exit 0, got {:?}", status.code()));
		}
		if !began.exists() {
			trouble.push("the cleanup never announced itself".to_owned());
		}
		if !raised.exists() {
			trouble.push(
				"the interrupt was never received, so the readers were never released".to_owned(),
			);
		}
		// Both halves of the release, reported by rnx and asserted here. A
		// reader that timed out and read anyway may have finished before the
		// event landed, and a late event would then let this pass having asked
		// nothing at all.
		if stderr.contains("never received") {
			trouble.push("the event was raised but the handler never saw it".to_owned());
		}
		if stderr.contains("waited to be released and never was") {
			trouble.push("a reader gave up waiting and read regardless".to_owned());
		}
		if !stderr.contains("raised a console interrupt: ok=true") {
			trouble.push(format!("the event was not raised: {:?}", stderr.trim()));
		}
		if said.trim() != "(true, false, true)" {
			trouble.push(format!(
				"expected (true, false, true) from a cleanup interrupt, got {:?}",
				said.trim()
			));
		}
	}
	if LEAKS.load(Ordering::Relaxed) > leaks_before {
		trouble.push("the event escaped the child's console".to_owned());
	}

	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}
