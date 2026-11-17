//! An invocation-owned disposable build directory, as `owned_build.py`'s
//! `OwnedBuild` keeps one: claimed with an ownership record, released
//! once its build has ended, its kept outputs and top-level diagnostics
//! retained and their digests journaled. Never a sweep or a shared cache.
use crate::child::{CONTEXT, value_error};
use crate::failure::Environment;
use crate::receipt::{atomic_json, digest, invocation_id};
use anyhow::{Context as _, Result, bail, ensure};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Owned {
	parent: PathBuf,
	path: PathBuf,
	record: PathBuf,
	owner: Value,
	retained: Vec<PathBuf>,
}

impl Owned {
	/// Claim `parent/name`, which must not exist yet.
	pub fn new(parent: &Path, name: &str) -> Result<Self> {
		let canonical = std::fs::canonicalize(parent)?;
		if canonical != parent
			|| Path::new(name).file_name().map(|n| n.to_string_lossy()) != Some(name.into())
			|| matches!(name, "." | ".." | "")
		{
			bail!(value_error("noncanonical build owner or outside path"));
		}
		let path = parent.join(name);
		if path.symlink_metadata().is_ok() {
			bail!(value_error("shared or preexisting build path"));
		}
		let record = parent.join(format!("{name}-OWNER.json"));
		if record.exists() {
			bail!(value_error("ownership already recorded"));
		}
		let meta = std::fs::metadata(parent)?;
		let owner = json!({"schema": 1, "path": path.to_string_lossy(), "parent": parent.to_string_lossy(),
			"parent_device": meta.dev(), "parent_inode": meta.ino(), "invocation": invocation_id()});
		atomic_json(&record, &owner)?;
		Ok(Self {
			parent: parent.to_owned(),
			path,
			record,
			owner,
			retained: Vec::new(),
		})
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Outputs kept outside the build directory after its release.
	pub fn keep(&mut self, paths: &[&Path]) {
		self.retained.extend(paths.iter().map(|p| p.to_path_buf()));
	}

	/// Remove the build directory, once nothing builds in it, and journal
	/// the release beside it (and, inside a final, for the stage's
	/// context). `original` is the error the build ended with, if any; a
	/// failed release is an error only when there was none.
	pub fn release(self, original: Option<&anyhow::Error>, env: &Environment) -> Result<Value> {
		let started = Instant::now();
		let mut row = Map::new();
		row.insert("status".into(), json!("FAIL"));
		row.insert("owner".into(), self.owner.clone());
		row.insert(
			"original_error".into(),
			original.map_or(Value::Null, |e| json!(format!("{e}"))),
		);
		match self.remove() {
			Ok(done) => {
				row.extend(done);
				row.insert("status".into(), json!("PASS"));
				row.insert("removed".into(), json!(true));
			}
			Err(error) => {
				row.insert("error".into(), json!(format!("{error:#}")));
			}
		}
		row.insert("seconds".into(), json!(started.elapsed().as_secs_f64()));
		let row = Value::Object(row);
		let name = self.path.file_name().unwrap_or_default().to_string_lossy();
		let journal = self.parent.join(format!("{name}-RELEASE.json"));
		atomic_json(&journal, &row)?;
		if let Some(context) = env.get(CONTEXT).filter(|c| !c.is_empty()) {
			let context = Path::new(context);
			let invocation = self.owner["invocation"].as_str().unwrap_or_default();
			atomic_json(
				&context
					.parent()
					.context("context without directory")?
					.join(format!("CLEANUP-{invocation}.json")),
				&json!({"path": journal.to_string_lossy(), "sha256": digest(&journal)?,
					"seconds": row["seconds"], "status": row["status"]}),
			)?;
		}
		if row["status"] != "PASS" && original.is_none() {
			bail!(
				"build removal failed: {}",
				row["error"].as_str().unwrap_or_default()
			);
		}
		Ok(row)
	}

	fn remove(&self) -> Result<Map<String, Value>> {
		let recorded: Value = serde_json::from_slice(&std::fs::read(&self.record)?)?;
		ensure!(recorded == self.owner, "ownership changed");
		let meta = std::fs::metadata(&self.parent)?;
		ensure!(
			std::fs::canonicalize(&self.parent)? == self.parent
				&& json!(meta.dev()) == self.owner["parent_device"]
				&& json!(meta.ino()) == self.owner["parent_inode"],
			"build owner replaced"
		);
		if let Ok(meta) = self.path.symlink_metadata() {
			ensure!(
				!meta.file_type().is_symlink() && std::fs::canonicalize(&self.path)? == self.path,
				"symlink or outside build path"
			);
		}
		// Top-level diagnostics are kept too, a failed build's included.
		let mut paths: BTreeSet<PathBuf> = self.retained.iter().cloned().collect();
		for entry in std::fs::read_dir(&self.parent)? {
			let entry = entry?;
			if entry.file_type()?.is_file() {
				paths.insert(entry.path());
			}
		}
		let mut hashes = Map::new();
		for p in &paths {
			let p = std::fs::canonicalize(p)?;
			ensure!(
				!p.starts_with(&self.path),
				"build output is retained evidence"
			);
			std::fs::File::open(&p)?.sync_all()?;
			hashes.insert(p.to_string_lossy().into_owned(), json!(digest(&p)?));
		}
		crate::receipt::sync_directory(&self.parent)?;
		let (mut logical, mut allocated, mut seen) = (0u64, 0u64, BTreeSet::new());
		if self.path.exists() {
			let mut stack = vec![self.path.clone()];
			while let Some(directory) = stack.pop() {
				for entry in std::fs::read_dir(&directory)? {
					let entry = entry?;
					let meta = entry.path().symlink_metadata()?;
					// As os.walk without following links: a link to a
					// directory is neither walked nor counted.
					let linked_directory = meta.file_type().is_symlink()
						&& std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir());
					if meta.is_dir() {
						stack.push(entry.path());
					} else if !linked_directory && seen.insert((meta.dev(), meta.ino())) {
						logical += meta.len();
						allocated += meta.blocks() * 512;
					}
				}
			}
			std::fs::remove_dir_all(&self.path)?;
		}
		ensure!(
			self.path.symlink_metadata().is_err(),
			"build output remains"
		);
		for (p, h) in &hashes {
			ensure!(digest(Path::new(p))? == *h, "retained evidence changed");
		}
		let mut row = Map::new();
		row.insert("logical_file_bytes".into(), json!(logical));
		row.insert("allocated_file_bytes".into(), json!(allocated));
		row.insert("retained".into(), Value::Object(hashes));
		Ok(row)
	}
}
