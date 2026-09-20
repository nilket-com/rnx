//! Versioned Git-source declarations and envelopes; legacy readers remain separate.
#![allow(dead_code)]
use crate::{artifact, input, manifest, new_identity, wire};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
fn digest(s: &str) -> bool {
	s.len() == 64
		&& s.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn git_coordinate(url: &str, rev: &str) -> Result<(), String> {
	if !(url.starts_with("https://") || url.starts_with("file:///"))
		|| url.len() > 4096
		|| url.contains(['\0', '\n', '\r', '?', '#'])
		|| url.split("://").nth(1).is_none_or(str::is_empty)
	{
		return Err("unsupported Git URL".into());
	}
	if rev.len() != 40
		|| !rev
			.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
	{
		return Err("full lowercase 40-hex Git revision required".into());
	}
	Ok(())
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Location {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub path: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub git: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rev: Option<String>,
}
impl Location {
	pub fn validate(&self) -> Result<(), String> {
		match (&self.path, &self.git, &self.rev) {
			(Some(p), None, None) => input::path(p),
			(None, Some(g), Some(r)) => git_coordinate(g, r),
			_ => Err("exactly path or git plus rev required".into()),
		}
	}
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Native {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub path: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub git: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rev: Option<String>,
	pub package: String,
	pub builder: String,
	pub hook: manifest::Hook,
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub presentation: bool,
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub shared_build: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Declaration {
	pub format: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub application: Option<manifest::Application>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub source: Option<manifest::Source>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub runtime: Option<Location>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub executable: Option<manifest::Location>,
	#[serde(default, deserialize_with = "unique_map")]
	pub sources: BTreeMap<String, manifest::Location>,
	#[serde(default, deserialize_with = "unique_map")]
	pub native: BTreeMap<String, Native>,
}
impl Declaration {
	pub fn validate(&self) -> Result<(), String> {
		if self.format != 2 {
			return Err("expected declaration format 2".into());
		}
		// Reuse the existing semantic/limit validator after replacing only native
		// locations with inert paths; this projection is never persisted or hashed.
		if let Some(r) = &self.runtime {
			r.validate()?;
		}
		for n in self.native.values() {
			Location {
				path: n.path.clone(),
				git: n.git.clone(),
				rev: n.rev.clone(),
			}
			.validate()?;
		}
		let mut v = serde_json::to_value(self).map_err(|e| e.to_string())?;
		v["format"] = 1.into();
		if let Some(r) = &self.runtime {
			v["runtime"] = serde_json::json!({"path":r.path.as_deref().unwrap_or("/probe/git")});
		}
		for (name, n) in &self.native {
			v["native"][name] = serde_json::json!({"path":n.path.as_deref().unwrap_or("/probe/git"),"package":n.package,"builder":n.builder,"hook":n.hook});
			if n.presentation {
				v["native"][name]["presentation"] = true.into();
			}
			if n.shared_build {
				v["native"][name]["shared_build"] = true.into();
			}
		}
		let old: manifest::Manifest = serde_json::from_value(v).map_err(|e| e.to_string())?;
		old.validate()
	}
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GitPackage {
	pub id: String,
	pub name: String,
	pub url: String,
	pub revision: String,
	pub checkout: String,
	pub manifest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lock {
	pub format: u32,
	pub declarations: Declaration,
	pub sources: wire::Handoff,
	pub inputs: wire::Inputs,
	pub git: Vec<GitPackage>,
	pub assembly: Assembly,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Assembly {
	Shared { identity: String },
	Executable { path: String, blake3: String },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
	pub format: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub assembly_key: Option<String>,
	pub lock_blake3: String,
	pub executable_blake3: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stamp: Option<artifact::Stamp>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ready {
	pub format: u32,
	pub key: String,
	pub identity: String,
	pub executable_blake3: String,
	pub artifact: String,
}
pub(crate) fn envelope(b: &[u8]) -> Result<u64, String> {
	if b.len() > input::DOCUMENT_LIMIT {
		return Err("document allowance".into());
	}
	let v: serde_json::Value = serde_json::from_slice(b).map_err(|e| e.to_string())?;
	v.get("format")
		.and_then(|v| v.as_u64())
		.ok_or("missing format".into())
}
pub(crate) fn parsed<T: serde::de::DeserializeOwned>(b: &[u8]) -> Result<T, String> {
	if b.len() > input::DOCUMENT_LIMIT {
		return Err("document allowance".into());
	}
	serde_json::from_slice(b).map_err(|e| e.to_string())
}
impl Lock {
	pub fn decode(b: &[u8]) -> Result<Self, String> {
		let d: Self = parsed(b)?;
		d.validate()?;
		Ok(d)
	}
	pub fn encode(&self) -> Result<Vec<u8>, String> {
		self.validate()?;
		wire::encode(self)
	}
	pub fn shared(&self) -> Result<Option<new_identity::Identity>, String> {
		match &self.assembly {
			Assembly::Shared { identity } => {
				new_identity::Identity::decode(identity.as_bytes()).map(Some)
			}
			_ => Ok(None),
		}
	}
	pub fn validate(&self) -> Result<(), String> {
		let d = self;
		if d.format != 4 {
			return Err("expected lock format 4".into());
		}
		d.declarations.validate()?;
		d.sources.validate()?;
		if d.declarations.application.is_none() {
			return Err("lock needs application".into());
		}
		// Existing source/path inventory bounds remain enforced by the old lock
		// validator on a shape-only executable projection. Git binding is checked below.
		let mut shape = serde_json::to_value(d).map_err(|e| e.to_string())?;
		shape.as_object_mut().unwrap().remove("git");
		shape["format"] = 3.into();
		shape["declarations"] = serde_json::json!({"format":1,"application":{"entry":"main.rn"},"executable":{"path":"/probe/executable"},"sources":{},"native":{}});
		shape["assembly"] = serde_json::json!({"kind":"executable","path":"/probe/executable","blake3":"0".repeat(64)});
		let old: wire::Lock = serde_json::from_value(shape).map_err(|e| e.to_string())?;
		old.validate()?;
		match &d.assembly {
			Assembly::Shared { identity } => {
				let i = new_identity::Identity::decode(identity.as_bytes())?;
				if d.declarations.runtime.is_none()
					|| d.inputs.native.as_ref() != Some(i.native())
					|| d.git != i.git()
				{
					return Err("identity/input binding mismatch".into());
				}
				bind(&d.declarations, i.wrapper().0)?;
			}
			Assembly::Executable { path, blake3 } => {
				if d.declarations.executable.is_none()
					|| d.inputs.native.is_some()
					|| !d.git.is_empty()
					|| !Path::new(path).is_absolute()
					|| !digest(blake3)
				{
					return Err("invalid override lock".into());
				}
			}
		}
		Ok(())
	}
}

fn bind(d: &Declaration, wrapper: &str) -> Result<(), String> {
	let doc: toml::Value = toml::from_str(wrapper).map_err(|e| e.to_string())?;
	let deps = doc
		.get("dependencies")
		.and_then(|v| v.as_table())
		.ok_or("missing dependencies")?;
	let Some(runtime) = &d.runtime else {
		return Err("missing runtime".into());
	};
	let check = |l: &Location, dep: &toml::Value| -> Result<(), String> {
		match (&l.path, &l.git, &l.rev) {
			(Some(p), None, None) => {
				if Path::new(p).is_absolute() && dep.get("path").and_then(|v| v.as_str()) != Some(p)
				{
					return Err("path binding".into());
				}
			}
			(None, Some(g), Some(r)) => {
				if dep.get("git").and_then(|v| v.as_str()) != Some(g)
					|| dep.get("rev").and_then(|v| v.as_str()) != Some(r)
				{
					return Err("Git binding".into());
				}
			}
			_ => return Err("location binding".into()),
		}
		Ok(())
	};
	check(
		runtime,
		deps.get("rnx").ok_or("missing runtime dependency")?,
	)?;
	if deps.len() != d.native.len() + 1 {
		return Err("native dependency count".into());
	}
	for (n, (_, native)) in d.native.iter().enumerate() {
		let dep = deps
			.get(&format!("native_{n}"))
			.ok_or("missing native alias")?;
		if dep.get("package").and_then(|v| v.as_str()) != Some(&native.package) {
			return Err("native package binding".into());
		}
		check(
			&Location {
				path: native.path.clone(),
				git: native.git.clone(),
				rev: native.rev.clone(),
			},
			dep,
		)?;
	}
	Ok(())
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

impl Declaration {
	pub fn parse(bytes: &[u8]) -> Result<Self, String> {
		if bytes.len() > input::MANIFEST_LIMIT {
			return Err("manifest allowance".into());
		}
		let d: Self = toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
			.map_err(|e| e.to_string())?;
		d.validate()?;
		Ok(d)
	}
	pub fn read(path: &Path) -> Result<Self, String> {
		Self::parse(&input::read(path, input::MANIFEST_LIMIT)?)
			.map_err(|e| format!("manifest {}: {e}", path.display()))
	}
	/// Only the graph fields are consumed; native locations are not resolved here.
	pub fn graph_manifest(&self) -> manifest::Manifest {
		manifest::Manifest {
			format: 1,
			application: self.application.clone(),
			source: self.source.clone(),
			runtime: None,
			executable: self.application.as_ref().map(|_| manifest::Location {
				path: "/unused-graph-runtime".into(),
			}),
			sources: self.sources.clone(),
			native: BTreeMap::new(),
		}
	}
}
impl Receipt {
	pub fn decode(bytes: &[u8]) -> Result<Self, String> {
		let r: Self = parsed(bytes)?;
		if r.format != 5 {
			return Err("unsupported receipt format (expected 5); run build".into());
		}
		let mut old = serde_json::to_value(&r).map_err(|e| e.to_string())?;
		old["format"] = 4.into();
		artifact::Receipt::decode(&wire::encode(&old)?)?;
		Ok(r)
	}
}

pub(crate) fn graph_read(path: &Path) -> Result<manifest::Manifest, String> {
	let bytes = input::read(path, input::MANIFEST_LIMIT)?;
	let v: toml::Value = toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
		.map_err(|e| e.to_string())?;
	if v.get("format").and_then(toml::Value::as_integer) == Some(2) {
		Ok(Declaration::parse(&bytes)?.graph_manifest())
	} else {
		let m: manifest::Manifest = v
			.try_into()
			.map_err(|e| format!("manifest {}: {e}", path.display()))?;
		m.validate()
			.map_err(|e| format!("manifest {}: {e}", path.display()))?;
		Ok(m)
	}
}

#[cfg(feature = "test-support")]
pub(crate) fn validate_vector(kind: &str, b: &[u8]) -> Result<Vec<u8>, String> {
	match kind {
		"declaration" => {
			let v: toml::Value = toml::from_str(std::str::from_utf8(b).map_err(|e| e.to_string())?)
				.map_err(|e| e.to_string())?;
			if v.get("format").and_then(toml::Value::as_integer) == Some(1) {
				wire::encode(&manifest::Manifest::parse(b)?)
			} else {
				wire::encode(&Declaration::parse(b)?)
			}
		}
		"identity" => {
			if envelope(b)? == 2 {
				Ok(crate::cache_identity::Identity::decode(b)?.bytes().to_vec())
			} else {
				Ok(new_identity::Identity::decode(b)?.bytes().to_vec())
			}
		}
		"canonical-identity" => new_identity::canonical(b),
		"lock" => {
			if envelope(b)? == 3 {
				wire::encode(&wire::Lock::decode(b)?)
			} else {
				Lock::decode(b)?.encode()
			}
		}
		"receipt" => {
			if envelope(b)? == 4 {
				artifact::Receipt::decode(b)?;
				Ok(b.to_vec())
			} else {
				wire::encode(&Receipt::decode(b)?)
			}
		}
		"ready" => {
			let d: Ready = parsed(b)?;
			let (key, format) = match d.format {
				2 => (
					crate::cache_identity::Identity::decode(d.identity.as_bytes())?
						.key()
						.to_owned(),
					2,
				),
				3 => (
					new_identity::Identity::decode(d.identity.as_bytes())?
						.key()
						.to_owned(),
					3,
				),
				_ => return Err("unsupported ready format".into()),
			};
			if d.format != format
				|| d.key != key
				|| !digest(&d.executable_blake3)
				|| d.artifact != format!("artifacts/{}", d.executable_blake3)
			{
				return Err("invalid ready binding".into());
			}
			wire::encode(&d)
		}
		_ => Err("unknown schema operation".into()),
	}
}
