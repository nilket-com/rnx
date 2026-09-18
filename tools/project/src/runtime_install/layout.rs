//! Static closure checks, not a Cargo resolver or a Rust/build-script sandbox.
use super::storage::err;
use crate::{catalogue, fingerprint, input};
use std::{
	collections::BTreeSet,
	path::{Component, Path, PathBuf},
};
fn reference(
	root: &Path,
	base: &Path,
	value: &toml::Value,
	file: &Path,
	field: &str,
	paths: &BTreeSet<PathBuf>,
) -> Result<(), String> {
	let bad = |why: &str| format!("{}: {field}: {why}", file.display());
	let text = value
		.as_str()
		.ok_or_else(|| bad("expected a literal relative path"))?;
	let p = Path::new(text);
	if text.is_empty() || p.is_absolute() || text.contains(['\\', '\0', '*', '?', '[']) {
		return Err(bad("unsupported or non-relative source path"));
	}
	let mut joined = base.to_owned();
	for c in p.components() {
		match c {
			Component::Normal(n) => joined.push(n),
			Component::CurDir => (),
			Component::ParentDir => {
				if joined == root {
					return Err(bad("path escapes installed snapshot"));
				}
				joined.pop();
			}
			_ => return Err(bad("unsupported path component")),
		}
	}
	if !joined.starts_with(root) || !paths.iter().any(|p| p == &joined || p.starts_with(&joined)) {
		return Err(bad(
			"path is outside the tracked snapshot or names no tracked source",
		));
	}
	Ok(())
}
fn inspect(
	root: &Path,
	base: &Path,
	v: &toml::Value,
	file: &Path,
	field: &str,
	paths: &BTreeSet<PathBuf>,
) -> Result<(), String> {
	if let Some(table) = v.as_table() {
		for (key, v) in table {
			let next = if field.is_empty() {
				key.clone()
			} else {
				format!("{field}.{key}")
			};
			if key == "metadata" {
				continue;
			}
			if key == "path"
				|| (field == "package" && key == "workspace")
				|| (field == "package"
					&& matches!(key.as_str(), "build" | "readme" | "license-file")
					&& v.is_str())
			{
				reference(root, base, v, file, &next, paths)?;
			} else if key == "workspace" && v.is_bool() {
				return Err(format!(
					"{}: {next}: workspace inheritance is unsupported by the installed layout",
					file.display()
				));
			} else if field == "workspace"
				&& matches!(key.as_str(), "members" | "exclude" | "default-members")
			{
				for item in v
					.as_array()
					.ok_or_else(|| format!("{}: {next}: expected paths", file.display()))?
				{
					reference(root, base, item, file, &next, paths)?;
				}
			} else {
				inspect(root, base, v, file, &next, paths)?;
			}
		}
	} else if let Some(array) = v.as_array() {
		for item in array {
			inspect(root, base, item, file, field, paths)?;
		}
	}
	Ok(())
}
pub(super) fn validate(root: &Path, tree: &fingerprint::Tree) -> Result<(), String> {
	let paths = tree
		.files
		.iter()
		.map(|f| root.join(&f.path))
		.collect::<BTreeSet<_>>();
	for required in [
		"Cargo.toml",
		"src/lib.rs",
		"src/main.rs",
		"adapters/polars/Cargo.toml",
		"adapters/polars/src/lib.rs",
		"adapters/postgres/Cargo.toml",
		"adapters/postgres/src/lib.rs",
	] {
		if !paths.contains(&root.join(required)) {
			return Err(format!(
				"unsupported runtime layout: missing tracked {required}"
			));
		}
	}
	for file in tree.files.iter().filter(|f| {
		Path::new(&f.path)
			.file_name()
			.is_some_and(|n| n == "Cargo.toml")
	}) {
		let path = root.join(&file.path);
		let raw = input::read(&path, input::MANIFEST_LIMIT)?;
		let value: toml::Value = toml::from_str(std::str::from_utf8(&raw).map_err(err)?)
			.map_err(|e| format!("{}: {e}", path.display()))?;
		inspect(root, path.parent().unwrap(), &value, &path, "", &paths)?;
	}
	// Reuse 0062's supported package names and each adapter's direct dependency on rnx.
	let raw = format!(
		"format=1\n[application]\nentry=\"src/main.rs\"\n[runtime]\npath={}\n",
		toml::Value::String(root.to_str().ok_or("non-Unicode runtime path")?.into())
	);
	catalogue::author(
		raw.into_bytes(),
		root,
		&catalogue::select(&["polars".into(), "postgres".into()])?,
	)?;
	Ok(())
}
