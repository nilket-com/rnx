//! Format-two project workflow. Legacy envelopes stay on the original path.
use super::*;
use crate::{
	cache_entry,
	cache_identity::{self, Context},
	cache_storage as storage, git_inventory,
	new_identity::Identity,
	schemas::{Assembly, Declaration, Lock, Receipt},
};
impl Project {
	pub(super) fn new_manifest(&self) -> Result<bool, String> {
		let b = input::read(&self.manifest, input::MANIFEST_LIMIT)?;
		let v: toml::Value = toml::from_str(std::str::from_utf8(&b).map_err(err)?).map_err(err)?;
		Ok(v.get("format").and_then(toml::Value::as_integer) == Some(2))
	}
	pub(super) fn new_lock(&self) -> Result<bool, String> {
		let bytes = input::read(&self.base.join("rnx.lock"), input::DOCUMENT_LIMIT)
			.map_err(|e| self.recovery(e))?;
		Ok(crate::schemas::envelope(&bytes)? == 4)
	}
	pub(super) fn read_git_lock(&self) -> Result<(Lock, Vec<u8>), String> {
		let bytes = input::read(&self.base.join("rnx.lock"), input::DOCUMENT_LIMIT)
			.map_err(|e| self.recovery(e))?;
		let lock = Lock::decode(&bytes).map_err(|e| self.format_refusal("rnx.lock", &bytes, e))?;
		if let Some(i) = lock.shared()? {
			i.check_lock(&input::read(
				&self.base.join("rnx.Cargo.lock"),
				input::DOCUMENT_LIMIT,
			)?)
			.map_err(|e| format!("lock pair mismatch: {e}; run lock"))?;
		}
		Ok((lock, bytes))
	}
	pub(super) fn git_verify_inputs(&self, lock: &Lock, full: bool) -> Result<(), String> {
		if Declaration::read(&self.manifest)? != lock.declarations
			|| Handoff::from_project(&self.manifest)? != lock.sources
		{
			return Err("project declarations or source map changed; run lock".into());
		}
		self.layout(&lock.sources)?;
		let source = inventory::sources(&self.manifest, &mut Allowance::default())?;
		if source != lock.inputs.source {
			return Err("project source changed; run lock".into());
		}
		if let Some(i) = lock.shared()? {
			let c = i.context();
			if storage::root(&c.cache_root)? != c.cache_root {
				return Err("cache root moved; run lock".into());
			}
			storage::project_context(&c.cache_root, &self.base)?;
			cache_entry::environment(&i)?;
			let stage = c.cache_root.join("entries").join(i.key()).join("assembly");
			i.verify(&stage, full).map_err(|e| self.recovery(e))?;
			shared::compatible_layout(&c.cache_root, &lock.inputs, &self.base)?;
			// The recipe the lock recorded, not the one a newer tool would
			// write: a retained generator-three lock keeps launching and
			// rebuilding its placeholder-named wrapper.
			let (manifest, main) =
				generate::git_wrapper_for(i.generator(), &lock.declarations, &self.base)?;
			if i.wrapper() != (manifest.as_str(), main.as_str()) {
				return Err("generated shared assembly changed; run lock".into());
			}
		} else if lock.inputs.native.is_some() {
			return Err("override has native inputs".into());
		}
		commands::check()
	}
	pub(super) fn git_lock(&self, offline: bool) -> Result<(), String> {
		let declarations = Declaration::read(&self.manifest)?;
		let sources = Handoff::from_project(&self.manifest)?;
		self.layout(&sources)?;
		let before_source = inventory::sources(&self.manifest, &mut Allowance::default())?;
		let (inputs, assembly, cargo_bytes) = if let Some(executable) = &declarations.executable {
			let path = self.base.join(&executable.path);
			let blake3 = assembly::executable_hash(&path)?;
			if !sources.mounts.is_empty() {
				crate::handshake::check(&path)?;
			}
			(
				self.inputs(None)?,
				Assembly::Executable {
					path: path
						.to_str()
						.ok_or("executable path is not Unicode")?
						.into(),
					blake3,
				},
				None,
			)
		} else {
			self.resolve_git(&declarations, offline)?
		};
		if inputs.source != before_source {
			return Err("source changed during lock; retry lock".into());
		}
		let git = match &assembly {
			Assembly::Shared { identity } => Identity::decode(identity.as_bytes())?.git().to_vec(),
			_ => vec![],
		};
		let lock = Lock {
			git,
			format: 4,
			declarations,
			sources,
			inputs,
			assembly,
		};
		self.git_verify_inputs(&lock, true)?;
		lock.validate()?;
		let bytes = wire::pretty(&lock)?;
		self.publish_pair(&bytes, cargo_bytes.as_deref())?;

		if matches!(lock.assembly, Assembly::Executable { .. })
			&& self.base.join("rnx.Cargo.lock").exists()
		{
			fs::remove_file(self.base.join("rnx.Cargo.lock")).map_err(err)?;
		}
		eprintln!("locked {}", self.manifest.display());
		Ok(())
	}
	pub(super) fn git_checked_artifact(
		&self,
		lock: &Lock,
		bytes: &[u8],
		verify: bool,
		refresh: bool,
	) -> Result<(artifact::Checked, String), String> {
		let lock_digest = hash(bytes);
		let receipt_path = self.dot.join("receipt.json");
		let receipt = match fs::symlink_metadata(&receipt_path) {
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
			Err(e) => return Err(err(e)),
			Ok(m) if !m.is_file() => return Err("receipt is not a regular file".into()),
			Ok(_) => {
				let bytes = input::read(&receipt_path, input::DOCUMENT_LIMIT)?;
				Some(
					Receipt::decode(&bytes)
						.map_err(|e| self.format_refusal("receipt", &bytes, e))?,
				)
			}
		};
		let identity = lock.shared()?;
		let (path, digest, stamp) = match &lock.assembly {
			Assembly::Shared { .. } => {
				let identity = identity.as_ref().unwrap();
				let r = receipt
					.as_ref()
					.ok_or("missing receipt; run build to attach shared assembly")?;
				if r.assembly_key.is_none()
					|| r.lock_blake3 != lock_digest
					|| r.assembly_key.as_deref() != Some(identity.key())
				{
					return Err("shared receipt does not match lock; run build".into());
				}
				let (path, digest) = crate::cache_entry::ready(identity)?;
				if r.executable_blake3 != digest {
					return Err("shared receipt does not match ready artifact; run build".into());
				}
				(path, digest, r.stamp.as_ref())
			}
			Assembly::Executable { path, blake3 } => {
				let r = receipt
					.as_ref()
					.ok_or_else(|| self.recovery("missing override receipt"))?;
				if r.assembly_key.is_some()
					|| r.lock_blake3 != lock_digest
					|| r.executable_blake3 != *blake3
				{
					return Err(self.recovery("override receipt does not match lock"));
				}
				(PathBuf::from(path), blake3.clone(), r.stamp.as_ref())
			}
		};
		let checked = artifact::check(&path, &digest, stamp, verify)?;
		if refresh && stamp != Some(&checked.stamp()) {
			fault("before-receipt-refresh")?;
			checked.recheck()?;
			self.atomic(
				&receipt_path,
				&wire::pretty(&Receipt {
					format: 5,
					assembly_key: identity.as_ref().map(|i| i.key().to_owned()),
					lock_blake3: lock_digest,
					executable_blake3: digest.clone(),
					stamp: Some(checked.stamp()),
				})?,
			)?;
		}

		Ok((checked, digest))
	}
	pub(super) fn git_launch(self, mode: Launch, verify: bool) -> Result<(), String> {
		let (lock, bytes) = self.read_git_lock()?;
		self.git_verify_inputs(&lock, verify)?;
		let (checked, digest) = self.git_checked_artifact(&lock, &bytes, verify, true)?;

		let mut command = match mode {
			Launch::Run(args) => {
				let maps = self.dot.join("maps");
				fs::create_dir_all(&maps).map_err(err)?;
				let map_bytes = lock.sources.encode()?;
				let map = maps.join(format!("{}.json", hash(&map_bytes)));
				crate::maps::ensure(&map, &map_bytes, || self.atomic(&map, &map_bytes))?;
				let mut command = assembly::command_checked(
					&checked,
					Some(&map),
					Path::new(&lock.sources.entry),
					&args,
				)?;
				command.env_remove(transition::CARRIER);
				command
			}
			Launch::Session { flags } => {
				let mut command = assembly::interactive_checked(&checked, &flags, None);
				command.env(
					transition::CARRIER,
					transition::git_association(&self, &lock, &checked, &digest)?,
				);
				command
			}
			Launch::Eval { flags, source } => {
				let mut command = assembly::interactive_checked(&checked, &flags, Some(&source));
				command.env_remove(transition::CARRIER);
				command
			}
		};
		commands::check()?;
		// Advisory lock descriptors are close-on-exec; the script is not a project
		// writer. The immutable map/artifact outlive this process without a parent.
		#[cfg(unix)]
		{
			use std::os::unix::process::CommandExt;
			Err(command.exec().to_string())
		}
		#[cfg(not(unix))]
		{
			let _ = &mut command;
			Err("run supervision is not implemented on this platform".into())
		}
	}

	pub(super) fn git_build(&self, offline: bool) -> Result<(), String> {
		let (lock, bytes) = self.read_git_lock()?;
		let receipt = self.dot.join("receipt.json");
		if receipt.exists() {
			fs::remove_file(&receipt).map_err(err)?;
		}
		self.git_verify_inputs(&lock, true)?;
		let recheck = || -> Result<(), String> {
			self.git_verify_inputs(&lock, true)?;
			if self.read_git_lock()?.1 != bytes {
				return Err("lock changed during build; no receipt published".into());
			}
			Ok(())
		};
		let (checked, digest, key, _entry) = match &lock.assembly {
			Assembly::Executable { path, blake3 } => (
				artifact::check(Path::new(path), blake3, None, true)?,
				blake3.clone(),
				None,
				None,
			),
			Assembly::Shared { .. } => {
				let i = lock.shared()?.unwrap();
				let cargo = input::read(&self.base.join("rnx.Cargo.lock"), input::DOCUMENT_LIMIT)?;
				let entry = cache_entry::acquire(&i, &cargo, &self.base, offline, recheck)?;
				let checked = artifact::check(
					entry.artifact.path(),
					&entry.digest,
					Some(&entry.artifact.stamp()),
					false,
				)?;
				(
					checked,
					entry.digest.clone(),
					Some(i.key().to_owned()),
					Some(entry),
				)
			}
		};
		fault("after-build")?;
		recheck()?;
		fault("before-shared-receipt")?;
		recheck()?;
		checked.recheck()?;
		self.atomic(
			&receipt,
			&wire::pretty(&Receipt {
				format: 5,
				assembly_key: key,
				lock_blake3: hash(&bytes),
				executable_blake3: digest,
				stamp: Some(checked.stamp()),
			})?,
		)?;
		eprintln!("built or attached {}", checked.path().display());
		Ok(())
	}
}

impl Project {
	pub(super) fn resolve_git(
		&self,
		declarations: &Declaration,
		offline: bool,
	) -> Result<(Inputs, Assembly, Option<Vec<u8>>), String> {
		let root = shared::selected_root()?;
		let source = inventory::sources(&self.manifest, &mut Allowance::default())?;
		shared::compatible_layout(
			&root,
			&Inputs {
				source,
				native: None,
			},
			&self.base,
		)?;
		storage::project_context(&root, &self.base)?;
		let resolver = shared::resolver(&root)?;
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
		let (manifest, main) = generate::git_wrapper(declarations, &self.base)?;
		fs::write(stage.join("Cargo.toml"), &manifest).map_err(err)?;
		fs::write(stage.join("src/main.rs"), &main).map_err(err)?;
		let seeded_by_project =
			match input::read(&self.base.join("rnx.Cargo.lock"), input::DOCUMENT_LIMIT) {
				Ok(old) => {
					fs::write(stage.join("Cargo.lock"), old).map_err(err)?;
					true
				}
				Err(_) => false,
			};
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
			let observation = git_inventory::acquisition::Observation::begin(&home)?;
			let graph = commands::run(cmd, true)
				.map_err(|e| format!("Cargo Git acquisition failed: {e}"))?;
			observation.report(&git_inventory::packages(&graph)?)?;
			Ok(graph)
		};
		let mut graph = metadata(false)?;
		// Record 0069, decision 4: a project without a previous lock starts
		// from the runtime's own Cargo.lock, so assemblies under one root and
		// runtime revision resolve alike where their constraints allow. The
		// first pass acquired the checkout that holds it; the second resolves
		// with it. A preference only: the lock digest identifies the result.
		if !seeded_by_project && let Some(seed) = runtime_lock(&graph)? {
			fs::write(stage.join("Cargo.lock"), seed).map_err(err)?;
			graph = metadata(false)?;
		}
		let source = inventory::sources(&self.manifest, &mut Allowance::default())?;
		let (native, git) =
			git_inventory::resolve(&graph, &stage, &root, &home, &mut Allowance::default())?;
		let inputs = Inputs {
			source,
			native: Some(native),
		};
		shared::compatible_layout(&root, &inputs, &self.base)?;
		if metadata(true)? != graph {
			return Err("Cargo graph changed during lock; retry lock".into());
		}
		let lock = input::read(&stage.join("Cargo.lock"), input::DOCUMENT_LIMIT)?;
		let (rustc, cargo, target) = versions(&stage)?;
		let mut context = Context {
			cache_root: root,
			cargo_home: home,
			rustup_home: std::env::var_os("RUSTUP_HOME").map(PathBuf::from),
			rustup_toolchain: std::env::var("RUSTUP_TOOLCHAIN").ok(),
			rustc,
			cargo,
			target,
			profile: "release".into(),
			features: vec!["project-sources".into()],
			build: None,
		};
		// Record 0069, one eligibility rule, decided here and nowhere else: a
		// Git runtime, every native Git, every native declaring shared_build.
		context.build = Some(if shared_build_eligible(declarations) {
			let settings =
				cache_identity::admitted_settings(&context, inputs.native.as_ref().unwrap())?;
			cache_identity::Build::Shared {
				key: cache_identity::build_key(&context, &settings),
			}
		} else {
			cache_identity::Build::Private
		});
		let identity = Identity::create(
			declarations,
			&self.base,
			context,
			inputs.native.clone().unwrap(),
			git,
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
}

/// Record 0069's one eligibility rule: a Git runtime, every native a Git
/// declaration, every native declaring `shared_build`. A path native with the
/// declaration disqualifies; a Git runtime with no natives qualifies.
pub(crate) fn shared_build_eligible(declarations: &Declaration) -> bool {
	declarations
		.runtime
		.as_ref()
		.is_some_and(|r| r.git.is_some())
		&& declarations
			.native
			.values()
			.all(|n| n.git.is_some() && n.shared_build)
}

/// The runtime's `Cargo.lock` from the checkout Cargo acquired for the `rnx`
/// package of a metadata graph, when it has one.
fn runtime_lock(graph: &[u8]) -> Result<Option<Vec<u8>>, String> {
	#[derive(serde::Deserialize)]
	struct Package {
		name: String,
		source: Option<String>,
		manifest_path: PathBuf,
	}
	#[derive(serde::Deserialize)]
	struct Metadata {
		packages: Vec<Package>,
	}
	let metadata: Metadata =
		serde_json::from_slice(graph).map_err(|e| format!("Cargo metadata: {e}"))?;
	let Some(runtime) = metadata
		.packages
		.iter()
		.find(|p| p.name == "rnx" && p.source.as_deref().is_some_and(|s| s.starts_with("git+")))
	else {
		return Ok(None);
	};
	let path = runtime
		.manifest_path
		.parent()
		.ok_or("runtime manifest has no parent")?
		.join("Cargo.lock");
	match input::read(&path, input::DOCUMENT_LIMIT) {
		Ok(bytes) => Ok(Some(bytes)),
		Err(_) => Ok(None),
	}
}

#[cfg(test)]
mod eligibility_tests {
	use super::*;
	use crate::schemas::{Declaration, Native};
	fn declaration(runtime_git: bool, natives: &[(bool, bool)]) -> Declaration {
		let mut d: Declaration = serde_json::from_value(serde_json::json!({
			"format": 2, "application": {"entry": "main.rn"},
			"runtime": if runtime_git { serde_json::json!({"git": "https://example.invalid/rnx", "rev": "0123456789abcdef0123456789abcdef01234567"}) } else { serde_json::json!({"path": "../rnx"}) },
		}))
		.unwrap();
		for (i, (git, shared)) in natives.iter().enumerate() {
			d.native.insert(
				format!("n{i}"),
				Native {
					path: (!git).then(|| "../adapter".into()),
					git: git.then(|| "https://example.invalid/rnx".into()),
					rev: git.then(|| "0123456789abcdef0123456789abcdef01234567".into()),
					package: "rnx-n".into(),
					builder: "build".into(),
					hook: crate::manifest::Hook::Plain,
					presentation: false,
					shared_build: *shared,
				},
			);
		}
		d
	}
	#[test]
	fn one_rule_git_runtime_git_natives_all_declared() {
		assert!(shared_build_eligible(&declaration(
			true,
			&[(true, true), (true, true)]
		)));
		assert!(shared_build_eligible(&declaration(true, &[])));
		assert!(!shared_build_eligible(&declaration(
			true,
			&[(true, true), (true, false)]
		)));
		assert!(!shared_build_eligible(&declaration(true, &[(false, true)])));
		assert!(!shared_build_eligible(&declaration(false, &[(true, true)])));
		assert!(!shared_build_eligible(&declaration(false, &[])));
	}
}
