//! Versioned assembly identity. Private staging for 0061 gate 2; the product
//! workflow does not select or publish shared entries until later gates.
#![allow(dead_code)]
use crate::{fingerprint, generate, input, inventory::Inventory, manifest::Manifest, wire};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
	collections::BTreeSet,
	path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Context {
	pub cache_root: PathBuf,
	pub cargo_home: PathBuf,
	pub rustup_home: Option<PathBuf>,
	pub rustup_toolchain: Option<String>,
	pub rustc: String,
	pub cargo: String,
	pub target: String,
	pub profile: String,
	pub features: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
	format: u32,
	generator: u32,
	context: Context,
	manifest: String,
	main: String,
	cargo_lock_sha256: String,
	native: Inventory,
}
/// Construction requires a live verified inventory. Decoding validates shape
/// and canonical encoding, not the truth of filesystem or compiler observations.
#[derive(Clone, Debug)]
pub(crate) struct Identity {
	document: Document,
	bytes: Vec<u8>,
	key: String,
}
fn digest(bytes: &[u8]) -> String {
	format!("{:x}", Sha256::digest(bytes))
}
fn valid_digest(s: &str) -> bool {
	s.len() == 64
		&& s.bytes()
			.all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn path(p: &Path) -> Result<(), String> {
	if !p.is_absolute()
		|| p.to_str().is_none()
		|| p.components().collect::<PathBuf>().as_os_str() != p.as_os_str()
		|| p.components()
			.any(|c| matches!(c, Component::ParentDir | Component::CurDir))
	{
		return Err("assembly identity requires normalized absolute Unicode paths".into());
	}
	Ok(())
}
fn directory(p: &Path) -> Result<PathBuf, String> {
	if !p.is_absolute() || p.to_str().is_none() {
		return Err("assembly directory requires an absolute Unicode path".into());
	}
	let p = p
		.canonicalize()
		.map_err(|e| format!("assembly directory {}: {e}", p.display()))?;
	if !p.is_dir() {
		return Err(format!(
			"assembly directory is not a directory: {}",
			p.display()
		));
	}
	path(&p)?;
	Ok(p)
}
fn nonempty(s: &str) -> Result<(), String> {
	if s.is_empty() || s.contains('\0') {
		Err("empty or NUL assembly identity field".into())
	} else {
		Ok(())
	}
}
fn relative(s: &str) -> Result<(), String> {
	if s.is_empty()
		|| s.contains(['\\', '\0'])
		|| s.split('/').any(|p| p.is_empty() || p == "." || p == "..")
		|| Path::new(s).is_absolute()
	{
		Err("invalid relative inventory path".into())
	} else {
		Ok(())
	}
}
impl Identity {
	pub(crate) fn create(
		manifest: &Manifest,
		base: &Path,
		mut context: Context,
		mut native: Inventory,
		cargo_lock: &[u8],
	) -> Result<Self, String> {
		if cargo_lock.is_empty() || cargo_lock.len() > input::DOCUMENT_LIMIT {
			return Err("Cargo lock is empty or exceeds document allowance".into());
		}
		context.cache_root = directory(&context.cache_root)?;
		context.cargo_home = directory(&context.cargo_home)?;
		context.rustup_home = context.rustup_home.as_deref().map(directory).transpose()?;
		context.features.sort();
		// The inventory owns canonical native roots. Refuse a caller handing
		// over a lexical or symlink alias rather than silently rewriting IDs.
		for tree in &native.trees {
			if directory(&tree.root)? != tree.root {
				return Err("native inventory root is not canonical".into());
			}
			if context.cache_root.starts_with(&tree.root)
				|| tree.root.starts_with(&context.cache_root)
			{
				return Err("native inputs and assembly cache must not contain one another".into());
			}
		}
		for tree in &mut native.trees {
			tree.files.sort_by(|a, b| a.path.cmp(&b.path));
		}
		native.trees.sort_by(|a, b| a.root.cmp(&b.root));
		native.packages.sort_by(|a, b| a.id.cmp(&b.id));
		native.external.sort_by(|a, b| a.path.cmp(&b.path));
		let (manifest, main) = generate::canonical_wrapper(manifest, base)?;
		Self::from_document(Document {
			format: 1,
			generator: 1,
			context,
			manifest,
			main,
			cargo_lock_sha256: digest(cargo_lock),
			native,
		})
	}
	fn from_document(document: Document) -> Result<Self, String> {
		document.validate()?;
		let bytes = wire::encode(&document)?;
		let key = digest(&bytes);
		Ok(Self {
			document,
			bytes,
			key,
		})
	}
	pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
		if bytes.len() > input::DOCUMENT_LIMIT {
			return Err("assembly identity exceeds document allowance".into());
		}
		let doc: Document =
			serde_json::from_slice(bytes).map_err(|e| format!("assembly identity: {e}"))?;
		let identity = Self::from_document(doc)?;
		if identity.bytes != bytes {
			return Err("assembly identity is not canonically encoded".into());
		}
		Ok(identity)
	}
	pub(crate) fn context(&self) -> &Context {
		&self.document.context
	}
	pub(crate) fn wrapper(&self) -> (&str, &str) {
		(&self.document.manifest, &self.document.main)
	}
	pub(crate) fn check_lock(&self, bytes: &[u8]) -> Result<(), String> {
		if digest(bytes) != self.document.cargo_lock_sha256 {
			return Err("assembly Cargo lock mismatch".into());
		}
		Ok(())
	}
	/// No Cargo resolution: use recorded associations to repeat the full input audit.
	pub(crate) fn revalidate(&self, stage: &Path) -> Result<(), String> {
		let c = self.context();
		let packages: Vec<_> = self
			.document
			.native
			.packages
			.iter()
			.map(
				|p| serde_json::json!({"id":p.id,"name":p.name,"source":null,"manifest_path":p.manifest}),
			)
			.collect();
		let metadata =
			wire::encode(&serde_json::json!({"workspace_root":stage,"packages":packages}))?;
		let native = crate::inventory::native(
			&metadata,
			stage,
			&c.cache_root,
			&c.cargo_home,
			&mut fingerprint::Allowance::default(),
		)?;
		crate::cache_storage::policy(&native)?;
		if native != self.document.native {
			return Err("assembly native/context inputs changed; run lock".into());
		}
		Ok(())
	}

	pub(crate) fn key(&self) -> &str {
		&self.key
	}
	pub(crate) fn bytes(&self) -> &[u8] {
		&self.bytes
	}
}
fn strictly_sorted<T: Ord>(values: impl IntoIterator<Item = T>) -> Result<(), String> {
	let mut previous = None;
	for value in values {
		if previous.as_ref().is_some_and(|p| p >= &value) {
			return Err("assembly identity collection is unsorted or duplicated".into());
		}
		previous = Some(value);
	}
	Ok(())
}
impl Document {
	fn validate(&self) -> Result<(), String> {
		if self.format != 1 || self.generator != 1 {
			return Err("unsupported assembly identity/generator version".into());
		}
		let c = &self.context;
		for p in [&c.cache_root, &c.cargo_home] {
			path(p)?;
		}
		if let Some(p) = &c.rustup_home {
			path(p)?;
		}
		if let Some(s) = &c.rustup_toolchain {
			nonempty(s)?;
		}
		for s in [
			&c.rustc,
			&c.cargo,
			&c.target,
			&c.profile,
			&self.manifest,
			&self.main,
			&self.native.platform,
		] {
			nonempty(s)?;
		}
		strictly_sorted(c.features.iter())?;
		for f in &c.features {
			nonempty(f)?;
		}
		if !valid_digest(&self.cargo_lock_sha256) {
			return Err("invalid Cargo lock digest".into());
		}
		let n = &self.native;
		if n.trees.is_empty()
			|| n.packages.is_empty()
			|| n.trees.len() > fingerprint::ENTRIES
			|| n.packages.len() > fingerprint::ENTRIES
			|| n.external.len() > fingerprint::ENTRIES
		{
			return Err("invalid assembly inventory counts".into());
		}
		strictly_sorted(n.trees.iter().map(|t| &t.root))?;
		strictly_sorted(n.packages.iter().map(|p| &p.id))?;
		strictly_sorted(n.external.iter().map(|e| &e.path))?;
		let mut entries = 0usize;
		let mut bytes = 0u64;
		let mut file = |f: &wire::File| -> Result<(), String> {
			relative(&f.path)?;
			if !valid_digest(&f.sha256) {
				return Err("invalid inventory digest".into());
			}
			entries += 1;
			bytes = bytes
				.checked_add(f.bytes)
				.ok_or("inventory size overflow")?;
			if entries > fingerprint::ENTRIES || bytes > fingerprint::BYTES {
				return Err("assembly inventory exceeds shared allowance".into());
			}
			Ok(())
		};
		for t in &n.trees {
			path(&t.root)?;
			if !valid_digest(&t.sha256) {
				return Err("invalid native tree digest".into());
			}
			if c.cache_root.starts_with(&t.root) || t.root.starts_with(&c.cache_root) {
				return Err("native inputs overlap assembly cache".into());
			}
			strictly_sorted(t.files.iter().map(|f| &f.path))?;
			for f in &t.files {
				file(f)?;
			}
		}
		let mut associations = BTreeSet::new();
		for p in &n.packages {
			path(&p.root)?;
			path(&p.manifest)?;
			nonempty(&p.id)?;
			nonempty(&p.name)?;
			if p.manifest != p.root.join("Cargo.toml")
				|| !n.trees.iter().any(|t| t.root == p.root)
				|| !associations.insert((&p.root, &p.name))
			{
				return Err("invalid native package association".into());
			}
		}
		let wrapper: toml::Value =
			toml::from_str(&self.manifest).map_err(|e| format!("assembly manifest: {e}"))?;
		let dependencies = wrapper
			.get("dependencies")
			.and_then(toml::Value::as_table)
			.ok_or("assembly manifest has no dependencies")?;
		if !dependencies.contains_key("rnx") {
			return Err("assembly manifest has no rnx dependency".into());
		}
		for (alias, dependency) in dependencies {
			let root = dependency
				.get("path")
				.and_then(toml::Value::as_str)
				.ok_or("assembly dependency needs a path")?;
			let name = dependency
				.get("package")
				.and_then(toml::Value::as_str)
				.unwrap_or(alias);
			path(Path::new(root))?;
			if !n
				.packages
				.iter()
				.any(|p| p.root == Path::new(root) && p.name == name)
			{
				return Err("assembly dependency missing from native inventory".into());
			}
		}
		for e in &n.external {
			path(&e.path)?;
			if let Some(f) = &e.file {
				file(f)?;
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests;
