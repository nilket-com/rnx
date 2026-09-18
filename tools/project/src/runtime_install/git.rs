use super::{hooks, storage};
use crate::{commands, fingerprint};
use std::{path::Path, process::Command};
use storage::err;
fn clean(c: &mut Command) {
	for (k, _) in std::env::vars_os() {
		if k.to_string_lossy().starts_with("GIT_") {
			c.env_remove(k);
		}
	}
	c.env("GIT_CONFIG_GLOBAL", "/dev/null")
		.env("GIT_CONFIG_SYSTEM", "/dev/null")
		.env("GIT_CONFIG_NOSYSTEM", "1")
		.env("GIT_ATTR_NOSYSTEM", "1")
		.env("GIT_OPTIONAL_LOCKS", "0")
		.env("GIT_NO_REPLACE_OBJECTS", "1");
}
pub(super) fn command(root: &Path) -> Command {
	let mut c = Command::new("git");
	use std::os::unix::process::CommandExt;
	// Private Git outputs without changing the parent's process-wide mask.
	unsafe {
		c.pre_exec(|| {
			libc::umask(0o077);
			Ok(())
		});
	}
	clean(&mut c);
	c.arg("-C").arg(root).args([
		"-c",
		"core.autocrlf=false",
		"-c",
		"core.fsmonitor=false",
		"-c",
		"core.safecrlf=false",
		"-c",
		"core.hooksPath=/dev/null",
		"-c",
		"core.attributesFile=/dev/null",
	]);
	c
}
pub(super) fn run(c: Command) -> Result<Vec<u8>, String> {
	commands::run_bounded(c)
		.map_err(|e| format!("Git prerequisite/operation failed (install Git if missing): {e}"))
}
pub(super) fn index(root: &Path, tree: &fingerprint::Tree) -> Result<(), String> {
	let mut c = command(root);
	c.args(["init", "--quiet", "--template=", "--object-format=sha1"]);
	run(c)?;
	// Bound argv per batch. hash-object --no-filters never runs .gitattributes filters.
	for batch in tree.files.chunks(64) {
		hooks::point("git-step")?;
		let mut c = command(root);
		c.args(["hash-object", "-w", "--no-filters", "--"]);
		for f in batch {
			c.arg(root.join(&f.path));
		}
		let out = String::from_utf8(run(c)?).map_err(err)?;
		let hashes: Vec<_> = out.lines().collect();
		if hashes.len() != batch.len() {
			return Err("object count mismatch".into());
		}
		let mut c = command(root);
		c.args(["update-index", "--add"]);
		for (f, hash) in batch.iter().zip(hashes) {
			if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
				return Err("bad Git object id".into());
			}
			c.arg("--cacheinfo").arg(format!(
				"{},{},{}",
				if f.executable { "100755" } else { "100644" },
				hash,
				f.path
			));
		}
		run(c)?;
		storage::walk(root, false)?;
	}
	Ok(())
}
// Compare raw blob identities; git diff/status may execute clean filters.
fn head_matches(root: &Path, tree: &fingerprint::Tree, commit: &str) -> Result<bool, String> {
	let mut c = command(root);
	c.args(["ls-tree", "-r", "-z", commit]);
	let output = run(c)?;
	let mut head = std::collections::BTreeMap::new();
	for record in output.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		let split = record
			.iter()
			.position(|b| *b == b'\t')
			.ok_or("bad tree record")?;
		let header = std::str::from_utf8(&record[..split]).map_err(err)?;
		let path = std::str::from_utf8(&record[split + 1..]).map_err(err)?;
		let parts: Vec<_> = header.split(' ').collect();
		if parts.len() != 3 || parts[1] != "blob" {
			return Ok(false);
		}
		head.insert(path.to_owned(), (parts[0].to_owned(), parts[2].to_owned()));
	}
	if head.len() != tree.files.len() {
		return Ok(false);
	}
	for batch in tree.files.chunks(64) {
		let mut c = command(root);
		c.args(["hash-object", "--no-filters", "--"]);
		for f in batch {
			c.arg(root.join(&f.path));
		}
		let raw = String::from_utf8(run(c)?).map_err(err)?;
		let hashes: Vec<_> = raw.lines().collect();
		if hashes.len() != batch.len() {
			return Err("bad raw hash count".into());
		}
		for (f, hash) in batch.iter().zip(hashes) {
			let mode = if f.executable { "100755" } else { "100644" };
			if head.get(&f.path) != Some(&(mode.to_owned(), hash.to_owned())) {
				return Ok(false);
			}
		}
	}
	Ok(true)
}

pub(super) fn inventory(root: &Path) -> Result<fingerprint::Tree, String> {
	commands::check()?;
	let tree = fingerprint::native_using(
		root,
		&mut fingerprint::Allowance::bounded(
			hooks::limit("RNX_INSTALL_SOURCE_FILES", 100000) as usize,
			hooks::limit("RNX_INSTALL_SOURCE_BYTES", 512 * 1024 * 1024),
		),
		|root, args, limit| {
			let mut c = command(root);
			c.args(args);
			let b = run(c)?;
			if b.len() > limit {
				return Err("Git inventory output exceeds allowance".into());
			}
			Ok(b)
		},
	)?;
	if tree.files.len() as u64 > hooks::limit("RNX_INSTALL_SOURCE_FILES", 100000)
		|| tree.files.iter().map(|f| f.bytes).sum::<u64>()
			> hooks::limit("RNX_INSTALL_SOURCE_BYTES", 512 * 1024 * 1024)
	{
		return Err("source exceeds installer allowance".into());
	}
	commands::check()?;
	Ok(tree)
}
pub(super) fn top(root: &Path) -> Result<(), String> {
	let mut c = command(root);
	c.args(["rev-parse", "--show-toplevel"]);
	let b = String::from_utf8(run(c)?).map_err(err)?;
	// Git adds one terminator; a literal newline in the root is not trimmed away.
	let p = Path::new(b.strip_suffix('\n').ok_or("bad Git root")?);
	if p.canonicalize().map_err(err)? != root {
		return Err("runtime source must be the top of a Git working tree".into());
	}
	Ok(())
}
pub(super) fn provenance(
	root: &Path,
	tree: &fingerprint::Tree,
) -> Result<(Option<String>, bool), String> {
	let mut c = command(root);
	c.args(["rev-parse", "--revs-only", "HEAD"]);
	let b = String::from_utf8(run(c)?).map_err(err)?;
	if b.is_empty() {
		return Ok((None, true));
	}
	let commit = b.trim_end_matches('\n');
	if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
		return Err("invalid source commit".into());
	}
	let dirty = !head_matches(root, tree, commit)?;
	Ok((Some(commit.into()), dirty))
}
pub(super) fn administration(root: &Path) -> Result<(), String> {
	let admin = root.join(".git");
	storage::dir(&admin)?;
	for e in std::fs::read_dir(&admin).map_err(err)? {
		let e = e.map_err(err)?;
		if !["HEAD", "config", "index", "objects", "refs"]
			.iter()
			.any(|s| e.file_name() == *s)
		{
			return Err(format!(
				"unexpected installed Git administration: {}",
				e.path().display()
			));
		}
	}
	for n in ["objects", "refs"] {
		storage::dir(&admin.join(n))?;
	}
	for n in ["HEAD", "index"] {
		storage::read(&admin.join(n), 16 * 1024 * 1024)?;
	}
	let config = String::from_utf8(storage::read(&admin.join("config"), 65536)?).map_err(err)?;
	let mut section = false;
	for line in config.lines().map(str::trim).filter(|s| !s.is_empty()) {
		if line == "[core]" {
			section = true;
			continue;
		}
		let Some((key, value)) = line.split_once('=') else {
			return Err("invalid installed Git config".into());
		};
		if !section
			|| !matches!(
				(key.trim(), value.trim()),
				("repositoryformatversion", "0")
					| ("filemode", "true")
					| ("bare", "false")
					| ("logallrefupdates", "true")
			) {
			return Err("unsupported installed Git configuration".into());
		}
	}
	for n in ["objects/info/alternates", "objects/info/http-alternates"] {
		if storage::exists(&admin.join(n))? {
			return Err("installed Git alternates are forbidden".into());
		}
	}
	let mut c = command(root);
	c.args(["fsck", "--full", "--no-reflogs"]);
	run(c)?;
	Ok(())
}
