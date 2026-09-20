//! Versioned assembly identity. Private staging for 0061 gate 2; the product
//! workflow does not select or publish shared entries until later gates.
#![allow(dead_code)]
use crate::{fingerprint, generate, input, inventory::Inventory, manifest::Manifest, wire};
use serde::{Deserialize, Serialize};

use std::{
	collections::{BTreeMap, BTreeSet},
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
	/// Record 0069: where the assembly compiles. Absent in identities that
	/// predate sharing (formats 2 and 3), which keeps their bytes canonical;
	/// required from Git-source identity format 4.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub build: Option<Build>,
}
/// The build directory an assembly is compiled in, fixed at lock time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Build {
	/// The entry's own target directory (every path-source assembly).
	Private,
	/// The cache root's shared build directory for this key.
	Shared { key: String },
}
impl Build {
	pub(crate) fn validate(&self) -> Result<(), String> {
		match self {
			Build::Private => Ok(()),
			Build::Shared { key } if valid_digest(key) => Ok(()),
			Build::Shared { .. } => Err("invalid shared build key".into()),
		}
	}
}
/// The shared build directory's key: BLAKE3 over a framed preimage with a
/// domain tag of everything that must not differ between two assemblies
/// sharing Cargo units — toolchain, target, profile, features, the canonical
/// cache root and Cargo home, and the admitted Cargo configuration values.
/// Package identities and per-unit features are left to Cargo's own
/// fingerprints inside the directory.
pub(crate) fn build_key(context: &Context, settings: &[String]) -> String {
	let mut hasher = blake3::Hasher::new();
	let mut frame = |part: &str| {
		hasher.update(&(part.len() as u64).to_le_bytes());
		hasher.update(part.as_bytes());
	};
	frame("rnx-build-key-1");
	frame(&context.rustc);
	frame(&context.cargo);
	frame(&context.target);
	frame(&context.profile);
	frame(&context.features.len().to_string());
	for f in &context.features {
		frame(f);
	}
	frame(&context.cache_root.to_string_lossy());
	frame(&context.cargo_home.to_string_lossy());
	frame(&settings.len().to_string());
	for s in settings {
		frame(s);
	}
	hasher.finalize().to_hex().to_string()
}
/// The admitted Cargo configuration (0067: only `target.<triple>.linker`
/// and `rustflags`) as the *effective* values Cargo would merge from the
/// inventory's authenticated configuration files: the linker from the
/// highest-precedence file that sets it, and `rustflags` joined in
/// ascending precedence, higher-precedence items later, as Cargo joins
/// arrays. Precedence follows Cargo's search: a file deeper in the build
/// directory's ancestry outranks a shallower one; Cargo home's own file is
/// the lowest. In one directory Cargo reads `config` *instead of*
/// `config.toml` when both exist, so the masked file contributes nothing.
/// Two arrangements with the same effective values share a key; swapping
/// which file sets the linker changes it.
pub(crate) fn admitted_settings(
	context: &Context,
	inventory: &Inventory,
) -> Result<Vec<String>, String> {
	let mut files = Vec::new();
	for external in &inventory.external {
		let Some(file) = &external.file else { continue };
		let name = external.path.file_name().and_then(|s| s.to_str());
		if !matches!(name, Some("config" | "config.toml")) {
			continue;
		}
		let bytes = input::read(&external.path, input::MANIFEST_LIMIT)?;
		if digest(&bytes) != file.blake3 {
			return Err("Cargo configuration changed".into());
		}
		let config: toml::Value =
			toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
				.map_err(|e| e.to_string())?;
		crate::cache_storage::config_policy(&config, &external.path)?;
		let in_home = external.path.parent() == Some(context.cargo_home.as_path());
		let precedence = (
			if in_home {
				0
			} else {
				1 + external.path.components().count()
			},
			u8::from(name == Some("config")),
		);
		files.push((precedence, external.path.clone(), config));
	}
	// One file per directory: `config` masks a sibling `config.toml`.
	let masked: BTreeSet<PathBuf> = files
		.iter()
		.filter(|(p, _, _)| p.1 == 1)
		.filter_map(|(_, path, _)| Some(path.parent()?.join("config.toml")))
		.collect();
	files.retain(|(_, path, _)| !masked.contains(path));
	files.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
	let mut linker: BTreeMap<String, String> = BTreeMap::new();
	let mut rustflags: BTreeMap<String, Vec<toml::Value>> = BTreeMap::new();
	for (_, _, config) in &files {
		let Some(targets) = config.get("target").and_then(|v| v.as_table()) else {
			continue;
		};
		for (triple, settings) in targets {
			let Some(settings) = settings.as_table() else {
				continue;
			};
			if let Some(v) = settings.get("linker").and_then(|v| v.as_str()) {
				linker.insert(triple.clone(), v.to_owned());
			}
			if let Some(a) = settings.get("rustflags").and_then(|v| v.as_array()) {
				rustflags
					.entry(triple.clone())
					.or_default()
					.extend(a.iter().cloned());
			}
		}
	}
	let mut out = Vec::new();
	for (triple, value) in &linker {
		out.push(format!(
			"target.{triple}.linker={}",
			String::from_utf8(wire::encode(value)?).map_err(|e| e.to_string())?
		));
	}
	for (triple, values) in &rustflags {
		out.push(format!(
			"target.{triple}.rustflags={}",
			String::from_utf8(wire::encode(values)?).map_err(|e| e.to_string())?
		));
	}
	Ok(out)
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
	format: u32,
	generator: u32,
	context: Context,
	manifest: String,
	main: String,
	cargo_lock_blake3: String,
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
	blake3::hash(bytes).to_hex().to_string()
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
			format: 2,
			generator: 2,
			context,
			manifest,
			main,
			cargo_lock_blake3: digest(cargo_lock),
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
	pub(crate) fn native(&self) -> &Inventory {
		&self.document.native
	}
	pub(crate) fn context(&self) -> &Context {
		&self.document.context
	}
	pub(crate) fn wrapper(&self) -> (&str, &str) {
		(&self.document.manifest, &self.document.main)
	}
	pub(crate) fn check_lock(&self, bytes: &[u8]) -> Result<(), String> {
		if digest(bytes) != self.document.cargo_lock_blake3 {
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
	pub(crate) fn git_names(&self) -> Vec<String> {
		Vec::new()
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
		if self.format != 2 || self.generator != 2 {
			return Err("unsupported assembly identity/generator version".into());
		}
		let c = &self.context;
		if c.build.is_some() {
			return Err("path assembly identity carries a build kind".into());
		}
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
		if !valid_digest(&self.cargo_lock_blake3) {
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
			if !valid_digest(&f.blake3) {
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
			if !valid_digest(&t.blake3) {
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

#[cfg(test)]
pub(crate) fn fixture_identity() -> Identity {
	Identity::from_document(tests::document()).unwrap()
}
