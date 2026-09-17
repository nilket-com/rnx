#![allow(dead_code)]
use crate::{graph, input};
use serde::{Deserialize, Serialize};
use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
	pub format: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub application: Option<Application>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub source: Option<Source>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub runtime: Option<Location>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub executable: Option<Location>,
	#[serde(default, deserialize_with = "unique_map")]
	pub sources: BTreeMap<String, Location>,
	#[serde(default, deserialize_with = "unique_map")]
	pub native: BTreeMap<String, Native>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Application {
	pub entry: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
	pub root: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Location {
	pub path: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Native {
	pub path: String,
	pub package: String,
	pub builder: String,
	pub hook: Hook,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Hook {
	Plain,
	Lifecycle,
}

fn unique_map<'de, D, T>(de: D) -> Result<BTreeMap<String, T>, D::Error>
where
	D: serde::Deserializer<'de>,
	T: Deserialize<'de>,
{
	struct Map<T>(std::marker::PhantomData<T>);
	impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Map<T> {
		type Value = BTreeMap<String, T>;
		fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			f.write_str("up to 256 distinct declarations")
		}
		fn visit_map<A: serde::de::MapAccess<'de>>(
			self,
			mut map: A,
		) -> Result<Self::Value, A::Error> {
			let mut out = BTreeMap::new();
			while let Some(key) = map.next_key::<String>()? {
				if out.len() == 256 || out.contains_key(&key) {
					return Err(serde::de::Error::custom(
						"too many or duplicate declarations",
					));
				}
				out.insert(key, map.next_value()?);
			}
			Ok(out)
		}
	}
	de.deserialize_map(Map::<T>(std::marker::PhantomData))
}
pub(crate) fn builder(path: &str) -> bool {
	path.split("::").all(|s| {
		let mut chars = s.chars();
		chars
			.next()
			.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
			&& chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
			&& s != "_"
			&& !matches!(
				s,
				"Self"
					| "self" | "super"
					| "crate" | "fn"
					| "mod" | "pub" | "use"
					| "let" | "const"
					| "static" | "async"
					| "await" | "move"
					| "ref" | "mut" | "type"
					| "struct" | "enum"
					| "impl" | "trait"
					| "where" | "for"
					| "in" | "loop" | "while"
					| "if" | "else" | "match"
					| "return" | "break"
					| "continue" | "unsafe"
					| "extern" | "as"
					| "true" | "false"
					| "dyn" | "abstract"
					| "become" | "box"
					| "do" | "final"
					| "macro" | "override"
					| "priv" | "typeof"
					| "unsized" | "virtual"
					| "yield" | "try"
					| "gen"
			)
	})
}
impl Manifest {
	pub fn parse(bytes: &[u8]) -> Result<Self, String> {
		if bytes.len() > input::MANIFEST_LIMIT {
			return Err("manifest exceeds 1048576 bytes".into());
		}
		let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
		let manifest: Self = toml::from_str(text).map_err(|e| e.to_string())?;
		manifest.validate()?;
		Ok(manifest)
	}
	pub fn read(path: &Path) -> Result<Self, String> {
		Self::parse(&input::read(path, input::MANIFEST_LIMIT)?)
			.map_err(|e| format!("manifest {}: {e}", path.display()))
	}
	pub fn validate(&self) -> Result<(), String> {
		if self.format != 1 {
			return Err("unsupported manifest format (expected 1)".into());
		}
		match (&self.application, &self.source) {
			(Some(app), None) => {
				input::path(&app.entry)?;
				if self.runtime.is_some() == self.executable.is_some() {
					return Err("application requires exactly one runtime or executable".into());
				}
				if self.executable.is_some() && !self.native.is_empty() {
					return Err("executable override cannot declare native extensions".into());
				}
			}
			(None, Some(source)) => {
				input::path(&source.root)?;
				if self.runtime.is_some() || self.executable.is_some() || !self.native.is_empty() {
					return Err(
						"source package cannot declare runtime, executable or native extensions"
							.into(),
					);
				}
			}
			_ => return Err("manifest requires exactly one application or source table".into()),
		}
		for location in self.runtime.iter().chain(self.executable.iter()) {
			input::path(&location.path)?;
		}
		if self.sources.len() > 256 || self.native.len() > 256 {
			return Err("manifest exceeds 256 source or native declarations".into());
		}
		for (alias, location) in &self.sources {
			if !input::identifier(alias) {
				return Err(format!("invalid source alias `{alias}`"));
			}
			if self.application.is_some()
				&& (input::reserved(alias) || self.native.contains_key(alias))
			{
				return Err(format!(
					"source alias `{alias}` collides with a native name or battery"
				));
			}
			input::path(&location.path)?;
		}
		for (alias, native) in &self.native {
			if !input::identifier(alias) || input::reserved(alias) {
				return Err(format!("invalid or reserved native name `{alias}`"));
			}
			input::path(&native.path)?;
			if native.package.is_empty()
				|| !native
					.package
					.bytes()
					.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
			{
				return Err(format!("invalid Cargo package for `{alias}`"));
			}
			if !builder(&native.builder) {
				return Err(format!("invalid Rust builder path for `{alias}`"));
			}
		}
		Ok(())
	}
	pub fn graph_node(&self, path: &Path) -> Result<graph::Package, String> {
		let base = path.parent().ok_or("manifest has no parent")?;
		let root = if let Some(source) = &self.source {
			base.join(&source.root)
		} else {
			base.join(
				&self
					.application
					.as_ref()
					.ok_or("missing application")?
					.entry,
			)
			.parent()
			.ok_or("entry has no directory")?
			.to_owned()
		};
		Ok(graph::Package {
			root,
			dependencies: self
				.sources
				.iter()
				.map(|(name, location)| (name.clone(), base.join(&location.path).join("rnx.toml")))
				.collect(),
		})
	}
}
pub(crate) fn resolve(path: &Path) -> Result<(Manifest, PathBuf, Vec<graph::Mount>), String> {
	let path = path.canonicalize().map_err(|e| e.to_string())?;
	let app = Manifest::read(&path)?;
	let entry = path.parent().unwrap().join(
		&app.application
			.as_ref()
			.ok_or("project entry must be an application manifest")?
			.entry,
	);
	let mounts = graph::expand(&path, |p| {
		let manifest = if p == path {
			app.clone()
		} else {
			let m = Manifest::read(p)?;
			if m.source.is_none() {
				return Err(format!(
					"dependency {} is not a source package",
					p.display()
				));
			}
			m
		};
		manifest.graph_node(p)
	})?;
	Ok((app, entry, mounts))
}
