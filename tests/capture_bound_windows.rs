//! Record 0023's capture contracts, asked with children that stay in the job.
//!
//! `tests/capture_bound.rs` asks these with a descendant that leaves the
//! process group and holds a pipe open past the call. Windows has no such
//! descendant — record 0025 decision 5 — and everything inside the job is dead
//! before the readers are reached, so that fixture cannot be ported.
//!
//! The **contracts** are not what is unavailable, only that mechanism. Decision
//! 1 says the contracts are the specification, so they are asked here by other
//! means, and the hardest of them — `truncated` and `cut_short` together — is
//! what this file exists for. Three earlier attempts failed and looked like
//! evidence that the pair was unreachable:
//!
//! - an in-job descendant holding the pipe: killed with the job before the
//!   readers are reached, so the stream is always at its end by then;
//! - the startup delay: holding a reader *before* it reads means the child
//!   fills the pipe and blocks instead of exiting, so the cap is never
//!   reached;
//! - a large child with a long allowance: the same blocking, more slowly.
//!
//! What works is a hold **after** the cap and before the reader's next look,
//! with a child small enough to finish writing. That is
//! `RNX_TEST_READER_HOLDS_AT_CAP`.
//!
//! **The size is a measured recipe, not a guarantee.** Cap plus sixteen
//! kibibytes fits this machine's pipe buffer, so the child can write its
//! remainder and exit while the reader is held. Nothing documents that
//! capacity as portable, so when the child cannot finish, the gate says
//! exactly that rather than reporting a flag mismatch.

#![cfg(all(windows, feature = "test-support"))]

mod harness;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// What `host::process` caps a single stream at.
const CAP: usize = 2 * 1024 * 1024;

/// How much more than the cap the child writes. Small enough to sit in the
/// pipe buffer so the child can exit while the reader is held at the cap.
const OVER: usize = 16 * 1024;

const LIMIT: Duration = Duration::from_secs(60);
const HANDSHAKE: Duration = Duration::from_secs(20);

fn scratch(what: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-capwin-{}-{what}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// Wait, bounded, for a file to appear. `false` means the bound is what ended
/// the wait, which every caller treats as a failure.
fn appeared(path: &Path, within: Duration) -> bool {
	let until = Instant::now() + within;
	while Instant::now() < until {
		if path.exists() {
			return true;
		}
		std::thread::sleep(Duration::from_millis(5));
	}
	false
}

/// `truncated` and `cut_short` are reported together, and each for its own
/// reason.
///
/// The ordering, every step waited for rather than timed:
///
/// 1. the reader consumes to the cap, sets `truncated`, and holds — it says so
///    by making its `.at-cap` file, so this waits for the reader to be there;
/// 2. the child writes its small remainder into the pipe buffer and exits;
/// 3. the call publishes its allowance, ends the job, and reaches the
///    readers — which it announces, and which is a **later** fact than the
///    cleanup merely beginning;
/// 4. only then is the reader released, and its next look finds the stop.
#[test]
fn a_stream_past_the_cap_is_also_cut_short_when_the_call_ends() {
	let dir = scratch("both");
	let mut trouble = Vec::new();

	let bulk = dir.join("bulk.txt");
	std::fs::write(&bulk, "x".repeat(CAP + OVER)).expect("cannot write the bulk file");
	let batch = dir.join("emit.bat");
	std::fs::write(
		&batch,
		format!("@echo off\r\ntype \"{}\"\r\n", bulk.display()),
	)
	.expect("cannot write the batch file");

	let script = "pub fn main(args) { let r = host::process(\"cmd\", [\"/c\", args[0]], 30000)?; \
	              println!(\"code={:?} timed_out={} cancelled={} truncated={} cut_short={} unreadable={} out={}\", \
	              r.code, r.timed_out, r.cancelled, r.truncated, r.cut_short, r.unreadable, r.stdout.len()); Ok(()) }";
	let path = dir.join("capture.rn");
	std::fs::write(&path, script).expect("cannot write the script");

	let hold = dir.join("release");
	let at_cap = dir.join("release.stdout.at-cap");
	let stopped = dir.join("stop-published");
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let out = std::fs::File::create(&out_path).expect("cannot make a stdout file");
	let err = std::fs::File::create(&err_path).expect("cannot make a stderr file");

	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.arg(&batch)
		.env("RNX_TEST_READER_HOLDS_AT_CAP", &hold)
		.env("RNX_TEST_SIGNAL_STOP_TO", &stopped)
		// Long enough for the reader to reach the cap after the child exits.
		// Zero cannot work: the readers are then reached the moment the child
		// is gone, and the child is gone as soon as its last bytes are in the
		// pipe buffer rather than read -- so the reader is cut short short of
		// the cap. Measured at zero: out=2067456, thirty kibibytes shy.
		.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", "3000")
		.env_remove("RNX_TEST_READER_STARTS_LATE_MS")
		.stdin(Stdio::null())
		.stdout(Stdio::from(out))
		.stderr(Stdio::from(err))
		.spawn()
		.expect("rnx did not start");

	// 1. The reader is at the cap, so `truncated` is already set.
	let reached_cap = appeared(&at_cap, HANDSHAKE);
	if !reached_cap {
		trouble.push("the reader never reached the cap".to_owned());
	}

	// 2 and 3. The child finishes and the stop is published. If this never
	// happens the child could not write its remainder — the recipe's one
	// assumption — and that is what is reported, rather than a flag mismatch
	// further down that would send a reader looking in the wrong place.
	let stop_published = reached_cap && appeared(&stopped, HANDSHAKE);
	if reached_cap && !stop_published {
		trouble.push(format!(
			"the stop was never published while the reader waited at the cap: the child could not finish writing its {OVER} bytes past the cap, so this machine's pipe buffer is smaller than the recipe assumes"
		));
	}

	// 4. Released only now.
	std::fs::write(&hold, "go").expect("cannot release the reader");

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

	// Reported rather than defaulted to empty: the stderr check below looks
	// for a handshake that timed out, and a stderr nobody could read would
	// find no complaint and let the flags stand for nothing.
	let said = harness::wrote(&out_path).map_err(|e| trouble.push(e)).ok();
	let stderr = harness::wrote(&err_path).map_err(|e| trouble.push(e)).ok();
	if let (Some(status), Some(said)) = (status, said.as_deref()) {
		if status.code() != Some(0) {
			trouble.push(format!("rnx exited {:?}", status.code()));
		}
		// Exact, and including the child's own clean exit: a child that was
		// deadlined or cancelled would reach the same two flags for reasons
		// this gate is not asking about.
		let want = format!(
			"code=0 timed_out=false cancelled=false truncated=true cut_short=true unreadable=false out={CAP}"
		);
		if said.trim() != want {
			trouble.push(format!("expected {want:?}, got {:?}", said.trim()));
		}
	}
	// A handshake that ended on its bound would have released a reader the
	// call had not yet reached, and the flags would then mean nothing.
	for complaint in ["waited at the cap and was never released", "never was"] {
		if let Some(stderr) = stderr.as_deref()
			&& stderr.contains(complaint)
		{
			trouble.push(format!("a handshake timed out: {:?}", stderr.trim()));
		}
	}

	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// The child exits only after stdout has captured "partial". Release that
/// reader only after cleanup publishes its stop, before it can observe EOF.
#[test]
fn a_completeness_check_can_see_a_prefix_that_truncation_cannot_windows() {
	let dir = scratch("prefix");
	let hold = dir.join("release");
	let reached = dir.join("release.after-prefix");
	let stopped = dir.join("stop-published");
	let emitter = r#"param([string]$reached)
$ErrorActionPreference = 'Stop'
$out = [Console]::OpenStandardOutput()
$bytes = [Text.Encoding]::UTF8.GetBytes('partial')
$out.Write($bytes, 0, $bytes.Length)
$out.Flush()
$bound = [DateTime]::UtcNow.AddSeconds(15)
while (!(Test-Path -LiteralPath $reached)) {
    if ([DateTime]::UtcNow -ge $bound) { throw 'reader never captured the prefix' }
    Start-Sleep -Milliseconds 5
}
exit 0
"#;
	let emitter = format!(
		"& {{ {emitter} }} '{}'",
		reached.to_string_lossy().replace('\'', "''")
	);
	let script = dir.join("prefix.rn");
	std::fs::write(&script, r#"pub fn main(args) {
    let r = host::process(args[0], ["-NoProfile", "-NonInteractive", "-Command", args[1]], 30000)?;
    println!("code={:?} timed_out={} cancelled={} truncated={} cut_short={} unreadable={} out={} err={}",
        r.code, r.timed_out, r.cancelled, r.truncated, r.cut_short, r.unreadable, r.stdout, r.stderr);
    Ok(())
}"#).unwrap();
	let out_path = dir.join("stdout");
	let err_path = dir.join("stderr");
	let powershell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
		.join("System32/WindowsPowerShell/v1.0/powershell.exe");
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&script)
		.arg(powershell)
		.arg(&emitter)
		.env("RNX_TEST_READER_PREFIX_BYTES", "7")
		.env("RNX_TEST_READER_HOLDS_AFTER_PREFIX", &hold)
		.env("RNX_TEST_SIGNAL_STOP_TO", &stopped)
		.env("RNX_TEST_CLEANUP_ALLOWANCE_MS", "0")
		.stdin(Stdio::null())
		.stdout(std::fs::File::create(&out_path).unwrap())
		.stderr(std::fs::File::create(&err_path).unwrap())
		.spawn()
		.expect("rnx did not start");
	let mut trouble = Vec::new();
	let at_prefix = appeared(&reached, HANDSHAKE);
	if !at_prefix {
		trouble.push("the reader never acknowledged the prefix".to_owned());
	} else if !matches!(std::fs::read_to_string(&reached).as_deref(), Ok("7")) {
		trouble.push("the reader did not acknowledge exactly seven captured bytes".to_owned());
	}
	if at_prefix && !appeared(&stopped, HANDSHAKE) {
		trouble.push("cleanup never published the reader stop".to_owned());
	}
	// Always release, including on failed handshakes, before bounded teardown.
	if let Err(e) = std::fs::write(&hold, "go") {
		trouble.push(format!("cannot release the reader: {e}"));
	}
	let status = harness::reaped_within(&mut child, Duration::from_secs(8));
	if status != Some(Some(0)) {
		trouble.push(format!("rnx did not return successfully: {status:?}"));
	}
	if status.is_none()
		&& let Some(problem) = harness::killed_within(&mut child, Duration::from_secs(5))
	{
		trouble.push(problem);
	}
	match harness::wrote(&out_path) {
		Ok(said)
			if said.trim()
				== "code=0 timed_out=false cancelled=false truncated=false cut_short=true unreadable=false out=partial err=" =>
			{}
		Ok(said) => trouble.push(format!("unexpected capture: {said:?}")),
		Err(e) => trouble.push(e),
	}
	match harness::wrote(&err_path) {
		Ok(said) if said.is_empty() => {}
		Ok(said) => trouble.push(format!(
			"rnx diagnostic (including any hook timeout): {said:?}"
		)),
		Err(e) => trouble.push(e),
	}
	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}
