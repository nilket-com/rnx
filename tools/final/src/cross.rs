//! `rnx-final stage cross` (ADR 0294): prepare a private Cargo graph and
//! launch the Rust cross-repository verifier, as the retired Python
//! launcher did. Observation, source binding, comparison and acceptance
//! belong to the verifier; this launcher never changes the product manifest
//! or lock. The engine pin checks (ADR 0211) live here too.
use crate::child::{Raised, message, raise, value_error};
use crate::failure::{Environment, python_repr};
use crate::receipt::digest;
use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

/// The census plan, under the controller's checkout.
pub const CENSUS_PLAN: &str = "tools/verify/fixtures/census/execution.tsv";
/// What a census failure's receipt lists for retention, in priority order:
/// the final keeps at most eight files, the receipt included.
const CENSUS_EVIDENCE: [(&str, &str); 6] = [
	("stock", "report/result.json"),
	("fresh", "report/result.json"),
	("stock", "stderr"),
	("fresh", "stderr"),
	("stock", "stdout"),
	("fresh", "stdout"),
];
const HEADLINE: &str = "unreviewed census execution differences";
const TOOLCHAIN: &str = "+1.98.1";

// --- Engine pin checks (ADR 0211): canonical path sources, no fetches. ---

fn engine_name(name: &str) -> bool {
	name == "rune" || name.starts_with("rune-")
}

fn table<'a>(value: &'a toml::Value, what: &str) -> Result<&'a toml::Table> {
	value
		.as_table()
		.with_context(|| format!("{what} is not a table"))
}

/// Every engine dependency a manifest declares, as (alias, spec), in
/// document order: its dependency sections, then each target's, each patch
/// table's, and the workspace's.
fn dependency_specs(document: &toml::Table, specs: &mut Vec<(String, toml::Value)>) -> Result<()> {
	for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
		let Some(entries) = document.get(section) else {
			continue;
		};
		for (alias, spec) in table(entries, section)? {
			let name = match spec.as_table().and_then(|t| t.get("package")) {
				Some(package) => package.as_str().context("package is not a string")?,
				None => alias,
			};
			if engine_name(name) {
				specs.push((alias.clone(), spec.clone()));
			}
		}
	}
	if let Some(targets) = document.get("target") {
		for target in table(targets, "target")?.values() {
			dependency_specs(table(target, "target entry")?, specs)?;
		}
	}
	if let Some(patches) = document.get("patch") {
		for patch in table(patches, "patch")?.values() {
			let mut wrapped = toml::Table::new();
			wrapped.insert("dependencies".into(), patch.clone());
			dependency_specs(&wrapped, specs)?;
		}
	}
	if let Some(workspace) = document.get("workspace") {
		let mut wrapped = toml::Table::new();
		if let Some(dependencies) = table(workspace, "workspace")?.get("dependencies") {
			wrapped.insert("dependencies".into(), dependencies.clone());
		}
		dependency_specs(&wrapped, specs)?;
	}
	Ok(())
}

/// Python's truth value of a TOML value.
fn truthy(value: &toml::Value) -> bool {
	match value {
		toml::Value::Boolean(b) => *b,
		toml::Value::String(s) => !s.is_empty(),
		toml::Value::Integer(i) => *i != 0,
		toml::Value::Float(f) => *f != 0.0,
		toml::Value::Array(a) => !a.is_empty(),
		toml::Value::Table(t) => !t.is_empty(),
		toml::Value::Datetime(_) => true,
	}
}

fn parse_toml(path: &Path) -> Result<toml::Table> {
	let text = std::fs::read_to_string(path)?;
	toml::from_str(&text).map_err(|e| value_error(&format!("{}: {e}", path.display())))
}

/// The engine crate's manifest inside the canonical engine.
pub fn canonical_manifest(root: &Path, name: &str) -> Result<PathBuf> {
	let engine = std::fs::canonicalize(root.join("rune"))?;
	let resolved = std::fs::canonicalize(engine.join("crates").join(name).join("Cargo.toml"))?;
	if !resolved.starts_with(&engine) {
		bail!(value_error("canonical engine path escapes imported source"));
	}
	Ok(resolved)
}

/// A manifest's engine dependencies: each an explicit path to its canonical
/// engine crate; their names.
pub fn check_manifest(root: &Path, path: &Path) -> Result<Vec<String>> {
	let mut specs = Vec::new();
	dependency_specs(&parse_toml(path)?, &mut specs)?;
	let shown = path.display();
	let mut checked = Vec::new();
	for (alias, spec) in specs {
		let Some(spec) = spec.as_table().filter(|t| {
			!t.contains_key("git")
				&& !t.contains_key("registry")
				&& !t.contains_key("registry-index")
		}) else {
			bail!(value_error(&format!(
				"{shown}: external engine dependency {alias}"
			)));
		};
		let name = match spec.get("package") {
			Some(package) => package.as_str().context("package is not a string")?,
			None => alias.as_str(),
		};
		if spec.get("workspace").is_some_and(truthy) {
			bail!(value_error(&format!(
				"{shown}: engine dependencies require explicit canonical paths"
			)));
		}
		let Some(selected) = spec.get("path") else {
			bail!(value_error(&format!(
				"{shown}: missing canonical engine path"
			)));
		};
		let selected = path
			.parent()
			.context("manifest without directory")?
			.join(selected.as_str().context("path is not a string")?)
			.join("Cargo.toml");
		let selected = std::fs::canonicalize(selected)?;
		if selected != canonical_manifest(root, name)? {
			bail!(value_error(&format!(
				"{shown}: engine dependency path differs from canonical source"
			)));
		}
		checked.push(name.to_owned());
	}
	Ok(checked)
}

/// A lock's engine packages: none from an external source, none twice.
pub fn check_lock(path: &Path) -> Result<Vec<String>> {
	let document = parse_toml(path)?;
	let mut seen = BTreeSet::new();
	let packages = match document.get("package") {
		Some(p) => p.as_array().context("package is not an array")?.as_slice(),
		None => &[],
	};
	for package in packages {
		let package = table(package, "package entry")?;
		let name = package
			.get("name")
			.context("package without name")?
			.as_str()
			.context("package name is not a string")?;
		if !engine_name(name) {
			continue;
		}
		if package.contains_key("source") {
			bail!(value_error(&format!(
				"{}: external engine source in lock",
				path.display()
			)));
		}
		if !seen.insert(name.to_owned()) {
			bail!(value_error(&format!(
				"{}: duplicate engine package",
				path.display()
			)));
		}
	}
	Ok(seen.into_iter().collect())
}

/// Cargo's resolved graph: every engine package local, once, inside the
/// canonical engine, an engine crate at its canonical manifest, and Rune
/// itself present.
pub fn check_metadata(root: &Path, document: &Value) -> Result<Vec<String>> {
	let mut seen = BTreeSet::new();
	for package in document["packages"]
		.as_array()
		.context("metadata without packages")?
	{
		let name = package["name"].as_str().context("package without name")?;
		if !engine_name(name) {
			continue;
		}
		let source = package.get("source").context("package without source")?;
		if !source.is_null() || seen.contains(name) {
			bail!(value_error("external or duplicate resolved engine source"));
		}
		let manifest = std::fs::canonicalize(
			package["manifest_path"]
				.as_str()
				.context("package without manifest path")?,
		)?;
		// Active engine tools may have Rune-prefixed names outside crates/.
		let engine = std::fs::canonicalize(root.join("rune"))?;
		if !manifest.starts_with(&engine) {
			bail!(value_error(
				"resolved engine manifest escapes canonical subtree"
			));
		}
		if engine.join("crates").join(name).exists() && manifest != canonical_manifest(root, name)?
		{
			bail!(value_error("resolved Rune manifest is not canonical"));
		}
		seen.insert(name.to_owned());
	}
	if !seen.contains("rune") {
		bail!(value_error("resolved graph has no canonical Rune"));
	}
	Ok(seen.into_iter().collect())
}

/// Every manifest and lock in the checkout, outside Git, targets, bytecode
/// caches, links and the historical `probes` and `provenance` trees, and
/// the two live lock graphs required.
pub fn check_tree(root: &Path) -> Result<Value> {
	let root = std::fs::canonicalize(root)?;
	let (mut manifests, mut locks) = (Vec::new(), Vec::new());
	walk(&root, &root, &mut manifests, &mut locks)?;
	for required in ["Cargo.lock", "tools/project/Cargo.lock"] {
		if !locks.iter().any(|l| l == required) {
			bail!(value_error(&format!(
				"missing canonical engine lock graph: {required}"
			)));
		}
	}
	Ok(json!({"mode": "canonical-path", "manifests": manifests, "locks": locks}))
}

/// One directory, then its subdirectories in name order, as `os.walk`
/// visits them without following links. An unreadable directory fails the
/// check rather than hiding what it holds.
fn walk(
	root: &Path,
	here: &Path,
	manifests: &mut Vec<String>,
	locks: &mut Vec<String>,
) -> Result<()> {
	let mut folders = Vec::new();
	let mut files = Vec::new();
	for entry in std::fs::read_dir(here)? {
		let entry = entry?;
		let name = entry.file_name();
		let kind = entry.file_type()?;
		let is_dir = if kind.is_symlink() {
			std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir())
		} else {
			kind.is_dir()
		};
		if is_dir {
			let excluded = matches!(name.to_str(), Some(".git" | "target" | "__pycache__"))
				|| (here == root && matches!(name.to_str(), Some("probes" | "provenance")));
			if !excluded && !kind.is_symlink() {
				folders.push(name);
			}
		} else if matches!(name.to_str(), Some("Cargo.toml" | "Cargo.lock")) {
			files.push(name);
		}
	}
	files.sort();
	folders.sort();
	for name in files {
		let path = here.join(&name);
		let relative = path
			.strip_prefix(root)?
			.to_str()
			.context("non-UTF-8 manifest path")?
			.to_owned();
		if name == "Cargo.toml" {
			if !check_manifest(root, &path)?.is_empty() {
				manifests.push(relative);
			}
		} else if !check_lock(&path)?.is_empty() {
			locks.push(relative);
		}
	}
	for name in folders {
		walk(root, &here.join(name), manifests, locks)?;
	}
	Ok(())
}

/// Reject an incomplete source binding before any Cargo compilation.
pub fn validate_request(request: &Value) -> Result<()> {
	if request.get("rune_rev").is_none_or(Value::is_null) {
		let path = request
			.get("rune_path")
			.and_then(Value::as_str)
			.filter(|p| !p.is_empty());
		let Some(path) = path else {
			bail!(value_error("missing request rune_path"));
		};
		let root = request["rune_root"].as_str().context("missing rune_root")?;
		if std::fs::canonicalize(path)? != std::fs::canonicalize(root)? {
			bail!(value_error(
				"requested Rune path differs from selected source"
			));
		}
	}
	Ok(())
}

// --- The terminal receipt and a census failure's evidence (ADR 0292). ---

/// A failed launch: its error as Python's repr wrote it, and its notes.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
	pub repr: String,
	pub notes: Vec<String>,
}

/// `base/relative`, when every component is a plain entry and the last a
/// regular file.
fn regular_file(base: &Path, relative: &str) -> Option<PathBuf> {
	let mut path = base.to_path_buf();
	let mut last = None;
	for part in Path::new(relative).components() {
		let Component::Normal(part) = part else {
			return None;
		};
		path.push(part);
		let meta = path.symlink_metadata().ok()?;
		if meta.file_type().is_symlink() {
			return None;
		}
		last = Some(meta);
	}
	last.filter(|m| m.is_file()).map(|_| path)
}

/// At most `limit` bytes of `path`, or None when it holds more.
fn bounded_read(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
	let mut data = Vec::new();
	std::io::Read::read_to_end(
		&mut std::io::Read::take(File::open(path)?, limit + 1),
		&mut data,
	)?;
	Ok((data.len() as u64 <= limit).then_some(data))
}

/// `text` within `limit` UTF-8 bytes, cut at a character boundary and marked.
fn cut_bytes(text: &str, limit: usize) -> String {
	if text.len() <= limit {
		return text.to_owned();
	}
	let mut end = limit - 3;
	while !text.is_char_boundary(end) {
		end -= 1;
	}
	format!("{}...", &text[..end])
}

/// Python's `' '.join(text.split())`.
fn squeeze(text: &str) -> String {
	text.split(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
		.filter(|w| !w.is_empty())
		.collect::<Vec<_>>()
		.join(" ")
}

/// One line naming the differences, as the verifier's headline bounds it:
/// the total, the first four cases (each within 512 bytes), "(+K more)",
/// the whole within 4096 bytes, named cases dropped before the total.
pub fn census_headline(differences: &[(String, String)]) -> String {
	let mut named: Vec<String> = differences
		.iter()
		.take(4)
		.map(|(id, reason)| cut_bytes(&format!("{}: {}", squeeze(id), squeeze(reason)), 512))
		.collect();
	let head = format!("{HEADLINE} ({} cases): ", differences.len());
	loop {
		let left = differences.len() - named.len();
		let more = if left > 0 {
			format!(" (+{left} more)")
		} else {
			String::new()
		};
		let line = format!("{head}{}{more}", named.join("; "));
		if line.len() <= 4096 || named.is_empty() {
			return line;
		}
		named.pop();
	}
}

/// A census failure's concrete error, its retained evidence and the
/// evidence its runs never wrote, from the census summary alone; None when
/// there is no readable census difference, so any other failure keeps its
/// receipt as it was.
pub fn census_failure(output: &Path, plan: &Path) -> Option<Value> {
	let summary_name = "census/live-execution/RESULT.json";
	let summary = regular_file(output, summary_name)?;
	let data = bounded_read(&summary, 1 << 20).ok()??;
	let summary: Value = serde_json::from_slice(&data).ok()?;
	let differences = summary.as_object()?.get("differences")?.as_array()?;
	if differences.is_empty() {
		return None;
	}
	let plan = std::fs::read_to_string(plan).ok()?;
	let known: BTreeSet<&str> = plan
		.lines()
		.skip(1)
		.filter(|l| !l.is_empty())
		.map(|l| l.split('\t').next().unwrap_or_default())
		.collect();
	// Every row must name a known case and a reason, or the summary is malformed.
	let mut rows = Vec::new();
	for row in differences {
		let id = row.as_object()?.get("id")?.as_str()?;
		let reason = row.get("reason")?.as_str()?;
		if !known.contains(id) {
			return None;
		}
		rows.push((id.to_owned(), reason.to_owned()));
	}
	let mut files = vec![summary_name.to_owned()];
	let mut unavailable = Vec::new();
	for (case, _) in &rows {
		for (engine, name) in CENSUS_EVIDENCE {
			let relative = format!("census/live-execution/{case}/{engine}/{name}");
			if regular_file(output, &relative).is_none() {
				unavailable.push(format!(
					"census evidence unavailable: {case} {engine} {name} (the run did not write it)"
				));
			} else if files.len() < 16 {
				files.push(relative);
			}
		}
	}
	unavailable.truncate(16);
	Some(json!({"error": census_headline(&rows), "diagnostic_files": files, "causes": unavailable}))
}

/// The launcher's published receipt: the controller's result, the owned
/// build's cleanup, and on failure the launcher's error and, for a census
/// difference, its concrete cause and bounded evidence.
pub fn terminal_receipt(
	output: &Path,
	plan: &Path,
	failure: Option<&Failure>,
	cleanup: Value,
) -> Result<Value> {
	let path = output.join("CONTROLLER-RESULT.json");
	// A frozen controller result is read only as the regular file it must
	// be: never through a link, a directory or a pipe. Any other entry, or
	// unreadable or invalid contents, stays on disk as evidence; the
	// receipt is still the launcher's FAIL.
	let mut result = json!({"status": "FAIL"});
	match path.symlink_metadata() {
		Err(_) => {}
		Ok(meta) if !meta.is_file() => {
			result["controller_result_error"] = json!("not a regular file");
		}
		Ok(_) => match std::fs::read(&path) {
			Err(error) => result["controller_result_error"] = json!(error.to_string()),
			Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
				Ok(value @ Value::Object(_)) => result = value,
				Ok(_) => result["controller_result_error"] = json!("not a JSON object"),
				Err(error) => result["controller_result_error"] = json!(error.to_string()),
			},
		},
	}
	let object = result
		.as_object_mut()
		.context("the receipt is not an object")?;
	object.insert("cleanup".into(), cleanup);
	if let Some(failure) = failure {
		object.insert("status".into(), json!("FAIL"));
		object.insert("launcher_error".into(), json!(failure.repr));
		object.insert("launcher_notes".into(), json!(failure.notes));
		// A census difference names its cases and keeps their bounded
		// evidence, never at the cost of the original failure.
		if let Some(Value::Object(census)) = census_failure(output, plan) {
			object.extend(census);
		}
	}
	Ok(result)
}

// --- The launcher. ---

/// Everything the launcher asks of its host, so a control can answer.
pub trait Host {
	/// Python's `shutil.which` over the environment's PATH.
	fn which(&self, name: &str, env: &Environment) -> Option<String>;
	/// The CPUs in this process's affinity.
	fn cpus(&self) -> Result<usize>;
	/// Cargo's jobs, as the stages decide them.
	fn jobs(&self, env: &Environment) -> Result<usize>;
	/// The running launcher: its path and SHA-256, from the process itself.
	fn launcher(&self) -> Result<Value>;
	/// A query's standard output (`bwrap --version`).
	fn output(&mut self, command: &[String], env: &Environment) -> Result<String>;
	/// A query's exit code (`git check-ignore`).
	fn status(&mut self, command: &[String], env: &Environment) -> Result<i32>;
	/// A namespace or verifier child, its output in the launcher's new
	/// files; its exit code. Its whole process group is drained before
	/// this returns.
	fn run(
		&mut self,
		command: &[String],
		env: &Environment,
		stdout: (&Path, File),
		stderr: (&Path, File),
	) -> Result<i32>;
}

/// The live host: children run under the controller's guardian, which
/// drains each one's whole group, never only its leader.
pub struct Live {
	pub guardian: PathBuf,
	pub groups: PathBuf,
	pub cwd: PathBuf,
	/// Raised by SIGINT or SIGTERM in a standalone launch: the running
	/// child is stopped and drained, and no further child starts.
	pub stop: Option<&'static crate::guardian::StopSignal>,
}

static INTERRUPTED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static STOP: std::sync::OnceLock<crate::guardian::StopSignal> = std::sync::OnceLock::new();

extern "C" fn on_signal(signal: libc::c_int) {
	// Only async-signal-safe work: an atomic store and a pipe write.
	INTERRUPTED.store(signal, std::sync::atomic::Ordering::SeqCst);
	if let Some(stop) = STOP.get() {
		stop.raise();
	}
}

/// A standalone launch turns SIGINT and SIGTERM into a stop, so it drains
/// its child, releases its private target and publishes its FAIL receipt,
/// as the Python launcher did. Inside a final the controller owns
/// interruption: it drains the stage and its owner recovery releases the
/// assigned payload.
pub fn interruptible() -> Result<&'static crate::guardian::StopSignal> {
	if STOP.get().is_none() {
		let _ = STOP.set(crate::guardian::StopSignal::new()?);
	}
	for signal in [libc::SIGINT, libc::SIGTERM] {
		// SAFETY: installing a handler that only stores and writes to a pipe.
		unsafe {
			let mut action: libc::sigaction = std::mem::zeroed();
			action.sa_sigaction = on_signal as *const () as usize;
			action.sa_flags = libc::SA_RESTART;
			libc::sigaction(signal, &action, std::ptr::null_mut());
		}
	}
	STOP.get().context("stop signal")
}

pub fn interrupted() -> Result<()> {
	match INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
		0 => Ok(()),
		signal => Err(raise(
			"RuntimeError",
			&format!("producer interrupted by signal {signal}"),
		)),
	}
}

impl Host for Live {
	fn which(&self, name: &str, env: &Environment) -> Option<String> {
		let path = env.get("PATH").map_or("/bin:/usr/bin", String::as_str);
		path.split(':').find_map(|dir| {
			let candidate = Path::new(if dir.is_empty() { "." } else { dir }).join(name);
			let meta = std::fs::metadata(&candidate).ok()?;
			let c = std::ffi::CString::new(candidate.as_os_str().as_encoded_bytes()).ok()?;
			// SAFETY: access(2) on a NUL-terminated path.
			let runnable = unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0;
			(runnable && !meta.is_dir()).then(|| candidate.to_string_lossy().into_owned())
		})
	}

	fn cpus(&self) -> Result<usize> {
		// SAFETY: sched_getaffinity fills a zeroed set for this process.
		unsafe {
			let mut set: libc::cpu_set_t = std::mem::zeroed();
			if libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set) != 0 {
				return Err(std::io::Error::last_os_error().into());
			}
			Ok(libc::CPU_COUNT(&set) as usize)
		}
	}

	fn jobs(&self, env: &Environment) -> Result<usize> {
		crate::stage::build_jobs(env)
	}

	fn launcher(&self) -> Result<Value> {
		// The running file itself, through its open inode; the path handed
		// to the verifier must still hold those bytes.
		let running = digest(Path::new("/proc/self/exe"))?;
		let path = std::fs::canonicalize("/proc/self/exe")?;
		if digest(&path)? != running {
			bail!(value_error(
				"the launcher's path no longer holds the running executable"
			));
		}
		Ok(json!({"path": path.to_string_lossy(), "sha256": running}))
	}

	fn output(&mut self, command: &[String], env: &Environment) -> Result<String> {
		let out = std::process::Command::new(&command[0])
			.args(&command[1..])
			.env_clear()
			.envs(env)
			.stderr(std::process::Stdio::inherit())
			.output()
			.map_err(|e| crate::stage::spawn_error(&command[0], &e))?;
		if !out.status.success() {
			return Err(crate::stage::called_process_error(
				command,
				out.status.code().unwrap_or(-1),
			));
		}
		Ok(String::from_utf8(out.stdout)?)
	}

	fn status(&mut self, command: &[String], env: &Environment) -> Result<i32> {
		let status = std::process::Command::new(&command[0])
			.args(&command[1..])
			.env_clear()
			.envs(env)
			.status()
			.map_err(|e| crate::stage::spawn_error(&command[0], &e))?;
		Ok(status.code().unwrap_or(-1))
	}

	fn run(
		&mut self,
		command: &[String],
		env: &Environment,
		stdout: (&Path, File),
		stderr: (&Path, File),
	) -> Result<i32> {
		if self.stop.is_some() {
			interrupted()?;
		}
		let environment: Vec<(std::ffi::OsString, std::ffi::OsString)> =
			env.iter().map(|(k, v)| (k.into(), v.into())).collect();
		let guarded = crate::guardian::spawn(
			&self.guardian,
			&self.groups,
			command,
			&self.cwd,
			&environment,
			stdout.1,
			stderr.1,
		)?;
		let outcome = guarded.wait_until(None, self.stop)?;
		if self.stop.is_some() {
			interrupted()?;
		}
		if let Some(stopped) = outcome.stopped {
			bail!(raise("RuntimeError", &stopped));
		}
		Ok(outcome.status)
	}
}

/// What a launch came to.
#[derive(Debug)]
pub enum Launched {
	/// Refused before any output, as argparse refuses: exit 2.
	Refused(String),
	/// Ran to its receipt: whether that receipt passed, and the failure,
	/// if the launch failed.
	Ran {
		output: PathBuf,
		passed: bool,
		failure: Option<Failure>,
	},
}

struct Options {
	suite: String,
	rune_rev: Option<String>,
	rune_path: Option<PathBuf>,
	rune_repo: Option<PathBuf>,
	fixtures_root: Option<PathBuf>,
	out: Option<PathBuf>,
	jobs: Option<i64>,
	registration_fixture: Option<PathBuf>,
}

fn parse(args: &[String], cwd: &Path) -> std::result::Result<Options, String> {
	let mut o = Options {
		suite: "all".into(),
		rune_rev: None,
		rune_path: None,
		rune_repo: None,
		fixtures_root: None,
		out: None,
		jobs: None,
		registration_fixture: None,
	};
	let mut seen = BTreeSet::new();
	let mut i = 0;
	while i < args.len() {
		let (flag, inline) = match args[i].split_once('=') {
			Some((f, v)) if f.starts_with("--") => (f.to_owned(), Some(v.to_owned())),
			_ => (args[i].clone(), None),
		};
		let known = [
			"--suite",
			"--rune-rev",
			"--rune-path",
			"--rune-repo",
			"--fixtures-root",
			"--out",
			"--jobs",
			"--registration-fixture",
		];
		if !known.contains(&flag.as_str()) {
			return Err(format!("unrecognized arguments: {}", args[i]));
		}
		if !seen.insert(flag.clone()) {
			return Err(format!("argument {flag}: given twice"));
		}
		let value = match inline {
			Some(v) => v,
			None => {
				i += 1;
				args.get(i)
					.cloned()
					.ok_or_else(|| format!("argument {flag}: expected one argument"))?
			}
		};
		let path = || {
			let p = PathBuf::from(&value);
			if p.is_absolute() { p } else { cwd.join(p) }
		};
		match flag.as_str() {
			"--suite" => {
				if !matches!(value.as_str(), "census" | "enrollment" | "all") {
					return Err(format!(
						"argument --suite: invalid choice: '{value}' (choose from 'census', 'enrollment', 'all')"
					));
				}
				o.suite = value.clone();
			}
			"--rune-rev" => o.rune_rev = Some(value.clone()),
			"--rune-path" => o.rune_path = Some(path()),
			"--rune-repo" => o.rune_repo = Some(path()),
			"--fixtures-root" => o.fixtures_root = Some(path()),
			"--out" => o.out = Some(path()),
			"--jobs" => {
				o.jobs = Some(
					value
						.trim()
						.parse()
						.map_err(|_| format!("argument --jobs: invalid int value: '{value}'"))?,
				)
			}
			_ => o.registration_fixture = Some(path()),
		}
		i += 1;
	}
	if o.rune_rev.is_some() && o.rune_path.is_some() {
		return Err("argument --rune-path: not allowed with argument --rune-rev".into());
	}
	Ok(o)
}

/// Where an absolute path really lands, as Python's non-strict
/// `Path.resolve` finds it: every link followed wherever it stands, a `..`
/// after a missing component included, the missing rest kept as named.
pub fn resolve(path: &Path) -> Result<PathBuf> {
	fn parts(path: &Path, pending: &mut Vec<Option<std::ffi::OsString>>) {
		// Pushed in reverse, so the first component pops first; None is `..`.
		for part in path.components().rev() {
			match part {
				Component::Normal(name) => pending.push(Some(name.to_owned())),
				Component::ParentDir => pending.push(None),
				_ => {}
			}
		}
	}
	let mut pending = Vec::new();
	parts(path, &mut pending);
	let mut resolved = PathBuf::from("/");
	let mut links = 0;
	while let Some(part) = pending.pop() {
		let Some(name) = part else {
			resolved.pop();
			continue;
		};
		resolved.push(&name);
		if resolved
			.symlink_metadata()
			.is_ok_and(|m| m.file_type().is_symlink())
		{
			links += 1;
			if links > 40 {
				bail!(value_error(&format!(
					"too many levels of symbolic links: {}",
					path.display()
				)));
			}
			let target = std::fs::read_link(&resolved)?;
			resolved.pop();
			if target.is_absolute() {
				resolved = PathBuf::from("/");
			}
			parts(&target, &mut pending);
		}
	}
	Ok(resolved)
}

fn text(path: &Path) -> String {
	path.to_string_lossy().into_owned()
}

/// A failure as it stands while the launcher runs: the error and its notes.
struct Failed {
	error: anyhow::Error,
	notes: Vec<String>,
}

/// Python's repr of a raised error.
fn repr(error: &anyhow::Error) -> String {
	match error.downcast_ref::<Raised>() {
		Some(raised) => raised.repr.clone(),
		None => format!(
			"RuntimeError({})",
			python_repr(&Value::String(format!("{error:#}")))
		),
	}
}

fn create(path: &Path) -> Result<(&Path, File)> {
	let file = std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)?;
	Ok((path, file))
}

/// Run `command` with its output in `name.stdout`-style files under
/// `output`; nonzero is Python's `CalledProcessError`.
fn checked(
	host: &mut dyn Host,
	command: &[String],
	env: &Environment,
	logs: (&Path, &Path),
) -> Result<()> {
	let code = host.run(command, env, create(logs.0)?, create(logs.1)?)?;
	if code != 0 {
		bail!(crate::stage::called_process_error(command, code));
	}
	Ok(())
}

/// The launch, from parsed arguments to the published receipt. `root` is
/// the controller's checkout; `args` follow the stage mode.
pub fn launch(
	args: &[String],
	root: &Path,
	cwd: &Path,
	env: &Environment,
	host: &mut dyn Host,
) -> Result<Launched> {
	let o = match parse(args, cwd) {
		Ok(o) => o,
		Err(refusal) => return Ok(Launched::Refused(refusal)),
	};
	// Inside a final the output is assigned, checked before the launcher
	// touches the filesystem.
	if env.get("RNX_FINAL").is_some_and(|v| !v.is_empty()) {
		let assigned = env
			.get("RNX_FINAL_CROSS_ROOT")
			.filter(|v| !v.is_empty())
			.map(PathBuf::from);
		let output = o.out.as_deref().map(resolve).transpose()?;
		let fits = match (&assigned, &output) {
			(Some(assigned), Some(output)) => {
				output.parent() == Some(assigned.as_path())
					&& output.file_name().is_some_and(|n| n == "payload")
			}
			_ => false,
		};
		if !fits {
			return Ok(Launched::Refused(
				"final cross output was not assigned".into(),
			));
		}
	}
	if let Some(out) = &o.out {
		let mut full = vec!["cross".to_string()];
		full.extend(args.iter().cloned());
		crate::stage::bound(&full, out, env)?;
	}
	let cpus = host.cpus()?;
	let jobs = match o.jobs {
		Some(j) => j,
		None => host.jobs(env)?.min(cpus) as i64,
	};
	if host.which("bwrap", env).is_none() {
		return Ok(Launched::Refused(
			"bwrap (bubblewrap) is required; no host-filesystem fallback is available".into(),
		));
	}
	let Some(cargo_path) = host.which("cargo", env) else {
		return Ok(Launched::Refused(
			"cargo is required on PATH (the runner uses toolchain 1.98.1)".into(),
		));
	};
	let version = host
		.output(&["bwrap".to_string(), "--version".to_string()], env)?
		.trim()
		.to_owned();
	if !(1..=cpus as i64).contains(&jobs) {
		return Ok(Launched::Refused(format!(
			"--jobs must be between 1 and the {cpus} CPUs in the current affinity"
		)));
	}
	let pin_consistency = match check_tree(root) {
		Ok(p) => p,
		Err(error) => return Ok(Launched::Refused(message(&error))),
	};
	let registration = o
		.registration_fixture
		.as_ref()
		.map(std::fs::canonicalize)
		.transpose()?;
	let rnx = registration.clone().unwrap_or_else(|| root.to_path_buf());
	let fixtures_root = std::fs::canonicalize(
		o.fixtures_root
			.clone()
			.unwrap_or_else(|| rnx.join("tools/verify/fixtures")),
	)?;
	let rune_path = o
		.rune_path
		.as_ref()
		.map(std::fs::canonicalize)
		.transpose()?;
	let rune = std::fs::canonicalize(root.join("rune"))?;
	if o.rune_repo.is_some()
		|| o.rune_rev.is_some()
		|| rune_path.as_ref().is_some_and(|p| *p != rune)
	{
		return Ok(Launched::Refused(
			"historical engine overrides require a standalone historical host checkout; this verifier binds the canonical monorepo engine".into(),
		));
	}
	let out = o.out.clone().unwrap_or_else(|| {
		let nanos = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map_or(0, |d| d.as_nanos());
		rnx.join("target/rnx-verify")
			.join(format!("{}-{nanos}", std::process::id()))
	});
	let output = resolve(&out)?;
	if output.starts_with(&rnx) {
		let scratch = resolve(&rnx.join("target/rnx-verify"))?;
		if !output.starts_with(&scratch) {
			return Ok(Launched::Refused(
				"in-tree output must be under target/rnx-verify".into(),
			));
		}
		let relative = text(output.strip_prefix(&rnx)?);
		let ignored = host.status(
			&[
				"git".into(),
				"-C".into(),
				text(&rnx),
				"check-ignore".into(),
				"-q".into(),
				"--".into(),
				relative,
			],
			env,
		)?;
		if ignored != 0 {
			return Ok(Launched::Refused(
				"in-tree output must be ignored by Git".into(),
			));
		}
	}
	if output.starts_with(&rune) {
		return Ok(Launched::Refused(
			"--out must be outside the Rune source tree".into(),
		));
	}
	std::fs::create_dir_all(output.parent().context("--out has no parent")?)?;
	std::fs::create_dir(&output)?;
	// The directory made is the one checked, or nothing more is written.
	if std::fs::canonicalize(&output)? != output {
		bail!(value_error("the output moved while it was created"));
	}
	let mut pins = serde_json::to_vec_pretty(&pin_consistency)?;
	pins.push(b'\n');
	std::io::Write::write_all(&mut create(&output.join("PIN-CONSISTENCY.json"))?.1, &pins)?;
	let owned = crate::owned::Owned::new(&output, "target")?;
	let mut failed: Option<Failed> = None;
	if let Err(error) = run(
		&output,
		root,
		&rnx,
		&rune,
		&cargo_path,
		&version,
		jobs,
		&o,
		&fixtures_root,
		registration.as_deref(),
		env,
		host,
	) {
		failed = Some(error);
	}
	// Freeze the controller's result before publishing the launcher's
	// terminal cleanup result. No previously bound receipt is rewritten.
	if output.join("RESULT.json").symlink_metadata().is_ok()
		&& let Err(error) = std::fs::rename(
			output.join("RESULT.json"),
			output.join("CONTROLLER-RESULT.json"),
		) {
		match &mut failed {
			Some(f) => f.notes.push(format!(
				"freezing the controller's result failed: {}",
				repr(&error.into())
			)),
			None => {
				failed = Some(Failed {
					error: error.into(),
					notes: vec![],
				})
			}
		}
	}
	let cleanup = match owned.release(failed.as_ref().map(|f| &f.error), env) {
		Ok(row) => {
			if row["status"] != "PASS"
				&& let Some(f) = &mut failed
			{
				f.notes.push(format!(
					"build removal failed: {}",
					row["error"].as_str().unwrap_or_default()
				));
			}
			row
		}
		Err(error) => {
			let row = json!({"status": "FAIL", "error": format!("{error:#}")});
			match &mut failed {
				Some(f) => f
					.notes
					.push(format!("cleanup receipt failure: {}", repr(&error))),
				None => {
					failed = Some(Failed {
						error,
						notes: vec![],
					})
				}
			}
			row
		}
	};
	let failure = failed.map(|f| Failure {
		repr: repr(&f.error),
		notes: f.notes,
	});
	let result = terminal_receipt(&output, &root.join(CENSUS_PLAN), failure.as_ref(), cleanup)?;
	crate::producer::publish(&output.join("RESULT.json"), &result, env)?;
	let passed = failure.is_none() && result["status"] == "PASS";
	Ok(Launched::Ran {
		output,
		passed,
		failure,
	})
}

/// Inside the owned build: the private graph, the namespace's metadata
/// check, the verifier's build, its run and host check.
#[allow(clippy::too_many_arguments)]
fn run(
	output: &Path,
	root: &Path,
	rnx: &Path,
	rune: &Path,
	cargo_path: &str,
	version: &str,
	jobs: i64,
	o: &Options,
	fixtures_root: &Path,
	registration: Option<&Path>,
	env: &Environment,
	host: &mut dyn Host,
) -> std::result::Result<(), Failed> {
	let plain = |error: anyhow::Error| Failed {
		error,
		notes: vec![],
	};
	let manifest = output.join("Cargo.toml");
	let lock = output.join("Cargo.lock");
	let original_manifest = output.join("original-Cargo.toml");
	let original_lock = output.join("original-Cargo.lock");
	let prepare = || -> Result<(PathBuf, Environment, Vec<String>)> {
		std::fs::copy(rnx.join("Cargo.toml"), &original_manifest)?;
		std::fs::copy(rnx.join("Cargo.lock"), &original_lock)?;
		let private = std::fs::read_to_string(rnx.join("Cargo.toml"))?
			.replacen(
				"[package]\n",
				"[package]\nautolib = false\nautobins = false\nautotests = false\nautoexamples = false\nautobenches = false\n",
				1,
			)
			.replacen("[features]\n", "[features]\ncross-verification = []\n", 1)
			+ "\n[[test]]\nname = \"rnx\"\npath = \"src/lib.rs\"\nharness = false\nrequired-features = [\"cross-verification\"]\n";
		std::fs::write(&manifest, private)?;
		std::fs::copy(rnx.join("Cargo.lock"), &lock)?;
		let mut request = Map::new();
		for (key, value) in [
			("command", json!("cross")),
			("suite", json!(o.suite)),
			("rune_rev", Value::Null),
			("rune_path", json!(text(rune))),
			("rune_repo", Value::Null),
			("fixtures_root", json!(text(fixtures_root))),
			("out", json!(text(output))),
			("jobs", json!(jobs)),
			(
				"registration_fixture",
				registration.map_or(Value::Null, |p| json!(text(p))),
			),
			("rnx_root", json!(text(rnx))),
			("rune_root", json!(text(rune))),
			("controller_root", json!(text(root))),
			("bwrap_version", json!(version)),
			("original_manifest", json!(text(&original_manifest))),
			("original_lock", json!(text(&original_lock))),
			("rune_revision_resolved", Value::Null),
			("rune_archive", Value::Null),
			("launcher", host.launcher()?),
		] {
			request.insert(key.into(), value);
		}
		let request = Value::Object(request);
		validate_request(&request)?;
		let request_path = output.join("REQUEST.json");
		let mut bytes = serde_json::to_vec_pretty(&request)?;
		bytes.push(b'\n');
		std::fs::write(&request_path, bytes)?;
		let mut child_env = env.clone();
		for (key, value) in [
			("CARGO_TARGET_DIR", text(&output.join("target"))),
			("RNX_VERIFY_RUNE_ROOT", text(rune)),
			("CARGO_BUILD_JOBS", jobs.to_string()),
			("CARGO_INCREMENTAL", "0".into()),
			("CARGO_PROFILE_DEV_DEBUG", "2".into()),
			("CARGO_PROFILE_TEST_DEBUG", "2".into()),
			("CARGO_PROFILE_DEV_STRIP", "none".into()),
			("CARGO_PROFILE_TEST_STRIP", "none".into()),
			("RUSTFLAGS", "-Cdebuginfo=2 -Cstrip=none".into()),
		] {
			child_env.insert(key.into(), value);
		}
		child_env.remove("CARGO_ENCODED_RUSTFLAGS");
		let cargo_home = match env.get("CARGO_HOME").filter(|h| !h.is_empty()) {
			Some(home) => PathBuf::from(home),
			None => {
				PathBuf::from(env.get("HOME").context("no HOME for Cargo's home")?).join(".cargo")
			}
		};
		let cargo_home = std::fs::canonicalize(cargo_home)?;
		let mut namespace: Vec<String> = [
			"bwrap",
			"--die-with-parent",
			"--ro-bind",
			"/",
			"/",
			"--tmpfs",
			"/tmp",
		]
		.map(String::from)
		.into();
		if env.contains_key("RNX_FINAL") {
			// Only Cargo runs in this read-only namespace with a writable /tmp.
			// The verifier and host check below inherit the stage-owned directory.
			namespace.extend(["--setenv", "TMPDIR", "/tmp"].map(String::from));
		}
		for (kind, from, to) in [
			("--ro-bind", text(rnx), text(rnx)),
			("--bind", text(output), text(output)),
			("--ro-bind", text(rune), text(rune)),
			("--bind", text(&cargo_home), text(&cargo_home)),
			("--ro-bind", text(&manifest), text(&rnx.join("Cargo.toml"))),
			("--bind", text(&lock), text(&rnx.join("Cargo.lock"))),
		] {
			namespace.extend([kind.to_string(), from, to]);
		}
		namespace.extend(
			["--proc", "/proc", "--dev", "/dev", "--chdir"]
				.map(String::from)
				.into_iter()
				.chain([text(rnx), cargo_path.to_owned(), TOOLCHAIN.to_owned()]),
		);
		Ok((request_path, child_env, namespace))
	};
	let (request_path, child_env, namespace) = prepare().map_err(plain)?;
	let request_text = text(&request_path);
	let mut build = || -> Result<String> {
		// Resolve only the private lock; the verifier validates every package.
		let mut metadata = namespace.clone();
		metadata.extend(
			[
				"metadata",
				"--offline",
				"--format-version",
				"1",
				"--features",
				"cross-verification",
			]
			.map(String::from),
		);
		checked(
			host,
			&metadata,
			&child_env,
			(
				&output.join("metadata.json"),
				&output.join("metadata.stderr"),
			),
		)?;
		check_metadata(
			root,
			&serde_json::from_slice(&std::fs::read(output.join("metadata.json"))?)?,
		)?;
		let mut test = namespace.clone();
		test.extend(
			[
				"test",
				"--offline",
				"--locked",
				"--test",
				"rnx",
				"--features",
				"cross-verification",
				"--no-run",
				"--message-format=json",
			]
			.map(String::from),
		);
		checked(
			host,
			&test,
			&child_env,
			(&output.join("cargo.stdout"), &output.join("cargo.stderr")),
		)?;
		// A launch candidate only: the verifier validates the whole artifact
		// stream, uniqueness and source/binary identity before use.
		for line in std::fs::read_to_string(output.join("cargo.stdout"))?.lines() {
			let row: Value = serde_json::from_str(line)?;
			if row["reason"] == "compiler-artifact"
				&& row["target"]["name"] == "rnx"
				&& row["target"]["kind"] == json!(["test"])
				&& let Some(executable) = row["executable"].as_str().filter(|e| !e.is_empty())
			{
				return Ok(executable.to_owned());
			}
		}
		Err(raise(
			"RuntimeError",
			"Cargo's artifact stream names no rnx test executable",
		))
	};
	let executable = build().map_err(plain)?;
	let controller = checked(
		host,
		&[executable, "--controller".into(), request_text.clone()],
		&child_env,
		(
			&output.join("controller.stdout"),
			&output.join("controller.stderr"),
		),
	);
	// The host check runs whenever the verifier left a bound child, its
	// own failure included; the controller's failure stays primary.
	let binary = output.join("bound-verification-child");
	let host_check = if binary.exists() {
		checked(
			host,
			&[text(&binary), "--host-check".into(), request_text],
			&child_env,
			(
				&output.join("host-check.stdout"),
				&output.join("host-check.stderr"),
			),
		)
	} else {
		Ok(())
	};
	match (controller, host_check) {
		(Err(error), Ok(())) => return Err(plain(error)),
		(Err(error), Err(host_error)) => {
			return Err(Failed {
				error,
				notes: vec![format!("host check also failed: {}", repr(&host_error))],
			});
		}
		(Ok(()), Err(host_error)) => return Err(plain(host_error)),
		(Ok(()), Ok(())) => {}
	}
	if !binary.is_file() {
		return Err(plain(raise(
			"RuntimeError",
			"the Rust controller did not bind an executable; see controller.stderr",
		)));
	}
	// A pass is the controller's own PASS result, never its absence.
	let result = output.join("RESULT.json");
	match result.symlink_metadata() {
		Err(_) => {
			return Err(plain(raise(
				"RuntimeError",
				"the Rust controller wrote no result; see controller.stderr",
			)));
		}
		Ok(meta) if !meta.is_file() => {
			return Err(plain(raise(
				"RuntimeError",
				"the Rust controller's result is not a regular file; see CONTROLLER-RESULT.json",
			)));
		}
		Ok(_) => {}
	}
	let status = std::fs::read(&result)
		.ok()
		.and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
		.and_then(|value| value.get("status").cloned());
	if status != Some(json!("PASS")) {
		return Err(plain(raise(
			"RuntimeError",
			"the Rust controller's result is not a PASS; see CONTROLLER-RESULT.json",
		)));
	}
	Ok(())
}

/// `rnx-final stage cross ...`: launch, publish, and exit as the Python
/// launcher did: 2 for a refusal, 1 for a failure, 0 for a pass.
pub fn main(args: &[String], root: &Path, env: &Environment) -> Result<i32> {
	let cwd = std::env::current_dir()?;
	let groups = match env.get("RNX_FINAL_GROUPS").filter(|g| !g.is_empty()) {
		Some(g) => PathBuf::from(g),
		None => {
			let g = std::env::temp_dir().join(format!(
				"rnx-final-cross-groups-{}",
				crate::receipt::invocation_id()
			));
			std::fs::create_dir(&g)?;
			g
		}
	};
	let standalone = env.get(crate::child::CONTEXT).is_none_or(|c| c.is_empty());
	let mut live = Live {
		guardian: std::fs::canonicalize("/proc/self/exe")?,
		groups,
		cwd: cwd.clone(),
		stop: if standalone {
			Some(interruptible()?)
		} else {
			None
		},
	};
	let launched = launch(args, root, &cwd, env, &mut live);
	if env.get("RNX_FINAL_GROUPS").is_none_or(|g| g.is_empty()) {
		let _ = std::fs::remove_dir_all(&live.groups);
	}
	match launched? {
		Launched::Refused(refusal) => {
			eprintln!("{} stage cross: error: {refusal}", crate::stage::PROGRAM);
			Ok(2)
		}
		Launched::Ran {
			output,
			passed,
			failure,
		} => {
			let result = output.join("RESULT.json");
			if let Some(failure) = &failure {
				eprintln!("{}", failure.repr);
				for note in &failure.notes {
					eprintln!("{note}");
				}
			}
			// The exit agrees with the published receipt.
			println!(
				"{} {}",
				if passed { "PASS" } else { "FAIL" },
				result.display()
			);
			Ok(if passed { 0 } else { 1 })
		}
	}
}
