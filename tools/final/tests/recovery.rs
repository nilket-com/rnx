//! Recovery after a final is killed outright: every stage's descendants,
//! detached ones included, are gone before any root is released; and a
//! guardian killed without its completion record makes recovery refuse.
mod common;
use anyhow::Result;
use rnx_final::controller::{self, Hooks, Settings};
use rnx_final::evidence::{self, Ledger};
use rnx_final::failure::Environment;
use rnx_final::guardian::{self, Process, read_record, start_ticks};
use rnx_final::receipt::invocation_id;
use rnx_final::scheduler::{Resources, Targets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct OneStage;
impl Hooks for OneStage {
	fn disk_check(&self, _: &Path, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn select(
		&self,
		_: &Path,
		_: &Path,
		_: Option<&[String]>,
		_: &str,
		_: &Environment,
	) -> Result<Value> {
		Ok(common::selection(
			"explicit",
			json!("c"),
			json!([["a", ["python3", "test_a.py"]]]),
		))
	}
	fn check_selection(&self, _: &Value, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn finalize(
		&self,
		_: &Value,
		_: &Map<String, Value>,
		_: &Path,
		_: &Environment,
	) -> Result<Value> {
		Ok(common::finalization())
	}
	fn resources(&self, name: &str, _: &[String], _: &Path, _: &Targets) -> Result<Resources> {
		Ok([name.to_string()].into())
	}
	fn weight(&self, _: &str, _: &[String], _: &Path) -> Result<usize> {
		Ok(1)
	}
	fn tools_identity(&self, _: &Settings) -> Result<Value> {
		Ok(json!({}))
	}
}

fn checkout(label: &str) -> PathBuf {
	let root = std::env::temp_dir().join(format!("rnx-final-recovery-{label}-{}", invocation_id()));
	std::fs::create_dir(&root).unwrap();
	let root = root.canonicalize().unwrap();
	assert!(
		std::process::Command::new("git")
			.arg("-C")
			.arg(&root)
			.args(["init", "-q"])
			.output()
			.unwrap()
			.status
			.success()
	);
	std::fs::write(root.join(".gitignore"), "*\n").unwrap();
	root
}

fn wait_for(path: &Path) {
	for _ in 0..1000 {
		if std::fs::metadata(path).is_ok_and(|m| m.len() > 0) {
			return;
		}
		std::thread::sleep(Duration::from_millis(10));
	}
	panic!("{} never appeared", path.display());
}

/// Run as the killed controller when the variable is set.
fn controller_child() -> bool {
	let Ok(root) = std::env::var("RNX_RECOVERY_CONTROLLER") else {
		return false;
	};
	let root = PathBuf::from(root);
	let settings = Settings {
		root: root.clone(),
		out: root.join("final"),
		names: Some(vec!["a".into()]),
		scope: "explicit".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: Arc::new(OneStage),
		tokens: Some(2),
	};
	let _ = controller::run(&settings);
	true
}

fn start_controller(test: &str, root: &Path) -> std::process::Child {
	let pids = root.join("pids");
	std::fs::write(root.join("test_a.py"), format!("import subprocess, time\np = subprocess.Popen(['sleep', '1000'], start_new_session=True)\nopen({:?}, 'w').write(str(p.pid))\ntime.sleep(1000)\n", pids.to_string_lossy())).unwrap();
	std::process::Command::new(std::env::current_exe().unwrap())
		.args(["--exact", test, "--nocapture", "--test-threads=1"])
		.env("RNX_RECOVERY_CONTROLLER", root)
		.spawn()
		.unwrap()
}

fn ledger(root: &Path) -> (PathBuf, Ledger) {
	let registry = root.join("target/final-owners");
	let path = std::fs::read_dir(&registry)
		.unwrap()
		.flatten()
		.map(|e| e.path())
		.find(|p| p.to_string_lossy().ends_with(".owner.json"))
		.unwrap();
	(
		path.clone(),
		serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap(),
	)
}

fn guard_record(groups: &Path) -> PathBuf {
	std::fs::read_dir(groups)
		.unwrap()
		.flatten()
		.map(|e| e.path())
		.find(|p| {
			p.file_name()
				.unwrap()
				.to_string_lossy()
				.starts_with("GUARD-")
		})
		.unwrap()
}

#[test]
fn a_killed_controller_is_recovered_only_after_its_stage_is_drained() {
	if controller_child() {
		return;
	}
	let root = checkout("killed");
	let mut controller = start_controller(
		"a_killed_controller_is_recovered_only_after_its_stage_is_drained",
		&root,
	);
	wait_for(&root.join("pids"));
	let grandchild: i32 = std::fs::read_to_string(root.join("pids"))
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	let grandchild = Process {
		pid: grandchild,
		started: start_ticks(grandchild).unwrap(),
	};
	controller.kill().unwrap();
	controller.wait().unwrap();
	let (_, row) = ledger(&root);
	let routing = PathBuf::from(&row.roots[0].path);
	assert!(routing.exists());
	// Recovery drains first: when the drain step runs, the root is still there.
	let registry = root.join("target/final-owners");
	let drained_with_root = std::sync::Mutex::new(None);
	evidence::recover(&registry, |groups, ledger| {
		guardian::drain_registry(groups, ledger)?;
		*drained_with_root.lock().unwrap() = Some((guardian::alive(&grandchild), routing.exists()));
		Ok(())
	})
	.unwrap();
	assert_eq!(
		*drained_with_root.lock().unwrap(),
		Some((false, true)),
		"(grandchild alive, root present) when the drain returned"
	);
	assert!(!routing.exists(), "the root is released after the drain");
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_guardian_killed_without_its_record_makes_recovery_refuse() {
	if controller_child() {
		return;
	}
	let root = checkout("guardian");
	let mut controller = start_controller(
		"a_guardian_killed_without_its_record_makes_recovery_refuse",
		&root,
	);
	wait_for(&root.join("pids"));
	let (_, row) = ledger(&root);
	let groups = PathBuf::from(&row.groups);
	let record = read_record(&guard_record(&groups)).unwrap();
	let grandchild: i32 = std::fs::read_to_string(root.join("pids"))
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	let grandchild = Process {
		pid: grandchild,
		started: start_ticks(grandchild).unwrap(),
	};
	// Pin all three fixture processes while the stage is waiting.
	let stage = record.stage.value().unwrap();
	let processes = [&record.guardian, stage, &grandchild];
	let handles = processes.map(|p| guardian::pidfd(p.pid).unwrap());
	// The guardian dies outright first, then the controller: nothing drains.
	assert!(guardian::signal(&record.guardian, libc::SIGKILL).unwrap());
	controller.kill().unwrap();
	controller.wait().unwrap();
	let routing = PathBuf::from(&row.roots[0].path);
	let error =
		evidence::recover(&root.join("target/final-owners"), guardian::drain_registry).unwrap_err();
	assert!(
		error.to_string().contains("stage drain incomplete"),
		"{error:#}"
	);
	assert!(
		routing.exists(),
		"no root is released without a completed drain or an ended unit"
	);
	// Recovery already drained the stage group. This fixture also owns the
	// detached sleep. Wait for every writer before releasing its ledger.
	assert!(guardian::signal(&grandchild, libc::SIGKILL).unwrap());
	let receipts = evidence::recover(&root.join("target/final-owners"), |_, _| {
		for handle in &handles {
			anyhow::ensure!(guardian::wait_exit(handle, Some(Duration::from_secs(10)))?);
		}
		Ok(())
	})
	.unwrap();
	assert_eq!(receipts.len(), 1);
	for entry in row.roots {
		assert!(
			!Path::new(&entry.path).try_exists().unwrap(),
			"{}",
			entry.path
		);
	}
	std::fs::remove_dir_all(&root).unwrap();
}
