//! `rune-suite`'s native contract cases (ADR 0289), as `native_contracts.py`
//! ran them: every case the native manifest declares, ignored ones
//! included, against one Rune test binary built from this checkout, each
//! with only its declared fixture environment, and the catalogue held to
//! the manifest by the finalizer's own check.
use crate::child::{self, value_error};
use crate::failure::{Environment, truthy};
use crate::owned::Owned;
use crate::receipt::{atomic_json, digest};
use crate::selection::{NATIVE_MANIFEST, catalogue_check, native_manifest};
use anyhow::{Context as _, Result, bail};
use job_budget::fifo::{Client, Slots};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// The memory a standalone run needs free before it builds.
const HEADROOM_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// How long a contract process may take to end after SIGTERM, past its
/// limit, before its group is killed.
const TERMINATION_GRACE: Duration = Duration::from_secs(5);
/// Fixture output folders every case's environment names.
const FOLDERS: [(&str, &str); 12] = [
	("GAP_OUT", "gap-out"),
	("C10_REPLAY_OUT", "c10-out"),
	("C11_REPLAY_OUT", "c11-out"),
	("C15B_STOCK_OUT", "c15b-out"),
	("C15C_STOCK_OUT", "c15c-out"),
	("C13_OUT", "c13-out"),
	("C15_OUT", "c15-out"),
	("C15B_OUT", "c15b-fresh-out"),
	("C15C_OUT", "c15c-fresh-out"),
	("C12_OUT", "c12-out"),
	("C14_OUT", "c14-out"),
	("C14B_OUT", "c14b-out"),
];

/// One contract process: its output files, environment (`None`: the
/// stage's own) and limit; `check` fails a nonzero exit.
#[derive(Clone, Debug)]
pub struct Call {
	pub command: Vec<String>,
	pub cwd: PathBuf,
	pub env: Option<Environment>,
	/// The limit as declared, fractions of a second included.
	pub timeout: Duration,
	pub check: bool,
	pub stdout: PathBuf,
	/// Its own file, or none for stderr into stdout's.
	pub stderr: Option<PathBuf>,
}

/// How contract processes run, so a control can trace them.
pub trait Process: Sync {
	/// Run `call` to its end, its output in new files; its exit code.
	fn run(&self, call: &Call) -> Result<i32>;
}

/// The processes run for real, each in its own process group, which is
/// drained before the call returns, as `run_process` drains it.
pub struct Live {
	pub env: Environment,
}

impl Process for Live {
	fn run(&self, call: &Call) -> Result<i32> {
		use std::os::unix::process::CommandExt as _;
		let create = |path: &Path| {
			std::fs::OpenOptions::new()
				.write(true)
				.create_new(true)
				.open(path)
				.with_context(|| format!("create {}", path.display()))
		};
		let stdout = create(&call.stdout)?;
		let stderr = match &call.stderr {
			Some(path) => create(path)?,
			None => stdout.try_clone()?,
		};
		let mut command = std::process::Command::new(&call.command[0]);
		command
			.args(&call.command[1..])
			.current_dir(&call.cwd)
			.env_clear()
			.envs(call.env.as_ref().unwrap_or(&self.env))
			.stdin(std::process::Stdio::null())
			.stdout(stdout)
			.stderr(stderr)
			.process_group(0);
		let code = wait_group(command, call.timeout, &call.command)?;
		if call.check && code != 0 {
			bail!(child::raise(
				"CalledProcessError",
				&format!(
					"Command '{}' returned non-zero exit status {code}.",
					crate::failure::python_repr(&json!(call.command))
				)
			));
		}
		Ok(code)
	}
}

/// Wait for a process group's leader, past `limit` terminating it and
/// then the group; every member of the group has exited before the
/// leader is reaped, so nothing it started outlives the call.
fn wait_group(mut command: std::process::Command, limit: Duration, argv: &[String]) -> Result<i32> {
	let mut leader = command.spawn().with_context(|| format!("spawn {argv:?}"))?;
	let group = leader.id() as i32;
	let pinned = crate::guardian::pidfd(group)?;
	let signal = |sig: i32| {
		// SAFETY: pidfd_send_signal with a valid pidfd and no siginfo.
		unsafe {
			libc::syscall(
				libc::SYS_pidfd_send_signal,
				std::os::fd::AsRawFd::as_raw_fd(&pinned),
				sig,
				std::ptr::null::<libc::siginfo_t>(),
				0,
			)
		};
	};
	let ended = crate::guardian::wait_exit(&pinned, Some(limit))?;
	if !ended {
		signal(libc::SIGTERM);
		crate::guardian::wait_exit(&pinned, Some(TERMINATION_GRACE))?;
	}
	// The unreaped leader keeps the group's id from reuse while it drains.
	drain_group(group)?;
	let status = leader.wait()?;
	if !ended {
		bail!(child::raise(
			"TimeoutExpired",
			&format!(
				"Command '{}' timed out after {} seconds",
				crate::failure::python_repr(&json!(argv)),
				limit.as_secs_f64()
			)
		));
	}
	Ok(status.code().unwrap_or(1))
}

/// Kill every member of a process group and wait for each to exit: a
/// kernel exit notification per member, never a sampling loop.
fn drain_group(group: i32) -> Result<()> {
	// SAFETY: the group was created by this process's own child.
	unsafe { libc::kill(-group, libc::SIGKILL) };
	let mut members = Vec::new();
	for entry in std::fs::read_dir("/proc")?.flatten() {
		let Some(pid) = entry
			.file_name()
			.to_str()
			.and_then(|n| n.parse::<i32>().ok())
		else {
			continue;
		};
		let Ok(fd) = crate::guardian::pidfd(pid) else {
			continue;
		};
		let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
			continue;
		};
		let fields: Vec<&str> = stat[stat.rfind(')').map_or(0, |i| i + 2)..]
			.split_whitespace()
			.collect();
		if fields.get(2).and_then(|g| g.parse::<i32>().ok()) == Some(group) {
			members.push(fd);
		}
	}
	for fd in &members {
		crate::guardian::wait_exit(fd, None)?;
	}
	Ok(())
}

/// Run `command` with stdout and stderr in the new file `log`, failing a
/// nonzero exit; the log's text.
fn checked(
	p: &dyn Process,
	command: Vec<String>,
	cwd: &Path,
	log: PathBuf,
	env: Option<Environment>,
	timeout: Duration,
) -> Result<String> {
	let call = Call {
		command,
		cwd: cwd.to_owned(),
		env,
		timeout,
		check: true,
		stdout: log.clone(),
		stderr: None,
	};
	p.run(&call)?;
	Ok(String::from_utf8_lossy(&std::fs::read(&log)?).into_owned())
}

static LISTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(.+): test$").unwrap());
static RESULT: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"(?m)^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;").unwrap()
});
static ROWS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^CONTRACT ROWS (\d+)$").unwrap());

/// Cargo's jobs for the build: inside a final its jobserver's width.
fn build_jobs(env: &Environment) -> Result<usize> {
	if let Some(width) = inherited(env) {
		return Ok(width);
	}
	job_budget::memory::build_jobs(&observe()?)
}

/// Concurrent single-threaded cases: half the CPUs leave room for their
/// children.
fn case_width(env: &Environment) -> Result<usize> {
	if let Some(width) = inherited(env) {
		return Ok((width / 2).max(1));
	}
	job_budget::memory::case_width(&observe()?)
}

fn inherited(env: &Environment) -> Option<usize> {
	env.get("MAKEFLAGS")
		.and_then(|f| job_budget::fifo::inherited_width(f))
}

fn observe() -> Result<job_budget::memory::Observation> {
	job_budget::memory::observe(&job_budget::memory::Sources::default())
}

/// Python's `Path.resolve()`: the canonical path, or the path itself where
/// it does not exist.
fn resolve(path: &Path) -> PathBuf {
	std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// The Rune compilation units the binary's DWARF names; each must have
/// been compiled from this checkout.
pub fn compilation_units(
	p: &dyn Process,
	binary: &Path,
	root: &Path,
	log: PathBuf,
) -> Result<Vec<Value>> {
	let text = checked(
		p,
		vec![
			"readelf".into(),
			"--debug-dump=info".into(),
			"--dwarf-depth=1".into(),
			binary.to_string_lossy().into_owned(),
		],
		root,
		log,
		None,
		Duration::from_secs(180),
	)?;
	let value = |line: &str| {
		line.rsplit(": ")
			.next()
			.unwrap_or_default()
			.trim()
			.to_owned()
	};
	let mut units = Vec::new();
	for block in text.split("Compilation Unit @ offset") {
		let names: Vec<&str> = block.lines().filter(|l| l.contains("DW_AT_name")).collect();
		if names.is_empty() || !names[0].contains("crates/rune/src/lib.rs") {
			continue;
		}
		let dirs: Vec<&str> = block
			.lines()
			.filter(|l| l.contains("DW_AT_comp_dir"))
			.collect();
		if dirs.len() != 1 {
			bail!(value_error("missing Rune compilation directory"));
		}
		let cwd = PathBuf::from(value(dirs[0]));
		let name = value(names[0]);
		let source = name.split("/@/").next().unwrap_or_default();
		if resolve(&cwd) != resolve(root)
			|| resolve(&cwd.join(source)) != resolve(&root.join("rune/crates/rune/src/lib.rs"))
		{
			bail!(value_error("Rune compilation source differs from checkout"));
		}
		units.push(json!({"name": name, "recorded_cwd": cwd.to_string_lossy()}));
	}
	if units.is_empty() {
		bail!(value_error("no Rune compilation units"));
	}
	Ok(units)
}

/// Build the one Rune test binary, copy it out of its owned target, and
/// release the target before any case runs; the binary and its receipt.
pub fn build(
	root: &Path,
	bulk: &Path,
	m: &Value,
	env: &Environment,
	p: &dyn Process,
) -> Result<(PathBuf, PathBuf)> {
	let features: Vec<&str> = m["libraries"]
		.as_array()
		.into_iter()
		.flatten()
		.filter(|l| {
			let set: Vec<&str> = l["features"]
				.as_str()
				.unwrap_or_default()
				.split(',')
				.collect();
			set.contains(&"execution") && set.contains(&"serde") && !truthy(&l["selector"])
		})
		.map(|l| l["features"].as_str().unwrap_or_default())
		.collect();
	if features.len() != 1 {
		bail!(value_error(
			"native contracts require one declared execution-serde library recipe"
		));
	}
	let mut owner = Owned::new(bulk, "build-target")?;
	let built = (|| -> Result<(PathBuf, PathBuf)> {
		let mut cargo_env = env.clone();
		cargo_env.insert(
			"CARGO_TARGET_DIR".into(),
			owner.path().to_string_lossy().into_owned(),
		);
		cargo_env.insert("CARGO_BUILD_JOBS".into(), build_jobs(env)?.to_string());
		let command: Vec<String> = [
			"cargo",
			"+1.98.1",
			"test",
			"--locked",
			"-p",
			"rune",
			"--profile",
			"engine-dev",
			"--lib",
			"--features",
			features[0],
			"--no-run",
			"--message-format=json",
		]
		.iter()
		.map(|s| s.to_string())
		.collect();
		let text = checked(
			p,
			command.clone(),
			root,
			bulk.join("build.log"),
			Some(cargo_env),
			Duration::from_secs(3600),
		)?;
		let mut found = Vec::new();
		for line in text.lines().filter(|l| l.starts_with('{')) {
			let r: Value = serde_json::from_str(line)
				.map_err(|e| child::raise("JSONDecodeError", &e.to_string()))?;
			if r["reason"] == "compiler-artifact"
				&& r["target"]["name"] == "rune"
				&& truthy(&r["profile"]["test"])
				&& truthy(&r["executable"])
			{
				found.push(PathBuf::from(r["executable"].as_str().unwrap_or_default()));
			}
		}
		if found.len() != 1 {
			bail!(value_error("ambiguous Rune test binary"));
		}
		let binary = bulk.join("rune-tests");
		std::fs::copy(&found[0], &binary)?;
		let units = compilation_units(p, &binary, root, bulk.join("dwarf.log"))?;
		let receipt = bulk.join("BINARY.json");
		atomic_json(
			&receipt,
			&json!({"status": "PASS", "expected_repo": root.to_string_lossy(), "binary_sha256": digest(&binary)?,
				"source": child::source(root)?, "candidate": child::head(root), "command": command, "units": units,
				"dwarf_sha256": digest(&bulk.join("dwarf.log"))?}),
		)?;
		owner.keep(&[&binary, &receipt]);
		Ok((binary, receipt))
	})();
	// The build has ended (its process group drained), so its target goes now.
	let released = owner.release(built.as_ref().err(), env);
	let built = built?;
	released?;
	Ok(built)
}

/// The JSON Python's `json.dumps(rows, ensure_ascii=False)` writes for the
/// row differences appended to a case's error.
fn differences_text(rows: &[Value]) -> String {
	let items: Vec<String> = rows
		.iter()
		.map(|r| {
			format!(
				"{{\"row\": {}, \"difference\": {}}}",
				serde_json::to_string(&r["row"]).unwrap_or_default(),
				serde_json::to_string(&r["difference"]).unwrap_or_default()
			)
		})
		.collect();
	format!("[{}]", items.join(", "))
}

/// Run one manifest case; its row, a failure included. Only a case
/// directory that cannot be created is an error.
pub fn run_case(
	c: &Value,
	binary: &Path,
	receipt: &Path,
	root: &Path,
	bulk: &Path,
	env: &Environment,
	p: &dyn Process,
) -> Result<Value> {
	let name = c["name"].as_str().unwrap_or_default();
	let d = bulk.join(name);
	std::fs::create_dir(&d).with_context(|| format!("create {}", d.display()))?;
	let mut r = Map::new();
	r.insert("name".into(), c["name"].clone());
	r.insert("selector".into(), c["selector"].clone());
	r.insert("specification".into(), json!(crate::pyjson::identity(c)?));
	r.insert("status".into(), json!("FAIL"));
	let started = Instant::now();
	let outcome = (|| -> Result<()> {
		let recorded: Value = serde_json::from_slice(&std::fs::read(receipt)?)?;
		if json!(digest(binary)?) != recorded["binary_sha256"] {
			bail!(value_error("test binary changed"));
		}
		let selector = c["selector"].as_str().unwrap_or_default().to_owned();
		let mut args = vec![binary.to_string_lossy().into_owned(), selector.clone()];
		if truthy(&c["exact"]) {
			args.push("--exact".into());
		}
		if truthy(&c["ignored"]) {
			args.push("--ignored".into());
		}
		let mut list = args.clone();
		list.push("--list".into());
		let text = checked(
			p,
			list,
			&d,
			d.join("discovery.log"),
			None,
			Duration::from_secs(60),
		)?;
		let selected: Vec<String> = LISTED
			.captures_iter(&text)
			.map(|m| m[1].to_owned())
			.collect();
		r.insert("selected".into(), json!(selected));
		if selected.is_empty() || truthy(&c["exact"]) && selected != [selector.clone()] {
			bail!(value_error("selector discovery differs"));
		}
		// Only declared fixture inputs enter a test process, never the
		// stage's environment.
		let mut case_env: Environment = [
			("PATH".to_string(), "/usr/bin:/bin".to_string()),
			("LANG".to_string(), "C.UTF-8".to_string()),
		]
		.into();
		for (key, folder) in FOLDERS {
			case_env.insert(key.into(), d.join(folder).to_string_lossy().into_owned());
		}
		for (key, value) in c["env"].as_object().into_iter().flatten() {
			case_env.insert(key.clone(), value.as_str().unwrap_or_default().into());
		}
		case_env.insert(
			"C15C_NATIVE_RECEIPT".into(),
			receipt.to_string_lossy().into_owned(),
		);
		case_env.insert("C15C_NATIVE_RECEIPT_SHA256".into(), digest(receipt)?);
		case_env.insert(
			"C15C_NATIVE_SOURCE".into(),
			root.to_string_lossy().into_owned(),
		);
		case_env.insert(
			"RNX_CONTRACT_DIFFS".into(),
			d.join("DIFFERENCES.json").to_string_lossy().into_owned(),
		);
		if let Some(groups) = env.get("RNX_FINAL_GROUPS").filter(|g| !g.is_empty()) {
			case_env.insert("RNX_FINAL_GROUPS".into(), groups.clone());
		}
		let mut command = args;
		command.extend(["--nocapture".into(), "--test-threads=1".into()]);
		let mut cwd = d.clone();
		if truthy(&c["sandbox"]) {
			let bulk_text = bulk.to_string_lossy().into_owned();
			let mut wrapped: Vec<String> = [
				"bwrap",
				"--ro-bind",
				"/",
				"/",
				"--bind",
				&bulk_text,
				&bulk_text,
				"--tmpfs",
				"/tmp",
				"--dev-bind",
				"/dev",
				"/dev",
				"--proc",
				"/proc",
				"--",
			]
			.iter()
			.map(|s| s.to_string())
			.collect();
			wrapped.extend(command);
			command = wrapped;
			cwd = PathBuf::from("/tmp");
		}
		// The manifest's positive limit, fractions included, as Python passed it.
		let timeout = c["timeout"]
			.as_f64()
			.and_then(|t| Duration::try_from_secs_f64(t).ok())
			.filter(|t| !t.is_zero())
			.ok_or_else(|| value_error("invalid manifest case"))?;
		let code = p.run(&Call {
			command,
			cwd,
			env: Some(case_env),
			timeout,
			check: false,
			stdout: d.join("stdout.log"),
			stderr: Some(d.join("stderr.log")),
		})?;
		r.insert("exit".into(), json!(code));
		let text = String::from_utf8_lossy(&std::fs::read(d.join("stdout.log"))?).into_owned();
		let counts: Vec<(u64, u64)> = RESULT
			.captures_iter(&text)
			.map(|m| {
				(
					m[1].parse().unwrap_or(u64::MAX),
					m[2].parse().unwrap_or(u64::MAX),
				)
			})
			.collect();
		if counts.len() != 1 || counts[0] != (selected.len() as u64, 0) || code != 0 {
			bail!(value_error(&format!(
				"native contract execution failed: {name}"
			)));
		}
		r.insert("passed".into(), json!(counts[0].0));
		let markers: Vec<String> = ROWS.captures_iter(&text).map(|m| m[1].to_owned()).collect();
		let rows = c.get("rows").filter(|r| !r.is_null());
		if let Some(rows) = rows {
			if markers != [rows.to_string()] {
				bail!(value_error("contract row total differs"));
			}
			r.insert("contract_rows".into(), json!(markers[0].parse::<u64>()?));
		}
		if truthy(&c["empty_stderr"]) && std::fs::metadata(d.join("stderr.log"))?.len() > 0 {
			bail!(value_error("nonempty contract stderr"));
		}
		let diffs = d.join("DIFFERENCES.json");
		if diffs.exists() {
			let differences: Value = serde_json::from_slice(&std::fs::read(&diffs)?)?;
			// Present, then empty: an omitted list is no evidence of none.
			let listed = differences
				.get("differences")
				.ok_or_else(|| child::raise("KeyError", "differences"))?;
			if truthy(listed) {
				bail!(value_error("nonempty row differences"));
			}
			let checked = differences
				.get("rows_checked")
				.cloned()
				.ok_or_else(|| child::raise("KeyError", "rows_checked"))?;
			r.insert("rows_checked".into(), checked);
			if rows.is_some_and(|rows| differences["rows_checked"] != *rows) {
				bail!(value_error("compared row total differs"));
			}
		}
		r.insert("status".into(), json!("PASS"));
		Ok(())
	})();
	if let Err(error) = outcome {
		let mut text = format!("{error:#}");
		let diffs = d.join("DIFFERENCES.json");
		// The original failure stays the verdict if the differences are unreadable.
		if diffs.is_file()
			&& let Ok(value) = serde_json::from_slice::<Value>(&std::fs::read(&diffs)?)
			&& let Some(rows) = value["differences"].as_array()
			&& rows
				.iter()
				.all(|row| row.get("row").is_some() && row.get("difference").is_some())
		{
			let kept: Vec<Value> = rows
				.iter()
				.map(|row| json!({"row": row["row"], "difference": row["difference"]}))
				.collect();
			text.push('\n');
			text.push_str(&differences_text(&kept));
			r.insert("row_differences".into(), json!(kept));
		}
		r.insert("error".into(), json!(text));
	}
	r.insert("seconds".into(), json!(started.elapsed().as_secs_f64()));
	let mut artifacts = Map::new();
	for entry in std::fs::read_dir(&d)?.flatten() {
		if entry.file_type()?.is_file() {
			artifacts.insert(
				entry.file_name().to_string_lossy().into_owned(),
				json!(digest(&entry.path())?),
			);
		}
	}
	r.insert("artifacts".into(), Value::Object(artifacts));
	let r = Value::Object(r);
	atomic_json(&d.join("RESULT.json"), &r)?;
	Ok(r)
}

/// Run every case, in the manifest's order: each exclusive one alone,
/// holding every jobserver token its stage does not; then the rest
/// concurrently, at most the case width, each on one of the stage's own
/// tokens or one from the pool. Every case runs whatever another returns.
pub fn run_cases(
	cases: &[Value],
	case: &(dyn Fn(&Value) -> Result<Value> + Sync),
	env: &Environment,
) -> Result<Vec<Value>> {
	let client = Client::from_environment(
		env.get("MAKEFLAGS").map(String::as_str),
		env.get("RNX_FINAL_STAGE_TOKENS").map(String::as_str),
	)?;
	let mut rows: Vec<Option<Value>> = vec![None; cases.len()];
	for (i, c) in cases
		.iter()
		.enumerate()
		.filter(|(_, c)| truthy(&c["exclusive"]))
	{
		let count = client.as_ref().map_or(0, |k| k.capacity() - k.held());
		let permit = match &client {
			Some(k) if count > 0 => Some(k.acquire(count)?),
			_ => None,
		};
		rows[i] = Some(case(c)?);
		if let Some(permit) = permit {
			permit.release()?;
		}
	}
	let shared: Vec<usize> = (0..cases.len())
		.filter(|i| !truthy(&cases[*i]["exclusive"]))
		.collect();
	if !shared.is_empty() {
		let width = case_width(env)?.min(shared.len());
		let slots = match &client {
			Some(k) => Some(Slots::new(k.held())?),
			None => None,
		};
		let next = AtomicUsize::new(0);
		let done: Mutex<Vec<(usize, Result<Value>)>> = Mutex::new(Vec::new());
		std::thread::scope(|s| {
			for _ in 0..width {
				s.spawn(|| {
					loop {
						let n = next.fetch_add(1, Ordering::SeqCst);
						let Some(&i) = shared.get(n) else {
							return;
						};
						let row = (|| -> Result<Value> {
							let permit = match (&client, &slots) {
								(Some(k), Some(slots)) => Some(k.acquire_one(slots)?),
								_ => None,
							};
							let row = case(&cases[i]);
							if let Some(permit) = permit {
								permit.release()?;
							}
							row
						})();
						done.lock().unwrap().push((i, row));
					}
				});
			}
		});
		for (i, row) in done.into_inner().unwrap() {
			rows[i] = Some(row?);
		}
	}
	rows.into_iter()
		.map(|r| r.context("a case did not run"))
		.collect()
}

/// Build the binary, run every case and hold them to the manifest;
/// `CONTRACTS.json` in `out`, and its value.
pub fn run(root: &Path, out: &Path, env: &Environment, p: &dyn Process) -> Result<Value> {
	let root = std::fs::canonicalize(root)?;
	let bulk = crate::producer::assigned_root(
		"RNX_FINAL_CONTRACT_ROOT",
		&out.join("contract-payload"),
		env,
	)?;
	match std::fs::create_dir(&bulk) {
		Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e.into()),
		_ => {}
	}
	let m = native_manifest(&root)?;
	let source = child::source(&root)?;
	let started = Instant::now();
	// Inside a final the jobserver's tokens bound memory; a live reading
	// would count the neighbors' builds those tokens already admit.
	if inherited(env).is_none() {
		let o = observe()?;
		let headroom = match o.ancestor_headroom_bytes {
			0 => u64::MAX,
			h => h,
		};
		if o.mem_available_bytes.min(headroom) < HEADROOM_BYTES {
			bail!(value_error(
				"native contract admission needs 4 GiB memory headroom"
			));
		}
	}
	let (binary, receipt) = build(&root, &bulk, &m, env, p)?;
	let cases: Vec<Value> = m["cases"].as_array().cloned().unwrap_or_default();
	let rows = run_cases(
		&cases,
		&|c| run_case(c, &binary, &receipt, &root, &bulk, env, p),
		env,
	)?;
	if child::source(&root)? != source {
		bail!(value_error("source changed during native contracts"));
	}
	let mut r = json!({"status": "FAIL", "manifest_sha256": digest(&root.join(NATIVE_MANIFEST))?, "source": source,
		"binary_sha256": digest(&binary)?, "cases": rows, "seconds": started.elapsed().as_secs_f64()});
	match catalogue_check(
		&m,
		r["cases"].as_array().map(Vec::as_slice).unwrap_or_default(),
	) {
		Ok(catalogue) => {
			r["catalogue"] = catalogue;
			r["status"] = json!("PASS");
		}
		Err(error) => r["error"] = json!(child::message(&error)),
	}
	atomic_json(&out.join("CONTRACTS.json"), &r)?;
	Ok(r)
}
