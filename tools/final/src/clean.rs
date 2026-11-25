//! Limits from the first complete final that included its build and cleanup.
use crate::receipt::digest;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const LIMITS: &str = "plans/impl/0338-final-limits.json";
pub const ADOPTION_BASE: &str = "b8d5ad340653623cc5e1855bc4d2c85ce00ccb8f";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
	pub receipt_sha256: String,
	pub seconds: u64,
	/// Filesystem roles are stable across reboots, device numbers are not.
	pub bytes: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cleanup {
	pub candidate: String,
	pub baseline: String,
	pub started: f64,
	pub finished: f64,
	pub directory: String,
	pub removed: bool,
	pub targets_cleaned: u64,
	pub peak_bytes: BTreeMap<String, u64>,
}

impl Cleanup {
	pub fn elapsed(&self) -> Result<f64> {
		ensure!(
			self.removed && !self.directory.is_empty(),
			"missing directory release"
		);
		let seconds = self.finished - self.started;
		ensure!(
			self.started >= 0. && seconds.is_finite() && seconds > 0.,
			"invalid clean final clock"
		);
		ensure!(!self.peak_bytes.is_empty(), "no filesystem measurements");
		Ok(seconds)
	}

	pub(crate) fn limits(&self, receipt: &Path) -> Result<Limits> {
		let seconds = (1.25 * self.elapsed()? / 60.).ceil() as u64;
		let bytes = self
			.peak_bytes
			.iter()
			.map(|(role, &peak)| {
				let size = peak
					.checked_add(peak.div_ceil(4))
					.context("disk limit overflow")?;
				Ok((role.clone(), size))
			})
			.collect::<Result<_>>()?;
		Ok(Limits {
			receipt_sha256: digest(receipt)?,
			seconds: seconds.checked_mul(60).context("time limit overflow")?,
			bytes,
		})
	}
}

pub fn check_successor(root: &Path, prior: &Path, changed: &[String], tested: &str) -> Result<()> {
	let result = prior.join("RESULT.json");
	let cleanup = prior.join("CLEANUP.json");
	let changes_limit = changed.iter().any(|path| path == LIMITS);
	if !cleanup.try_exists()? {
		ensure!(!changes_limit, "limits addition needs a clean final");
		return Ok(());
	}
	let record: Cleanup = serde_json::from_reader(std::fs::File::open(cleanup)?)?;
	if record.baseline != ADOPTION_BASE {
		ensure!(
			!changes_limit,
			"docs successor changed the published limits"
		);
		return Ok(());
	}
	ensure!(
		changes_limit && record.candidate == tested,
		"calibration successor requires its own limits"
	);
	let actual: Limits = serde_json::from_reader(std::fs::File::open(root.join(LIMITS))?)?;
	ensure!(
		actual == record.limits(&result)?,
		"limits differ from the calibration receipt"
	);
	Ok(())
}

pub fn release(root: &Path, out: &Path, run: &serde_json::Value) -> Result<Cleanup> {
	use std::os::unix::process::CommandExt;
	crate::guardian::own_unit()?;
	let mut command = std::process::Command::new("/bin/bash");
	command
		.arg(root.join("tools/final/clean/release.sh"))
		.arg(run["directory"].as_str().context("no run directory")?)
		.arg(out)
		.arg("terminal");
	// SAFETY: only the dedicated release child inherits the verified area lease.
	unsafe {
		command.pre_exec(|| {
			if libc::fcntl(7, libc::F_SETFD, 0) == -1 {
				return Err(std::io::Error::last_os_error());
			}
			Ok(())
		});
	}
	let done = command.output()?;
	ensure!(
		done.status.success(),
		"cleanup failed: {}",
		String::from_utf8_lossy(&done.stderr)
	);
	Ok(serde_json::from_reader(std::fs::File::open(
		out.join("CLEANUP.json"),
	)?)?)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn calibration_uses_the_post_cleanup_clock_and_rounds_up_each_filesystem() {
		let record = Cleanup {
			candidate: "tested".into(),
			baseline: ADOPTION_BASE.into(),
			started: 20.,
			finished: 80.1,
			directory: "/owned".into(),
			removed: true,
			targets_cleaned: 2,
			peak_bytes: BTreeMap::from([("build".into(), 101), ("memory".into(), 4)]),
		};
		let result = Path::new(file!());
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
		let limits = record.limits(&root.join(result)).unwrap();
		assert_eq!(limits.seconds, 120);
		assert_eq!(limits.bytes["build"], 127);
		assert_eq!(limits.bytes["memory"], 5);
	}
}
