//! Installed Linux stock-file bundle. The private generation is immutable.
//! The helper and engine are both opened and verified before exec.
use std::{
	env,
	ffi::{CString, OsStr},
	fs::{self, DirBuilder, File, OpenOptions},
	io::{self, Read, Write},
	os::{
		fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
		unix::{
			ffi::OsStrExt,
			fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
			process::CommandExt,
		},
	},
	path::{Path, PathBuf},
	process::Command,
	sync::atomic::{AtomicU64, Ordering},
};

const IMAGE_FD: RawFd = 9;
const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libengine_image.so"));
const HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stock-file"));
const GENERATION: &str = concat!(
	include_str!(concat!(env!("OUT_DIR"), "/libengine_image.so.sha256")),
	"-",
	include_str!(concat!(env!("OUT_DIR"), "/stock-file.sha256"))
);

const STAMP_PREFIX: &[u8] = b"rnx-file-v1/x86_64-linux-gnu/";
const STAMP_LEN: usize = STAMP_PREFIX.len() + GENERATION.len() + 10 * 8;

fn invalid(message: &'static str) -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, message)
}

fn private_dir(path: &Path) -> io::Result<File> {
	let dir = OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
		.open(path)?;
	let meta = dir.metadata()?;
	// SAFETY: geteuid has no arguments or preconditions.
	if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
		return Err(invalid("unsafe installed engine cache directory"));
	}
	Ok(dir)
}

fn fd_path(fd: RawFd) -> PathBuf {
	PathBuf::from(format!("/proc/self/fd/{fd}"))
}

enum CacheError {
	Unavailable,
	Failed(io::Error),
}

impl From<io::Error> for CacheError {
	fn from(error: io::Error) -> Self {
		Self::Failed(error)
	}
}

// Classify storage errors only at the operations which create or write
// our cache. Opening and verifying an existing member always fails closed.
fn storage(error: io::Error) -> CacheError {
	match error.raw_os_error() {
		Some(
			libc::EACCES | libc::EPERM | libc::EROFS | libc::ENOSPC | libc::EDQUOT | libc::ENOENT,
		) => CacheError::Unavailable,
		_ => CacheError::Failed(error),
	}
}

fn cache() -> Result<File, CacheError> {
	let base = env::var_os("XDG_CACHE_HOME")
		.map(PathBuf::from)
		.filter(|p| p.is_absolute())
		.or_else(|| {
			env::home_dir()
				.filter(|p| p.is_absolute())
				.map(|p| p.join(".cache"))
		})
		.ok_or(CacheError::Unavailable)?;
	fs::create_dir_all(&base).map_err(storage)?;
	let root = base.join("rnx-file-images");
	match DirBuilder::new().mode(0o700).create(&root) {
		Ok(()) => (),
		Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
		Err(e) => return Err(storage(e)),
	}
	Ok(private_dir(&root)?)
}

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temporary(PathBuf);
impl Drop for Temporary {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

fn temporary_name(name: &str, prefix: &str) -> bool {
	name.strip_prefix(prefix)
		.and_then(|suffix| suffix.split_once('-'))
		.is_some_and(|(pid, seq)| {
			[pid, seq]
				.into_iter()
				.all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
		})
}

fn generation_name(name: &str) -> bool {
	name.split_once('-').is_some_and(|(image, helper)| {
		[image, helper].into_iter().all(|s| {
			s.len() == 64
				&& s.bytes()
					.all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
		})
	})
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
	match result {
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
		other => other,
	}
}

// Publishers serialize on the cache root. Readers hold a shared flock on
// their generation through the startup helper's final named-member check.
fn prune(cache: &File) -> io::Result<()> {
	for entry in fs::read_dir(fd_path(cache.as_raw_fd()))? {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else { continue };
		if name == GENERATION {
			continue;
		}
		if temporary_name(name, ".verifying-") {
			ignore_missing(fs::remove_file(entry.path()))?;
			continue;
		}
		if !generation_name(name) && !temporary_name(name, ".creating-") {
			continue;
		}
		let dir = match private_dir(&entry.path()) {
			Ok(dir) => dir,
			Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
			Err(e) => return Err(e),
		};
		dir.lock()?;
		for file in fs::read_dir(fd_path(dir.as_raw_fd()))? {
			let file = file?;
			if file.file_name().to_str().is_some_and(|n| {
				matches!(n, "engine" | "helper" | "verified") || temporary_name(n, ".verifying-")
			}) {
				ignore_missing(fs::remove_file(file.path()))?;
			}
		}
		ignore_missing(fs::remove_dir(entry.path()))?;
	}
	Ok(())
}

fn open_generation(path: &Path) -> io::Result<File> {
	let dir = private_dir(path)?;
	dir.lock_shared()?;
	// A publisher could retire the directory between open and flock.
	if dir.metadata()?.nlink() == 0 {
		return Err(io::Error::from(io::ErrorKind::NotFound));
	}
	Ok(dir)
}

fn generation(cache: &File) -> Result<File, CacheError> {
	let base = fd_path(cache.as_raw_fd());
	let path = base.join(GENERATION);
	match open_generation(&path) {
		Ok(dir) => return Ok(dir),
		Err(e) if e.kind() == io::ErrorKind::NotFound => (),
		Err(e) => return Err(e.into()),
	}
	cache.lock()?;
	match open_generation(&path) {
		Ok(dir) => return Ok(dir),
		Err(e) if e.kind() == io::ErrorKind::NotFound => (),
		Err(e) => return Err(e.into()),
	}

	let tmp = loop {
		let tmp = base.join(format!(
			".creating-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		match DirBuilder::new().mode(0o700).create(&tmp) {
			Ok(()) => break Temporary(tmp),
			Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
			Err(e) => return Err(storage(e)),
		}
	};
	for (name, bytes, mode) in [("engine", IMAGE, 0o400), ("helper", HELPER, 0o500)] {
		let mut file = OpenOptions::new()
			.write(true)
			.create_new(true)
			.mode(mode)
			.open(tmp.0.join(name))
			.map_err(storage)?;
		file.write_all(bytes).map_err(storage)?;
		file.sync_all().map_err(storage)?;
	}
	File::open(&tmp.0)?.sync_all().map_err(storage)?;
	let name = |p: &OsStr| CString::new(p.as_bytes()).map_err(io::Error::other);
	let from = name(tmp.0.file_name().unwrap())?;
	let to = name(OsStr::new(GENERATION))?;
	// SAFETY: both C names and the pinned parent descriptor live across the syscall.
	if unsafe {
		libc::renameat2(
			cache.as_raw_fd(),
			from.as_ptr(),
			cache.as_raw_fd(),
			to.as_ptr(),
			libc::RENAME_NOREPLACE,
		)
	} < 0
	{
		let e = io::Error::last_os_error();
		if e.kind() != io::ErrorKind::AlreadyExists {
			return Err(storage(e));
		}
	} else {
		cache.sync_all().map_err(storage)?;
		prune(cache)?;
	}
	Ok(open_generation(&path)?)
}

#[derive(PartialEq, Eq)]
struct Identity {
	dev: u64,
	ino: u64,
	len: u64,
	ctime: i64,
	nsec: i64,
}

impl From<&fs::Metadata> for Identity {
	fn from(meta: &fs::Metadata) -> Self {
		Self {
			dev: meta.dev(),
			ino: meta.ino(),
			len: meta.len(),
			ctime: meta.ctime(),
			nsec: meta.ctime_nsec(),
		}
	}
}

fn member(path: &Path, size: usize) -> io::Result<(File, Identity)> {
	let file = OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
		.open(path)?;
	let meta = file.metadata()?;
	// SAFETY: geteuid has no arguments or preconditions.
	if !meta.is_file()
		|| meta.uid() != unsafe { libc::geteuid() }
		|| meta.mode() & 0o222 != 0
		|| meta.len() != size as u64
	{
		return Err(invalid("invalid installed file bundle member"));
	}
	Ok((file, Identity::from(&meta)))
}

fn verify_bytes(file: &mut File, bytes: &[u8]) -> io::Result<()> {
	let mut buffer = [0; 32768];
	for chunk in bytes.chunks(buffer.len()) {
		file.read_exact(&mut buffer[..chunk.len()])?;
		if &buffer[..chunk.len()] != chunk {
			return Err(invalid("installed file bundle content mismatch"));
		}
	}
	Ok(())
}

fn stamp_matches(stamp: &Path, expected: &[u8; STAMP_LEN]) -> io::Result<bool> {
	let mut file = match OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
		.open(stamp)
	{
		Ok(file) => file,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
		Err(e) => return Err(e),
	};
	let meta = file.metadata()?;
	// SAFETY: geteuid has no arguments or preconditions.
	if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o777 != 0o400 {
		return Err(invalid("unsafe installed engine verification stamp"));
	}
	if meta.len() != expected.len() as u64 {
		return Ok(false);
	}
	let mut content = [0; STAMP_LEN];
	file.read_exact(&mut content)?;
	Ok(&content == expected)
}

fn publish_stamp(directory: &File, path: &Path, expected: &[u8]) -> io::Result<()> {
	let (mut file, tmp) = loop {
		let tmp = path.with_file_name(format!(
			".verifying-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		match OpenOptions::new()
			.create_new(true)
			.write(true)
			.mode(0o400)
			.open(&tmp)
		{
			Ok(file) => break (file, tmp),
			Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
			Err(e) => return Err(e),
		}
	};
	let result = (|| {
		file.write_all(expected)?;
		file.sync_all()?;
		fs::rename(&tmp, path)?;
		directory.sync_all()
	})();
	if result.is_err() {
		let _ = fs::remove_file(tmp);
	}
	result
}

fn verified_pair(directory: &File) -> Result<(File, File), CacheError> {
	let path = fd_path(directory.as_raw_fd());
	let engine = path.join("engine");
	let helper = path.join("helper");
	let (mut image, image_id) = member(&engine, IMAGE.len())?;
	let (mut executable, exe_id) = member(&helper, HELPER.len())?;
	// The stamp lives in the owned, non-writable-by-others generation.
	// The two embedded content hashes and both ctimes bind a full comparison
	// to these open files. Missing or stale stamps force that comparison.
	// A fixed-width encoding also keeps the verification work independent
	// of the decimal length of an inode or timestamp.
	let mut expected = [0; STAMP_LEN];
	let (prefix, tail) = expected.split_at_mut(STAMP_PREFIX.len());
	prefix.copy_from_slice(STAMP_PREFIX);
	let (generation, ids) = tail.split_at_mut(GENERATION.len());
	generation.copy_from_slice(GENERATION.as_bytes());
	for (bytes, id) in ids
		.as_chunks_mut::<40>()
		.0
		.iter_mut()
		.zip([&image_id, &exe_id])
	{
		for (chunk, n) in bytes.as_chunks_mut::<8>().0.iter_mut().zip([
			id.dev,
			id.ino,
			id.len,
			id.ctime as u64,
			id.nsec as u64,
		]) {
			chunk.copy_from_slice(&n.to_le_bytes());
		}
	}
	let stamp = path.join("verified");
	let hit = stamp_matches(&stamp, &expected)?;
	if !hit {
		verify_bytes(&mut image, IMAGE)?;
		verify_bytes(&mut executable, HELPER)?;
	}
	if image_id != Identity::from(&fs::symlink_metadata(engine)?)
		|| exe_id != Identity::from(&fs::symlink_metadata(helper)?)
	{
		return Err(invalid("installed file bundle changed during verification").into());
	}
	if !hit {
		publish_stamp(directory, &stamp, &expected).map_err(storage)?;
	}
	Ok((image, executable))
}

fn sealed_member(name: &std::ffi::CStr, bytes: &[u8]) -> io::Result<File> {
	let flags = libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING;
	// SAFETY: name is a live NUL-terminated string. Older kernels do not
	// recognize MFD_EXEC and use executable memfds by default.
	let mut fd = unsafe { libc::memfd_create(name.as_ptr(), flags | libc::MFD_EXEC) };
	if fd < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
		fd = unsafe { libc::memfd_create(name.as_ptr(), flags) };
	}
	if fd < 0 {
		return Err(io::Error::last_os_error());
	}
	let mut file = unsafe { File::from_raw_fd(fd) };
	file.write_all(bytes)?;
	let seals = libc::F_SEAL_WRITE | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL;
	if unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, seals) } < 0 {
		return Err(io::Error::last_os_error());
	}
	// exec requires the last writable description of the ELF to close.
	File::open(fd_path(fd))
}

fn cached_bundle() -> Result<(Option<File>, File, File), CacheError> {
	let cache = cache()?;
	let dir = generation(&cache)?;
	let (image, helper) = verified_pair(&dir)?;
	Ok((Some(dir), image, helper))
}

fn duplicate_above_image(fd: RawFd) -> io::Result<OwnedFd> {
	// SAFETY: fcntl creates a new descriptor owned by this result.
	let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, IMAGE_FD + 1) };
	if copy < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(unsafe { OwnedFd::from_raw_fd(copy) })
}

pub(super) fn launch(explicit_legacy: bool) -> io::Result<()> {
	if explicit_legacy {
		if let Some(value) = env::var_os("RNX_FILE_ROUTE_FD") {
			let fd = value
				.to_str()
				.and_then(|s| s.parse::<RawFd>().ok())
				.filter(|&n| n >= 3)
				.ok_or_else(|| invalid("invalid file route descriptor"))?;
			// SAFETY: duplicate the caller's descriptor; only the duplicate is owned.
			let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
			if copy < 0 {
				return Err(io::Error::last_os_error());
			}
			unsafe { File::from_raw_fd(copy) }.write_all(b"{\"route\":\"legacy_explicit\",\"category\":null,\"reason\":null,\"location\":null}\n")?;
		}
		return Ok(());
	}
	// Read the caller's slot before opening any files which might occupy it.
	// SAFETY: F_GETFD only inspects an fd and returns EBADF when absent.
	let flags = unsafe { libc::fcntl(IMAGE_FD, libc::F_GETFD) };
	if flags < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EBADF) {
		return Err(io::Error::last_os_error());
	}
	let original = if flags >= 0 {
		Some(duplicate_above_image(IMAGE_FD)?)
	} else {
		None
	};
	let (dir, image, helper) = match cached_bundle() {
		Ok(bundle) => bundle,
		Err(CacheError::Unavailable) => (
			None,
			sealed_member(c"rnx-engine", IMAGE)?,
			sealed_member(c"rnx-helper", HELPER)?,
		),
		Err(CacheError::Failed(error)) => return Err(error),
	};
	// Pin the ELF helper above the reserved slot, including if open assigned
	// the helper that very slot. A CLOEXEC descriptor can exec an ELF.
	let helper_fd = duplicate_above_image(helper.as_raw_fd())?;
	let directory = dir
		.as_ref()
		.map(|dir| duplicate_above_image(dir.as_raw_fd()))
		.transpose()?;
	let directory_fd = directory.as_ref().map_or(-1, AsRawFd::as_raw_fd);
	let restore = original
		.as_ref()
		.filter(|_| flags & libc::FD_CLOEXEC == 0)
		.map_or(-1, AsRawFd::as_raw_fd);
	let executable_fd = helper_fd.as_raw_fd();
	let mut command = Command::new(fd_path(executable_fd));
	command
		.arg(format!("{directory_fd},{executable_fd},{restore}"))
		.args(env::args_os().skip(1));
	// SAFETY: this stock CLI is about to exec before starting any service
	// threads. Only fcntl/dup2 run between descriptor setup and exec.
	let image_fd = image.as_raw_fd();
	unsafe {
		command.pre_exec(move || {
			if libc::dup2(image_fd, IMAGE_FD) < 0
				|| libc::fcntl(IMAGE_FD, libc::F_SETFD, 0) < 0
				|| (directory_fd >= 0 && libc::fcntl(directory_fd, libc::F_SETFD, 0) < 0)
				|| libc::fcntl(executable_fd, libc::F_SETFD, 0) < 0
				|| (restore >= 0 && libc::fcntl(restore, libc::F_SETFD, 0) < 0)
			{
				return Err(io::Error::last_os_error());
			}
			Ok(())
		});
	}
	let error = command.exec();
	// exec failed. The process must terminate before Rust closes a reused slot.
	eprintln!("rnx: could not execute installed file helper: {error}");
	std::process::exit(1)
}
