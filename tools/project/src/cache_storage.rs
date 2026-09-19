//! Checked managed cache paths and the Cargo context policy proven at gate one.
#![allow(dead_code)]
use crate::{input, inventory};

use std::{
	fs,
	path::{Path, PathBuf},
};
fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}
fn forbidden(dir: &Path, stage: bool) -> Result<(), String> {
	for name in [
		"Cargo.toml",
		".cargo/config",
		".cargo/config.toml",
		"rust-toolchain",
		"rust-toolchain.toml",
	] {
		if stage && name == "Cargo.toml" {
			continue;
		}
		let p = dir.join(name);
		match fs::symlink_metadata(&p) {
			Ok(_) => return Err(format!("unexpected managed Cargo input {}", p.display())),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
			Err(e) => return Err(error(e)),
		}
	}
	// A .cargo symlink with absent target files must not bypass the checks.
	if let Ok(m) = fs::symlink_metadata(dir.join(".cargo"))
		&& !m.is_dir()
	{
		return Err(format!(
			"managed .cargo is not a directory: {}",
			dir.display()
		));
	}
	Ok(())
}
pub(crate) fn root(path: &Path) -> Result<PathBuf, String> {
	let root = path
		.canonicalize()
		.map_err(|e| format!("cache root {}: {e}", path.display()))?;
	private_directory(&root)?;
	Ok(root)
}
pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
	#[cfg(unix)]
	use std::os::unix::fs::MetadataExt;
	let m = fs::symlink_metadata(path)
		.map_err(|e| format!("cache directory {}: {e}; run build", path.display()))?;
	let invalid = !m.is_dir();
	#[cfg(unix)]
	let invalid = invalid || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o022 != 0;
	if invalid {
		return Err(format!("not a private owned directory: {}", path.display()));
	}
	Ok(())
}
pub(crate) fn guard(root: &Path, stage: &Path, project: &Path) -> Result<(), String> {
	// Only the selected root may have a user symlink spelling. Below it, walk
	// lexical components rather than canonicalizing away a managed symlink.
	let relative = stage.strip_prefix(root).map_err(error)?;
	if relative.as_os_str().is_empty() {
		return Err("stage must be below cache root".into());
	}
	let mut cursor = root.to_owned();
	for component in relative.components() {
		let std::path::Component::Normal(part) = component else {
			return Err("non-normal managed path".into());
		};
		cursor.push(part);
		private_directory(&cursor)?;
		forbidden(&cursor, cursor == stage)?;
	}
	match fs::symlink_metadata(stage.join("src")) {
		Ok(_) => private_directory(&stage.join("src"))?,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
		Err(e) => return Err(error(e)),
	}
	for name in ["Cargo.toml", "Cargo.lock", "src/main.rs"] {
		let path = stage.join(name);
		match fs::symlink_metadata(&path) {
			Ok(m) if !m.is_file() => {
				return Err(format!("managed input is not regular: {}", path.display()));
			}
			Ok(_) => (),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
			Err(e) => return Err(error(e)),
		}
	}
	project_context(root, project)
}
pub(crate) fn project_context(root: &Path, project: &Path) -> Result<(), String> {
	let cache_ancestors: Vec<_> = root.ancestors().collect();
	for dir in project.ancestors().filter(|p| !cache_ancestors.contains(p)) {
		for name in [
			".cargo/config",
			".cargo/config.toml",
			"rust-toolchain",
			"rust-toolchain.toml",
		] {
			let p = dir.join(name);
			match fs::symlink_metadata(&p) {
				Ok(_) => {
					return Err(format!(
						"project Cargo input {} is outside shared cache context {}",
						p.display(),
						root.display()
					));
				}
				Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
				Err(e) => return Err(error(e)),
			}
		}
	}
	Ok(())
}
pub(crate) fn policy(inv: &inventory::Inventory) -> Result<(), String> {
	for external in &inv.external {
		let Some(file) = &external.file else { continue };
		if !matches!(
			external.path.file_name().and_then(|s| s.to_str()),
			Some("config" | "config.toml")
		) {
			continue;
		}
		let bytes = input::read(&external.path, input::MANIFEST_LIMIT)?;
		if blake3::hash(&bytes).to_hex().to_string() != file.blake3 {
			return Err("config changed".into());
		}
		let config: toml::Value =
			toml::from_str(std::str::from_utf8(&bytes).map_err(error)?).map_err(error)?;
		config_policy(&config, &external.path)?;
	}
	Ok(())
}

/// Only the explicitly modelled native linker configuration is admitted.
/// The caller authenticates the config bytes against the input inventory.
pub(crate) fn config_policy(config: &toml::Value, path: &Path) -> Result<(), String> {
	let refuse = |key: &str| {
		format!(
			"unsupported Cargo configuration key {key} in {}; remove the unsupported setting before retrying; project-local Cargo config is not a workaround",
			path.display()
		)
	};
	for (key, value) in config.as_table().ok_or("Cargo config must be a table")? {
		if matches!(
			key.as_str(),
			"http" | "net" | "registry" | "registries" | "term"
		) {
			continue;
		}
		if key != "target" {
			return Err(refuse(key));
		}
		for (target, settings) in value.as_table().ok_or_else(|| refuse("target"))? {
			let prefix = format!("target.{target}");
			if target != "x86_64-unknown-linux-gnu" {
				return Err(refuse(&prefix));
			}
			for (name, value) in settings.as_table().ok_or_else(|| refuse(&prefix))? {
				let valid = match name.as_str() {
					"linker" => value.as_str().is_some_and(|s| !s.is_empty()),
					"rustflags" => value
						.as_array()
						.is_some_and(|a| a.iter().all(|v| v.as_str().is_some())),
					_ => false,
				};
				if !valid {
					return Err(refuse(&format!("{prefix}.{name}")));
				}
			}
		}
	}
	Ok(())
}

pub(crate) fn directory(path: &Path) -> Result<(), String> {
	let builder = fs::DirBuilder::new();
	#[cfg(unix)]
	let builder = {
		let mut builder = builder;
		use std::os::unix::fs::DirBuilderExt;
		builder.mode(0o700);
		builder
	};
	match builder.create(path) {
		Ok(()) => (),
		Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
		Err(e) => return Err(error(e)),
	}
	private_directory(path)
}

#[cfg(test)]
mod config_tests {
	use super::*;
	#[test]
	fn admits_slim_linker_config_and_refuses_unmodelled_settings() {
		let slim = "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\nrustflags = [\"-C\", \"link-arg=-fuse-ld=mold\"]\n";
		let check =
			|s: &str| config_policy(&toml::from_str(s).unwrap(), Path::new("/cargo/config.toml"));
		check(slim).unwrap();
		for extra in [
			"[patch.crates-io]\n",
			"[build]\n",
			"[env]\n",
			"[alias]\n",
			"[profile.dev]\n",
			"[source.crates-io]\n",
			"[unstable]\n",
			"[target.aarch64-unknown-linux-gnu]\n",
		] {
			let error = check(&format!("{slim}{extra}")).unwrap_err();
			assert!(error.contains("/cargo/config.toml"));
			assert!(error.contains("project-local Cargo config is not a workaround"));
		}
		for fields in [
			"runner = 'x'",
			"linker = 3",
			"rustflags = ['-C', 3]",
			"rustflags = '-C opt-level=2'",
		] {
			assert!(check(&format!("[target.x86_64-unknown-linux-gnu]\n{fields}")).is_err());
		}
	}
}
