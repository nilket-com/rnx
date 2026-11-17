//! The stage programs the controller runs as its own executable:
//! `rnx-final stage suite --stage NAME --out DIR` (the package, format,
//! lint, suite and Clippy stages `suite_stage.py` ran, all but
//! `rune-suite`) and `rnx-final stage clippy [--adapter] --out DIR` (the
//! comparison `clippy.py` ran). Each publishes its receipt through the
//! producer protocol, inside the final's guardian, jobserver and limits.
use crate::catalogue::{self, Command};
use crate::child::{discovered, executed, value_error};
use crate::failure::Environment;
use crate::producer::{assigned_root, publish};
use crate::receipt::digest;
use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The logical program a stage command names; the controller runs it as
/// its own executable.
pub const PROGRAM: &str = "rnx-final";
/// The published commit `clippy` and `adapter-clippy` compare against.
pub const CLIPPY_BASE: &str = "0873610926649d8c79094c1aa346fa6728c5251a";
/// Kept in the Polars Clippy comparison: the existing preview test
/// diagnostic is counted at warning level, not denied.
pub const POLARS_LINT_ARGS: [&str; 2] = ["--", "-Wclippy::erasing_op"];

/// How a stage runs its children, so a control can trace them.
pub trait Invoke {
	/// Run `command` in `cwd` with stdout and stderr in the new file `log`,
	/// failing on a nonzero exit; the log's text.
	fn invoke(
		&mut self,
		command: &[String],
		cwd: &Path,
		log: &Path,
		env: &[(String, String)],
	) -> Result<String>;
	/// Write `git archive COMMIT` of `root` into the new file `archive`.
	fn archive(&mut self, root: &Path, commit: &str, archive: &Path) -> Result<()>;
	/// Run a Clippy comparison's Cargo, stdout and stderr in their own new
	/// files, with `CARGO_TARGET_DIR` and `CARGO_BUILD_JOBS`; its exit code.
	fn clippy(
		&mut self,
		command: &[String],
		cwd: &Path,
		stdout: &Path,
		stderr: &Path,
		target: &Path,
		jobs: usize,
	) -> Result<i32>;
}

/// The children run for real, each bounded as the Python stages bound
/// them.
pub struct Live {
	pub env: Environment,
}

impl Invoke for Live {
	fn invoke(
		&mut self,
		command: &[String],
		cwd: &Path,
		log: &Path,
		env: &[(String, String)],
	) -> Result<String> {
		let file = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(log)?;
		let mut child = std::process::Command::new(&command[0]);
		child
			.args(&command[1..])
			.current_dir(cwd)
			.env_clear()
			.envs(&self.env)
			.envs(env.iter().map(|(k, v)| (k, v)))
			.stdout(file.try_clone()?)
			.stderr(file);
		let code = wait(child, Duration::from_secs(10800))?;
		if code != 0 {
			bail!(value_error(&format!(
				"Command {command:?} returned non-zero exit status {code}."
			)));
		}
		Ok(String::from_utf8_lossy(&std::fs::read(log)?).into_owned())
	}

	fn archive(&mut self, root: &Path, commit: &str, archive: &Path) -> Result<()> {
		let file = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(archive)?;
		let mut child = crate::child::git_command();
		child
			.arg("archive")
			.arg(commit)
			.current_dir(root)
			.stdout(file);
		let code = wait(child, Duration::from_secs(3600))?;
		if code != 0 {
			bail!(value_error(&format!("git archive {commit} failed: {code}")));
		}
		Ok(())
	}

	fn clippy(
		&mut self,
		command: &[String],
		cwd: &Path,
		stdout: &Path,
		stderr: &Path,
		target: &Path,
		jobs: usize,
	) -> Result<i32> {
		let out = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(stdout)?;
		let err = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(stderr)?;
		let mut child = std::process::Command::new(&command[0]);
		child
			.args(&command[1..])
			.current_dir(cwd)
			.env_clear()
			.envs(&self.env)
			.env("CARGO_TARGET_DIR", target)
			.env("CARGO_BUILD_JOBS", jobs.to_string())
			.stdout(out)
			.stderr(err);
		wait(child, Duration::from_secs(7200))
	}
}

/// Wait for a child, killing it past `limit`; its exit code. The child is
/// pinned by a pidfd opened before anything can reap it, waited on through
/// that descriptor, signalled only through it, and reaped last.
pub fn wait(mut command: std::process::Command, limit: Duration) -> Result<i32> {
	let mut child = command
		.spawn()
		.with_context(|| format!("spawn {command:?}"))?;
	let pinned = crate::guardian::pidfd(child.id() as i32)?;
	if !crate::guardian::wait_exit(&pinned, Some(limit))? {
		// SAFETY: pidfd_send_signal with a valid pidfd and no siginfo.
		unsafe {
			libc::syscall(
				libc::SYS_pidfd_send_signal,
				std::os::fd::AsRawFd::as_raw_fd(&pinned),
				libc::SIGKILL,
				std::ptr::null::<libc::siginfo_t>(),
				0,
			)
		};
		child.wait()?;
		bail!(value_error(&format!(
			"child timed out after {}s",
			limit.as_secs()
		)));
	}
	Ok(child.wait()?.code().unwrap_or(1))
}

/// A suite stage's row for each recipe, as `suite_stage.run` writes them.
pub fn suite(
	stage: &str,
	out: &Path,
	root: &Path,
	record_baseline: &dyn Fn() -> Result<String>,
	env: &Environment,
	run: &mut dyn Invoke,
) -> Result<Vec<Value>> {
	if stage == "rune-suite" {
		bail!(value_error("rune-suite runs tools/column/suite_stage.py"));
	}
	let commands = catalogue::recipes(stage, root)?;
	if commands.is_empty() {
		bail!(value_error("empty recipe"));
	}
	let formats: Vec<String> = catalogue::formats().into_iter().map(|(n, _)| n).collect();
	let packages: Vec<&str> = catalogue::package_checks()
		.into_iter()
		.map(|(_, n, _)| n)
		.collect();
	let mut rows = Vec::new();
	for (i, command) in commands.iter().enumerate() {
		let mut row = Map::new();
		row.insert("command".into(), json!(command));
		let probe = probe_environment(command, env)?;
		if command.iter().any(|a| a == "test") {
			let mut list = command.clone();
			list.extend(["--".into(), "--list".into()]);
			let names = discovered(&run.invoke(
				&list,
				root,
				&out.join(format!("{i}-discovery.log")),
				&probe,
			)?)?;
			let mut threads = command.clone();
			threads.extend(["--".into(), "--test-threads=4".into()]);
			let passed = executed(&run.invoke(
				&threads,
				root,
				&out.join(format!("{i}-test.log")),
				&probe,
			)?)?;
			row.insert("discovered".into(), json!(names));
			row.insert("passed".into(), json!(passed));
		} else if packages.contains(&stage) {
			run.invoke(command, root, &out.join(format!("{i}-package.log")), &probe)?;
		} else if formats.iter().any(|f| f == stage) {
			run.invoke(command, root, &out.join(format!("{i}-format.log")), &probe)?;
		} else if stage == "rune-clippy" {
			run.invoke(command, root, &out.join(format!("{i}-clippy.log")), &probe)?;
		} else {
			// The record baseline's diagnostics bound the candidate's, with
			// no new allowance.
			let baseline = record_baseline()?;
			let bulk = assigned_root("RNX_FINAL_BASELINE_ROOT", out, env)?;
			let base = bulk.join(format!("baseline-{i}"));
			std::fs::create_dir(&base)?;
			let archive = bulk.join(format!("baseline-{i}.tar"));
			run.archive(root, &baseline, &archive)?;
			extract(&archive, &base)?;
			let mut lint = command.clone();
			lint.push("--message-format=json".into());
			if stage == "clippy-adapter-polars" {
				lint.extend(POLARS_LINT_ARGS.iter().map(|s| s.to_string()));
			}
			let mut sets = Vec::new();
			for (role, directory) in [("baseline", base.as_path()), ("candidate", root)] {
				let log = out.join(format!("{i}-{role}-clippy.log"));
				run.invoke(&lint, directory, &log, &probe)?;
				sets.push(diagnostics(&std::fs::read_to_string(&log)?, directory));
			}
			let added = added(&sets[0], &sets[1]);
			if !added.is_empty() {
				bail!(value_error(&format!(
					"new Clippy diagnostics: {}",
					render(&added)
				)));
			}
			row.insert("baseline".into(), json!(baseline));
		}
		rows.push(Value::Object(row));
	}
	Ok(rows)
}

/// The column probe's builds and lints read Valgrind's client header; the
/// format check does not.
fn probe_environment(command: &[String], env: &Environment) -> Result<Vec<(String, String)>> {
	if !command
		.iter()
		.any(|a| a == "rune/tools/xtask/column/probe/Cargo.toml")
		|| command.iter().any(|a| a == "fmt")
	{
		return Ok(vec![]);
	}
	let include = valgrind_root(env)?.join("include");
	if !include.join("valgrind/callgrind.h").is_file() {
		bail!(value_error(
			"column probe requires Valgrind include/valgrind/callgrind.h"
		));
	}
	Ok(vec![(
		"VALGRIND_INCLUDE".into(),
		include.to_string_lossy().into_owned(),
	)])
}

/// `RNX_VALGRIND_ROOT`, else the installation of the first executable
/// `valgrind` on PATH, as `fixed_route_gate.valgrind_root` resolves it:
/// either way the root holds an executable `bin/valgrind`.
pub fn valgrind_root(env: &Environment) -> Result<PathBuf> {
	let root = match env.get("RNX_VALGRIND_ROOT").filter(|v| !v.is_empty()) {
		Some(configured) => {
			std::fs::canonicalize(configured).unwrap_or_else(|_| PathBuf::from(configured))
		}
		None => {
			let binary = env
				.get("PATH")
				.into_iter()
				.flat_map(|p| std::env::split_paths(p).collect::<Vec<_>>())
				.map(|d| d.join("valgrind"))
				.find(|p| executable(p))
				.context("Valgrind missing: set RNX_VALGRIND_ROOT or put valgrind on PATH")?;
			let binary = std::fs::canonicalize(binary)?;
			binary
				.parent()
				.and_then(Path::parent)
				.context("valgrind has no installation")?
				.to_owned()
		}
	};
	if !executable(&root.join("bin/valgrind")) {
		bail!(value_error(
			"Valgrind root must contain executable bin/valgrind"
		));
	}
	Ok(root)
}

/// A regular file this process may execute.
fn executable(path: &Path) -> bool {
	use std::os::unix::ffi::OsStrExt as _;
	let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
		return false;
	};
	// SAFETY: a valid C string.
	path.is_file() && unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0
}

/// Unpack a tar of the published tree; no member may leave `into`.
fn extract(archive: &Path, into: &Path) -> Result<()> {
	let mut tar = tar::Archive::new(std::fs::File::open(archive)?);
	tar.set_preserve_permissions(false);
	tar.set_overwrite(false);
	for entry in tar.entries()? {
		let mut entry = entry?;
		if !entry.unpack_in(into)? {
			bail!(value_error("archive member outside its root"));
		}
	}
	Ok(())
}

/// A diagnostic's identity: target, level, lint code, message and its
/// primary files, normalized to the root, so code movement is not new.
pub type Identity = (String, String, Option<String>, String, Vec<String>);

/// Each warning or error in Cargo's JSON message lines, counted.
pub fn diagnostics(text: &str, root: &Path) -> BTreeMap<Identity, u64> {
	let mut found = BTreeMap::new();
	for line in text.lines() {
		let Ok(item) = serde_json::from_str::<Value>(line) else {
			continue;
		};
		if item["reason"] != "compiler-message" {
			continue;
		}
		let message = &item["message"];
		let level = message["level"].as_str().unwrap_or_default();
		if level != "warning" && level != "error" {
			continue;
		}
		let mut files: Vec<String> = message["spans"]
			.as_array()
			.into_iter()
			.flatten()
			.filter(|s| s["is_primary"] == true)
			.filter_map(|s| s["file_name"].as_str())
			.map(|f| {
				let p = Path::new(f);
				match p.strip_prefix(root) {
					Ok(relative) if p.is_absolute() => relative.to_string_lossy().into_owned(),
					_ => f.to_owned(),
				}
			})
			.collect();
		files.sort();
		files.dedup();
		let key = (
			item["target"]["name"]
				.as_str()
				.unwrap_or_default()
				.to_owned(),
			level.to_owned(),
			message["code"]["code"].as_str().map(str::to_owned),
			message["message"].as_str().unwrap_or_default().to_owned(),
			files,
		);
		*found.entry(key).or_insert(0) += 1;
	}
	found
}

/// What the candidate has beyond the baseline, by count.
pub fn added(
	baseline: &BTreeMap<Identity, u64>,
	candidate: &BTreeMap<Identity, u64>,
) -> BTreeMap<Identity, u64> {
	candidate
		.iter()
		.filter_map(|(k, n)| {
			let more = n.saturating_sub(*baseline.get(k).unwrap_or(&0));
			(more > 0).then(|| (k.clone(), more))
		})
		.collect()
}

/// Identities with their counts, as `[[target, level, code, message,
/// [files]], count]` rows.
pub fn render(counts: &BTreeMap<Identity, u64>) -> Value {
	Value::Array(
		counts
			.iter()
			.map(|((t, l, c, m, f), n)| json!([[t, l, c, m, f], n]))
			.collect(),
	)
}

/// `clippy`'s comparison against the fixed published commit: its receipt,
/// written before an added diagnostic fails the stage.
pub fn clippy(
	adapter: bool,
	out: &Path,
	root: &Path,
	env: &Environment,
	run: &mut dyn Invoke,
) -> Result<Value> {
	let bulk = assigned_root("RNX_FINAL_BASELINE_ROOT", out, env)?;
	let base = bulk.join("published");
	std::fs::create_dir(&base)?;
	let archive = bulk.join("published.tar");
	run.archive(root, CLIPPY_BASE, &archive)?;
	extract(&archive, &base)?;
	let jobs = build_jobs(env)?;
	let mut command: Command = [
		"cargo",
		"clippy",
		"--locked",
		"--lib",
		"--tests",
		"--message-format=json",
	]
	.iter()
	.map(|s| s.to_string())
	.collect();
	if adapter {
		command.extend(
			[
				"--manifest-path",
				"adapters/polars/Cargo.toml",
				"--features",
				"test-support",
			]
			.map(String::from),
		);
		command.extend(POLARS_LINT_ARGS.map(String::from));
	} else {
		command.push("--no-default-features".into());
	}
	let mut sets = BTreeMap::new();
	for (role, directory) in [("published", base.as_path()), ("candidate", root)] {
		let target = root.join("target").join(format!(
			"column-clippy-{}{role}",
			if adapter { "adapter-" } else { "" }
		));
		let jsonl = out.join(format!("{role}.jsonl"));
		let code = run.clippy(
			&command,
			directory,
			&jsonl,
			&out.join(format!("{role}.stderr")),
			&target,
			jobs,
		)?;
		sets.insert(
			role,
			diagnostics(&std::fs::read_to_string(&jsonl)?, directory),
		);
		if code != 0 {
			bail!(value_error(&format!(
				"{role} Clippy failed; see {}",
				out.display()
			)));
		}
	}
	let added = added(&sets["published"], &sets["candidate"]);
	Ok(
		json!({"status": if added.is_empty() { "PASS" } else { "FAIL" }, "published": CLIPPY_BASE,
		"existing": render(&sets["published"]), "candidate": render(&sets["candidate"]), "added": render(&added)}),
	)
}

/// Cargo's jobs for a Clippy comparison: inside a final its jobserver's
/// width, else what the host's CPUs and memory admit.
fn build_jobs(env: &Environment) -> Result<usize> {
	if let Some(width) = env
		.get("MAKEFLAGS")
		.and_then(|f| job_budget::fifo::inherited_width(f))
	{
		return Ok(width);
	}
	let observation = job_budget::memory::observe(&job_budget::memory::Sources::default())?;
	job_budget::memory::build_jobs(&observation)
}

/// Inside a final, the stage must be the one its context names, before it
/// does anything: this executable (path and digest), this invocation of
/// the logical program, and this output directory.
pub fn bound(args: &[String], out: &Path, env: &Environment) -> Result<()> {
	let Some(context) = env.get(crate::child::CONTEXT).filter(|v| !v.is_empty()) else {
		return Ok(());
	};
	let ctx: Value = serde_json::from_slice(&std::fs::read(context)?)?;
	let program = std::fs::canonicalize("/proc/self/exe")?;
	if ctx["executable"]["path"] != json!(program.to_string_lossy())
		|| ctx["executable"]["sha256"] != json!(digest(&program)?)
	{
		bail!(value_error(
			"the stage is not the executable its context names"
		));
	}
	let mut command = vec![PROGRAM.to_string()];
	command.extend(["stage".to_string()]);
	command.extend(args.iter().cloned());
	if ctx["command"] != json!(command) {
		bail!(value_error(
			"the stage is not the command its context names"
		));
	}
	if ctx["result_directory"] != json!(out.to_string_lossy()) {
		bail!(value_error(
			"the stage's output is not its context's result directory"
		));
	}
	Ok(())
}

/// `rnx-final stage MODE ...`: run the stage and publish its receipt; the
/// process exit is nonzero unless it passed.
pub fn main(arguments: &[OsString], root: &Path) -> Result<i32> {
	let args: Vec<String> = arguments
		.iter()
		.map(|a| a.to_string_lossy().into_owned())
		.collect();
	let value = |flag: &str| {
		args.iter()
			.position(|a| a == flag)
			.and_then(|i| args.get(i + 1))
			.cloned()
	};
	let out = PathBuf::from(value("--out").context("a stage needs --out")?);
	let out = if out.is_absolute() {
		out
	} else {
		std::env::current_dir()?.join(out)
	};
	let env: Environment = std::env::vars().collect();
	bound(&args, &out, &env)?;
	match args.first().map(String::as_str) {
		Some("suite") => {
			let stage = value("--stage").context("a suite stage needs --stage")?;
			std::fs::create_dir_all(out.parent().context("--out has no parent")?)?;
			std::fs::create_dir(&out)?;
			let mut r = json!({"status": "FAIL", "stage": stage});
			let record = || -> Result<String> {
				Ok(crate::selection::record(root, "HEAD")?["baseline"]
					.as_str()
					.context("record without baseline")?
					.to_owned())
			};
			match suite(
				&stage,
				&out,
				root,
				&record,
				&env,
				&mut Live { env: env.clone() },
			) {
				Ok(rows) => {
					r["checks"] = json!(rows);
					r["status"] = json!("PASS");
				}
				Err(error) => r["error"] = json!(format!("{error:#}")),
			}
			let mut artifacts = Map::new();
			for entry in std::fs::read_dir(&out)?.flatten() {
				let path = entry.path();
				if path.extension().is_some_and(|e| e == "log") {
					artifacts.insert(path.to_string_lossy().into_owned(), json!(digest(&path)?));
				}
			}
			r["artifacts"] = Value::Object(artifacts);
			publish(&out.join("RESULT.json"), &r, &env)?;
			println!(
				"{} {}",
				r["status"].as_str().unwrap_or("FAIL"),
				out.join("RESULT.json").display()
			);
			Ok(if r["status"] == "PASS" { 0 } else { 1 })
		}
		Some("clippy") => {
			let adapter = args.iter().any(|a| a == "--adapter");
			std::fs::create_dir_all(out.parent().context("--out has no parent")?)?;
			std::fs::create_dir(&out)?;
			// A failed Cargo or extraction publishes nothing, as clippy.py did.
			let result = clippy(adapter, &out, root, &env, &mut Live { env: env.clone() })?;
			publish(&out.join("RESULT.json"), &result, &env)?;
			if result["status"] != "PASS" {
				bail!(value_error(&format!(
					"new Clippy diagnostics: {}",
					result["added"]
				)));
			}
			Ok(0)
		}
		Some(other) => bail!(value_error(&format!("unknown stage mode: {other}"))),
		None => bail!(value_error("a stage needs a mode")),
	}
}
