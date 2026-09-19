//! Git coordinates are launch identities, not a claim about mutable checkout
//! bytes. Only full operations authenticate raw objects and working-tree bytes.
#![allow(dead_code)]
pub(crate) mod acquisition;
use crate::{commands, fingerprint, inventory, schemas::GitPackage, wire};
use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	io::Read,
	path::{Path, PathBuf},
	process::Command,
};
fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}
fn command(root: &Path) -> Command {
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
		.env("GIT_NO_REPLACE_OBJECTS", "1")
		.env("GIT_OPTIONAL_LOCKS", "0")
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
		]);
	c
}
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
	let mut c = command(root);
	c.args(args);
	commands::run_bounded(c)
}
fn text(b: &[u8]) -> Result<&str, String> {
	std::str::from_utf8(b).map_err(error)
}
fn canonical_dir(path: &Path) -> Result<(), String> {
	if !path.is_absolute() || path.canonicalize().map_err(error)? != path {
		return Err(format!(
			"Git checkout is missing or noncanonical: {}",
			path.display()
		));
	}
	for ancestor in path.ancestors() {
		if !fs::symlink_metadata(ancestor).map_err(error)?.is_dir() {
			return Err(format!(
				"Git source directory is not regular: {}",
				ancestor.display()
			));
		}
	}
	Ok(())
}
fn relative(s: &str) -> Result<(), String> {
	if s.is_empty()
		|| s.contains(['\\', '\0'])
		|| s.split('/').any(|p| p.is_empty() || p == "." || p == "..")
		|| Path::new(s).is_absolute()
	{
		Err("invalid Git tree path".into())
	} else {
		Ok(())
	}
}
/// No Git or source-content read. These structural checks deliberately do not
/// authenticate a replacement directory with the same canonical pathname.
pub(crate) fn structure(packages: &[GitPackage]) -> Result<(), String> {
	let mut roots = BTreeSet::new();
	for p in packages {
		relative(&p.manifest)?;
		let root = Path::new(&p.checkout);
		if roots.insert(root) {
			canonical_dir(root)?;
		}
		let parent = root
			.join(&p.manifest)
			.parent()
			.ok_or("Git manifest parent")?
			.to_owned();
		canonical_dir(&parent)?;
	}
	Ok(())
}
#[derive(Clone)]
struct Blob {
	name: String,
	oid: String,
	executable: bool,
	len: u64,
}
fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
	let mut o = fs::OpenOptions::new();
	o.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
	}
	let f = o.open(path).map_err(error)?;
	let m = f.metadata().map_err(error)?;
	if !m.is_file() || m.len() > limit {
		return Err(format!(
			"Git input is not bounded regular file: {}",
			path.display()
		));
	}
	let mut b = Vec::new();
	(&f).take(limit + 1).read_to_end(&mut b).map_err(error)?;
	if b.len() as u64 != m.len() || f.metadata().map_err(error)?.len() != m.len() {
		return Err("Git input changed during read".into());
	}
	Ok(b)
}
pub(crate) fn verify(packages: &[GitPackage]) -> Result<(), String> {
	verify_with(packages, &mut fingerprint::Allowance::default())
}
fn verify_with(
	packages: &[GitPackage],
	allowance: &mut fingerprint::Allowance,
) -> Result<(), String> {
	structure(packages)?;
	let mut roots = BTreeMap::new();
	for p in packages {
		if let Some(rev) = roots.insert(&p.checkout, &p.revision)
			&& rev != &p.revision
		{
			return Err("one checkout has multiple revisions".into());
		}
	}
	for (root, rev) in roots {
		verify_root(Path::new(root), rev, allowance)
			.map_err(|e| format!("Git checkout {root} at {rev}: {e}"))?;
	}
	Ok(())
}
fn verify_root(
	root: &Path,
	rev: &str,
	allowance: &mut fingerprint::Allowance,
) -> Result<(), String> {
	if text(&git(root, &["rev-parse", "HEAD"])?)?.trim() != rev {
		return Err("HEAD differs from locked revision".into());
	}
	canonical_dir(&root.join(".git"))?;
	if text(&git(root, &["rev-parse", "--show-toplevel"])?)?.trim()
		!= root.to_str().ok_or("non-Unicode checkout")?
	{
		return Err("Git worktree routing differs".into());
	}
	// Authenticate this commit, then check connectivity/corruption from its tree
	// rather than requesting a whole-history fsck. Blob contents are checked below.
	let commit = git(root, &["cat-file", "commit", rev])?;
	let tree_id = text(&commit)?
		.lines()
		.next()
		.and_then(|s| s.strip_prefix("tree "))
		.ok_or("invalid commit tree")?
		.to_owned();
	crate::schemas::git_coordinate("file:///object", &tree_id)?;
	let mut hash = command(root);
	hash.args(["hash-object", "--no-filters", "-t", "commit", "--stdin"]);
	if text(&commands::run_input(hash, commit, 128)?)?.trim() != rev {
		return Err("corrupt committed object identity".into());
	}
	git(
		root,
		&[
			"fsck",
			"--connectivity-only",
			"--no-dangling",
			"--no-reflogs",
			&tree_id,
		],
	)?;
	let tree = git(root, &["ls-tree", "-r", "-z", rev])?;
	let mut rows = Vec::new();
	let mut dirs = BTreeSet::new();
	for row in tree.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		let (head, name) = text(row)?.split_once('\t').ok_or("invalid tree entry")?;
		relative(name)?;
		let fields: Vec<_> = head.split(' ').collect();
		if fields.len() != 3 || fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
			return Err("unsupported Git tree entry".into());
		}
		let path = root.join(name);
		for parent in path.parent().unwrap().ancestors() {
			if parent == root || !dirs.insert(parent.to_owned()) {
				break;
			}
			if !fs::symlink_metadata(parent).map_err(error)?.is_dir() {
				return Err("Git source has symlink/non-directory component".into());
			}
		}
		let m = fs::symlink_metadata(&path).map_err(error)?;
		if !m.is_file() {
			return Err("Git source is not a regular file".into());
		}
		#[cfg(unix)]
		{
			use std::os::unix::fs::MetadataExt;
			if (m.mode() & 0o111 != 0) != (fields[0] == "100755") {
				return Err("Git source executable mode differs".into());
			}
		}
		allowance
			.entry()
			.map_err(|e| format!("Git input allowance exceeded: {e}"))?;
		allowance
			.charge_bytes(m.len())
			.map_err(|e| format!("Git input allowance exceeded: {e}"))?;
		rows.push(Blob {
			name: name.into(),
			oid: fields[2].into(),
			executable: fields[0] == "100755",
			len: m.len(),
		});
	}
	if rows.is_empty() {
		return Err("empty Git tree".into());
	}
	let index = git(root, &["ls-files", "--stage", "-z"])?;
	let staged: Vec<_> = index.split(|b| *b == 0).filter(|r| !r.is_empty()).collect();
	if staged.len() != rows.len() {
		return Err("Git tracked set differs".into());
	}
	for (raw, row) in staged.iter().zip(&rows) {
		let (head, name) = text(raw)?.split_once('\t').ok_or("invalid index")?;
		let expected = format!(
			"{} {} 0",
			if row.executable { "100755" } else { "100644" },
			row.oid
		);
		if head != expected || name != row.name {
			return Err("Git staged tree differs or is unmerged".into());
		}
	}
	let extra = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
	for name in extra.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		if name != b".cargo-ok" {
			return Err("untracked Git source".into());
		}
	}
	// The exception is the root marker alone, including when ignore rules hide it.
	match fs::symlink_metadata(root.join(".cargo-ok")) {
		Ok(_) => {
			read_regular(&root.join(".cargo-ok"), 4096)?;
		}
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
		Err(e) => return Err(error(e)),
	}
	// Batch raw-object reads authenticate the referenced objects too. Comparing
	// their bytes directly with no-follow file reads avoids filters and does not
	// mistake an unchanged object name for proof that its stored bytes are sound.
	for chunk in rows.chunks(64) {
		let names = chunk
			.iter()
			.map(|r| format!("{}\n", r.oid))
			.collect::<String>()
			.into_bytes();
		let mut cmd = command(root);
		cmd.args(["cat-file", "--batch"]);
		let limit = chunk.iter().try_fold(0usize, |n, r| {
			n.checked_add(r.len as usize + 256)
				.ok_or("object allowance")
		})?;
		let objects = commands::run_input(cmd, names, limit)?;
		let mut rest = objects.as_slice();
		for row in chunk {
			let end = rest
				.iter()
				.position(|b| *b == b'\n')
				.ok_or("missing Git object")?;
			if text(&rest[..end])? != format!("{} blob {}", row.oid, row.len) {
				return Err("missing, corrupt or changed Git blob".into());
			}
			rest = &rest[end + 1..];
			let len = row.len as usize;
			if rest.len() <= len || rest[len] != b'\n' {
				return Err("truncated Git blob".into());
			}
			if read_regular(&root.join(&row.name), row.len)? != rest[..len] {
				return Err(format!("raw Git blob differs: {}", row.name));
			}
			rest = &rest[len + 1..];
		}
		if !rest.is_empty() {
			return Err("extra Git object output".into());
		}
		let mut args = vec!["hash-object", "--no-filters", "--"];
		args.extend(chunk.iter().map(|r| r.name.as_str()));
		let hashes = git(root, &args)?;
		if text(&hashes)?
			.lines()
			.ne(chunk.iter().map(|r| r.oid.as_str()))
		{
			return Err("raw Git object identity differs".into());
		}
	}
	Ok(())
}

pub(crate) fn resolve(
	metadata: &[u8],
	stage: &Path,
	root: &Path,
	home: &Path,
	allowance: &mut fingerprint::Allowance,
) -> Result<(inventory::Inventory, Vec<GitPackage>), String> {
	let packages = packages(metadata)?;
	verify_with(&packages, allowance)?;
	let native = partition(metadata, &packages, stage, root, home, allowance, true, &[])?;
	Ok((native, packages))
}
pub(crate) fn packages(metadata: &[u8]) -> Result<Vec<GitPackage>, String> {
	let doc: serde_json::Value = serde_json::from_slice(metadata).map_err(error)?;
	let mut packages = Vec::new();
	for p in doc
		.get("packages")
		.and_then(serde_json::Value::as_array)
		.ok_or("missing Cargo packages")?
	{
		let Some(source) = p
			.get("source")
			.and_then(serde_json::Value::as_str)
			.filter(|s| s.starts_with("git+"))
		else {
			continue;
		};
		let (coordinate, revision) = source
			.strip_prefix("git+")
			.unwrap()
			.rsplit_once('#')
			.ok_or("Git package has no resolved revision")?;
		let url = coordinate.split('?').next().unwrap();
		crate::schemas::git_coordinate(url, revision)?;
		let manifest = PathBuf::from(
			p.get("manifest_path")
				.and_then(serde_json::Value::as_str)
				.ok_or("Git manifest path")?,
		);
		let package = manifest.parent().ok_or("Git package root")?;
		canonical_dir(package)?;
		let checkout = package
			.ancestors()
			.find(|a| fs::symlink_metadata(a.join(".git")).is_ok())
			.ok_or("Git checkout administration missing")?;
		if !fs::symlink_metadata(checkout.join(".git"))
			.map_err(error)?
			.is_dir()
		{
			return Err("Git checkout requires local administration directory".into());
		}
		packages.push(GitPackage {
			id: p
				.get("id")
				.and_then(serde_json::Value::as_str)
				.ok_or("Git package ID")?
				.into(),
			name: p
				.get("name")
				.and_then(serde_json::Value::as_str)
				.ok_or("Git package name")?
				.into(),
			url: url.into(),
			revision: revision.into(),
			checkout: checkout
				.to_str()
				.ok_or("Git checkout is not Unicode")?
				.into(),
			manifest: manifest
				.strip_prefix(checkout)
				.map_err(error)?
				.to_str()
				.ok_or("Git manifest is not Unicode")?
				.into(),
		});
	}
	packages.sort_by(|a, b| a.id.cmp(&b.id));
	Ok(packages)
}
#[allow(clippy::too_many_arguments)]
fn partition(
	metadata: &[u8],
	git: &[GitPackage],
	stage: &Path,
	root: &Path,
	home: &Path,
	allowance: &mut fingerprint::Allowance,
	full: bool,
	recorded: &[inventory::External],
) -> Result<inventory::Inventory, String> {
	let manifests = git
		.iter()
		.map(|p| Path::new(&p.checkout).join(&p.manifest))
		.collect::<Vec<_>>();
	let roots = git
		.iter()
		.map(|p| PathBuf::from(&p.checkout))
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect::<Vec<_>>();
	let mut n = inventory::native_partitioned(
		metadata,
		stage,
		root,
		home,
		allowance,
		&manifests,
		if full { &[] } else { &roots },
		recorded,
	)?;
	crate::cache_storage::policy(&n)?;
	n.external
		.retain(|e| !roots.iter().any(|r| e.path.starts_with(r)));
	Ok(n)
}
pub(crate) fn replay(
	native: &inventory::Inventory,
	git: &[GitPackage],
	stage: &Path,
	root: &Path,
	home: &Path,
	full: bool,
) -> Result<inventory::Inventory, String> {
	let mut allowance = fingerprint::Allowance::default();
	if full {
		verify_with(git, &mut allowance)?;
	} else {
		structure(git)?;
	}
	let packages = native
		.packages
		.iter()
		.map(
			|p| serde_json::json!({"id":p.id,"name":p.name,"source":null,"manifest_path":p.manifest}),
		)
		.collect::<Vec<_>>();
	let metadata = wire::encode(&serde_json::json!({"workspace_root":stage,"packages":packages}))?;
	partition(
		&metadata,
		git,
		stage,
		root,
		home,
		&mut allowance,
		full,
		if full { &[] } else { &native.external },
	)
}
