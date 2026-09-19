//! Private describe/prepare protocol. No public CLI spelling and no Rune execution.
use super::*;
use crate::{catalogue, dep_wire as protocol};
use std::io::Read;
pub(super) const CARRIER: &str = "RNX_INTERNAL_SESSION_V1";
fn text(p: &Path) -> Result<String, String> {
	p.to_str()
		.map(str::to_owned)
		.ok_or("non-Unicode transition path".into())
}
fn recipe(m: &Manifest) -> Result<String, String> {
	Ok(hash(&wire::encode(&(
		&m.runtime,
		&m.executable,
		&m.native,
	))?))
}
fn roster(m: &Manifest) -> String {
	m.native.keys().cloned().collect::<Vec<_>>().join("\n")
}
pub(super) fn association(
	p: &Project,
	lock: &Lock,
	checked: &artifact::Checked,
	digest: &str,
) -> Result<String, String> {
	let fields = protocol::Fields::from([
		(1, text(&p.manifest)?),
		(
			2,
			text(
				&std::env::current_exe()
					.map_err(err)?
					.canonicalize()
					.map_err(err)?,
			)?,
		),
		(3, recipe(&lock.declarations)?),
		(4, roster(&lock.declarations)),
		(5, digest.into()),
		(6, text(checked.path())?),
	]);
	let encoded = protocol::hex(&protocol::encode(7, &fields)?);
	protocol::capsule(&encoded)?;
	Ok(encoded)
}
impl Project {
	// Describe must not create a command lock, .rnx, map or receipt refresh.
	fn inspect(path: &Path) -> Result<Self, String> {
		let manifest = path.canonicalize().map_err(err)?;
		let base = manifest
			.parent()
			.ok_or("manifest has no parent")?
			.to_owned();
		let dot = base.join(".rnx");
		Ok(Self {
			manifest,
			base,
			dot: dot.clone(),
			stage: dot.join("assembly"),
			_guard: None,
		})
	}
}
fn validate_association(
	p: &Project,
	capsule: &str,
	executable: &str,
	installed: &str,
) -> Result<(), String> {
	let a = protocol::capsule(capsule)?;
	let (lock, bytes) = p.read_lock()?;
	let current = Manifest::read(&p.manifest)?;
	if a[&1] != text(&p.manifest)?
		|| a[&3] != recipe(&current)?
		|| a[&3] != recipe(&lock.declarations)?
	{
		return Err("session association declarations changed; reopen the project session".into());
	}
	if a[&4] != installed || a[&4] != roster(&current) {
		return Err(
			"session association extension roster mismatch; reopen the project session".into(),
		);
	}
	// Source bytes may be stale; preparation resolves them. The lock pair,
	// receipt binding, ready document and artifact still have to be valid.
	let (checked, digest) = p.checked_artifact(&lock, &bytes, false, false)?;
	if a[&5] != digest
		|| a[&6] != text(checked.path())?
		|| Path::new(executable).canonicalize().map_err(err)?
			!= checked.path().canonicalize().map_err(err)?
	{
		return Err("session association executable mismatch; reopen the project session".into());
	}
	Ok(())
}
struct Description {
	manifest: PathBuf,
	original: Vec<u8>,
	candidate: catalogue::Candidate,
	entries: Vec<catalogue::Entry>,
	scratch: bool,
	offline: bool,
	runtime: Option<crate::runtime_install::Selection>,
	git: Option<crate::entry::Coordinates>,
}
impl Description {
	fn fields(&self) -> Result<protocol::Fields, String> {
		let mut notice = format!(
			"{}: {}\nAdding: {}\nAlready declared: {}\n{}\n",
			if self.scratch {
				"New scratch project"
			} else {
				"Project"
			},
			self.manifest.display(),
			if self.candidate.added.is_empty() {
				"(none)".into()
			} else {
				self.candidate.added.join(", ")
			},
			if self.candidate.existing.is_empty() {
				"(none)".into()
			} else {
				self.candidate.existing.join(", ")
			},
			if self.offline {
				"Offline: Cargo may not fetch sources."
			} else {
				"Online: Cargo may fetch sources."
			}
		);
		if let Some(runtime) = &self.runtime {
			notice.push_str(&runtime.notice);
			notice.push('\n');
		}
		if let Some(git) = self.git {
			notice.push_str(&git.notice()?);
			notice.push('\n');
		}
		if self.candidate.added.iter().any(|n| n == "polars") {
			notice.push_str("A first Polars build took about 100 seconds with registry sources cached; its retained shared entry is about 1.5 GB. A reusable entry avoids compilation.\n");
		}
		notice.push_str("This prepares a new executable and restarts the session on success.\nExisting bindings and declarations will be lost. Saved history remains available with up-arrow; nothing is replayed.");
		Ok(protocol::Fields::from([
			(1, hash(&self.candidate.bytes)),
			(2, notice),
			(3, text(&self.manifest)?),
			(4, if self.scratch { "scratch" } else { "project" }.into()),
			(5, self.candidate.added.join("\n")),
			(6, self.candidate.existing.join("\n")),
			(7, if self.offline { "offline" } else { "online" }.into()),
		]))
	}
}
fn state_root() -> Result<PathBuf, String> {
	let path = if let Some(p) = std::env::var_os("XDG_STATE_HOME") {
		PathBuf::from(p)
	} else {
		PathBuf::from(std::env::var_os("HOME").ok_or("HOME is missing")?).join(".local/state")
	};
	if !path.is_absolute() {
		return Err("state root must be absolute".into());
	}
	// Resolve the user's existing prefix without creating anything in describe.
	let mut cursor = path.clone();
	let mut rest = Vec::new();
	while !cursor.try_exists().map_err(err)? {
		rest.push(
			cursor
				.file_name()
				.ok_or("state root has no parent")?
				.to_owned(),
		);
		cursor.pop();
	}
	let mut canonical = cursor.canonicalize().map_err(err)?;
	if !canonical.is_dir() {
		return Err("state root is not a directory".into());
	}
	for part in rest.into_iter().rev() {
		canonical.push(part)
	}
	Ok(canonical)
}
fn scratch_path() -> Result<PathBuf, String> {
	let base = state_root()?.join("rnx/sessions");
	for p in [base.parent().unwrap(), base.as_path()] {
		match fs::symlink_metadata(p) {
			Ok(_) => crate::cache_storage::private_directory(p)?,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
			Err(e) => return Err(err(e)),
		}
	}
	let nonce = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map_err(err)?
		.as_nanos();
	Ok(base
		.join(format!("session-{}-{nonce}", std::process::id()))
		.join("rnx.toml"))
}
fn describe(request: &protocol::Fields, proposed: Option<&Path>) -> Result<Description, String> {
	protocol::exact(
		request,
		if request.contains_key(&6) {
			&[1, 2, 3, 4, 5, 6]
		} else {
			&[1, 2, 3, 4, 5]
		},
	)?;
	super::startup::flags(request.get(&6).map(String::as_str).unwrap_or(""))?;
	let entries = catalogue::select(&request[&1].lines().map(str::to_owned).collect::<Vec<_>>())?;
	let offline = match request[&5].as_str() {
		"offline" => true,
		"online" => false,
		_ => return Err("invalid offline mode".into()),
	};
	let scratch = request[&2].is_empty();
	let mut selection = None;
	let mut git = None;
	let (manifest, original) = if scratch {
		if !request[&4].is_empty() {
			return Err("custom executable needs a project association; open it through rnx project session".into());
		}
		let runtime = if crate::entry::stock() && std::env::var_os("RNX_DEP_RUNTIME").is_none() {
			let coordinate = crate::entry::coordinates().ok_or("missing stock coordinates")?;
			coordinate.notice()?;
			git = Some(coordinate);
			None
		} else {
			let runtime = crate::runtime_install::Selection::discover()?;
			let path = runtime.source.clone();
			selection = Some(runtime);
			Some(path)
		};

		let selected = scratch_path()?;
		let manifest = match proposed {
			Some(p)
				if p.parent().and_then(Path::parent)
					== selected.parent().and_then(Path::parent) =>
			{
				p.to_owned()
			}
			Some(_) => return Err("scratch state root changed; request a new description".into()),
			None => selected,
		};
		let parent = manifest.parent().unwrap();
		if runtime
			.as_ref()
			.is_some_and(|r| parent.starts_with(r) || r.starts_with(parent))
		{
			return Err("scratch and native root must not contain one another".into());
		}
		if fs::symlink_metadata(parent).is_ok() {
			return Err(
				"proposed scratch directory is already occupied; request a new description".into(),
			);
		}
		let raw = if let Some(git) = git {
			format!(
				"format = 2\n[application]\nentry = \"entry.rn\"\n[runtime]\ngit = {}\nrev = {}\n",
				toml::Value::String(git.url.into()),
				toml::Value::String(git.revision.into())
			)
		} else {
			format!(
				"format = 1\n[application]\nentry = \"entry.rn\"\n[runtime]\npath = {}\n",
				toml::Value::String(text(runtime.as_ref().unwrap())?)
			)
		};
		(manifest, raw.into_bytes())
	} else {
		let a = protocol::capsule(&request[&2])?;
		if a[&2]
			!= text(
				&std::env::current_exe()
					.map_err(err)?
					.canonicalize()
					.map_err(err)?,
			)? {
			return Err("session association tool mismatch".into());
		}
		let p = Project::inspect(Path::new(&a[&1]))?;
		validate_association(&p, &request[&2], &request[&3], &request[&4])?;
		let raw = input::read(&p.manifest, input::MANIFEST_LIMIT)?;
		(p.manifest, raw)
	};
	let candidate = if let Some(git) = git {
		catalogue::git_candidate(original.clone(), &entries, git.url, git.revision)
	} else {
		catalogue::author(original.clone(), manifest.parent().unwrap(), &entries)?
	};
	Ok(Description {
		manifest,
		original,
		candidate,
		entries,
		scratch,
		offline,
		runtime: selection,
		git,
	})
}
fn private_dir(path: &Path) -> Result<(), String> {
	match fs::symlink_metadata(path) {
		Ok(_) => crate::cache_storage::private_directory(path),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			if let Some(parent) = path.parent() {
				private_dir(parent)?;
			}
			let mut b = fs::DirBuilder::new();
			#[cfg(unix)]
			{
				use std::os::unix::fs::DirBuilderExt;
				b.mode(0o700);
			}
			b.create(path).map_err(err)
		}
		Err(e) => Err(err(e)),
	}
}
fn create_scratch(d: &Description) -> Result<(), String> {
	let parent = d.manifest.parent().unwrap();
	// Existing ancestors above the selected user root need not be private.
	let root = state_root()?;
	if !root.exists() {
		let mut directories = fs::DirBuilder::new();
		directories.recursive(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::DirBuilderExt;
			directories.mode(0o700);
		}
		directories.create(&root).map_err(err)?;
	}
	private_dir(&root.join("rnx"))?;
	private_dir(&root.join("rnx/sessions"))?;
	let mut b = fs::DirBuilder::new();
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;
		b.mode(0o700);
	}
	b.create(parent)
		.map_err(|e| format!("scratch allocation refused: {e}"))?;
	for (path, bytes) in [
		(d.manifest.clone(), d.original.as_slice()),
		(
			parent.join("entry.rn"),
			b"pub fn main(_) { () }\n".as_slice(),
		),
	] {
		let mut options = OpenOptions::new();
		options.create_new(true).write(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options
				.mode(0o600)
				.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
		}
		let mut f = options.open(path).map_err(err)?;
		f.write_all(bytes).map_err(err)?;
		f.sync_all().map_err(err)?;
	}
	File::open(parent).and_then(|f| f.sync_all()).map_err(err)
}
#[cfg(unix)]
pub(super) fn serve(args: Vec<OsString>) -> Result<(), String> {
	use std::os::fd::{AsRawFd, FromRawFd};
	use std::os::unix::net::UnixStream;
	let capability = args.len() == 1 && args[0] == "management-version";
	if !args.is_empty() && !capability {
		return Err("private transition takes no arguments".into());
	}
	let fd = std::env::var("RNX_INTERNAL_DEP_FD")
		.map_err(err)?
		.parse::<i32>()
		.map_err(err)?;
	if fd < 3 {
		return Err("invalid transition descriptor".into());
	}
	if unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
		return Err("invalid transition descriptor".into());
	}
	let mut socket = unsafe { UnixStream::from_raw_fd(fd) };
	if unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
		return Err(err(std::io::Error::last_os_error()));
	}
	socket
		.set_read_timeout(Some(std::time::Duration::from_secs(1800)))
		.map_err(err)?;
	socket
		.set_write_timeout(Some(std::time::Duration::from_secs(5)))
		.map_err(err)?;
	if capability {
		socket
			.set_read_timeout(Some(std::time::Duration::from_secs(5)))
			.map_err(err)?;
		let (kind, fields) = protocol::read(&mut socket)?;
		if kind != 6 {
			return Err("expected management capability request".into());
		}
		protocol::exact(&fields, &[])?;
		return protocol::write(&mut socket, 6, &protocol::Fields::from([(1, "1".into())]));
	}
	commands::install_signals()?;
	commands::preparation_deadline();
	let outcome = (|| {
		let (kind, request) = protocol::read(&mut socket)?;
		if kind != 1 {
			return Err("expected describe request".into());
		}
		let description = describe(&request, None)?;
		let fields = description.fields()?;
		protocol::write(&mut socket, 2, &fields)?;
		let (kind, answer) = protocol::read(&mut socket)?;
		if kind == 3 {
			protocol::exact(&answer, &[])?;
			return Ok(());
		}
		if kind != 4 {
			return Err("expected consent or decline".into());
		}
		protocol::exact(&answer, &[1])?;
		if answer[&1] != fields[&1] {
			return Err("consent description mismatch".into());
		}
		// Confirmation is one message; extra bytes are never another command.
		let mut tail = [0];
		if socket.read(&mut tail).map_err(err)? != 0 {
			return Err("trailing consent data".into());
		}
		environment()?;
		let fresh = describe(&request, Some(&description.manifest))?;
		if fresh.original != description.original || fresh.fields()? != fields {
			return Err("description changed; request consent again".into());
		}
		if fresh.candidate.added.is_empty() {
			return Ok(());
		}
		if let Some(git) = fresh.git {
			return Err(format!(
				"Git-source preparation is not enabled in this checkpoint (0067 gate 3); no sources fetched or scratch written.\n{}",
				git.override_help()
			));
		}
		if let Some(runtime) = &fresh.runtime {
			runtime.validate()?;
		}
		if fresh.scratch {
			create_scratch(&fresh)?;
		}
		let p = Project::open(&fresh.manifest)?;
		if input::read(&p.manifest, input::MANIFEST_LIMIT)? != fresh.original {
			return Err("manifest changed before authoring; request consent again".into());
		}
		if !fresh.scratch {
			validate_association(&p, &request[&2], &request[&3], &request[&4])?;
		}
		let mut phase = "author";
		let prepare: Result<(), String> = (|| {
			eprintln!("dependency phase: author");
			#[cfg(feature = "test-support")]
			protocol::trace("tool-author");
			fault("dep-author")?;
			p.add(&fresh.entries)?;
			phase = "resolve";
			eprintln!("dependency phase: resolve");
			#[cfg(feature = "test-support")]
			protocol::trace("tool-resolve");
			fault("dep-resolve")?;
			p.lock(fresh.offline)?;
			phase = "build/attach";
			eprintln!("dependency phase: build/attach");
			#[cfg(feature = "test-support")]
			protocol::trace("tool-build");
			fault("dep-build")?;
			p.build(fresh.offline)?;
			let (lock, bytes) = p.read_lock()?;
			p.verify_inputs(&lock)?;
			let (checked, digest) = p.checked_artifact(&lock, &bytes, false, true)?;
			checked.recheck()?;
			phase = "startup check";
			eprintln!("dependency phase: startup check");
			#[cfg(feature = "test-support")]
			protocol::trace("tool-probe");
			super::startup::check(&checked, request.get(&6).map(String::as_str).unwrap_or(""))?;
			commands::check()?;
			p.verify_inputs(&lock)?;
			let (again, again_bytes) = p.read_lock()?;
			if again != lock || again_bytes != bytes {
				return Err("lock changed during startup check".into());
			}
			checked.recheck()?;
			let replacement = association(&p, &lock, &checked, &digest)?;
			let stamp = protocol::stamp(checked.path())?;
			checked.recheck()?;
			let mut ready =
				protocol::Fields::from([(1, text(checked.path())?), (2, replacement), (3, stamp)]);
			if fresh.scratch {
				ready.insert(
					4,
					format!(
						"{} session --manifest {}",
						crate::entry::project_prefix()?,
						super::add::shell_word(&p.manifest)?
					),
				);
			}
			#[cfg(feature = "test-support")]
			protocol::trace("tool-ready");
			protocol::write(&mut socket, 5, &ready)?;
			Ok(())
		})();
		prepare.map_err(|e| {
			format!(
				"{phase}: {e}; project files may have been published; retry with:\n{}",
				crate::entry::recovery(&p.manifest).unwrap_or_else(|e| e)
			)
		})
	})();
	if let Err(e) = outcome {
		protocol::write(&mut socket, 8, &protocol::Fields::from([(1, e)]))?;
	}
	Ok(())
}
#[cfg(not(unix))]
pub(super) fn serve(_: Vec<OsString>) -> Result<(), String> {
	Err("dependency transitions require Unix supervision".into())
}
