//! A single manifest replacement; no lock, receipt, build or cache publication.
use super::{Project, commands, err, fault};
use crate::{catalogue, input};
use std::{
	fs::{self, File, Metadata, OpenOptions},
	io::{Read, Write},
	path::{Path, PathBuf},
};

#[derive(PartialEq)]
struct Identity {
	len: u64,
	modified: std::time::SystemTime,
	#[cfg(unix)]
	device: u64,
	#[cfg(unix)]
	inode: u64,
	#[cfg(unix)]
	mode: u32,
}
fn identity(m: &Metadata) -> Result<Identity, String> {
	#[cfg(unix)]
	use std::os::unix::fs::MetadataExt;
	Ok(Identity {
		len: m.len(),
		modified: m.modified().map_err(err)?,
		#[cfg(unix)]
		device: m.dev(),
		#[cfg(unix)]
		inode: m.ino(),
		#[cfg(unix)]
		mode: m.mode(),
	})
}
struct Snapshot {
	bytes: Vec<u8>,
	identity: Identity,
	metadata: Metadata,
}
impl Snapshot {
	fn read(path: &Path) -> Result<Self, String> {
		let read = || {
			let mut options = OpenOptions::new();
			options.read(true);
			#[cfg(unix)]
			{
				use std::os::unix::fs::OpenOptionsExt;
				options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
			}
			let mut file = options.open(path).map_err(err)?;
			let metadata = file.metadata().map_err(err)?;
			if !metadata.is_file() {
				return Err("not a regular manifest".into());
			}
			let before = identity(&metadata)?;
			let mut bytes = Vec::new();
			(&mut file)
				.take(input::MANIFEST_LIMIT as u64 + 1)
				.read_to_end(&mut bytes)
				.map_err(err)?;
			if bytes.len() > input::MANIFEST_LIMIT {
				return Err("manifest exceeds 1048576 bytes".into());
			}
			if before != identity(&file.metadata().map_err(err)?)? {
				return Err("manifest changed while reading".into());
			}
			Ok(Self {
				bytes,
				identity: before,
				metadata,
			})
		};
		read().map_err(|e: String| format!("manifest {}: {e}", path.display()))
	}
	fn recheck(&self, path: &Path) -> Result<(), String> {
		let now = Self::read(path)?;
		if now.identity != self.identity || now.bytes != self.bytes {
			return Err(format!(
				"manifest {} changed before publication; retry add",
				path.display()
			));
		}
		Ok(())
	}
}
struct Temporary {
	path: PathBuf,
	file: File,
	unpublished: bool,
}
impl Drop for Temporary {
	fn drop(&mut self) {
		if self.unpublished {
			let _ = fs::remove_file(&self.path);
		}
	}
}
impl Temporary {
	fn create(path: PathBuf) -> Result<Self, String> {
		let mut options = OpenOptions::new();
		options.write(true).create_new(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options
				.mode(0o600)
				.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
		}
		let file = options.open(&path).map_err(|e| {
			format!(
				"cannot create private add temporary {}: {e}",
				path.display()
			)
		})?;
		Ok(Self {
			path,
			file,
			unpublished: true,
		})
	}
	fn recheck(&self) -> Result<(), String> {
		let path = fs::symlink_metadata(&self.path).map_err(err)?;
		if !path.is_file() || identity(&path)? != identity(&self.file.metadata().map_err(err)?)? {
			return Err("add temporary was replaced".into());
		}
		Ok(())
	}
}
fn shell_word(path: &Path) -> Result<String, String> {
	let s = path.to_str().ok_or("manifest path is not Unicode")?;
	Ok(format!("'{}'", s.replace('\'', "'\\''")))
}
impl Project {
	pub(super) fn add(&self, entries: &[catalogue::Entry]) -> Result<(), String> {
		let original = Snapshot::read(&self.manifest)?;
		let candidate = catalogue::author(original.bytes.clone(), &self.base, entries)
			.map_err(|e| format!("manifest {}: {e}", self.manifest.display()))?;
		let word = shell_word(&self.manifest)?;
		if !candidate.added.is_empty() {
			commands::check()?;
			let mut temp = Temporary::create(self.dot.join("add-manifest.new"))?;
			temp.file.write_all(&candidate.bytes).map_err(err)?;
			#[cfg(unix)]
			{
				use std::os::unix::fs::{MetadataExt, PermissionsExt};
				temp.file
					.set_permissions(fs::Permissions::from_mode(original.metadata.mode() & 0o777))
					.map_err(err)?;
			}
			#[cfg(not(unix))]
			temp.file
				.set_permissions(original.metadata.permissions())
				.map_err(err)?;
			temp.file.sync_all().map_err(err)?;
			fault("after-add-temp")?;
			fault("before-add-rename")?;
			original.recheck(&self.manifest)?;
			temp.recheck()?;
			commands::check()?;
			fs::rename(&temp.path, &self.manifest).map_err(err)?;
			temp.unpublished = false;
			let finish = || {
				fault("after-add-rename")?;
				fault("add-directory-sync")?;
				File::open(&self.base)
					.and_then(|f| f.sync_all())
					.map_err(err)?;
				commands::check()
			};
			finish().map_err(|e| {
				format!(
					"manifest {} was replaced; durability not confirmed: {e}",
					self.manifest.display()
				)
			})?;
		} else {
			original.recheck(&self.manifest)?;
			commands::check()?;
		}
		for name in candidate.added {
			println!("added {name}");
		}
		for name in candidate.existing {
			println!("already present: {name}");
		}
		println!(
			"Next:\n  rnx-project lock --manifest {word}\n  rnx-project build --manifest {word}\n  rnx-project session --manifest {word}"
		);
		Ok(())
	}
}
