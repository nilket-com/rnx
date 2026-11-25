//! Final-owned bulk roots, their release, recovery after a killed final,
//! and the compact manifest that revalidation reads after release.
use crate::pyjson;
use crate::receipt::{Field, atomic_json, digest, invocation_id, sync_directory};
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const CAP: usize = 256 * 1024;

/// An absolute path with no link or `.`/`..` anywhere in it.
pub fn canonical(path: &Path) -> Result<PathBuf> {
	let path = if path.is_absolute() {
		path.to_owned()
	} else {
		std::env::current_dir()?.join(path)
	};
	let path = lexical(&path);
	let resolved = resolve(&path);
	ensure!(resolved == path, "symlink or noncanonical owner path");
	Ok(path)
}

/// Python's `Path.absolute()` leaves `..` in place; `resolve()` follows links.
fn lexical(path: &Path) -> PathBuf {
	path.components()
		.filter(|c| !matches!(c, std::path::Component::CurDir))
		.collect()
}

/// `Path.resolve()` (non-strict): follow links as far as the path exists.
fn resolve(path: &Path) -> PathBuf {
	if let Ok(real) = std::fs::canonicalize(path) {
		return real;
	}
	match (path.parent(), path.file_name()) {
		(Some(parent), Some(name)) => resolve(parent).join(name),
		_ => path.to_owned(),
	}
}

fn lock(path: &Path, create_new: bool) -> Result<File> {
	let file = if create_new {
		std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(path)?
	} else {
		std::fs::OpenOptions::new()
			.read(true)
			.append(true)
			.create(true)
			.open(path)?
	};
	// SAFETY: flock on a descriptor this function owns.
	if unsafe {
		libc::flock(
			std::os::fd::AsRawFd::as_raw_fd(&file),
			libc::LOCK_EX | libc::LOCK_NB,
		)
	} != 0
	{
		let error = std::io::Error::last_os_error();
		if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
			bail!("live final owner: {}", path.display());
		}
		return Err(error.into());
	}
	Ok(file)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Root {
	pub path: String,
	pub parent: String,
	pub stage: String,
	pub state: String,
	pub parent_device: u64,
	pub parent_inode: u64,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub device: Field<u64>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub inode: Field<u64>,
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub creation: Field<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
	/// 1 is what the retired Python controller wrote, still read for its
	/// finals; this controller writes 2.
	pub schema: u64,
	pub invocation: String,
	pub out: String,
	pub groups: String,
	pub roots: Vec<Root>,
	pub status: String,
	/// The final's own systemd unit's control group, when it runs in one:
	/// the containment recovery relies on when a guardian was killed.
	#[serde(default, skip_serializing_if = "Field::is_absent")]
	pub unit: Field<String>,
}

fn read_ledger(path: &Path) -> Result<Ledger> {
	let value: Value = serde_json::from_slice(&std::fs::read(path)?)?;
	ensure!(
		value.get("schema") == Some(&json!(1)) || value.get("schema") == Some(&json!(2)),
		"unknown ownership schema"
	);
	Ok(serde_json::from_value(value)?)
}

/// The roots one final owns, journaled before each is created.
pub struct Owners {
	pub registry: PathBuf,
	pub out: PathBuf,
	pub id: String,
	pub path: PathBuf,
	pub groups: PathBuf,
	pub row: Ledger,
	_lock: File,
}

impl Owners {
	pub fn new(registry: &Path, out: &Path) -> Result<Self> {
		Self::with_unit(registry, out, None)
	}

	pub fn with_unit(registry: &Path, out: &Path, unit: Option<&str>) -> Result<Self> {
		let registry = canonical(registry)?;
		std::fs::create_dir_all(&registry)?;
		let out = canonical(out)?;
		std::fs::create_dir_all(&out)?;
		let id = invocation_id();
		let path = registry.join(format!("{id}.owner.json"));
		let groups = registry.join(format!("{id}.groups"));
		std::fs::create_dir(&groups)?;
		let lock = lock(&path.with_extension("lock"), true)?;
		let row = Ledger {
			schema: 2,
			unit: unit.map_or(Field::Absent, |u| Field::Value(u.to_owned())),
			invocation: id.clone(),
			out: out.to_string_lossy().into_owned(),
			groups: groups.to_string_lossy().into_owned(),
			roots: Vec::new(),
			status: "active".into(),
		};
		atomic_json(&path, &row)?;
		Ok(Self {
			registry,
			out,
			id,
			path,
			groups,
			row,
			_lock: lock,
		})
	}

	/// A new root `parent/name`, journaled as creating before it exists.
	pub fn assign(&mut self, parent: &Path, label: &str, name: Option<&str>) -> Result<PathBuf> {
		self.claim(parent, label, name, &std::fs::DirBuilder::new())
	}

	pub fn assign_private(&mut self, parent: &Path, label: &str) -> Result<PathBuf> {
		use std::os::unix::fs::DirBuilderExt;
		self.claim(parent, label, None, std::fs::DirBuilder::new().mode(0o700))
	}

	fn claim(
		&mut self,
		parent: &Path,
		label: &str,
		name: Option<&str>,
		builder: &std::fs::DirBuilder,
	) -> Result<PathBuf> {
		let parent = canonical(parent)?;
		std::fs::create_dir_all(&parent)?;
		let name = name.map_or_else(|| format!("{}-{label}", self.id), str::to_owned);
		ensure!(
			!name.is_empty() && name != "." && name != ".." && !name.contains('/'),
			"outside owner name"
		);
		let path = parent.join(&name);
		ensure!(
			std::fs::symlink_metadata(&path).is_err(),
			"preexisting evidence root"
		);
		let before = std::fs::metadata(&parent)?;
		self.row.roots.push(Root {
			path: path.to_string_lossy().into_owned(),
			parent: parent.to_string_lossy().into_owned(),
			stage: label.into(),
			state: "creating".into(),
			parent_device: before.dev(),
			parent_inode: before.ino(),
			device: Field::Absent,
			inode: Field::Absent,
			creation: Field::Absent,
		});
		atomic_json(&self.path, &self.row)?;
		builder.create(&path)?;
		sync_directory(&parent)?;
		let (made, after) = (std::fs::metadata(&path)?, std::fs::metadata(&parent)?);
		let entry = self.row.roots.last_mut().expect("just pushed");
		entry.state = "owned".into();
		entry.device = Field::Value(made.dev());
		entry.inode = Field::Value(made.ino());
		entry.parent_device = after.dev();
		entry.parent_inode = after.ino();
		atomic_json(&self.path, &self.row)?;
		Ok(path)
	}

	pub fn release(&mut self) -> Result<Value> {
		let result = release(&self.path)?;
		self.row = read_ledger(&self.path)?;
		Ok(result)
	}

	/// Drop one stage's scratch when its users drain. The outer ownership
	/// journal remains available to the manifest and to crash recovery.
	pub fn release_child(&self, stage: &str, name: &str) -> Result<()> {
		ensure!(
			Path::new(name).file_name() == Some(name.as_ref()),
			"invalid child"
		);
		let root = self
			.row
			.roots
			.iter()
			.find(|root| root.stage == stage)
			.context("missing scratch owner")?;
		let path = canonical(Path::new(&root.path))?;
		let meta = path.metadata()?;
		ensure!(
			root.state == "owned"
				&& root.device == Field::Value(meta.dev())
				&& root.inode == Field::Value(meta.ino()),
			"scratch owner changed"
		);
		let child = path.join(name);
		match child.symlink_metadata() {
			Ok(meta) => {
				ensure!(meta.is_dir(), "scratch directory replaced");
				std::fs::remove_dir_all(child)?;
				sync_directory(&path)?;
			}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
			Err(e) => return Err(e.into()),
		}
		Ok(())
	}

	pub fn retire(&self) -> Result<()> {
		retire(&self.path)
	}

	/// Give up the owner's lock, as a killed final would.
	pub fn close(self) {}
}

/// Remove every owned root whose identity still matches, journaling each.
pub fn release(ledger: &Path) -> Result<Value> {
	let mut row = read_ledger(ledger)?;
	let mut results = Vec::new();
	for index in 0..row.roots.len() {
		let entry = row.roots[index].clone();
		let path = PathBuf::from(&entry.path);
		let parent = canonical(Path::new(&entry.parent))?;
		let name = path
			.file_name()
			.map(|n| n.to_string_lossy().into_owned())
			.unwrap_or_default();
		ensure!(
			path.parent() == Some(parent.as_path())
				&& !name.is_empty()
				&& name != "."
				&& name != "..",
			"outside owned parent"
		);
		let stat = std::fs::metadata(&parent)?;
		ensure!(
			(stat.dev(), stat.ino()) == (entry.parent_device, entry.parent_inode),
			"parent identity mismatch"
		);
		let exists = std::fs::symlink_metadata(&path).ok();
		match entry.state.as_str() {
			"released" => {
				ensure!(exists.is_none(), "released root reappeared");
				results.push(entry);
				continue;
			}
			"creating" => {
				if let Some(meta) = &exists {
					canonical(&path)?;
					ensure!(
						meta.is_dir(),
						"incomplete root is not directory: {}",
						path.display()
					);
					// Never adopt or recursively remove a root with no recorded inode.
					std::fs::remove_dir(&path).map_err(|_| {
						anyhow::anyhow!("incomplete nonempty root refused: {}", path.display())
					})?;
					sync_directory(&parent)?;
				}
				let entry = &mut row.roots[index];
				entry.state = "released".into();
				entry.creation = Field::Value("resolved without adoption".into());
				atomic_json(ledger, &row)?;
				results.push(row.roots[index].clone());
				continue;
			}
			"owned" | "releasing" => {}
			_ => bail!("invalid root state: {}", path.display()),
		}
		if let Some(meta) = exists {
			canonical(&path)?;
			ensure!(
				meta.is_dir()
					&& Field::Value(meta.dev()) == entry.device
					&& Field::Value(meta.ino()) == entry.inode,
				"root identity mismatch"
			);
			if entry.state == "owned" {
				row.roots[index].state = "releasing".into();
				atomic_json(ledger, &row)?;
			}
			// remove_dir_all unlinks interior links; it never follows them.
			std::fs::remove_dir_all(&path)?;
			sync_directory(&parent)?;
		} else if entry.state != "releasing" {
			bail!("owned root disappeared before release");
		}
		row.roots[index].state = "released".into();
		atomic_json(ledger, &row)?;
		results.push(row.roots[index].clone());
	}
	row.status = "released".into();
	atomic_json(ledger, &row)?;
	Ok(json!({"status": "PASS", "ledger_sha256": digest(ledger)?, "roots": results}))
}

pub fn group_directory(ledger: &Path, row: &Ledger) -> Result<PathBuf> {
	let path = canonical(Path::new(&row.groups))?;
	let expected = ledger
		.parent()
		.unwrap_or(Path::new(""))
		.join(format!("{}.groups", row.invocation));
	ensure!(
		path == expected && path.is_dir(),
		"missing or invalid producer registry: {}",
		path.display()
	);
	Ok(path)
}

/// Forget a released owner: its group registry, lock and ledger.
pub fn retire(ledger: &Path) -> Result<()> {
	let mut row = read_ledger(ledger)?;
	ensure!(
		row.status == "released" || row.status == "retiring",
		"cannot retire unreleased owner"
	);
	let groups = canonical(Path::new(&row.groups))?;
	ensure!(
		groups
			== ledger
				.parent()
				.unwrap_or(Path::new(""))
				.join(format!("{}.groups", row.invocation)),
		"invalid group registry"
	);
	row.status = "retiring".into();
	atomic_json(ledger, &row)?;
	// The release record is already durable; recovery no longer needs the groups.
	if groups.exists() {
		std::fs::remove_dir_all(&groups)?;
	}
	let _ = std::fs::remove_file(ledger.with_extension("lock"));
	std::fs::remove_file(ledger)?;
	sync_directory(ledger.parent().unwrap_or(Path::new("/")))
}

/// Release the roots of every final that died without releasing, from the
/// registry alone. `drain` stops each recorded producer group first.
pub fn recover(
	registry: &Path,
	drain: impl Fn(&Path, &Ledger) -> Result<()>,
) -> Result<Vec<String>> {
	let registry = canonical(registry)?;
	std::fs::create_dir_all(&registry)?;
	let mut ledgers: Vec<PathBuf> = std::fs::read_dir(&registry)?
		.filter_map(|e| e.ok().map(|e| e.path()))
		.filter(|p| p.to_string_lossy().ends_with(".owner.json"))
		.collect();
	ledgers.sort();
	let mut rows = Vec::new();
	for ledger in ledgers {
		canonical(&ledger)?;
		let initial = read_ledger(&ledger)?;
		let lock_path = ledger.with_extension("lock");
		let _held = if initial.status == "retiring" {
			lock(&lock_path, false)?
		} else {
			let file = std::fs::OpenOptions::new()
				.read(true)
				.write(true)
				.open(&lock_path)?;
			// SAFETY: flock on a descriptor this function owns.
			if unsafe {
				libc::flock(
					std::os::fd::AsRawFd::as_raw_fd(&file),
					libc::LOCK_EX | libc::LOCK_NB,
				)
			} != 0
			{
				bail!("live final owner: {}", ledger.display());
			}
			file
		};
		let row = read_ledger(&ledger)?;
		if row.status == "retiring" {
			retire(&ledger)?;
			continue;
		}
		let groups = group_directory(&ledger, &row)?;
		drain(&groups, &row)?;
		let mut result = release(&ledger)?;
		let mut records: Vec<PathBuf> = std::fs::read_dir(&groups)?
			.filter_map(|e| e.ok().map(|e| e.path()))
			.filter(|p| {
				p.file_name().is_some_and(|n| {
					n.to_string_lossy().starts_with("GROUP-")
						&& n.to_string_lossy().ends_with(".json")
				})
			})
			.collect();
		records.sort();
		let producer_groups: Vec<Value> = records
			.iter()
			.map(|p| Ok(serde_json::from_slice(&std::fs::read(p)?)?))
			.collect::<Result<_>>()?;
		result["interrupted"] = json!(true);
		result["producer_groups"] = json!(producer_groups);
		let receipt = registry.join(format!("{}-RECOVERY.json", row.invocation));
		atomic_json(&receipt, &result)?;
		retire(&ledger)?;
		rows.push(receipt.to_string_lossy().into_owned());
	}
	Ok(rows)
}

fn mode_kind(meta: &std::fs::Metadata) -> u32 {
	meta.mode() & libc::S_IFMT
}

/// A payload's identity: link text (never its target) and file contents,
/// sorted by relative name. `known` reuses digests the final already took.
pub fn tree(path: &Path, known: &Map<String, Value>) -> Result<Value> {
	let started = Instant::now();
	let (mut rows, mut logical, mut allocated, mut reused) = (Vec::new(), 0u64, 0u64, 0u64);
	let mut stack = vec![path.to_owned()];
	while let Some(directory) = stack.pop() {
		let mut names: Vec<_> = std::fs::read_dir(&directory)?
			.map(|e| e.map(|e| e.file_name()))
			.collect::<std::io::Result<_>>()?;
		names.sort();
		for name in names {
			let p = directory.join(&name);
			let meta = std::fs::symlink_metadata(&p)?;
			let kind = mode_kind(&meta);
			let value = if meta.file_type().is_symlink() {
				Value::String(pyjson::hex(&Sha256::digest(
					std::fs::read_link(&p)?.as_os_str().as_encoded_bytes(),
				)))
			} else if meta.is_file() {
				match known.get(&*p.to_string_lossy()) {
					Some(Value::String(known)) => {
						reused += 1;
						Value::String(known.clone())
					}
					_ => Value::String(digest(&p)?),
				}
			} else if meta.is_dir() {
				stack.push(p.clone());
				Value::Null
			} else {
				bail!("unsupported payload file type: {}", p.display());
			};
			let size = if value.is_null() { 0 } else { meta.size() };
			let relative = p
				.strip_prefix(path)?
				.to_str()
				.with_context(|| format!("payload name is not UTF-8: {}", p.display()))?
				.to_owned();
			rows.push(json!([relative, kind, size, value]));
			logical += meta.size();
			allocated += meta.blocks() * 512;
		}
	}
	rows.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
	Ok(json!({
		"sha256": pyjson::identity(&rows)?, "entries": rows.len(), "logical_bytes": logical,
		"allocated_bytes": allocated, "reused_file_digests": reused, "seconds": started.elapsed().as_secs_f64(),
	}))
}

/// The manifest that outlives the released bulk: each stage's receipt
/// digest and artifact-set identity, and each root's payload identity.
pub fn compact(
	selection: &Value,
	evidence: &Map<String, Value>,
	owners: &Owners,
	tools: &Value,
) -> Result<Value> {
	let mut stages = Vec::new();
	let mut known = Map::new();
	for stage in selection["stages"]
		.as_array()
		.context("selection has no stages")?
	{
		let (name, command) = (stage[0].as_str().unwrap_or_default(), &stage[1]);
		let Some(reference) = evidence.get(name) else {
			continue;
		};
		let row: Value = serde_json::from_slice(&std::fs::read(
			reference["path"].as_str().unwrap_or_default(),
		)?)?;
		if row["status"] == "PASS"
			&& let Some(artifacts) = row["artifacts"].as_object()
		{
			known.extend(artifacts.clone());
		}
		let mut names: Vec<Value> = row["context"]["environment"]["keys"]
			.as_array()
			.cloned()
			.unwrap_or_default();
		names.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
		stages.push(json!({
			"name": name, "command_template": command, "status": row["status"], "exit": row["process_exit"],
			"seconds": row["seconds"], "cleanup_seconds": row["cleanup_seconds"], "environment_names": names,
			"receipt_sha256": reference["sha256"], "artifact_set_sha256": pyjson::identity(&row["artifacts"])?,
		}));
	}
	let roots: Vec<Value> = owners
		.row
		.roots
		.iter()
		.map(|entry| Ok(json!({"owner": entry, "payload": tree(Path::new(&entry.path), &known)?})))
		.collect::<Result<_>>()?;
	let mut manifest = json!({
		"schema": 1, "candidate": selection["record"]["candidate"], "source": selection["source"], "policy": selection["policy"],
		"acceptance_scope": selection.get("scope").cloned().unwrap_or(Value::Null), "selection_sha256": pyjson::identity(selection)?,
		"tools": tools, "stages": stages, "roots": roots,
	});
	if selection["record"]["phase"] == "fix" {
		manifest["fix_subject"] = selection["record"]["subject"].clone();
	}
	ensure!(
		pyjson::pretty(&manifest)?.len() - 1 <= CAP,
		"compact manifest exceeds 256 KiB"
	);
	Ok(manifest)
}

pub fn publication_eligible(scope: &str) -> Result<bool> {
	ensure!(
		["required", "complete", "explicit", "docs"].contains(&scope),
		"unknown final scope"
	);
	Ok(scope == "required" || scope == "complete")
}

fn read(path: &Path) -> Result<Value> {
	Ok(serde_json::from_slice(
		&std::fs::read(path).with_context(|| format!("read {}", path.display()))?,
	)?)
}

/// A final that declares its schedule binds it as a terminal record, and
/// the record holds exactly its selected stages, consistently. A final from
/// before ADR 0288 declares none and binds none.
pub fn check_schedule(
	out: &Path,
	result: &crate::types::FinalResult,
	acceptance: &crate::types::Acceptance,
) -> Result<()> {
	let declared = acceptance.schedule_sha256.value();
	let bound = result.records.get("SCHEDULE");
	ensure!(
		declared == bound,
		"the declared schedule is not the bound one"
	);
	let Some(bound) = bound else {
		return Ok(());
	};
	ensure!(
		digest(&out.join("SCHEDULE.json"))? == *bound,
		"SCHEDULE differs from its terminal record"
	);
	let schedule: crate::schedule::Schedule =
		serde_json::from_slice(&std::fs::read(out.join("SCHEDULE.json"))?)?;
	let selection: crate::types::Selection = crate::types::read(&out.join("SELECTION.json"))?;
	let names: Vec<String> = selection.stages.iter().map(|(n, _)| n.clone()).collect();
	crate::schedule::validate(&schedule, &names)
}

/// Check a released final from its terminal records alone, each read as
/// its declared type.
pub fn revalidate(out: &Path) -> Result<Value> {
	use crate::types::{self, Acceptance, FinalResult, Manifest, Release, Status};
	let result: FinalResult = types::read(&out.join("RESULT.json"))?;
	for name in ["ACCEPTANCE", "MANIFEST", "RELEASE"] {
		ensure!(
			result.records.contains_key(name),
			"terminal record missing: {name}"
		);
	}
	// Every record the final bound: the three above and any other it kept.
	for (name, recorded) in &result.records {
		ensure!(
			!name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase()),
			"malformed terminal record name: {name}"
		);
		ensure!(
			digest(&out.join(format!("{name}.json")))? == *recorded,
			"terminal record changed: {name}"
		);
	}
	if result.status == Status::Pass
		&& let Some(hash) = result.records.get("CACHE")
	{
		let released = read(&out.join("CACHE-RELEASE.json"))?;
		let binding = if released["schema"] == 2 {
			let txn = read(&out.join("CACHE.json"))?;
			txn["complete"] == true
				&& txn["release_sha256"] == digest(&out.join("CACHE-RELEASE.json"))?
		} else {
			released["transaction"] == *hash
				&& released["result"] == digest(&out.join("RESULT.json"))?
		};
		ensure!(
			binding && released["footprint"] == digest(&out.join("CACHE-FOOTPRINT.json"))?,
			"cache sweep receipt mismatch"
		);
	}
	let a: Acceptance = types::read(&out.join("ACCEPTANCE.json"))?;
	let m: Manifest = types::read(&out.join("MANIFEST.json"))?;
	let manifest = serde_json::to_value(&m)?;
	if let Some(run) = manifest["tools"]["controller"]["bootstrap"].get("clean_run")
		&& result.acceptance_scope != types::Scope::Docs
	{
		ensure!(
			result.records.contains_key("CLEANUP"),
			"missing cleanup evidence"
		);
		let record: crate::clean::Cleanup =
			serde_json::from_value(read(&out.join("CLEANUP.json"))?)?;
		ensure!(
			run["directory"] == record.directory
				&& run["baseline"] == record.baseline
				&& manifest["candidate"] == record.candidate
				&& (record.elapsed()? - result.seconds).abs() < 0.001,
			"cleanup does not describe this run"
		);
	}

	if manifest["tools"]["controller"]["bootstrap"]
		.get("checkpoint")
		.is_some()
		|| result.records.contains_key("CHECKPOINT")
	{
		ensure!(
			result.records.contains_key("CHECKPOINT"),
			"missing checkpoint timing"
		);
		let timing = serde_json::from_value(read(&out.join("CHECKPOINT.json"))?)?;
		crate::checkpoint::revalidate(
			out,
			&manifest,
			&timing,
			result
				.candidate
				.as_deref()
				.context("no checkpoint candidate")?,
			result.seconds,
		)?;
		let selected = read(&out.join("SELECTION.json"))?;
		ensure!(
			result.records.contains_key("CHECKPOINTRELEASE"),
			"missing checkpoint release"
		);
		let released = read(&out.join("CHECKPOINTRELEASE.json"))?;
		ensure!(
			released["developer_state_sha256"] == timing.admission.developer_state_sha256
				&& released["terminal_state_sha256"]
					.as_str()
					.is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
			"checkpoint release identity differs"
		);
		ensure!(
			selected["record"]["baseline"] == timing.admission.baseline,
			"different published baseline in selection"
		);
	}
	let r: Release = types::read(&out.join("RELEASE.json"))?;
	check_schedule(out, &result, &a)?;
	ensure!(
		result.status == Status::Pass && a.status == Status::Pass && r.status == Status::Pass,
		"final not complete"
	);
	ensure!(
		result.candidate == m.candidate && a.candidate == m.candidate && m.candidate.is_some(),
		"candidate differs"
	);
	ensure!(
		result.fix_subject == m.fix_subject && !matches!(m.fix_subject, Field::Null),
		"fix subject differs"
	);
	if let Some(subject) = m.fix_subject.value() {
		let selection = read(&out.join("SELECTION.json"))?;
		let typed: types::Selection = types::conform(&selection, "fix selection")?;
		let types::Candidate::Fix(fix) = typed.record else {
			bail!("fix has no fix selection");
		};
		ensure!(
			m.selection_sha256 == Field::Value(pyjson::identity(&selection)?)
				&& Some(&fix.candidate) == m.candidate.as_ref()
				&& typed.scope == result.acceptance_scope
				&& &fix.subject == subject
				&& crate::selection::fix_subject(subject),
			"fix selection differs"
		);
	}
	ensure!(
		r.acceptance_sha256.value() == result.records.get("ACCEPTANCE")
			&& r.manifest_sha256.value() == result.records.get("MANIFEST"),
		"release not bound"
	);
	let Field::Value(released) = &r.roots else {
		anyhow::bail!("incomplete release: no released roots")
	};
	let manifest_paths: Vec<&String> = m.roots.iter().map(|x| &x.owner.path).collect();
	let released_paths: Vec<&String> = released.iter().map(|x| &x.path).collect();
	ensure!(
		released_paths == manifest_paths
			&& released.iter().all(|x| x.state == "released")
			&& m.incomplete.is_absent(),
		"incomplete release"
	);
	ensure!(
		Field::Value(a.acceptance_scope) == m.acceptance_scope
			&& a.acceptance_scope == result.acceptance_scope,
		"scope differs"
	);
	let scope = result.acceptance_scope;
	ensure!(
		result.publication_eligible == publication_eligible(scope.name())?
			&& result.complete_final == (scope == types::Scope::Complete),
		"scope eligibility differs"
	);
	let mut verdict = json!({
		"status": "PASS", "candidate": m.candidate, "bulk": "released; not rehashed", "release": result.records["RELEASE"],
		"acceptance_scope": scope.name(), "complete_final": result.complete_final, "publication_eligible": result.publication_eligible,
	});
	if let Some(subject) = result.fix_subject.value() {
		verdict["fix_subject"] = json!(subject);
	}
	Ok(verdict)
}

/// Remove a failed final's general diagnostics on request, recording it.
pub fn abandon_diagnostics(out: &Path) -> Result<()> {
	let result = read(&out.join("RESULT.json"))?;
	ensure!(
		result.get("status") == Some(&json!("FAIL")),
		"only failed final diagnostics can be abandoned"
	);
	let path = out.join("DIAGNOSTICS.json");
	ensure!(
		!std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
		"diagnostic symlink"
	);
	let sha = digest(&path)?;
	atomic_json(
		&out.join("DIAGNOSTICS-RELEASE-INTENT.json"),
		&json!({"sha256": sha, "reason": "explicit abandon"}),
	)?;
	std::fs::remove_file(&path)?;
	sync_directory(out)?;
	atomic_json(
		&out.join("DIAGNOSTICS-RELEASE.json"),
		&json!({"status": "PASS", "sha256": sha, "reason": "explicit abandon"}),
	)
}
