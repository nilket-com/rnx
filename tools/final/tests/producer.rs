//! The producer side of the stage protocol in Rust (`producer::publish`,
//! `bind`), and a Rust stage run end to end through the child runner: its
//! contract, the controller's own executable and digest in its context, its
//! bound receipt, and the runner's validation.
use rnx_final::child::{self, CONTEXT, Run, evidence};
use rnx_final::failure::Environment;
use rnx_final::producer::{bind, publish};
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const GUARDIAN: &str = env!("CARGO_BIN_EXE_rnx-final");

struct Repo(PathBuf);
impl Drop for Repo {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// A committed checkout with an ignored output directory.
fn repo() -> Repo {
	let root = std::env::temp_dir().join(format!("rnx-final-producer-{}", invocation_id()));
	std::fs::create_dir(&root).unwrap();
	let root = root.canonicalize().unwrap();
	let git = |args: &[&str]| {
		assert!(
			std::process::Command::new("git")
				.arg("-C")
				.arg(&root)
				.args(args)
				.status()
				.unwrap()
				.success()
		)
	};
	git(&["init", "-q"]);
	std::fs::write(root.join(".gitignore"), "runs/\n").unwrap();
	git(&["add", ".gitignore"]);
	git(&[
		"-c",
		"user.email=t@t",
		"-c",
		"user.name=t",
		"commit",
		"-qm",
		"base",
	]);
	std::fs::create_dir(root.join("runs")).unwrap();
	Repo(root)
}

/// A context as the runner writes it, and the environment naming it.
fn context(root: &Path, existing: Value, resume: bool) -> (PathBuf, Environment) {
	let dir = root.join("runs").join(invocation_id());
	std::fs::create_dir_all(&dir).unwrap();
	let env: Environment = [("PATH".to_string(), std::env::var("PATH").unwrap())].into();
	let ctx = json!({"schema": 1, "invocation": invocation_id(), "root": root, "cwd": root, "command": ["x"],
		"candidate": child::head(root), "source": child::source(root).unwrap(), "environment": child::environment(&env).unwrap(),
		"kind": "producer", "result_directory": root.join("runs"), "existing_results": existing, "resume": resume});
	let path = dir.join("CONTEXT.json");
	atomic_json(&path, &ctx).unwrap();
	let mut bound = env;
	bound.insert(CONTEXT.into(), path.to_string_lossy().into_owned());
	(path, bound)
}

#[test]
fn a_bound_receipt_carries_its_context_and_one_envelope() {
	let r = repo();
	let (ctx, env) = context(&r.0, json!({}), false);
	let receipt = r.0.join("runs/RESULT.json");
	publish(&receipt, &json!({"status": "PASS"}), &env).unwrap();
	let value: Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
	let context: Value = serde_json::from_slice(&std::fs::read(&ctx).unwrap()).unwrap();
	assert_eq!(
		value["validation_context"],
		json!(rnx_final::pyjson::identity(&context).unwrap())
	);
	let envelope: Value =
		serde_json::from_slice(&std::fs::read(ctx.with_file_name("CHILD.json")).unwrap()).unwrap();
	assert_eq!(
		(envelope["status"].clone(), envelope["sha256"].clone()),
		(json!("PASS"), json!(digest(&receipt).unwrap()))
	);
	// One envelope only.
	assert!(
		bind(&receipt, &env)
			.unwrap_err()
			.to_string()
			.contains("already exists")
	);
}

#[test]
fn an_unbound_publish_writes_no_context_and_no_envelope() {
	let r = repo();
	let receipt = r.0.join("runs/RESULT.json");
	publish(&receipt, &json!({"status": "PASS"}), &Environment::new()).unwrap();
	let value: Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
	assert!(value.get("validation_context").is_none());
	assert!(!r.0.join("runs/CHILD.json").exists());
}

#[test]
fn a_prior_result_is_replaced_only_on_an_explicit_resume_of_an_unchanged_pass() {
	for (status, resume, changed, ok) in [
		("PASS", true, false, true),
		("PASS", false, false, false),
		("FAIL", true, false, false),
		("PASS", true, true, false),
	] {
		let r = repo();
		let receipt = r.0.join("runs/RESULT.json");
		atomic_json(&receipt, &json!({"status": status})).unwrap();
		let prior = digest(&receipt).unwrap();
		let (ctx, env) = context(&r.0, json!({receipt.to_string_lossy(): prior}), resume);
		if changed {
			atomic_json(&receipt, &json!({"status": status, "edited": true})).unwrap();
		}
		let result = publish(&receipt, &json!({"status": "PASS"}), &env);
		assert_eq!(
			result.is_ok(),
			ok,
			"{status} resume={resume} changed={changed}: {result:?}"
		);
		if ok {
			assert!(
				ctx.with_file_name(format!("PRIOR-{prior}.json")).is_file(),
				"the prior is archived"
			);
		}
	}
}

#[test]
fn a_changed_source_or_an_invalid_status_is_not_bound() {
	let r = repo();
	let (_, env) = context(&r.0, json!({}), false);
	std::fs::write(r.0.join("tracked.rs"), "changed").unwrap();
	let receipt = r.0.join("runs/RESULT.json");
	assert!(
		publish(&receipt, &json!({"status": "PASS"}), &env)
			.unwrap_err()
			.to_string()
			.contains("changed")
	);
	let r = repo();
	let (_, env) = context(&r.0, json!({}), false);
	let receipt = r.0.join("runs/RESULT.json");
	atomic_json(&receipt, &json!({"status": "MAYBE"})).unwrap();
	assert!(
		bind(&receipt, &env)
			.unwrap_err()
			.to_string()
			.contains("invalid producer status")
	);
}

#[test]
fn only_the_stage_modes_of_the_logical_program_are_producers() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let contract = |c: &[&str]| {
		child::contract(
			&c.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
			&root,
			&root,
		)
	};
	assert_eq!(
		contract(&["rnx-final", "stage", "suite", "--stage", "x", "--out", "o"]).unwrap(),
		"producer"
	);
	assert_eq!(
		contract(&["rnx-final", "stage", "clippy", "--out", "o"]).unwrap(),
		"producer"
	);
	for refused in [
		&["rnx-final", "stage", "other", "--out", "o"][..],
		&["rnx-final", "guardian"],
		&["rnx-final", "--complete", "--out", "o"],
		&[GUARDIAN, "stage", "suite", "--stage", "x", "--out", "o"],
		&[
			"/usr/local/bin/rnx-final",
			"stage",
			"suite",
			"--stage",
			"x",
			"--out",
			"o",
		],
	] {
		assert!(contract(refused).is_err(), "{refused:?}");
	}
}

/// A real format stage in this checkout: the runner gives the logical
/// program the controller's own executable, records its digest, and
/// validates the stage's bound receipt.
#[test]
fn a_rust_stage_runs_as_the_controllers_executable() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let runs = std::env::temp_dir().join(format!("rnx-final-stage-run-{}", invocation_id()));
	std::fs::create_dir_all(&runs).unwrap();
	let out = runs.join("format-package-site");
	let command: Vec<String> = [
		"rnx-final",
		"stage",
		"suite",
		"--stage",
		"format-package-site",
		"--out",
	]
	.iter()
	.map(|s| s.to_string())
	.chain([out.to_string_lossy().into_owned()])
	.collect();
	let env: Environment = std::env::vars().filter(|(k, _)| k != CONTEXT).collect();
	let done = child::run(Run {
		command: &command,
		cwd: &root,
		root: &root,
		env: &env,
		log: &runs.join("stage.log"),
		timeout: None,
		directory: &runs.join("attempt"),
		groups: &runs,
		guardian: Path::new(GUARDIAN),
		after: None,
		stop: None,
	})
	.unwrap();
	let log = std::fs::read_to_string(runs.join("stage.log")).unwrap_or_default();
	assert_eq!(done.returncode, 0, "{log}");
	let row = evidence(&done.validation, &json!(child::source(&root).unwrap())).unwrap();
	assert_eq!(row["context"]["kind"], "producer");
	let executable = &row["context"]["executable"];
	let program = Path::new(GUARDIAN).canonicalize().unwrap();
	assert_eq!(executable["path"], json!(program.to_string_lossy()));
	assert_eq!(executable["sha256"], json!(digest(&program).unwrap()));
	let semantic: Value =
		serde_json::from_slice(&std::fs::read(out.join("RESULT.json")).unwrap()).unwrap();
	assert_eq!(semantic["status"], "PASS");
	assert!(semantic["validation_context"].is_string());
	let _ = std::fs::remove_dir_all(&runs);
}

/// Run the real binary as a stage under a planted context.
fn stage_under(ctx: &Value, args: &[String]) -> (std::process::Output, PathBuf) {
	let dir = std::env::temp_dir().join(format!("rnx-final-bound-{}", invocation_id()));
	std::fs::create_dir_all(&dir).unwrap();
	let context = dir.join("CONTEXT.json");
	atomic_json(&context, ctx).unwrap();
	let output = std::process::Command::new(GUARDIAN)
		.arg("stage")
		.args(args)
		.env(CONTEXT, &context)
		.output()
		.unwrap();
	(output, dir)
}

/// Inside a final a Rust stage runs only as the executable, invocation and
/// output its context names: otherwise it refuses before any child, writes
/// no output directory and binds no envelope.
#[test]
fn a_stage_refuses_a_context_naming_another_executable_command_or_output() {
	let program = Path::new(GUARDIAN).canonicalize().unwrap();
	let base = std::env::temp_dir().join(format!("rnx-final-bound-out-{}", invocation_id()));
	let out = base.join("format-package-site");
	let args: Vec<String> = ["suite", "--stage", "format-package-site", "--out"]
		.iter()
		.map(|s| s.to_string())
		.chain([out.to_string_lossy().into_owned()])
		.collect();
	let command: Vec<String> = ["rnx-final", "stage"]
		.iter()
		.map(|s| s.to_string())
		.chain(args.iter().cloned())
		.collect();
	let good = json!({"executable": {"path": program, "sha256": digest(&program).unwrap()}, "command": command, "result_directory": out});
	let mut cases = vec![];
	let mut wrong_digest = good.clone();
	wrong_digest["executable"]["sha256"] = json!("0".repeat(64));
	cases.push(("another digest", wrong_digest));
	let mut wrong_path = good.clone();
	wrong_path["executable"]["path"] = json!("/usr/bin/true");
	cases.push(("another path", wrong_path));
	let mut missing = good.clone();
	missing.as_object_mut().unwrap().remove("executable");
	cases.push(("no executable", missing));
	let mut other_stage = good.clone();
	other_stage["command"][4] = json!("format-package-builder");
	cases.push(("another stage", other_stage));
	let mut other_out = good.clone();
	other_out["result_directory"] = json!(base.join("elsewhere"));
	cases.push(("another output", other_out));
	for (name, ctx) in cases {
		let (output, dir) = stage_under(&ctx, &args);
		assert!(
			!output.status.success(),
			"{name}: {}",
			String::from_utf8_lossy(&output.stdout)
		);
		assert!(!out.exists(), "{name}: the stage started");
		assert!(
			!dir.join("CHILD.json").exists(),
			"{name}: an envelope was bound"
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
	let _ = std::fs::remove_dir_all(&base);
}

/// A real Rust stage whose Cargo leaves a detached descendant behind is
/// stopped through its Stopper once that Cargo says it is ready, and its
/// guardian drains the descendant before the run returns.
#[test]
fn a_stopped_rust_stage_has_its_descendants_drained() {
	use std::os::fd::AsRawFd as _;
	use std::os::unix::fs::PermissionsExt as _;
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let runs = std::env::temp_dir().join(format!("rnx-final-stage-stop-{}", invocation_id()));
	let bin = runs.join("bin");
	std::fs::create_dir_all(&bin).unwrap();
	let pid_file = runs.join("descendant.pid");
	let ready = runs.join("ready.fifo");
	let fifo = std::ffi::CString::new(ready.to_string_lossy().as_bytes()).unwrap();
	// SAFETY: a valid path.
	assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
	// Read and write ends held here, so neither side's open blocks.
	let handshake = std::fs::OpenOptions::new()
		.read(true)
		.write(true)
		.open(&ready)
		.unwrap();
	// A fake Cargo: a detached descendant, its identity, readiness, then a
	// wait that only a stop ends.
	std::fs::write(
		bin.join("cargo"),
		format!(
			"#!/bin/sh\nsetsid sleep 1000 & echo $! > {}\necho ready > {}\nsleep 1000\n",
			pid_file.display(),
			ready.display()
		),
	)
	.unwrap();
	std::fs::set_permissions(bin.join("cargo"), std::fs::Permissions::from_mode(0o755)).unwrap();
	let out = runs.join("format-package-site");
	let command: Vec<String> = [
		"rnx-final",
		"stage",
		"suite",
		"--stage",
		"format-package-site",
		"--out",
	]
	.iter()
	.map(|s| s.to_string())
	.chain([out.to_string_lossy().into_owned()])
	.collect();
	let mut env: Environment = std::env::vars().filter(|(k, _)| k != CONTEXT).collect();
	let path = format!(
		"{}:{}",
		bin.display(),
		env.get("PATH").cloned().unwrap_or_default()
	);
	env.insert("PATH".into(), path);
	let stopper = child::Stopper::default();
	let done = std::thread::scope(|scope| {
		let run = scope.spawn(|| {
			child::run(Run {
				command: &command,
				cwd: &root,
				root: &root,
				env: &env,
				log: &runs.join("stage.log"),
				// The bound on the whole run, should the stop never come.
				timeout: Some(std::time::Duration::from_secs(120)),
				directory: &runs.join("attempt"),
				groups: &runs,
				guardian: Path::new(GUARDIAN),
				after: None,
				stop: Some(&stopper),
			})
		});
		// A bounded wait for readiness, on the handshake's descriptor.
		let mut poll = libc::pollfd {
			fd: handshake.as_raw_fd(),
			events: libc::POLLIN,
			revents: 0,
		};
		// SAFETY: one valid pollfd.
		let ready = unsafe { libc::poll(&mut poll, 1, 60_000) } == 1;
		stopper.stop();
		let done = run.join().unwrap().unwrap();
		assert!(ready, "the fake Cargo never became ready");
		done
	});
	assert_ne!(done.returncode, 0);
	let pid: i32 = std::fs::read_to_string(&pid_file)
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	assert!(
		rnx_final::guardian::start_ticks(pid).is_none(),
		"the descendant {pid} outlived its stage"
	);
	drop(handshake);
	let _ = std::fs::remove_dir_all(&runs);
}
