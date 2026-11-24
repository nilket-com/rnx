//! The final: live validation of every selected stage, durable acceptance,
//! then named-root release.
use crate::catalogue::{self, Bulk};
use crate::child::{self, Stopper, value_error};
use crate::evidence::{self, Owners, publication_eligible};
use crate::failure::{self, ERROR_CAP, Environment, FINAL_CAP, bounded, sanitize};
use crate::guardian;
use crate::outputs;
use crate::receipt::{atomic_json, digest};
use crate::scheduler::{self, ALONE, Resources, Targets};
use crate::selection;
use crate::types;
use anyhow::{Context as _, Result, bail};
use job_budget::fifo::{Pool, Tokens};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const DISK_FLOOR_BYTES: u64 = 20 * 1024 * 1024 * 1024;
/// People wait on the complete final. Past an hour it fails, so a slow
/// stage is fixed when it appears rather than after the loop has doubled.
pub const WALL_BUDGET_SECONDS: f64 = 3600.0;
const STAGE_TIMEOUT: Duration = Duration::from_secs(10800);

pub fn wall_budget_error(scope: &str, seconds: f64) -> Option<String> {
	(scope == "complete" && seconds > WALL_BUDGET_SECONDS).then(|| {
		format!(
			"complete final took {:.1} minutes; its budget is {}",
			seconds / 60.0,
			WALL_BUDGET_SECONDS as u64 / 60
		)
	})
}

/// What a test may replace, as the Python controls patch the same calls.
pub trait Hooks: Send + Sync {
	fn cargo_caches(&self) -> bool {
		false
	}
	fn head(&self, root: &Path) -> Option<String> {
		child::head(root)
	}
	fn disk_check(&self, root: &Path, out: &Path, environment: &Environment) -> Result<()> {
		disk_check(root, out, environment)
	}
	fn select(
		&self,
		root: &Path,
		out: &Path,
		names: Option<&[String]>,
		scope: &str,
		environment: &Environment,
	) -> Result<Value> {
		selection::select(root, out, names, "HEAD", Some(scope), environment)
	}
	fn check_selection(
		&self,
		selection_value: &Value,
		root: &Path,
		environment: &Environment,
	) -> Result<()> {
		selection::check_selection(selection_value, root, true, environment)
	}
	fn finalize(
		&self,
		selection_value: &Value,
		evidence_map: &Map<String, Value>,
		root: &Path,
		environment: &Environment,
	) -> Result<Value> {
		selection::finalize(selection_value, evidence_map, root, environment)
	}
	fn resources(
		&self,
		name: &str,
		command: &[String],
		root: &Path,
		targets: &Targets,
	) -> Result<Resources> {
		scheduler::resources(name, command, root, targets)
	}
	fn weight(&self, name: &str, command: &[String], root: &Path) -> Result<usize> {
		scheduler::weight(name, command, root)
	}
	fn tools_identity(&self, settings: &Settings) -> Result<Value> {
		tools_identity(settings)
	}
	fn disk_free(&self, path: &Path) -> Result<u64> {
		disk_free(path)
	}
	fn release(&self, owner: &mut Owners) -> Result<Value> {
		owner.release()
	}
	fn compact(
		&self,
		selection_value: &Value,
		evidence_map: &Map<String, Value>,
		owner: &Owners,
		tools: &Value,
	) -> Result<Value> {
		evidence::compact(selection_value, evidence_map, owner, tools)
	}
	fn diagnostics(
		&self,
		out: &Path,
		entries: &[evidence::Root],
		error: &str,
		environment: &Environment,
	) -> Result<()> {
		diagnostics(out, entries, error, environment)
	}
	/// Seconds since the final began.
	fn elapsed(&self, started: Instant) -> f64 {
		started.elapsed().as_secs_f64()
	}
	/// Tests run a fake child; None runs the stage's command.
	fn stage(&self, _call: &StageCall) -> Option<Result<child::Done>> {
		None
	}
	/// Called just before the controller blocks between passes; a control
	/// writes there to land between a refusal and the block.
	fn before_block(&self) {}
	/// The controller has taken a `Tokens` event and not yet cleared its
	/// flag: a control writes there to land while the event is outstanding.
	fn tokens_received(&self) {}
	/// A stage was just refused its tokens, inside its pass.
	fn after_token_refusal(&self) {}
	/// The watcher read a batch with a write in it and sent an event
	/// (`sent`) or found one outstanding.
	fn watcher_decided(&self, _sent: bool) {}
	/// The watch on the jobserver's writes; a control substitutes one.
	fn jobserver_watch(&self, pool: &Pool) -> Result<job_budget::fifo::Watch> {
		pool.watch()
	}
}

/// The real calls.
pub struct Live;
impl Hooks for Live {
	fn cargo_caches(&self) -> bool {
		true
	}
}

/// Everything about one final that is fixed before its first stage.
#[derive(Clone)]
pub struct Settings {
	pub root: PathBuf,
	pub out: PathBuf,
	pub names: Option<Vec<String>>,
	pub scope: String,
	pub fresh: bool,
	pub environment: Environment,
	/// The running controller, which also serves as every stage's guardian.
	pub guardian: PathBuf,
	/// When the final began: before the controller was compiled, for a
	/// complete final the entry started.
	pub started: Instant,
	pub bootstrap: Option<Value>,
	/// The final's own unit, required for a complete final.
	pub unit: Option<String>,
	pub hooks: Arc<dyn Hooks>,
	/// Tests replace the job count; None derives it from the host.
	pub tokens: Option<usize>,
}

impl Settings {
	fn checkpoint(&self) -> Option<&Value> {
		self.bootstrap
			.as_ref()
			.and_then(|record| record.get("checkpoint"))
	}
}

/// One stage's call to its child, as a test runner sees it.
pub struct StageCall {
	pub name: String,
	pub command: Vec<String>,
	pub environment: Environment,
	pub log: PathBuf,
	pub directory: PathBuf,
}

pub fn disk_free(path: &Path) -> Result<u64> {
	let mut path = path.to_owned();
	while !path.exists() {
		path = path.parent().context("no existing ancestor")?.to_owned();
	}
	let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
	let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
	// SAFETY: a valid path and an owned statvfs.
	if unsafe { libc::statvfs(c.as_ptr(), &mut stat) } != 0 {
		return Err(std::io::Error::last_os_error().into());
	}
	Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
}

/// The shared 20 GiB admission of every build launcher: the final's root,
/// its directory and any `CARGO_TARGET_DIR`. The Python launchers ask
/// `ci/disk_space.py`, which a control holds to this rule.
pub fn disk_check(root: &Path, out: &Path, environment: &Environment) -> Result<()> {
	disk_check_with(root, out, environment, &disk_free)
}

/// `disk_check` with the free-space reading given, as a control replaces it.
pub fn disk_check_with(
	root: &Path,
	out: &Path,
	environment: &Environment,
	free: &dyn Fn(&Path) -> Result<u64>,
) -> Result<()> {
	let (pass, text) =
		disk_observation(&disk_paths(root, out, environment), DISK_FLOOR_BYTES, free)?;
	std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(out.join("disk-admission.json"))
		.and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()))?;
	if !pass {
		return Err(raise_called_process("ci/disk_space.py"));
	}
	Ok(())
}

/// The paths a launch is admitted for: the given ones, then a nonempty
/// `CARGO_TARGET_DIR`.
pub fn disk_paths(root: &Path, out: &Path, environment: &Environment) -> Vec<PathBuf> {
	let mut paths = vec![root.to_owned(), out.to_owned()];
	if let Some(target) = environment
		.get("CARGO_TARGET_DIR")
		.filter(|t| !t.is_empty())
	{
		paths.push(PathBuf::from(target));
	}
	paths
}

/// Each path's filesystem, judged at its nearest existing ancestor, against
/// the floor: whether every one has room, and the record as
/// `ci/disk_space.py` prints it. `free` reads a filesystem's free bytes.
pub fn disk_observation(
	paths: &[PathBuf],
	floor: u64,
	free: &dyn Fn(&Path) -> Result<u64>,
) -> Result<(bool, String)> {
	anyhow::ensure!(floor > 0, "positive disk reserve required");
	let mut rows = Vec::new();
	let mut pass = !paths.is_empty();
	for path in paths {
		let mut existing = std::fs::canonicalize(path).unwrap_or(path.clone());
		while !existing.exists() {
			existing = existing
				.parent()
				.context("no existing ancestor")?
				.to_owned();
		}
		let bytes = free(&existing)?;
		pass &= bytes >= floor;
		rows.push(format!("    {{\n      \"path\": {},\n      \"free_bytes\": {bytes},\n      \"minimum_free_bytes\": {floor}\n    }}", crate::pyjson::compact(&json!(existing.to_string_lossy()))?));
	}
	let text = format!(
		"{{\n  \"status\": \"{}\",\n  \"filesystems\": [\n{}\n  ]\n}}\n",
		if pass { "PASS" } else { "FAIL" },
		rows.join(",\n")
	);
	Ok((pass, text))
}

fn raise_called_process(what: &str) -> anyhow::Error {
	child::raise(
		"CalledProcessError",
		&format!("disk admission failed: {what}"),
	)
}

pub fn fresh_violation(root: &Path, fresh: bool, environment: &Environment) -> Option<String> {
	if environment
		.get("CARGO_TARGET_DIR")
		.is_some_and(|t| !t.is_empty())
	{
		return Some("final refuses CARGO_TARGET_DIR override".into());
	}
	if fresh {
		for manifest in catalogue::MANIFESTS {
			if root
				.join(manifest)
				.parent()
				.is_some_and(|p| p.join("target").exists())
			{
				return Some(format!(
					"fresh acceptance refuses preexisting native target: {manifest}"
				));
			}
		}
	}
	None
}

fn executable_digest(name: &str, environment: &Environment) -> Option<String> {
	let path = environment
		.get("PATH")?
		.split(':')
		.map(|d| Path::new(d).join(name))
		.find(|p| p.is_file())?;
	digest(&std::fs::canonicalize(path).ok()?).ok()
}

/// The toolchain and the controller that ran this final.
fn tools_identity(settings: &Settings) -> Result<Value> {
	let rustc = std::process::Command::new("rustc")
		.args(["+1.98.1", "-vV"])
		.output()?;
	let executables: Map<String, Value> = ["cargo", "rustc", "git", "python3", "bwrap"]
		.iter()
		.filter_map(|n| {
			executable_digest(n, &settings.environment).map(|d| (n.to_string(), json!(d)))
		})
		.collect();
	Ok(json!({
		"rustc": String::from_utf8_lossy(&rustc.stdout),
		"controller": {"binary_sha256": digest(Path::new("/proc/self/exe"))?, "bootstrap": settings.bootstrap, "unit": settings.unit},
		"executables": executables,
	}))
}

/// General diagnostics of a failed final, bounded per stage and in total.
pub fn diagnostics(
	out: &Path,
	entries: &[evidence::Root],
	error: &str,
	environment: &Environment,
) -> Result<()> {
	let mut budget: usize = 4 * 1024 * 1024;
	let mut files = Vec::new();
	let mut stage_budgets: BTreeMap<String, usize> = BTreeMap::new();
	'entries: for entry in entries {
		let mut stage_budget = *stage_budgets
			.entry(entry.stage.clone())
			.or_insert(256 * 1024);
		let mut paths = Vec::new();
		walk(Path::new(&entry.path), &mut paths);
		paths.sort();
		for p in paths {
			let meta = std::fs::symlink_metadata(&p)?;
			let suffix = p
				.extension()
				.map(|e| format!(".{}", e.to_string_lossy()))
				.unwrap_or_default();
			if !meta.is_file() || ![".log", ".stderr", ".stdout"].contains(&suffix.as_str()) {
				continue;
			}
			if stage_budget.min(budget) < 1024 {
				break;
			}
			let take = stage_budget.min(budget).min(32768);
			let (data, truncated) = failure::log_excerpt(&p, take)?;
			let row = json!({
				"stage": entry.stage, "name": sanitize(&p.to_string_lossy(), environment), "bytes": meta.len(),
				"sha256": digest(&p)?, "truncated": truncated, "text": sanitize(&String::from_utf8_lossy(&data), environment),
			});
			let cost = python_dumps_len(&row)?;
			if cost > stage_budget.min(budget) {
				continue;
			}
			files.push(row);
			stage_budget -= cost;
			budget -= cost;
		}
		stage_budgets.insert(entry.stage.clone(), stage_budget);
		if budget < 1024 {
			break 'entries;
		}
	}
	let error = bounded(&sanitize(error, environment), 16384).0;
	let mut r = json!({"error": error, "files": files, "release_point": "explicit abandon; no automatic retention sweep"});
	while python_dumps_len(&r)? > 4 * 1024 * 1024 {
		r["files"].as_array_mut().unwrap().pop();
	}
	atomic_json(&out.join("DIAGNOSTICS.json"), &r)
}

/// `len(json.dumps(value).encode())`: default separators, ASCII escapes.
fn python_dumps_len(value: &Value) -> Result<usize> {
	let compact = crate::pyjson::compact(value)?;
	// Each separator gains a space: ", " and ": " (none inside strings, which
	// the compact form escapes).
	let mut extra = 0;
	let mut in_string = false;
	let mut escaped = false;
	for c in compact.chars() {
		if in_string {
			match (escaped, c) {
				(true, _) => escaped = false,
				(false, '\\') => escaped = true,
				(false, '"') => in_string = false,
				_ => {}
			}
		} else if c == '"' {
			in_string = true;
		} else if c == ',' || c == ':' {
			extra += 1;
		}
	}
	Ok(compact.len() + extra)
}

fn walk(path: &Path, out: &mut Vec<PathBuf>) {
	for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
		let p = entry.path();
		if entry.file_type().is_ok_and(|t| t.is_dir()) {
			walk(&p, out);
		}
		out.push(p);
	}
}

/// Each declared variable's digest, its value with this final's own paths
/// and job count substituted, so two finals' stage environments compare
/// without any value being kept, as the retired Python controller computed
/// them (ADR 0284's witness fixture).
pub fn environment_digests(
	env: &Environment,
	out: &Path,
	owner_id: &str,
	cross: Option<&Path>,
) -> Result<BTreeMap<String, String>> {
	static JOBS: std::sync::LazyLock<regex::Regex> =
		std::sync::LazyLock::new(|| regex::Regex::new(r"(^|\s)-j\d+").unwrap());
	let keys = child::environment(env)?;
	let mut rows = BTreeMap::new();
	for key in keys["keys"]
		.as_array()
		.into_iter()
		.flatten()
		.filter_map(Value::as_str)
	{
		let mut value = env[key].clone();
		if let Some(cross) = cross {
			value = value.replace(&*cross.to_string_lossy(), "<CROSS>");
		}
		value = value.replace(&*out.to_string_lossy(), "<OUT>");
		if !owner_id.is_empty() {
			value = value.replace(owner_id, "<OWNER>");
		}
		if key == "CARGO_BUILD_JOBS" {
			value = "<TOKENS>".into();
		} else if key.ends_with("MAKEFLAGS") {
			value = JOBS.replace_all(&value, "${1}-j<TOKENS>").into_owned();
		}
		rows.insert(
			key.to_owned(),
			crate::pyjson::hex(&<sha2::Sha256 as sha2::Digest>::digest(value.as_bytes())),
		);
	}
	Ok(rows)
}

/// Producer receipts inside owned roots outlive release, for parity.
fn retain_semantic(out: &Path, evidence_map: &Map<String, Value>, owner: &Owners) -> Result<()> {
	let roots: Vec<PathBuf> = owner
		.row
		.roots
		.iter()
		.map(|e| PathBuf::from(&e.path))
		.collect();
	for (name, reference) in evidence_map {
		let row: types::ChildResult =
			types::read(Path::new(reference["path"].as_str().unwrap_or_default()))?;
		let Some(binding) = row.producer.value() else {
			continue;
		};
		let receipt = PathBuf::from(&binding.receipt);
		if !roots.iter().any(|root| receipt.starts_with(root)) {
			continue;
		}
		anyhow::ensure!(
			digest(&receipt)? == binding.sha256,
			"producer receipt changed: {name}"
		);
		std::fs::create_dir_all(out.join("retained"))?;
		std::fs::copy(
			&receipt,
			out.join("retained").join(format!("{name}-RESULT.json")),
		)?;
	}
	Ok(())
}

/// The first failing stage: the final's cause, whatever ran beside it.
struct StageFailed {
	message: String,
	row: Value,
}

/// Git, Cargo and rustup search ancestors, including through a linked worktree.
fn neutral_parent(path: &Path) -> Result<bool> {
	for ancestor in path.ancestors() {
		for name in [
			".git",
			"Cargo.toml",
			".cargo",
			"rust-toolchain",
			"rust-toolchain.toml",
		] {
			match ancestor.join(name).symlink_metadata() {
				Ok(_) => return Ok(false),
				Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
				Err(e) => return Err(e.into()),
			}
		}
	}
	Ok(true)
}

fn scratch_parent(out: &Path) -> Result<&Path> {
	if neutral_parent(out)? {
		return Ok(out);
	}
	let parent = Path::new("/var/tmp");
	evidence::canonical(parent)?;
	anyhow::ensure!(
		neutral_parent(parent)?,
		"stage scratch under {} would inherit project configuration",
		parent.display()
	);
	Ok(parent)
}

/// Owned roots and the environment of one stage, fixed before it starts.
fn stage_environment(
	settings: &Settings,
	stage: (usize, &str),
	temporaries: &Path,
	command: &[String],
	owner: &mut Owners,
	base: &Environment,
	pool: Option<(&Pool, usize)>,
) -> Result<Environment> {
	let (root, out) = (&settings.root, &settings.out);
	// Keep Cargo payloads outside caller configuration, even when the
	// durable receipt lives in the caller's checkout.
	let payload_parent = if settings.checkpoint().is_some() {
		temporaries.parent().context("scratch parent")?
	} else {
		out.as_path()
	};
	let lint_parent = if settings.checkpoint().is_some() {
		// The archived xtask queries its enclosing Git revision. Keep its
		// original context below the verified checkout's ignored target.
		root.join("target/routing-cli")
	} else {
		out.clone()
	};
	let mut env = base.clone();
	env.remove("RNX_FINAL_CHECKPOINT_ROOT");
	env.remove("RNX_FINAL_CHECKPOINT_STORE");
	env.remove("RNX_FINAL_CHECKPOINT_INVOCATION");
	let (index, name) = stage;
	let scratch = temporaries.join(index.to_string());
	use std::os::unix::fs::DirBuilderExt as _;
	std::fs::DirBuilder::new().mode(0o700).create(&scratch)?;
	env.insert("TMPDIR".into(), scratch.display().to_string());
	// Product tests exercise the real cache-selection paths. Start them
	// cold, including children of Cargo that inherit the stage environment.
	let product_cache = scratch.join("product-cache");
	std::fs::DirBuilder::new()
		.mode(0o700)
		.create(&product_cache)?;
	env.insert("XDG_CACHE_HOME".into(), product_cache.display().to_string());
	env.insert(
		"RNX_PROJECT_CACHE".into(),
		product_cache.join("rnx/assemblies").display().to_string(),
	);
	env.insert(
		"RNX_FINAL_ROUTING_ROOT".into(),
		owner
			.assign(&root.join("target/routing-cli"), name, None)?
			.to_string_lossy()
			.into_owned(),
	);
	for kind in catalogue::bulk_roots(name) {
		let (key, path) = match kind {
			Bulk::Baseline => (
				"RNX_FINAL_BASELINE_ROOT",
				owner.assign_private(&lint_parent, &format!("{name}-baseline"))?,
			),
			Bulk::Fixed => (
				"RNX_FINAL_FIXED_ROOT",
				owner.assign_private(payload_parent, &format!("{name}-payload"))?,
			),
			Bulk::Project => (
				"RNX_FINAL_PROJECT_ROOT",
				owner.assign_private(
					temporaries.parent().context("scratch parent")?,
					&format!("{name}-project"),
				)?,
			),
			Bulk::Cross => {
				let parent = PathBuf::from(
					&command[command
						.iter()
						.position(|a| a == "--out")
						.context("cross without --out")?
						+ 1],
				);
				let parent = parent
					.parent()
					.context("cross output has no parent")?
					.to_owned();
				let label = parent.file_name().map(|n| n.to_string_lossy().into_owned());
				let cross =
					owner.assign(&root.join("target/rnx-verify"), name, label.as_deref())?;
				if cross != parent {
					return Err(value_error("cross assignment differs from selection"));
				}
				("RNX_FINAL_CROSS_ROOT", cross)
			}
			Bulk::Contracts => (
				"RNX_FINAL_CONTRACT_ROOT",
				owner.assign_private(payload_parent, &format!("{name}-contracts"))?,
			),
		};
		env.insert(key.into(), path.to_string_lossy().into_owned());
	}
	if matches!(name, "polars-inventories" | "polars-suite") {
		let scratch = Path::new(&env["RNX_FINAL_ROUTING_ROOT"]).join("generator");
		env.insert(
			"RNX_FINAL_POLARS_SCRATCH".into(),
			scratch.display().to_string(),
		);
		let assets = if name == "polars-inventories" {
			owner.assign(out, "polars-assets", None)?
		} else {
			let entry = owner
				.row
				.roots
				.iter()
				.find(|entry| entry.stage == "polars-assets")
				.context("Polars suite started without the inventory producer")?;
			PathBuf::from(&entry.path)
		};
		env.insert(
			"RNX_FINAL_POLARS_ASSETS".into(),
			assets.join("inventories").display().to_string(),
		);
	}
	if name == "polars-suite" {
		env.insert(
			"RNX_ORACLE_RESULTS".into(),
			outputs::owned_report(out).to_string_lossy().into_owned(),
		);
	}
	// The fixed-route screen keeps its pinned recipe outside the shared pool.
	if let Some((pool, held)) = pool {
		env.extend(scheduler::stage_environment(name, command, pool, held));
	}
	Ok(env)
}

/// Copy a failed stage's cause while invocation-owned bulk still exists.
fn stage_failure(
	settings: &Settings,
	name: &str,
	done: &Value,
	owner: &Owners,
	row: &mut Value,
) -> String {
	let out = &settings.out;
	let environment = &settings.environment;
	let mut stage_error = done
		.get("error")
		.and_then(Value::as_str)
		.unwrap_or("child failed")
		.to_owned();
	let mut roots = vec![out.clone()];
	let attempt = (|| -> Result<Value> {
		roots = std::iter::once(out.clone())
			.chain(owner.row.roots.iter().map(|e| PathBuf::from(&e.path)))
			.collect();
		let validation_path = PathBuf::from(
			done["validation"]["path"]
				.as_str()
				.context("no validation")?,
		);
		let validation = failure::value(&validation_path, &roots)?;
		let producer = validation_path
			.parent()
			.unwrap_or(Path::new(""))
			.join("CHILD.json");
		let semantic = if producer.is_file() {
			PathBuf::from(
				failure::value(&producer, &roots)?["receipt"]
					.as_str()
					.unwrap_or_default(),
			)
		} else {
			validation_path.clone()
		};
		let cause = failure::value(&semantic, &roots)?;
		stage_error = failure::reason(&cause);
		let retained = if let Some(reference) = validation
			.get("failure")
			.or_else(|| cause.get("failure"))
			.filter(|f| failure::truthy(f))
		{
			failure::copy_bundle(out, reference, &roots, environment)?
		} else {
			let error = cause
				.get("error")
				.or_else(|| validation.get("error"))
				.map(failure::python_str)
				.unwrap_or_else(|| "child failed".into());
			failure::retain(
				out,
				name,
				Some(&semantic),
				&[out.join(format!("{name}.log"))],
				&roots,
				environment,
				&error,
			)?
		};
		let size: u64 = std::fs::read_dir(out.join("failure"))?
			.flatten()
			.filter_map(|e| e.metadata().ok())
			.map(|m| m.len())
			.sum::<u64>()
			+ std::fs::metadata(out.join("FAILURE.json"))?.len();
		if size as usize > FINAL_CAP {
			bail!(value_error("final failure diagnostics exceed bound"));
		}
		Ok(retained)
	})();
	let mut retained = None;
	match attempt {
		Ok(value) => retained = Some(value),
		Err(problem) => {
			// Diagnostics cannot replace the original failure or prevent release.
			row["failure_retention_error"] = json!(
				problem
					.downcast_ref::<child::Raised>()
					.map_or("Exception", |r| r.class)
			);
			if !out.join("failure").exists()
				&& let Some(path) = done["validation"]["path"].as_str()
			{
				retained = failure::retain(
					out,
					name,
					Some(Path::new(path)),
					&[out.join(format!("{name}.log"))],
					&roots,
					environment,
					&stage_error,
				)
				.ok();
			}
		}
	}
	if let Some(retained) = &retained {
		row["failure"] = retained.clone();
	}
	let detail = match &retained {
		Some(r) => r["error"].as_str().unwrap_or_default().to_owned(),
		None => bounded(&sanitize(&stage_error, environment), ERROR_CAP).0,
	};
	format!("stage failed: {name}; {detail}")
}

enum Event {
	Done(usize, Result<child::Done>),
	Signal(i32),
	/// The jobserver fifo was written, or its watch overflowed: capacity
	/// may have changed (ADR 0290). At most one is outstanding.
	Tokens,
	/// The jobserver watch ended with an error; the controller must not
	/// sleep on it.
	WatchFailed(String),
}

/// The thread that turns jobserver writes into [`Event::Tokens`]. Dropping
/// it, on every return and unwinding, stops and joins the thread, so it
/// never outlives the pool.
struct Watcher {
	stop: job_budget::fifo::StopWatch,
	handle: Option<std::thread::JoinHandle<()>>,
}

impl Watcher {
	fn start(
		watch: job_budget::fifo::Watch,
		sender: Notifier,
		pending: Arc<AtomicBool>,
		hooks: Arc<dyn Hooks>,
	) -> Result<Self> {
		use job_budget::fifo::Notice;
		let stop = watch.stopper()?;
		let handle = std::thread::spawn(move || {
			loop {
				match watch.wait() {
					// The batch is read; one event covers it and any later
					// write until the controller clears the flag.
					Ok(Notice::Modified) => {
						let sent = !pending.swap(true, Ordering::SeqCst);
						hooks.watcher_decided(sent);
						if sent && !sender.send(Event::Tokens) {
							return;
						}
					}
					Ok(Notice::Stopped) => return,
					Err(error) => {
						let _ = sender.send(Event::WatchFailed(format!("{error:#}")));
						return;
					}
				}
			}
		});
		Ok(Self {
			stop,
			handle: Some(handle),
		})
	}
}

impl Drop for Watcher {
	fn drop(&mut self) {
		let _ = self.stop.stop();
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
	}
}

/// Sends events to a running final: stage completions and signals.
#[derive(Clone)]
struct Notifier {
	sender: mpsc::Sender<Event>,
}

impl Notifier {
	fn send(&self, event: Event) -> bool {
		self.sender.send(event).is_ok()
	}
}

fn notifier() -> (Notifier, mpsc::Receiver<Event>) {
	let (sender, receiver) = mpsc::channel();
	(Notifier { sender }, receiver)
}

/// Run the selection concurrently; rows come back in selection order.
/// What the stages leave for acceptance: each stage's evidence, its row
/// in selection order, and the disk admission record.
#[derive(Default)]
struct Collected {
	cache: Option<crate::cache::CachePlan>,
	evidence: Map<String, Value>,
	rows: Vec<Value>,
	disk: Map<String, Value>,
}

pub(crate) fn bulk_demand(name: &str, checkpoint: bool, cached: bool) -> [u64; 3] {
	let mut checkout = 0;
	let mut output = 0;
	let mut scratch = 0;
	for kind in catalogue::bulk_roots(name) {
		let bytes = scheduler::bulk_bytes(kind);
		match kind {
			Bulk::Cross => checkout += bytes,
			Bulk::Baseline if checkpoint => checkout += bytes,
			Bulk::Project => scratch += bytes,
			_ if checkpoint => scratch += bytes,
			_ => output += bytes,
		}
	}
	if cached {
		match name {
			"polars-inventories" => {
				checkout += 2 * scheduler::GIB; // generator
				output += 2 * scheduler::GIB; // consumer's inventories
			}
			"polars-suite" => checkout += 2 * scheduler::GIB,
			_ => {}
		}
	}
	[checkout, output, scratch]
}

fn run_stages(
	settings: &Settings,
	selection_value: &Value,
	owner: &mut Owners,
	base: &Environment,
	collected: &mut Collected,
	interrupted: &Arc<AtomicI32>,
	events: (&Notifier, &mpsc::Receiver<Event>),
) -> Result<Option<StageFailed>> {
	let Collected {
		cache,
		evidence: evidence_map,
		rows,
		disk,
	} = collected;
	let root = &settings.root;
	let stages: Vec<(String, Vec<String>)> = selection_value["stages"]
		.as_array()
		.into_iter()
		.flatten()
		.map(|s| {
			(
				s[0].as_str().unwrap_or_default().to_owned(),
				serde_json::from_value(s[1].clone()).unwrap_or_default(),
			)
		})
		.collect();
	let targets = Targets::new(root);
	let plan: Vec<Resources> = stages
		.iter()
		.map(|(n, c)| {
			let mut resources = settings.hooks.resources(n, c, root, &targets)?;
			if let Some(cache) = cache.as_ref() {
				let lanes = cache.targets(n);
				if !lanes.is_empty()
					|| matches!(
						n.as_str(),
						"polars-inventory-preflight"
							| "polars-inventories"
							| "production-cli-controls"
					) {
					resources.retain(|r| r == scheduler::FIRST || r == scheduler::ALONE);
					resources.extend(lanes);
				}
			}
			Ok(resources)
		})
		.collect::<Result<_>>()?;
	let needs = scheduler::producers(&stages);
	let bulk: Vec<[u64; 3]> = stages
		.iter()
		.map(|(name, _)| bulk_demand(name, settings.checkpoint().is_some(), cache.is_some()))
		.collect();
	let tokens: usize = base
		.get("CARGO_BUILD_JOBS")
		.and_then(|j| j.parse().ok())
		.unwrap_or(1);
	let weights: Vec<usize> = stages
		.iter()
		.map(|(n, c)| settings.hooks.weight(n, c, root))
		.collect::<Result<_>>()?;
	// A stage runs its full test width; it is refused, not run on fewer tokens.
	let wide: Vec<String> = stages
		.iter()
		.zip(&weights)
		.filter(|(_, w)| **w > tokens)
		.map(|((n, _), w)| format!("{n} ({w})"))
		.collect();
	if !wide.is_empty() {
		return Err(value_error(&format!(
			"budget of {tokens} tokens is below the test width of {}",
			wide.join(", ")
		)));
	}
	let growth: BTreeMap<_, _> = plan
		.iter()
		.flatten()
		.collect::<BTreeSet<_>>()
		.into_iter()
		.map(|r| {
			let bytes = match &*cache {
				Some(plan) => plan.growth(r, &stages)?,
				None => scheduler::growth(r, root),
			};
			Ok((r.clone(), bytes))
		})
		.collect::<Result<_>>()?;
	let parent = scratch_parent(&settings.out)?;
	let temporaries = owner.assign_private(parent, "stage-temporaries")?;
	// Charge each writer to its device; sites sharing a device also share one floor.
	use std::os::unix::fs::MetadataExt as _;
	let sites = [root.as_path(), settings.out.as_path(), parent];
	let devices = [
		sites[0].metadata()?.dev(),
		sites[1].metadata()?.dev(),
		sites[2].metadata()?.dev(),
	];
	let bindings = std::array::from_fn::<_, 3, _>(|i| {
		devices[..i]
			.iter()
			.position(|&dev| dev == devices[i])
			.unwrap_or(i)
	});
	let declared = |indices: &mut dyn Iterator<Item = usize>| {
		let mut lanes = BTreeSet::new();
		let mut bytes = [0; 3];
		let mut count = 0;
		for i in indices {
			count += 1;
			for (site, demand) in bulk[i].iter().enumerate() {
				bytes[bindings[site]] += demand;
			}
			lanes.extend(plan[i].iter().cloned());
		}
		bytes[bindings[0]] += lanes.iter().map(|r| growth[r]).sum::<u64>();
		bytes[bindings[2]] += count.min(tokens) as u64 * scheduler::STAGE_SCRATCH_BYTES;
		bytes
	};
	let pool = Arc::new(Pool::create(&settings.out.join("jobserver.fifo"), tokens)?);
	// Armed before any stage, so no write after a refusal is missed.
	let tokens_pending = Arc::new(AtomicBool::new(false));
	let watch = settings
		.hooks
		.jobserver_watch(&pool)
		.context("start the jobserver watch")?;
	let watcher = Watcher::start(
		watch,
		events.0.clone(),
		tokens_pending.clone(),
		settings.hooks.clone(),
	)?;
	let estimates = declared(&mut (0..stages.len()));
	let keys = [
		("binding", "estimate_bytes", "lowest_free_bytes"),
		(
			"output_path",
			"output_estimate_bytes",
			"output_lowest_free_bytes",
		),
		(
			"scratch_path",
			"scratch_estimate_bytes",
			"scratch_lowest_free_bytes",
		),
	];
	disk.insert("floor_bytes".into(), json!(DISK_FLOOR_BYTES));
	disk.insert("lowest_free_bytes".into(), Value::Null);
	for (site, &(path, estimate, _)) in keys.iter().enumerate() {
		if bindings[site] == site {
			disk.insert(path.into(), json!(sites[site]));
			disk.insert(estimate.into(), json!(DISK_FLOOR_BYTES + estimates[site]));
		}
	}
	let mut pending: Vec<usize> = (0..stages.len()).collect();
	let mut running: BTreeMap<usize, (Tokens, Arc<Stopper>, std::thread::JoinHandle<()>)> =
		BTreeMap::new();
	let (mut finished, mut succeeded) = (BTreeSet::new(), BTreeSet::new());
	let mut done_rows: BTreeMap<usize, Value> = BTreeMap::new();
	let mut failed: Option<StageFailed> = None;
	let (sender, receiver) = events;
	let mut environments: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
	let names: Vec<String> = stages.iter().map(|(n, _)| n.clone()).collect();
	let recorder = std::cell::RefCell::new(crate::schedule::Recorder::new(&names));
	let now = || settings.hooks.elapsed(settings.started);
	let mut complete = |owner: &Owners,
	                    index: usize,
	                    result: Result<child::Done>,
	                    running: &mut BTreeMap<
		usize,
		(Tokens, Arc<Stopper>, std::thread::JoinHandle<()>),
	>,
	                    failed: &mut Option<StageFailed>,
	                    finished: &mut BTreeSet<usize>,
	                    succeeded: &mut BTreeSet<usize>| {
		let (tokens, _, handle) = running.remove(&index).expect("a running stage");
		let _ = handle.join();
		let _ = pool.give(tokens);
		finished.insert(index);
		let name = &stages[index].0;
		let result = result.and_then(|done| {
			if matches!(name.as_str(), "polars-inventories" | "polars-suite") {
				owner.release_child(name, "generator")?;
				if name == "polars-suite" {
					owner.release_child("polars-assets", "inventories")?;
				}
			}
			Ok(done)
		});
		let done = match result {
			Ok(d) => {
				json!({"returncode": d.returncode, "validation": d.validation, "declared_output": d.declared_output})
			}
			Err(error) => {
				json!({"returncode": 1, "validation": null, "error": format!("{error:#}")})
			}
		};
		let mut row =
			json!({"name": name, "exit": done["returncode"], "validation": done["validation"]});
		if !done["declared_output"].is_null() {
			row["declared_output"] = done["declared_output"].clone();
		}
		if !done["validation"].is_null() {
			evidence_map.insert(name.clone(), done["validation"].clone());
		}
		if done["returncode"] != 0 && failed.is_none() {
			let message = stage_failure(settings, name, &done, owner, &mut row);
			*failed = Some(StageFailed {
				message,
				row: row.clone(),
			});
		}
		if let Err(error) = owner.release_child("stage-temporaries", &index.to_string()) {
			row["exit"] = json!(1);
			row["cleanup_error"] = json!(error.to_string());
			failed.get_or_insert_with(|| StageFailed {
				message: error.to_string(),
				row: row.clone(),
			});
		}
		let passed = row["exit"] == 0;
		if passed {
			succeeded.insert(index);
		}
		recorder.borrow_mut().end(index, now(), passed);
		done_rows.insert(index, row);
	};
	let (mut token_waiting, mut tokens_noted) = (false, false);
	let outcome = (|| -> Result<()> {
		loop {
			if pending.is_empty() && running.is_empty() {
				return Ok(());
			}
			let signal = interrupted.load(Ordering::SeqCst);
			if signal != 0 {
				bail!(child::raise(
					"RuntimeError",
					&format!("final interrupted by signal {signal}")
				));
			}
			// A refused stage waits for tokens until a later pass admits it.
			token_waiting = false;
			if failed.is_none() {
				let pass = pending.clone();
				// The stages a refusal leaves unevaluated in this pass.
				let queue_rest = |position: usize, by: usize| {
					for &later in &pass[position + 1..] {
						recorder.borrow_mut().wait(
							later,
							now(),
							"queued",
							vec![stages[by].0.clone()],
							Value::Null,
						);
					}
				};
				for (position, &index) in pass.iter().enumerate() {
					// A stage that already failed stops admission before the next start.
					while let Ok(event) = receiver.try_recv() {
						match event {
							Event::Done(done, result) => {
								complete(
									owner,
									done,
									result,
									&mut running,
									&mut failed,
									&mut finished,
									&mut succeeded,
								);
								settings.hooks.check_selection(
									selection_value,
									root,
									&settings.environment,
								)?;
							}
							Event::Signal(signal) => interrupted.store(signal, Ordering::SeqCst),
							// Noted, its flag still set: earlier stages of this
							// pass are rechecked only by the next pass.
							Event::Tokens => tokens_noted = true,
							Event::WatchFailed(error) => {
								bail!(value_error(&format!("jobserver watch failed: {error}")))
							}
						}
					}
					if failed.is_some() || interrupted.load(Ordering::SeqCst) != 0 {
						break;
					}
					let running_set: BTreeSet<usize> = running.keys().copied().collect();
					if let Some(gate) = scheduler::gate(
						index,
						&plan,
						&finished,
						&running_set,
						&needs[index],
						&succeeded,
					) {
						let name = |j: &usize| stages[*j].0.clone();
						let (gate, behind) = match gate {
							scheduler::Gate::Alone => {
								("alone", running_set.iter().map(name).collect())
							}
							scheduler::Gate::Producers(p) => {
								("producers", p.iter().map(name).collect())
							}
							scheduler::Gate::Lanes(j) => ("lanes", vec![name(&j)]),
						};
						recorder
							.borrow_mut()
							.wait(index, now(), gate, behind, Value::Null);
						continue;
					}
					recorder.borrow_mut().ready(index, now());
					let estimates =
						declared(&mut pending.iter().copied().chain(running.keys().copied()));
					let mut refusal = None;
					for (site, &path) in sites.iter().enumerate() {
						if bindings[site] != site {
							continue;
						}
						let key = keys[site].2;
						let bytes = estimates[site];
						let free = settings.hooks.disk_free(path)?;
						if disk
							.get(key)
							.and_then(Value::as_u64)
							.is_none_or(|l| free < l)
						{
							disk.insert(key.into(), json!(free));
						}
						let needed = DISK_FLOOR_BYTES + bytes;
						if free < needed {
							refusal = Some((path, free, needed));
							break;
						}
					}
					if let Some((path, free, needed)) = refusal {
						recorder.borrow_mut().wait(
							index,
							now(),
							"disk",
							vec![],
							json!({"path": path,"free_bytes": free, "needed_bytes": needed}),
						);
						queue_rest(position, index);
						if !running.is_empty() {
							break;
						}
						return Err(value_error(&format!(
							"disk admission before {} at {}: {free} bytes free, {needed} needed",
							stages[index].0,
							path.display()
						)));
					}
					// A stage's tokens, whole or not at all: no part is held while
					// it waits, so a running stage's clients can always take them.
					// Too few is a refusal with nothing read or written back, so
					// it raises no write to wake on.
					let lease = if pool.available()? >= weights[index] {
						pool.take(weights[index])?
					} else {
						None
					};
					let Some(lease) = lease else {
						token_waiting = true;
						settings.hooks.after_token_refusal();
						recorder.borrow_mut().wait(
							index,
							now(),
							"tokens",
							vec![],
							json!({"weight": weights[index]}),
						);
						queue_rest(position, index);
						break;
					};
					let (name, command) = stages[index].clone();
					settings
						.hooks
						.check_selection(selection_value, root, &settings.environment)?;
					let shared = (!plan[index].contains(ALONE)).then_some((&*pool, weights[index]));
					let mut env = match stage_environment(
						settings,
						(index, &name),
						&temporaries,
						&command,
						owner,
						base,
						shared,
					) {
						Ok(env) => env,
						Err(error) => {
							drop(lease);
							return Err(error);
						}
					};
					if let Some(cache) = cache.as_ref() {
						cache.environment(&name, &mut env)?;
					}
					let cross = command
						.windows(2)
						.find(|w| w[0] == "--out" && w[1].contains("rnx-verify"))
						.and_then(|w| Path::new(&w[1]).parent().map(Path::to_owned));
					environments.insert(
						name.clone(),
						environment_digests(&env, &settings.out, &owner.id, cross.as_deref())?,
					);
					pending.retain(|i| *i != index);
					recorder.borrow_mut().start(index, now());
					let stopper = Arc::new(Stopper::default());
					let handle = spawn_stage(
						settings,
						index,
						name,
						command,
						env,
						stopper.clone(),
						sender.clone(),
					);
					running.insert(index, (lease, stopper, handle));
					if plan[index].contains(ALONE) {
						queue_rest(position, index);
						break;
					}
				}
			}
			if running.is_empty() {
				if failed.is_some() || pending.is_empty() {
					return Ok(());
				}
				// Nothing runs and nothing could start: no later pass would differ.
				return Err(value_error(&format!(
					"no selected stage can start: {}",
					stages[pending[0]].0
				)));
			}
			// Wait on a stage's end, a signal, or a write to the jobserver
			// while a stage waits on tokens.
			enum Woken {
				Tokens,
				Event(Event),
				Closed,
			}
			let woken = loop {
				if tokens_noted {
					tokens_noted = false;
					// Cleared before the recheck, never after: a write coalesced
					// under the old flag is covered by the coming pass, and a
					// later write sends a new event.
					tokens_pending.store(false, Ordering::SeqCst);
					if token_waiting {
						break Woken::Tokens;
					}
				}
				settings.hooks.before_block();
				recorder.borrow_mut().controller(now(), "blocked", None);
				match receiver.recv() {
					Ok(Event::Tokens) => {
						settings.hooks.tokens_received();
						tokens_noted = true;
					}
					Ok(Event::WatchFailed(error)) => {
						bail!(value_error(&format!("jobserver watch failed: {error}")))
					}
					Ok(event) => break Woken::Event(event),
					Err(_) => break Woken::Closed,
				}
			};
			let event = match woken {
				Woken::Tokens => {
					recorder
						.borrow_mut()
						.controller(now(), "woken", Some("tokens".into()));
					continue;
				}
				Woken::Event(event) => Some(event),
				Woken::Closed => None,
			};
			let by = match &event {
				Some(Event::Done(index, _)) => Some(stages[*index].0.clone()),
				Some(Event::Signal(_)) => Some("signal".to_string()),
				_ => None,
			};
			recorder.borrow_mut().controller(now(), "woken", by);
			match event {
				Some(Event::Done(index, result)) => {
					complete(
						owner,
						index,
						result,
						&mut running,
						&mut failed,
						&mut finished,
						&mut succeeded,
					);
					// Recheck the clean tree and the pinned selection after every stage.
					settings
						.hooks
						.check_selection(selection_value, root, &settings.environment)?;
				}
				Some(Event::Signal(signal)) => interrupted.store(signal, Ordering::SeqCst),
				_ => {}
			}
		}
	})();
	if outcome.is_err() {
		// A signal or controller error: stop every running stage, keep receipts.
		for (_, stopper, _) in running.values() {
			stopper.stop();
		}
		while !running.is_empty() {
			match receiver.recv() {
				Ok(Event::Done(index, result)) => complete(
					owner,
					index,
					result,
					&mut running,
					&mut failed,
					&mut finished,
					&mut succeeded,
				),
				Ok(Event::Signal(_) | Event::Tokens | Event::WatchFailed(_)) => {}
				Err(_) => break,
			}
		}
	}
	rows.extend(done_rows.into_values());
	atomic_json(&settings.out.join("ENVIRONMENTS.json"), &environments)?;
	let schedule = std::mem::replace(
		&mut *recorder.borrow_mut(),
		crate::schedule::Recorder::new(&[]),
	)
	.finish();
	atomic_json(&settings.out.join("SCHEDULE.json"), &schedule)?;
	// The watcher stops and joins before the pool closes and its fifo goes.
	drop(watcher);
	let pool = Arc::try_unwrap(pool).map_err(|_| anyhow::anyhow!("jobserver still shared"))?;
	let closed = pool.close();
	outcome?;
	closed?;
	Ok(failed)
}

fn spawn_stage(
	settings: &Settings,
	index: usize,
	name: String,
	command: Vec<String>,
	env: Environment,
	stopper: Arc<Stopper>,
	sender: Notifier,
) -> std::thread::JoinHandle<()> {
	let (root, out, guardian_exe, hooks) = (
		settings.root.clone(),
		settings.out.clone(),
		settings.guardian.clone(),
		settings.hooks.clone(),
	);
	let groups = PathBuf::from(env.get("RNX_FINAL_GROUPS").cloned().unwrap_or_default());
	std::thread::spawn(move || {
		let log = out.join(format!("{name}.log"));
		let directory = out.join(format!("{name}-child"));
		let call = StageCall {
			name: name.clone(),
			command: command.clone(),
			environment: env.clone(),
			log: log.clone(),
			directory: directory.clone(),
		};
		let result = match hooks.stage(&call) {
			Some(result) => result,
			None => {
				let after_root = root.clone();
				let after_out = out.clone();
				let after_name = name.clone();
				let after = move || outputs::finish_stage(&after_root, &after_name, &after_out);
				child::run(child::Run {
					command: &command,
					cwd: &root,
					root: &root,
					env: &env,
					log: &log,
					timeout: Some(STAGE_TIMEOUT),
					directory: &directory,
					groups: &groups,
					guardian: &guardian_exe,
					after: Some(&after),
					stop: Some(&stopper),
				})
			}
		};
		let _ = sender.send(Event::Done(index, result));
	})
}

/// Snapshot errors cannot prevent release after failed acceptance.
pub fn terminal(
	settings: &Settings,
	owner: Option<&mut Owners>,
	selection_value: Option<&Value>,
	evidence_map: &Map<String, Value>,
	tools: &Value,
	acceptance: &mut Value,
) -> Value {
	let out = &settings.out;
	let mut release = json!({"status": "FAIL"});
	let mut manifest = json!({"schema": 1, "candidate": acceptance["candidate"], "roots": []});
	let outcome = (|| -> Result<()> {
		let mut owner = owner;
		if let Some(owner) = owner.as_deref_mut() {
			guardian::drain_python_groups(&owner.groups)?;
			guardian::drain_guardians(&owner.groups, owner.row.unit.value().map(String::as_str))?;
			if let Some(selection_value) = selection_value {
				match settings
					.hooks
					.compact(selection_value, evidence_map, owner, tools)
				{
					Ok(m) => manifest = m,
					Err(error) => {
						acceptance["manifest_error"] = json!(format!("{error:#}"));
						if acceptance["status"] == "PASS" {
							// A passing acceptance may not release without its full manifest.
							return Err(error);
						}
						manifest["incomplete"] = json!(true);
						manifest["roots"] = json!(
							owner
								.row
								.roots
								.iter()
								.map(|e| json!({"owner": e}))
								.collect::<Vec<_>>()
						);
					}
				}
			}
			if acceptance["status"] != "PASS"
				&& let Err(error) = settings.hooks.diagnostics(
					out,
					&owner.row.roots,
					acceptance
						.get("error")
						.and_then(Value::as_str)
						.unwrap_or("None"),
					&settings.environment,
				) {
				acceptance["diagnostics_error"] = json!(format!("{error:#}"));
			}
		}
		types::write_as::<types::Acceptance>(&out.join("ACCEPTANCE.json"), acceptance)?;
		types::write_as::<types::Manifest>(&out.join("MANIFEST.json"), &manifest)?;
		if let Some(owner) = owner {
			retain_semantic(out, evidence_map, owner)?;
			release = settings.hooks.release(owner)?;
			release["acceptance_sha256"] = json!(digest(&out.join("ACCEPTANCE.json"))?);
			release["manifest_sha256"] = json!(digest(&out.join("MANIFEST.json"))?);
		}
		Ok(())
	})();
	if let Err(error) = outcome {
		release["status"] = json!("FAIL");
		release["error"] = json!(format!("{error:#}"));
		// Persist the original outcome even when the manifest failed.
		if !out.join("ACCEPTANCE.json").exists() {
			let _ = types::write_as::<types::Acceptance>(&out.join("ACCEPTANCE.json"), acceptance);
		}
	}
	if let Err(error) = types::write_as::<types::Release>(&out.join("RELEASE.json"), &release) {
		// A release that is not its type is recorded as a failed one.
		release = json!({"status": "FAIL", "error": format!("{error:#}")});
		let _ = types::write_as::<types::Release>(&out.join("RELEASE.json"), &release);
	}
	if release["status"] == "PASS" && out.join("fixed-route/RESULT.json").is_file() {
		let _ = (|| -> Result<()> {
			atomic_json(
				&out.join("fixed-route/PAYLOAD-RELEASE.json"),
				&json!({
					"status": "PASS", "payload": "released; artifact paths in measurement receipts are historical",
					"manifest_sha256": digest(&out.join("MANIFEST.json"))?, "release_sha256": digest(&out.join("RELEASE.json"))?,
				}),
			)
		})();
	}
	release
}

static SIGNAL_PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static SIGNAL_LISTENERS: std::sync::Mutex<Vec<Notifier>> = std::sync::Mutex::new(Vec::new());
static SIGNAL_SETUP: std::sync::Once = std::sync::Once::new();

extern "C" fn on_signal(signal: libc::c_int) {
	let fd = SIGNAL_PIPE.load(Ordering::Relaxed);
	let byte = signal as u8;
	// SAFETY: write(2) is async-signal-safe.
	unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
}

/// SIGINT and SIGTERM become events for every running final, whichever
/// thread the kernel delivers them to; a stage's exec resets them.
fn signal_events(sender: Notifier) -> Result<()> {
	SIGNAL_LISTENERS.lock().unwrap().push(sender);
	let mut failure = None;
	SIGNAL_SETUP.call_once(|| {
		let mut fds = [0; 2];
		// SAFETY: two descriptors for the self-pipe, close-on-exec.
		if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
			failure = Some(std::io::Error::last_os_error());
			return;
		}
		SIGNAL_PIPE.store(fds[1], Ordering::Relaxed);
		std::thread::spawn(move || {
			let mut byte = 0u8;
			// SAFETY: blocking reads of the self-pipe's read end.
			while unsafe { libc::read(fds[0], (&mut byte as *mut u8).cast(), 1) } == 1 {
				SIGNAL_LISTENERS
					.lock()
					.unwrap()
					.retain(|s| s.send(Event::Signal(byte as i32)));
			}
		});
		for signal in [libc::SIGINT, libc::SIGTERM] {
			let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
			action.sa_sigaction = on_signal as *const () as usize;
			action.sa_flags = libc::SA_RESTART;
			// SAFETY: installing a handler that only writes to a pipe.
			unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };
		}
	});
	match failure {
		Some(error) => Err(error.into()),
		None => Ok(()),
	}
}

pub fn run(settings: &Settings) -> Result<Value> {
	let scope = settings.scope.as_str();
	publication_eligible(scope)?;
	let root = std::fs::canonicalize(&settings.root)?;
	let out = &settings.out;
	// Inspect fresh targets before this invocation creates any directory.
	let violation = fresh_violation(&root, settings.fresh, &settings.environment);
	std::fs::create_dir_all(out.parent().context("output has no parent")?)?;
	if settings.checkpoint().is_some() {
		let entries: BTreeSet<_> = std::fs::read_dir(out)?
			.map(|e| e.map(|entry| entry.file_name()))
			.collect::<std::io::Result<_>>()?;
		anyhow::ensure!(
			entries == BTreeSet::from(["preparation".into()]),
			"unexpected checkpoint output contents"
		);
	} else {
		std::fs::create_dir(out).with_context(|| format!("output exists: {}", out.display()))?;
	}
	let registry = root.join("target/final-owners");
	std::fs::create_dir_all(&registry)?;
	let free_before = disk_free(&root)?;
	let (sender, receiver) = notifier();
	signal_events(sender.clone())?;
	let interrupted = Arc::new(AtomicI32::new(0));
	// Stages fingerprint .git: no git command may refresh the index meanwhile.
	let mut environment = settings.environment.clone();
	environment.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
	let observation = job_budget::memory::observe(&Default::default());
	let events_before = job_budget::memory::memory_events();
	let tokens = match settings.tokens {
		Some(tokens) => tokens,
		None => job_budget::memory::final_jobs(&observation?)?,
	};
	environment.insert("CARGO_BUILD_JOBS".into(), tokens.to_string());
	environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
	environment.insert("RNX_FINAL".into(), "1".into());
	let settings = Settings {
		environment: environment.clone(),
		root: root.clone(),
		out: out.clone(),
		..settings.clone()
	};
	let candidate = settings.hooks.head(&root);
	let mut acceptance = json!({"status": "FAIL", "candidate": candidate, "acceptance_scope": scope, "mode": if settings.fresh { "fresh-clone" } else { "ordinary" }});
	let (mut selection_value, mut owner, mut failed_row) = (None, None, None);
	// Set once the stages begin: from then on the final owes a schedule.
	let mut scheduled = false;
	let mut cache = None;
	let (mut evidence_map, mut tools, mut rows, mut disk) =
		(Map::new(), json!({}), Vec::new(), Map::new());
	let lock = std::fs::OpenOptions::new()
		.read(true)
		.append(true)
		.create(true)
		.open(registry.join("DRIVER.lock"))?;
	let outcome = (|| -> Result<()> {
		// SAFETY: flock on a descriptor this function owns.
		if unsafe {
			libc::flock(
				std::os::fd::AsRawFd::as_raw_fd(&lock),
				libc::LOCK_EX | libc::LOCK_NB,
			)
		} != 0
		{
			bail!(child::raise(
				"BlockingIOError",
				"another final holds the driver lock"
			));
		}
		let recovered = evidence::recover(&registry, guardian::drain_registry)?;
		let mut owners = Owners::with_unit(&registry, out, settings.unit.as_deref())?;
		environment.insert(
			"RNX_FINAL_GROUPS".into(),
			owners.groups.to_string_lossy().into_owned(),
		);
		let result = (|| -> Result<()> {
			if let Some(violation) = &violation {
				return Err(value_error(violation));
			}
			if settings.hooks.cargo_caches() {
				let mut prepared =
					crate::cache::CachePlan::prepare(&root, out, scope == "complete")?;
				if settings.bootstrap.is_some() {
					prepared.bootstrap()?;
				}
				cache = Some(prepared);
			}
			settings.hooks.disk_check(&root, out, &environment)?;
			let selected = settings.hooks.select(
				&root,
				out,
				settings.names.as_deref(),
				scope,
				&settings.environment,
			)?;
			settings
				.hooks
				.check_selection(&selected, &root, &settings.environment)?;
			types::write_as::<types::Selection>(&out.join("SELECTION.json"), &selected)?;
			selection_value = Some(selected.clone());
			tools = settings.hooks.tools_identity(&settings)?;
			let mut collected = Collected {
				cache: cache.take(),
				..Default::default()
			};
			scheduled = true;
			let failure = run_stages(
				&settings,
				&selected,
				&mut owners,
				&environment,
				&mut collected,
				&interrupted,
				(&sender, &receiver),
			);
			cache = collected.cache.take();
			// Rows and evidence count even when the stages ended in an error.
			(evidence_map, rows, disk) = (collected.evidence, collected.rows, collected.disk);
			let failure = failure?;
			if let Some(failure) = failure {
				failed_row = Some(failure.row);
				bail!(failure.message);
			}
			acceptance["status"] = json!("PASS");
			acceptance["finalization"] =
				settings
					.hooks
					.finalize(&selected, &evidence_map, &root, &settings.environment)?;
			acceptance["recovered"] = json!(recovered);
			Ok(())
		})();
		owner = Some(owners);
		result
	})();
	if let Err(error) = &outcome {
		acceptance["status"] = json!("FAIL");
		acceptance["error"] = match failed_row.as_ref().and_then(|r| r.get("failure")) {
			Some(f) => f["error"].clone(),
			None => json!(format!("{error:#}")),
		};
	}
	acceptance["checks"] = json!(rows);
	acceptance["seconds"] = json!(settings.hooks.elapsed(settings.started));
	if !disk.is_empty() {
		acceptance["disk"] = Value::Object(disk.clone());
	}
	// A final that ran its stages declares their schedule, so revalidation
	// requires it; without the record it cannot pass.
	if scheduled {
		match digest(&out.join("SCHEDULE.json")) {
			Ok(sha) => acceptance["schedule_sha256"] = json!(sha),
			Err(error) => {
				if acceptance["status"] == "PASS" || acceptance["error"].is_null() {
					acceptance["error"] = json!(format!("schedule record unreadable: {error:#}"));
				}
				acceptance["status"] = json!("FAIL");
			}
		}
	}
	// Throttling at the control group's high limit is evidence about a failed deadline.
	acceptance["memory"] = json!({"tokens": tokens.to_string(), "events_before": events_before, "events_after": job_budget::memory::memory_events()});
	if let Some(row) = &failed_row {
		acceptance["failed_stage"] = row["name"].clone();
		if let Some(f) = row.get("failure") {
			acceptance["failure"] = f.clone();
		}
	}
	let release = terminal(
		&settings,
		owner.as_mut(),
		selection_value.as_ref(),
		&evidence_map,
		&tools,
		&mut acceptance,
	);
	let pass = acceptance["status"] == "PASS" && release["status"] == "PASS";
	// The declared schedule must still be the one bound below.
	let declared = acceptance.get("schedule_sha256").cloned();
	let schedule_now = digest(&out.join("SCHEDULE.json")).ok().map(|d| json!(d));
	let schedule_error = (declared.is_some() && declared != schedule_now)
		.then(|| json!("schedule record changed before binding"));
	let pass = pass && schedule_error.is_none();
	let records: Map<String, Value> = [
		"ACCEPTANCE",
		"MANIFEST",
		"RELEASE",
		"ENVIRONMENTS",
		"SCHEDULE",
	]
	.iter()
	.filter_map(|n| {
		digest(&out.join(format!("{n}.json")))
			.ok()
			.map(|d| (n.to_string(), json!(d)))
	})
	.collect();
	let mut r = json!({
		"status": if pass { "PASS" } else { "FAIL" }, "candidate": acceptance["candidate"], "seconds": settings.hooks.elapsed(settings.started),
		"checks": rows.iter().map(|row| json!({"name": row["name"], "exit": row["exit"]})).collect::<Vec<_>>(),
		"error": acceptance.get("error").cloned().filter(|e| !e.is_null()).or(schedule_error).unwrap_or(Value::Null), "release_error": release.get("error").cloned().unwrap_or(Value::Null),
		"manifest_error": acceptance.get("manifest_error").cloned().unwrap_or(Value::Null), "diagnostics_error": acceptance.get("diagnostics_error").cloned().unwrap_or(Value::Null),
		"finalization": acceptance.get("finalization").cloned().unwrap_or(Value::Null), "remote_context": null,
		"acceptance_scope": scope, "complete_final": scope == "complete" && pass, "publication_eligible": publication_eligible(scope)? && pass,
		"free_bytes_before": free_before, "free_bytes_after": disk_free(&root)?, "records": records,
	});
	if let Some(selection) = &selection_value
		&& selection["record"]["phase"] == "fix"
	{
		r["fix_subject"] = selection["record"]["subject"].clone();
	}
	if let Some(row) = &failed_row {
		r["failed_stage"] = row["name"].clone();
		if let Some(f) = row.get("failure") {
			r["failure"] = f.clone();
		}
	}
	if let Some(owner) = &owner
		&& release["status"] == "PASS"
		&& let Err(error) = owner.retire()
	{
		r["status"] = json!("FAIL");
		r["complete_final"] = json!(false);
		r["publication_eligible"] = json!(false);
		r["retirement_error"] = json!(format!("{error:#}"));
	}
	if r["status"] == "PASS"
		&& let Some(plan) = &cache
	{
		match plan.transaction(scope == "complete") {
			Ok(hash) => r["records"]["CACHE"] = json!(hash),
			Err(error) => {
				r["status"] = json!("FAIL");
				r["complete_final"] = json!(false);
				r["publication_eligible"] = json!(false);
				r["error"] = json!(format!("Cargo cache transaction: {error:#}"));
			}
		}
	}
	// The budget covers the whole run: stages, finalization and release.
	if let Some(value) = settings.checkpoint() {
		let settled = (|| -> Result<()> {
			let run = PathBuf::from(
				settings
					.environment
					.get("RNX_FINAL_CHECKPOINT_INVOCATION")
					.context("missing invocation directory")?,
			);
			let output = std::process::Command::new("/bin/bash")
				.arg(root.join("tools/final/checkpoint/settle.sh"))
				.arg(&run)
				.arg(
					r["status"]
						.as_str()
						.context("missing provisional verdict")?,
				)
				.arg(out.join("preparation/admission.json"))
				.output()?;
			anyhow::ensure!(
				output.status.success(),
				"checkpoint release: {}",
				String::from_utf8_lossy(&output.stderr)
			);
			let released: Value = serde_json::from_slice(&output.stdout)?;
			anyhow::ensure!(
				released["developer_state_sha256"] == value["developer_state_sha256"],
				"developer home changed"
			);
			let path = out.join("CHECKPOINTRELEASE.json");
			crate::receipt::atomic_json(&path, &released)?;
			r["records"]["CHECKPOINTRELEASE"] = json!(digest(&path)?);
			Ok(())
		})();
		if let Err(error) = settled {
			r["status"] = json!("FAIL");
			r["publication_eligible"] = json!(false);
			r["complete_final"] = json!(false);
			r["release_error"] = json!(format!("{error:#}"));
		}
	}
	let seconds = if let Some(value) = settings.checkpoint() {
		let admission: crate::checkpoint::Admission = serde_json::from_value(value.clone())?;
		let seconds = crate::bootstrap::boottime() - admission.admitted;
		let proof = out.join("CHECKPOINT.json");
		let timing = admission.timing(seconds);
		crate::receipt::atomic_json(&proof, &timing)?;
		r["records"]["CHECKPOINT"] = json!(digest(&proof)?);
		eprintln!(
			"Preparation {:.1} s, timed final {:.1} s, total wait {:.1} s",
			timing.preparation_seconds, seconds, timing.total_seconds
		);
		seconds
	} else {
		settings.hooks.elapsed(settings.started)
	};
	r["seconds"] = json!(seconds);
	if let Some(over) = wall_budget_error(scope, seconds) {
		r["status"] = json!("FAIL");
		r["complete_final"] = json!(false);
		r["publication_eligible"] = json!(false);
		r["wall_budget_error"] = json!(over);
		// A stage failure stays the reported cause; otherwise the budget is.
		if r["error"].is_null() {
			r["error"] = json!(over);
			if let Some(store) = settings.environment.get("RNX_FINAL_CHECKPOINT_STORE")
				&& r["records"].get("CHECKPOINT").is_some()
				&& pass && r["release_error"].is_null()
				&& r.get("retirement_error").is_none()
			{
				let candidate = r["candidate"]
					.as_str()
					.context("overrun without candidate")?;
				let path = Path::new(store).join("overruns").join(candidate);
				atomic_json(&path, out)?;
			}
		}
	}
	child_publish(&out.join("RESULT.json"), &r)?;
	if r["status"] == "PASS"
		&& let Some(store) = settings.environment.get("RNX_FINAL_CHECKPOINT_STORE")
		&& r["records"].get("CHECKPOINT").is_some()
	{
		// The next entry revalidates this terminal and the actual state
		// under its lease before it may admit a published successor.
		let store = Path::new(store);
		let release: Value =
			serde_json::from_reader(std::fs::File::open(out.join("CHECKPOINTRELEASE.json"))?)?;
		atomic_json(
			&store.join("previous.json"),
			&json!({
				"candidate":r["candidate"], "checkout":root, "out":out,
				"result":digest(&out.join("RESULT.json"))?, "seal":release["terminal_state_sha256"]
			}),
		)?;
		std::fs::remove_file(store.join("in-use"))?;
		crate::receipt::sync_directory(store)?;
	}
	if r["status"] == "PASS"
		&& scope != "complete"
		&& let Some(cache) = cache
	{
		cache.finish()?;
	}
	Ok(r)
}

/// The controller's own result: no validation context binds it.
fn child_publish(path: &Path, value: &Value) -> Result<()> {
	types::write_as::<types::FinalResult>(path, value)
}

/// The paths an amend changed, when it changed nothing but the plans.
pub fn docs_change(root: &Path, prior: &str, candidate: &str) -> Result<Vec<String>> {
	let git = |args: &[&str]| -> Result<Vec<u8>> {
		let output = crate::child::git_command()
			.args(["-c", "color.ui=false"])
			.args(args)
			.current_dir(root)
			.output()?;
		anyhow::ensure!(output.status.success(), "git {args:?} failed");
		Ok(output.stdout)
	};
	if git(&["rev-parse", &format!("{prior}^")])? != git(&["rev-parse", &format!("{candidate}^")])?
	{
		return Err(value_error(
			"docs successor must amend the passed commit onto the same parent",
		));
	}
	// Without rename folding, a file moved into plans/ also lists its source.
	let changed: Vec<String> = git(&[
		"diff",
		"--no-renames",
		"--name-only",
		"-z",
		prior,
		candidate,
	])?
	.split(|b| *b == 0)
	.filter(|p| !p.is_empty())
	.map(|p| String::from_utf8_lossy(p).into_owned())
	.collect();
	if changed.is_empty() || changed.iter().any(|p| !p.starts_with("plans/")) {
		return Err(value_error(&format!(
			"docs successor allows changes under plans/ only: {}",
			failure::python_repr(&json!(changed))
		)));
	}
	Ok(changed)
}

/// Extend a passed complete final to a docs-only amend of its commit.
pub fn successor(settings: &Settings, prior: &Path) -> Result<Value> {
	let prior = std::fs::canonicalize(prior)?;
	let earlier = revalidate_any(&prior)?;
	if earlier["complete_final"] != true || earlier["publication_eligible"] != true {
		return Err(value_error(
			"docs successor requires a passed complete final",
		));
	}
	let root = &settings.root;
	let candidate = settings.hooks.head(root).context("no candidate")?;
	let status = crate::child::git_command()
		.args(["status", "--porcelain"])
		.current_dir(root)
		.output()?;
	if !status.stdout.is_empty() {
		return Err(value_error("docs successor requires a clean tree"));
	}
	let changed = docs_change(
		root,
		earlier["candidate"].as_str().unwrap_or_default(),
		&candidate,
	)?;
	let out = &settings.out;
	std::fs::create_dir(out)?;
	let plans_final = out.join("plans-final");
	let docs = run(&Settings {
		out: plans_final.clone(),
		scope: "docs".into(),
		names: None,
		fresh: false,
		..settings.clone()
	})?;
	let mut r = json!({
		"status": "FAIL", "candidate": candidate, "prior": prior.to_string_lossy(), "prior_candidate": earlier["candidate"],
		"prior_result_sha256": digest(&prior.join("RESULT.json"))?, "changed": changed, "reran": catalogue::READS_PLANS, "plans_final": plans_final.to_string_lossy(),
	});
	if docs["status"] == "PASS" {
		r["plans_final_result_sha256"] = json!(digest(&plans_final.join("RESULT.json"))?);
		r["status"] = json!("PASS");
	}
	types::write_as::<types::Successor>(&out.join("SUCCESSOR.json"), &r)?;
	let pass = r["status"] == "PASS";
	r["acceptance_scope"] = if pass { json!("complete") } else { Value::Null };
	Ok(r)
}

/// A final or a docs successor, whichever `out` holds.
pub fn revalidate_any(out: &Path) -> Result<Value> {
	if out.join("SUCCESSOR.json").exists() {
		revalidate_successor(out, None)
	} else {
		evidence::revalidate(out)
	}
}

pub fn revalidate_successor(out: &Path, root: Option<&Path>) -> Result<Value> {
	let typed: types::Successor = types::read(&out.join("SUCCESSOR.json"))?;
	let r = serde_json::to_value(&typed)?;
	if typed.status != types::Status::Pass {
		return Err(value_error("docs successor did not pass"));
	}
	let prior = PathBuf::from(r["prior"].as_str().unwrap_or_default());
	if Some(digest(&prior.join("RESULT.json"))?.as_str()) != r["prior_result_sha256"].as_str() {
		return Err(value_error("prior final changed"));
	}
	let earlier = evidence::revalidate(&prior)?;
	if earlier["candidate"] != r["prior_candidate"] || earlier["complete_final"] != true {
		return Err(value_error("prior final differs"));
	}
	let plans_final = PathBuf::from(r["plans_final"].as_str().unwrap_or_default());
	if Some(digest(&plans_final.join("RESULT.json"))?.as_str())
		!= r["plans_final_result_sha256"].as_str()
	{
		return Err(value_error("plans final changed"));
	}
	let plans = evidence::revalidate(&plans_final)?;
	if plans["candidate"] != r["candidate"] {
		return Err(value_error("plans final is for another commit"));
	}
	if plans["acceptance_scope"] != "docs" {
		return Err(value_error("plans final ran another selection"));
	}
	let default_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let root = root.unwrap_or(&default_root);
	let changed: Value = json!(docs_change(
		root,
		r["prior_candidate"].as_str().unwrap_or_default(),
		r["candidate"].as_str().unwrap_or_default()
	)?);
	if changed != r["changed"] {
		return Err(value_error("docs change differs"));
	}
	Ok(
		json!({"status": "PASS", "candidate": r["candidate"], "acceptance_scope": "complete", "complete_final": true, "publication_eligible": true, "extends": r["prior"], "changed": r["changed"]}),
	)
}
