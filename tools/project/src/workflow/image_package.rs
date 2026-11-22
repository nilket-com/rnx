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
		"engine" => false,
		"stock" | "candle,polars" => true,
		_ => return Err("unknown image package kind".into()),
	};
	let names = fs::read_dir(dir)
		.map_err(err)?
		.map(|e| e.map(|e| e.file_name()))
		.collect::<Result<Vec<_>, _>>()
		.map_err(err)?;
	if names.len() != if launcher { 3 } else { 2 }
		|| p.recipe.abi != 4
		|| p.recipe.target != TARGET
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
		let inputs = engine_inputs(root)?;
		let origin = if let Some(bundle) = self.bundle {
			let frozen = read_package(bundle)?;
			if frozen.recipe.kind != "engine"
				|| frozen.recipe.engine_inputs != inputs
				|| frozen.recipe.runtime != root
			{
				return Err("image package: stale engine bundle for this runtime".into());
			}
			sha(&bundle.join(RECEIPT))?
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
			abi: 4,
			target,
			runtime: root.to_owned(),
			engine_inputs: inputs,
			origin,
			kind: if self.bundle.is_none() {
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
			if (p.recipe.kind == "engine") != self.bundle.is_none() {
				return Err("image package: output has a different artifact kind".into());
			}
			if p.recipe == recipe {
				println!("{}", dest.display());
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
		println!("{}", dest.display());
		Ok(())
	}
}

pub(super) fn standalone(args: &[OsString]) -> Result<(), String> {
	// Explicit positions keep stock builds independent of manifest discovery.
	let (args, offline) = match args.last().and_then(|s| s.to_str()) {
		Some("--offline") => (&args[..args.len() - 1], true),
		_ => (args, false),
	};
	let (root, bundle, out) = match args {
		[mode, root, out] if mode == "engine" => (root, None, out),
		[mode, root, bundle, out] if mode == "stock" => {
			(root, Some(Path::new(bundle).canonicalize().map_err(err)?), out)
		}
		_ => return Err("usage: image engine RUNTIME OUTPUT | image stock RUNTIME ENGINE-BUNDLE OUTPUT [--offline]".into()),
	};
	commands::install_signals()?;
	environment()?;
	Source {
		root: &Path::new(root).canonicalize().map_err(err)?,
		bundle: bundle.as_deref(),
		pair: false,
		lock: String::new(),
		offline,
	}
	.build(Path::new(out))
}

fn location(
	base: &Path,
	path: Option<&str>,
	git: Option<&str>,
	rev: Option<&str>,
	name: &str,
	packages: &[crate::schemas::GitPackage],
) -> Result<PathBuf, String> {
	let path = match path {
		Some(p) => base.join(p),
		None => {
			let mut rows = packages.iter().filter(|p| {
				p.name == name && Some(p.url.as_str()) == git && Some(p.revision.as_str()) == rev
			});
			let row = rows.next().ok_or("locked package source missing")?;
			if rows.next().is_some() {
				return Err("ambiguous locked package source".into());
			}
			Path::new(&row.manifest)
				.parent()
				.ok_or("missing manifest parent")?
				.to_owned()
		}
	};
	path.canonicalize().map_err(err)
}

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
		let names: Vec<_> = decl.native.keys().map(String::as_str).collect();
		let pair = match names.as_slice() {
			[] => false,
			["candle", "polars"] => true,
			_ => {
				return Err(format!(
					"image package: unsupported native adapters [{}]; supported sets are [] and [candle, polars]",
					names.join(", ")
				));
			}
		};
		if !decl.sources.is_empty() {
			return Err("image package: source-alias module mappings are unsupported".into());
		}
		let runtime = decl.runtime.as_ref().ok_or(
			"image package requires an application runtime; executable overrides are unsupported",
		)?;
		for (alias, n) in &decl.native {
			if n.package != format!("rnx-{alias}")
				|| n.builder != "build"
				|| n.hook != crate::manifest::Hook::Plain
				|| !n.presentation
			{
				return Err(format!(
					"image package: unsupported native declaration for '{alias}'; expected the reviewed rnx-{alias} provider"
				));
			}
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
		let root = location(
			&self.base,
			runtime.path.as_deref(),
			runtime.git.as_deref(),
			runtime.rev.as_deref(),
			"rnx",
			&git,
		)?;
		for (alias, n) in &decl.native {
			let expected = root.join("adapters").join(alias);
			let resolved = location(
				&self.base,
				n.path.as_deref(),
				n.git.as_deref(),
				n.rev.as_deref(),
				&n.package,
				&git,
			);
			let reviewed = expected.canonicalize().map_err(err)?;
			let in_lock = native.as_ref().is_some_and(|natives| {
				natives
					.packages
					.iter()
					.any(|pkg| pkg.name == n.package && pkg.root == reviewed)
			}) || git.iter().any(|pkg| {
				pkg.name == n.package && Path::new(&pkg.manifest) == reviewed.join("Cargo.toml")
			});
			if !in_lock || resolved.as_ref().ok() != Some(&reviewed) {
				return Err(format!(
					"image package: unsupported native declaration for '{alias}'; expected the reviewed rnx-{alias} provider"
				));
			}
		}
		Source {
			root: &root,
			bundle: Some(&bundle.canonicalize().map_err(err)?),
			pair,
			lock: hash(&bytes),
			offline,
		}
		.build(out)
	}
}
