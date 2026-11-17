//! Concurrent final stages: what each writes, what it weighs, and when it
//! may start.
//!
//! A stage writes Cargo target directories, one per profile it builds. It
//! starts once every earlier stage that shares one of them has finished, so
//! builds into one directory keep selection order and never wait on Cargo's
//! lock; and once its declared producers have passed.
use crate::catalogue::{self, Bulk, Command};
use crate::child::value_error;
use anyhow::{Context as _, Result};
use job_budget::fifo::Pool;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const GIB: u64 = 1024 * 1024 * 1024;
/// Before anything else: the commit checks and the dependency fetches.
pub const FIRST: &str = "first";
/// Nothing else runs while it does: its screen needs a quiet machine.
pub const ALONE: &str = "alone";
pub const ALONE_STAGES: [&str; 1] = ["fixed-route"];
/// Test threads where a command names none.
pub const TEST_THREADS: usize = 4;
/// A warm target directory grows by what a candidate rebuilds; a missing
/// one grows to its full size.
pub const WARM_GROWTH_BYTES: u64 = GIB;
pub const WARM_GROWTH_SHARE: f64 = 0.05;
pub const COLD_BYTES: u64 = 4 * GIB;

pub type Resources = BTreeSet<String>;

/// Bytes each owned bulk root may reach, rounded up from complete finals.
pub fn bulk_bytes(kind: Bulk) -> u64 {
	match kind {
		Bulk::Baseline => 2 * GIB,
		Bulk::Cross | Bulk::Fixed | Bulk::Project => GIB,
		Bulk::Contracts => 6 * GIB,
	}
}

pub fn declared_bulk(name: &str) -> u64 {
	catalogue::bulk_roots(name)
		.into_iter()
		.map(bulk_bytes)
		.sum()
}

/// Wrapper stages by script: what each builds beyond the manifests it names.
fn wrapper(script: &str) -> Option<&'static [(&'static str, &'static str)]> {
	Some(match script {
		"tools/column/clippy.py" | "tools/column/suite_stage.py" => &[],
		"tools/column/inventories.py" => &[
			("target/column-inventory-extract", "debug"),
			("tools/polars-gen/Cargo.toml", "debug"),
			("target/0072", ""),
		],
		"tools/column/jupyter_controls.py" => {
			&[("Cargo.toml", "debug"), ("jupyter/Cargo.toml", "debug")]
		}
		"tools/column/cli.py" => &[("adapters/polars/Cargo.toml", "release")],
		"tools/verify.py"
		| "tools/column/fixed_route_gate.py"
		| "ci/preflight.py"
		| "tools/test_verify_pins.py" => &[],
		_ => return None,
	})
}

/// Builds a stage's tests start themselves, beyond its own commands.
fn nested(name: &str) -> &'static [(&'static str, &'static str)] {
	match name {
		"polars-suite" => &[("target/0073", "")],
		_ => &[],
	}
}

pub fn profile(command: &[String]) -> String {
	if command.iter().any(|a| a == "--release") {
		return "release".into();
	}
	if let Some(i) = command.iter().position(|a| a == "--profile") {
		let name = command.get(i + 1).map(String::as_str).unwrap_or_default();
		return match name {
			"dev" | "test" => "debug",
			"bench" => "release",
			other => other,
		}
		.into();
	}
	"debug".into()
}

pub fn cargo_verb(command: &[String]) -> &str {
	command[1..]
		.iter()
		.find(|a| !a.starts_with('+'))
		.map(String::as_str)
		.unwrap_or_default()
}

pub fn manifest(command: &[String]) -> &str {
	command
		.iter()
		.position(|a| a == "--manifest-path")
		.and_then(|i| command.get(i + 1))
		.map_or("Cargo.toml", String::as_str)
}

pub fn script(command: &[String]) -> Option<&str> {
	command[1..]
		.iter()
		.find(|a| a.ends_with(".py"))
		.map(String::as_str)
}

fn is_cargo(command: &[String]) -> bool {
	Path::new(&command[0])
		.file_name()
		.is_some_and(|n| n == "cargo")
}

fn is_unittest(command: &[String]) -> bool {
	command.get(2).map(String::as_str) == Some("-m")
		&& command.get(3).map(String::as_str) == Some("unittest")
}

/// Refuse a stage whose writes the scheduler cannot name.
pub fn declared(name: &str, command: &[String]) -> Result<()> {
	if ALONE_STAGES.contains(&name)
		|| name == "commit-discipline"
		|| name.starts_with("fetch-")
		|| is_cargo(command)
	{
		return Ok(());
	}
	if command[0] == "python3"
		&& (script(command).is_some_and(|s| wrapper(s).is_some()) || is_unittest(command))
	{
		return Ok(());
	}
	Err(value_error(&format!(
		"stage writes are not declared: {name}"
	)))
}

/// Each manifest's Cargo target directory, as Cargo resolves it.
pub struct Targets {
	root: PathBuf,
	known: Mutex<BTreeMap<String, PathBuf>>,
}

impl Targets {
	pub fn new(root: &Path) -> Self {
		Self {
			root: root.to_owned(),
			known: Mutex::new(BTreeMap::new()),
		}
	}

	pub fn resolve(&self, path: &str) -> Result<PathBuf> {
		if !path.ends_with("Cargo.toml") {
			return Ok(self.root.join(path));
		}
		if let Some(known) = self.known.lock().unwrap().get(path) {
			return Ok(known.clone());
		}
		let output = std::process::Command::new("cargo")
			.args([
				"+1.98.1",
				"metadata",
				"--format-version",
				"1",
				"--no-deps",
				"--offline",
				"--manifest-path",
				path,
			])
			.current_dir(&self.root)
			.output()?;
		anyhow::ensure!(
			output.status.success(),
			"cargo metadata {path}: {}",
			String::from_utf8_lossy(&output.stderr).trim()
		);
		let meta: serde_json::Value = serde_json::from_slice(&output.stdout)?;
		let target = PathBuf::from(
			meta["target_directory"]
				.as_str()
				.context("cargo metadata has no target directory")?,
		);
		self.known
			.lock()
			.unwrap()
			.insert(path.into(), target.clone());
		Ok(target)
	}
}

fn lane(directory: &Path, profile: &str) -> String {
	// Python's `directory / ''` is the directory itself.
	if profile.is_empty() {
		directory.to_string_lossy().into_owned()
	} else {
		directory.join(profile).to_string_lossy().into_owned()
	}
}

/// The (target directory/profile) lanes a stage writes, or FIRST or ALONE.
pub fn resources(
	name: &str,
	command: &[String],
	root: &Path,
	targets: &Targets,
) -> Result<Resources> {
	declared(name, command)?;
	if ALONE_STAGES.contains(&name) {
		return Ok([ALONE.to_string()].into());
	}
	if name == "commit-discipline" || name.starts_with("fetch-") {
		return Ok([FIRST.to_string()].into());
	}
	let cargo = |argv: &[String]| -> Result<Resources> {
		if ["fmt", "fetch", "metadata"].contains(&cargo_verb(argv)) {
			return Ok(Resources::new());
		}
		Ok([lane(&targets.resolve(manifest(argv))?, &profile(argv))].into())
	};
	let mut lanes = Resources::new();
	for (place, kind) in nested(name) {
		lanes.insert(lane(&targets.resolve(place)?, kind));
	}
	if is_cargo(command) {
		lanes.extend(cargo(command)?);
		return Ok(lanes);
	}
	match script(command) {
		Some("tools/column/suite_stage.py") => {
			let stage = &command[command
				.iter()
				.position(|a| a == "--stage")
				.context("suite stage without --stage")?
				+ 1];
			let mut all = Resources::new();
			for recipe in catalogue::recipes(stage, root)? {
				all.extend(cargo(&recipe)?);
			}
			Ok(all)
		}
		Some("tools/column/clippy.py") => {
			let prefix = format!(
				"column-clippy-{}",
				if command.iter().any(|a| a == "--adapter") {
					"adapter-"
				} else {
					""
				}
			);
			["published", "candidate"]
				.iter()
				.map(|role| {
					Ok(lane(
						&targets.resolve(&format!("target/{prefix}{role}"))?,
						"debug",
					))
				})
				.collect()
		}
		Some(path) => wrapper(path)
			.unwrap_or(&[])
			.iter()
			.map(|(place, kind)| Ok(lane(&targets.resolve(place)?, kind)))
			.collect(),
		None => Ok(Resources::new()),
	}
}

/// Threads a test command runs: its --test-threads, else the default.
pub fn test_threads(command: &[String]) -> usize {
	command
		.iter()
		.find_map(|a| a.strip_prefix("--test-threads="))
		.and_then(|n| n.parse().ok())
		.unwrap_or(TEST_THREADS)
}

/// Tokens a stage holds while it runs: its widest test run, else one.
pub fn weight(_name: &str, command: &[String], root: &Path) -> Result<usize> {
	if is_cargo(command) {
		return Ok(if cargo_verb(command) == "test" {
			test_threads(command)
		} else {
			1
		});
	}
	match script(command) {
		Some("tools/column/suite_stage.py") => {
			let stage = &command[command
				.iter()
				.position(|a| a == "--stage")
				.context("suite stage without --stage")?
				+ 1];
			// suite_stage runs every test recipe with --test-threads=4.
			Ok(
				if catalogue::recipes(stage, root)?
					.iter()
					.any(|r| r.iter().any(|a| a == "test"))
				{
					4
				} else {
					1
				},
			)
		}
		Some("tools/verify.py") => Ok(TEST_THREADS),
		_ => Ok(1),
	}
}

pub fn conflict(a: &Resources, b: &Resources) -> bool {
	!a.is_disjoint(b)
		|| [a, b]
			.iter()
			.any(|r| r.contains(FIRST) || r.contains(ALONE))
}

/// Its producers have passed and every earlier stage sharing a resource
/// has finished; an ALONE stage also waits for quiet.
pub fn ready(
	index: usize,
	plan: &[Resources],
	finished: &BTreeSet<usize>,
	running: &BTreeSet<usize>,
	producers: &[usize],
	succeeded: &BTreeSet<usize>,
) -> bool {
	if plan[index].contains(ALONE) && !running.is_empty() {
		return false;
	}
	if !producers.iter().all(|j| succeeded.contains(j)) {
		return false;
	}
	(0..index).all(|j| finished.contains(&j) || !conflict(&plan[index], &plan[j]))
}

/// Selection indices of each stage's declared prerequisites.
pub fn producers(stages: &[(String, Command)]) -> Vec<Vec<usize>> {
	let order: BTreeMap<&str, usize> = stages
		.iter()
		.enumerate()
		.map(|(i, (n, _))| (n.as_str(), i))
		.collect();
	stages
		.iter()
		.map(|(name, _)| {
			catalogue::prerequisites(name)
				.iter()
				.filter_map(|p| order.get(p).copied())
				.collect()
		})
		.collect()
}

fn tree_bytes(path: &Path) -> std::io::Result<u64> {
	let mut total = 0;
	for entry in std::fs::read_dir(path)? {
		let entry = entry?;
		let kind = entry.file_type()?;
		if kind.is_dir() {
			total += tree_bytes(&entry.path()).unwrap_or(0);
		} else if kind.is_file() {
			total += entry.metadata().map(|m| m.len()).unwrap_or(0);
		}
	}
	Ok(total)
}

/// Bytes a stage's target directory may add: a share of a warm one, all of
/// a missing one.
pub fn growth(resource: &str, root: &Path) -> u64 {
	if resource == FIRST || resource == ALONE || !resource.starts_with(&*root.to_string_lossy()) {
		return 0;
	}
	match tree_bytes(Path::new(resource)) {
		Ok(size) if size > 0 => WARM_GROWTH_BYTES.max((size as f64 * WARM_GROWTH_SHARE) as u64),
		_ => COLD_BYTES,
	}
}

/// The jobserver environment of a stage that shares the pool: every stage
/// but the fixed-route screen and the Python test suites.
pub fn stage_environment(
	name: &str,
	command: &[String],
	pool: &Pool,
	held: usize,
) -> Vec<(String, String)> {
	if ALONE_STAGES.contains(&name)
		|| is_unittest(command)
		|| script(command) == Some("tools/test_verify_pins.py")
	{
		return vec![];
	}
	let flags = pool.flags();
	vec![
		("MAKEFLAGS".into(), flags.clone()),
		("CARGO_MAKEFLAGS".into(), flags),
		("RUST_TEST_THREADS".into(), TEST_THREADS.to_string()),
		("RNX_FINAL_STAGE_TOKENS".into(), held.to_string()),
	]
}

/// Each stage's resources, weight, bulk roots, environment and producers
/// against the Python controller's at ADR 0284's witness revision.
#[cfg(test)]
mod witness {
	use super::*;
	use serde_json::json;

	#[test]
	fn every_stage_is_scheduled_as_the_witness_schedules_it() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../..")
			.canonicalize()
			.unwrap();
		let out = std::env::temp_dir().join("rnx-final-witness/FINAL-x");
		let stages = catalogue::commands(&out, &root).unwrap();
		let expected = crate::witness::expected(
			"scheduling",
			&[
				("<OUT>", &out.to_string_lossy()),
				("<ROOT>", &root.to_string_lossy()),
			],
		);
		let targets = Targets::new(&root);
		let pool_dir = std::env::temp_dir().join(format!(
			"rnx-final-witness-pool-{}",
			crate::receipt::invocation_id()
		));
		std::fs::create_dir_all(&pool_dir).unwrap();
		let pool = Pool::create(&pool_dir.join("fifo"), 7).unwrap();
		let flags = pool.flags();
		for ((name, command), row) in stages.iter().zip(expected["rows"].as_array().unwrap()) {
			assert_eq!(json!(name), row["name"]);
			assert_eq!(
				json!(resources(name, command, &root, &targets).unwrap()),
				row["resources"],
				"{name} resources"
			);
			assert_eq!(
				json!(weight(name, command, &root).unwrap()),
				row["weight"],
				"{name} weight"
			);
			assert_eq!(json!(declared_bulk(name)), row["bulk"], "{name} bulk");
			let environment: BTreeMap<String, String> = stage_environment(name, command, &pool, 7)
				.into_iter()
				.map(|(k, v)| (k, v.replace(&flags, "FLAGS")))
				.collect();
			let python: BTreeMap<String, String> =
				serde_json::from_value(row["environment"].clone()).unwrap();
			assert_eq!(environment, python, "{name} environment");
		}
		assert_eq!(json!(producers(&stages)), expected["producers"]);
		pool.close().unwrap();
		let _ = std::fs::remove_dir_all(&pool_dir);
	}
}
