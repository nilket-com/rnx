//! Record 0025's Windows answer for a reply pipe still held when the deadline
//! lands, with the holder inside rnx's job rather than escaped from it.
//!
//! The Unix fixture of the same name gives the reply pipes to a `setsid`
//! descendant that outlives the group's termination. Decision 5 and gate 10
//! rule that unreachable here, and the record's amendment says what survives
//! the ruling: the escaped **lifetime** is the unreachable premise, not the
//! contract that a held pipe must not hold the call. So the holder is an
//! ordinary grandchild, in the job like everything else rnx starts, and the
//! question asked of it is the one that is still askable — a deadline ends the
//! call promptly, and the job ends both processes rather than the call waiting
//! on either.
//!
//! The normal-exit case releases the direct child only after the grandchild
//! has emitted both markers and both processes have been observed alive. It
//! requires the child's chosen exit 7, no timeout or cancellation, and the
//! grandchild's termination while rnx is still alive. With test-support the
//! cleanup allowance is stretched so job-handle drop cannot hide an omitted
//! explicit job end behind the usual short allowance.
//!
//! **The holder really does hold the pipes**, which is the part a fixture can
//! get wrong without noticing. A grandchild started with no redirection
//! inherits its parent's handles, and those are rnx's capture pipes: measured
//! here, a pipe so inherited stays open after the direct child leaves. Each of
//! the four cases below detaches a different pair of the grandchild's streams,
//! so what varies is exactly which of rnx's pipes are still held.
//!
//! **Both processes are watched by handle, not by id.** They are opened while
//! both are alive and kept for every observation after, so nothing that later
//! wears the same number can answer for them.
#![cfg(windows)]

mod harness;

use std::fs::File;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{FALSE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
	GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
	WaitForSingleObject,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The call's deadline. Long enough that the holder has published its ids and
/// the handles are open before it lands, short enough to keep the case quick.
const DEADLINE_MS: u32 = 2_000;

/// How long the holder and its grandchild stay. Far past the deadline, so a
/// call that waited for either would be unmistakable.
const HOLDER_SECONDS: u32 = 30;

/// What the call must not exceed. The holder's own life is the thing being
/// ruled out, and this is comfortably inside it.
const PROMPTLY: Duration = Duration::from_secs(8);

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-capture-hold-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// A process still running, by an owned handle rather than by its id.
fn opened(id: u32, what: &str) -> Result<OwnedHandle, String> {
	// SAFETY: the id was published by this run's own holder, the open is
	// checked before it is adopted, and `OwnedHandle` closes it exactly once.
	let handle = unsafe {
		OpenProcess(
			PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
			FALSE,
			id,
		)
	};
	if handle.is_null() {
		return Err(format!(
			"cannot open {what} {id}: {}",
			std::io::Error::last_os_error()
		));
	}
	Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// Whether a process has ended. A zero wait, so this never blocks.
fn ended(handle: &OwnedHandle) -> bool {
	// SAFETY: the handle is owned for the whole of this call.
	unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) == WAIT_OBJECT_0 }
}

fn still_running(handle: &OwnedHandle) -> bool {
	// SAFETY: as above.
	unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) == WAIT_TIMEOUT }
}

/// The child rnx runs: a holder that starts a grandchild keeping the streams
/// named and publishes both IDs. It either stays past the deadline or waits
/// for an explicit release and exits 7, leaving its grandchild in the job.
///
/// A stream the grandchild does **not** keep is redirected to the holder,
/// which stays alive holding it: closing it instead would have ended the
/// grandchild's write rather than merely detaching it, and a grandchild that
/// died of a broken pipe is not a holder at all.
///
/// The grandchild writes a marker to each stream before it settles down to
/// wait, so what rnx captured says which of its pipes the grandchild actually
/// had. Without that the four cases could all be the same case. `OUT` is three
/// bytes; the standard error marker is four, because `set /p` takes the space
/// before the redirection as part of the text it prints.
///
/// `settles` is what the grandchild does once its markers are out: waiting, or
/// writing without stopping. It is a parameter rather than two near-copies of
/// this function, because everything else about the two cases has to be the
/// same for their answers to be comparable.
fn holder(
	ids: &Path,
	keeps_stdout: bool,
	keeps_stderr: bool,
	exit_release: Option<&Path>,
	settles: &str,
) -> String {
	let quote = |p: String| format!("'{}'", p.replace('\'', "''"));
	let quoted = quote(ids.display().to_string());
	// Written under another name and renamed onto this one, so the gate can
	// never read a file that exists and is still empty. The first draft did
	// exactly that and reported ids it could not parse.
	let partial = quote(format!("{}.tmp", ids.display()));
	let ready = ids.with_extension("grandchild-ready");
	let marker = if exit_release.is_some() {
		format!("type nul > \"{}\"& ", ready.display())
	} else {
		String::new()
	};
	let child_arguments = quote(format!(
		"/c echo|set /p=OUT& echo|set /p=ERR 1>&2& {marker}{settles}"
	));
	let wait_for = |path: &Path| {
		format!(
			"$until = [DateTime]::UtcNow.AddSeconds(20); while (![IO.File]::Exists({})) {{ if ([DateTime]::UtcNow -ge $until) {{ throw 'holder handshake timed out' }}; [Threading.Thread]::Sleep(5) }}; ",
			quote(path.display().to_string())
		)
	};
	// In the normal-exit case publish IDs only after the grandchild has
	// actually written its markers. Releasing the parent sooner could kill
	// the grandchild before it ever demonstrated inheriting the pipes.
	let ready_wait = if exit_release.is_some() {
		wait_for(&ready)
	} else {
		String::new()
	};
	let finish = match exit_release {
		Some(path) => format!("{} exit 7", wait_for(path)),
		None => format!("Start-Sleep -Seconds {HOLDER_SECONDS}"),
	};
	let script = format!(
		"$ErrorActionPreference = 'Stop'; \
		 $si = New-Object Diagnostics.ProcessStartInfo; \
		 $si.FileName = 'cmd.exe'; \
		 $si.Arguments = {child_arguments}; \
		 $si.UseShellExecute = $false; \
		 $si.RedirectStandardOutput = ${}; \
		 $si.RedirectStandardError = ${}; \
		 $p = [Diagnostics.Process]::Start($si); \
		 {ready_wait} \
		 [IO.File]::WriteAllText({partial}, \"$PID $($p.Id)\"); \
		 Move-Item -Force -LiteralPath {partial} -Destination {quoted}; \
		 {finish}",
		if keeps_stdout { "false" } else { "true" },
		if keeps_stderr { "false" } else { "true" },
	);
	let executable = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
		.join("System32/WindowsPowerShell/v1.0/powershell.exe");
	format!(
		"{}, {}",
		serde_json::to_string(executable.to_str().expect("a UTF-8 path")).unwrap(),
		serde_json::to_string(&["-NoProfile", "-NonInteractive", "-Command", &script]).unwrap()
	)
}

/// One case: the grandchild keeps the named streams. Either the deadline or
/// the child's controlled normal exit must return without waiting for it.
fn a_held_pipe_does_not_hold_the_call(
	which: &str,
	keeps_stdout: bool,
	keeps_stderr: bool,
	normal_exit: bool,
) {
	let dir = scratch();
	let path = dir.join("held.rn");
	let ids = dir.join("ids");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let release = dir.join("exit-holder");
	let deadline_ms = if normal_exit { 30_000 } else { DEADLINE_MS };
	// `host::stdin()` after the call keeps rnx alive, so the ends of the child
	// and its grandchild can be observed from outside while it is still
	// running. The teardown closes that stdin to let it leave.
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{ let r = host::process({}, {deadline_ms})?; \
			 println!(\"code={{:?}} timed_out={{}} cancelled={{}} out={{}} err={{}}\", r.code, r.timed_out, r.cancelled, r.stdout.len(), r.stderr.len()); \
			 host::stdin()?; Ok(()) }}",
			holder(
				&ids,
				keeps_stdout,
				keeps_stderr,
				normal_exit.then_some(&release),
				&format!("ping -n {HOLDER_SECONDS} 127.0.0.1 > NUL")
			)
		),
	)
	.unwrap();
	let started = Instant::now();
	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command
		.arg("run")
		.arg(&path)
		.stdin(Stdio::piped())
		.stdout(File::create(&out_path).unwrap())
		.stderr(File::create(&err_path).unwrap());
	if normal_exit {
		// Under test-support, an omitted job end must visibly spend this
		// allowance before the job handle's drop could clean up on its behalf.
		command.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", "12000");
	}
	let mut rnx = command.spawn().expect("rnx did not start");

	// Nothing is asserted until the teardown below has run: a failure here
	// must not also leave rnx, a holder and a grandchild behind.
	let observed = (|| -> Result<(), String> {
		if !harness::appeared(&ids, Duration::from_secs(10)) {
			return Err(format!("{which}: the holder never published its ids"));
		}
		let published = harness::wrote(&ids)?;
		let numbers: Vec<u32> = published
			.split_whitespace()
			.map(|n| n.parse().map_err(|_| format!("bad ids: {published:?}")))
			.collect::<Result<_, _>>()?;
		if numbers.len() != 2 {
			return Err(format!("bad ids: {published:?}"));
		}
		let child = opened(numbers[0], "the holder")?;
		let descendant = opened(numbers[1], "the grandchild")?;
		if !still_running(&child) || !still_running(&descendant) {
			return Err(format!(
				"{which}: the holder or its grandchild had already ended, so no pipe was held"
			));
		}
		if normal_exit {
			std::fs::write(&release, "exit 7").map_err(|e| e.to_string())?;
		}

		let until = Instant::now() + PROMPTLY;
		while harness::wrote(&out_path)?.is_empty() {
			if Instant::now() >= until {
				return Err(format!(
					"{which}: the call did not return in {PROMPTLY:?}, so it waited for a held pipe"
				));
			}
			std::thread::sleep(Duration::from_millis(10));
		}
		let took = started.elapsed();
		// The capture says which of rnx's pipes the grandchild had: its
		// markers arrive on exactly those and nowhere else. Without this the
		// four cases would differ only in what the fixture believes.
		let expected = format!(
			"code={} timed_out={} cancelled=false out={} err={}",
			if normal_exit { 7 } else { 1 },
			!normal_exit,
			if keeps_stdout { 3 } else { 0 },
			if keeps_stderr { 4 } else { 0 }
		);
		if harness::wrote(&out_path)?.trim() != expected {
			return Err(format!(
				"{which}: the call reported {:?} rather than {expected:?}",
				harness::wrote(&out_path)?.trim()
			));
		}
		if !ended(&child) {
			return Err(format!(
				"{which}: the holder was still alive when the call reported"
			));
		}
		if !ended(&descendant) {
			return Err(format!(
				"{which}: the grandchild was still alive when the call reported"
			));
		}
		if normal_exit {
			let mut code = 0;
			// SAFETY: owned, signalled process handle with query access.
			if unsafe { GetExitCodeProcess(child.as_raw_handle(), &mut code) } == FALSE || code != 7
			{
				return Err(format!(
					"{which}: the direct child did not choose exit 7 (code={code})"
				));
			}
		}
		if rnx.try_wait().map_err(|e| e.to_string())?.is_some() {
			return Err(format!(
				"{which}: rnx exited before the ends could be observed"
			));
		}
		if took >= PROMPTLY {
			return Err(format!("{which}: the call took {took:?}"));
		}
		Ok(())
	})();

	let mut trouble: Vec<String> = observed.err().into_iter().collect();
	if normal_exit && let Err(error) = std::fs::write(&release, "exit 7") {
		trouble.push(error.to_string());
	}
	drop(rnx.stdin.take());
	let status = harness::reaped_within(&mut rnx, Duration::from_secs(5));
	if status.is_none() {
		trouble.push(format!(
			"{which}: rnx failed to exit after its stdin was closed"
		));
		if let Some(why) = harness::killed_within(&mut rnx, Duration::from_secs(5)) {
			trouble.push(why);
		}
	} else if status.flatten() != Some(0) {
		let said = harness::wrote(&err_path).unwrap_or_default();
		trouble.push(format!(
			"{which}: rnx exited {:?}: {said}",
			status.flatten()
		));
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

#[test]
fn a_held_reply_pipe_no_longer_holds_the_call_windows() {
	// The four rows of record 0023's table, with an in-job holder in place of
	// an escaped one. `neither` is the control: the grandchild keeps none of
	// rnx's pipes, so what is left holding them is the direct child, and the
	// deadline still has to be what ends the call.
	for (which, out, err) in [
		("neither", false, false),
		("stdout", true, false),
		("stderr", false, true),
		("both", true, true),
	] {
		a_held_pipe_does_not_hold_the_call(which, out, err, false);
	}
}

#[test]
fn a_normal_exit_ends_the_in_job_pipe_holder_during_cleanup() {
	a_held_pipe_does_not_hold_the_call("normal exit", true, true, true);
}

#[test]
fn a_descendant_that_never_stops_writing_does_not_hold_the_call_windows() {
	// The case no reasoning about a pipeful covers: bytes keep arriving, so a
	// reader that decided when to stop by asking whether more is coming would
	// never stop. What bounds it is reading the clock before every read, and
	// that is what this asks after — with an in-job grandchild, because the
	// Unix question is stronger than Windows can be asked: a writer that
	// survives the group's termination and goes on producing bytes is what
	// decision 5 and gate 10 rule out. The job ending the writer is not
	// evidence about a writer that outlived it, and this claims none.
	let dir = scratch();
	let path = dir.join("writing.rn");
	let ids = dir.join("ids");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	std::fs::write(
		&path,
		format!(
			"pub fn main(_) {{ let r = host::process({}, {DEADLINE_MS})?; \
			 println!(\"timed_out={{}} cancelled={{}} cut_short={{}} unreadable={{}} out={{}}\", r.timed_out, r.cancelled, r.cut_short, r.unreadable, r.stdout.len()); \
			 host::stdin()?; Ok(()) }}",
			holder(
				&ids,
				true,
				true,
				None,
				// Infinite: `for /l` with a step of nothing never reaches its
				// end, so the only thing that stops this grandchild is rnx.
				"for /l %i in (1,0,2) do @echo xxxxxxxxxxxxxxxx"
			)
		),
	)
	.unwrap();
	let started = Instant::now();
	let mut rnx = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.stdin(Stdio::piped())
		.stdout(File::create(&out_path).unwrap())
		.stderr(File::create(&err_path).unwrap())
		.spawn()
		.expect("rnx did not start");

	let observed = (|| -> Result<(), String> {
		if !harness::appeared(&ids, Duration::from_secs(10)) {
			return Err("the holder never published its ids".into());
		}
		let published = harness::wrote(&ids)?;
		let numbers: Vec<u32> = published
			.split_whitespace()
			.map(|n| n.parse().map_err(|_| format!("bad ids: {published:?}")))
			.collect::<Result<_, _>>()?;
		if numbers.len() != 2 {
			return Err(format!("bad ids: {published:?}"));
		}
		let child = opened(numbers[0], "the holder")?;
		let descendant = opened(numbers[1], "the grandchild")?;
		if !still_running(&child) || !still_running(&descendant) {
			return Err("the writer had already ended, so nothing was writing".into());
		}

		let until = Instant::now() + PROMPTLY;
		while harness::wrote(&out_path)?.is_empty() {
			if Instant::now() >= until {
				return Err(format!(
					"the call did not return in {PROMPTLY:?}, so a writer that never stops held it"
				));
			}
			std::thread::sleep(Duration::from_millis(10));
		}
		let took = started.elapsed();
		let said = harness::wrote(&out_path)?;
		let said = said.trim();
		for flag in ["timed_out=true", "cancelled=false", "unreadable=false"] {
			if !said.contains(flag) {
				return Err(format!("the call reported {said:?}, wanted {flag}"));
			}
		}
		// **The opposite of the Unix answer, and not a weakening.** There the
		// writer survives the group's termination and goes on producing, so
		// the reader gives up on a stream that never ends and the capture is
		// cut short. Here the writer is in the job: the deadline ends it, its
		// write end closes, and the reader drains what is left and sees the
		// end of the file inside the allowance. `cut_short=true` would mean
		// rnx stopped before a stream that had already finished.
		//
		// Measured, so the difference is asserted rather than left unsaid.
		if !said.contains("cut_short=false") {
			return Err(format!(
				"the capture was cut short, so the reader stopped before an ended stream: {said:?}"
			));
		}
		// And bytes really were arriving: more than a pipe holds, so the
		// reader went round its loop rather than taking one bufferful and
		// being stopped by the deadline before it asked again.
		// By field rather than by searching for `out=`, which `timed_out=`
		// also contains: the first draft parsed the word `true`.
		let captured: usize = said
			.split_whitespace()
			.find_map(|field| field.strip_prefix("out="))
			.and_then(|n| n.parse().ok())
			.ok_or_else(|| format!("no byte count in {said:?}"))?;
		if captured <= 65_536 {
			return Err(format!(
				"only {captured} bytes were captured, which a single pipeful covers"
			));
		}
		if !ended(&child) || !ended(&descendant) {
			return Err("the writer was still alive when the call reported".into());
		}
		if rnx.try_wait().map_err(|e| e.to_string())?.is_some() {
			return Err("rnx exited before the ends could be observed".into());
		}
		if took >= PROMPTLY {
			return Err(format!("the call took {took:?}"));
		}
		Ok(())
	})();

	let mut trouble: Vec<String> = observed.err().into_iter().collect();
	drop(rnx.stdin.take());
	let status = harness::reaped_within(&mut rnx, Duration::from_secs(5));
	if status.is_none() {
		trouble.push("rnx failed to exit after its stdin was closed".into());
		if let Some(why) = harness::killed_within(&mut rnx, Duration::from_secs(5)) {
			trouble.push(why);
		}
	} else if status.flatten() != Some(0) {
		let said = harness::wrote(&err_path).unwrap_or_default();
		trouble.push(format!("rnx exited {:?}: {said}", status.flatten()));
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}
