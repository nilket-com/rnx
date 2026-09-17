//! Cargo's resolved local packages and the external files governing assembly.
#![allow(dead_code)]
use crate::{
	fingerprint::{self, Allowance, Tree},
	input,
};
use serde::{Deserialize, Serialize};
use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inventory {
	pub platform: String,
	pub packages: Vec<Association>,
	pub trees: Vec<Tree>,
	/// Absent candidates are identities too: creating one invalidates the snapshot.
	pub external: Vec<External>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Association {
	pub id: String,
	pub name: String,
	pub manifest: PathBuf,
	pub root: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct External {
	pub path: PathBuf,
	pub file: Option<crate::wire::File>,
}
#[derive(Deserialize)]
struct Metadata {
	packages: Vec<Package>,
	workspace_root: PathBuf,
}
#[derive(Deserialize)]
struct Package {
	id: String,
	name: String,
	source: Option<String>,
	manifest_path: PathBuf,
}
fn error(path: &Path, e: impl std::fmt::Display) -> String {
	format!("Cargo input {}: {e}", path.display())
}
fn absolute(path: &Path) -> Result<(), String> {
	if !path.is_absolute() || path.to_str().is_none() {
		return Err(error(path, "requires an absolute Unicode path"));
	}
	Ok(())
}
fn ancestors(path: &Path, candidates: &mut BTreeSet<PathBuf>) {
	for dir in path.ancestors() {
		for name in [
			"Cargo.toml",
			".cargo/config",
			".cargo/config.toml",
			"rust-toolchain",
			"rust-toolchain.toml",
		] {
			candidates.insert(dir.join(name));
		}
	}
}
/// Caller supplies bounded Cargo metadata from the exact generated wrapper and
/// build flags. No build scripts run here. Cargo remains the graph resolver.
pub(crate) fn native(
	metadata: &[u8],
	generated_root: &Path,
	invocation_dir: &Path,
	cargo_home: &Path,
	allowance: &mut Allowance,
) -> Result<Inventory, String> {
	if metadata.len() > input::DOCUMENT_LIMIT {
		return Err("Cargo metadata exceeds 16 MiB".into());
	}
	for p in [generated_root, invocation_dir, cargo_home] {
		absolute(p)?;
	}
	let metadata: Metadata =
		serde_json::from_slice(metadata).map_err(|e| format!("Cargo metadata: {e}"))?;
	absolute(&metadata.workspace_root)?;
	let mut packages = vec![];
	let mut roots = BTreeSet::new();
	let mut candidates = BTreeSet::new();
	ancestors(invocation_dir, &mut candidates);
	candidates.insert(cargo_home.join("config"));
	candidates.insert(cargo_home.join("config.toml"));
	// Root workspace can lie outside the package's ancestors through package.workspace.
	candidates.insert(metadata.workspace_root.join("Cargo.toml"));
	for p in metadata.packages.into_iter().filter(|p| p.source.is_none()) {
		absolute(&p.manifest_path)?;
		if fs::symlink_metadata(
			p.manifest_path
				.parent()
				.ok_or("Cargo package without parent")?,
		)
		.map_err(|e| error(&p.manifest_path, e))?
		.file_type()
		.is_symlink()
		{
			return Err(error(&p.manifest_path, "symlink package root"));
		}
		let root = p
			.manifest_path
			.parent()
			.ok_or("Cargo package without parent")?
			.canonicalize()
			.map_err(|e| error(&p.manifest_path, e))?;
		if root == generated_root {
			continue;
		}
		if packages.len() >= fingerprint::ENTRIES {
			return Err("too many Cargo package associations".into());
		}
		ancestors(&root, &mut candidates);
		roots.insert(root.clone());
		packages.push(Association {
			id: p.id,
			name: p.name,
			manifest: p.manifest_path,
			root,
		});
	}
	packages.sort_by(|a, b| a.id.cmp(&b.id));
	if packages.windows(2).any(|p| p[0].id == p[1].id) {
		return Err("duplicate Cargo package identity".into());
	}
	let mut trees = vec![];
	for root in roots {
		trees.push(fingerprint::native(&root, allowance)?);
	}
	// Inspect all ancestor manifests, not just metadata.workspace_root (which is
	// the generated wrapper's workspace). Explicit workspace redirects are followed.
	let mut external = BTreeMap::new();
	while let Some(path) = candidates.pop_first() {
		if external.contains_key(&path) {
			continue;
		}
		if external.len() >= fingerprint::ENTRIES {
			return Err("too many ancestor Cargo inputs".into());
		}
		absolute(&path)?;
		let file = match fs::symlink_metadata(&path) {
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
			Err(e) => return Err(error(&path, e)),
			Ok(_) => {
				let file = fingerprint::one(&path, allowance)?;
				// Parse from another bounded read; both digests must match. Charge that read
				// to the same allowance and refuse an ordinary edit between the two reads.
				if file.bytes > input::MANIFEST_LIMIT as u64
					|| file.bytes > allowance.remaining_bytes()
				{
					return Err(error(&path, "manifest or fingerprint byte allowance"));
				}
				use std::io::Read;
				let mut options = fs::OpenOptions::new();
				options.read(true);
				#[cfg(unix)]
				{
					use std::os::unix::fs::OpenOptionsExt;
					options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
				}
				let handle = options.open(&path).map_err(|e| error(&path, e))?;
				if !handle.metadata().map_err(|e| error(&path, e))?.is_file() {
					return Err(error(&path, "not a regular file"));
				}
				let mut bytes = vec![];
				(&handle)
					.take(file.bytes)
					.read_to_end(&mut bytes)
					.map_err(|e| error(&path, e))?;
				allowance.charge_bytes(bytes.len() as u64)?;
				if handle.metadata().map_err(|e| error(&path, e))?.len() != file.bytes {
					return Err(error(&path, "changed during audit"));
				}
				use sha2::{Digest, Sha256};
				if format!("{:x}", Sha256::digest(&bytes)) != file.sha256 {
					return Err(error(&path, "changed during audit"));
				}
				let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
				if matches!(name, "Cargo.toml" | "config" | "config.toml") {
					let doc: toml::Value = toml::from_str(
						std::str::from_utf8(&bytes).map_err(|_| error(&path, "not UTF-8"))?,
					)
					.map_err(|e| error(&path, e))?;
					if name == "Cargo.toml" {
						if let Some(value) = doc.get("package").and_then(|v| v.get("workspace")) {
							let target = value.as_str().ok_or_else(|| {
								error(&path, "workspace redirect is not a string")
							})?;
							let target = path
								.parent()
								.unwrap()
								.join(target)
								.canonicalize()
								.map_err(|e| error(&path, e))?;
							candidates.insert(target.join("Cargo.toml"));
						}
					} else if doc.get("include").is_some() {
						return Err(error(
							&path,
							"stop: Cargo configuration includes require an explicit inventory policy before building",
						));
					}
				}
				Some(file)
			}
		};
		external.insert(path.clone(), External { path, file });
	}
	Ok(Inventory {
		platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
		packages,
		trees,
		external: external.into_values().collect(),
	})
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Sources {
	pub packages: Vec<SourcePackage>,
	pub trees: Vec<Tree>,
	pub outside_manifests: Vec<External>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePackage {
	pub manifest: PathBuf,
	pub root: PathBuf,
}
pub(crate) fn sources(app: &Path, allowance: &mut Allowance) -> Result<Sources, String> {
	if fs::symlink_metadata(app)
		.map_err(|e| error(app, e))?
		.file_type()
		.is_symlink()
	{
		return Err(error(app, "symlink manifest"));
	}
	let app = app.canonicalize().map_err(|e| error(app, e))?;
	let mut packages = vec![];
	let mut application_root = None;
	crate::graph::expand(&app, |path| {
		let manifest = crate::manifest::Manifest::read(path)?;
		if path == app && manifest.application.is_none() {
			return Err("entry must be an application".into());
		}
		if path != app && manifest.source.is_none() {
			return Err("dependency must be a source package".into());
		}
		let node = manifest.graph_node(path)?;
		if fs::symlink_metadata(&node.root)
			.map_err(|e| error(&node.root, e))?
			.file_type()
			.is_symlink()
		{
			return Err(error(&node.root, "symlink source root"));
		}
		for dependency in node.dependencies.values() {
			if fs::symlink_metadata(dependency)
				.map_err(|e| error(dependency, e))?
				.file_type()
				.is_symlink()
			{
				return Err(error(dependency, "symlink manifest"));
			}
		}
		let root = node.root.canonicalize().map_err(|e| error(&node.root, e))?;
		if path == app {
			application_root = Some(root.clone());
		}
		packages.push(SourcePackage {
			manifest: path.to_owned(),
			root,
		});
		Ok(node)
	})?;
	packages.sort_by(|a, b| a.manifest.cmp(&b.manifest));
	let roots = packages
		.iter()
		.map(|p| p.root.clone())
		.collect::<BTreeSet<_>>();
	let mut trees = vec![];
	for root in roots {
		trees.push(fingerprint::source(
			&root,
			Some(&root) == application_root.as_ref(),
			allowance,
		)?);
	}
	let mut outside_manifests = vec![];
	for p in &packages {
		if !trees.iter().any(|t| {
			p.manifest
				.strip_prefix(&t.root)
				.is_ok_and(|rel| t.files.iter().any(|f| Path::new(&f.path) == rel))
		}) {
			outside_manifests.push(External {
				path: p.manifest.clone(),
				file: Some(fingerprint::one(&p.manifest, allowance)?),
			});
		}
	}
	Ok(Sources {
		packages,
		trees,
		outside_manifests,
	})
}
