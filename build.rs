//! Gate-1 prototype: build provenance only. No network or source acquisition.
use std::{
	collections::BTreeMap,
	fs,
	path::{Path, PathBuf},
	process::Command,
};
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
	let mut c = Command::new("git");
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
	let p = c
		.arg("-C")
		.arg(root)
		.args([
			"-c",
			"core.autocrlf=false",
			"-c",
			"core.fsmonitor=false",
			"-c",
			"core.hooksPath=/dev/null",
			"-c",
			"core.attributesFile=/dev/null",
		])
		.args(args)
		.output()
		.map_err(|e| e.to_string())?;
	if !p.status.success() {
		return Err(String::from_utf8_lossy(&p.stderr).into_owned());
	}
	if p.stdout.len() > 16 * 1024 * 1024 {
		return Err("Git output allowance".into());
	}
	Ok(p.stdout)
}
fn text(root: &Path, args: &[&str]) -> Result<String, String> {
	String::from_utf8(git(root, args)?)
		.map(|s| s.trim().to_owned())
		.map_err(|e| e.to_string())
}
fn revision(root: &Path) -> Result<String, String> {
	if PathBuf::from(text(root, &["rev-parse", "--show-toplevel"])?)
		.canonicalize()
		.map_err(|e| e.to_string())?
		!= root.canonicalize().map_err(|e| e.to_string())?
	{
		return Err("unrelated enclosing repository".into());
	}
	let r = text(root, &["rev-parse", "HEAD"])?;
	if r.len() != 40 || !r.bytes().all(|b| b.is_ascii_hexdigit()) {
		return Err("unknown revision".into());
	}
	Ok(r)
}
fn clean(root: &Path, rev: &str) -> Result<bool, String> {
	let tree = git(root, &["ls-tree", "-r", "-z", rev])?;
	let mut rows = BTreeMap::new();
	let mut bytes = 0u64;
	for record in tree.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		let pos = record
			.iter()
			.position(|b| *b == b'\t')
			.ok_or("tree record")?;
		let fields = std::str::from_utf8(&record[..pos])
			.map_err(|e| e.to_string())?
			.split(' ')
			.collect::<Vec<_>>();
		let name = std::str::from_utf8(&record[pos + 1..]).map_err(|e| e.to_string())?;
		if fields.len() != 3 || fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
			return Ok(false);
		}
		let p = root.join(name);
		let m = match fs::symlink_metadata(&p) {
			Ok(m) => m,
			Err(_) => return Ok(false),
		};
		if !m.is_file() {
			return Ok(false);
		}
		for dir in p.parent().unwrap().ancestors() {
			if dir == root {
				break;
			}
			if !fs::symlink_metadata(dir)
				.map_err(|e| e.to_string())?
				.is_dir()
			{
				return Ok(false);
			}
		}
		use std::os::unix::fs::PermissionsExt;
		if (m.permissions().mode() & 0o111 != 0) != (fields[0] == "100755") {
			return Ok(false);
		}
		bytes += m.len();
		if bytes > 512 * 1024 * 1024 || rows.len() >= 100_000 {
			return Err("source allowance".into());
		}
		rows.insert(name.to_owned(), fields[2].to_owned());
	}
	let paths = rows.keys().map(String::as_str).collect::<Vec<_>>();
	for chunk in paths.chunks(64) {
		let mut args = vec!["hash-object", "--no-filters", "--"];
		args.extend_from_slice(chunk);
		let out = text(root, &args)?;
		if out.lines().collect::<Vec<_>>()
			!= chunk.iter().map(|n| rows[*n].as_str()).collect::<Vec<_>>()
		{
			return Ok(false);
		}
	}
	let index = git(root, &["ls-files", "--stage", "-z"])?;
	let mut names = vec![];
	for record in index.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		let pos = record
			.iter()
			.position(|b| *b == b'\t')
			.ok_or("index record")?;
		if !record[..pos].ends_with(b" 0") {
			return Ok(false);
		}
		names.push(std::str::from_utf8(&record[pos + 1..]).map_err(|e| e.to_string())?);
	}
	if names != paths {
		return Ok(false);
	}
	let extra = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
	if extra
		.split(|b| *b == 0)
		.any(|n| !n.is_empty() && n != b".cargo-ok")
	{
		return Ok(false);
	}
	if let Ok(m) = fs::symlink_metadata(root.join(".cargo-ok"))
		&& (!m.is_file() || m.len() > 4096)
	{
		return Ok(false);
	}
	Ok(true)
}
fn acquired(root: &Path, rev: &str, url: &str) -> bool {
	let Some(home) = std::env::var_os("CARGO_HOME")
		.map(PathBuf::from)
		.or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
	else {
		return false;
	};
	let Ok(home) = home.canonicalize() else {
		return false;
	};
	let Ok(root) = root.canonicalize() else {
		return false;
	};
	if !root.starts_with(home.join("git/checkouts")) {
		return false;
	}
	let Ok(origin) = text(&root, &["config", "--get", "remote.origin.url"]) else {
		return false;
	};
	let Some(local) = origin.strip_prefix("file://") else {
		return false;
	};
	let Ok(db) = Path::new(local).canonicalize() else {
		return false;
	};
	if db.parent() != Some(home.join("git/db").as_path()) {
		return false;
	}
	let Ok(fetched) = fs::read_to_string(db.join("FETCH_HEAD")) else {
		return false;
	};
	if fetched.len() > 1024 * 1024 {
		return false;
	}
	for line in fetched.lines() {
		let Some((oid, description)) = line.split_once('\t') else {
			continue;
		};
		if oid.len() != 40
			|| !oid.bytes().all(|b| b.is_ascii_hexdigit())
			|| !description.ends_with(&format!(" of {url}"))
		{
			continue;
		}
		if git(&db, &["merge-base", "--is-ancestor", rev, oid]).is_ok() {
			return true;
		}
	}
	false
}
fn main() {
	println!("cargo:rerun-if-env-changed=CARGO_FEATURE_STOCK_MANAGEMENT");
	if std::env::var_os("CARGO_FEATURE_STOCK_MANAGEMENT").is_none() {
		return;
	}
	let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
	// Cargo mtime tracking does not detect chmod. A missing private output asks
	// Cargo to rerun this scan on every stock build, including mode-only edits.
	let marker = out.join("coordinate-rescan-required");
	let _ = fs::remove_file(&marker);
	println!("cargo:rerun-if-changed={}", marker.display());
	let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
	let url = std::env::var("CARGO_PKG_REPOSITORY").unwrap_or_default();
	let rev = revision(&root).unwrap_or_else(|_| "unknown".into());
	let state = if rev == "unknown" {
		"unknown"
	} else {
		match clean(&root, &rev) {
			Ok(false) => "dirty",
			Err(_) => "unknown",
			Ok(true) => {
				if acquired(&root, &rev, &url) {
					"acquired"
				} else {
					"unverified"
				}
			}
		}
	};
	let data = format!(
		"pub const URL:&str={url:?};\npub const REV:&str={rev:?};\npub const STATE:&str={state:?};\npub const SOURCE:&str={:?};\n",
		root.to_string_lossy()
	);
	let dest = out.join("coordinates.rs");
	if fs::read_to_string(&dest).ok().as_deref() != Some(&data) {
		fs::write(dest, data).unwrap();
	}
}
