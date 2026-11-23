//! File routes derived from the resolved application runtime and adapters.
use crate::inventory;
use std::path::{Path, PathBuf};
fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
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

/// Classify only after verifying the lock. Missing or ambiguous locked input is
/// an error; a verified adapter from another source is an ineligible project.
#[derive(Debug, PartialEq)]
pub(crate) enum FileRoute {
	Image { root: PathBuf, pair: bool },
	Current(&'static str),
}

pub(crate) fn reviewed_registration(alias: &str, n: &crate::schemas::Native) -> bool {
	n.package == format!("rnx-{alias}")
		&& n.builder == "build"
		&& n.hook == crate::manifest::Hook::Plain
		&& n.presentation
}

pub(crate) fn declared_pair(decl: &crate::schemas::Declaration) -> Result<bool, &'static str> {
	if decl.runtime.is_none() {
		return Err("executable_override");
	}
	if !decl.sources.is_empty() {
		return Err("source_alias");
	}
	let names: Vec<_> = decl.native.keys().map(String::as_str).collect();
	let pair = match names.as_slice() {
		[] => false,
		["candle", "polars"] => true,
		_ => return Err("native_set"),
	};
	for (alias, n) in &decl.native {
		if !reviewed_registration(alias, n) {
			return Err("native_registration");
		}
	}
	Ok(pair)
}

pub(crate) fn file_route(
	base: &Path,
	decl: &crate::schemas::Declaration,
	git: &[crate::schemas::GitPackage],
	native: Option<&inventory::Inventory>,
) -> Result<FileRoute, String> {
	let pair = match declared_pair(decl) {
		Ok(pair) => pair,
		Err(reason) => return Ok(FileRoute::Current(reason)),
	};
	let runtime = decl.runtime.as_ref().ok_or("missing runtime")?;
	let root = location(
		base,
		runtime.path.as_deref(),
		runtime.git.as_deref(),
		runtime.rev.as_deref(),
		"rnx",
		git,
	)?;
	for (alias, n) in &decl.native {
		let resolved = location(
			base,
			n.path.as_deref(),
			n.git.as_deref(),
			n.rev.as_deref(),
			&n.package,
			git,
		)?;
		let in_lock = native.is_some_and(|natives| {
			natives
				.packages
				.iter()
				.any(|p| p.name == n.package && p.root == resolved)
		}) || git
			.iter()
			.any(|p| p.name == n.package && Path::new(&p.manifest) == resolved.join("Cargo.toml"));
		if !in_lock {
			return Err(format!(
				"project lock lacks the resolved source of {}",
				n.package
			));
		}
		match root.join("adapters").join(alias).canonicalize() {
			Ok(reviewed) if reviewed == resolved => (),
			Ok(_) => return Ok(FileRoute::Current("native_source")),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
				return Ok(FileRoute::Current("native_source"));
			}
			Err(error) => return Err(err(error)),
		}
	}
	Ok(FileRoute::Image { root, pair })
}

/// Absent in historical identities. A freshly locked project records its
/// launch recipe even when its runtime predates the image entry.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Recipe {
	Image { root: PathBuf, pair: bool },
	Current { reason: String },
	RetainedRuntime,
}

impl Recipe {
	pub(crate) fn resolve(
		base: &Path,
		decl: &crate::schemas::Declaration,
		git: &[crate::schemas::GitPackage],
		native: &inventory::Inventory,
	) -> Result<Self, String> {
		let r = decl
			.runtime
			.as_ref()
			.ok_or("expected a generated project")?;
		let root = location(
			base,
			r.path.as_deref(),
			r.git.as_deref(),
			r.rev.as_deref(),
			"rnx",
			git,
		)?;
		if !native
			.packages
			.iter()
			.any(|p| p.name == "rnx" && p.root == root)
			&& !git
				.iter()
				.any(|p| p.name == "rnx" && Path::new(&p.manifest) == root.join("Cargo.toml"))
		{
			return Err("project lock lacks its resolved runtime".into());
		}
		let bytes = crate::input::read(&root.join("Cargo.toml"), crate::input::MANIFEST_LIMIT)?;
		let manifest: toml::Value =
			toml::from_str(std::str::from_utf8(&bytes).map_err(err)?).map_err(err)?;
		if !cfg!(all(
			target_arch = "x86_64",
			target_os = "linux",
			target_env = "gnu"
		)) || manifest
			.get("features")
			.and_then(|f| f.get("project-file-bundle"))
			.is_none()
		{
			return Ok(Self::RetainedRuntime);
		}
		match file_route(base, decl, git, Some(native))? {
			FileRoute::Image { root, pair } => Ok(Self::Image { root, pair }),
			FileRoute::Current(reason) => Ok(Self::Current {
				reason: reason.into(),
			}),
		}
	}
}

/// Only the manager can report launches of binaries built before this protocol.
pub(crate) fn manager_reason(executable: bool, recipe: Option<&Recipe>) -> Option<&'static str> {
	if executable {
		return Some("executable_override");
	}
	match recipe {
		None => Some("historical_lock"),
		Some(Recipe::RetainedRuntime) => Some("runtime_without_handoff"),
		Some(Recipe::Image { .. } | Recipe::Current { .. }) => None,
	}
}
pub(crate) fn features(recipe: Option<&Recipe>) -> Vec<String> {
	let mut features = vec![String::from("project-sources")];
	if matches!(recipe, Some(Recipe::Image { .. } | Recipe::Current { .. })) {
		features.insert(0, "project-file-bundle".into());
	}
	features
}
