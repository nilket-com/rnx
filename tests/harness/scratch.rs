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

#[test]
fn a_colliding_scratch_directory_is_never_touched() {
	// The control for ownership: a directory already at the scratch name,
	// planted with a sentinel, survives both the acquisition and the drop.
	// The plant lives inside an outer directory this test creates atomically
	// itself (not through the helper under test), so nothing outside it is
	// ever removed.
	let mut outer = None;
	for n in 0..1000u32 {
		let candidate =
			std::env::temp_dir().join(format!("rnx-scratch-collision-{}-{n}", std::process::id()));
		match std::fs::create_dir(&candidate) {
			Ok(()) => {
				outer = Some(candidate);
				break;
			}
			Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
			Err(e) => panic!("cannot create {}: {e}", candidate.display()),
		}
	}
	let outer = outer.expect("no free collision directory");
	let base = outer.join("scratch");
	std::fs::create_dir(&base).unwrap();
	std::fs::write(base.join("sentinel"), b"kept").unwrap();
	let owned = {
		let scratch = Scratch::dir(base.clone());
		assert_ne!(&*scratch, base.as_path());
		assert!(scratch.exists());
		scratch.to_path_buf()
	};
	assert!(!owned.exists(), "the guard's own directory was not removed");
	assert_eq!(std::fs::read(base.join("sentinel")).unwrap(), b"kept");
	std::fs::remove_dir_all(&outer).unwrap();
}
