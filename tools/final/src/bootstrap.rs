//! The entry's handoff: the controller trusts its build only after checking
//! the single-use record the entry wrote before and after compiling it.
use crate::child::{head, value_error};
use crate::receipt::digest;
use anyhow::{Context as _, Result};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

/// Seconds since boot, suspend included: the clock /proc/uptime reports.
pub fn boottime() -> f64 {
	let mut now: libc::timespec = unsafe { std::mem::zeroed() };
	// SAFETY: an owned timespec.
	unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) };
	now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}

fn sha256_of_command(program: &str, args: &[&str]) -> Result<String> {
	let output = std::process::Command::new(program).args(args).output()?;
	anyhow::ensure!(output.status.success(), "{program} {args:?} failed");
	Ok(crate::pyjson::hex(&<sha2::Sha256 as sha2::Digest>::digest(
		&output.stdout,
	)))
}

/// Read, check and consume the handoff; the instant the final began.
pub fn accept(record_path: &Path, root: &Path) -> Result<(Value, Instant)> {
	let bytes = std::fs::read(record_path)
		.map_err(|_| value_error("bootstrap handoff missing or already used"))?;
	// Single use: gone before anything else is believed.
	std::fs::remove_file(record_path)?;
	let record: Value =
		serde_json::from_slice(&bytes).map_err(|_| value_error("bootstrap handoff is not JSON"))?;
	if record["schema"] != 1 {
		return Err(value_error("unknown bootstrap handoff"));
	}
	if record["fixture"] != false {
		return Err(value_error(
			"a bootstrap admitted from fixture sources cannot start a final",
		));
	}
	let (started, built, now) = (
		record["started"].as_f64(),
		record["built"].as_f64(),
		boottime(),
	);
	let (Some(started), Some(built)) = (started, built) else {
		return Err(value_error("bootstrap times missing"));
	};
	if !started.is_finite() || !built.is_finite() || started > built || built > now {
		return Err(value_error(
			"bootstrap times are out of order or in the future",
		));
	}
	if let Some(run) = record.get("clean_run") {
		use std::os::unix::fs::MetadataExt;
		let state = std::env::var_os("RNX_FINAL_AREA").context("missing build area")?;
		let work = Path::new(&state).join("run");
		anyhow::ensure!(
			run["directory"].as_str().map(Path::new) == Some(&work)
				&& work.join("source") == root
				&& root.canonicalize()? == root,
			"wrong build directory"
		);
		let named = std::env::var("RNX_FINAL_BASELINE")?;
		anyhow::ensure!(
			run["baseline"] == named
				&& named.len() == 40
				&& named.bytes().all(|c| c.is_ascii_hexdigit()),
			"wrong published baseline"
		);
		if named == crate::clean::ADOPTION_BASE {
			let parent = crate::child::git_command()
				.current_dir(root)
				.args(["rev-parse", "HEAD^"])
				.output()?;
			anyhow::ensure!(
				parent.status.success()
					&& parent.stdout == b"d18a457ae86fd53f03986925489ef8dcb147ea5d\n"
					&& run["limits"].is_null(),
				"uncalibrated candidate"
			);
		} else {
			let data = crate::child::git_command()
				.current_dir(root)
				.args(["show", &format!("{named}:{}", crate::clean::LIMITS)])
				.output()?;
			anyhow::ensure!(
				data.status.success()
					&& data.stdout == std::fs::read(root.join(crate::clean::LIMITS))?,
				"published limits missing or changed"
			);
			let limits: crate::clean::Limits = serde_json::from_slice(&data.stdout)?;
			anyhow::ensure!(
				limits.seconds > 0 && serde_json::to_value(limits)? == run["limits"],
				"wrong calibration limits"
			);
		}
		let directory = std::fs::File::open(&state)?;
		let held = std::fs::metadata("/proc/self/fd/7")?;
		let expected = directory.metadata()?;
		anyhow::ensure!(
			held.is_dir() && (held.dev(), held.ino()) == (expected.dev(), expected.ino()),
			"lost run lease"
		);
		match directory.try_lock() {
			Err(std::fs::TryLockError::WouldBlock) => {}
			_ => anyhow::bail!("run area is not exclusively leased"),
		}
		// SAFETY: flock neither dereferences a pointer nor takes ownership.
		// The independent open above was refused. This succeeds only if the
		// inherited open file description is itself the holder.
		anyhow::ensure!(
			unsafe { libc::flock(7, libc::LOCK_EX | libc::LOCK_NB) } == 0,
			"another process owns the run lease"
		);
		// SAFETY: descriptor 7 is the verified lease above. The controller
		// owns it until exit; no subprocess may prolong that ownership.
		anyhow::ensure!(
			unsafe { libc::fcntl(7, libc::F_SETFD, libc::FD_CLOEXEC) } == 0,
			"cannot confine run lease to controller"
		);
	}
	if record["artifact_sha256"].as_str() != Some(digest(Path::new("/proc/self/exe"))?.as_str()) {
		return Err(value_error("running controller is not the bootstrap build"));
	}
	if head(root).as_deref() != record["candidate"].as_str() {
		return Err(value_error("candidate changed after the bootstrap build"));
	}
	let status = crate::child::git_command()
		.args(["status", "--porcelain", "-z"])
		.current_dir(root)
		.output()?;
	if !status.status.success() {
		return Err(value_error(&format!(
			"git status failed: {}",
			String::from_utf8_lossy(&status.stderr).trim()
		)));
	}
	if !status.stdout.is_empty() || record["clean"] != true {
		return Err(value_error("the checkout is not clean"));
	}
	if record["cargo_lock_sha256"].as_str() != Some(digest(&root.join("Cargo.lock"))?.as_str()) {
		return Err(value_error("Cargo.lock changed after the bootstrap build"));
	}
	if record["toolchain_sha256"].as_str()
		!= Some(sha256_of_command("rustc", &["+1.98.1", "-vV"])?.as_str())
	{
		return Err(value_error("toolchain changed after the bootstrap build"));
	}
	let elapsed = Duration::from_secs_f64((now - started).max(0.0));
	let began = Instant::now()
		.checked_sub(elapsed)
		.context("bootstrap start precedes this clock")?;
	Ok((record, began))
}
