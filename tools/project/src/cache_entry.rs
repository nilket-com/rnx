//! Shared-entry publication. Gate-three staging: receipt/CLI selection is gate four.
//! Callers hold their project lock first and keep the returned entry lease until
//! their receipt is published. Readiness is always re-read after acquiring a key.
#![allow(dead_code)]
use crate::{artifact, cache_storage as storage, commands, fingerprint, input, wire};
use serde::{Deserialize, Serialize};
use std::{
	fs::{self, File, OpenOptions},
	io::{Read, Write},
	path::{Path, PathBuf},
	process::Command,
	time::Duration,
};
pub(crate) trait Identity {
	fn context(&self) -> &crate::cache_identity::Context;
	/// Package names of the assembly's natives, for refusals.
	fn natives(&self) -> Vec<String>;
	fn wrapper(&self) -> (&str, &str);
	fn key(&self) -> &str;
	fn bytes(&self) -> &[u8];
	fn check_lock(&self, bytes: &[u8]) -> Result<(), String>;
	fn revalidate(&self, stage: &Path) -> Result<(), String>;
	fn ready_format(&self) -> u32;
}
macro_rules! identity {
	($ty:ty,$format:expr) => {
		impl Identity for $ty {
			fn context(&self) -> &crate::cache_identity::Context {
				self.context()
			}
			fn natives(&self) -> Vec<String> {
				self.native()
					.packages
					.iter()
					.map(|p| p.name.clone())
					.chain(self.git_names())
					.collect()
			}
			fn wrapper(&self) -> (&str, &str) {
				self.wrapper()
			}
			fn key(&self) -> &str {
				self.key()
			}
			fn bytes(&self) -> &[u8] {
				self.bytes()
			}
			fn check_lock(&self, bytes: &[u8]) -> Result<(), String> {
				self.check_lock(bytes)
			}
			fn revalidate(&self, stage: &Path) -> Result<(), String> {
				self.revalidate(stage)
			}
			fn ready_format(&self) -> u32 {
				$format
			}
		}
	};
}
identity!(crate::cache_identity::Identity, 2);
identity!(crate::new_identity::Identity, 3);
fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
	format: u32,
	key: String,
	identity: String,
	executable_blake3: String,
	artifact: String,
}
pub(crate) struct Entry {
	_lock: File,
	pub artifact: artifact::Checked,
	pub digest: String,
	pub hit: bool,
}
fn options() -> OpenOptions {
	let o = OpenOptions::new();
	#[cfg(unix)]
	let o = {
		let mut o = o;
		use std::os::unix::fs::OpenOptionsExt;
		o.mode(0o600)
			.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
		o
	};
	o
}
fn regular(f: &File, path: &Path) -> Result<(), String> {
	let m = f.metadata().map_err(err)?;
	if !m.is_file() {
		return Err(format!("not a regular cache file: {}", path.display()));
	}
	#[cfg(unix)]
	{
		use std::os::unix::fs::MetadataExt;
		if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o022 != 0 {
			return Err(format!(
				"not a private owned cache file: {}",
				path.display()
			));
		}
	}
	Ok(())
}
fn exists(path: &Path) -> Result<bool, String> {
	match fs::symlink_metadata(path) {
		Ok(_) => Ok(true),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
		Err(e) => Err(err(e)),
	}
}
/// Record 0069: a builder's shared hold on the build directory's coordination
/// lock, at a stable path outside the removable directory. Removal takes the
/// same lock exclusively and refuses while any builder holds it; a builder
/// arriving during a removal waits for it to finish.
fn lock_shared(path: &Path) -> Result<File, String> {
	let f = options()
		.read(true)
		.write(true)
		.create(true)
		.open(path)
		.map_err(err)?;
	regular(&f, path)?;
	loop {
		commands::check()?;
		match f.try_lock_shared() {
			Ok(()) => return Ok(f),
			Err(std::fs::TryLockError::WouldBlock) => {
				std::thread::sleep(Duration::from_millis(10));
			}
			Err(e) => return Err(format!("build directory lock {}: {e}", path.display())),
		}
	}
}
/// The shared build directory an identity recorded, if any: `<root>/build/<key>`.
pub(crate) fn shared_build_directory(identity: &impl Identity) -> Option<PathBuf> {
	match &identity.context().build {
		Some(crate::cache_identity::Build::Shared { key }) => {
			Some(identity.context().cache_root.join("build").join(key))
		}
		_ => None,
	}
}
/// The executable scan of decision 3: an executable published from a shared
/// build must not hold the directory's path. It finds `env!("OUT_DIR")`
/// readers and code generated into the directory; it cannot find a reader
/// that learns the path at runtime, which the declaration covers.
fn references(executable: &Path, directory: &Path) -> Result<bool, String> {
	let needle = directory.as_os_str().as_encoded_bytes();
	let mut f = options().read(true).open(executable).map_err(err)?;
	regular(&f, executable)?;
	let mut bytes = Vec::new();
	(&mut f)
		.take(fingerprint::BYTES + 1)
		.read_to_end(&mut bytes)
		.map_err(err)?;
	if bytes.len() as u64 > fingerprint::BYTES {
		return Err("artifact exceeds byte allowance".into());
	}
	let first = needle[0];
	Ok(bytes
		.iter()
		.enumerate()
		.filter(|(_, b)| **b == first)
		.any(|(i, _)| bytes[i..].starts_with(needle)))
}
fn lock(path: &Path) -> Result<File, String> {
	let f = options()
		.read(true)
		.write(true)
		.create(true)
		.open(path)
		.map_err(err)?;
	regular(&f, path)?;
	let mut announced = false;
	loop {
		commands::check()?;
		match f.try_lock() {
			Ok(()) => {
				commands::check()?;
				return Ok(f);
			}
			Err(std::fs::TryLockError::WouldBlock) => {
				if !announced {
					fault("waiting")?;
					announced = true;
				}
				std::thread::sleep(Duration::from_millis(10));
			}
			Err(e) => return Err(format!("cache lock {}: {e}", path.display())),
		}
	}
}
fn read_managed(path: &Path) -> Result<Vec<u8>, String> {
	let file = options()
		.read(true)
		.open(path)
		.map_err(|e| format!("cache file {}: {e}", path.display()))?;
	regular(&file, path)?;
	let mut bytes = Vec::new();
	file.take(input::DOCUMENT_LIMIT as u64 + 1)
		.read_to_end(&mut bytes)
		.map_err(err)?;
	if bytes.len() > input::DOCUMENT_LIMIT {
		return Err("cache document exceeds 16 MiB".into());
	}
	Ok(bytes)
}
fn sync_dir(p: &Path) -> Result<(), String> {
	File::open(p).and_then(|f| f.sync_all()).map_err(err)
}
fn write_new(p: &Path, bytes: &[u8]) -> Result<(), String> {
	let mut f = options()
		.write(true)
		.create_new(true)
		.open(p)
		.map_err(err)?;
	f.write_all(bytes).map_err(err)?;
	f.sync_all().map_err(err)
}
fn version(program: &str, arg: &str, cwd: &Path) -> Result<String, String> {
	let mut command = Command::new(program);
	command.arg(arg).current_dir(cwd);
	String::from_utf8(commands::run(command, true)?).map_err(err)
}
pub(crate) fn environment(identity: &impl Identity) -> Result<(), String> {
	let c = identity.context();
	for (key, _) in std::env::vars_os() {
		let s = key.to_string_lossy();
		if (s.starts_with("CARGO_") && s != "CARGO_HOME")
			|| (s.starts_with("RUST") && s != "RUSTUP_HOME" && s != "RUSTUP_TOOLCHAIN")
		{
			return Err(format!(
				"build-affecting environment variable {s} is unsupported"
			));
		}
	}
	let home = std::env::var_os("CARGO_HOME")
		.map(PathBuf::from)
		.or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
		.ok_or("Cargo home unavailable")?;
	let rustup = std::env::var_os("RUSTUP_HOME")
		.map(PathBuf::from)
		.map(|p| p.canonicalize().map_err(err))
		.transpose()?;
	if home.canonicalize().map_err(err)? != c.cargo_home
		|| rustup != c.rustup_home
		|| std::env::var("RUSTUP_TOOLCHAIN").ok() != c.rustup_toolchain
	{
		return Err("assembly toolchain environment changed; run lock".into());
	}
	if let Some(root) = std::env::var_os("RNX_PROJECT_CACHE") {
		let root = PathBuf::from(root);
		if !root.is_absolute() || root.to_str().is_none() {
			return Err("cache selection requires an absolute Unicode path".into());
		}
		if root.canonicalize().map_err(err)? != c.cache_root {
			return Err("cache root changed; run lock".into());
		}
	}
	if c.profile != "release" || c.features != ["project-sources"] {
		return Err("unsupported assembly build profile/features".into());
	}
	Ok(())
}
fn validate(identity: &impl Identity, stage: &Path, project: &Path) -> Result<(), String> {
	storage::guard(&identity.context().cache_root, stage, project)?;
	environment(identity)?;
	identity.revalidate(stage)?;
	let c = identity.context();
	let rustc = version("rustc", "-Vv", stage)?;
	let cargo = version("cargo", "-V", stage)?;
	if rustc != c.rustc
		|| cargo != c.cargo
		|| rustc.lines().find_map(|s| s.strip_prefix("host: ")) != Some(c.target.as_str())
	{
		return Err("assembly compiler identity changed; run lock".into());
	}
	Ok(())
}
/// Bounded readiness and fixed-path validation, also used by future launch code.
/// This never compiles, populates a directory, or repairs a published entry.
pub(crate) fn ready(identity: &impl Identity) -> Result<(PathBuf, String), String> {
	let root = &identity.context().cache_root;
	if storage::root(root)? != *root {
		return Err("locked cache root moved; run lock".into());
	}
	let entry = root.join("entries").join(identity.key());
	for p in [root.join("entries"), entry.clone(), entry.join("artifacts")] {
		storage::private_directory(&p)?;
	}
	let path = entry.join("ready.json");
	let bytes = read_managed(&path)
		.map_err(|e| format!("cache ready {}: {e}; run build", path.display()))?;
	let (artifact, digest) = decode_ready(&bytes, identity, &path)?;
	Ok((entry.join(artifact), digest))
}
fn decode_ready(
	bytes: &[u8],
	identity: &impl Identity,
	path: &Path,
) -> Result<(String, String), String> {
	let r: Ready = serde_json::from_slice(bytes)
		.map_err(|e| format!("cache ready {}: {e}", path.display()))?;
	if r.format != identity.ready_format()
		|| r.key != identity.key()
		|| r.identity.as_bytes() != identity.bytes()
		|| !artifact::digest_valid(&r.executable_blake3)
		|| r.artifact != format!("artifacts/{}", r.executable_blake3)
	{
		return Err(format!("cache ready binding invalid: {}", path.display()));
	}
	Ok((r.artifact, r.executable_blake3))
}
/// `project_check` rechecks the locked project identity without compiling or
/// publishing. It runs after waiting and after independent assembly publication.
/// Gate four supplies the product's lock/source verification and receipt writer.
pub(crate) fn acquire(
	identity: &impl Identity,
	cargo_lock: &[u8],
	project: &Path,
	offline: bool,
	mut project_check: impl FnMut() -> Result<(), String>,
) -> Result<Entry, String> {
	commands::check()?;
	identity.check_lock(cargo_lock)?;
	let root = &identity.context().cache_root;
	if storage::root(root)? != *root {
		return Err("locked cache root moved; run lock".into());
	}
	storage::directory(&root.join("locks"))?;
	storage::directory(&root.join("entries"))?;
	let guard = lock(&root.join("locks").join(format!("{}.lock", identity.key())))?;
	fault("locked")?;
	// Never infer readiness from the previous owner's release of the lock.
	project_check()?;
	let entry = root.join("entries").join(identity.key());
	storage::directory(&entry)?;
	let stage = entry.join("assembly");
	let hit = exists(&entry.join("ready.json"))?;
	if hit {
		storage::private_directory(&stage)?;
	} else {
		storage::directory(&stage)?;
	}
	validate(identity, &stage, project)?;
	let operation: Result<(artifact::Checked, String), String> = (|| {
		if !hit {
			// Only unpublished data may be discarded. remove_dir_all does not
			// follow a symlink, and the managed top-level paths are checked first.
			for name in ["assembly", "target", "artifacts"] {
				let p = entry.join(name);
				if exists(&p)? {
					storage::private_directory(&p)?;
					fs::remove_dir_all(&p).map_err(err)?;
				}
				storage::directory(&p)?;
			}
			{
				let p = entry.join("ready.new");
				if exists(&p)? {
					let f = options().read(true).open(&p).map_err(err)?;
					regular(&f, &p)?;
					fs::remove_file(&p).map_err(err)?;
				}
			}
			storage::directory(&stage.join("src"))?;
			let (manifest, main) = identity.wrapper();
			write_new(&stage.join("Cargo.toml"), manifest.as_bytes())?;
			write_new(&stage.join("src/main.rs"), main.as_bytes())?;
			write_new(&stage.join("Cargo.lock"), cargo_lock)?;
			sync_dir(&stage.join("src"))?;
			sync_dir(&stage)?;
			fault("before-build")?;
			validate(identity, &stage, project)?;
			// Record 0069: a declared assembly compiles in the root's shared
			// build directory, holding its coordination lock from here until
			// publication; the final artifact still lands in the entry.
			let shared = shared_build_directory(identity);
			let _build_lock = match &shared {
				Some(dir) => {
					storage::directory(&root.join("build"))?;
					storage::directory(dir)?;
					Some(lock_shared(&root.join("locks").join(format!(
						"build-{}.lock",
						dir.file_name().unwrap().to_string_lossy()
					)))?)
				}
				None => None,
			};
			let mut command = Command::new("cargo");
			command
				.current_dir(&stage)
				.args(["build", "--locked", "--release", "--target-dir"])
				.arg(entry.join("target"));
			if let Some(dir) = &shared {
				command.arg("--config").arg(format!(
					"build.build-dir={}",
					toml::Value::String(
						dir.to_str().ok_or("build directory is not Unicode")?.into()
					)
				));
			}
			if offline {
				command.arg("--offline");
			}
			#[cfg(unix)]
			{
				use std::os::unix::process::CommandExt;
				// Cargo creates descendants too. Set its mask in the child,
				// not the host, so a user's group-writable umask cannot make
				// tool-managed target directories fail the ownership policy.
				unsafe {
					command.pre_exec(|| {
						libc::umask(0o077);
						Ok(())
					});
				}
			}
			commands::run(command, false)?;
			fault("after-build")?;
			validate(identity, &stage, project)?;
			if read_managed(&stage.join("Cargo.lock"))? != cargo_lock
				|| read_managed(&stage.join("Cargo.toml"))? != manifest.as_bytes()
				|| read_managed(&stage.join("src/main.rs"))? != main.as_bytes()
			{
				return Err("generated assembly inputs changed during build".into());
			}
			// The executable carries the wrapper's package name: the placeholder
			// for retained generator-two and -three wrappers, the digest name
			// from generator four.
			let source = entry
				.join("target/release")
				.join(crate::generate::executable_name(manifest)?);
			storage::private_directory(&entry.join("target"))?;
			storage::private_directory(&entry.join("target/release"))?;
			if let Some(dir) = &shared
				&& references(&source, dir)?
			{
				return Err(format!(
					"shared build refused: the executable references the shared build directory {}; nothing was published. A native in this assembly reads retained build output at runtime; drop `shared_build = true` from its declaration ({}) and lock again",
					dir.display(),
					identity
						.natives()
						.into_iter()
						.filter(|n| n != "rnx")
						.collect::<Vec<_>>()
						.join(", ")
				));
			}
			let expected =
				fingerprint::one(&source, &mut fingerprint::Allowance::default())?.blake3;
			let temporary = entry.join("artifacts/executable.new");
			let mut from = options().read(true).open(&source).map_err(err)?;
			regular(&from, &source)?;
			let mut to = options()
				.write(true)
				.create_new(true)
				.open(&temporary)
				.map_err(err)?;
			let n = std::io::copy(&mut (&mut from).take(fingerprint::BYTES + 1), &mut to)
				.map_err(err)?;
			if n > fingerprint::BYTES {
				return Err("artifact exceeds byte allowance".into());
			}
			#[cfg(unix)]
			{
				use std::os::unix::fs::PermissionsExt;
				to.set_permissions(fs::Permissions::from_mode(0o700))
					.map_err(err)?;
			}
			to.sync_all().map_err(err)?;
			drop(to);
			artifact::check(&temporary, &expected, None, true)?;
			let installed = entry.join("artifacts").join(&expected);
			fs::rename(&temporary, &installed).map_err(err)?;
			sync_dir(&entry.join("artifacts"))?;
			artifact::check(&installed, &expected, None, true)?;
			fault("before-ready")?;
			validate(identity, &stage, project)?;
			let ready = Ready {
				format: identity.ready_format(),
				key: identity.key().into(),
				identity: String::from_utf8(identity.bytes().to_vec()).map_err(err)?,
				executable_blake3: expected.clone(),
				artifact: format!("artifacts/{expected}"),
			};
			write_new(&entry.join("ready.new"), &wire::encode(&ready)?)?;
			fault("ready-written")?;
			validate(identity, &stage, project)?;
			commands::check()?;
			fs::rename(entry.join("ready.new"), entry.join("ready.json")).map_err(err)?;
			sync_dir(&entry)?;
		}
		fault("after-ready")?;
		let (path, digest) = ready(identity)?;
		let artifact = artifact::check(&path, &digest, None, true)?;
		fault("before-attach")?;
		validate(identity, &stage, project)?;
		project_check()?;
		artifact.recheck()?;
		commands::check()?;
		Ok((artifact, digest))
	})();
	match operation {
		Ok((artifact, digest)) => Ok(Entry {
			_lock: guard,
			artifact,
			digest,
			hit,
		}),
		Err(e) => {
			// Do not modify a published entry, including its diagnostics.
			if !hit
				&& !exists(&entry.join("ready.json")).unwrap_or(true)
				&& let Ok(mut f) = options()
					.write(true)
					.create(true)
					.truncate(true)
					.open(entry.join("failure.txt"))
			{
				let _ = f.write_all(e.as_bytes().get(..e.len().min(8192)).unwrap_or_default());
			}
			Err(format!("cache entry {}: {e}", entry.display()))
		}
	}
}
fn fault(name: &str) -> Result<(), String> {
	#[cfg(feature = "test-support")]
	{
		if std::env::var("RNX_CACHE_FAIL").ok().as_deref() == Some(name) {
			return Err(format!("injected failure at {name}"));
		}
		if std::env::var("RNX_CACHE_PAUSE").ok().as_deref() == Some(name) {
			let p =
				PathBuf::from(std::env::var_os("RNX_CACHE_MARKER").ok_or("pause marker missing")?);
			if !p.exists() {
				fs::write(&p, name).map_err(err)?;
			}
			while p.exists() {
				commands::check()?;
				std::thread::sleep(Duration::from_millis(10));
			}
		}
	}
	let _ = name;
	Ok(())
}

#[cfg(test)]
mod encoding_tests {
	use super::*;
	#[test]
	fn ready_format_and_artifact_binding_are_strict() {
		let id = crate::cache_identity::fixture_identity();
		let digest = blake3::hash(b"artifact").to_hex().to_string();
		let doc = serde_json::json!({"format":2,"key":id.key(),
            "identity":String::from_utf8(id.bytes().to_vec()).unwrap(),
            "executable_blake3":digest,"artifact":format!("artifacts/{digest}")});
		let path = Path::new("/fixture/ready.json");
		assert!(decode_ready(&wire::encode(&doc).unwrap(), &id, path).is_ok());
		for (key, value) in [
			("format", serde_json::json!(1)),
			("format", serde_json::json!(99)),
			("key", serde_json::json!("a".repeat(64))),
			("identity", serde_json::json!("{}")),
			("executable_blake3", serde_json::json!("b".repeat(64))),
			("artifact", serde_json::json!("../artifact")),
			("executable_sha256", serde_json::json!(digest)),
			("unknown", serde_json::json!(true)),
		] {
			let mut bad = doc.clone();
			bad[key] = value;
			assert!(
				decode_ready(&wire::encode(&bad).unwrap(), &id, path).is_err(),
				"{key}"
			);
		}
	}
}

#[cfg(test)]
mod shared_build_tests {
	use super::*;
	fn temp(tag: &str) -> PathBuf {
		let d = std::env::temp_dir().join(format!("rnx-shared-build-{}-{tag}", std::process::id()));
		let _ = fs::remove_dir_all(&d);
		fs::create_dir_all(&d).unwrap();
		d
	}
	/// Written as the tool writes its own files: private, so `regular` admits them.
	fn write(path: &Path, bytes: &[u8]) {
		let _ = fs::remove_file(path);
		let mut f = options().write(true).create_new(true).open(path).unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			f.set_permissions(fs::Permissions::from_mode(0o600))
				.unwrap();
		}
		f.write_all(bytes).unwrap();
	}
	/// The scan finds the directory's path anywhere in the executable and
	/// nothing else; a prefix or a different directory is not a reference.
	#[test]
	fn the_scan_finds_the_shared_directory_path_and_only_that() {
		let d = temp("scan");
		let dir = d.join("build").join("k");
		let exe = d.join("exe");
		write(
			&exe,
			&[
				b"ELF junk ".as_slice(),
				dir.as_os_str().as_encoded_bytes(),
				b"/out/retained.txt\0more",
			]
			.concat(),
		);
		assert!(references(&exe, &dir).unwrap());
		write(
			&exe,
			&[
				b"ELF junk ".as_slice(),
				d.join("build").join("j").as_os_str().as_encoded_bytes(),
				b"\0",
			]
			.concat(),
		);
		assert!(!references(&exe, &dir).unwrap());
		write(&exe, b"no path at all");
		assert!(!references(&exe, &dir).unwrap());
		// The needle's first byte appearing alone does not match.
		write(&exe, b"/////");
		assert!(!references(&exe, &dir).unwrap());
	}
	/// A builder's shared hold blocks an exclusive taker, and two builders
	/// hold together; releasing the last holder frees the exclusive taker.
	#[test]
	fn builders_share_the_coordination_lock_and_exclude_removal() {
		let d = temp("lock");
		let path = d.join("build-k.lock");
		let a = lock_shared(&path).unwrap();
		let b = lock_shared(&path).unwrap();
		let remover = options().read(true).write(true).open(&path).unwrap();
		assert!(matches!(
			remover.try_lock(),
			Err(std::fs::TryLockError::WouldBlock)
		));
		drop(a);
		assert!(matches!(
			remover.try_lock(),
			Err(std::fs::TryLockError::WouldBlock)
		));
		drop(b);
		assert!(remover.try_lock().is_ok());
	}
	#[test]
	fn the_shared_directory_follows_the_recorded_build_kind() {
		let private = crate::new_identity::fixture(crate::cache_identity::Build::Private);
		assert!(shared_build_directory(&private).is_none());
		let key = "e".repeat(64);
		let shared =
			crate::new_identity::fixture(crate::cache_identity::Build::Shared { key: key.clone() });
		assert_eq!(
			shared_build_directory(&shared).unwrap(),
			shared.context().cache_root.join("build").join(key)
		);
		let retained = crate::cache_identity::fixture_identity();
		assert!(shared_build_directory(&retained).is_none());
	}
}
