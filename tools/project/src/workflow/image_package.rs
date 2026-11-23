//! The opt-in Linux package is assembled beside its destination. The only
//! persistent cache entry is the last verified package at that destination.
use super::*;
use crate::fingerprint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::unix::{ffi::OsStrExt, fs::DirBuilderExt};

const LIB: &str = "libengine_image.so";
const RECEIPT: &str = "image-package.json";
const TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
	abi: u32,
	target: String,
	runtime: PathBuf,
	engine_inputs: String,
	// In the frozen bundle this is source provenance. In a runnable package
	// it is the digest of the verified input bundle's complete receipt.
	origin: String,
	kind: String,
	launcher_inputs: String,
	project_lock: String,
	tools: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Published {
	recipe: Recipe,
	engine_sha256: String,
	launcher_sha256: Option<String>,
}

fn sha(path: &Path) -> Result<String, String> {
	if !fs::symlink_metadata(path).map_err(err)?.is_file() {
		return Err(format!("expected a regular file: {}", path.display()));
	}
	let mut digest = Sha256::new();
	std::io::copy(&mut File::open(path).map_err(err)?, &mut digest).map_err(err)?;
	Ok(format!("{:x}", digest.finalize()))
}

fn read_package(dir: &Path) -> Result<Published, String> {
	if !fs::symlink_metadata(dir).map_err(err)?.is_dir() {
		return Err("image package: expected a package directory".into());
	}
	let path = dir.join(RECEIPT);
	if !fs::symlink_metadata(&path).map_err(err)?.is_file() {
		return Err("image package: invalid receipt file".into());
	}
	let p: Published =
		serde_json::from_slice(&input::read(&path, input::MANIFEST_LIMIT)?).map_err(err)?;
	let launcher = match p.recipe.kind.as_str() {
		"engine" | "project-engine" => false,
		"stock" | "candle,polars" => true,
		_ => return Err("unknown image package kind".into()),
	};
	let names = fs::read_dir(dir)
		.map_err(err)?
		.map(|e| e.map(|e| e.file_name()))
		.collect::<Result<Vec<_>, _>>()
		.map_err(err)?;
	if names.len() != if launcher { 3 } else { 2 }
		|| p.recipe.abi
			!= if p.recipe.kind == "project-engine" {
				5
			} else {
				4
			} || p.recipe.target != TARGET
		|| p.engine_sha256 != sha(&dir.join(LIB))?
		|| p.launcher_sha256
			!= if launcher {
				Some(sha(&dir.join("rnx"))?)
			} else {
				None
			} {
		return Err("image package: stale receipt or altered package".into());
	}
	Ok(p)
}

fn sources(root: &Path, dirs: &[&str], files: &[&str]) -> Result<String, String> {
	let mut allowance = Allowance::default();
	let mut rows = Vec::new();
	for name in dirs {
		rows.push((
			*name,
			fingerprint::native(&root.join(name), &mut allowance)?.blake3,
		));
	}
	for name in files {
		rows.push((*name, sha(&root.join(name))?));
	}
	Ok(hash(&wire::encode(&rows)?))
}

fn engine_inputs(root: &Path) -> Result<String, String> {
	sources(
		root,
		&["src", "rune/crates", "engine/image/src"],
		&[
			"Cargo.toml",
			"Cargo.lock",
			"build.rs",
			"tools/build_identity.rs",
			"engine/image/Cargo.toml",
			"engine/image/provider_abi.rs",
			"engine/image/engine_image.h",
			"engine/image/adapter_provider.h",
		],
	)
}

struct Private(PathBuf);
impl Drop for Private {
	fn drop(&mut self) {
		if let Err(e) = fs::remove_dir_all(&self.0) {
			eprintln!(
				"failed to remove private image build {}: {e}",
				self.0.display()
			);
		}
	}
}

fn helper(root: &Path, program: &str, args: &[&str]) -> Result<String, String> {
	let mut c = Command::new(program);
	c.current_dir(root).args(args);
	String::from_utf8(commands::run(c, true)?)
		.map(|s| s.trim().to_owned())
		.map_err(err)
}

fn size(path: &Path) -> Result<u64, String> {
	let mut c = Command::new("du");
	c.args(["-s", "-B1", "--"]).arg(path);
	String::from_utf8(commands::run(c, true)?)
		.map_err(err)?
		.split_whitespace()
		.next()
		.ok_or("empty du output")?
		.parse()
		.map_err(err)
}

fn cargo_build(root: &Path, work: &Path, offline: bool, args: &[&str]) -> Result<(), String> {
	let mut c = Command::new("cargo");
	c.current_dir(root)
		.args(["build", "--release", "--locked", "--target-dir"])
		.arg(work.join("target"))
		.args(args);
	if offline {
		c.arg("--offline");
	}
	commands::run(c, false)?;
	eprintln!(
		"image package: private build allocated {} bytes",
		size(work)?
	);
	Ok(())
}

fn publish(a: &Path, b: &Path, replace: bool) -> Result<(), String> {
	use std::ffi::CString;
	let a = CString::new(a.as_os_str().as_bytes()).map_err(err)?;
	let b = CString::new(b.as_os_str().as_bytes()).map_err(err)?;
	if unsafe {
		libc::renameat2(
			libc::AT_FDCWD,
			a.as_ptr(),
			libc::AT_FDCWD,
			b.as_ptr(),
			if replace {
				libc::RENAME_EXCHANGE
			} else {
				libc::RENAME_NOREPLACE
			},
		)
	} != 0
	{
		return Err(std::io::Error::last_os_error().to_string());
	}
	Ok(())
}

struct Source<'a> {
	project_engine: bool,
	root: &'a Path,
	bundle: Option<&'a Path>,
	pair: bool,
	lock: String,
	offline: bool,
}

impl Source<'_> {
	fn recipe(&self) -> Result<Recipe, String> {
		let root = self.root;
		let configuration = build_configuration(root, root)?;
		let (rustc, cargo, target) = versions(root)?;
		if target != TARGET {
			return Err(format!("image package requires {TARGET}"));
		}
		let mut inputs = engine_inputs(root)?;
		if self.project_engine {
			// Provider sources are exclusively helper inputs.
			let startup = sources(
				root,
				&[],
				&[
					"engine/launcher/installed/process.rs",
					"engine/launcher/installed/image.sh",
				],
			)?;
			inputs = hash(&wire::encode(&(inputs, startup))?);
		}
		let origin = if let Some(bundle) = self.bundle {
			let frozen = read_package(bundle)?;
			if frozen.recipe.kind != "engine"
				|| frozen.recipe.engine_inputs != inputs
				|| frozen.recipe.runtime != root
			{
				return Err("image package: stale engine bundle for this runtime".into());
			}
			sha(&bundle.join(RECEIPT))?
		} else if self.project_engine {
			"installed project ABI v1".into()
		} else {
			let revision = helper(root, "git", &["rev-parse", "HEAD"])?;
			let dirty = !helper(
				root,
				"git",
				&[
					"--no-optional-locks",
					"status",
					"--porcelain",
					"--untracked-files=normal",
				],
			)?
			.is_empty();
			format!("{revision};dirty={dirty}")
		};
		let mut dirs = vec!["engine/launcher"];
		if self.pair {
			dirs.extend([
				"adapters/file-provider",
				"adapters/candle",
				"adapters/polars",
			]);
		}
		Ok(Recipe {
			abi: if self.project_engine { 5 } else { 4 },
			target,
			runtime: root.to_owned(),
			engine_inputs: inputs,
			origin,
			kind: if self.project_engine {
				"project-engine"
			} else if self.bundle.is_none() {
				"engine"
			} else if self.pair {
				"candle,polars"
			} else {
				"stock"
			}
			.into(),
			launcher_inputs: if self.bundle.is_none() {
				String::new()
			} else {
				sources(
					root,
					&dirs,
					&["tools/project/src/workflow/image_package.rs"],
				)?
			},
			project_lock: self.lock.clone(),
			tools: vec![
				rustc,
				cargo,
				helper(root, "cc", &["--version"])?,
				helper(root, "ld", &["--version"])?,
				hash(&wire::encode(&configuration.external)?),
			],
		})
	}

	fn build(&self, output: &Path) -> Result<(), String> {
		let leaf = output.file_name().ok_or("expected named package output")?;
		let parent = output
			.parent()
			.filter(|p| !p.as_os_str().is_empty())
			.unwrap_or(Path::new("."))
			.canonicalize()
			.map_err(err)?;
		let dest = parent.join(leaf);
		if self.root.starts_with(&dest)
			|| self
				.bundle
				.is_some_and(|bundle| bundle.starts_with(&dest) || dest.starts_with(bundle))
		{
			return Err("image package: output overlaps its inputs".into());
		}
		let lockname = format!(".rnx-image-{}.lock", hash(leaf.as_bytes()));
		use std::os::unix::fs::OpenOptionsExt;
		let guard = OpenOptions::new()
			.read(true)
			.write(true)
			.create(true)
			.truncate(false)
			.mode(0o600)
			.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
			.open(parent.join(lockname))
			.map_err(err)?;
		if !guard.metadata().map_err(err)?.is_file() {
			return Err("invalid image package lock".into());
		}
		guard
			.try_lock()
			.map_err(|_| "another image package build owns this output")?;
		let recipe = self.recipe()?;
		let old = match fs::symlink_metadata(&dest) {
			Ok(_) => Some(read_package(&dest)?),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
			Err(e) => return Err(err(e)),
		};
		if let Some(p) = &old {
			if matches!(p.recipe.kind.as_str(), "engine" | "project-engine")
				!= self.bundle.is_none()
			{
				return Err("image package: output has a different artifact kind".into());
			}
			if p.recipe == recipe {
				if !self.project_engine {
					println!("{}", dest.display());
				}
				return Ok(());
			}
		}
		let tmp = parent.join(format!(".rnx-image-build-{}", std::process::id()));
		fs::DirBuilder::new()
			.mode(0o700)
			.create(&tmp)
			.map_err(err)?;
		let tmp = Private(tmp);
		let new = tmp.0.join("package");
		fs::create_dir(&new).map_err(err)?;
		let image = new.join(LIB);
		if let Some(bundle) = self.bundle {
			let frozen = read_package(bundle)?;
			if sha(&bundle.join(RECEIPT))? != recipe.origin {
				return Err("engine bundle changed before assembly".into());
			}
			fs::copy(bundle.join(LIB), &image).map_err(err)?;
			if sha(&image)? != frozen.engine_sha256 {
				return Err("engine bundle changed during copy".into());
			}
			let mut cc = Command::new("sh");
			cc.current_dir(self.root)
				.arg(self.root.join("engine/launcher/link.sh"))
				.arg(&new);
			if self.pair {
				cargo_build(
					self.root,
					&tmp.0,
					self.offline,
					&[
						"--manifest-path",
						"adapters/file-provider/Cargo.toml",
						"--lib",
					],
				)?;
				cc.arg(tmp.0.join("target/release/librnx_file_provider.a"))
					.arg(&tmp.0);
			}
			commands::run(cc, false)?;
		} else if self.project_engine {
			let mut build = Command::new("sh");
			build
				.current_dir(self.root)
				.arg("engine/launcher/installed/image.sh")
				.arg(&tmp.0)
				.arg("project-files")
				.env("CARGO", "cargo")
				.env("RUSTC", "rustc")
				.env("TARGET", TARGET);
			if self.offline {
				build.arg("--offline");
			}
			commands::run(build, false)?;
			fs::rename(
				tmp.0
					.join("target")
					.join(TARGET)
					.join("release/examples")
					.join(LIB),
				&image,
			)
			.map_err(err)?;
		} else {
			cargo_build(
				self.root,
				&tmp.0,
				self.offline,
				&[
					"-p",
					"rnx-engine-image",
					"--example",
					"engine_image",
					"--features",
					"linked",
				],
			)?;
			fs::rename(tmp.0.join("target/release/examples").join(LIB), &image).map_err(err)?;
		}
		if self.recipe()? != recipe {
			return Err("image package: build inputs changed".into());
		}
		let published = Published {
			recipe,
			engine_sha256: sha(&image)?,
			launcher_sha256: if self.bundle.is_some() {
				Some(sha(&new.join("rnx"))?)
			} else {
				None
			},
		};
		fs::write(new.join(RECEIPT), wire::encode(&published)?).map_err(err)?;
		read_package(&new)?;
		eprintln!(
			"image package: private assembly allocated {} bytes before publication",
			size(&tmp.0)?
		);
		commands::check()?;
		publish(&new, &dest, old.is_some())?;
		if !self.project_engine {
			println!("{}", dest.display());
		}
		Ok(())
	}
}

pub(super) fn standalone(args: &[OsString]) -> Result<(), String> {
	// Explicit positions keep stock builds independent of manifest discovery.
	let (args, offline) = match args.last().and_then(|s| s.to_str()) {
		Some("--offline") => (&args[..args.len() - 1], true),
		_ => (args, false),
	};
	commands::install_signals()?;
	environment()?;
	let (root, bundle, out) = match args {
		[mode, manifest, bundle, out] if mode == "project" => {
			return Project::open(Path::new(manifest))?.package_image(Path::new(bundle), Path::new(out), offline);
		}
		[mode, root, out] if mode == "engine" => (root, None, out),
		[mode, root, bundle, out] if mode == "stock" => {
			(root, Some(Path::new(bundle).canonicalize().map_err(err)?), out)
		}
		_ => return Err("usage: image engine RUNTIME OUTPUT | image stock RUNTIME ENGINE-BUNDLE OUTPUT | image project MANIFEST ENGINE-BUNDLE OUTPUT [--offline]".into()),
	};
	Source {
		project_engine: false,
		root: &Path::new(root).canonicalize().map_err(err)?,
		bundle: bundle.as_deref(),
		pair: false,
		lock: String::new(),
		offline,
	}
	.build(Path::new(out))
}

use crate::project_files::{FileRoute, declared_pair, file_route, reviewed_registration};

impl Project {
	pub(super) fn package_image(
		&self,
		bundle: &Path,
		out: &Path,
		offline: bool,
	) -> Result<(), String> {
		let format_two = self.new_manifest()?;
		let decl = if format_two {
			crate::schemas::Declaration::read(&self.manifest)?
		} else {
			let old = Manifest::read(&self.manifest)?;
			serde_json::from_value(serde_json::to_value(old).map_err(err)?).map_err(err)?
		};
		// The explicit ABI-v4 client reports declaration errors even before lock.
		if let Err(reason) = declared_pair(&decl) {
			let names = decl.native.keys().map(String::as_str).collect::<Vec<_>>();
			let message = match reason {
				"native_set" => format!("unsupported native adapters [{}]; supported sets are [] and [candle, polars]", names.join(", ")),
				"native_registration" => {
					let (alias, _) = decl.native.iter().find(|(a,n)| !reviewed_registration(a,n)).ok_or("missing unsupported registration")?;
					format!("unsupported native declaration for '{alias}'; expected the reviewed rnx-{alias} provider")
				}
				"source_alias" => "source-alias module mappings are unsupported".into(),
				_ => return Err("image package requires an application runtime; executable overrides are unsupported".into()),
			};
			return Err(format!("image package: {message}"));
		}
		let (bytes, git, native) = if format_two {
			let (lock, bytes) = self.read_git_lock()?;
			self.git_verify_inputs(&lock, true)?;
			(bytes, lock.git, lock.inputs.native)
		} else {
			let (lock, bytes) = self.read_lock()?;
			self.verify_inputs(&lock)?;
			(bytes, Vec::new(), lock.inputs.native)
		};
		let (root, pair) = match file_route(&self.base, &decl, &git, native.as_ref())? {
			FileRoute::Image { root, pair } => (root, pair),
			FileRoute::Current(reason) => {
				return Err(format!("image package: project is ineligible ({reason})"));
			}
		};
		Source {
			project_engine: false,
			root: &root,
			bundle: Some(&bundle.canonicalize().map_err(err)?),
			pair,
			lock: hash(&bytes),
			offline,
		}
		.build(out)
	}
}

/// The manager owns route evidence for artifacts without the reporting entry.
pub(super) fn report_manager_route(command: &mut Command, reason: &str) -> Result<(), String> {
	use std::os::fd::FromRawFd;

	let Some(raw) = std::env::var_os("RNX_FILE_ROUTE_FD") else {
		return Ok(());
	};
	let fd = raw
		.to_str()
		.and_then(|s| s.parse::<i32>().ok())
		.filter(|&fd| fd >= 3)
		.ok_or("invalid file route descriptor")?;
	// Borrow the caller's stream through a private close-on-exec descriptor.
	let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
	if copy < 0 {
		return Err(err(std::io::Error::last_os_error()));
	}
	let mut file = unsafe { File::from_raw_fd(copy) };
	let route = if reason == "historical_lock" {
		"historical_lock"
	} else {
		"manifest_ineligible"
	};
	let mut event = wire::encode(
		&serde_json::json!({"route":route,"reason":reason,"category":null,"location":null}),
	)?;
	event.push(b'\n');
	file.write_all(&event).map_err(err)?;
	command.env_remove("RNX_FILE_ROUTE_FD");
	Ok(())
}

/// Pin one copy of the common image for this build under the assembly entry's
/// lock. Another builder can replace the single cached generation afterwards.
pub(super) fn prepare_project_bundle(
	cmd: &mut Command,
	stage: &Path,
	cache: &Path,
	runtime: &Path,
	pair: bool,
	offline: bool,
) -> Result<super::shared::Resolver, String> {
	use std::os::unix::fs::OpenOptionsExt;
	let private = super::shared::resolver(cache)?;
	let frozen = cache.join("project-file-engine");
	let source = Source {
		project_engine: true,
		root: runtime,
		bundle: None,
		pair: false,
		lock: String::new(),
		offline,
	};
	let image = private.0.join(LIB);
	let engine_receipt = {
		let guard = OpenOptions::new()
			.read(true)
			.write(true)
			.create(true)
			.truncate(false)
			.mode(0o600)
			.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
			.open(cache.join("locks/project-file-engine.lock"))
			.map_err(err)?;
		if !guard.metadata().map_err(err)?.is_file() {
			return Err("invalid project engine lock".into());
		}
		guard.lock().map_err(err)?;
		source.build(&frozen)?;
		let stamp = read_package(&frozen)?;
		if stamp.recipe != source.recipe()? {
			return Err("project engine recipe changed during build".into());
		}
		fs::hard_link(frozen.join(LIB), &image).map_err(err)?;
		if sha(&image)? != stamp.engine_sha256 {
			return Err("project engine changed before link".into());
		}
		stamp
	};
	let helper = private.0.join("helper");
	let mut link = Command::new("sh");
	link.current_dir(runtime)
		.arg("engine/launcher/installed/link.sh")
		.arg(&image)
		.arg(&helper);
	if pair {
		cargo_build(
			runtime,
			&private.0,
			offline,
			&[
				"--manifest-path",
				"adapters/file-provider/Cargo.toml",
				"--lib",
			],
		)?;
		link.arg(private.0.join("target/release/librnx_file_provider.a"));
	}
	commands::run(link, false)?;
	let engine_sha256 = engine_receipt.engine_sha256;
	let helper_sha256 = sha(&helper)?;
	cmd.env("RNX_PROJECT_IMAGE", &image)
		.env("RNX_PROJECT_HELPER", &helper)
		.env(
			"RNX_PROJECT_GENERATION",
			format!("{engine_sha256}-{helper_sha256}"),
		);
	let build = serde_json::json!({"abi":5, "project_handoff":1,"target":TARGET,"engine_sha256":engine_sha256,
		"helper_sha256":helper_sha256,"engine_recipe":engine_receipt.recipe,"provider":if pair {"candle,polars"} else {"stock"}});
	crate::cache_entry::write_new(&stage.join("file-bundle.json"), &wire::encode(&build)?)?;
	Ok(private)
}
