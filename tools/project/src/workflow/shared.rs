//! Shared assemblies are selected only by an explicit format-two lock.
use super::*;
use crate::{
	cache_entry,
	cache_identity::{Context, Identity},
	cache_storage as storage,
};

struct Resolver(PathBuf);
impl Drop for Resolver {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}
fn selected_root() -> Result<PathBuf, String> {
	let root = if let Some(p) = std::env::var_os("RNX_PROJECT_CACHE") {
		PathBuf::from(p)
	} else if let Some(p) = std::env::var_os("XDG_CACHE_HOME") {
		PathBuf::from(p).join("rnx/assemblies")
	} else {
		PathBuf::from(std::env::var_os("HOME").ok_or("no HOME or cache selection")?)
			.join(".cache/rnx/assemblies")
	};
	if !root.is_absolute() || root.to_str().is_none() {
		return Err("cache root must be an absolute Unicode path".into());
	}
	let mut builder = fs::DirBuilder::new();
	builder.recursive(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;
		builder.mode(0o700);
	}
	builder.create(&root).map_err(err)?;
	storage::root(&root)
}
fn resolver(root: &Path) -> Result<Resolver, String> {
	storage::directory(&root.join("resolve"))?;
	for n in 0..1000 {
		let dir = root
			.join("resolve")
			.join(format!("{}-{n}", std::process::id()));
		let builder = fs::DirBuilder::new();
		#[cfg(unix)]
		let builder = {
			let mut builder = builder;
			use std::os::unix::fs::DirBuilderExt;
			builder.mode(0o700);
			builder
		};
		match builder.create(&dir) {
			Ok(()) => return Ok(Resolver(dir)),
			Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
			Err(e) => return Err(err(e)),
		}
	}
	Err("cannot allocate private cache resolver".into())
}
fn compatible_layout(root: &Path, inputs: &Inputs, project: &Path) -> Result<(), String> {
	for tree in inputs
		.source
		.trees
		.iter()
		.chain(inputs.native.iter().flat_map(|n| n.trees.iter()))
	{
		if root.starts_with(&tree.root) || tree.root.starts_with(root) {
			return Err(format!(
				"cache and fingerprinted root {} must not contain one another",
				tree.root.display()
			));
		}
	}
	if let Some(n) = &inputs.native
		&& n.trees.iter().any(|t| project.starts_with(&t.root))
	{
		return Err("project lock would be inside native package root; move the project".into());
	}
	Ok(())
}
impl Project {
	fn shared_inputs(
		&self,
		metadata: &[u8],
		stage: &Path,
		root: &Path,
		home: &Path,
	) -> Result<Inputs, String> {
		self.layout(&Handoff::from_project(&self.manifest)?)?;
		let mut allowance = Allowance::default();
		let source = inventory::sources(&self.manifest, &mut allowance)?;
		let native = inventory::native(metadata, stage, root, home, &mut allowance)?;
		storage::policy(&native)?;
		let inputs = Inputs {
			source,
			native: Some(native),
		};
		compatible_layout(root, &inputs, &self.base)?;
		Ok(inputs)
	}
	pub(super) fn verify_shared(&self, lock: &Lock, identity: &Identity) -> Result<(), String> {
		let c = identity.context();
		if storage::root(&c.cache_root)? != c.cache_root {
			return Err("cache root moved; run lock".into());
		}
		storage::project_context(&c.cache_root, &self.base)?;
		cache_entry::environment(identity)?;
		let stage = c
			.cache_root
			.join("entries")
			.join(identity.key())
			.join("assembly");
		// A launch never calls Cargo. The graph associations were resolved by lock.
		let metadata = wire::encode(
			&serde_json::json!({"workspace_root":stage,"packages":identity.native().packages.iter().map(|p|serde_json::json!({"id":p.id,"name":p.name,"source":null,"manifest_path":p.manifest})).collect::<Vec<_>>()}),
		)?;
		if self.shared_inputs(&metadata, &stage, &c.cache_root, &c.cargo_home)? != lock.inputs {
			return Err("project source or Cargo input changed; run lock".into());
		}
		let (manifest, main) = generate::canonical_wrapper(&lock.declarations, &self.base)?;
		if identity.wrapper() != (manifest.as_str(), main.as_str()) {
			return Err("generated shared assembly changed; run lock".into());
		}
		commands::check()
	}
	pub(super) fn resolve_shared(
		&self,
		declarations: &Manifest,
		offline: bool,
	) -> Result<(Inputs, Assembly, Option<Vec<u8>>), String> {
		let root = selected_root()?;
		let source = inventory::sources(&self.manifest, &mut Allowance::default())?;
		compatible_layout(
			&root,
			&Inputs {
				source,
				native: None,
			},
			&self.base,
		)?;
		storage::project_context(&root, &self.base)?;
		let resolver = resolver(&root)?;
		let stage = resolver.0.join("assembly");
		storage::directory(&stage)?;
		storage::directory(&stage.join("src"))?;
		storage::guard(&root, &stage, &self.base)?;
		let home = cargo_home()?.canonicalize().map_err(err)?;
		let empty = wire::encode(&serde_json::json!({"workspace_root":stage,"packages":[]}))?;
		storage::policy(&inventory::native(
			&empty,
			&stage,
			&root,
			&home,
			&mut Allowance::default(),
		)?)?;
		let (manifest, main) = generate::canonical_wrapper(declarations, &self.base)?;
		fs::write(stage.join("Cargo.toml"), &manifest).map_err(err)?;
		fs::write(stage.join("src/main.rs"), &main).map_err(err)?;
		if let Ok(old) = input::read(&self.base.join("rnx.Cargo.lock"), input::DOCUMENT_LIMIT) {
			fs::write(stage.join("Cargo.lock"), old).map_err(err)?;
		}
		let metadata = |locked: bool| -> Result<Vec<u8>, String> {
			storage::guard(&root, &stage, &self.base)?;
			let mut cmd = Command::new("cargo");
			cmd.current_dir(&stage)
				.args(["metadata", "--format-version", "1"]);
			if locked {
				cmd.arg("--locked");
			}
			if offline {
				cmd.arg("--offline");
			}
			commands::run(cmd, true)
		};
		let graph = metadata(false)?;
		let inputs = self.shared_inputs(&graph, &stage, &root, &home)?;
		if metadata(true)? != graph {
			return Err("Cargo graph changed during lock; retry lock".into());
		}
		let lock = input::read(&stage.join("Cargo.lock"), input::DOCUMENT_LIMIT)?;
		let (rustc, cargo, target) = versions(&stage)?;
		let context = Context {
			cache_root: root,
			cargo_home: home,
			rustup_home: std::env::var_os("RUSTUP_HOME").map(PathBuf::from),
			rustup_toolchain: std::env::var("RUSTUP_TOOLCHAIN").ok(),
			rustc,
			cargo,
			target,
			profile: "release".into(),
			features: vec!["project-sources".into()],
		};
		let identity = Identity::create(
			declarations,
			&self.base,
			context,
			inputs.native.clone().unwrap(),
			&lock,
		)?;
		identity.revalidate(&stage)?;
		Ok((
			inputs,
			Assembly::Shared {
				identity: String::from_utf8(identity.bytes().to_vec()).map_err(err)?,
			},
			Some(lock),
		))
	}
	pub(super) fn build_shared(
		&self,
		lock: &Lock,
		bytes: &[u8],
		identity: &Identity,
		offline: bool,
	) -> Result<(), String> {
		let cargo = input::read(&self.base.join("rnx.Cargo.lock"), input::DOCUMENT_LIMIT)?;
		let recheck = || -> Result<(), String> {
			self.verify_inputs(lock)?;
			if self.read_lock()?.1 != bytes {
				return Err("lock changed during build; no receipt published".into());
			}
			Ok(())
		};
		let entry = cache_entry::acquire(identity, &cargo, &self.base, offline, recheck)?;
		fault("after-build")?;
		recheck()?;
		fault("before-shared-receipt")?;
		recheck()?;
		entry.artifact.recheck()?;
		self.atomic(
			&self.dot.join("receipt.json"),
			&wire::pretty(&Receipt {
				format: 3,
				assembly_key: Some(identity.key().into()),
				lock_sha256: hash(bytes),
				executable_sha256: entry.digest.clone(),
				stamp: Some(entry.artifact.stamp()),
			})?,
		)?;
		eprintln!(
			"{} shared assembly {}: {}",
			if entry.hit { "attached" } else { "built" },
			identity.key(),
			entry.artifact.path().display()
		);
		Ok(())
	}
}
