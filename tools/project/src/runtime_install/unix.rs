use super::{
	git, hooks, layout,
	storage::{self, err},
};
use crate::{commands, fingerprint};
use serde::{Deserialize, Serialize};

use std::{
	ffi::OsString,
	fs,
	io::{Read, Write},
	os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
	path::{Path, PathBuf},
	time::{SystemTime, UNIX_EPOCH},
};
const DOCUMENT: usize = 65536;
fn hash(b: &[u8]) -> String {
	blake3::hash(b).to_hex().to_string()
}
fn digest(s: &str) -> bool {
	s.len() == 64
		&& s.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn identity(tree: &str) -> String {
	hash(format!("rnx-installed-runtime-v2\n{tree}\n").as_bytes())
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Installation {
	pub format: u32,
	pub id: String,
	pub tree_blake3: String,
	pub files: u64,
	pub bytes: u64,
	pub layout: String,
	pub source_path: PathBuf,
	pub source_commit: Option<String>,
	pub dirty_tracked: bool,
	pub installed_utc: String,
	pub tool_version: String,
	pub tool_blake3: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub migrated_from: Option<Migration>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Migration {
	pub format: u32,
	pub id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Current {
	format: u32,
	id: String,
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
	let mut b = serde_json::to_vec_pretty(v).map_err(err)?;
	b.push(b'\n');
	if b.len() > DOCUMENT {
		return Err("runtime document exceeds 65536 bytes".into());
	}
	Ok(b)
}
fn utc() -> Result<String, String> {
	let seconds = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_err(err)?
		.as_secs();
	let seconds: libc::time_t = seconds.try_into().map_err(err)?;
	// gmtime_r writes only this caller-owned tm and consults no local timezone.
	let mut tm: libc::tm = unsafe { std::mem::zeroed() };
	if unsafe { libc::gmtime_r(&seconds, &mut tm) }.is_null() {
		return Err("cannot represent installation UTC time".into());
	}
	Ok(format!(
		"{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
		tm.tm_year + 1900,
		tm.tm_mon + 1,
		tm.tm_mday,
		tm.tm_hour,
		tm.tm_min,
		tm.tm_sec
	))
}
fn timestamp(s: &str) -> bool {
	let b = s.as_bytes();
	if b.len() != 20 {
		return false;
	}
	for (i, c) in b.iter().enumerate() {
		if let Some(expected) = match i {
			4 | 7 => Some(b'-'),
			10 => Some(b'T'),
			13 | 16 => Some(b':'),
			19 => Some(b'Z'),
			_ => None,
		} {
			if *c != expected {
				return false;
			}
		} else if !c.is_ascii_digit() {
			return false;
		}
	}
	let n = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(u32::MAX);
	let y = n(0..4);
	let m = n(5..7);
	let days = match m {
		1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
		4 | 6 | 9 | 11 => 30,
		2 => {
			if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
				29
			} else {
				28
			}
		}
		_ => 0,
	};
	(1..=days).contains(&n(8..10)) && n(11..13) < 24 && n(14..16) < 60 && n(17..19) < 60
}
fn metadata(entry: &Path, id: &str) -> Result<Installation, String> {
	if !digest(id) {
		return Err("installation ID must be a full lowercase BLAKE3 digest".into());
	}
	storage::dir(entry)?;
	let p = entry.join("installation.json");
	decode_installation(&storage::read(&p, DOCUMENT)?, id, &p)
}
fn decode_installation(bytes: &[u8], id: &str, p: &Path) -> Result<Installation, String> {
	let d: Installation =
		serde_json::from_slice(bytes).map_err(|e| format!("{}: {e}", p.display()))?;
	if d.format != 2
		|| d.layout != "rnx-shipped-v1"
		|| d.id != id
		|| !digest(&d.tree_blake3)
		|| identity(&d.tree_blake3) != d.id
		|| !digest(&d.tool_blake3)
		|| d.files == 0
		|| d.files > 100000
		|| d.bytes > 512 * 1024 * 1024
		|| !d.source_path.is_absolute()
		|| d.source_path.to_str().is_none()
		|| d.migrated_from
			.as_ref()
			.is_some_and(|m| m.format != 1 || !digest(&m.id))
		|| d.tool_version.is_empty()
		|| !timestamp(&d.installed_utc)
		|| d.source_commit.as_ref().is_some_and(|c| {
			!matches!(c.len(), 40 | 64) || !c.bytes().all(|b| b.is_ascii_hexdigit())
		}) || (d.source_commit.is_none() && !d.dirty_tracked)
	{
		return Err(format!(
			"invalid runtime installation document: {}",
			p.display()
		));
	}
	Ok(d)
}
fn validate(root: &Path, id: &str) -> Result<Installation, String> {
	validate_inner(root, id, false)
}
fn validate_inner(root: &Path, id: &str, repair: bool) -> Result<Installation, String> {
	storage::dir(root)?;
	storage::dir(&root.join("entries"))?;
	let entry = root.join("entries").join(id);
	let d = metadata(&entry, id)?;
	let admin = entry.join("source/.git");
	storage::walk_admin(&entry, false, repair.then_some(admin.as_path()))?;
	for e in fs::read_dir(&entry).map_err(err)? {
		let e = e.map_err(err)?;
		if e.file_name() != "source" && e.file_name() != "installation.json" {
			return Err("unexpected installed entry member".into());
		}
	}
	let source = entry.join("source");
	git::administration_drift(&source, repair)?;
	let t = git::inventory(&source)?;
	layout::validate(&source, &t)?;
	if t.blake3 != d.tree_blake3
		|| t.files.len() as u64 != d.files
		|| t.files.iter().map(|f| f.bytes).sum::<u64>() != d.bytes
	{
		return Err(format!(
			"corrupt installed runtime {id}: source fingerprint differs"
		));
	}
	if repair {
		git::tighten(&source)?;
		storage::walk(&entry, false)?;
	}
	Ok(d)
}
const INSTALL_HELP: &str = "no runtime is installed; run rnx-project runtime install --from /path/to/rnx; or open an existing project with rnx-project session --manifest /path/to/rnx.toml";

fn current() -> Result<(PathBuf, Installation), String> {
	let root = storage::selected()?;
	if !storage::exists(&root)? {
		return Err(INSTALL_HELP.into());
	}
	storage::dir(&root)?;
	if !storage::exists(&root.join("current.json"))? {
		return Err(INSTALL_HELP.into());
	}
	let c = decode_current(&storage::read(&root.join("current.json"), DOCUMENT)?)?;
	storage::dir(&root.join("entries"))?;
	let entry = root.join("entries").join(&c.id);
	if !storage::exists(&entry)? {
		return Err(format!(
			"selected runtime installation {} is missing; run rnx-project runtime install --from /path/to/rnx",
			c.id
		));
	}
	let d = metadata(&entry, &c.id)?;
	storage::dir(&entry.join("source"))?;
	Ok((root, d))
}

/// A description-time selection. No Git, writer lock, or mutation here.
pub(crate) struct Selection {
	pub source: PathBuf,
	pub notice: String,
	installed: Option<(PathBuf, Installation)>,
}
impl Selection {
	pub(crate) fn discover() -> Result<Self, String> {
		if let Some(path) = std::env::var_os("RNX_DEP_RUNTIME") {
			let path = PathBuf::from(path);
			if !path.is_absolute() || path.to_str().is_none() {
				return Err("RNX_DEP_RUNTIME must be an absolute Unicode path; export RNX_DEP_RUNTIME=/absolute/path/to/rnx".into());
			}
			let source = path
				.canonicalize()
				.map_err(|e| format!("RNX_DEP_RUNTIME override {}: {e}", path.display()))?;
			return Ok(Self {
				notice: format!("Runtime: override {}", source.display()),
				source,
				installed: None,
			});
		}
		let (root, d) = current()?;
		let source = root.join("entries").join(&d.id).join("source");
		let notice = format!(
			"Runtime: installation {} at {} (installed from {}, {}, {}, {})",
			d.id,
			source.display(),
			d.source_path.display(),
			d.source_commit.as_deref().unwrap_or("no source commit"),
			if d.dirty_tracked {
				"dirty tracked snapshot"
			} else {
				"clean tracked snapshot"
			},
			d.installed_utc
		);
		Ok(Self {
			source,
			notice,
			installed: Some((root, d)),
		})
	}
	pub(crate) fn validate(&self) -> Result<(), String> {
		if let Some((root, expected)) = &self.installed {
			let found = validate(root, &expected.id)?;
			if &found != expected {
				return Err("runtime installation changed; request consent again".into());
			}
		}
		Ok(())
	}
}
struct Temp(PathBuf);
impl Drop for Temp {
	fn drop(&mut self) {
		let _ = if self.0.is_dir() {
			fs::remove_dir_all(&self.0)
		} else {
			fs::remove_file(&self.0)
		};
	}
}
fn copy(source: &Path, dest: &Path, tree: &fingerprint::Tree) -> Result<(), String> {
	storage::mkdir(dest)?;
	let mut total = 0u64;
	let mut entries = 1u64;
	for file in &tree.files {
		commands::check()?;
		let mut source_parent = source.to_owned();
		let mut dest_parent = dest.to_owned();
		let path = Path::new(&file.path);
		for c in path.parent().unwrap().components() {
			source_parent.push(c);
			if !fs::symlink_metadata(&source_parent).map_err(err)?.is_dir() {
				return Err("source directory changed".into());
			}
			dest_parent.push(c);
			if !storage::exists(&dest_parent)? {
				entries += 1;
				if entries > hooks::limit("RNX_INSTALL_RETAINED_FILES", 400000) {
					return Err("installed copy exceeds 400000 entries".into());
				}
			}
			storage::mkdir(&dest_parent)?;
		}
		let from = source.join(path);
		let to = dest.join(path);
		let mut f = storage::options()
			.read(true)
			.open(&from)
			.map_err(|e| format!("{}: {e}", from.display()))?;
		let before = f.metadata().map_err(err)?;
		if !before.is_file()
			|| before.len() != file.bytes
			|| (before.mode() & 0o111 != 0) != file.executable
		{
			return Err(format!("source changed: {}", from.display()));
		}
		entries += 1;
		if entries > hooks::limit("RNX_INSTALL_RETAINED_FILES", 400000) {
			return Err("installed copy exceeds 400000 entries".into());
		}
		let mut out = storage::options()
			.create_new(true)
			.write(true)
			.open(&to)
			.map_err(err)?;
		out.set_permissions(fs::Permissions::from_mode(if file.executable {
			0o700
		} else {
			0o600
		}))
		.map_err(err)?;
		let mut content = blake3::Hasher::new();
		let mut n = 0u64;
		let mut buffer = [0u8; 16384];
		loop {
			hooks::point("copy-chunk")?;
			let remaining = (file.bytes - n).min(
				hooks::limit("RNX_INSTALL_COPY_BYTES", 512 * 1024 * 1024).saturating_sub(total),
			);
			let take = buffer.len().min((remaining + 1) as usize);
			let count = f.read(&mut buffer[..take]).map_err(err)?;
			hooks::read(&from, count)?;
			if count == 0 {
				break;
			}
			n += count as u64;
			if n > file.bytes {
				return Err(format!("source grew during copy: {}", from.display()));
			}
			total += count as u64;
			if total > hooks::limit("RNX_INSTALL_COPY_BYTES", 512 * 1024 * 1024) {
				return Err("copy exceeds source byte allowance".into());
			}
			content.update(&buffer[..count]);
			out.write_all(&buffer[..count]).map_err(err)?;
		}
		let after = f.metadata().map_err(err)?;
		if n != file.bytes
			|| content.finalize().to_hex().to_string() != file.blake3
			|| after.len() != before.len()
			|| after.mode() & 0o111 != before.mode() & 0o111
		{
			return Err(format!("source changed during copy: {}", from.display()));
		}
		out.sync_all().map_err(err)?;
	}
	Ok(())
}
fn select_inner(root: &Path, id: &str, selected: &mut bool) -> Result<(), String> {
	validate_inner(root, id, true)?;
	// A corrupt current document may be replaced explicitly, but never a special file.
	if storage::exists(&root.join("current.json"))? {
		storage::read(&root.join("current.json"), DOCUMENT)?;
	}
	hooks::point("before-current-write")?;
	let tmp = Temp(storage::unique(root, "current")?);
	storage::write(
		&tmp.0,
		&json(&Current {
			format: 2,
			id: id.into(),
		})?,
	)?;
	hooks::point("after-current-write")?;
	hooks::point("before-current-rename")?;
	fs::rename(&tmp.0, root.join("current.json")).map_err(err)?;
	*selected = true;
	hooks::point("after-current-rename")?;
	storage::sync(root)?;
	hooks::point("after-current-sync")?;
	Ok(())
}
fn install(source: &Path) -> Result<Installation, String> {
	let source = source
		.canonicalize()
		.map_err(|e| format!("runtime source {}: {e}", source.display()))?;
	let root = storage::selected()?;
	let own_installation = source.strip_prefix(&root).ok().is_some_and(|p| {
		let parts = p.iter().collect::<Vec<_>>();
		parts.len() == 3
			&& parts[0] == "entries"
			&& parts[2] == "source"
			&& parts[1].to_str().is_some_and(digest)
	});
	if root.starts_with(&source) || (source.starts_with(&root) && !own_installation) {
		return Err("runtime source and store must not contain one another".into());
	}
	git::top(&source)?;
	let before = git::inventory(&source)?;
	layout::validate(&source, &before)?;
	let id = identity(&before.blake3);
	if own_installation && source != root.join("entries").join(&id).join("source") {
		return Err("installed source does not match its containing identity".into());
	}
	storage::create_root(&root)?;
	let _lock = storage::lock(&root)?;
	storage::reclaim(&root)?;
	storage::mkdir(&root.join("entries"))?;
	if git::inventory(&source)? != before {
		return Err("source changed while waiting for installation lock".into());
	}
	let entry = root.join("entries").join(&id);
	let mut published = false;
	let mut selected = false;
	let result = (|| {
		let doc = if storage::exists(&entry)? {
			published = true;
			validate_inner(&root, &id, true)?
		} else {
			let temp = Temp(storage::unique(&root, "stage")?);
			fs::DirBuilder::new()
				.mode(0o700)
				.create(&temp.0)
				.map_err(err)?;
			hooks::point("after-snapshot")?;
			copy(&source, &temp.0.join("source"), &before)?;
			hooks::point("after-copy")?;
			git::index(&temp.0.join("source"), &before)?;
			hooks::point("after-index")?;
			let copied = git::inventory(&temp.0.join("source"))?;
			if copied.blake3 != before.blake3 || copied.files != before.files {
				return Err("installed copy differs from source fingerprint".into());
			}
			layout::validate(&temp.0.join("source"), &copied)?;
			git::administration(&temp.0.join("source"))?;
			let (source_commit, dirty_tracked) = git::provenance(&source, &before)?;
			if git::inventory(&source)? != before {
				return Err("source changed during installation".into());
			}
			let doc = Installation {
				format: 2,
				id: id.clone(),
				tree_blake3: before.blake3.clone(),
				files: before.files.len() as u64,
				bytes: before.files.iter().map(|f| f.bytes).sum(),
				layout: "rnx-shipped-v1".into(),
				source_path: source.clone(),
				source_commit,
				dirty_tracked,
				installed_utc: utc()?,
				migrated_from: None,
				tool_version: env!("CARGO_PKG_VERSION").into(),
				tool_blake3: fingerprint::one(
					&std::env::current_exe().map_err(err)?,
					&mut fingerprint::Allowance::default(),
				)?
				.blake3,
			};
			hooks::point("before-document")?;
			storage::write(&temp.0.join("installation.json"), &json(&doc)?)?;
			hooks::point("after-document")?;
			storage::walk(&temp.0, true)?;
			hooks::point("before-entry-rename")?;
			fs::rename(&temp.0, &entry).map_err(err)?;
			published = true;
			hooks::point("after-entry-rename")?;
			storage::sync(&root.join("entries"))?;
			hooks::point("after-entry-sync")?;
			validate(&root, &id)?;
			doc
		};
		if git::inventory(&source)? != before {
			return Err("source changed before selection".into());
		}
		select_inner(&root, &id, &mut selected)?;
		Ok(doc)
	})();
	result.map_err(|e| {
		if selected {
			format!("runtime {id}: selection may already have changed: {e}")
		} else if published {
			format!("runtime {id} installed but not selected: {e}")
		} else {
			e
		}
	})
}
pub(crate) fn cli(args: &[OsString]) -> Result<(), String> {
	let verb = args
		.first()
		.and_then(|v| v.to_str())
		.ok_or("expected runtime install --from PATH, show, or select ID")?;
	match verb {
		"install" if args.len() == 3 && args[1] == "--from" => {
			commands::install_signals()?;
			let d = install(Path::new(&args[2]))?;
			println!(
				"Installed and selected runtime {}\nSource: {}\nSnapshot: {} files, {} bytes\nAssembly requires Git, Cargo/Rust and native build tools.",
				d.id,
				storage::selected()?
					.join("entries")
					.join(&d.id)
					.join("source")
					.display(),
				d.files,
				d.bytes
			);
			Ok(())
		}
		"select" if args.len() == 2 => {
			let id = args[1]
				.to_str()
				.filter(|s| digest(s))
				.ok_or("installation ID must be a full lowercase BLAKE3 digest")?;
			commands::install_signals()?;
			let root = storage::selected()?;
			if !storage::exists(&root.join("entries").join(id))? {
				return Err(format!("unknown installation ID: {id}"));
			}
			storage::dir(&root)?;
			let _lock = storage::lock(&root)?;
			storage::reclaim(&root)?;
			let mut selected = false;
			select_inner(&root, id, &mut selected).map_err(|e| {
				if selected {
					format!("selection may already have changed: {e}")
				} else {
					e
				}
			})?;
			println!("Selected runtime {id}");
			Ok(())
		}
		"show" if args.len() == 1 => {
			let (root, d) = current()?;
			println!(
				"Selected runtime {}\nSource: {}",
				d.id,
				root.join("entries").join(&d.id).join("source").display()
			);
			if let Some(p) = std::env::var_os("RNX_DEP_RUNTIME") {
				println!("RNX_DEP_RUNTIME override: {:?}", p);
			}
			Ok(())
		}
		_ => Err("expected runtime install --from PATH, show, or select ID".into()),
	}
}

fn decode_current(bytes: &[u8]) -> Result<Current, String> {
	let c: Current = serde_json::from_slice(bytes).map_err(err)?;
	if c.format != 2 || !digest(&c.id) {
		return Err("invalid current runtime document".into());
	}
	Ok(c)
}
#[cfg(test)]
mod encoding_tests {
	use super::*;
	#[test]
	fn installation_and_selection_formats_are_strict() {
		let tree = blake3::hash(b"tree").to_hex().to_string();
		let id = identity(&tree);
		let doc = serde_json::json!({"format":2,"id":id,"tree_blake3":tree,
            "files":1,"bytes":1,"layout":"rnx-shipped-v1","source_path":"/fixture/source",
            "source_commit":null,"dirty_tracked":true,"installed_utc":"2026-09-18T00:00:00Z",
            "tool_version":"fixture","tool_blake3":blake3::hash(b"tool").to_hex().to_string()});
		let path = Path::new("/fixture/installation.json");
		assert!(decode_installation(&json(&doc).unwrap(), &id, path).is_ok());
		for (key, value) in [
			("format", serde_json::json!(1)),
			("format", serde_json::json!(99)),
			("tree_sha256", serde_json::json!("a".repeat(64))),
			("tree_blake3", serde_json::json!("a".repeat(64))),
			("tool_blake3", serde_json::json!("bad")),
			("unknown", serde_json::json!(true)),
		] {
			let mut bad = doc.clone();
			bad[key] = value;
			assert!(
				decode_installation(&json(&bad).unwrap(), &id, path).is_err(),
				"{key}"
			);
		}
		let mut migrated = doc.clone();
		migrated["migrated_from"] = serde_json::json!({"format":1,"id":"a".repeat(64)});
		assert!(decode_installation(&json(&migrated).unwrap(), &id, path).is_ok());
		migrated["migrated_from"]["format"] = serde_json::json!(2);
		assert!(decode_installation(&json(&migrated).unwrap(), &id, path).is_err());
		let current = serde_json::json!({"format":2,"id":id});
		assert!(decode_current(&json(&current).unwrap()).is_ok());
		for (key, value) in [
			("format", serde_json::json!(1)),
			("format", serde_json::json!(99)),
			("id", serde_json::json!("../x")),
			("unknown", serde_json::json!(true)),
		] {
			let mut bad = current.clone();
			bad[key] = value;
			assert!(decode_current(&json(&bad).unwrap()).is_err());
		}
	}
}
