//! Typed persisted documents. Syntax/shape validation is not content verification.
#![allow(dead_code)]
use crate::{input, manifest::Manifest};
use serde::{Deserialize, Serialize};
use std::{
	collections::BTreeSet,
	path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Handoff {
	pub format: u32,
	pub entry: String,
	pub mounts: Vec<Mount>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Mount {
	pub prefix: Vec<String>,
	pub root: String,
}
fn absolute(text: &str) -> Result<(), String> {
	input::path(text)?;
	if !Path::new(text).is_absolute() {
		return Err("document requires absolute paths".into());
	}
	Ok(())
}
fn digest(text: &str) -> Result<(), String> {
	if text.len() != 64
		|| !text
			.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
	{
		return Err("digest must be 64 lowercase hexadecimal characters".into());
	}
	Ok(())
}
fn prefix(parts: &[String]) -> Result<(), String> {
	if parts.is_empty() || parts.len() > 16 || !parts.iter().all(|s| input::identifier(s)) {
		return Err("prefix needs 1 to 16 valid Rune identifiers".into());
	}
	Ok(())
}
fn version(n: u32) -> Result<(), String> {
	if n != 1 {
		return Err("unsupported document format (expected 1)".into());
	}
	Ok(())
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
	if bytes.len() > input::DOCUMENT_LIMIT {
		return Err("document exceeds 16777216 bytes".into());
	}
	serde_json::from_slice(bytes).map_err(|e| e.to_string())
}
pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
	encoded(value, false)
}
pub(crate) fn pretty<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
	encoded(value, true)
}
fn encoded<T: Serialize>(value: &T, pretty: bool) -> Result<Vec<u8>, String> {
	// Cap while serializing, including any escaping expansion.
	struct Writer(Vec<u8>);
	impl std::io::Write for Writer {
		fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
			if bytes.len() > input::DOCUMENT_LIMIT.saturating_sub(self.0.len()) {
				return Err(std::io::Error::other("document exceeds 16777216 bytes"));
			}
			self.0.extend_from_slice(bytes);
			Ok(bytes.len())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}
	let mut writer = Writer(vec![]);
	if pretty {
		serde_json::to_writer_pretty(&mut writer, value)
	} else {
		serde_json::to_writer(&mut writer, value)
	}
	.map_err(|e| e.to_string())?;
	Ok(writer.0)
}
impl Handoff {
	pub fn validate(&self) -> Result<(), String> {
		version(self.format)?;
		absolute(&self.entry)?;
		if self.mounts.len() > 256 {
			return Err("source map exceeds 256 mounts".into());
		}
		let mut seen = BTreeSet::new();
		for mount in &self.mounts {
			prefix(&mount.prefix)?;
			absolute(&mount.root)?;
			if !seen.insert(&mount.prefix) {
				return Err("duplicate source prefix".into());
			}
		}
		Ok(())
	}
	pub fn decode(bytes: &[u8]) -> Result<Self, String> {
		let doc: Self = parse(bytes)?;
		doc.validate()?;
		Ok(doc)
	}
	pub fn encode(&self) -> Result<Vec<u8>, String> {
		self.validate()?;
		encode(self)
	}
	pub fn from_project(path: &Path) -> Result<Self, String> {
		let (_, entry, mounts) = crate::manifest::resolve(path)?;
		let string = |p: PathBuf| {
			p.into_os_string()
				.into_string()
				.map_err(|_| "project path is not Unicode".to_owned())
		};
		let handoff = Self {
			format: 1,
			entry: string(entry)?,
			mounts: mounts
				.into_iter()
				.map(|m| {
					Ok(Mount {
						prefix: m.prefix,
						root: string(m.root)?,
					})
				})
				.collect::<Result<_, String>>()?,
		};
		handoff.validate()?;
		Ok(handoff)
	}
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lock {
	pub format: u32,
	pub declarations: Manifest,
	pub sources: Handoff,
	pub inputs: Inputs,
	pub assembly: Assembly,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Assembly {
	Shared {
		identity: String,
	},
	Generated {
		manifest_blake3: String,
		main_blake3: String,
		cargo_lock_blake3: String,
		target: String,
		profile: String,
		features: Vec<String>,
		rustc: String,
		cargo: String,
	},
	Executable {
		path: String,
		blake3: String,
	},
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
	pub source: crate::inventory::Sources,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub native: Option<crate::inventory::Inventory>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct File {
	pub path: String,
	pub executable: bool,
	pub bytes: u64,
	pub blake3: String,
}
impl Lock {
	pub fn shared(&self) -> Result<Option<crate::cache_identity::Identity>, String> {
		match &self.assembly {
			Assembly::Shared { identity } => {
				crate::cache_identity::Identity::decode(identity.as_bytes()).map(Some)
			}
			_ => Ok(None),
		}
	}
	pub fn validate(&self) -> Result<(), String> {
		if self.format != 3 {
			return Err("unsupported lock format (expected 3); run lock and build".into());
		}
		self.declarations.validate()?;
		self.sources.validate()?;
		if self.declarations.application.is_none() {
			return Err("project lock needs application declarations".into());
		}
		let mut files = 0usize;
		let mut bytes = 0u64;
		let mut roots = BTreeSet::new();
		for (kind, package) in self.inputs.source.trees.iter().map(|p| (0, p)).chain(
			self.inputs
				.native
				.iter()
				.flat_map(|n| n.trees.iter())
				.map(|p| (1, p)),
		) {
			absolute(package.root.to_str().ok_or("non-Unicode root")?)?;
			digest(&package.blake3)?;
			if !roots.insert((kind, &package.root)) {
				return Err("duplicate package root".into());
			}
			let mut paths = BTreeSet::new();
			for file in &package.files {
				input::path(&file.path)?;
				if file.path.contains('\\')
					|| file
						.path
						.split('/')
						.any(|p| p.is_empty() || p == "." || p == "..")
					|| Path::new(&file.path).is_absolute()
				{
					return Err("inventory path must be a normalized relative slash path".into());
				}
				if !paths.insert(&file.path) {
					return Err("duplicate inventory path".into());
				}
				digest(&file.blake3)?;
				files += 1;
				bytes = bytes
					.checked_add(file.bytes)
					.ok_or("inventory size overflow")?;
				if files > 100_000 || bytes > 512 * 1024 * 1024 {
					return Err("inventory exceeds 100000 entries or 512 MiB".into());
				}
			}
		}

		if self.inputs.source.packages.len() > 64 {
			return Err("too many source packages".into());
		}
		for p in &self.inputs.source.packages {
			absolute(p.manifest.to_str().ok_or("non-Unicode manifest")?)?;
			absolute(p.root.to_str().ok_or("non-Unicode source root")?)?;
			if !self.inputs.source.trees.iter().any(|t| t.root == p.root) {
				return Err("source package has no tree".into());
			}
		}
		if let Some(n) = &self.inputs.native {
			if n.platform.is_empty() || n.packages.len() > 100_000 || n.external.len() > 100_000 {
				return Err("invalid native inventory bounds".into());
			}
			let mut ids = BTreeSet::new();
			for p in &n.packages {
				absolute(p.manifest.to_str().ok_or("non-Unicode manifest")?)?;
				absolute(p.root.to_str().ok_or("non-Unicode native root")?)?;
				if p.name.is_empty()
					|| !ids.insert(&p.id)
					|| !n.trees.iter().any(|t| t.root == p.root)
				{
					return Err("invalid native package association".into());
				}
			}
		}
		let mut external_paths = BTreeSet::new();
		for external in self
			.inputs
			.source
			.outside_manifests
			.iter()
			.chain(self.inputs.native.iter().flat_map(|n| n.external.iter()))
		{
			absolute(external.path.to_str().ok_or("non-Unicode external path")?)?;
			// The same file can legitimately govern both source and native roots.
			if !external_paths.insert(&external.path) {
				continue;
			}
			if let Some(file) = &external.file {
				digest(&file.blake3)?;
				if Some(std::ffi::OsStr::new(&file.path)) != external.path.file_name() {
					return Err("external file path mismatch".into());
				}
				files += 1;
				bytes = bytes
					.checked_add(file.bytes)
					.ok_or("inventory size overflow")?;
				if files > 100_000 || bytes > 512 * 1024 * 1024 {
					return Err("inventory exceeds 100000 entries or 512 MiB".into());
				}
			}
		}
		match &self.assembly {
			Assembly::Shared { identity } => {
				let identity = crate::cache_identity::Identity::decode(identity.as_bytes())?;
				if self.declarations.runtime.is_none()
					|| self.inputs.native.as_ref() != Some(identity.native())
				{
					return Err("shared identity and native inventory disagree".into());
				}
			}
			Assembly::Generated { .. } => {
				return Err("local-generated locks are superseded; run lock and build".into());
			}
			Assembly::Executable { path, blake3 } => {
				if self.declarations.executable.is_none() {
					return Err("override lock needs an executable declaration".into());
				}
				absolute(path)?;
				digest(blake3)?;
			}
		}
		Ok(())
	}
	pub fn decode(bytes: &[u8]) -> Result<Self, String> {
		let doc: Self = parse(bytes)?;
		doc.validate()?;
		Ok(doc)
	}
	pub fn encode(&self) -> Result<Vec<u8>, String> {
		self.validate()?;
		encode(self)
	}
	pub fn read(path: &Path) -> Result<Self, String> {
		Self::decode(&input::read(path, input::DOCUMENT_LIMIT)?)
			.map_err(|e| format!("lock {}: {e}", path.display()))
	}
}
