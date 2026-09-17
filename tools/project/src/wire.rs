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
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
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
	serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
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
	pub packages: Vec<Package>,
	pub assembly: Assembly,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Assembly {
	Generated {
		manifest_sha256: String,
		main_sha256: String,
		cargo_lock_sha256: String,
		target: String,
		profile: String,
		features: Vec<String>,
		rustc: String,
		cargo: String,
	},
	Executable {
		path: String,
		sha256: String,
	},
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Package {
	pub root: String,
	pub manifest: String,
	pub tree_sha256: String,
	pub files: Vec<File>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct File {
	pub path: String,
	pub executable: bool,
	pub bytes: u64,
	pub sha256: String,
}
impl Lock {
	pub fn validate(&self) -> Result<(), String> {
		version(self.format)?;
		self.declarations.validate()?;
		self.sources.validate()?;
		if self.declarations.application.is_none() {
			return Err("project lock needs application declarations".into());
		}
		let mut files = 0usize;
		let mut bytes = 0u64;
		let mut roots = BTreeSet::new();
		for package in &self.packages {
			absolute(&package.root)?;
			absolute(&package.manifest)?;
			digest(&package.tree_sha256)?;
			if !roots.insert(&package.root) {
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
				digest(&file.sha256)?;
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
			Assembly::Generated {
				manifest_sha256,
				main_sha256,
				cargo_lock_sha256,
				target,
				profile,
				features,
				rustc,
				cargo,
			} => {
				if self.declarations.runtime.is_none() {
					return Err("generated lock needs a runtime declaration".into());
				}
				for hash in [manifest_sha256, main_sha256, cargo_lock_sha256] {
					digest(hash)?;
				}
				if [target, profile, rustc, cargo].iter().any(|s| s.is_empty()) {
					return Err(
						"generated lock needs target, profile and toolchain identities".into(),
					);
				}
				if features.iter().any(|f| f.is_empty())
					|| features.iter().collect::<BTreeSet<_>>().len() != features.len()
				{
					return Err("features must be nonempty and unique".into());
				}
			}
			Assembly::Executable { path, sha256 } => {
				if self.declarations.executable.is_none() {
					return Err("override lock needs an executable declaration".into());
				}
				absolute(path)?;
				digest(sha256)?;
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
