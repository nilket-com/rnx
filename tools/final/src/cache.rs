//! Persistent Cargo cache ownership and per-recipe compiler artifact marks.
#[path = "cache_plan.rs"]
mod plan;
use crate::receipt::{atomic_json, invocation_id, sync_directory};
use anyhow::{Context, Result, ensure};
pub use plan::{CachePlan, Route, cargo_main, checkpoint_recipes, disk_declaration, lookup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, ErrorKind};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Identity {
	device: u64,
	inode: u64,
}

impl Identity {
	fn of(path: &Path) -> Result<Self> {
		let meta = path.symlink_metadata()?;
		Ok(Self {
			device: meta.dev(),
			inode: meta.ino(),
		})
	}
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
	file: Identity,
	changed: (i64, i64),
	/// Resolved Cargo unit (package, target, profile, features) for this recipe.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	unit: Option<String>,
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	test: bool,
}

pub type Marks = BTreeMap<PathBuf, Artifact>;

impl Artifact {
	fn read(path: &Path) -> Result<Self> {
		let m = path.symlink_metadata()?;
		ensure!(
			m.is_file(),
			"Cargo artifact is not a regular file: {}",
			path.display()
		);
		Ok(Self {
			file: Identity {
				device: m.dev(),
				inode: m.ino(),
			},
			changed: (m.ctime(), m.ctime_nsec()),
			unit: None,
			test: false,
		})
	}
}

/// An existing directory must have the marker written *before* the claim
/// was renamed into place. A directory copied or replaced at the same path
/// has a different identity. The caller holds its parent's lease.
pub fn claim(path: &Path) -> Result<PathBuf> {
	let parent = path.parent().context("cache has no parent")?;
	ensure!(
		parent.canonicalize()? == parent,
		"noncanonical cache parent"
	);
	let marker = ".rnx-cache-owner.json";
	if let Err(e) = path.symlink_metadata() {
		if e.kind() != ErrorKind::NotFound {
			return Err(e.into());
		}
		let tmp = parent.join(format!(".claim-{}", invocation_id()));
		fs::create_dir(&tmp)?;
		let id = Identity::of(&tmp)?;
		atomic_json(&tmp.join(marker), &(path, id))?;
		// RENAME_NOREPLACE also preserves a foreign empty directory that
		// appears between the initial check and the claim.
		use std::ffi::CString;
		let a = CString::new(tmp.as_os_str().as_encoded_bytes())?;
		let b = CString::new(path.as_os_str().as_encoded_bytes())?;
		// SAFETY: both paths are live NUL-terminated strings.
		if unsafe {
			libc::renameat2(
				libc::AT_FDCWD,
				a.as_ptr(),
				libc::AT_FDCWD,
				b.as_ptr(),
				libc::RENAME_NOREPLACE,
			)
		} != 0
		{
			let error = std::io::Error::last_os_error();
			fs::remove_file(tmp.join(marker))?;
			fs::remove_dir(tmp)?;
			return Err(error.into());
		}
		sync_directory(parent)?;
	}
	check_owner(path)?;
	Ok(path.to_owned())
}

fn check_owner(path: &Path) -> Result<()> {
	ensure!(
		path.canonicalize()? == path && path.is_dir(),
		"noncanonical cache directory"
	);
	let marker = path.join(".rnx-cache-owner.json");
	ensure!(marker.symlink_metadata()?.is_file(), "invalid cache owner");
	let (claimed, id): (PathBuf, Identity) = serde_json::from_slice(&fs::read(&marker)?)?;
	ensure!(
		claimed == path && id == Identity::of(path)?,
		"cache ownership changed"
	);
	Ok(())
}

fn matches_file(stamp: &Artifact, path: &Path) -> Result<bool> {
	let current = Artifact::read(path)?;
	Ok(current.file == stamp.file && current.changed == stamp.changed)
}

fn output_path(rel: &Path) -> bool {
	rel.components().count() >= 2
		&& rel.components().all(|c| {
			matches!(c, Component::Normal(name)
		if !name.as_encoded_bytes().starts_with(b".") && name != "incremental")
		})
}

/// Capture exactly the outputs Cargo named. A partial observation can add
/// positive evidence but cannot replace a recipe's previous manifest.
fn record_outputs(lane: &Path, msg: &Value, marks: &mut Marks) -> Result<bool> {
	let names = msg["filenames"]
		.as_array()
		.context("Cargo artifact lacks filenames")?;
	let exe = msg["executable"].as_str();
	let unit = crate::pyjson::identity(&(
		&msg["package_id"],
		&msg["features"],
		&msg["profile"],
		&msg["target"],
	))?;
	let mut complete = true;
	for value in names
		.iter()
		.chain(msg.get("executable").filter(|v| !v.is_null()))
	{
		let name = value.as_str().context("invalid Cargo output")?;
		let path = Path::new(name);
		let rel = path
			.strip_prefix(lane)
			.context("Cargo output outside its lane")?;
		ensure!(output_path(rel), "invalid Cargo output: {rel:?}");
		match path.symlink_metadata() {
			Ok(m) if m.is_file() && path.canonicalize()? == path => {
				let mut stamp = Artifact::read(path)?;
				stamp.unit = Some(unit.clone());
				stamp.test = exe == Some(name) && msg["profile"]["test"] == true;
				marks.insert(rel.to_owned(), stamp);
			}
			Ok(_) => complete = false,
			Err(e) if e.kind() == ErrorKind::NotFound => complete = false,
			Err(e) => return Err(e.into()),
		}
	}
	Ok(complete)
}

/// Cargo's structured output precedes the program or libtest output.
/// A fresh artifact is as authoritative as one just rebuilt. An absent
/// report is never evidence that the previous executable disappeared.
fn stream_marks(lane: &Path, output: impl BufRead) -> Result<Option<Marks>> {
	let mut marks = Marks::new();
	for line in output.lines() {
		let line = line?;
		if !line.starts_with('{') {
			continue;
		}
		let msg: Value = serde_json::from_str(&line)?;
		match msg["reason"].as_str() {
			Some("build-finished") => {
				ensure!(msg["success"] == true, "Cargo build did not succeed");
				return Ok((!marks.is_empty()).then_some(marks));
			}
			Some("compiler-artifact") if !record_outputs(lane, &msg, &mut marks)? => {
				return Ok(None);
			}
			_ => {}
		}
	}
	Ok(None)
}

/// Retain proofs of replaced tests until removal has finished. If the
/// process dies midway, another owner can replay the pending removals.
#[derive(Default, Serialize, Deserialize)]
struct Ledger {
	recipes: BTreeMap<String, Marks>,
	pending: Marks,
}

pub struct Lane {
	path: PathBuf,
	_lease: File,
	bootstrap: bool,
	inherited: bool,
}

impl Drop for Lane {
	fn drop(&mut self) {
		// A concurrently forked child can briefly hold the same open file
		// description before exec closes its inherited descriptors.
		if !self.inherited {
			let _ = self._lease.unlock();
		}
	}
}

impl Lane {
	pub fn open(path: &Path) -> Result<Self> {
		check_owner(path)?;
		let lease = File::open(path)?;
		lease.try_lock().context("Cargo target lane is leased")?;
		check_owner(path)?;
		ensure!(
			lease.metadata()?.ino() == Identity::of(path)?.inode,
			"cache replaced during lock"
		);
		Ok(Self {
			path: path.to_owned(),
			_lease: lease,
			bootstrap: false,
			inherited: false,
		})
	}

	/// final.sh holds descriptor 9 on the bootstrap directory across exec.
	/// A standalone recovery must acquire that same lease before replay.
	fn bootstrap(root: &Path) -> Result<Self> {
		use std::os::fd::FromRawFd;
		let path = root.join("target/final-controller");
		let owner = path.join("OWNER");
		ensure!(
			owner.canonicalize()? == owner && owner.symlink_metadata()?.is_file(),
			"invalid bootstrap owner"
		);
		let stamp = |p: &Path| -> Result<String> {
			let id = Identity::of(p)?;
			Ok(format!("{}:{}\n", id.device, id.inode))
		};
		let directory = File::open(&path)?;
		let dir = directory.metadata()?;
		// SAFETY: F_DUPFD_CLOEXEC returns a new owned descriptor on success.
		let fd = unsafe { libc::fcntl(9, libc::F_DUPFD_CLOEXEC, 3) };
		let inherited = (fd >= 0)
			.then(|| unsafe { File::from_raw_fd(fd) })
			.filter(|f| {
				f.metadata()
					.is_ok_and(|m| (m.dev(), m.ino()) == (dir.dev(), dir.ino()))
			});
		let borrowed = inherited.is_some();
		let lease = inherited.unwrap_or(directory);
		lease.try_lock().context("bootstrap target is leased")?;
		ensure!(
			fs::read_to_string(&owner)? == stamp(root)? + &stamp(&path)?,
			"bootstrap owner changed"
		);
		Ok(Self {
			path,
			_lease: lease,
			bootstrap: true,
			inherited: borrowed,
		})
	}

	fn ledger(&self) -> Result<Ledger> {
		match fs::read(self.path.join(".rnx-marks.json")) {
			Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
			Err(e) if e.kind() == ErrorKind::NotFound => Ok(Ledger::default()),
			Err(e) => Err(e.into()),
		}
	}

	/// Report allocated blocks once per inode, distinguishing positively marked tests.
	pub fn footprint(&self) -> Result<Value> {
		let ledger = self.ledger()?;
		let mut tests = BTreeSet::new();
		let mut all = BTreeSet::new();
		let observations = self.observations()?;
		for marks in ledger.recipes.values().chain(observations.values()) {
			for (rel, artifact) in marks {
				let path = self.path.join(rel);
				if path.symlink_metadata().is_ok_and(|m| m.is_file())
					&& matches_file(artifact, &path)?
				{
					let key = (artifact.file.device, artifact.file.inode);
					all.insert(key);
					if artifact.test {
						tests.insert(key);
					}
				}
			}
		}
		let (mut test_bytes, mut non_executable) = (0, 0);
		let allocated = self.allocated_bytes(|m, bytes| {
			if m.is_file() && m.mode() & 0o111 == 0 {
				non_executable += bytes;
			}
			if tests.contains(&(m.dev(), m.ino())) {
				test_bytes += bytes;
			}
		})?;
		Ok(
			serde_json::json!({"allocated_bytes":allocated,"marked_outputs":all.len(),"marked_executable_bytes":test_bytes,"other_bytes":allocated-test_bytes,"non_executable_regular_bytes":non_executable,"marked_executables":tests.len(),"uncommitted_observation_recipes":observations.len()}),
		)
	}

	/// Count allocated disk blocks once per inode without following symlinks.
	fn allocated_bytes(&self, mut visit: impl FnMut(&fs::Metadata, u64)) -> Result<u64> {
		let mut allocated = 0;
		let mut seen = BTreeSet::new();
		let mut stack = vec![self.path.clone()];
		while let Some(path) = stack.pop() {
			let m = path.symlink_metadata()?;
			if !seen.insert((m.dev(), m.ino())) {
				continue;
			}
			let bytes = m.blocks() * 512;
			allocated += bytes;
			visit(&m, bytes);
			if m.is_dir() {
				for e in fs::read_dir(path)? {
					stack.push(e?.path());
				}
			}
		}
		Ok(allocated)
	}

	/// Called only for committed successful runs. Untouched recipes retain
	/// their proofs; catalogue retirement is permitted only for complete runs.
	pub fn commit(
		&self,
		updates: BTreeMap<String, Marks>,
		live: &BTreeSet<String>,
		complete: bool,
	) -> Result<u64> {
		let file = self.path.join(".rnx-marks.json");
		let mut ledger = self.ledger()?;
		// Positive observations can outlive a failed or interrupted run.
		// Only a replacement report (or catalogue retirement) makes an
		// older observation eligible for deletion.
		let mut consumed = Vec::new();
		let mut latest = Marks::new();
		for (recipe, marks) in self.observations()? {
			if updates.get(&recipe).is_some_and(|m| !m.is_empty())
				|| (complete && !live.contains(&recipe))
			{
				latest.extend(marks);
				consumed.push(self.path.join(".rnx-observed").join(recipe));
			}
		}
		let incomplete = self.path.join(".rnx-incomplete");
		if incomplete.try_exists()? {
			for entry in fs::read_dir(&incomplete)? {
				let entry = entry?;
				let recipe = entry.file_name();
				let name = recipe.to_str().context("invalid incomplete recipe")?;
				if !name.starts_with('.')
					&& (updates.get(name).is_some_and(|m| !m.is_empty())
						|| complete && !live.contains(name))
				{
					consumed.push(entry.path());
				}
			}
		}
		for (recipe, marks) in updates {
			ensure!(live.contains(&recipe), "mark for retired recipe");
			if !marks.is_empty()
				&& let Some(old) = ledger.recipes.insert(recipe, marks)
			{
				ledger.pending.extend(old);
			}
		}
		if complete {
			ledger.recipes.retain(|recipe, old| {
				if live.contains(recipe) {
					true
				} else {
					ledger.pending.extend(old.clone());
					false
				}
			});
		}
		ledger.pending.extend(latest);
		for marks in ledger.recipes.values() {
			for p in marks.keys() {
				ledger.pending.remove(p);
			}
		}
		atomic_json(&file, &ledger)?;
		for path in consumed {
			fs::remove_file(path)?;
		}
		if !complete {
			return Ok(0);
		}
		self.sweep(&file, ledger)
	}

	fn reclaim_committed(&self) -> Result<u64> {
		let file = self.path.join(".rnx-marks.json");
		if !file.try_exists()? {
			return Ok(0);
		}
		self.sweep(&file, serde_json::from_slice(&fs::read(&file)?)?)
	}

	fn observations(&self) -> Result<BTreeMap<String, Marks>> {
		let root = self.path.join(".rnx-observed");
		let mut rows = BTreeMap::new();
		if root.try_exists()? {
			for entry in fs::read_dir(&root)? {
				let entry = entry?;
				let recipe = entry
					.file_name()
					.into_string()
					.map_err(|_| anyhow::anyhow!("invalid observed recipe"))?;
				// atomic_json may leave its unpublished temporary on power loss.
				if !recipe.starts_with('.') {
					rows.insert(recipe, serde_json::from_slice(&fs::read(entry.path())?)?);
				}
			}
		}
		Ok(rows)
	}

	fn sweep(&self, file: &Path, mut ledger: Ledger) -> Result<u64> {
		let incomplete = self.path.join(".rnx-incomplete");
		if incomplete.try_exists()? {
			for entry in fs::read_dir(&incomplete)? {
				if !entry?.file_name().as_encoded_bytes().starts_with(b".") {
					return Ok(0);
				}
			}
		}
		if ledger.pending.is_empty() {
			return Ok(0);
		}
		// A successful replacement from one recipe cannot invalidate a
		// second live recipe's unfinished report for the same path.
		let observations = self.observations()?;
		let mut protected: BTreeSet<PathBuf> = observations
			.values()
			.flat_map(|marks| marks.keys().cloned())
			.collect();
		if self.bootstrap {
			// The shell can already have rebuilt the next candidate before
			// recovery replays a previous PASS. Protect its positive report.
			let source = File::open(self.path.join("bootstrap-build.jsonl"))?;
			let Some(current) = stream_marks(&self.path, std::io::BufReader::new(source))? else {
				return Ok(0);
			};
			protected.extend(current.into_keys());
		}
		// Removing one name changes ctime on its hard links. Validate all
		// names first and hold the inode across removal of its proven links.
		let mut files = BTreeMap::<(u64, u64), Vec<(PathBuf, (i64, i64))>>::new();
		for (rel, stamp) in &ledger.pending {
			if protected.contains(rel) {
				continue;
			}
			ensure!(output_path(rel), "invalid deletion record");
			let path = self.path.join(rel);
			match fs::symlink_metadata(&path) {
				Ok(m)
					if m.is_file()
						&& path.canonicalize()? == path
						&& matches_file(stamp, &path)? =>
				{
					files
						.entry((m.dev(), m.ino()))
						.or_default()
						.push((path, stamp.changed));
				}
				Ok(_) => {}
				Err(e) if e.kind() == ErrorKind::NotFound => {}
				Err(e) => return Err(e.into()),
			}
		}
		let mut bytes = 0;
		let mut relinked = BTreeMap::new();
		for (identity, names) in files {
			let pin = File::open(&names[0].0)?;
			let original = names[0].1;
			let mut changed = original;
			for (path, _) in names {
				let m = path.symlink_metadata()?;
				let pinned = pin.metadata()?;
				if (m.dev(), m.ino()) == identity
					&& (pinned.dev(), pinned.ino()) == identity
					&& (pinned.ctime(), pinned.ctime_nsec()) == changed
					&& path.canonicalize()? == path
				{
					fs::remove_file(path)?;
					if m.nlink() == 1 {
						bytes += m.blocks() * 512;
					}
					let after = pin.metadata()?;
					changed = (after.ctime(), after.ctime_nsec());
				}
			}
			if changed != original {
				relinked.insert((identity, original), changed);
			}
		}
		ledger.pending.retain(|p, _| protected.contains(p));
		for stamp in ledger
			.recipes
			.values_mut()
			.flat_map(|m| m.values_mut())
			.chain(ledger.pending.values_mut())
		{
			if let Some(new) = relinked.get(&((stamp.file.device, stamp.file.inode), stamp.changed))
			{
				stamp.changed = *new;
			}
		}
		atomic_json(file, &ledger)?;
		Ok(bytes)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::process::Command;

	#[test]
	fn sweeping_a_reported_alias_preserves_the_live_links_identity() {
		let root = std::env::temp_dir().join(format!("rnx-linked-{}", invocation_id()));
		fs::create_dir(&root).unwrap();
		let target = claim(&root.join("target")).unwrap();
		let lane = Lane::open(&target).unwrap();
		fs::create_dir(target.join("debug")).unwrap();
		let a = target.join("debug/a");
		let b = target.join("debug/b");
		fs::write(&a, b"shared").unwrap();
		fs::hard_link(&a, &b).unwrap();
		let marks = |p: &Path| {
			Marks::from([(
				p.strip_prefix(&target).unwrap().to_owned(),
				Artifact::read(p).unwrap(),
			)])
		};
		lane.commit(
			BTreeMap::from([("a".into(), marks(&a)), ("b".into(), marks(&b))]),
			&BTreeSet::from(["a".into(), "b".into()]),
			true,
		)
		.unwrap();
		assert_eq!(
			lane.commit(BTreeMap::new(), &BTreeSet::from(["b".into()]), true)
				.unwrap(),
			0
		);
		assert!(!a.exists() && b.exists());
		assert_eq!(lane.footprint().unwrap()["marked_outputs"], 1);
		lane.commit(BTreeMap::new(), &BTreeSet::new(), true)
			.unwrap();
		assert!(!b.exists());
		drop(lane);
		fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn an_interrupted_unflushed_stream_defers_deletion_until_its_recipe_reports() {
		let root = std::env::temp_dir().join(format!("rnx-unflushed-{}", invocation_id()));
		fs::create_dir(&root).unwrap();
		let target = claim(&root.join("target")).unwrap();
		let lane = Lane::open(&target).unwrap();
		fs::create_dir(target.join("debug")).unwrap();
		let old = target.join("debug/a");
		let new = target.join("debug/b");
		fs::write(&old, b"a").unwrap();
		fs::write(&new, b"b").unwrap();
		let report = |path: &Path| {
			Marks::from([(
				path.strip_prefix(&target).unwrap().to_owned(),
				Artifact::read(path).unwrap(),
			)])
		};
		let live = BTreeSet::from(["first".into(), "other".into()]);
		lane.commit(
			BTreeMap::from([("first".into(), report(&old))]),
			&live,
			true,
		)
		.unwrap();
		let incomplete = target.join(".rnx-incomplete");
		fs::create_dir(&incomplete).unwrap();
		atomic_json(&incomplete.join("other"), &true).unwrap();
		lane.commit(
			BTreeMap::from([("first".into(), report(&new))]),
			&live,
			true,
		)
		.unwrap();
		lane.reclaim_committed().unwrap();
		assert!(old.is_file());
		lane.commit(
			BTreeMap::from([("other".into(), report(&new))]),
			&live,
			true,
		)
		.unwrap();
		assert!(!old.exists() && new.is_file());
		drop(lane);
		fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn cargo_reports_fresh_tests_and_the_union_survives_a_partial_run() {
		let root = std::env::temp_dir().join(format!("rnx-cache-test-{}", invocation_id()));
		fs::create_dir(&root).unwrap();
		struct Scratch(PathBuf);
		impl Drop for Scratch {
			fn drop(&mut self) {
				let _ = fs::remove_dir_all(&self.0);
			}
		}
		let _scratch = Scratch(root.clone());
		let path = claim(&root.join("lane")).unwrap();
		let leased = Lane::open(&path).unwrap();
		assert_eq!(leased.footprint().unwrap()["marked_executables"], 0);
		assert!(Lane::open(&path).is_err());
		fs::create_dir(root.join("src")).unwrap();
		fs::write(
			root.join("Cargo.toml"),
			"[package]\nname='mark-test'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ncommon={path='common'}\noptional={path='optional', optional=true}\n[features]\nextra=['dep:optional']\n",
		)
		.unwrap();
		fs::write(root.join("src/lib.rs"), "#[test] fn case() {}\n").unwrap();
		for dep in ["common", "optional"] {
			let dir = root.join(dep);
			fs::create_dir_all(dir.join("src")).unwrap();
			fs::write(
				dir.join("Cargo.toml"),
				format!("[package]\nname='{dep}'\nversion='0.1.0'\nedition='2024'\n"),
			)
			.unwrap();
			fs::write(dir.join("src/lib.rs"), "").unwrap();
		}
		let build = |args: &[&str]| {
			let result = Command::new("cargo")
				.current_dir(&root)
				.args([
					"+1.98.1",
					"test",
					"--offline",
					"--lib",
					"--no-run",
					"--message-format=json",
					"--target-dir",
				])
				.arg(&path)
				.args(args)
				.output()
				.unwrap();
			assert!(
				result.status.success(),
				"{}",
				String::from_utf8_lossy(&result.stderr)
			);
			if args.contains(&"extra") {
				let msgs: Vec<Value> = result
					.stdout
					.split(|b| *b == b'\n')
					.filter_map(|line| serde_json::from_slice(line).ok())
					.collect();
				assert!(
					msgs.iter()
						.any(|m| m["target"]["name"] == "common" && m["fresh"] == true)
				);
			}
			let fresh = result
				.stdout
				.split(|b| *b == b'\n')
				.filter_map(|line| serde_json::from_slice::<Value>(line).ok())
				.filter(|m| m["reason"] == "compiler-artifact")
				.all(|m| m["fresh"] == true);
			(
				stream_marks(&path, result.stdout.as_slice())
					.unwrap()
					.unwrap(),
				fresh,
			)
		};
		let (old, fresh) = build(&[]);
		assert!(!fresh);
		let (again, fresh) = build(&[]);
		assert!(fresh);
		assert_eq!(again, old);
		let live = BTreeSet::from(["first".into(), "second".into()]);
		leased
			.commit(
				BTreeMap::from([("first".into(), old.clone()), ("second".into(), again)]),
				&live,
				true,
			)
			.unwrap();
		let (new, _) = build(&["--features", "extra"]);
		assert!(new.keys().any(|p| !old.contains_key(p)));
		leased
			.commit(
				BTreeMap::from([("first".into(), new.clone())]),
				&live,
				false,
			)
			.unwrap();
		let old_bin = path.join(old.iter().find(|(_, v)| v.test).unwrap().0);
		let new_bin = path.join(new.iter().find(|(_, v)| v.test).unwrap().0);
		assert!(old_bin.exists() && new_bin.exists());
		assert!(stream_marks(&path, &b""[..]).unwrap().is_none());
		assert!(
			stream_marks(
				&path,
				&b"{\"reason\":\"build-finished\",\"success\":false}\n"[..]
			)
			.is_err()
		);
		leased.commit(BTreeMap::new(), &live, true).unwrap();
		assert!(
			old_bin.exists(),
			"the second producer still needs this test"
		);
		let pending = path.join(".rnx-observed");
		fs::create_dir(&pending).unwrap();
		atomic_json(&pending.join("interrupted"), &old).unwrap();
		let mut live = live;
		live.insert("interrupted".into());
		let foreign = old_bin.with_extension("unrelated");
		fs::write(&foreign, b"foreign").unwrap();
		leased
			.commit(
				BTreeMap::from([("second".into(), new.clone())]),
				&live,
				true,
			)
			.unwrap();
		assert!(
			old_bin.exists(),
			"unfinished co-owner still observed the old test"
		);
		leased.reclaim_committed().unwrap();
		assert!(old_bin.exists());
		leased
			.commit(BTreeMap::from([("interrupted".into(), new)]), &live, true)
			.unwrap();
		assert!(!old_bin.exists());
		assert!(new_bin.exists() && foreign.exists());
		// Retiring the extra configuration must drop its library and metadata,
		// while leaving every output used by the default build warm.
		let (default, _) = build(&[]);
		let (extra, _) = build(&["--features", "extra"]);
		let optional: Vec<_> = extra
			.keys()
			.filter(|p| p.to_string_lossy().contains("liboptional"))
			.cloned()
			.collect();
		assert!(optional.iter().any(|p| p.extension().unwrap() == "rlib"));
		assert!(optional.iter().any(|p| p.extension().unwrap() == "rmeta"));
		leased
			.commit(
				BTreeMap::from([("default".into(), default)]),
				&BTreeSet::from(["default".into()]),
				true,
			)
			.unwrap();
		assert!(optional.iter().all(|p| !path.join(p).exists()));
		assert!(build(&[]).1);
		let (extra, _) = build(&["--features", "extra"]);
		leased
			.commit(
				BTreeMap::from([("extra".into(), extra)]),
				&BTreeSet::from(["extra".into()]),
				true,
			)
			.unwrap();
		// A later occupant at the exact same path is still foreign.
		let replacement = path.join("other-file");
		fs::write(&replacement, b"keep replacement").unwrap();
		fs::rename(&replacement, &new_bin).unwrap();
		leased
			.commit(BTreeMap::new(), &BTreeSet::new(), true)
			.unwrap();
		assert_eq!(fs::read(new_bin).unwrap(), b"keep replacement");
		let unowned = root.join("developer-target");
		fs::create_dir(&unowned).unwrap();
		assert!(claim(&unowned).is_err());
	}
}
