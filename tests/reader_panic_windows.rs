//! Collect the held stderr worker before propagating a stdout reader panic.
#![cfg(all(windows, feature = "test-support"))]

mod harness;

use std::fs::File;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{FALSE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
	GetProcessIdOfThread, OpenThread, THREAD_QUERY_INFORMATION, THREAD_SYNCHRONIZE,
	WaitForSingleObject,
};

#[test]
fn a_reader_that_panics_does_not_strand_the_other_windows() {
	let dir = harness::scratch("reader-panic-windows");
	let script = dir.join("panic.rn");
	let out = dir.join("stdout");
	let err = dir.join("stderr");
	let stopped = dir.join("readers-stopped");
	std::fs::write(
		&script,
		r#"pub fn main(_) {
    match host::process("cmd", ["/c", "exit 0"], 30000) {
        Ok(_) => println!("unexpected success"),
        Err(error) => println!("error={}", error),
    }
    host::stdin()?;
    Ok(())
}"#,
	)
	.unwrap();
	let mut rnx = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&script)
		.env("RNX_TEST_READER_COLLECTION_CONTROL", &dir)
		.env("RNX_TEST_READER_PANICS", "stdout")
		.env("RNX_TEST_SIGNAL_STOP_TO", &stopped)
		.stdin(Stdio::piped())
		.stdout(File::create(&out).unwrap())
		.stderr(File::create(&err).unwrap())
		.spawn()
		.expect("rnx did not start");
	let observed = (|| -> Result<(), String> {
		if !harness::appeared(&dir.join("stderr-id"), Duration::from_secs(5)) {
			return Err("stderr never reached its hold".into());
		}
		let id = harness::wrote(&dir.join("stderr-id"))?
			.parse::<u32>()
			.map_err(|e| e.to_string())?;
		// SAFETY: the worker publishes its own ID while held. Validate the
		// open and its owner, then retain the handle through the observation.
		let raw = unsafe { OpenThread(THREAD_QUERY_INFORMATION | THREAD_SYNCHRONIZE, FALSE, id) };
		if raw.is_null() {
			return Err(format!(
				"cannot open stderr worker: {}",
				std::io::Error::last_os_error()
			));
		}
		let worker = unsafe { OwnedHandle::from_raw_handle(raw) };
		if unsafe { GetProcessIdOfThread(worker.as_raw_handle()) } != rnx.id() {
			return Err("stderr worker belongs to a different process".into());
		}
		if unsafe { WaitForSingleObject(worker.as_raw_handle(), 0) } != WAIT_TIMEOUT {
			return Err("stderr worker ended before the panic was released".into());
		}
		std::fs::write(dir.join("release-stdout"), "panic now").map_err(|e| e.to_string())?;
		if !harness::appeared(&stopped, Duration::from_secs(5)) {
			return Err("cleanup never published the reader stops".into());
		}
		let until = Instant::now() + Duration::from_secs(5);
		while !harness::wrote(&err)?.contains("the stdout reader was told to panic") {
			if Instant::now() >= until {
				return Err("stdout never reached its injected panic".into());
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		// An early `?` on stdout's join reports the error during this hold.
		let until = Instant::now() + Duration::from_millis(300);
		while Instant::now() < until {
			if !harness::wrote(&out)?.is_empty() {
				return Err("the call reported before collecting the held stderr worker".into());
			}
			if unsafe { WaitForSingleObject(worker.as_raw_handle(), 0) } != WAIT_TIMEOUT {
				return Err("stderr did not remain alive during its hold".into());
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		std::fs::write(dir.join("release-stderr"), "finish").map_err(|e| e.to_string())?;
		let until = Instant::now() + Duration::from_secs(3);
		while !harness::wrote(&out)?.contains('\n') {
			if Instant::now() >= until {
				return Err("the call did not report after stderr was released".into());
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		if unsafe { WaitForSingleObject(worker.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
			return Err("stderr was still alive when the call reported".into());
		}
		if rnx.try_wait().map_err(|e| e.to_string())?.is_some() {
			return Err("rnx exited before collection could be observed".into());
		}
		Ok(())
	})();
	let mut trouble: Vec<String> = observed.err().into_iter().collect();
	for stream in ["stdout", "stderr"] {
		if let Err(e) = std::fs::write(dir.join(format!("release-{stream}")), "finish") {
			trouble.push(e.to_string());
		}
	}
	drop(rnx.stdin.take());
	let status = harness::reaped_within(&mut rnx, Duration::from_secs(5));
	if status != Some(Some(0)) {
		trouble.push(format!("unexpected rnx exit: {status:?}"));
	}
	if status.is_none()
		&& let Some(e) = harness::killed_within(&mut rnx, Duration::from_secs(5))
	{
		trouble.push(e);
	}
	match (harness::wrote(&out), harness::wrote(&err)) {
		(Ok(out), Ok(err)) => {
			if out.trim() != "error=stdout reader panicked" {
				trouble.push(format!("wrong call result: {out:?}"));
			}
			if !err.contains("the stdout reader was told to panic") {
				trouble.push(format!("missing injected panic: {err:?}"));
			}
			if err.contains("rnx test-support:") {
				trouble.push(format!("hook failed: {err:?}"));
			}
		}
		(result_out, result_err) => trouble.push(format!(
			"cannot read results: {result_out:?} {result_err:?}"
		)),
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}
