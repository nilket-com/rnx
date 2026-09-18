//! Frozen v1 bounded reader, private to installation migration and conformance tests.
#![allow(dead_code)] // Only migration and conformance targets use this frozen reader.
mod wire {
	#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
	#[serde(deny_unknown_fields)]
	pub struct File {
		pub path: String,
		pub executable: bool,
		pub bytes: u64,
		pub sha256: String,
	}
}
use sha2::{Digest, Sha256};
use std::{
	collections::BTreeMap,
	fs,
	io::Read,
	path::{Path, PathBuf},
	process::{Command, Stdio},
};

pub(crate) const ENTRIES: usize = 100_000;
pub(crate) const BYTES: u64 = 512 * 1024 * 1024;
#[derive(Debug)]
pub(crate) struct Allowance {
	entries: usize,
	bytes: u64,
}
impl Default for Allowance {
	fn default() -> Self {
		Self {
			entries: ENTRIES,
			bytes: BYTES,
		}
	}
}
impl Allowance {
	/// Installer test allowances can be reduced without changing production bounds.
	pub(crate) fn bounded(entries: usize, bytes: u64) -> Self {
		Self {
			entries: entries.min(ENTRIES),
			bytes: bytes.min(BYTES),
		}
	}

	pub(crate) fn remaining_bytes(&self) -> u64 {
		self.bytes
	}
	pub(crate) fn charge_bytes(&mut self, n: u64) -> Result<(), String> {
		self.bytes = self
			.bytes
			.checked_sub(n)
			.ok_or("fingerprint exceeds byte allowance")?;
		Ok(())
	}

	fn entry(&mut self) -> Result<(), String> {
		self.entries = self
			.entries
			.checked_sub(1)
			.ok_or("fingerprint exceeds entry allowance")?;
		Ok(())
	}
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tree {
	pub root: PathBuf,
	pub sha256: String,
	pub files: Vec<wire::File>,
}
fn fail(path: &Path, e: impl std::fmt::Display) -> String {
	format!("fingerprint {}: {e}", path.display())
}
fn text(path: &Path) -> Result<String, String> {
	let mut parts = vec![];
	for c in path.components() {
		let std::path::Component::Normal(p) = c else {
			return Err(fail(path, "expected relative normal path"));
		};
		let p = p.to_str().ok_or_else(|| fail(path, "non-Unicode name"))?;
		if p.contains('\\') {
			return Err(fail(path, "backslash in inventory name is unsupported"));
		}
		parts.push(p);
	}
	if parts.is_empty() {
		return Err(fail(path, "empty inventory path"));
	}
	Ok(parts.join("/"))
}
fn regular(path: &Path) -> Result<fs::Metadata, String> {
	let m = fs::symlink_metadata(path).map_err(|e| fail(path, e))?;
	if !m.is_file() {
		return Err(fail(
			path,
			"not a regular file (symlinks and special files are refused)",
		));
	}
	Ok(m)
}
fn executable(m: &fs::Metadata) -> bool {
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		m.permissions().mode() & 0o111 != 0
	}
	#[cfg(not(unix))]
	{
		let _ = m;
		false
	}
}
fn components_no_links(root: &Path, relative: &Path) -> Result<(), String> {
	let mut path = root.to_owned();
	for c in relative.components() {
		path.push(c);
		if fs::symlink_metadata(&path)
			.map_err(|e| fail(&path, e))?
			.file_type()
			.is_symlink()
		{
			return Err(fail(&path, "symlink refused"));
		}
	}
	Ok(())
}
fn hash_files(root: &Path, paths: Vec<PathBuf>, allowance: &mut Allowance) -> Result<Tree, String> {
	let mut ordered = BTreeMap::new();
	for path in paths {
		let key = text(&path)?;
		if ordered.insert(key, path).is_some() {
			return Err("duplicate inventory path".into());
		}
	}
	let mut tree = Sha256::new();
	tree.update(b"rnx-tree-v1\0");
	let mut files = vec![];
	for (name, relative) in ordered {
		files.push(hash_file(root, relative, name, allowance, Some(&mut tree))?.0);
	}
	Ok(Tree {
		root: root.to_owned(),
		sha256: format!("{:x}", tree.finalize()),
		files,
	})
}
/// One bounded reader; tree callers additionally include bytes in their tree digest.
fn hash_file(
	root: &Path,
	relative: PathBuf,
	name: String,
	allowance: &mut Allowance,
	mut tree: Option<&mut Sha256>,
) -> Result<(wire::File, fs::Metadata, fs::Metadata), String> {
	components_no_links(root, &relative)?;
	let path = root.join(relative);
	let before = regular(&path)?;
	if before.len() > allowance.bytes {
		return Err(fail(&path, "fingerprint exceeds byte allowance"));
	}
	let mut options = fs::OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
	}
	let mut file = options.open(&path).map_err(|e| fail(&path, e))?;
	let opened = file.metadata().map_err(|e| fail(&path, e))?;
	if !opened.is_file()
		|| opened.len() != before.len()
		|| executable(&opened) != executable(&before)
	{
		return Err(fail(&path, "file changed before read"));
	}
	if let Some(tree) = tree.as_mut() {
		tree.update((name.len() as u64).to_be_bytes());
		tree.update(name.as_bytes());
		tree.update([u8::from(executable(&opened))]);
		tree.update(opened.len().to_be_bytes());
	}
	#[cfg(test)]
	super::tests::after_open(&path);
	let mut content = Sha256::new();
	let mut remaining = opened.len();
	let mut buffer = [0u8; 16384];
	while remaining > 0 {
		let take = remaining.min(buffer.len() as u64) as usize;
		let n = file.read(&mut buffer[..take]).map_err(|e| fail(&path, e))?;
		if n == 0 {
			return Err(fail(&path, "size changed during read"));
		}
		allowance.bytes -= n as u64;
		remaining -= n as u64;
		if let Some(tree) = tree.as_mut() {
			tree.update(&buffer[..n]);
		}
		content.update(&buffer[..n]);
		#[cfg(feature = "test-support")]
		crate::artifact::observed_read(&path, n);
	}
	// The detection byte is charged too; never read it beyond the global allowance.
	if allowance.bytes == 0 {
		if file.metadata().map_err(|e| fail(&path, e))?.len() != opened.len() {
			return Err(fail(&path, "size changed during read"));
		}
	} else {
		let n = file.read(&mut buffer[..1]).map_err(|e| fail(&path, e))?;
		allowance.bytes -= n as u64;
		if n != 0 {
			return Err(fail(&path, "size changed during read"));
		}
	}
	let after = file.metadata().map_err(|e| fail(&path, e))?;
	if after.len() != opened.len() || executable(&after) != executable(&opened) {
		return Err(fail(&path, "file changed during read"));
	}
	Ok((
		wire::File {
			path: name,
			executable: executable(&opened),
			bytes: opened.len(),
			sha256: format!("{:x}", content.finalize()),
		},
		opened,
		after,
	))
}

fn root(path: &Path) -> Result<PathBuf, String> {
	if !path.is_absolute() {
		return Err(fail(path, "root must be absolute"));
	}
	// Refuse a symlink root before canonicalizing the identity.
	if !fs::symlink_metadata(path)
		.map_err(|e| fail(path, e))?
		.is_dir()
	{
		return Err(fail(path, "root must be a directory, not a symlink"));
	}
	path.canonicalize().map_err(|e| fail(path, e))
}
pub(crate) fn source(
	path: &Path,
	application: bool,
	allowance: &mut Allowance,
) -> Result<Tree, String> {
	let root = root(path)?;
	let mut dirs = vec![PathBuf::new()];
	let mut paths = vec![];
	while let Some(dir) = dirs.pop() {
		for item in fs::read_dir(root.join(&dir)).map_err(|e| fail(&root, e))? {
			let item = item.map_err(|e| fail(&root, e))?;
			let relative = dir.join(item.file_name());
			text(&relative)?;
			let ty = item.file_type().map_err(|e| fail(&item.path(), e))?;
			if ty.is_dir() && item.file_name() == ".git" {
				continue;
			}
			if application
				&& dir.as_os_str().is_empty()
				&& [".rnx", "rnx.lock", "rnx.Cargo.lock"]
					.iter()
					.any(|n| item.file_name() == *n)
			{
				continue;
			}
			allowance.entry()?;
			if ty.is_dir() {
				dirs.push(relative);
			} else if ty.is_file() {
				paths.push(relative);
			} else {
				return Err(fail(&item.path(), "symlink or special file refused"));
			}
		}
	}
	hash_files(&root, paths, allowance)
}
pub(crate) fn one(path: &Path, allowance: &mut Allowance) -> Result<wire::File, String> {
	Ok(one_observed(path, allowance)?.0)
}
pub(crate) fn one_observed(
	path: &Path,
	allowance: &mut Allowance,
) -> Result<(wire::File, fs::Metadata, fs::Metadata), String> {
	allowance.entry()?;
	let root = path.parent().ok_or("input has no parent")?;
	let name = path.file_name().ok_or("input has no name")?;
	let relative = PathBuf::from(name);
	let name = text(&relative)?;
	hash_file(root, relative, name, allowance, None)
}

/// Git output is bounded independently from file content. Kill and reap on overflow.
fn git(root: &Path, args: &[&str], limit: usize) -> Result<Vec<u8>, String> {
	let mut child = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::null())
		.spawn()
		.map_err(|e| fail(root, e))?;
	let mut bytes = vec![];
	let read = child
		.stdout
		.take()
		.unwrap()
		.take(limit as u64 + 1)
		.read_to_end(&mut bytes);
	if read.is_err() || bytes.len() > limit {
		let _ = child.kill();
		let _ = child.wait();
		return Err(fail(
			root,
			"Git inventory exceeds bounded output or cannot be read",
		));
	}
	if !child.wait().map_err(|e| fail(root, e))?.success() {
		return Err(fail(
			root,
			"Git inventory failed; native paths require a Git working tree",
		));
	}
	Ok(bytes)
}
pub(crate) fn native(path: &Path, allowance: &mut Allowance) -> Result<Tree, String> {
	native_using(path, allowance, git)
}
/// Same inventory and encoding, with installer-owned Git supervision/configuration.
pub(crate) fn native_using(
	path: &Path,
	allowance: &mut Allowance,
	git: impl Fn(&Path, &[&str], usize) -> Result<Vec<u8>, String>,
) -> Result<Tree, String> {
	let root = root(path)?;
	if !git(
		&root,
		&["rev-parse", "--show-superproject-working-tree"],
		16384,
	)?
	.is_empty()
	{
		return Err(fail(&root, "native root is a submodule"));
	}
	if !git(
		&root,
		&[
			"ls-files",
			"--others",
			"--exclude-standard",
			"-z",
			"--",
			".",
		],
		16 * 1024 * 1024,
	)?
	.is_empty()
	{
		return Err(fail(&root, "untracked non-ignored native file"));
	}
	let bytes = git(
		&root,
		&["ls-files", "--stage", "-z", "--", "."],
		16 * 1024 * 1024,
	)?;
	let mut paths = vec![];
	for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		allowance.entry()?;
		let record =
			std::str::from_utf8(record).map_err(|_| fail(&root, "non-Unicode tracked name"))?;
		let (header, path) = record.split_once('\t').ok_or("malformed Git inventory")?;
		let fields = header.split_whitespace().collect::<Vec<_>>();
		if fields.len() != 3 || fields[2] != "0" {
			return Err(fail(&root, "unmerged Git index"));
		}
		if !matches!(fields[0], "100644" | "100755") {
			return Err(fail(
				&root,
				"tracked symlink, submodule or unsupported mode",
			));
		}
		paths.push(PathBuf::from(path));
	}
	if paths.is_empty() {
		return Err(fail(&root, "native root has no tracked files"));
	}
	hash_files(&root, paths, allowance)
}
