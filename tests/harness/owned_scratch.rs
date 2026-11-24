//! A scratch directory owned by the test that made it: removed when the test
//! is done with it, so runs leave nothing behind in the shared temporary
//! directory.
#![allow(dead_code)]
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// A path inside a scratch directory that this guard created, which is
/// removed on drop. It dereferences to the path, so `scratch.to_str()` and
/// `&scratch` work where a `&Path` is expected.
pub struct Scratch {
	dir: PathBuf,
	path: PathBuf,
}

impl Scratch {
	/// Claim a per-test directory directly under the active temporary root.
	pub fn temp(label: &str) -> Self {
		Self::dir(std::env::temp_dir().join(format!("{label}-{}", std::process::id())))
	}

	/// Claim a directory and name a fixture file inside it.
	pub fn temp_file(label: &str, file: &str) -> Self {
		Self::file(
			std::env::temp_dir().join(format!("{label}-{}", std::process::id())),
			file,
		)
	}

	/// A new, empty directory owned by this guard; the path is the directory
	/// itself. The directory is created atomically. If `base` already exists
	/// (another process, or a reused process id), a fresh sibling is created
	/// instead, so an existing directory is never touched or removed.
	pub fn dir(base: PathBuf) -> Self {
		let mut candidate = base.clone();
		for n in 1..=1000u32 {
			match std::fs::create_dir(&candidate) {
				Ok(()) => {
					return Self {
						path: candidate.clone(),
						dir: candidate,
					};
				}
				Err(e) if e.kind() == ErrorKind::AlreadyExists => {
					let mut name = base.file_name().unwrap().to_os_string();
					name.push(format!("-{n}"));
					candidate = base.with_file_name(name);
				}
				Err(e) => panic!(
					"cannot create scratch directory {}: {e}",
					candidate.display()
				),
			}
		}
		panic!("no free scratch directory beside {}", base.display());
	}

	/// A new directory owned by this guard, with the path naming `name`
	/// inside it.
	pub fn file(base: PathBuf, name: &str) -> Self {
		let mut scratch = Self::dir(base);
		scratch.path = scratch.dir.join(name);
		scratch
	}
}

impl std::ops::Deref for Scratch {
	type Target = Path;
	fn deref(&self) -> &Path {
		&self.path
	}
}

impl AsRef<Path> for Scratch {
	fn as_ref(&self) -> &Path {
		&self.path
	}
}

impl AsRef<std::ffi::OsStr> for Scratch {
	fn as_ref(&self) -> &std::ffi::OsStr {
		self.path.as_os_str()
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.dir);
	}
}

impl std::fmt::Debug for Scratch {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Debug::fmt(&self.path, f)
	}
}
