mod add;
mod shared;
mod startup;
mod transition;
use crate::artifact::{self, Receipt};
use crate::{
	assembly, commands,
	fingerprint::Allowance,
	generate, input, inventory,
	manifest::Manifest,
	wire::{self, Assembly, Handoff, Inputs, Lock},
};
use sha2::{Digest, Sha256};
use std::{
	ffi::OsString,
	fs::{self, File, OpenOptions},
	io::Write,
	path::{Path, PathBuf},
	process::Command,
};
fn hash(bytes: &[u8]) -> String {
	format!("{:x}", Sha256::digest(bytes))
}
fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
}
struct Project {
	manifest: PathBuf,
	base: PathBuf,
	dot: PathBuf,
	stage: PathBuf,
	_guard: Option<File>,
}
impl Project {
	fn open(path: &Path) -> Result<Self, String> {
		let manifest = path.canonicalize().map_err(err)?;
		let base = manifest
			.parent()
			.ok_or("manifest has no directory")?
			.to_owned();
		let dot = base.join(".rnx");
		let mut directory = fs::DirBuilder::new();
		directory.recursive(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::DirBuilderExt;
			directory.mode(0o700);
		}
		directory.create(&dot).map_err(err)?;
		if fs::symlink_metadata(&dot)
			.map_err(err)?
			.file_type()
			.is_symlink()
		{
			return Err(".rnx must not be a symlink".into());
		}
		let mut options = OpenOptions::new();
		options.read(true).write(true).create(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options
				.mode(0o600)
				.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
		}
		let guard = options.open(dot.join("command.lock")).map_err(err)?;
		if !guard.metadata().map_err(err)?.is_file() {
			return Err("command lock is not regular".into());
		}
		guard
			.try_lock()
			.map_err(|_| "another project command is active")?;
		fs::write(dot.join(".gitignore"), "*\n").map_err(err)?;
		Ok(Self {
			manifest,
			base,
			dot: dot.clone(),
			stage: dot.join("assembly"),
			_guard: Some(guard),
		})
	}
	fn atomic(&self, destination: &Path, bytes: &[u8]) -> Result<(), String> {
		commands::check()?;
		let temp = self.dot.join("publication.new");
		let mut f = File::create(&temp).map_err(err)?;
		f.write_all(bytes).map_err(err)?;
		f.sync_all().map_err(err)?;
		drop(f);
		commands::check()?;
		fs::rename(&temp, destination).map_err(err)?;
		#[cfg(unix)]
		File::open(destination.parent().unwrap())
			.and_then(|d| d.sync_all())
			.map_err(err)?;
		Ok(())
	}
	fn read_lock(&self) -> Result<(Lock, Vec<u8>), String> {
		let bytes = input::read(&self.base.join("rnx.lock"), input::DOCUMENT_LIMIT)
			.map_err(|e| format!("{e}; run lock"))?;
		let lock = Lock::decode(&bytes).map_err(|e| format!("invalid rnx.lock: {e}; run lock"))?;
		if !matches!(lock.assembly, Assembly::Executable { .. }) != lock.inputs.native.is_some() {
			return Err("lock assembly and native inventory disagree; run lock".into());
		}
		if let Assembly::Generated {
			cargo_lock_sha256, ..
		} = &lock.assembly
		{
			let cargo = input::read(&self.base.join("rnx.Cargo.lock"), input::DOCUMENT_LIMIT)?;
			if hash(&cargo) != *cargo_lock_sha256 {
				return Err("lock pair mismatch; run lock (run/build never repair it)".into());
			}
		}
		if let Some(identity) = lock.shared()? {
			identity
				.check_lock(&input::read(
					&self.base.join("rnx.Cargo.lock"),
					input::DOCUMENT_LIMIT,
				)?)
				.map_err(|e| format!("lock pair mismatch: {e}; run lock"))?;
		}
		Ok((lock, bytes))
	}
	fn cargo(&self, operation: &str, offline: bool) -> Command {
		let mut command = Command::new("cargo");
		command
			.current_dir(&self.base)
			.arg(operation)
			.arg("--manifest-path")
			.arg(self.stage.join("Cargo.toml"));
		if offline {
			command.arg("--offline");
		}
		command
	}
	fn generate(&self) -> Result<(), String> {
		if self.stage.exists() {
			fs::remove_dir_all(&self.stage).map_err(err)?;
		}
		assembly::prepare(&self.manifest, &self.stage)
	}
	fn layout(&self, map: &Handoff) -> Result<(), String> {
		let entry_root = Path::new(&map.entry)
			.parent()
			.ok_or("entry has no directory")?
			.canonicalize()
			.map_err(err)?;
		let mut roots = vec![entry_root.clone()];
		for mount in &map.mounts {
			roots.push(Path::new(&mount.root).canonicalize().map_err(err)?);
		}
		for root in roots {
			if self.base.starts_with(&root) && !(root == self.base && entry_root == self.base) {
				return Err(format!(
					"project outputs would be inside source root {}; place the manifest outside that root or at its entry root to avoid self-referential fingerprints",
					root.display()
				));
			}
		}
		Ok(())
	}
	fn inputs(&self, metadata: Option<&[u8]>) -> Result<Inputs, String> {
		self.layout(&Handoff::from_project(&self.manifest)?)?;
		let mut allowance = Allowance::default();
		let source = inventory::sources(&self.manifest, &mut allowance)?;
		let native = metadata
			.map(|m| inventory::native(m, &self.stage, &self.base, &cargo_home()?, &mut allowance))
			.transpose()?;
		if let Some(n) = &native {
			if let Some(root) = n.trees.iter().find(|t| self.base.starts_with(&t.root)) {
				return Err(format!(
					"project lock would be inside native package root {}; place the project outside that root to avoid a self-referential fingerprint",
					root.root.display()
				));
			}
			config_policy(n)?;
		}
		Ok(Inputs { source, native })
	}
	fn metadata_for(&self, lock: &Lock) -> Result<Option<Vec<u8>>, String> {
		lock.inputs.native.as_ref().map(|n|wire::encode(&serde_json::json!({"workspace_root":self.stage,"packages":n.packages.iter().map(|p|serde_json::json!({"id":p.id,"name":p.name,"source":null,"manifest_path":p.manifest})).collect::<Vec<_>>()}))).transpose()
	}
	fn verify_inputs(&self, lock: &Lock) -> Result<(), String> {
		let current = Manifest::read(&self.manifest)?;
		if current != lock.declarations || Handoff::from_project(&self.manifest)? != lock.sources {
			return Err("project declarations or source map changed; run lock".into());
		}
		if let Some(identity) = lock.shared()? {
			return self.verify_shared(lock, &identity);
		}
		let metadata = self.metadata_for(lock)?;
		if self.inputs(metadata.as_deref())? != lock.inputs {
			return Err("project source or Cargo input changed; run lock".into());
		}
		match &lock.assembly {
			Assembly::Generated {
				manifest_sha256,
				main_sha256,
				..
			} => {
				let (cargo, main) = generate::wrapper(&current, &self.base)?;
				if hash(cargo.as_bytes()) != *manifest_sha256
					|| hash(main.as_bytes()) != *main_sha256
				{
					return Err("generated assembly changed; run lock".into());
				}
			}
			Assembly::Executable { .. } => (),
			Assembly::Shared { .. } => unreachable!(),
		}
		commands::check()
	}
	fn lock(&self, offline: bool) -> Result<(), String> {
		let declarations = Manifest::read(&self.manifest)?;
		let sources = Handoff::from_project(&self.manifest)?;
		self.layout(&sources)?;
		let before_source = inventory::sources(&self.manifest, &mut Allowance::default())?;
		let (inputs, assembly, cargo_bytes) = if let Some(executable) = &declarations.executable {
			let path = self.base.join(&executable.path);
			let sha256 = assembly::executable_hash(&path)?;
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
					sha256,
				},
				None,
			)
		} else {
			self.resolve_shared(&declarations, offline)?
		};
		if inputs.source != before_source {
			return Err("source changed during lock; retry lock".into());
		}
		let lock = Lock {
			format: if matches!(assembly, Assembly::Shared { .. }) {
				2
			} else {
				1
			},
			declarations,
			sources,
			inputs,
			assembly,
		};
		self.verify_inputs(&lock)?;
		lock.validate()?;
		let bytes = wire::pretty(&lock)?;
		// Cargo first, JSON last: rnx.lock is the commit marker for the pair. A
		// crash between renames is a detectable mismatch, never an accepted new lock
		// paired with old Cargo bytes. Keep the old pair privately for normal errors.

		let cargo_path = self.base.join("rnx.Cargo.lock");
		let project_path = self.base.join("rnx.lock");
		let old = |p: &Path| -> Result<Option<Vec<u8>>, String> {
			if p.exists() {
				Ok(Some(input::read(p, input::DOCUMENT_LIMIT)?))
			} else {
				Ok(None)
			}
		};
		let old_cargo = old(&cargo_path)?;
		let old_project = old(&project_path)?;
		let publish = || -> Result<(), String> {
			if let Some(cargo) = &cargo_bytes {
				self.atomic(&cargo_path, cargo)?;
			}
			fault("after-cargo-publication")?;
			self.atomic(&project_path, &bytes)?;
			fault("after-json-publication")
		};
		if let Err(e) = publish() {
			if !commands::interrupted() {
				let restore = |p: &Path, b: &Option<Vec<u8>>| -> Result<(), String> {
					if let Some(b) = b {
						self.atomic(p, b)
					} else {
						match fs::remove_file(p) {
							Ok(()) => Ok(()),
							Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
							Err(e) => Err(err(e)),
						}
					}
				};
				// Restore the old commit marker first. If that fails, leave
				// new Cargo bytes in place rather than pairing new JSON with old Cargo.
				restore(&project_path, &old_project)
					.and_then(|_| restore(&cargo_path, &old_cargo))
					.map_err(|recovery| {
						format!("{e}; restoring the old pair failed: {recovery}; run lock")
					})?;
			}
			return Err(e);
		}

		if matches!(lock.assembly, Assembly::Executable { .. }) && cargo_path.exists() {
			fs::remove_file(cargo_path).map_err(err)?;
		}
		eprintln!("locked {}", self.manifest.display());
		Ok(())
	}
	fn build(&self, offline: bool) -> Result<(), String> {
		let receipt = self.dot.join("receipt.json");
		if receipt.exists() {
			fs::remove_file(&receipt).map_err(err)?;
		}
		let (lock, bytes) = self.read_lock()?;
		self.verify_inputs(&lock)?;
		if let Some(identity) = lock.shared()? {
			return self.build_shared(&lock, &bytes, &identity, offline);
		}
		let Assembly::Generated {
			target,
			profile,
			features,
			rustc,
			cargo,
			..
		} = &lock.assembly
		else {
			return Err("executable overrides are not built; use run".into());
		};
		let versions = versions(&self.base)?;
		if versions.0 != *rustc
			|| versions.1 != *cargo
			|| versions.2 != *target
			|| profile != "release"
			|| features != &["project-sources"]
		{
			return Err("toolchain or build flags changed; run lock".into());
		}
		self.generate()?;
		fs::write(
			self.stage.join("Cargo.lock"),
			input::read(&self.base.join("rnx.Cargo.lock"), input::DOCUMENT_LIMIT)?,
		)
		.map_err(err)?;
		preflight(&self.base, &self.stage)?;
		let mut command = self.cargo("build", offline);
		command
			.args(["--locked", "--release", "--target-dir"])
			.arg(self.dot.join("target"));
		commands::run(command, false)?;
		fault("after-build")?;
		self.verify_inputs(&lock)?;
		let (again, again_bytes) = self.read_lock()?;
		if again_bytes != bytes || again != lock {
			return Err("lock changed during build; no receipt published".into());
		}
		let executable = self
			.dot
			.join("target")
			.join("release")
			.join(format!("rnx-project-app{}", std::env::consts::EXE_SUFFIX));
		let digest = assembly::executable_hash(&executable)?;
		let artifacts = self.dot.join("artifacts");
		fs::create_dir_all(&artifacts).map_err(err)?;
		let artifact = artifacts.join(&digest);
		let temp = artifacts.join("new");
		fs::copy(&executable, &temp).map_err(err)?;
		assembly::verify(&temp, &digest)?;
		File::open(&temp).and_then(|f| f.sync_all()).map_err(err)?;
		fs::rename(temp, &artifact).map_err(err)?;
		self.verify_inputs(&lock)?;
		let checked = artifact::check(&artifact, &digest, None, true)?;
		self.atomic(
			&receipt,
			&wire::pretty(&Receipt {
				format: 2,
				assembly_key: None,
				stamp: Some(checked.stamp()),
				lock_sha256: hash(&bytes),
				executable_sha256: digest,
			})?,
		)?;
		eprintln!("built {}", artifact.display());
		Ok(())
	}
	fn checked_artifact(
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
			Ok(_) => Some(Receipt::decode(&input::read(
				&receipt_path,
				input::DOCUMENT_LIMIT,
			)?)?),
		};
		let identity = lock.shared()?;
		if receipt.as_ref().is_some_and(|r| r.format == 3)
			&& matches!(lock.assembly, Assembly::Generated { .. })
		{
			return Err("shared receipt cannot be used with a local lock; run build".into());
		}
		let (path, digest, stamp) = match &lock.assembly {
			Assembly::Shared { .. } => {
				let identity = identity.as_ref().unwrap();
				let r = receipt
					.as_ref()
					.ok_or("missing receipt; run build to attach shared assembly")?;
				if r.format != 3
					|| r.lock_sha256 != lock_digest
					|| r.assembly_key.as_deref() != Some(identity.key())
				{
					return Err("shared receipt does not match lock; run build".into());
				}
				let (path, digest) = crate::cache_entry::ready(identity)?;
				if r.executable_sha256 != digest {
					return Err("shared receipt does not match ready artifact; run build".into());
				}
				(path, digest, r.stamp.as_ref())
			}
			Assembly::Executable { path, sha256 } => {
				let stamp = receipt
					.as_ref()
					.filter(|r| {
						r.format != 3
							&& r.lock_sha256 == lock_digest
							&& r.executable_sha256 == *sha256
					})
					.and_then(|r| r.stamp.as_ref());
				(PathBuf::from(path), sha256.clone(), stamp)
			}
			Assembly::Generated { .. } => {
				let r = receipt.as_ref().ok_or("missing receipt; run build")?;
				if r.lock_sha256 != lock_digest {
					return Err("build receipt does not match lock; run build".into());
				}
				(
					self.dot.join("artifacts").join(&r.executable_sha256),
					r.executable_sha256.clone(),
					r.stamp.as_ref(),
				)
			}
		};
		let checked = artifact::check(&path, &digest, stamp, verify)?;
		if refresh && stamp != Some(&checked.stamp()) {
			fault("before-receipt-refresh")?;
			checked.recheck()?;
			self.atomic(
				&receipt_path,
				&wire::pretty(&Receipt {
					format: if identity.is_some() { 3 } else { 2 },
					assembly_key: identity.as_ref().map(|i| i.key().to_owned()),
					lock_sha256: lock_digest,
					executable_sha256: digest.clone(),
					stamp: Some(checked.stamp()),
				})?,
			)?;
		}

		Ok((checked, digest))
	}
	fn launch(self, mode: Launch, verify: bool) -> Result<(), String> {
		let (lock, bytes) = self.read_lock()?;
		self.verify_inputs(&lock)?;
		let (checked, digest) = self.checked_artifact(&lock, &bytes, verify, true)?;

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
					transition::association(&self, &lock, &checked, &digest)?,
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
}
fn cargo_home() -> Result<PathBuf, String> {
	let home = std::env::var_os("CARGO_HOME")
		.map(PathBuf::from)
		.or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
		.ok_or("cannot locate Cargo home")?;
	if !home.is_absolute() {
		return Err("CARGO_HOME must be absolute".into());
	}
	Ok(home)
}
fn environment() -> Result<(), String> {
	for (name, _) in std::env::vars_os() {
		let name = name.to_string_lossy();
		if ((name.starts_with("CARGO_") && name != "CARGO_HOME") || name.starts_with("RUST"))
			&& !matches!(name.as_ref(), "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN")
		{
			return Err(format!(
				"unsupported build environment override {name}; unset it"
			));
		}
	}
	Ok(())
}
fn config_policy(inputs: &inventory::Inventory) -> Result<(), String> {
	for file in &inputs.external {
		if file.file.is_none()
			|| !matches!(
				file.path.file_name().and_then(|p| p.to_str()),
				Some("config" | "config.toml")
			) {
			continue;
		}
		let bytes = input::read(&file.path, input::MANIFEST_LIMIT)?;
		if hash(&bytes) != file.file.as_ref().unwrap().sha256 {
			return Err("Cargo configuration changed during audit".into());
		}
		let value: toml::Value =
			toml::from_str(std::str::from_utf8(&bytes).map_err(err)?).map_err(err)?;
		let table = value.as_table().ok_or("Cargo config must be a table")?;
		for key in table.keys() {
			if !matches!(
				key.as_str(),
				"http" | "net" | "registry" | "registries" | "term"
			) {
				return Err(format!(
					"unsupported Cargo configuration key {key} in {}",
					file.path.display()
				));
			}
		}
	}
	Ok(())
}
fn preflight(base: &Path, stage: &Path) -> Result<(), String> {
	let metadata = wire::encode(&serde_json::json!({"workspace_root":stage,"packages":[]}))?;
	let config = inventory::native(
		&metadata,
		stage,
		base,
		&cargo_home()?,
		&mut Allowance::default(),
	)?;
	config_policy(&config)
}
fn versions(base: &Path) -> Result<(String, String, String), String> {
	let mut rustc = Command::new("rustc");
	rustc.current_dir(base).arg("-Vv");
	let rustc = String::from_utf8(commands::run(rustc, true)?).map_err(err)?;
	let target = rustc
		.lines()
		.find_map(|s| s.strip_prefix("host: "))
		.ok_or("rustc did not report its host target")?
		.to_owned();
	let mut cargo = Command::new("cargo");
	cargo.current_dir(base).arg("-V");
	let cargo = String::from_utf8(commands::run(cargo, true)?).map_err(err)?;
	Ok((rustc, cargo, target))
}
fn fault(name: &str) -> Result<(), String> {
	#[cfg(feature = "test-support")]
	if std::env::var("RNX_PROJECT_FAIL").ok().as_deref() == Some(name) {
		return Err(format!("injected failure at {name}"));
	}
	#[cfg(feature = "test-support")]
	if std::env::var("RNX_PROJECT_PAUSE").ok().as_deref() == Some(name) {
		let path =
			PathBuf::from(std::env::var_os("RNX_PROJECT_PAUSE_FILE").ok_or("pause file missing")?);
		fs::write(&path, name).map_err(err)?;
		while path.exists() {
			commands::check()?;
			std::thread::sleep(std::time::Duration::from_millis(10));
		}
	}
	let _ = name;
	Ok(())
}

enum Launch {
	Run(Vec<OsString>),
	Session {
		flags: Vec<OsString>,
	},
	Eval {
		flags: Vec<OsString>,
		source: OsString,
	},
}

pub(crate) fn cli(args: Vec<OsString>) -> Result<(), String> {
	if std::env::var_os("RNX_INTERNAL_DEP_FD").is_some() {
		return transition::serve(args);
	}
	if args.first().and_then(|s| s.to_str()) == Some("runtime") {
		return crate::runtime_install::cli(&args[1..]);
	}
	if args.first().and_then(|s| s.to_str()) == Some("adapters") {
		if args.len() != 1 {
			return Err("adapters takes no arguments".into());
		}
		print!("{}", crate::catalogue::listing());
		return Ok(());
	}
	if args.first().and_then(|s| s.to_str()) == Some("add") {
		let mut manifest = None;
		let mut names = Vec::new();
		let mut n = 1;
		while n < args.len() {
			match args[n].to_str() {
				Some("--manifest") if manifest.is_none() => {
					n += 1;
					manifest = Some(PathBuf::from(args.get(n).ok_or("--manifest needs a path")?));
				}
				Some(s) if !s.starts_with('-') => names.push(s.to_owned()),
				_ => return Err(format!("unexpected or duplicate add option {:?}", args[n])),
			}
			n += 1;
		}
		let entries = crate::catalogue::select(&names)?;
		let manifest = manifest.ok_or("--manifest is required; no upward search")?;
		commands::install_signals()?;
		return Project::open(&manifest)?.add(&entries);
	}
	if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "help")) {
		println!(
			"rnx-project runtime install --from PATH\nrnx-project runtime show\nrnx-project runtime select ID\nrnx-project adapters\nrnx-project add --manifest FILE NAME [NAME...]\nrnx-project lock|build|run|session|eval --manifest FILE\nrun [--verify] [-- script arguments]\nsession [--verify] [--color=auto|always|never] [--no-splash]\neval [--verify] [--color=auto|always|never] -- SOURCE\nLaunch checks your sources, trusts your build output unless you ask it to verify.\nUse --verify for a full artifact hash. Changed metadata triggers a full check.\nlock/build accept --offline; run/session/eval never build."
		);
		return Ok(());
	}
	let command = args
		.first()
		.and_then(|a| a.to_str())
		.ok_or("expected adapters, add, lock, build, run, session or eval")?;
	if !matches!(command, "lock" | "build" | "run" | "session" | "eval") {
		return Err("expected adapters, add, lock, build, run, session or eval".into());
	}
	let launch = matches!(command, "run" | "session" | "eval");
	let interactive = matches!(command, "session" | "eval");
	let mut manifest = None;
	let mut offline = false;
	let mut verify = false;
	let mut color = false;
	let mut splash = false;
	let mut flags = Vec::new();
	let mut script = Vec::new();
	let mut source = None;
	let mut n = 1;
	while n < args.len() {
		match args[n].to_str() {
			Some("--manifest") if manifest.is_none() => {
				n += 1;
				manifest = Some(PathBuf::from(args.get(n).ok_or("--manifest needs a path")?));
			}
			Some("--offline") if !launch && !offline => offline = true,
			Some("--verify") if launch && !verify => verify = true,
			Some("--no-splash") if command == "session" && !splash => {
				splash = true;
				flags.push(args[n].clone());
			}
			Some(value) if interactive && !color && value.starts_with("--color=") => {
				if !matches!(value, "--color=auto" | "--color=always" | "--color=never") {
					return Err("--color takes auto, always, or never".into());
				}
				color = true;
				flags.push(args[n].clone());
			}
			Some("--") if command == "run" => {
				script = args[n + 1..].to_vec();
				break;
			}
			Some("--") if command == "eval" => {
				if args.len() != n + 2 {
					return Err("eval needs exactly one source argument after --".into());
				}
				source = Some(args[n + 1].clone());
				break;
			}
			_ => return Err(format!("unexpected or duplicate option {:?}", args[n])),
		}
		n += 1;
	}
	if command == "eval" && source.is_none() {
		return Err("eval needs exactly one source argument after --".into());
	}
	let manifest = manifest.ok_or("--manifest is required; no upward search")?;
	commands::install_signals()?;
	environment()?;
	let project = Project::open(&manifest)?;
	match command {
		"lock" => project.lock(offline),
		"build" => project.build(offline),
		"run" => project.launch(Launch::Run(script), verify),
		"session" => project.launch(Launch::Session { flags }, verify),
		"eval" => project.launch(
			Launch::Eval {
				flags,
				source: source.unwrap(),
			},
			verify,
		),
		_ => unreachable!(),
	}
}
