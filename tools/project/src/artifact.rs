//! Private validated artifact capability and versioned receipt.
#![allow(dead_code)] // Also built by the isolated assembly probe/library tests.
use crate::fingerprint;
use serde::{Deserialize, Serialize};
use std::{
	fs,
	path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stamp {
	bytes: u64,
	mtime_seconds: i64,
	mtime_nanoseconds: u32,
	executable: bool,
	#[cfg(unix)]
	device: u64,
	#[cfg(unix)]
	inode: u64,
}
impl Stamp {
	fn from_metadata(m: &fs::Metadata) -> Result<Self, String> {
		if !m.is_file() || m.len() > fingerprint::BYTES {
			return Err("artifact must be a bounded regular file".into());
		}
		#[cfg(unix)]
		{
			use std::os::unix::fs::MetadataExt;
			let ns = u32::try_from(m.mtime_nsec()).map_err(|_| "invalid artifact mtime")?;
			if ns >= 1_000_000_000 {
				return Err("invalid artifact mtime".into());
			}
			Ok(Self {
				bytes: m.len(),
				mtime_seconds: m.mtime(),
				mtime_nanoseconds: ns,
				executable: m.mode() & 0o111 != 0,
				device: m.dev(),
				inode: m.ino(),
			})
		}
		#[cfg(not(unix))]
		{
			use std::time::UNIX_EPOCH;
			let time = m.modified().map_err(|e| e.to_string())?;
			let (sec, ns) = match time.duration_since(UNIX_EPOCH) {
				Ok(d) => (
					i64::try_from(d.as_secs()).map_err(|_| "mtime out of range")?,
					d.subsec_nanos(),
				),
				Err(e) => {
					let d = e.duration();
					let s = i64::try_from(d.as_secs()).map_err(|_| "mtime out of range")?;
					if d.subsec_nanos() == 0 {
						(-s, 0)
					} else {
						(
							s.checked_neg()
								.and_then(|v| v.checked_sub(1))
								.ok_or("mtime out of range")?,
							1_000_000_000 - d.subsec_nanos(),
						)
					}
				}
			};
			Ok(Self {
				bytes: m.len(),
				mtime_seconds: sec,
				mtime_nanoseconds: ns,
				executable: false,
			})
		}
	}
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
	pub format: u32,
	pub lock_sha256: String,
	pub executable_sha256: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stamp: Option<Stamp>,
}
pub(crate) fn digest_valid(s: &str) -> bool {
	s.len() == 64
		&& s.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Receipt {
	pub fn decode(bytes: &[u8]) -> Result<Self, String> {
		let r: Self = serde_json::from_slice(bytes).map_err(|e| format!("invalid receipt: {e}"))?;
		if !digest_valid(&r.lock_sha256) || !digest_valid(&r.executable_sha256) {
			return Err("invalid receipt digest".into());
		}
		match (r.format, &r.stamp) {
			(1, None) => (),
			(2, Some(s))
				if s.bytes <= fingerprint::BYTES && s.mtime_nanoseconds < 1_000_000_000 => {}
			_ => return Err("invalid receipt version or stamp".into()),
		}
		Ok(r)
	}
}
pub(crate) struct Checked {
	path: PathBuf,
	stamp: Stamp,
}
impl Checked {
	pub fn path(&self) -> &Path {
		&self.path
	}
	pub fn recheck(&self) -> Result<(), String> {
		if inspect(&self.path)? != self.stamp {
			return Err("artifact changed before receipt publication".into());
		}
		Ok(())
	}
	pub fn stamp(&self) -> Stamp {
		self.stamp.clone()
	}
}
fn inspect(path: &Path) -> Result<Stamp, String> {
	let name = path
		.file_name()
		.and_then(|s| s.to_str())
		.ok_or("artifact has no Unicode filename")?;
	if name.contains('\\') {
		return Err("artifact filename contains backslash".into());
	}
	let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
	if !meta.is_file() {
		return Err("artifact is not a regular file".into());
	}
	let mut o = fs::OpenOptions::new();
	o.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
	}
	let f = o.open(path).map_err(|e| e.to_string())?;
	Stamp::from_metadata(&f.metadata().map_err(|e| e.to_string())?)
}
/// A matching stamp permits no payload read; all other paths retain the known digest.
pub(crate) fn check(
	path: &Path,
	expected: &str,
	previous: Option<&Stamp>,
	force: bool,
) -> Result<Checked, String> {
	let work = || -> Result<Checked, String> {
		if !digest_valid(expected) {
			return Err("invalid executable digest".into());
		}
		let initial = inspect(path)?;
		if !force && previous == Some(&initial) {
			return Ok(Checked {
				path: path.to_owned(),
				stamp: initial,
			});
		}
		let (file, before, after) =
			fingerprint::one_observed(path, &mut fingerprint::Allowance::default())?;
		if file.sha256 != expected {
			return Err("hash mismatch; run build (or relock an intended override)".into());
		}
		let before = Stamp::from_metadata(&before)?;
		let after = Stamp::from_metadata(&after)?;
		if initial != before || before != after || after != inspect(path)? {
			return Err("changed during artifact verification".into());
		}
		#[cfg(feature = "test-support")]
		pause("artifact-hashed")?;
		// A pause hook tests the hash-to-stamp boundary. No fresh metadata is trusted.
		if after != inspect(path)? {
			return Err("changed after artifact verification".into());
		}
		Ok(Checked {
			path: path.to_owned(),
			stamp: after,
		})
	};
	work().map_err(|e| format!("executable {}: {e}", path.display()))
}
#[cfg(feature = "test-support")]
pub(crate) fn observed_read(path: &Path, n: usize) {
	if std::env::var_os("RNX_PROJECT_COUNT_ARTIFACT").as_deref() == Some(path.as_os_str())
		&& let Some(out) = std::env::var_os("RNX_PROJECT_READ_LOG")
	{
		use std::io::Write;
		if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(out) {
			let _ = writeln!(f, "{n}");
		}
	}
}
#[cfg(feature = "test-support")]
fn pause(name: &str) -> Result<(), String> {
	if std::env::var("RNX_PROJECT_PAUSE").ok().as_deref() == Some(name) {
		let p =
			PathBuf::from(std::env::var_os("RNX_PROJECT_PAUSE_FILE").ok_or("pause file missing")?);
		fs::write(&p, name).map_err(|e| e.to_string())?;
		while p.exists() {
			std::thread::sleep(std::time::Duration::from_millis(10));
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{
		sync::atomic::{AtomicUsize, Ordering},
		time::{Duration, UNIX_EPOCH},
	};
	struct Temp(PathBuf);
	impl Temp {
		fn new() -> Self {
			static N: AtomicUsize = AtomicUsize::new(0);
			let p = std::env::temp_dir().join(format!(
				"rnx-stamp-{}-{}",
				std::process::id(),
				N.fetch_add(1, Ordering::Relaxed)
			));
			fs::create_dir(&p).unwrap();
			Self(p)
		}
	}
	impl Drop for Temp {
		fn drop(&mut self) {
			fs::remove_dir_all(&self.0).unwrap();
		}
	}
	fn set_time(p: &Path, t: std::time::SystemTime) {
		fs::File::options()
			.write(true)
			.open(p)
			.unwrap()
			.set_times(fs::FileTimes::new().set_modified(t))
			.unwrap();
	}
	#[test]
	fn metadata_default_and_explicit_hash_have_the_documented_difference() {
		let t = Temp::new();
		let p = t.0.join("inert");
		fs::write(&p, b"first").unwrap();
		let digest = crate::assembly::executable_hash(&p).unwrap();
		let original = check(&p, &digest, None, false).unwrap();
		let stamp = original.stamp();
		let time = fs::metadata(&p).unwrap().modified().unwrap();
		assert_eq!(
			check(&p, &digest, Some(&stamp), false).unwrap().stamp(),
			stamp
		);
		fs::write(&p, b"other").unwrap();
		set_time(&p, time);
		assert_eq!(inspect(&p).unwrap(), stamp);
		// Inert bytes are checked, never executed. This is the intentional miss.
		assert!(check(&p, &digest, Some(&stamp), false).is_ok());
		assert!(check(&p, &digest, Some(&stamp), true).is_err());
		set_time(&p, time + Duration::from_secs(1));
		assert!(check(&p, &digest, Some(&stamp), false).is_err());
		fs::write(&p, b"first").unwrap();
		set_time(&p, time + Duration::from_secs(2));
		let fresh = check(&p, &digest, Some(&stamp), false).unwrap();
		assert_ne!(fresh.stamp(), stamp);
		fs::write(&p, b"longer").unwrap();
		assert!(fresh.recheck().is_err());
		assert!(check(&p, &digest, Some(&fresh.stamp()), false).is_err());
	}
	#[cfg(unix)]
	#[test]
	fn replacement_mode_and_pre_epoch_time_are_observed() {
		use std::os::unix::fs::{PermissionsExt, symlink};
		let t = Temp::new();
		let p = t.0.join("inert");
		fs::write(&p, b"first").unwrap();
		set_time(&p, UNIX_EPOCH - Duration::from_nanos(1));
		let digest = crate::assembly::executable_hash(&p).unwrap();
		let old = check(&p, &digest, None, true).unwrap().stamp();
		assert_eq!(
			(old.mtime_seconds, old.mtime_nanoseconds),
			(-1, 999_999_999)
		);
		let held = fs::File::open(&p).unwrap();
		let replacement = t.0.join("new");
		fs::write(&replacement, b"other").unwrap();
		set_time(&replacement, UNIX_EPOCH - Duration::from_nanos(1));
		fs::rename(&replacement, &p).unwrap();
		assert_ne!(old.inode, inspect(&p).unwrap().inode);
		assert!(check(&p, &digest, Some(&old), false).is_err());
		drop(held);
		fs::write(&replacement, b"first").unwrap();
		fs::rename(&replacement, &p).unwrap();
		let fresh = check(&p, &digest, Some(&old), false).unwrap().stamp();
		assert_ne!(old, fresh);
		fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
		assert!(
			check(&p, &digest, Some(&fresh), false)
				.unwrap()
				.stamp()
				.executable
		);
		let link = t.0.join("link");
		symlink(&p, &link).unwrap();
		assert!(check(&link, &digest, None, true).is_err());
		fs::File::options()
			.write(true)
			.open(&p)
			.unwrap()
			.set_len(fingerprint::BYTES + 1)
			.unwrap();
		assert!(check(&p, &digest, None, true).is_err());
	}
	#[test]
	fn receipt_versions_and_fields_are_validated() {
		let t = Temp::new();
		let p = t.0.join("inert");
		fs::write(&p, b"data").unwrap();
		let digest = crate::assembly::executable_hash(&p).unwrap();
		let stamp = check(&p, &digest, None, true).unwrap().stamp();
		let legacy =
			serde_json::json!({"format":1,"lock_sha256":digest,"executable_sha256":digest});
		assert!(Receipt::decode(&serde_json::to_vec(&legacy).unwrap()).is_ok());
		let current = serde_json::json!({"format":2,"lock_sha256":digest,"executable_sha256":digest,"stamp":stamp});
		assert!(Receipt::decode(&serde_json::to_vec(&current).unwrap()).is_ok());
		for (key, value) in [
			("format", serde_json::json!(99)),
			("stamp", serde_json::Value::Null),
			("lock_sha256", serde_json::json!("bad")),
			("unexpected", serde_json::json!(true)),
		] {
			let mut bad = current.clone();
			bad[key] = value;
			assert!(Receipt::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
		}
		let mut bad = current.clone();
		bad["stamp"]["mtime_nanoseconds"] = serde_json::json!(1_000_000_000u32);
		assert!(Receipt::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
	}
}
