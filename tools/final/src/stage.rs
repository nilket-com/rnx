//! The stage programs the controller runs as its own executable:
//! `rnx-final stage suite --stage NAME --out DIR` (the package, format,
//! lint, suite and Clippy stages, `rune-suite` with its native contract
//! cases) and `rnx-final stage clippy [--adapter] --out DIR` (the
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
		.map_err(|e| spawn_error(&command.get_program().to_string_lossy(), &e))?;
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

/// A suite stage's row for each recipe, as `suite_stage.run` wrote them.
pub fn suite(
	stage: &str,
	out: &Path,
	root: &Path,
	record_baseline: &dyn Fn() -> Result<String>,
	env: &Environment,
	run: &mut dyn Invoke,
) -> Result<Vec<Value>> {
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
			// An API boundary runs exactly its twins, every one passing.
			if stage == "rune-suite" && command.iter().any(|a| a == "--doc") {
				let twins = catalogue::TWINS
					.iter()
					.find(|(s, _)| Some(*s) == command.last().map(String::as_str))
					.map(|(_, n)| *n);
				if twins != Some(names.len()) || passed != names.len() as u64 {
					bail!(value_error(
						"API boundary requires both compiling and compile-fail twins"
					));
				}
			}
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
	valgrind_root_from(None, env)
}

/// The Valgrind installation: an explicit root first, then
/// `RNX_VALGRIND_ROOT`, then the `valgrind` on PATH, resolved through any
/// link before its root is derived.
pub fn valgrind_root_from(explicit: Option<&Path>, env: &Environment) -> Result<PathBuf> {
	let configured = explicit
		.map(|p| p.to_string_lossy().into_owned())
		.or_else(|| {
			env.get("RNX_VALGRIND_ROOT")
				.filter(|v| !v.is_empty())
				.cloned()
		});
	let root = match configured {
		Some(configured) => {
			std::fs::canonicalize(&configured).unwrap_or_else(|_| PathBuf::from(&configured))
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

/// Cargo's jobs for a stage's builds: inside a final its jobserver's
/// width, else what the host's CPUs and memory admit.
pub fn build_jobs(env: &Environment) -> Result<usize> {
	if let Some(width) = env
		.get("MAKEFLAGS")
		.and_then(|f| job_budget::fifo::inherited_width(f))
	{
		return Ok(width);
	}
	let observation = job_budget::memory::observe(&job_budget::memory::Sources::default())?;
	job_budget::memory::build_jobs(&observation)
}

/// An `AssertionError` as Python's `repr` and `str` write it.
fn assertion(message: &str) -> anyhow::Error {
	anyhow::Error::new(crate::child::Raised {
		class: "AssertionError",
		repr: if message.is_empty() {
			"AssertionError()".into()
		} else {
			format!(
				"AssertionError({})",
				crate::failure::python_repr(&json!(message))
			)
		},
		message: message.into(),
	})
}

/// The release CLI's routing and fuel contract over one run's output, as
/// `cli.validate` held it.
pub fn validate_cli(
	code: i32,
	stdout: &str,
	stderr: &str,
	exhausted: bool,
	stock: bool,
) -> Result<()> {
	let check = |ok: bool, message: &str| if ok { Ok(()) } else { Err(assertion(message)) };
	check(
		stderr.contains("rnx: engine=legacy"),
		"outer script did not select stock VM",
	)?;
	check(
		!stderr.contains("rnx: engine=execution"),
		"opaque script entered execution engine",
	)?;
	check(
		stdout.contains("COLUMN_ENTER"),
		"never reached the map call",
	)?;
	check(
		!stdout.contains("COLUMN_CAUGHT"),
		"fuel halt became a catchable Result",
	)?;
	if exhausted {
		check(code != 0, "fuel exhaustion returned success")?;
		check(
			!stdout.contains("COLUMN_OK"),
			"completed despite insufficient fuel",
		)?;
		check(
			stderr.contains("work units") && stderr.contains("--budget"),
			"missing CLI fuel diagnostic",
		)?;
	} else {
		check(code == 0, "successful control failed")?;
		check(
			stdout.contains("COLUMN_OK"),
			"output equality/length checks never completed",
		)?;
		check(
			stdout.contains("completed=true; rows=10000; valid_rows=10000;"),
			"",
		)?;
		if stock {
			check(stdout.contains("path=scalar-fallback;"), "")?;
			check(stdout.contains("attempts=0; replays=0;"), "")?;
		} else {
			check(
				stdout.contains("path=avx2;") || stdout.contains("path=scalar-primitives;"),
				"",
			)?;
			check(stdout.contains("attempts=1; replays=0; refusal=None"), "")?;
		}
	}
	Ok(())
}

/// The checker's own controls: two runs it must pass, and planted evidence
/// it must refuse, as `cli.controls` held them.
pub fn cli_controls() -> Result<()> {
	let good = "COLUMN_ENTER\nCOLUMN_OK path=avx2; completed=true; rows=10000; valid_rows=10000; attempts=1; replays=0; refusal=None";
	let legacy = "rnx: engine=legacy refusal=HostContract";
	validate_cli(0, good, legacy, false, false)?;
	validate_cli(
		1,
		"COLUMN_ENTER",
		&format!("{legacy} 2000 work units exceeded; --budget N raises it"),
		true,
		false,
	)?;
	let planted: [(i32, String, String, bool); 6] = [
		(0, good.into(), "rnx: engine=execution".into(), false),
		(
			0,
			good.replace("rows=10000", "rows=0"),
			legacy.into(),
			false,
		),
		(
			0,
			good.replace("COLUMN_OK", "SKIPPED"),
			legacy.into(),
			false,
		),
		(
			0,
			good.replace("path=avx2", "path=scalar-fallback"),
			legacy.into(),
			false,
		),
		(
			0,
			"COLUMN_ENTER\nCOLUMN_CAUGHT fuel".into(),
			legacy.into(),
			true,
		),
		(
			1,
			String::new(),
			format!("{legacy}work units --budget"),
			true,
		),
	];
	for (code, stdout, stderr, exhausted) in planted {
		if validate_cli(code, &stdout, &stderr, exhausted, false).is_ok() {
			bail!(assertion("invalid CLI evidence passed the checker"));
		}
	}
	Ok(())
}

/// How the CLI stage runs the binary, so a control can answer for it.
pub trait CliRun {
	/// Run `command` in `cwd` within `limit`; its exit, stdout and stderr.
	fn run(
		&mut self,
		command: &[String],
		cwd: &Path,
		limit: Duration,
	) -> Result<(i32, String, String)>;
}

/// The binary run for real, with the stage's environment, its output
/// captured through files in `out`.
pub struct LiveCli {
	pub env: Environment,
	pub out: PathBuf,
}

impl CliRun for LiveCli {
	fn run(
		&mut self,
		command: &[String],
		cwd: &Path,
		limit: Duration,
	) -> Result<(i32, String, String)> {
		let id = crate::receipt::invocation_id();
		let (stdout, stderr) = (
			self.out.join(format!(".{id}.stdout")),
			self.out.join(format!(".{id}.stderr")),
		);
		let mut child = std::process::Command::new(&command[0]);
		child
			.args(&command[1..])
			.current_dir(cwd)
			.env_clear()
			.envs(&self.env)
			.stdin(std::process::Stdio::null())
			.stdout(std::fs::File::create_new(&stdout)?)
			.stderr(std::fs::File::create_new(&stderr)?);
		let result = wait(child, limit).and_then(|code| {
			Ok((
				code,
				text_mode(&std::fs::read(&stdout)?)?,
				text_mode(&std::fs::read(&stderr)?)?,
			))
		});
		let _ = std::fs::remove_file(&stdout);
		let _ = std::fs::remove_file(&stderr);
		result
	}
}

/// A child's output as Python's `subprocess.run(text=True)` reads it:
/// strict UTF-8, and universal newlines (`\r\n` and `\r` become `\n`).
pub fn text_mode(bytes: &[u8]) -> Result<String> {
	let text = std::str::from_utf8(bytes).map_err(|e| {
		crate::child::raise(
			"UnicodeDecodeError",
			&format!(
				"'utf-8' codec can't decode byte at position {}",
				e.valid_up_to()
			),
		)
	})?;
	Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `rnx-final stage cli`: the release CLI over both fixtures, both engines
/// and both budgets, each run held to the contract, and the binary
/// unchanged throughout. The receipt, without its seconds; the error, if
/// any, is also written as `TRACEBACK.txt`.
pub fn cli(binary: &Path, out: &Path, root: &Path, run: &mut dyn CliRun) -> Result<Value> {
	let mut result = json!({"status": "FAIL", "cases": []});
	let outcome = (|| -> Result<()> {
		cli_controls()?;
		let binary = std::fs::canonicalize(binary)
			.map_err(|e| crate::child::raise("FileNotFoundError", &e.to_string()))?;
		result["binary"] = json!(binary.to_string_lossy());
		result["binary_sha256"] = json!(digest(&binary)?);
		for engine in ["default", "legacy"] {
			for stock in [false, true] {
				let fixture =
					std::fs::canonicalize(root.join("tools/column/fixtures").join(if stock {
						"cli-stock.rn"
					} else {
						"cli-map.rn"
					}))?;
				for work in [2000u64, 1_000_000] {
					let name = format!(
						"{engine}-{}-{work}",
						if stock { "stock" } else { "primitives" }
					);
					let mut command: Vec<String> = vec![
						binary.to_string_lossy().into_owned(),
						"run".into(),
						"--debug-engine".into(),
						"--budget".into(),
						work.to_string(),
					];
					if engine == "legacy" {
						command.push("--engine=legacy".into());
					}
					command.push(fixture.to_string_lossy().into_owned());
					let (code, stdout, stderr) =
						run.run(&command, root, Duration::from_secs(120))?;
					std::fs::write(out.join(format!("{name}.stdout")), &stdout)?;
					std::fs::write(out.join(format!("{name}.stderr")), &stderr)?;
					result["cases"].as_array_mut().unwrap().push(
						json!({"name": name, "command": command, "exit": code, "fixture_sha256": digest(&fixture)?}),
					);
					validate_cli(code, &stdout, &stderr, work == 2000, stock)?;
				}
			}
		}
		if json!(digest(&binary)?) != result["binary_sha256"] {
			bail!(assertion(""));
		}
		result["status"] = json!("PASS");
		Ok(())
	})();
	if let Err(error) = outcome {
		result["error"] = json!(format!("{error}"));
		let last = match error.downcast_ref::<crate::child::Raised>() {
			Some(r) if r.message.is_empty() => r.class.to_string(),
			Some(r) => format!("{}: {}", r.class, r.message),
			None => format!("{error:#}"),
		};
		std::fs::write(out.join("TRACEBACK.txt"), format!("{last}\n"))?;
	}
	Ok(result)
}

/// The Jupyter stage's children, in the order its runner runs them: the
/// declaration the route checker reads, with inert paths there.
pub fn jupyter_children(python: &str, root: &Path) -> Vec<Command> {
	let text = |p: &Path| p.to_string_lossy().into_owned();
	vec![
		["cargo", "+1.98.1", "build", "--locked", "--bin", "rnx"]
			.iter()
			.map(|s| s.to_string())
			.collect(),
		[
			"cargo",
			"+1.98.1",
			"build",
			"--locked",
			"--manifest-path",
			"jupyter/Cargo.toml",
			"--bin",
			"rnx-jupyter",
		]
		.iter()
		.map(|s| s.to_string())
		.collect(),
		vec![
			python.to_owned(),
			"-B".into(),
			"tests/test_jupyter_execution.py".into(),
			text(&root.join("jupyter/target/debug/rnx-jupyter")),
			text(&root.join("target/debug/rnx")),
		],
	]
}

/// The children a Rust stage mode runs, as it declares them, with inert
/// paths: what a test route through that stage is checked against.
pub fn declared_children(command: &[String]) -> Vec<Command> {
	match command.get(2).map(String::as_str) {
		Some("jupyter")
			if command[0] == PROGRAM && command.get(1).map(String::as_str) == Some("stage") =>
		{
			jupyter_children("<pinned-python>", Path::new("<root>"))
		}
		_ => Vec::new(),
	}
}

/// How the Jupyter stage runs a child under the producer protocol.
pub trait ChildRun {
	/// Run `command` in `root`, its output in the new file `log`, with its
	/// receipt directory `directory`, within `limit`.
	fn run(
		&mut self,
		command: &[String],
		root: &Path,
		env: &Environment,
		log: &Path,
		directory: &Path,
		limit: Duration,
	) -> Result<crate::child::Done>;
}

/// The children run by the controller's Rust child runner, under this
/// executable as their guardian.
pub struct LiveChild {
	pub groups: PathBuf,
}

impl ChildRun for LiveChild {
	fn run(
		&mut self,
		command: &[String],
		root: &Path,
		env: &Environment,
		log: &Path,
		directory: &Path,
		limit: Duration,
	) -> Result<crate::child::Done> {
		let guardian = std::fs::canonicalize("/proc/self/exe")?;
		crate::child::run(crate::child::Run {
			command,
			cwd: root,
			root,
			env,
			log,
			timeout: Some(limit),
			directory,
			groups: &self.groups,
			guardian: &guardian,
			after: None,
			stop: None,
		})
	}
}

/// `rnx-final stage jupyter`: the two builds and the pinned client's
/// installed-kernelspec contract, in order, each a child with its own
/// receipt, as `jupyter_controls.main` ran them.
pub fn jupyter(
	out: &Path,
	root: &Path,
	env: &Environment,
	jobs: &dyn Fn(&Environment) -> Result<usize>,
	run: &mut dyn ChildRun,
) -> Value {
	let mut r = json!({"status": "FAIL", "checks": []});
	let outcome = (|| -> Result<()> {
		let python = env
			.get("RNX_JUPYTER_PYTHON")
			.map(PathBuf::from)
			.unwrap_or_else(|| root.join("target/jupyter-client/bin/python"));
		if !python.is_file() {
			bail!(value_error(
				"set RNX_JUPYTER_PYTHON to the pinned Jupyter client environment"
			));
		}
		let mut child_env = env.clone();
		child_env.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
		// One budget decision, which every child reuses.
		child_env.insert("CARGO_BUILD_JOBS".into(), jobs(env)?.to_string());
		for (i, command) in jupyter_children(&python.to_string_lossy(), root)
			.into_iter()
			.enumerate()
		{
			let done = run.run(
				&command,
				root,
				&child_env,
				&out.join(format!("{i}.log")),
				&out.join(format!("{i}-child")),
				Duration::from_secs(1800),
			)?;
			r["checks"]
				.as_array_mut()
				.unwrap()
				.push(json!({"command": command, "validation": done.validation}));
			if done.returncode != 0 {
				bail!(crate::child::raise(
					"RuntimeError",
					&format!("stage {i} failed")
				));
			}
		}
		r["status"] = json!("PASS");
		Ok(())
	})();
	if let Err(error) = outcome {
		r["error"] = json!(format!("{error:#}"));
	}
	r
}

/// The pinned Polars inventory configurations, in order.
pub const INVENTORY_CONFIGURATIONS: [&str; 3] = ["adapter-json", "adapter", "adapter-narrow"];
/// The nightly toolchain that documents the pinned release.
pub const NIGHTLY: &str = "nightly-2026-09-20";
/// The extractor's and the generator's manifests.
const INVENTORY_MANIFESTS: [&str; 2] = [
	"probes/0072/extract/Cargo.toml",
	"tools/polars-gen/Cargo.toml",
];

/// An `AssertionError` whose message is Python's `str` of a tuple or list:
/// its repr shows the value itself, not a quoted string.
fn assertion_value(shown: &str) -> anyhow::Error {
	anyhow::Error::new(crate::child::Raised {
		class: "AssertionError",
		repr: format!("AssertionError({shown})"),
		message: shown.into(),
	})
}

/// A required field, as Python's `value[key]` reads it: missing is a
/// `KeyError`.
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
	value
		.get(key)
		.ok_or_else(|| crate::child::raise("KeyError", key))
}

/// A feature list's names in sorted order, counting duplicates, for a
/// multiset comparison; anything but a list of strings is a `TypeError`.
fn sorted_items(value: &Value) -> Result<Vec<&str>> {
	let not_names = || crate::child::raise("TypeError", "a feature list is not a list of names");
	let mut names = value
		.as_array()
		.ok_or_else(not_names)?
		.iter()
		.map(|item| item.as_str().ok_or_else(not_names))
		.collect::<Result<Vec<&str>>>()?;
	names.sort_unstable();
	Ok(names)
}

/// An inventory against its documentation's pins, as
/// `inventories.validate_inventory` held it: every field required, checked
/// in the same short-circuit order.
pub fn validate_inventory(cfg: &str, pins: &Value, inventory: &Value) -> Result<()> {
	let truthy = crate::failure::truthy;
	if !(*field(pins, "release")? == "0.55.2"
		&& *field(pins, "cfg")? == cfg
		&& *field(pins, "toolchain")? == NIGHTLY)
	{
		bail!(assertion(""));
	}
	if !(truthy(field(pins, "crates")?) && !truthy(field(pins, "failed")?)) {
		bail!(assertion("incomplete documentation"));
	}
	let provenance = field(inventory, "provenance")?;
	if !(*field(provenance, "release")? == "0.55.2" && *field(provenance, "cfg")? == cfg) {
		bail!(assertion(""));
	}
	let features = sorted_items(field(provenance, "features")?)?;
	if features != sorted_items(field(pins, "resolved_polars_features")?)? {
		bail!(assertion(""));
	}
	if !truthy(field(inventory, "callables")?) {
		bail!(assertion("empty inventory"));
	}
	Ok(())
}

/// Python's `CalledProcessError` for a failed query: its repr and its
/// traceback line, module included.
pub fn called_process_error(command: &[String], code: i32) -> anyhow::Error {
	let list = crate::failure::python_repr(&json!(command));
	anyhow::Error::new(crate::child::Raised {
		class: "subprocess.CalledProcessError",
		repr: format!("CalledProcessError({code}, {list})"),
		message: format!("Command '{list}' returned non-zero exit status {code}."),
	})
}

/// A process that could not start, named by its program and the OS error
/// only: never the command's environment, which may hold secrets.
pub fn spawn_error(program: &str, error: &std::io::Error) -> anyhow::Error {
	anyhow::anyhow!("spawn {program}: {error}")
}

/// How the inventories stage runs its named steps and git queries, so a
/// control can answer for them.
pub trait InventoryRun {
	/// Run a named step with its own environment, stdout and stderr in new
	/// files, within `limit`; its exit code.
	fn run(
		&mut self,
		command: &[String],
		cwd: &Path,
		env: &Environment,
		stdout: &Path,
		stderr: &Path,
		limit: Duration,
	) -> Result<i32>;
	/// A git query's output, read as text mode reads it; a failure is an error.
	fn query(&mut self, command: &[String], cwd: &Path) -> Result<String>;
}

/// The steps and queries run for real; queries take the stage's own
/// environment, through the controller's git command for git.
pub struct LiveInventory {
	pub env: Environment,
}

impl InventoryRun for LiveInventory {
	fn run(
		&mut self,
		command: &[String],
		cwd: &Path,
		env: &Environment,
		stdout: &Path,
		stderr: &Path,
		limit: Duration,
	) -> Result<i32> {
		let mut child = std::process::Command::new(&command[0]);
		child
			.args(&command[1..])
			.current_dir(cwd)
			.env_clear()
			.envs(env)
			.stdin(std::process::Stdio::null())
			.stdout(std::fs::File::create_new(stdout)?)
			.stderr(std::fs::File::create_new(stderr)?);
		wait(child, limit)
	}

	fn query(&mut self, command: &[String], cwd: &Path) -> Result<String> {
		let mut child = if command[0] == "git" {
			crate::child::git_command()
		} else {
			std::process::Command::new(&command[0])
		};
		// The stage's environment, with the controller's lock-free git.
		let mut env = self.env.clone();
		env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
		// stderr goes to the stage's own log, as check_output left it.
		let output = child
			.args(&command[1..])
			.current_dir(cwd)
			.env_clear()
			.envs(&env)
			.stdin(std::process::Stdio::null())
			.stderr(std::process::Stdio::inherit())
			.output()
			.map_err(|e| spawn_error(&command[0], &e))?;
		if !output.status.success() {
			bail!(called_process_error(
				command,
				output.status.code().unwrap_or(1)
			));
		}
		text_mode(&output.stdout)
	}
}

/// `rnx-final stage inventories [--preflight]`: the pinned Polars
/// inventories prepared from source and the generated bindings checked
/// against them, as `inventories.main` ran it. The receipt, without its
/// seconds; on an error, `TRACEBACK.txt` holds its last line.
pub fn inventories(
	out: &Path,
	root: &Path,
	env: &Environment,
	jobs: usize,
	preflight: bool,
	run: &mut dyn InventoryRun,
) -> Result<Value> {
	let mut base = env.clone();
	base.insert("CARGO_BUILD_JOBS".into(), jobs.to_string());
	base.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
	base.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
	base.remove("CARGO_TARGET_DIR");
	let mut offline = base.clone();
	offline.insert("CARGO_NET_OFFLINE".into(), "true".into());
	let mut result =
		json!({"status": "FAIL", "toolchain": NIGHTLY, "checks": [], "inventories": []});
	let strings = |items: &[&str]| -> Vec<String> { items.iter().map(|s| s.to_string()).collect() };
	let outcome = (|| -> Result<()> {
		let step = |run: &mut dyn InventoryRun,
		            result: &mut Value,
		            name: &str,
		            command: Vec<String>,
		            environment: &Environment|
		 -> Result<()> {
			let code = run.run(
				&command,
				root,
				environment,
				&out.join(format!("{name}.stdout")),
				&out.join(format!("{name}.stderr")),
				Duration::from_secs(10800),
			)?;
			result["checks"]
				.as_array_mut()
				.unwrap()
				.push(json!({"name": name, "command": command, "exit": code}));
			if code != 0 {
				bail!(assertion_value(&format!(
					"({}, {code})",
					crate::failure::python_repr(&json!(name))
				)));
			}
			Ok(())
		};
		let head = run
			.query(&strings(&["git", "rev-parse", "HEAD"]), root)?
			.trim()
			.to_owned();
		if !run
			.query(&strings(&["git", "status", "--porcelain"]), root)?
			.trim()
			.is_empty()
		{
			bail!(assertion(""));
		}
		result["source_commit"] = json!(head);
		let mut locks: Vec<String> = INVENTORY_CONFIGURATIONS
			.iter()
			.map(|c| format!("probes/0072/inventory/locks/0.55.2-{c}.lock"))
			.collect();
		locks.extend(INVENTORY_MANIFESTS.iter().map(|m| {
			Path::new(m)
				.with_file_name("Cargo.lock")
				.to_string_lossy()
				.into_owned()
		}));
		let mut before = Map::new();
		for lock in &locks {
			before.insert(lock.clone(), json!(digest(&root.join(lock))?));
		}
		// Toolchain installation is a prerequisite, not an implicit download.
		step(
			&mut *run,
			&mut result,
			"nightly-preflight",
			strings(&["rustup", "run", NIGHTLY, "rustc", "--version"]),
			&offline,
		)?;
		if preflight {
			result["locks_sha256"] = Value::Object(before);
			result["status"] = json!("PASS");
			return Ok(());
		}
		for (i, manifest) in INVENTORY_MANIFESTS.iter().enumerate() {
			step(
				&mut *run,
				&mut result,
				&format!("fetch-tool-{i}"),
				strings(&[
					"cargo",
					"+1.98.1",
					"fetch",
					"--locked",
					"--manifest-path",
					manifest,
				]),
				&base,
			)?;
		}
		for cfg in INVENTORY_CONFIGURATIONS {
			step(
				&mut *run,
				&mut result,
				&format!("fetch-{cfg}"),
				strings(&[
					"bash",
					"probes/0072/inventory/doc.sh",
					"0.55.2",
					cfg,
					"--fetch-only",
				]),
				&base,
			)?;
		}
		let target = root.join("target/column-inventory-extract");
		let mut build = offline.clone();
		build.insert(
			"CARGO_TARGET_DIR".into(),
			target.to_string_lossy().into_owned(),
		);
		step(
			&mut *run,
			&mut result,
			"build-extractor",
			strings(&[
				"cargo",
				"+1.98.1",
				"build",
				"--locked",
				"--manifest-path",
				INVENTORY_MANIFESTS[0],
			]),
			&build,
		)?;
		for cfg in INVENTORY_CONFIGURATIONS {
			step(
				&mut *run,
				&mut result,
				&format!("document-{cfg}"),
				strings(&["bash", "probes/0072/inventory/doc.sh", "0.55.2", cfg]),
				&offline,
			)?;
			let docs = root.join(format!("probes/0072/out/0.55.2-{cfg}"));
			let pins: Value = serde_json::from_slice(&std::fs::read(docs.join("pins.json"))?)?;
			let crates = field(&pins, "crates")?;
			if !(crate::failure::truthy(crates) && !crate::failure::truthy(field(&pins, "failed")?))
			{
				bail!(assertion_value(&crate::failure::python_repr(
					&pins["failed"]
				)));
			}
			// Each documented crate's file, before the extractor runs: a
			// string names no crate files (Python iterated its characters),
			// and anything but a list of names is a TypeError.
			let documented = match crates {
				Value::String(_) => false,
				Value::Array(names) => {
					let mut all = true;
					for name in names {
						let name = name.as_str().ok_or_else(|| {
							crate::child::raise("TypeError", "a crate name is not a string")
						})?;
						all &= docs.join(format!("{name}.json")).is_file();
					}
					all
				}
				_ => bail!(crate::child::raise("TypeError", "crates is not a list")),
			};
			if !documented {
				bail!(assertion(""));
			}
			let surface = target.join("debug/surface");
			step(
				&mut *run,
				&mut result,
				&format!("extract-{cfg}"),
				vec![
					surface.to_string_lossy().into_owned(),
					docs.to_string_lossy().into_owned(),
					"polars".into(),
					docs.join("result").to_string_lossy().into_owned(),
				],
				&offline,
			)?;
			let inventory = docs.join("result/inventory.json");
			validate_inventory(
				cfg,
				&pins,
				&serde_json::from_slice(&std::fs::read(&inventory)?)?,
			)?;
			result["inventories"].as_array_mut().unwrap().push(json!({"cfg": cfg, "sha256": digest(&inventory)?, "pins_sha256": digest(&docs.join("pins.json"))?}));
		}
		// The same policy and bucket selection as the full-suite drift test.
		let tests = std::fs::read_to_string(root.join("adapters/polars/tests/generated.rs"))?;
		let mut constants = BTreeMap::new();
		for name in ["BUCKETS", "RELEASE_FILE"] {
			let pattern = regex::Regex::new(&format!(r#"const {name}: &str = "([^"]+)";"#))?;
			let found: Vec<String> = pattern
				.captures_iter(&tests)
				.map(|c| c[1].to_owned())
				.collect();
			if found.len() != 1 {
				bail!(assertion_value(&format!(
					"('generator test contract changed', {})",
					crate::failure::python_repr(&json!(name))
				)));
			}
			constants.insert(name, found[0].clone());
		}
		let text = |p: PathBuf| p.to_string_lossy().into_owned();
		step(
			&mut *run,
			&mut result,
			"generated-bindings-check",
			vec![
				"cargo".into(),
				"+1.98.1".into(),
				"run".into(),
				"--locked".into(),
				"--manifest-path".into(),
				INVENTORY_MANIFESTS[1].into(),
				"--".into(),
				text(root.join("probes/0072/out/0.55.2-adapter-json/result/inventory.json")),
				text(root.join("adapters/polars")),
				"--buckets".into(),
				constants["BUCKETS"].clone(),
				"--release".into(),
				text(
					root.join("tools/polars-gen/releases")
						.join(&constants["RELEASE_FILE"]),
				),
				"--check".into(),
			],
			&offline,
		)?;
		let mut after = Map::new();
		for lock in &locks {
			after.insert(lock.clone(), json!(digest(&root.join(lock))?));
		}
		if after != before {
			bail!(assertion("locked dependencies changed"));
		}
		result["locks_sha256"] = Value::Object(before);
		if run
			.query(&strings(&["git", "rev-parse", "HEAD"]), root)?
			.trim() != head
		{
			bail!(assertion(""));
		}
		if !run
			.query(&strings(&["git", "status", "--porcelain"]), root)?
			.trim()
			.is_empty()
		{
			bail!(assertion(""));
		}
		result["status"] = json!("PASS");
		Ok(())
	})();
	if let Err(error) = outcome {
		result["error"] = json!(format!("{error}"));
		let last = match error.downcast_ref::<crate::child::Raised>() {
			Some(r) if r.message.is_empty() => r.class.to_string(),
			Some(r) => format!("{}: {}", r.class, r.message),
			None => format!("{error:#}"),
		};
		std::fs::write(out.join("TRACEBACK.txt"), format!("{last}\n"))?;
	}
	Ok(result)
}

/// A suite stage's receipt, as `suite_stage.main` wrote it: its rows,
/// then for `rune-suite` its native contracts, which must pass; and every
/// log in `out`, by digest. An error is the receipt's, not the caller's.
pub fn receipt(
	stage: &str,
	out: &Path,
	rows: impl FnOnce() -> Result<Vec<Value>>,
	contracts: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
	let mut r = json!({"status": "FAIL", "stage": stage});
	let outcome = (|| -> Result<()> {
		r["checks"] = json!(rows()?);
		if stage == "rune-suite" {
			let c = contracts()?;
			r["contracts"] = c.clone();
			if c["status"] != "PASS" {
				bail!(value_error(c["error"].as_str().unwrap_or_default()));
			}
		}
		r["status"] = json!("PASS");
		Ok(())
	})();
	if let Err(error) = outcome {
		r["error"] = json!(format!("{error:#}"));
	}
	let mut artifacts = Map::new();
	for entry in std::fs::read_dir(out)?.flatten() {
		let path = entry.path();
		if path.extension().is_some_and(|e| e == "log") {
			artifacts.insert(path.to_string_lossy().into_owned(), json!(digest(&path)?));
		}
	}
	r["artifacts"] = Value::Object(artifacts);
	Ok(r)
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
	if args.first().map(String::as_str) == Some("cross") {
		return crate::cross::main(&args[1..], root, &std::env::vars().collect());
	}
	if args.first().map(String::as_str) == Some("fixed-route") {
		return crate::fixed_route_stage::main(&args[1..], root, &std::env::vars().collect());
	}
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
			let record = || -> Result<String> {
				Ok(crate::selection::record(root, "HEAD")?["baseline"]
					.as_str()
					.context("record without baseline")?
					.to_owned())
			};
			let r = receipt(
				&stage,
				&out,
				|| {
					suite(
						&stage,
						&out,
						root,
						&record,
						&env,
						&mut Live { env: env.clone() },
					)
				},
				|| {
					crate::contracts::run(
						root,
						&out,
						&env,
						&crate::contracts::Live { env: env.clone() },
					)
				},
			)?;
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
		Some("cli") => {
			let binary = PathBuf::from(value("--binary").context("the CLI stage needs --binary")?);
			std::fs::create_dir_all(out.parent().context("--out has no parent")?)?;
			std::fs::create_dir(&out)?;
			let started = std::time::Instant::now();
			let mut result = cli(
				&binary,
				&out,
				root,
				&mut LiveCli {
					env: env.clone(),
					out: out.clone(),
				},
			)?;
			result["seconds"] = json!(started.elapsed().as_secs_f64());
			publish(&out.join("RESULT.json"), &result, &env)?;
			Ok(if result["status"] == "PASS" { 0 } else { 1 })
		}
		Some("inventories") => {
			let preflight = args.iter().any(|a| a == "--preflight");
			std::fs::create_dir_all(out.parent().context("--out has no parent")?)?;
			std::fs::create_dir(&out)?;
			let started = std::time::Instant::now();
			let jobs = build_jobs(&env)?;
			let mut result = inventories(
				&out,
				root,
				&env,
				jobs,
				preflight,
				&mut LiveInventory { env: env.clone() },
			)?;
			result["seconds"] = json!(started.elapsed().as_secs_f64());
			publish(&out.join("RESULT.json"), &result, &env)?;
			println!(
				"{} {}",
				result["status"].as_str().unwrap_or("FAIL"),
				out.join("RESULT.json").display()
			);
			Ok(if result["status"] == "PASS" { 0 } else { 1 })
		}
		Some("jupyter") => {
			std::fs::create_dir_all(out.parent().context("--out has no parent")?)?;
			std::fs::create_dir(&out)?;
			let groups = match env.get("RNX_FINAL_GROUPS").filter(|g| !g.is_empty()) {
				Some(g) => PathBuf::from(g),
				None => {
					let g = out.join("groups");
					std::fs::create_dir(&g)?;
					g
				}
			};
			let r = jupyter(&out, root, &env, &build_jobs, &mut LiveChild { groups });
			publish(&out.join("RESULT.json"), &r, &env)?;
			println!(
				"{} {}",
				r["status"].as_str().unwrap_or("FAIL"),
				out.join("RESULT.json").display()
			);
			Ok(if r["status"] == "PASS" { 0 } else { 1 })
		}
		Some(other) => bail!(value_error(&format!("unknown stage mode: {other}"))),
		None => bail!(value_error("a stage needs a mode")),
	}
}
