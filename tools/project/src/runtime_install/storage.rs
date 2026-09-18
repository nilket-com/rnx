use super::hooks;
use crate::{cache_storage, commands};
use std::{
	fs::{self, File, OpenOptions},
	io::{Read, Write},
	os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
	path::{Path, PathBuf},
	time::Duration,
};
pub(super) fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
}
pub(super) fn exists(p: &Path) -> Result<bool, String> {
	match fs::symlink_metadata(p) {
		Ok(_) => Ok(true),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
		Err(e) => Err(format!("{}: {e}", p.display())),
	}
}
pub(super) fn absolute(p: PathBuf) -> Result<PathBuf, String> {
	if !p.is_absolute() || p.to_str().is_none() {
		Err("runtime store selection requires a nonempty absolute Unicode path".into())
	} else {
		Ok(p)
	}
}
pub(super) fn future_canonical(p: &Path) -> Result<PathBuf, String> {
	match p.canonicalize() {
		Ok(p) => Ok(p),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			if exists(p)? {
				return Err(format!("broken path: {}", p.display()));
			}
			let parent = p.parent().ok_or("path has no parent")?;
			Ok(future_canonical(parent)?.join(p.file_name().ok_or("path has no final name")?))
		}
		Err(e) => Err(format!("{}: {e}", p.display())),
	}
}
fn envpath(
	key: &str,
	fallback: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<PathBuf, String> {
	match std::env::var_os(key) {
		Some(p) => absolute(p.into()),
		None => fallback(),
	}
}
fn home() -> Result<PathBuf, String> {
	absolute(std::env::var_os("HOME").ok_or("HOME is missing")?.into())
}
pub(super) fn selected() -> Result<PathBuf, String> {
	let p = envpath("XDG_DATA_HOME", || Ok(home()?.join(".local/share")))?.join("rnx/runtimes");
	let p = future_canonical(&p)?;
	let cache = envpath("RNX_PROJECT_CACHE", || {
		Ok(envpath("XDG_CACHE_HOME", || Ok(home()?.join(".cache")))?.join("rnx/assemblies"))
	})?;
	let scratch =
		envpath("XDG_STATE_HOME", || Ok(home()?.join(".local/state")))?.join("rnx/sessions");
	for other in [cache, scratch] {
		let other = future_canonical(&other)?;
		if p.starts_with(&other) || other.starts_with(&p) {
			return Err(
				"runtime store and cache/scratch storage must not contain one another".into(),
			);
		}
	}
	for ancestor in p.ancestors() {
		for marker in ["Cargo.toml", "rnx.toml"] {
			if exists(&ancestor.join(marker))? {
				return Err(format!(
					"runtime store is inside a native/project root: {}",
					ancestor.display()
				));
			}
		}
	}
	Ok(p)
}
pub(super) fn create_root(p: &Path) -> Result<(), String> {
	if !exists(p)? {
		fs::DirBuilder::new()
			.recursive(true)
			.mode(0o700)
			.create(p)
			.map_err(err)?;
	}
	dir(p)
}
pub(super) fn dir(p: &Path) -> Result<(), String> {
	cache_storage::private_directory(p)
}
pub(super) fn mkdir(p: &Path) -> Result<(), String> {
	cache_storage::directory(p)
}
pub(super) fn options() -> OpenOptions {
	let mut o = OpenOptions::new();
	o.mode(0o600)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
	o
}
pub(super) fn regular(m: &fs::Metadata, p: &Path) -> Result<(), String> {
	if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o022 != 0 {
		return Err(format!("not a private owned regular file: {}", p.display()));
	}
	Ok(())
}
pub(super) fn read(p: &Path, limit: usize) -> Result<Vec<u8>, String> {
	let f = options()
		.read(true)
		.open(p)
		.map_err(|e| format!("{}: {e}", p.display()))?;
	regular(&f.metadata().map_err(err)?, p)?;
	let mut b = vec![];
	f.take(limit as u64 + 1).read_to_end(&mut b).map_err(err)?;
	if b.len() > limit {
		return Err(format!("{} exceeds {limit} bytes", p.display()));
	}
	Ok(b)
}
pub(super) fn write(p: &Path, bytes: &[u8]) -> Result<(), String> {
	let mut f = options()
		.write(true)
		.create_new(true)
		.open(p)
		.map_err(err)?;
	f.write_all(bytes).map_err(err)?;
	f.sync_all().map_err(err)
}
pub(super) fn sync(p: &Path) -> Result<(), String> {
	dir(p)?;
	File::open(p).and_then(|f| f.sync_all()).map_err(err)
}
pub(super) fn lock(root: &Path) -> Result<File, String> {
	let p = root.join("install.lock");
	let f = options()
		.read(true)
		.write(true)
		.create(true)
		.open(&p)
		.map_err(err)?;
	regular(&f.metadata().map_err(err)?, &p)?;
	loop {
		commands::check()?;
		match f.try_lock() {
			Ok(()) => {
				hooks::point("locked")?;
				return Ok(f);
			}
			Err(fs::TryLockError::WouldBlock) => {
				hooks::point("waiting")?;
				std::thread::sleep(Duration::from_millis(10));
			}
			Err(e) => return Err(err(e)),
		}
	}
}
// Applies to retained source AND generated administration. Directory entries are
// charged too, so traversal itself cannot outgrow the file bound.
pub(super) fn walk(root: &Path, sync_files: bool) -> Result<(u64, u64), String> {
	dir(root)?;
	let mut dirs = vec![root.to_owned()];
	let mut ordered = vec![];
	let mut count = 0u64;
	let mut bytes = 0u64;
	while let Some(d) = dirs.pop() {
		commands::check()?;
		ordered.push(d.clone());
		for e in fs::read_dir(&d).map_err(err)? {
			let e = e.map_err(err)?;
			let p = e.path();
			if p.to_str().is_none() {
				return Err("non-Unicode managed name".into());
			}
			count += 1;
			if count > hooks::limit("RNX_INSTALL_RETAINED_FILES", 400_000) {
				return Err("installed tree exceeds 400000 entries".into());
			}
			let m = fs::symlink_metadata(&p).map_err(err)?;
			if m.is_dir() {
				dir(&p)?;
				dirs.push(p);
			} else {
				regular(&m, &p)?;
				bytes = bytes
					.checked_add(m.len())
					.ok_or("installed byte overflow")?;
				if bytes > hooks::limit("RNX_INSTALL_RETAINED_BYTES", 2 * 1024 * 1024 * 1024) {
					return Err("installed tree exceeds 2 GiB".into());
				}
				if sync_files {
					let f = options().read(true).open(&p).map_err(err)?;
					regular(&f.metadata().map_err(err)?, &p)?;
					f.sync_all().map_err(err)?;
				}
			}
		}
	}
	if sync_files {
		for d in ordered.into_iter().rev() {
			sync(&d)?;
		}
	}
	Ok((count, bytes))
}
pub(super) fn reclaim(root: &Path) -> Result<(), String> {
	let mut n = 0;
	for e in fs::read_dir(root).map_err(err)? {
		n += 1;
		if n > 400_000 {
			return Err("too many runtime store entries".into());
		}
		let e = e.map_err(err)?;
		let name = e.file_name();
		let Some(name) = name.to_str() else {
			return Err("non-Unicode store name".into());
		};
		if name.starts_with(".stage-") {
			walk(&e.path(), false)?;
			fs::remove_dir_all(e.path()).map_err(err)?;
		} else if name.starts_with(".current-") {
			read(&e.path(), 65536)?;
			fs::remove_file(e.path()).map_err(err)?;
		}
	}
	Ok(())
}
pub(super) fn unique(root: &Path, prefix: &str) -> Result<PathBuf, String> {
	for i in 0..1000 {
		let p = root.join(format!(".{prefix}-{}-{i}", std::process::id()));
		if !exists(&p)? {
			return Ok(p);
		}
	}
	Err("runtime temporary names exhausted".into())
}
