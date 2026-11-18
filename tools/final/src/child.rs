//! One stage child: run once under a guardian, and persist the parent's
//! validation. Exit zero alone never validates a producer; its own receipt,
//! bound to this invocation's context, must say PASS.
use crate::failure::{self, Environment, python_str};
use crate::guardian;
use crate::pyjson;
use crate::receipt::{digest, invocation_id, sync_directory};
use anyhow::{Context as _, Result};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub const CONTEXT: &str = "RNX_VALIDATION_CONTEXT";
const ENV_KEYS: [&str; 20] = [
	"RUSTFLAGS",
	"RUSTDOCFLAGS",
	"RUSTUP_TOOLCHAIN",
	"RUSTC",
	"RUSTDOC",
	"RUSTC_WRAPPER",
	"RUSTC_WORKSPACE_WRAPPER",
	"MIRIFLAGS",
	"GLIBC_TUNABLES",
	"MALLOC_CONF",
	"PYTHONHASHSEED",
	"PYTHONDONTWRITEBYTECODE",
	"CC",
	"CXX",
	"CFLAGS",
	"CXXFLAGS",
	"LDFLAGS",
	"LANG",
	"LC_ALL",
	"TZ",
];
const ENV_PREFIXES: [&str; 2] = ["CARGO_", "RNX_"];
const COLUMN_PRODUCERS: [&str; 19] = [
	"accept",
	"final",
	"verify",
	"clippy",
	"resume",
	"recover_cross",
	"chain",
	"borrowed_controls",
	"ir_pool",
	"old_path",
	"boolean_capture",
	"f64_plan_capture",
	"f64_plan_large",
	"f64_output_capture",
	"accept_remote",
	"cross_program_controls",
	"cross_program_gate",
	"miri_exact",
	"repl_session_controls",
];
const OTHER_PRODUCERS: [&str; 4] = [
	"rune/tools/xtask/column/ck1.py",
	"rune/tools/xtask/column/measure.py",
	"rune/tools/xtask/column/cli_capture.py",
	"rune/tools/xtask/supplemental/run.py",
];
const LEAVES: [&str; 6] = [
	"ci/preflight.py",
	"ci/disk_space.py",
	"ci/hygiene.py",
	"ci/commit_subjects.py",
	"rune/tools/xtask/column/records.py",
	"rune/tools/xtask/column/dependencies.py",
];
const RAW_LEAVES: [&str; 7] = [
	"cargo", "rustc", "git", "ssh", "rustup", "python3", "python",
];

/// An error with the Python exception class the receipts name.
#[derive(Debug)]
pub struct Raised {
	pub class: &'static str,
	pub repr: String,
	/// What Python's `str()` of the exception gives.
	pub message: String,
}

/// The message of a raised error, as Python's `str(error)`; any other
/// error in full.
pub fn message(error: &anyhow::Error) -> String {
	error
		.downcast_ref::<Raised>()
		.map_or_else(|| format!("{error:#}"), |r| r.message.clone())
}

impl std::fmt::Display for Raised {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.repr)
	}
}

impl std::error::Error for Raised {}

/// `ValueError('message')`, as Python's repr writes it.
pub fn value_error(message: &str) -> anyhow::Error {
	raise("ValueError", message)
}

pub fn raise(class: &'static str, message: &str) -> anyhow::Error {
	anyhow::Error::new(Raised {
		class,
		repr: format!(
			"{class}({})",
			failure::python_repr(&Value::String(message.into()))
		),
		message: message.into(),
	})
}

/// Every git command of the controller: no optional locks, so no status
/// refreshes the index under a stage that fingerprints `.git`.
pub fn git_command() -> std::process::Command {
	let mut command = std::process::Command::new("git");
	command.env("GIT_OPTIONAL_LOCKS", "0");
	command
}

fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
	let output = git_command()
		.arg("-C")
		.arg(root)
		.args(arguments)
		.stderr(std::process::Stdio::null())
		.output()?;
	anyhow::ensure!(output.status.success(), "git {arguments:?} failed");
	Ok(output.stdout)
}

/// The checkout's commit, or None in an uncommitted repository.
pub fn head(root: &Path) -> Option<String> {
	git(root, &["rev-parse", "--verify", "HEAD"])
		.ok()
		.map(|out| String::from_utf8_lossy(&out).trim().to_owned())
}

/// Every tracked and unignored file's content digest and executable bit,
/// and every link's text: the identity a producer binds to.
pub fn source(root: &Path) -> Result<String> {
	use std::os::unix::fs::PermissionsExt as _;
	let root = std::fs::canonicalize(root)?;
	let listing = git(
		&root,
		&[
			"ls-files",
			"-z",
			"--cached",
			"--others",
			"--exclude-standard",
		],
	)?;
	let mut rows = Map::new();
	for raw in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
		let name = std::str::from_utf8(raw).context("a checkout path is not UTF-8")?;
		let path = root.join(name);
		let meta = std::fs::symlink_metadata(&path);
		let value = match meta {
			Ok(meta) if meta.file_type().is_symlink() => {
				let target = std::fs::read_link(&path)?;
				json!(pyjson::identity(&json!([
					"symlink",
					target.to_str().context("a link target is not UTF-8")?
				]))?)
			}
			Ok(meta) if meta.is_file() => {
				json!([digest(&path)?, meta.permissions().mode() & 0o111 != 0])
			}
			_ => json!("deleted"),
		};
		rows.insert(name.to_owned(), value);
	}
	pyjson::identity(&Value::Object(rows))
}

/// The declared recipe inputs of an environment: names and one digest,
/// never the values.
pub fn environment(env: &Environment) -> Result<Value> {
	let declared: Map<String, Value> = env
		.iter()
		.filter(|(k, _)| {
			k.as_str() != CONTEXT
				&& (ENV_KEYS.contains(&k.as_str()) || ENV_PREFIXES.iter().any(|p| k.starts_with(p)))
		})
		.map(|(k, v)| (k.clone(), json!(v)))
		.collect();
	let mut keys: Vec<&String> = declared.keys().collect();
	keys.sort();
	Ok(json!({"keys": keys, "sha256": pyjson::identity(&Value::Object(declared.clone()))?}))
}

fn is_python(program: &str) -> bool {
	Path::new(program)
		.file_name()
		.is_some_and(|n| n.to_string_lossy().starts_with("python"))
}

fn script(command: &[String], cwd: &Path) -> Option<PathBuf> {
	if !is_python(&command[0]) {
		return None;
	}
	for part in &command[1..] {
		if part == "-m" {
			return None;
		}
		if !part.starts_with('-') {
			return Some(lexical_resolve(&cwd.join(part)));
		}
	}
	None
}

/// Python's `Path.resolve()` (non-strict).
fn lexical_resolve(path: &Path) -> PathBuf {
	if let Ok(real) = std::fs::canonicalize(path) {
		return real;
	}
	match (path.parent(), path.file_name()) {
		(Some(parent), Some(name)) => lexical_resolve(parent).join(name),
		_ => path.to_owned(),
	}
}

/// "producer" when the child writes its own bound receipt, "leaf" when its
/// exit and logs are the evidence; anything unregistered is refused.
pub fn contract(command: &[String], cwd: &Path, root: &Path) -> Result<&'static str> {
	// A Rust stage is a producer only as the logical program in one of its
	// modes; the runner gives it the controller's own executable.
	if command[0] == crate::stage::PROGRAM {
		return match (
			command.get(1).map(String::as_str),
			command.get(2).map(String::as_str),
		) {
			(
				Some("stage"),
				Some(
					"suite" | "clippy" | "cli" | "jupyter" | "inventories" | "cross"
					| "fixed-route",
				),
			) => Ok("producer"),
			_ => Err(value_error(&format!(
				"unregistered rnx-final child: {command:?}"
			))),
		};
	}
	if is_python(&command[0])
		&& let Some(i) = command.iter().position(|p| p == "-m")
		&& command.get(i + 1).map(String::as_str) != Some("unittest")
	{
		return Err(value_error("unregistered Python module child"));
	}
	if let Some(path) = script(command, cwd) {
		let root = std::fs::canonicalize(root)?;
		let Ok(relative) = path.strip_prefix(&root) else {
			return Err(value_error("unregistered external Python child"));
		};
		let name = relative.to_string_lossy();
		let column = name
			.strip_prefix("tools/column/")
			.and_then(|n| n.strip_suffix(".py"));
		if column.is_some_and(|n| COLUMN_PRODUCERS.contains(&n))
			|| OTHER_PRODUCERS.contains(&name.as_ref())
		{
			return Ok("producer");
		}
		if LEAVES.contains(&name.as_ref())
			|| path
				.file_name()
				.is_some_and(|n| n.to_string_lossy().starts_with("test_"))
		{
			return Ok("leaf");
		}
		return Err(value_error(&format!("unregistered Python child: {name}")));
	}
	let program = Path::new(&command[0])
		.file_name()
		.map(|n| n.to_string_lossy().into_owned())
		.unwrap_or_default();
	if RAW_LEAVES.contains(&program.as_str()) {
		return Ok("leaf");
	}
	Err(value_error(&format!(
		"unregistered raw child: {}",
		command[0]
	)))
}

static DISCOVERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(.+): test$").unwrap());
static EXECUTED: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"test result: ok\. (\d+) passed;").unwrap());

pub fn discovered(text: &str) -> Result<Vec<String>> {
	// Python's `.` excludes only \n; a \r stays in the name.
	let names: Vec<String> = DISCOVERED
		.captures_iter(text)
		.map(|c| c[1].to_owned())
		.collect();
	if names.is_empty() {
		return Err(value_error("empty test discovery"));
	}
	Ok(names)
}

pub fn executed(text: &str) -> Result<u64> {
	let counts: Vec<u64> = EXECUTED
		.captures_iter(text)
		.filter_map(|c| c[1].parse().ok())
		.collect();
	if counts.is_empty() || counts.iter().sum::<u64>() == 0 {
		return Err(value_error("empty executed suite"));
	}
	Ok(counts.iter().sum())
}

fn read_json(path: &Path) -> Result<Value> {
	let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
	serde_json::from_slice(&bytes).map_err(|e| raise("JSONDecodeError", &e.to_string()))
}

/// The producer's bound terminal envelope and its semantic receipt.
pub fn validate(directory: &Path, expected: &Value, exit_code: i32) -> Result<Value> {
	if exit_code != 0 {
		return Err(value_error("child exit is nonzero"));
	}
	let context: crate::types::ChildContext =
		crate::types::read(&directory.join("CONTEXT.json"))
			.map_err(|_| value_error("invocation context changed"))?;
	if serde_json::to_value(&context)? != *expected {
		return Err(value_error("invocation context changed"));
	}
	let binding: crate::types::ProducerBinding = crate::types::read(&directory.join("CHILD.json"))
		.map_err(|_| value_error("child receipt is not PASS"))?;
	let row = serde_json::to_value(&binding)?;
	if binding.schema != 1 || binding.status != crate::types::Status::Pass {
		return Err(value_error("child receipt is not PASS"));
	}
	for (key, value) in [
		("context", json!(pyjson::identity(expected)?)),
		("invocation", expected["invocation"].clone()),
		("source", expected["source"].clone()),
	] {
		if row.get(key) != Some(&value) {
			return Err(value_error(&format!("child binding differs: {key}")));
		}
	}
	let path = PathBuf::from(row["receipt"].as_str().unwrap_or_default());
	if path.parent().map(|p| p.to_string_lossy().into_owned())
		!= expected["result_directory"].as_str().map(str::to_owned)
	{
		return Err(value_error(
			"producer result is outside this fresh invocation",
		));
	}
	let value = read_json(&path)?;
	let key = path.to_string_lossy().into_owned();
	if let Some(prior_sha) = expected["existing_results"].get(&key) {
		if expected["resume"] != json!(true)
			|| value.get("validation_context") != Some(&json!(pyjson::identity(expected)?))
		{
			return Err(value_error("stale or unbound resumed producer result"));
		}
		let prior = directory.join(format!(
			"PRIOR-{}.json",
			prior_sha.as_str().unwrap_or_default()
		));
		if Some(digest(&prior)?.as_str()) != prior_sha.as_str()
			|| read_json(&prior)?.get("status") != Some(&json!("PASS"))
		{
			return Err(value_error("cannot reuse failed result"));
		}
	}
	if Some(digest(&path)?.as_str()) != row["sha256"].as_str()
		|| value.get("status") != Some(&json!("PASS"))
	{
		return Err(value_error("producer result missing, changed or failed"));
	}
	Ok(row)
}

/// What a stage child returned to the controller.
pub struct Done {
	pub returncode: i32,
	pub validation: Value,
	pub declared_output: Option<Value>,
}

pub struct Run<'a> {
	pub command: &'a [String],
	pub cwd: &'a Path,
	pub root: &'a Path,
	pub env: &'a Environment,
	/// The stage log: stdout and stderr together.
	pub log: &'a Path,
	pub timeout: Option<Duration>,
	pub directory: &'a Path,
	pub groups: &'a Path,
	pub guardian: &'a Path,
	pub after: Option<&'a (dyn Fn() -> Result<Option<Value>> + Sync)>,
	/// Lets the controller stop the stage from another thread.
	pub stop: Option<&'a Stopper>,
}

/// The guardians a stage is running, so a controller signal reaches them;
/// once stopped, the stage starts no further child.
pub struct Stopper {
	state: std::sync::Mutex<(bool, Vec<guardian::Process>)>,
	signal: guardian::StopSignal,
}

impl Default for Stopper {
	fn default() -> Self {
		Self {
			state: Default::default(),
			signal: guardian::StopSignal::new().expect("a stop pipe"),
		}
	}
}

impl Stopper {
	/// Stop the stage: its guardians are told now, and any wait on them
	/// starts its drain deadline now.
	pub fn stop(&self) {
		let mut state = self.state.lock().unwrap();
		state.0 = true;
		for process in &state.1 {
			let _ = guardian::signal(process, libc::SIGTERM);
		}
		self.signal.raise();
	}

	fn enter(&self, process: &guardian::Process) -> bool {
		let mut state = self.state.lock().unwrap();
		state.1.push(process.clone());
		!state.0
	}

	fn leave(&self, process: &guardian::Process) {
		self.state.lock().unwrap().1.retain(|p| p != process);
	}
}

fn class(error: &anyhow::Error) -> (&'static str, String) {
	match error.downcast_ref::<Raised>() {
		Some(raised) => (raised.class, raised.repr.clone()),
		None => {
			let message = format!("{error:#}");
			let class = if error.downcast_ref::<std::io::Error>().is_some() {
				"OSError"
			} else {
				"RuntimeError"
			};
			(
				class,
				format!("{class}({})", failure::python_repr(&Value::String(message))),
			)
		}
	}
}

fn run_guarded(run: &Run, command: &[String], env: &Environment, log: &Path) -> Result<i32> {
	let file = std::fs::OpenOptions::new().append(true).open(log)?;
	let environment: Vec<(OsString, OsString)> =
		env.iter().map(|(k, v)| (k.into(), v.into())).collect();
	if run.stop.is_some_and(|s| s.state.lock().unwrap().0) {
		return Err(raise("RuntimeError", "producer interrupted by signal 15"));
	}
	let guarded = guardian::spawn(
		run.guardian,
		run.groups,
		command,
		run.cwd,
		&environment,
		file.try_clone()?,
		file,
	)?;
	let process = guarded.guardian().clone();
	if let Some(stop) = run.stop
		&& !stop.enter(&process)
	{
		guarded.stop()?;
	}
	let outcome = guarded.wait_until(run.timeout, run.stop.map(|s| &s.signal));
	if let Some(stop) = run.stop {
		stop.leave(&process);
	}
	let outcome = outcome?;
	if run.stop.is_some_and(|s| s.state.lock().unwrap().0) {
		return Err(raise("RuntimeError", "producer interrupted by signal 15"));
	}
	if let Some(stopped) = outcome.stopped {
		if stopped.contains("timed out") {
			let timeout = run.timeout.map(|t| t.as_secs()).unwrap_or_default();
			let list = failure::python_repr(&json!(command));
			return Err(anyhow::Error::new(Raised {
				class: "TimeoutExpired",
				repr: format!("TimeoutExpired({list}, {timeout})"),
				message: format!("Command '{list}' timed out after {timeout} seconds"),
			}));
		}
		return Err(raise("RuntimeError", &stopped));
	}
	Ok(outcome.status)
}

/// Run a stage child once and write its validation receipt.
/// The executable `guardian` names, `/proc/self/exe` resolved to the file.
fn own_executable(guardian: &Path) -> Result<PathBuf> {
	Ok(std::fs::canonicalize(guardian)?)
}

pub fn run(run: Run) -> Result<Done> {
	let started = Instant::now();
	let cwd = std::fs::canonicalize(run.cwd)?;
	let root = std::fs::canonicalize(run.root)?;
	std::fs::create_dir_all(run.directory.parent().unwrap_or(Path::new("/")))?;
	std::fs::create_dir(run.directory).map_err(|e| {
		anyhow::Error::new(e).context(format!(
			"attempt directory exists: {}",
			run.directory.display()
		))
	})?;
	let directory = run.directory;
	let mut row = Map::new();
	row.insert("schema".into(), json!(1));
	row.insert("status".into(), json!("FAIL"));
	row.insert("command".into(), json!(run.command));
	row.insert("process_exit".into(), Value::Null);
	let mut returncode = 1;
	std::fs::OpenOptions::new()
		.create_new(true)
		.write(true)
		.open(run.log)?;
	// The controller's own executable, which a Rust stage runs as.
	let program = own_executable(run.guardian)?;
	let mut resolved = run.command.to_vec();
	if resolved[0] == crate::stage::PROGRAM {
		resolved[0] = program.to_string_lossy().into_owned();
	}
	let result = (|| -> Result<()> {
		let kind = contract(run.command, &cwd, &root)?;
		let mut clean = run.env.clone();
		clean.remove(CONTEXT);
		let mut ctx = json!({
			"schema": 1, "invocation": invocation_id(), "root": root.to_string_lossy(), "cwd": cwd.to_string_lossy(),
			"command": run.command, "candidate": head(&root), "source": source(&root)?, "environment": environment(&clean)?, "kind": kind,
		});
		if kind == "producer" {
			if run.command.iter().filter(|p| *p == "--out").count() != 1 {
				return Err(value_error("producer needs one explicit output directory"));
			}
			let out = &run.command[run.command.iter().position(|p| p == "--out").unwrap() + 1];
			let result_dir = lexical_resolve(&cwd.join(out));
			let mut existing = Map::new();
			if let Ok(entries) = std::fs::read_dir(&result_dir) {
				for entry in entries.flatten() {
					let p = entry.path();
					if p.extension().is_some_and(|e| e == "json") {
						existing.insert(
							lexical_resolve(&p).to_string_lossy().into_owned(),
							json!(digest(&p)?),
						);
					}
				}
			}
			ctx["result_directory"] = json!(result_dir.to_string_lossy());
			ctx["existing_results"] = Value::Object(existing);
			ctx["resume"] = json!(run.command.iter().any(|p| p == "--resume"));
		}
		if run.command[0] == crate::stage::PROGRAM {
			ctx["executable"] =
				json!({"path": program.to_string_lossy(), "sha256": digest(&program)?});
		}
		crate::types::write_as::<crate::types::ChildContext>(
			&directory.join("CONTEXT.json"),
			&ctx,
		)?;
		row.insert("context".into(), ctx.clone());
		if kind == "producer" {
			clean.insert(
				CONTEXT.into(),
				std::fs::canonicalize(directory.join("CONTEXT.json"))?
					.to_string_lossy()
					.into_owned(),
			);
		}
		let is_test = Path::new(&run.command[0])
			.file_name()
			.is_some_and(|n| n == "cargo")
			&& run.command.iter().any(|p| p == "test")
			&& !run.command.iter().any(|p| p == "--list");
		if is_test {
			let split = run.command.iter().position(|p| p == "--");
			let mut discovery: Vec<String> =
				run.command[..split.unwrap_or(run.command.len())].to_vec();
			let tail: Vec<String> = split
				.map(|i| run.command[i + 1..].to_vec())
				.unwrap_or_default();
			// Keep filters and the ignored policy; drop execution threads.
			discovery.push("--".into());
			discovery.extend(
				tail.into_iter()
					.filter(|v| !v.starts_with("--test-threads")),
			);
			discovery.push("--list".into());
			let discovery_log = directory.join("discovery.log");
			std::fs::OpenOptions::new()
				.create_new(true)
				.write(true)
				.open(&discovery_log)?;
			if run_guarded(&run, &discovery, &clean, &discovery_log)? != 0 {
				return Err(value_error("test discovery failed"));
			}
			row.insert(
				"discovery".into(),
				json!(discovered(&std::fs::read_to_string(&discovery_log)?)?),
			);
		}
		let code = run_guarded(&run, &resolved, &clean, run.log)?;
		row.insert("process_exit".into(), json!(code));
		returncode = code;
		if let Some(after) = run.after {
			row.insert("declared_output".into(), after()?.unwrap_or(Value::Null));
		}
		if code != 0 {
			return Err(value_error("child exit is nonzero"));
		}
		if is_test {
			row.insert(
				"test_count".into(),
				json!(executed(&String::from_utf8_lossy(&std::fs::read(
					run.log
				)?))?),
			);
		}
		if head(&root) != ctx["candidate"].as_str().map(str::to_owned)
			|| source(&root)? != ctx["source"]
		{
			return Err(value_error("source changed during child"));
		}
		if kind == "producer" {
			row.insert("producer".into(), validate(directory, &ctx, code)?);
		}
		row.insert("status".into(), json!("PASS"));
		Ok(())
	})();
	if let Err(error) = &result {
		let (class, repr) = class(error);
		row.insert("error".into(), json!(repr));
		row.insert("error_type".into(), json!(class));
		returncode = if returncode == 0 { 1 } else { returncode };
	}
	let mut artifacts = Map::new();
	std::fs::File::open(run.log)?.sync_all()?;
	artifacts.insert(
		std::fs::canonicalize(run.log)?
			.to_string_lossy()
			.into_owned(),
		json!(digest(run.log)?),
	);
	if let Some(Value::Object(output)) = row.get("declared_output") {
		let archive = PathBuf::from(output["archive"].as_str().unwrap_or_default());
		std::fs::File::open(&archive)?.sync_all()?;
		artifacts.insert(
			lexical_resolve(&archive).to_string_lossy().into_owned(),
			json!(digest(&archive)?),
		);
	}
	let discovery_log = directory.join("discovery.log");
	if discovery_log.exists() {
		artifacts.insert(
			std::fs::canonicalize(&discovery_log)?
				.to_string_lossy()
				.into_owned(),
			json!(digest(&discovery_log)?),
		);
	}
	let mut cleanup = Vec::new();
	let mut events: Vec<PathBuf> = std::fs::read_dir(directory)?
		.flatten()
		.map(|e| e.path())
		.filter(|p| {
			p.file_name().is_some_and(|n| {
				n.to_string_lossy().starts_with("CLEANUP-")
					&& n.to_string_lossy().ends_with(".json")
			})
		})
		.collect();
	events.sort();
	for p in events {
		let event = read_json(&p)?;
		let path = event["path"].as_str().unwrap_or_default();
		anyhow::ensure!(
			Some(digest(Path::new(path))?.as_str()) == event["sha256"].as_str(),
			"cleanup evidence changed"
		);
		artifacts.insert(
			std::fs::canonicalize(&p)?.to_string_lossy().into_owned(),
			json!(digest(&p)?),
		);
		artifacts.insert(path.to_owned(), event["sha256"].clone());
		if event["status"] != "PASS" {
			row.insert("status".into(), json!("FAIL"));
			row.insert("cleanup_error".into(), json!("owned build removal failed"));
			returncode = if returncode == 0 { 1 } else { returncode };
		}
		cleanup.push(event);
	}
	let cleanup_seconds: f64 = cleanup.iter().filter_map(|e| e["seconds"].as_f64()).sum();
	row.insert("seconds".into(), json!(started.elapsed().as_secs_f64()));
	row.insert(
		"cleanup_seconds".into(),
		if cleanup.is_empty() {
			json!(0)
		} else {
			json!(cleanup_seconds)
		},
	);
	row.insert("cleanup".into(), json!(cleanup));
	if row["status"] == "FAIL"
		&& let Err(problem) = retain_failure(&run, directory, &mut row, &mut artifacts)
	{
		row.insert("failure_retention_error".into(), json!(class(&problem).0));
	}
	row.insert("artifacts".into(), Value::Object(artifacts));
	crate::types::write_as::<crate::types::ChildResult>(
		&directory.join("RESULT.json"),
		&Value::Object(row.clone()),
	)?;
	let result_path = std::fs::canonicalize(directory.join("RESULT.json"))?;
	Ok(Done {
		returncode: if row["status"] == "PASS" {
			0
		} else {
			returncode.max(1)
		},
		validation: json!({"path": result_path.to_string_lossy(), "sha256": digest(&result_path)?}),
		declared_output: row.get("declared_output").filter(|v| !v.is_null()).cloned(),
	})
}

fn retain_failure(
	run: &Run,
	directory: &Path,
	row: &mut Map<String, Value>,
	artifacts: &mut Map<String, Value>,
) -> Result<()> {
	let absolute = |p: &Path| {
		if p.is_absolute() {
			p.to_owned()
		} else {
			std::env::current_dir().unwrap_or_default().join(p)
		}
	};
	let mut logs = vec![absolute(run.log)];
	if directory.join("discovery.log").exists() {
		logs.push(absolute(&directory.join("discovery.log")));
	}
	let mut roots: Vec<PathBuf> = vec![absolute(directory)];
	roots.extend(logs.iter().filter_map(|p| p.parent().map(Path::to_owned)));
	let mut semantic = None;
	if let Some(context) = row.get("context").and_then(Value::as_object)
		&& context.get("kind") == Some(&json!("producer"))
	{
		let result_dir = PathBuf::from(context["result_directory"].as_str().unwrap_or_default());
		roots.push(result_dir.clone());
		let bound = directory.join("CHILD.json");
		if bound.is_file() {
			let binding = read_json(&bound)?;
			let candidate = PathBuf::from(binding["receipt"].as_str().unwrap_or_default());
			if candidate.parent() != Some(result_dir.as_path())
				|| Some(digest(&candidate)?.as_str()) != binding["sha256"].as_str()
			{
				return Err(value_error("failed producer diagnostic binding differs"));
			}
			semantic = Some(candidate);
		}
	}
	let error = row
		.get("error")
		.map(python_str)
		.unwrap_or_else(|| "child failed".into());
	let name = directory
		.file_name()
		.map(|n| n.to_string_lossy().into_owned())
		.unwrap_or_default();
	let reference = failure::retain(
		directory,
		&name,
		semantic.as_deref(),
		&logs,
		&roots,
		run.env,
		&error,
	)?;
	row.insert("error".into(), reference["error"].clone());
	row.insert("failure".into(), reference.clone());
	let index = std::fs::canonicalize(reference["path"].as_str().unwrap_or_default())?;
	artifacts.insert(
		index.to_string_lossy().into_owned(),
		reference["sha256"].clone(),
	);
	for entry in std::fs::read_dir(directory.join("failure"))?.flatten() {
		let p = std::fs::canonicalize(entry.path())?;
		artifacts.insert(p.to_string_lossy().into_owned(), json!(digest(&p)?));
	}
	sync_directory(directory)
}

/// A stage's receipt and every artifact it names, unchanged and passing.
pub fn evidence(reference: &Value, expected_source: &Value) -> Result<Value> {
	use crate::types::{ChildResult, Kind, Status};
	let path = PathBuf::from(reference["path"].as_str().unwrap_or_default());
	if Some(digest(&path)?.as_str()) != reference["sha256"].as_str() {
		return Err(value_error("stage evidence changed"));
	}
	let row: ChildResult = crate::types::read(&path)?;
	let context = row
		.context
		.value()
		.ok_or_else(|| value_error("stage did not pass for expected source"))?;
	if row.status != Status::Pass
		|| row.process_exit != Some(0)
		|| json!(context.source) != *expected_source
	{
		return Err(value_error("stage did not pass for expected source"));
	}
	if context.kind == Kind::Producer {
		validate(
			path.parent().unwrap_or(Path::new("")),
			&serde_json::to_value(context)?,
			0,
		)?;
	}
	for (name, sha) in &row.artifacts {
		if digest(Path::new(name))? != *sha {
			return Err(value_error("stage artifact changed"));
		}
	}
	Ok(serde_json::to_value(row)?)
}

#[cfg(test)]
mod tests {
	/// Only git_command launches git, so every git call runs lock-free.
	#[test]
	fn every_git_command_goes_through_git_command() {
		let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
		for entry in std::fs::read_dir(source).unwrap().flatten() {
			let text = std::fs::read_to_string(entry.path()).unwrap();
			let launches = text.matches(concat!("Command::new(", "\"git\")")).count();
			let allowed = if entry.file_name() == "child.rs" {
				1
			} else {
				0
			};
			assert_eq!(launches, allowed, "{}", entry.path().display());
		}
	}
}
