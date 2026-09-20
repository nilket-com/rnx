use super::{Args, hooks};
use crate::commands;
mod annotations;
mod documents;
use serde_json::{Value, json};
use std::{
	collections::HashSet,
	ffi::{CStr, CString, OsStr, OsString},
	fs::{File, OpenOptions},
	io::{Read, Write},
	os::{
		fd::{AsRawFd, FromRawFd},
		unix::{
			ffi::{OsStrExt, OsStringExt},
			fs::{MetadataExt, OpenOptionsExt},
		},
	},
	path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}
fn io_error(label: &str) -> String {
	format!("{label}: {}", std::io::Error::last_os_error())
}
fn name(s: &OsStr) -> Result<CString> {
	CString::new(s.as_bytes()).map_err(error)
}
#[repr(C)]
struct How {
	flags: u64,
	mode: u64,
	resolve: u64,
}
const RESOLVE: u64 = 0x08 | 0x04 | 0x01; // BENEATH | NO_SYMLINKS | NO_XDEV
fn open(dir: &File, n: &OsStr, flags: i32, mode: u32) -> Result<File> {
	let n = name(n)?;
	let how = How {
		flags: (flags
			| libc::O_CLOEXEC
			| libc::O_NOFOLLOW
			| if flags & libc::O_PATH == 0 {
				libc::O_NONBLOCK
			} else {
				0
			}) as u64,
		mode: mode as u64,
		resolve: RESOLVE,
	};
	let injected = hooks::open_error();
	let fd = if injected.is_some() {
		-1
	} else {
		unsafe {
			libc::syscall(
				libc::SYS_openat2,
				dir.as_raw_fd(),
				n.as_ptr(),
				&how,
				std::mem::size_of::<How>(),
			) as i32
		}
	};
	if fd < 0 {
		let e = injected
			.map(std::io::Error::from_raw_os_error)
			.unwrap_or_else(std::io::Error::last_os_error);
		return Err(format!(
			"openat2 {:?}: {e}; guarded open refused, no fallback",
			n
		));
	}
	Ok(unsafe { File::from_raw_fd(fd) })
}
fn directory(d: &File, n: &OsStr) -> Result<File> {
	open(d, n, libc::O_RDONLY | libc::O_DIRECTORY, 0)
}
fn stat(d: &File, n: &OsStr) -> Result<Option<libc::stat>> {
	let n = name(n)?;
	let mut s = std::mem::MaybeUninit::<libc::stat>::uninit();
	if unsafe {
		libc::fstatat(
			d.as_raw_fd(),
			n.as_ptr(),
			s.as_mut_ptr(),
			libc::AT_SYMLINK_NOFOLLOW,
		)
	} != 0
	{
		let e = std::io::Error::last_os_error();
		if e.kind() == std::io::ErrorKind::NotFound {
			return Ok(None);
		}
		return Err(error(e));
	}
	Ok(Some(unsafe { s.assume_init() }))
}
fn private(f: &File, dir: bool) -> Result<()> {
	let m = f.metadata().map_err(error)?;
	if m.uid() != unsafe { libc::geteuid() }
		|| m.mode() & 0o022 != 0
		|| (if dir { !m.is_dir() } else { !m.is_file() })
	{
		return Err("not a private owned control object".into());
	}
	Ok(())
}
fn same(f: &File, s: &libc::stat) -> Result<()> {
	let m = f.metadata().map_err(error)?;
	if m.dev() != s.st_dev || m.ino() != s.st_ino {
		return Err("directory identity changed".into());
	}
	Ok(())
}
fn sync(f: &File) -> Result<()> {
	f.sync_all().map_err(error)
}
fn mkdir(d: &File, n: &str) -> Result<File> {
	let c = name(OsStr::new(n))?;
	if unsafe { libc::mkdirat(d.as_raw_fd(), c.as_ptr(), 0o700) } != 0
		&& std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
	{
		return Err(io_error("mkdirat"));
	}
	let f = directory(d, OsStr::new(n))?;
	private(&f, true)?;
	Ok(f)
}
fn bounded_document(d: &File, n: &str, limit: usize, budget: &mut Budget) -> Result<Option<Value>> {
	if stat(d, OsStr::new(n))?.is_none() {
		return Ok(None);
	}
	let f = open(d, OsStr::new(n), libc::O_RDONLY, 0)?;
	private(&f, false)?;
	let bytes = read_bytes(&f, limit, budget)?;
	documents::decode(&bytes, budget).map(Some)
}
fn read_bytes(mut f: &File, limit: usize, budget: &mut Budget) -> Result<Vec<u8>> {
	let mut out = vec![];
	let mut buffer = [0u8; 16384];
	loop {
		commands::check()?;
		let n = f.read(&mut buffer).map_err(error)?;
		if n == 0 {
			break;
		}
		if n > limit.saturating_sub(out.len()) {
			return Err(format!("document exceeds {limit} bytes"));
		}
		budget.reserve(n as u64)?;
		out.extend_from_slice(&buffer[..n]);
	}
	Ok(out)
}
fn selected(root: &File, budget: &mut Budget) -> Result<Option<String>> {
	let Some(v) = bounded_document(root, "current.json", 65536, budget)
		.map_err(|e| format!("uninterpretable runtime selection: {e}"))?
	else {
		return Ok(None);
	};
	if v.as_object().is_none_or(|v| v.len() != 2) || !matches!(v["format"].as_u64(), Some(1 | 2)) {
		return Err("uninterpretable runtime selection".into());
	}
	Ok(Some(
		v["id"]
			.as_str()
			.filter(|v| valid_id(v))
			.ok_or("uninterpretable runtime selection")?
			.into(),
	))
}
fn valid_id(s: &str) -> bool {
	s.len() == 64
		&& s.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Default)]
struct Budget {
	nodes: u64,
	files: u64,
	memory: u64,
	logical: u64,
	allocated: u64,
	seen: HashSet<(u64, u64)>,
}
impl Budget {
	fn reserve(&mut self, bytes: u64) -> Result<()> {
		let next = self
			.memory
			.checked_add(bytes)
			.ok_or("accounting overflow")?;
		if next > hooks::limit("RNX_REMOVE_MEMORY_LIMIT", 256 * 1024 * 1024) {
			return Err("memory allowance exceeded".into());
		}
		self.memory = next;
		commands::check()
	}

	fn charge(&mut self, bytes: usize) -> Result<()> {
		self.nodes += 1;
		self.reserve(bytes as u64 + 128)?;
		let limit = hooks::limit("RNX_REMOVE_NODE_LIMIT", 1_000_000);
		if self.nodes > limit
			|| self.memory > hooks::limit("RNX_REMOVE_MEMORY_LIMIT", 256 * 1024 * 1024)
		{
			return Err("traversal allowance exceeded".into());
		}
		commands::check()
	}
	fn account(&mut self, s: &libc::stat) -> Result<()> {
		if self.seen.insert((s.st_dev, s.st_ino)) {
			self.allocated = self
				.allocated
				.checked_add(
					(s.st_blocks as u64)
						.checked_mul(512)
						.ok_or("block overflow")?,
				)
				.ok_or("block overflow")?;
			if s.st_mode & libc::S_IFMT == libc::S_IFREG {
				self.files += 1;
				self.logical = self
					.logical
					.checked_add(s.st_size as u64)
					.ok_or("byte overflow")?;
			}
		}
		Ok(())
	}
}
fn names(d: &File, b: &mut Budget) -> Result<Vec<OsString>> {
	names_capped(d, b, None)
}
fn names_capped(d: &File, b: &mut Budget, mut ids: Option<u64>) -> Result<Vec<OsString>> {
	// A fresh open description, rather than dup sharing a previous directory offset.
	let f = directory(d, OsStr::new("."))?;
	use std::os::fd::IntoRawFd;
	let fd = f.into_raw_fd();
	let dir = unsafe { libc::fdopendir(fd) };
	if dir.is_null() {
		unsafe { libc::close(fd) };
		return Err(io_error("fdopendir"));
	}
	let result = (|| {
		let mut out = vec![];
		loop {
			unsafe { *libc::__errno_location() = 0 };
			let e = unsafe { libc::readdir(dir) };
			if e.is_null() {
				if unsafe { *libc::__errno_location() } != 0 {
					return Err(io_error("readdir"));
				}
				break;
			}
			let n = unsafe { CStr::from_ptr((*e).d_name.as_ptr()) }.to_bytes();
			if n == b"." || n == b".." {
				continue;
			}
			if let Some(remaining) = &mut ids
				&& std::str::from_utf8(n).is_ok_and(valid_id)
			{
				if *remaining == 0 {
					return Err("listing exceeds entry allowance".into());
				}
				*remaining -= 1;
			}
			b.charge(n.len())?;
			out.push(OsString::from_vec(n.to_vec()));
		}
		out.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
		Ok(out)
	})();
	unsafe { libc::closedir(dir) };
	result
}
fn walk(d: &File, b: &mut Budget, depth: usize, erase: bool, deleted: &mut u64) -> Result<()> {
	if depth as u64 > hooks::limit("RNX_REMOVE_DEPTH_LIMIT", 128) {
		return Err("depth exceeds 128".into());
	}
	for n in names(d, b)? {
		commands::check()?;
		let s = stat(d, &n)?.ok_or("entry disappeared during walk")?;
		if s.st_uid != unsafe { libc::geteuid() } {
			return Err(format!("unowned data: {n:?}"));
		}
		b.account(&s)?;
		match s.st_mode & libc::S_IFMT {
			libc::S_IFDIR => {
				let child = directory(d, &n)?;
				same(&child, &s)?;
				walk(&child, b, depth + 1, erase, deleted)?;
				if erase {
					same(&child, &stat(d, &n)?.ok_or("directory disappeared")?)?;
					unlink(d, &n, true)?;
				}
			}
			libc::S_IFREG | libc::S_IFLNK => {
				// Open even regular leaves for metadata with the same mount guard.
				// O_PATH does not require file read/write permission or read contents.
				if s.st_mode & libc::S_IFMT == libc::S_IFREG {
					let f = open(d, &n, libc::O_PATH, 0)?;
					same(&f, &s)?;
				}
				if erase {
					hooks::point("before-unlink")?;
					unlink(d, &n, false)?;
					*deleted += 1;
					if *deleted == 1 {
						hooks::point("during-delete")?;
					}
				}
			}
			_ => return Err(format!("special file refused: {n:?}")),
		}
	}
	if erase {
		sync(d)?
	}
	Ok(())
}
fn unlink(d: &File, n: &OsStr, dir: bool) -> Result<()> {
	let n = name(n)?;
	if unsafe {
		libc::unlinkat(
			d.as_raw_fd(),
			n.as_ptr(),
			if dir { libc::AT_REMOVEDIR } else { 0 },
		)
	} != 0
	{
		return Err(io_error("unlinkat"));
	}
	Ok(())
}
fn quote(p: &OsStr) -> String {
	format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}
pub(super) fn run(mut a: Args) -> Result<()> {
	commands::install_signals()?;
	let mut budget = Budget::default();
	if !a.list && !a.dry {
		emit(
			&json!({"quiescence_acknowledged":true,"notice":"All consumers must remain stopped, including older tools, surviving build children, direct executions, sessions, servers and kernels. Retained projects and kernelspecs may break. Removing a runtime can discard its only source copy."}),
			&mut budget,
		)?;
	}
	a.root = future_root(&a.root)?;
	if std::fs::symlink_metadata(&a.root).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
		if !a.list {
			return Err(format!("store {:?} is absent", a.root));
		}
		let refs = annotations::inspect(&a, &mut budget)?;
		return report(
			json!({"root":a.root,"absent":true,"entries":[],"unknown":[]}),
			refs,
			&a,
			&mut budget,
			None,
		);
	}
	let root = OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW)
		.open(&a.root)
		.map_err(error)?;
	private(&root, true)?;
	let entries = directory(&root, OsStr::new("entries"))?;
	private(&entries, true)?;
	let lock_parent = if a.kind == "cache" {
		if stat(&root, OsStr::new("install.lock"))?.is_some()
			|| stat(&root, OsStr::new("current.json"))?.is_some()
		{
			return Err("opposite runtime controls".into());
		}
		let f = directory(&root, OsStr::new("locks"))?;
		private(&f, true)?;
		Some(f)
	} else {
		if stat(&root, OsStr::new("locks"))?.is_some() {
			return Err("opposite cache controls".into());
		}
		let f = open(&root, OsStr::new("install.lock"), libc::O_RDONLY, 0)?;
		private(&f, false)?;
		None
	};
	if a.list {
		let current = if a.kind == "runtime" {
			selected(&root, &mut budget)?
		} else {
			None
		};
		let refs = annotations::inspect(&a, &mut budget)?;
		let mut records = vec![];
		let mut unknown = vec![];
		let mut total = 0u64;
		for n in names(&root, &mut budget)? {
			if !matches!(
				n.to_str(),
				Some("entries" | "removing" | "locks" | "install.lock" | "current.json" | "build")
			) {
				budget.reserve(n.len() as u64 + 128)?;
				unknown.push(format!("{n:?}"));
			}
		}
		for location in ["entries", "removing", "build"] {
			if location == "build" && a.kind != "cache" {
				continue;
			}
			if stat(&root, OsStr::new(location))?.is_none() {
				continue;
			}
			let dir = directory(&root, OsStr::new(location))?;
			private(&dir, true)?;
			for n in names_capped(
				&dir,
				&mut budget,
				Some(hooks::limit("RNX_REMOVE_ENTRY_LIMIT", 10_000) - total),
			)? {
				let (key, pending_build) = match n.to_str() {
					Some(v) if valid_id(v) => (v, false),
					Some(v)
						if location == "removing"
							&& a.kind == "cache" && v
							.strip_prefix("build-")
							.is_some_and(valid_id) =>
					{
						(v.strip_prefix("build-").unwrap(), true)
					}
					_ => {
						unknown.push(format!("{location}/{n:?}"));
						continue;
					}
				};
				if total == hooks::limit("RNX_REMOVE_ENTRY_LIMIT", 10_000) {
					return Err("listing exceeds entry allowance".into());
				}
				total += 1;
				let entry = directory(&dir, &n)?;
				private(&entry, true)?;
				budget.seen.clear();
				let before = (budget.nodes, budget.files, budget.logical, budget.allocated);
				budget.charge(0)?;
				budget.account(&stat(&dir, &n)?.ok_or("entry disappeared")?)?;
				walk(&entry, &mut budget, 0, false, &mut 0)?;
				let metadata = if location == "build" || pending_build {
					build_metadata(&entries, key, &mut budget)?
				} else {
					metadata(&entry, &a.kind, key, &mut budget)?
				};
				let id = if location == "build" || pending_build {
					format!("build-{key}")
				} else {
					key.to_owned()
				};
				budget.reserve(4096 + a.root.as_os_str().len() as u64 * 2)?;
				records.push(json!({"id":id,"location":location,"path":a.root.join(location).join(&n),"selected":current.as_deref()==Some(key),"nodes":budget.nodes-before.0,"files":budget.files-before.1,"logical":budget.logical-before.2,"allocated_estimate":budget.allocated-before.3,"metadata":metadata}));
			}
		}
		return report(
			json!({"root":a.root,"entries":records,"unknown":unknown}),
			refs,
			&a,
			&mut budget,
			Some(&entries),
		);
	}

	let key = a.id.as_ref().unwrap();
	// Record 0069: a shared build directory keeps its kind through the
	// pending name, so every later spelling — listing, resume — reaches
	// it as `build-<key>` and takes the build directory's own lock.
	let pending_name = if a.build {
		format!("build-{key}")
	} else {
		key.clone()
	};
	let pending_os = OsStr::new(&pending_name);
	let key_os = if a.resume {
		pending_os
	} else {
		OsStr::new(key)
	};
	let location = if a.resume {
		"removing"
	} else if a.build {
		"build"
	} else {
		"entries"
	};
	let _lock = if a.dry {
		None
	} else {
		// Record 0069: a shared build directory's coordination lock is
		// `locks/build-<key>.lock`, held shared by every builder from before
		// Cargo until publication; removal takes it exclusively and refuses
		// while any builder holds it.
		let (parent, n, flags, mode) = if let Some(p) = &lock_parent {
			(
				p,
				if a.build {
					format!("build-{key}.lock")
				} else {
					format!("{key}.lock")
				},
				libc::O_RDWR | libc::O_CREAT,
				0o600,
			)
		} else {
			(&root, "install.lock".into(), libc::O_RDWR, 0)
		};
		let f = open(parent, OsStr::new(&n), flags, mode)?;
		private(&f, false)?;
		f.try_lock().map_err(|e| {
			if a.build {
				format!("busy: a builder holds the shared build directory build-{key} at {:?}: {e}; wait for every build to finish, then retry", a.root)
			} else {
				format!("busy or invalid writer lock for {key} at {:?}: {e}", a.root)
			}
		})?;

		Some(f)
	};
	if a.kind == "runtime" && selected(&root, &mut budget)?.as_deref() == Some(key) {
		return Err(format!(
			"selected runtime {key} in {:?} refused; select or install a replacement first",
			a.root
		));
	}
	if !a.resume && stat(&root, OsStr::new("removing"))?.is_some() {
		let d = directory(&root, OsStr::new("removing"))?;
		private(&d, true)?;
		if stat(&d, pending_os)?.is_some() {
			return Err(format!(
				"pending removal exists; resume explicitly:\n{}",
				resume_command(&a, key)?
			));
		}
	}
	if stat(&root, OsStr::new(location))?.is_none() {
		return Err(format!(
			"no {location}/{key} in {:?}; nothing removed",
			a.root
		));
	}
	let parent = directory(&root, OsStr::new(location))?;
	if stat(&parent, key_os)?.is_none() {
		return Err(format!(
			"no {location}/{key} in {:?}; nothing removed",
			a.root
		));
	}
	private(&parent, true)?;
	let target = directory(&parent, key_os)?;
	private(&target, true)?;
	// The target's actual component: `build-<key>` under `removing/` for a
	// pending build directory, so containment and reporting name what is
	// really removed.
	let path = a.root.join(location).join(key_os);
	for own in [
		std::env::current_dir().map_err(error)?,
		std::env::current_exe().map_err(error)?,
	] {
		if own.starts_with(&path) {
			return Err("current executable or working directory is inside target".into());
		}
	}
	let initial = target.metadata().map_err(error)?;
	budget.charge(0)?;
	budget.account(&stat(&parent, key_os)?.ok_or("target disappeared")?)?;
	walk(&target, &mut budget, 0, false, &mut 0)?;
	if a.dry {
		let refs = annotations::inspect(&a, &mut budget)?;
		let metadata = if a.build {
			build_metadata(&entries, key, &mut budget)?
		} else {
			metadata(&target, &a.kind, key, &mut budget)?
		};
		return report(
			json!({"root":a.root,"dry_run":true,"entries":[{"id":a.display_id(),"location":location,"path":path,"nodes":budget.nodes,"files":budget.files,"logical":budget.logical,"allocated_estimate":budget.allocated,"metadata":metadata}],"writer":"not reserved"}),
			refs,
			&a,
			&mut budget,
			Some(&entries),
		);
	}
	let command = resume_command(&a, key)?;
	let mut committed = a.resume;
	let operation = (|| {
		hooks::point("before-rename")?;
		let again = directory(&parent, key_os)?;
		let now = again.metadata().map_err(error)?;
		if initial.dev() != now.dev() || initial.ino() != now.ino() {
			return Err("target identity changed before commit".into());
		}
		let pending = if a.resume {
			parent
		} else {
			let pending = mkdir(&root, "removing")?;
			hooks::point("before-root-sync")?;
			sync(&root)?;
			hooks::point("after-root-sync")?;
			let key_c = name(key_os)?;
			let pending_c = name(pending_os)?;
			if unsafe {
				libc::syscall(
					libc::SYS_renameat2,
					parent.as_raw_fd(),
					key_c.as_ptr(),
					pending.as_raw_fd(),
					pending_c.as_ptr(),
					libc::RENAME_NOREPLACE,
				)
			} != 0
			{
				return Err(io_error("renameat2"));
			}
			committed = true;
			emit(
				&json!({"committed":true,"pending":a.root.join("removing").join(&pending_name),"resume_command":command}),
				&mut budget,
			)?;
			std::io::stdout().flush().map_err(error)?;
			let moved = directory(&pending, pending_os)?;
			let m = moved.metadata().map_err(error)?;
			if m.dev() != initial.dev() || m.ino() != initial.ino() {
				return Err("target identity changed after rename".into());
			}
			hooks::point("after-rename")?;
			hooks::point("before-entries-sync")?;
			sync(&parent)?;
			hooks::point("after-entries-sync")?;
			hooks::point("before-pending-sync")?;
			sync(&pending)?;
			hooks::point("after-pending-sync")?;
			pending
		};
		let mut deletion = Budget {
			nodes: budget.nodes,
			memory: budget.memory,
			..Budget::default()
		};
		let mut deleted = 0;
		walk(&target, &mut deletion, 0, true, &mut deleted)?;
		same(
			&target,
			&stat(&pending, pending_os)?.ok_or("pending entry disappeared")?,
		)?;
		unlink(&pending, pending_os, true)?;
		hooks::point("before-final-sync")?;
		sync(&pending)?;
		hooks::point("after-final-sync")?;
		emit(
			&json!({"removed":a.display_id(),"location":location,"deleted_leaves":deleted,"logical_before":budget.logical,"allocated_estimate_before":budget.allocated}),
			&mut budget,
		)?;
		Ok(())
	})();
	operation.map_err(|e| {
		if committed {
			format!("pending removal may remain: {e}\n{command}")
		} else {
			format!("before commitment: {e}")
		}
	})
}
fn future_root(path: &Path) -> Result<PathBuf> {
	match path.canonicalize() {
		Ok(p) => Ok(p),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			if std::fs::symlink_metadata(path).is_ok() {
				return Err(format!("broken store path: {path:?}"));
			}
			let parent = path.parent().ok_or("store has no parent")?;
			Ok(future_root(parent)?.join(path.file_name().ok_or("store has no name")?))
		}
		Err(e) => Err(format!("store {path:?}: {e}")),
	}
}
fn resume_command(a: &Args, key: &str) -> Result<String> {
	Ok(format!(
		"{} {} remove {} --root {} --resume --quiescent",
		quote(std::env::current_exe().map_err(error)?.as_os_str()),
		a.kind,
		if a.build {
			format!("build-{key}")
		} else {
			key.to_owned()
		},
		quote(a.root.as_os_str())
	))
}
/// Record 0069: what the tool recorded about a shared build directory — the
/// local entries whose ready document names its key. Not a discovery of
/// every consumer: only what this root's ready documents say.
fn build_metadata(entries: &File, key: &str, budget: &mut Budget) -> Result<Value> {
	let mut referencing = 0u64;
	let mut unreadable = 0u64;
	for n in names_capped(
		entries,
		budget,
		Some(hooks::limit("RNX_REMOVE_ENTRY_LIMIT", 10_000)),
	)? {
		let Some(id) = n.to_str().filter(|v| valid_id(v)) else {
			continue;
		};
		let _ = id;
		let entry = match directory(entries, &n) {
			Ok(d) => d,
			Err(_) => {
				unreadable += 1;
				continue;
			}
		};
		match bounded_document(&entry, "ready.json", crate::input::DOCUMENT_LIMIT, budget) {
			Ok(Some(v)) => {
				let identity: Option<Value> = v["identity"]
					.as_str()
					.and_then(|s| serde_json::from_str(s).ok());
				if identity
					.as_ref()
					.and_then(|i| i["context"]["build"]["key"].as_str())
					== Some(key)
				{
					referencing += 1;
				}
			}
			Ok(None) => {}
			Err(_) => unreadable += 1,
		}
	}
	Ok(
		json!({"status":"shared build directory","kind":"build","key":key,"referencing_entries":referencing,"unreadable_entries":unreadable,"scope":"local entries whose ready document names this key; not every consumer"}),
	)
}
fn metadata(entry: &File, kind: &str, key: &str, budget: &mut Budget) -> Result<Value> {
	let (n, limit) = if kind == "cache" {
		("ready.json", crate::input::DOCUMENT_LIMIT)
	} else {
		("installation.json", 65536)
	};
	match bounded_document(entry, n, limit, budget) {
		Ok(Some(v)) => {
			let known = matches!(v["format"].as_u64(), Some(1 | 2))
				|| (kind == "cache" && v["format"] == 3);
			let id = v[if kind == "cache" { "key" } else { "id" }].as_str() == Some(key);
			let suffix = if v["format"] == 1 { "sha256" } else { "blake3" };
			let shape = if kind == "cache" {
				v.as_object().is_some_and(|o| o.len() == 5)
					&& v["identity"].is_string()
					&& v["artifact"].is_string()
					&& v[format!("executable_{suffix}")]
						.as_str()
						.is_some_and(valid_id)
			} else {
				v[format!("tree_{suffix}")].as_str().is_some_and(valid_id)
					&& v[format!("tool_{suffix}")].as_str().is_some_and(valid_id)
					&& v["files"].is_u64()
					&& v["bytes"].is_u64()
					&& v["layout"].is_string()
					&& v["source_path"].is_string()
					&& v["dirty_tracked"].is_boolean()
					&& v["installed_utc"].is_string()
					&& v["tool_version"].is_string()
			};
			Ok(
				json!({"status":if known&&id&&shape {"recognized envelope; not authenticated"}else{"unrecognized"},"format":v.get("format"),"source_path":v.get("source_path"),"source_commit":v.get("source_commit"),"installed_utc":v.get("installed_utc")}),
			)
		}
		Ok(None) => Ok(json!({"status":"unrecognized","reason":"document missing"})),
		Err(e) => {
			commands::check()?;
			Ok(json!({"status":"unrecognized","reason":e}))
		}
	}
}
fn emit(value: &Value, budget: &mut Budget) -> Result<()> {
	render(value, budget, false)
}
fn render(value: &Value, budget: &mut Budget, pretty: bool) -> Result<()> {
	let bytes = if pretty {
		crate::wire::pretty(value)?
	} else {
		crate::wire::encode(value)?
	};
	budget.reserve(bytes.len() as u64)?;
	let text = std::str::from_utf8(&bytes).map_err(error)?;
	let mut out = String::new();
	let limit = hooks::limit("RNX_REMOVE_OUTPUT_LIMIT", 16 * 1024 * 1024) as usize;
	for c in text.chars() {
		let mut buffer = [0u8; 4];
		let escaped;
		let piece = if c == '\u{7f}'
			|| ('\u{80}'..='\u{9f}').contains(&c)
			|| matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{2028}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
		{
			{
				escaped = format!("\\u{:04x}", c as u32);
				escaped.as_str()
			}
		} else {
			c.encode_utf8(&mut buffer)
		};
		if piece.len() > limit.saturating_sub(out.len()) {
			return Err("rendered output exceeds allowance".into());
		}
		budget.reserve(piece.len() as u64)?;
		out.push_str(piece);
	}
	writeln!(std::io::stdout().lock(), "{out}").map_err(error)
}
fn report(
	mut value: Value,
	mut refs: Vec<Value>,
	a: &Args,
	budget: &mut Budget,
	store_entries: Option<&File>,
) -> Result<()> {
	let entries = value["entries"]
		.as_array_mut()
		.ok_or("report entries missing")?;
	for reference in &mut refs {
		if let Some(path) = reference["path"].as_str() {
			budget.reserve(path.len() as u64)?;
			let path = PathBuf::from(path);
			let matched = entries.iter_mut().find(|e| {
				matches!(
					reference["kind"].as_str(),
					Some("shared assembly" | "installed runtime")
				) && e["path"].as_str().is_some_and(|p| Path::new(p) == path)
			});
			if let Some(entry) = matched {
				if entry.get("referenced_by").is_none() {
					entry["referenced_by"] = json!([])
				}
				entry["referenced_by"]
					.as_array_mut()
					.unwrap()
					.push(reference["manifest"].clone());
				reference["relation"] = json!("referenced by named manifest");
			} else if !path.starts_with(&a.root) {
				reference["relation"] = json!("outside selected store")
			} else {
				reference["relation"] = match recorded_path(&path, a, store_entries) {
					Ok(v) => json!(v),
					Err(e) => {
						reference["status"] = json!("indeterminate");
						json!(e)
					}
				};
			}
		}
	}
	let incomplete = refs.iter().any(|r| r["status"] == "indeterminate");
	value["references"] = json!(refs);
	value["reference_scope"] = json!(
		"Only the manifests named were checked; other consumers may exist. Recorded references are not lifetime leases or deletion authorization."
	);
	value["size_note"] = json!("Allocated bytes are estimates, not a promise of reclaimed space.");
	render(&value, budget, true)?;
	if incomplete {
		Err("one or more named project annotations are indeterminate; no completeness claim".into())
	} else {
		Ok(())
	}
}

fn recorded_path(path: &Path, a: &Args, entries: Option<&File>) -> Result<&'static str> {
	let Ok(relative) = path.strip_prefix(a.root.join("entries")) else {
		return Ok("recorded path not part of this inspection");
	};
	let Some(id) = relative.to_str().filter(|v| valid_id(v)) else {
		return Ok("recorded path not part of this inspection");
	};
	let Some(entries) = entries else {
		return Ok("recorded entry missing");
	};
	if stat(entries, OsStr::new(id))?.is_none() {
		return Ok("recorded entry missing");
	}
	let entry = directory(entries, OsStr::new(id))?;
	private(&entry, true)?;
	Ok("recorded path not part of this inspection")
}
