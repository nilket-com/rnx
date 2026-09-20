//! Git-aware assembly identity. Path-only envelopes retain the original reader.
#![allow(dead_code)]
pub(crate) use crate::cache_identity::Context;
use crate::{fingerprint, input, inventory::Inventory, schemas::Declaration, wire};
use serde::{Deserialize, Serialize};

use std::{
	collections::BTreeSet,
	path::{Component, Path, PathBuf},
};

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
	git: Vec<crate::schemas::GitPackage>,
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
		manifest: &Declaration,
		base: &Path,
		mut context: Context,
		mut native: Inventory,
		mut git: Vec<crate::schemas::GitPackage>,
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
		if context.build.is_none() {
			return Err("Git-source assembly identity requires a build kind".into());
		}
		let (manifest, main) = crate::generate::git_wrapper(manifest, base)?;
		Self::from_document(Document {
			format: 4,
			generator: 4,
			context,
			manifest,
			main,
			cargo_lock_blake3: digest(cargo_lock),
			native,
			git: {
				git.sort_by(|a, b| a.id.cmp(&b.id));
				git
			},
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
	pub(crate) fn git_names(&self) -> Vec<String> {
		self.document.git.iter().map(|g| g.name.clone()).collect()
	}
	pub(crate) fn git(&self) -> &[crate::schemas::GitPackage] {
		&self.document.git
	}
	pub(crate) fn native(&self) -> &Inventory {
		&self.document.native
	}
	pub(crate) fn context(&self) -> &Context {
		&self.document.context
	}
	/// Which wrapper recipe this identity recorded (decision 5 of 0069).
	pub(crate) fn generator(&self) -> u32 {
		self.document.generator
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
	pub(crate) fn revalidate(&self, stage: &Path) -> Result<(), String> {
		self.verify(stage, true)
	}
	pub(crate) fn verify(&self, stage: &Path, full: bool) -> Result<(), String> {
		let c = self.context();
		let native = crate::git_inventory::replay(
			self.native(),
			self.git(),
			stage,
			&c.cache_root,
			&c.cargo_home,
			full,
		)?;
		crate::cache_storage::policy(&native)?;
		if &native != self.native() {
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
		// Format 3 predates sharing: no build kind, placeholder wrapper name.
		// Format 4 records the build kind and names the wrapper by digest.
		match (self.format, self.generator, &self.context.build) {
			(3, 3, None) => {}
			(4, 4, Some(build)) => {
				build.validate()?;
				// The recorded name must be the digest of the recorded wrapper.
				let name = crate::generate::executable_name(&self.manifest)?;
				let placeholder = self.manifest.replacen(
					&format!("name = {name:?}"),
					&format!("name = {:?}", crate::generate::PLACEHOLDER),
					1,
				);
				if name != crate::generate::wrapper_name(&placeholder, &self.main) {
					return Err("generator-four wrapper name does not match its content".into());
				}
			}
			_ => return Err("unsupported assembly identity/generator version".into()),
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
		if !valid_digest(&self.cargo_lock_blake3) {
			return Err("invalid Cargo lock digest".into());
		}
		let n = &self.native;
		if (n.packages.is_empty() && self.git.is_empty())
			|| self.git.len() > fingerprint::ENTRIES
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
		strictly_sorted(self.git.iter().map(|g| &g.id))?;
		let mut git_associations = BTreeSet::new();
		for g in &self.git {
			nonempty(&g.id)?;
			nonempty(&g.name)?;
			crate::schemas::git_coordinate(&g.url, &g.revision)?;
			let checkout = Path::new(&g.checkout);
			path(checkout)?;
			relative(&g.manifest)?;
			if Path::new(&g.manifest).file_name().and_then(|s| s.to_str()) != Some("Cargo.toml")
				|| checkout.starts_with(&c.cache_root)
				|| c.cache_root.starts_with(checkout)
				|| n.packages
					.iter()
					.any(|p| p.id == g.id || p.root.starts_with(checkout))
				|| n.trees
					.iter()
					.any(|t| t.root.starts_with(checkout) || checkout.starts_with(&t.root))
				|| !git_associations.insert((&g.checkout, &g.manifest))
			{
				return Err("invalid Git package association".into());
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
		for (alias, dep) in dependencies {
			let name = dep
				.get("package")
				.and_then(toml::Value::as_str)
				.unwrap_or(alias);
			let table = dep.as_table().ok_or("dependency table")?;
			for k in table.keys() {
				if ![
					"package",
					"path",
					"git",
					"rev",
					"features",
					"default-features",
				]
				.contains(&k.as_str())
				{
					return Err("unsupported wrapper dependency field".into());
				}
			}
			match (
				dep.get("path").and_then(toml::Value::as_str),
				dep.get("git").and_then(toml::Value::as_str),
				dep.get("rev").and_then(toml::Value::as_str),
			) {
				(Some(root), None, None) => {
					path(Path::new(root))?;
					if !n
						.packages
						.iter()
						.any(|p| p.root == Path::new(root) && p.name == name)
					{
						return Err("missing path association".into());
					}
				}
				(None, Some(url), Some(rev)) => {
					crate::schemas::git_coordinate(url, rev)?;
					if !self
						.git
						.iter()
						.any(|p| p.url == url && p.revision == rev && p.name == name)
					{
						return Err("missing Git association".into());
					}
				}
				_ => return Err("exactly path or git plus rev in wrapper".into()),
			}
		}

		for e in &n.external {
			path(&e.path)?;
			if self.git.iter().any(|g| e.path.starts_with(&g.checkout)) {
				return Err("Git-owned input in external inventory".into());
			}
			if let Some(f) = &e.file {
				file(f)?;
			}
		}
		Ok(())
	}
}

pub(crate) fn canonical(bytes: &[u8]) -> Result<Vec<u8>, String> {
	if bytes.len() > input::DOCUMENT_LIMIT {
		return Err("document allowance".into());
	}
	let d = serde_json::from_slice(bytes).map_err(|e| format!("identity: {e}"))?;
	Ok(Identity::from_document(d)?.bytes)
}

#[cfg(test)]
pub(crate) fn fixture(build: crate::cache_identity::Build) -> Identity {
	Identity::from_document(format_tests::current(build)).unwrap()
}

#[cfg(test)]
mod format_tests {
	use super::*;
	use crate::cache_identity::Build;
	/// A retained format-three document: placeholder wrapper, no build kind.
	fn retained() -> Document {
		let base = std::env::temp_dir().join("rnx-identity-shape");
		let native = base.join("native");
		let manifest = format!(
			"[package]\nname = \"rnx-project-app\"\n\n[dependencies.rnx]\npath = {}\n",
			toml::Value::String(native.to_str().unwrap().into())
		);
		Document {
			format: 3,
			generator: 3,
			context: Context {
				cache_root: base.join("cache"),
				cargo_home: base.join("cargo"),
				rustup_home: None,
				rustup_toolchain: None,
				rustc: "rustc fixture".into(),
				cargo: "cargo fixture".into(),
				target: "fixture-target".into(),
				profile: "release".into(),
				features: vec!["project-sources".into()],
				build: None,
			},
			manifest,
			main: "fn main() { rnx::main_with(rnx::Extensions::none()) }\n".into(),
			cargo_lock_blake3: "a".repeat(64),
			native: Inventory {
				platform: "fixture-platform".into(),
				packages: vec![crate::inventory::Association {
					id: "fixture-package-id".into(),
					name: "rnx".into(),
					manifest: native.join("Cargo.toml"),
					root: native.clone(),
				}],
				trees: vec![fingerprint::Tree {
					root: native.clone(),
					blake3: "b".repeat(64),
					files: vec![wire::File {
						path: "Cargo.toml".into(),
						bytes: 0,
						executable: false,
						blake3: "c".repeat(64),
					}],
				}],
				external: vec![],
			},
			git: vec![],
		}
	}
	/// The same wrapper as generator four names it.
	pub(super) fn current(build: Build) -> Document {
		let mut d = retained();
		d.format = 4;
		d.generator = 4;
		d.context.build = Some(build);
		let name = crate::generate::wrapper_name(&d.manifest, &d.main);
		d.manifest =
			d.manifest
				.replacen("name = \"rnx-project-app\"", &format!("name = {name:?}"), 1);
		d
	}
	#[test]
	fn retained_format_three_still_decodes_without_a_build_kind() {
		let old = Identity::from_document(retained()).unwrap();
		let again = Identity::decode(old.bytes()).unwrap();
		assert_eq!(old.key(), again.key());
		assert!(old.context().build.is_none());
		assert!(!String::from_utf8_lossy(old.bytes()).contains("\"build\""));
		assert_eq!(
			crate::generate::executable_name(old.wrapper().0).unwrap(),
			"rnx-project-app"
		);
	}
	#[test]
	fn format_four_requires_a_valid_build_kind_and_the_digest_name() {
		for build in [
			Build::Private,
			Build::Shared {
				key: "d".repeat(64),
			},
		] {
			let good = Identity::from_document(current(build.clone())).unwrap();
			let again = Identity::decode(good.bytes()).unwrap();
			assert_eq!(good.key(), again.key());
			assert_eq!(again.context().build.as_ref(), Some(&build));
			let name = crate::generate::executable_name(good.wrapper().0).unwrap();
			assert!(name.starts_with("rnx-app-") && name.len() == 72);
		}
		let mut d = current(Build::Private);
		d.context.build = None;
		assert!(
			Identity::from_document(d).is_err(),
			"format four without a build kind"
		);
		let mut d = retained();
		d.context.build = Some(Build::Private);
		assert!(
			Identity::from_document(d).is_err(),
			"format three with a build kind"
		);
		for (format, generator) in [(3, 4), (4, 3), (5, 5)] {
			let mut d = current(Build::Private);
			d.format = format;
			d.generator = generator;
			assert!(Identity::from_document(d).is_err(), "{format}/{generator}");
		}
		let mut d = current(Build::Private);
		d.main.push('\n');
		assert!(
			Identity::from_document(d).is_err(),
			"name must match the wrapper content"
		);
		let mut d = current(Build::Private);
		d.manifest = d.manifest.replacen("rnx-app-", "rnx-app-0", 1);
		assert!(Identity::from_document(d).is_err(), "a tampered name");
		let d = current(Build::Shared {
			key: "not-hex".into(),
		});
		assert!(
			Identity::from_document(d).is_err(),
			"a malformed shared key"
		);
	}
	#[test]
	fn equal_wrappers_share_a_name_and_a_one_call_difference_does_not() {
		let a = current(Build::Private);
		let mut b = current(Build::Private);
		b.main = b.main.replace(
			"rnx::Extensions::none()",
			"rnx::Extensions::none().with(\"x\", x::build)",
		);
		let b_name = crate::generate::wrapper_name(&retained().manifest, &b.main);
		b.manifest = retained().manifest.replacen(
			"name = \"rnx-project-app\"",
			&format!("name = {b_name:?}"),
			1,
		);
		let a = Identity::from_document(a).unwrap();
		let b = Identity::from_document(b).unwrap();
		assert_ne!(
			crate::generate::executable_name(a.wrapper().0).unwrap(),
			crate::generate::executable_name(b.wrapper().0).unwrap()
		);
		assert_ne!(a.key(), b.key());
	}
}
