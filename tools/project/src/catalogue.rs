//! The shipped catalogue authors ordinary native tables, never resolves builds.
use crate::input;
use crate::manifest::{Hook, Manifest, Native};
use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
};
fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}
fn canonical(path: &Path) -> Result<PathBuf, String> {
	path.canonicalize()
		.map_err(|e| format!("{}: {e}", path.display()))
}
fn cargo(root: &Path, expected: &str) -> Result<toml::Value, String> {
	let path = root.join("Cargo.toml");
	let bytes = input::read(&path, input::MANIFEST_LIMIT)?;
	let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
	let value: toml::Value =
		toml::from_str(text).map_err(|e| format!("{}: {e}", path.display()))?;
	if value
		.get("package")
		.and_then(|p| p.get("name"))
		.and_then(|p| p.as_str())
		!= Some(expected)
	{
		return Err(format!(
			"{}: expected explicit package.name {expected}",
			path.display()
		));
	}
	Ok(value)
}

#[derive(Clone, Copy)]
pub(crate) struct Entry {
	pub name: &'static str,
	package: &'static str,
	hook: Hook,
}
const ENTRIES: [Entry; 2] = [
	Entry {
		name: "polars",
		package: "rnx-polars",
		hook: Hook::Plain,
	},
	Entry {
		name: "postgres",
		package: "rnx-postgres",
		hook: Hook::Lifecycle,
	},
];
pub(crate) fn listing() -> String {
	let mut text = String::from("NAME      PACKAGE       HOOK       PATH BELOW RUNTIME\n");
	for e in ENTRIES {
		let hook = match e.hook {
			Hook::Plain => "plain",
			Hook::Lifecycle => "lifecycle",
		};
		text += &format!(
			"{:<9} {:<13} {:<10} adapters/{}\n",
			e.name, e.package, hook, e.name
		);
	}
	text
}
pub(crate) fn select(names: &[String]) -> Result<Vec<Entry>, String> {
	if names.is_empty() || names.len() > 32 {
		return Err("add needs 1 to 32 distinct adapter names".into());
	}
	let mut entries = BTreeMap::new();
	for name in names {
		let entry = ENTRIES
			.iter()
			.find(|e| e.name == name)
			.ok_or_else(|| format!("unknown adapter {name}; available: polars, postgres"))?;
		if entries.insert(entry.name, *entry).is_some() {
			return Err(format!("duplicate adapter {name}"));
		}
	}
	Ok(entries.into_values().collect())
}
pub(crate) struct Candidate {
	pub bytes: Vec<u8>,
	pub added: Vec<String>,
	pub existing: Vec<String>,
}
pub(crate) fn author(raw: Vec<u8>, base: &Path, entries: &[Entry]) -> Result<Candidate, String> {
	let value: toml::Value =
		toml::from_str(std::str::from_utf8(&raw).map_err(error)?).map_err(error)?;
	if value.get("format").and_then(toml::Value::as_integer) == Some(2) {
		return author_git(raw, base, entries);
	}
	let original = Manifest::parse(&raw)?;
	let runtime = original
		.runtime
		.as_ref()
		.ok_or("add requires an application with a runtime path")?;
	let root = canonical(&base.join(&runtime.path))?;
	cargo(&root, "rnx")?;
	let mut additions = BTreeMap::new();
	let mut existing = Vec::new();
	for entry in entries {
		let name = entry.name.to_owned();
		let package = entry.package;
		let hook = entry.hook;
		let suffix = Path::new("adapters").join(&name);
		let adapter = canonical(&root.join(&suffix))?;
		let value = cargo(&adapter, package)?;
		let dep = value
			.get("dependencies")
			.and_then(|v| v.get("rnx"))
			.ok_or_else(|| format!("{name}: missing dependencies.rnx"))?;
		if [
			"workspace",
			"git",
			"branch",
			"tag",
			"rev",
			"registry",
			"registry-index",
		]
		.iter()
		.any(|key| dep.get(key).is_some())
			|| dep
				.get("package")
				.is_some_and(|v| v.as_str() != Some("rnx"))
		{
			return Err(format!("{name}: requires direct rnx path dependency"));
		}
		let dep = dep
			.get("path")
			.and_then(|v| v.as_str())
			.ok_or_else(|| format!("{name}: requires direct rnx path dependency"))?;
		if canonical(&adapter.join(dep))? != root {
			return Err(format!("{name}: adapter depends on a different runtime"));
		}
		let written = Path::new(&runtime.path).join(suffix);
		if canonical(&base.join(&written))? != adapter {
			return Err(format!(
				"{name}: written path does not reach validated adapter"
			));
		}
		let native = Native {
			path: written.to_str().ok_or("non-Unicode adapter path")?.into(),
			package: package.into(),
			builder: "build".into(),
			hook,
		};
		if let Some(old) = original.native.get(&name) {
			if canonical(&base.join(&old.path))? != adapter
				|| old.package != native.package
				|| old.builder != native.builder
				|| old.hook != native.hook
			{
				return Err(format!(
					"native namespace {name} already has a different declaration"
				));
			}
			existing.push(name);
		} else {
			additions.insert(name, native);
		}
	}
	#[derive(serde::Serialize)]
	struct Tables<'a> {
		native: &'a BTreeMap<String, Native>,
	}
	let mut candidate = String::from_utf8(raw).map_err(error)?;
	if !additions.is_empty() {
		let append = toml::to_string(&Tables { native: &additions }).map_err(error)?;
		if candidate.len() + 2 + append.len() > input::MANIFEST_LIMIT {
			return Err("candidate manifest exceeds 1048576 bytes".into());
		}
		candidate.push_str("\n\n");
		candidate.push_str(&append);
	}
	let parsed = Manifest::parse(candidate.as_bytes())
		.map_err(|e| format!("cannot append native tables; use explicit table layout: {e}"))?;
	let mut expected = original;
	expected.native.extend(additions.clone());
	if parsed != expected {
		return Err("append changed unrelated manifest meaning".into());
	}

	Ok(Candidate {
		bytes: candidate.into_bytes(),
		added: additions.into_keys().collect(),
		existing,
	})
}

/// Author coordinates without acquisition. Consent and lock own the first fetch.
fn author_git(raw: Vec<u8>, base: &Path, entries: &[Entry]) -> Result<Candidate, String> {
	use crate::schemas::{Declaration, Native};
	let original = Declaration::parse(&raw)?;
	let runtime = original.runtime.as_ref().ok_or("add requires a runtime")?;
	let mut additions = BTreeMap::new();
	let mut existing = Vec::new();
	let path_natives = if let Some(path) = &runtime.path {
		let mut m = original.graph_manifest();
		m.executable = None;
		m.runtime = Some(crate::manifest::Location { path: path.clone() });
		let candidate = author(
			toml::to_string(&m).map_err(error)?.into_bytes(),
			base,
			entries,
		)?;
		Some(Manifest::parse(&candidate.bytes)?.native)
	} else {
		None
	};
	for e in entries {
		let n = if let Some(n) = path_natives.as_ref().and_then(|n| n.get(e.name)) {
			Native {
				path: Some(n.path.clone()),
				git: None,
				rev: None,
				package: n.package.clone(),
				builder: n.builder.clone(),
				hook: n.hook,
			}
		} else {
			Native {
				path: None,
				git: runtime.git.clone(),
				rev: runtime.rev.clone(),
				package: e.package.into(),
				builder: "build".into(),
				hook: e.hook,
			}
		};
		if let Some(old) = original.native.get(e.name) {
			let mut comparison = old.clone();
			let mut expected = n.clone();
			if let (Some(a), Some(b)) = (&old.path, &n.path) {
				comparison.path = Some(
					canonical(&base.join(a))?
						.to_str()
						.ok_or("non-Unicode path")?
						.into(),
				);
				expected.path = Some(
					canonical(&base.join(b))?
						.to_str()
						.ok_or("non-Unicode path")?
						.into(),
				);
			}
			if comparison != expected {
				return Err(format!(
					"native namespace {} already has a different declaration",
					e.name
				));
			}
			existing.push(e.name.into());
		} else {
			additions.insert(e.name.to_owned(), n);
		}
	}
	#[derive(serde::Serialize)]
	struct Tables<'a> {
		native: &'a BTreeMap<String, Native>,
	}
	let mut candidate = String::from_utf8(raw).map_err(error)?;
	if !additions.is_empty() {
		let append = toml::to_string(&Tables { native: &additions }).map_err(error)?;
		if candidate.len() + 2 + append.len() > input::MANIFEST_LIMIT {
			return Err("candidate manifest allowance".into());
		}
		candidate.push_str("\n\n");
		candidate.push_str(&append);
	}
	let parsed = Declaration::parse(candidate.as_bytes())
		.map_err(|e| format!("cannot append native tables; use explicit table layout: {e}"))?;
	let mut expected = original;
	expected.native.extend(additions.clone());
	if parsed != expected {
		return Err("append changed unrelated manifest meaning".into());
	}
	Ok(Candidate {
		bytes: candidate.into_bytes(),
		added: additions.into_keys().collect(),
		existing,
	})
}

/// Validate a diagnostic hint using the same shipped layouts as add, without Git.
pub(crate) fn supported_runtime(path: &Path) -> bool {
	let Some(path) = path.to_str() else {
		return false;
	};
	let raw = format!(
		"format=1\n[application]\nentry=\"entry.rn\"\n[runtime]\npath={}\n",
		toml::Value::String(path.into())
	);
	author(raw.into_bytes(), Path::new("/"), &ENTRIES).is_ok()
}
