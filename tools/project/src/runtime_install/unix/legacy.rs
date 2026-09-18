//! Authentication for explicit imports of retained installation-1 sources only.
use super::{DOCUMENT, digest, err, git, hooks, layout, storage, timestamp};
use crate::{commands, fingerprint};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
	fs,
	path::{Path, PathBuf},
};
fn identity(tree: &str) -> String {
	format!(
		"{:x}",
		Sha256::digest(format!("rnx-installed-runtime-v1\n{tree}\n").as_bytes())
	)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Installation {
	pub format: u32,
	pub id: String,
	pub tree_sha256: String,
	pub files: u64,
	pub bytes: u64,
	pub layout: String,
	pub source_path: PathBuf,
	pub source_commit: Option<String>,
	pub dirty_tracked: bool,
	pub installed_utc: String,
	pub tool_version: String,
	pub tool_sha256: String,
}

pub(super) fn decode(bytes: &[u8], id: &str, path: &Path) -> Result<Installation, String> {
	let d: Installation =
		serde_json::from_slice(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
	if d.format != 1
		|| d.layout != "rnx-shipped-v1"
		|| d.id != id
		|| !digest(&d.tree_sha256)
		|| identity(&d.tree_sha256) != d.id
		|| !digest(&d.tool_sha256)
		|| d.files == 0
		|| d.files > 100000
		|| d.bytes > 512 * 1024 * 1024
		|| !d.source_path.is_absolute()
		|| d.source_path.to_str().is_none()
		|| d.tool_version.is_empty()
		|| !timestamp(&d.installed_utc)
		|| d.source_commit.as_ref().is_some_and(|c| {
			!matches!(c.len(), 40 | 64) || !c.bytes().all(|b| b.is_ascii_hexdigit())
		}) || (d.source_commit.is_none() && !d.dirty_tracked)
	{
		return Err(format!(
			"invalid runtime installation document: {}",
			path.display()
		));
	}
	Ok(d)
}
pub(super) fn is_old(bytes: &[u8]) -> bool {
	#[derive(Deserialize)]
	struct Envelope {
		format: u32,
	}
	serde_json::from_slice::<Envelope>(bytes).is_ok_and(|v| v.format == 1)
}
pub(super) fn recovery(root: &Path, id: &str) -> String {
	let source = root.join("entries").join(id).join("source");
	let quote = |path: &Path| {
		path.to_str()
			.map(|s| format!("'{}'", s.replace('\'', "'\\''")))
			.ok_or("non-Unicode recovery path".to_string())
	};
	let command = (|| {
		let tool = std::env::current_exe().map_err(err)?;
		Ok::<_, String>(format!(
			"{} runtime install --from {}",
			quote(&tool)?,
			quote(&source)?
		))
	})();
	format!(
		"runtime installation {id} uses format 1; authenticate and reinstall with:\n{}\nOld runtime and assembly entries are retained; migrating reclaims no disk space.",
		command.unwrap_or_else(|e| format!("cannot render recovery command: {e}"))
	)
}
pub(super) fn authenticate(root: &Path, id: &str, repair: bool) -> Result<Installation, String> {
	storage::dir(root)?;
	storage::dir(&root.join("entries"))?;
	let entry = root.join("entries").join(id);
	storage::dir(&entry)?;
	let path = entry.join("installation.json");
	let doc = decode(&storage::read(&path, DOCUMENT)?, id, &path)?;
	let source = entry.join("source");
	let admin = source.join(".git");
	storage::walk_admin(&entry, false, repair.then_some(admin.as_path()))?;
	for item in fs::read_dir(&entry).map_err(err)? {
		let item = item.map_err(err)?;
		if item.file_name() != "source" && item.file_name() != "installation.json" {
			return Err("unexpected installed entry member".into());
		}
	}
	git::administration_drift(&source, repair)?;
	let tree = fingerprint::legacy::native_using(
		&source,
		&mut fingerprint::legacy::Allowance::bounded(
			hooks::limit("RNX_INSTALL_SOURCE_FILES", 100000) as usize,
			hooks::limit("RNX_INSTALL_SOURCE_BYTES", 512 * 1024 * 1024),
		),
		|root, args, limit| {
			let mut c = git::command(root);
			c.args(args);
			let b = git::run(c)?;
			if b.len() > limit {
				return Err("Git inventory output exceeds allowance".into());
			}
			Ok(b)
		},
	)?;
	if tree.sha256 != doc.tree_sha256
		|| tree.files.len() as u64 != doc.files
		|| tree.files.iter().map(|f| f.bytes).sum::<u64>() != doc.bytes
	{
		return Err(format!(
			"cannot authenticate legacy runtime {id}: source fingerprint differs"
		));
	}
	let current = git::inventory(&source)?;
	layout::validate(&source, &current)?;
	if repair {
		git::tighten(&source)?;
		storage::walk(&entry, false)?;
	}
	commands::check()?;
	Ok(doc)
}
