//! Shared-entry publication. Gate-three staging: receipt/CLI selection is gate four.
//! Callers hold their project lock first and keep the returned entry lease until
//! their receipt is published. Readiness is always re-read after acquiring a key.
#![allow(dead_code)]
use crate::{
	artifact, cache_identity::Identity, cache_storage as storage, commands, fingerprint, input,
	wire,
};
use serde::{Deserialize, Serialize};
use std::{
	fs::{self, File, OpenOptions},
	io::{Read, Write},
	path::{Path, PathBuf},
	process::Command,
	time::Duration,
};
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
pub(crate) fn environment(identity: &Identity) -> Result<(), String> {
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
fn validate(identity: &Identity, stage: &Path, project: &Path) -> Result<(), String> {
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
pub(crate) fn ready(identity: &Identity) -> Result<(PathBuf, String), String> {
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
	identity: &Identity,
	path: &Path,
) -> Result<(String, String), String> {
	let r: Ready = serde_json::from_slice(bytes)
		.map_err(|e| format!("cache ready {}: {e}", path.display()))?;
	if r.format != 2
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
	identity: &Identity,
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
			let mut command = Command::new("cargo");
			command
				.current_dir(&stage)
				.args(["build", "--locked", "--release", "--target-dir"])
				.arg(entry.join("target"));
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
			let source = entry.join("target/release/rnx-project-app");
			storage::private_directory(&entry.join("target"))?;
			storage::private_directory(&entry.join("target/release"))?;
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
				format: 2,
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
