//! A signal to the controller drains every running stage and releases.
//! Its own test binary: the signal reaches every controller in the process.
mod common;
use anyhow::Result;
use rnx_final::controller::{self, Hooks, Settings};
use rnx_final::failure::Environment;
use rnx_final::receipt::invocation_id;
use rnx_final::scheduler::{Resources, Targets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Real;
impl Hooks for Real {
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
		let stages: Vec<Value> = ["a", "b"]
			.iter()
			.map(|n| json!([n, ["python3", format!("test_{n}.py")]]))
			.collect();
		Ok(common::selection("explicit", json!("c"), json!(stages)))
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
	fn compact(
		&self,
		_: &Value,
		_: &Map<String, Value>,
		_: &rnx_final::evidence::Owners,
		_: &Value,
	) -> Result<Value> {
		Ok(json!({"schema": 1, "candidate": "c", "roots": []}))
	}
}

#[test]
fn a_signal_drains_every_running_stage() {
	let root = std::env::temp_dir().join(format!("rnx-final-signals-{}", invocation_id()));
	std::fs::create_dir(&root).unwrap();
	let root = root.canonicalize().unwrap();
	let git = |args: &[&str]| {
		assert!(
			std::process::Command::new("git")
				.arg("-C")
				.arg(&root)
				.args(args)
				.output()
				.unwrap()
				.status
				.success()
		)
	};
	git(&["init", "-q"]);
	std::fs::write(root.join(".gitignore"), "*\n").unwrap();
	let pids = root.join("pids");
	for name in ["a", "b"] {
		// A detached grandchild as well: it must not outlive the drain.
		std::fs::write(root.join(format!("test_{name}.py")), format!("import os, subprocess, time\np = subprocess.Popen(['sleep', '1000'], start_new_session=True)\nopen({:?}, 'a').write(str(p.pid) + '\\n')\ntime.sleep(30)\n", pids.to_string_lossy())).unwrap();
	}
	let settings = Settings {
		root: root.clone(),
		out: root.join("final"),
		names: Some(vec!["a".into(), "b".into()]),
		scope: "explicit".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: Arc::new(Real),
		tokens: Some(4),
	};
	let started = Instant::now();
	let signaller = std::thread::spawn(|| {
		std::thread::sleep(Duration::from_secs(2));
		// SAFETY: a signal to this process, which the controller handles.
		unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
	});
	let r = controller::run(&settings).unwrap();
	signaller.join().unwrap();
	assert!(
		started.elapsed() < Duration::from_secs(20),
		"the stages were not stopped"
	);
	assert_eq!(r["status"], "FAIL");
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("interrupted by signal 15"),
		"{r}"
	);
	let mut exits: Vec<(String, i64)> = r["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| {
			(
				c["name"].as_str().unwrap().into(),
				c["exit"].as_i64().unwrap(),
			)
		})
		.collect();
	exits.sort();
	assert_eq!(exits.len(), 2);
	assert!(exits.iter().all(|(_, e)| *e != 0), "{exits:?}");
	assert!(!root.join("final/jobserver.fifo").exists());
	for pid in std::fs::read_to_string(&pids).unwrap().lines() {
		assert!(
			rnx_final::guardian::start_ticks(pid.parse().unwrap()).is_none(),
			"detached grandchild {pid} survived"
		);
	}
	assert_eq!(
		serde_json::from_slice::<Value>(&std::fs::read(root.join("final/RELEASE.json")).unwrap())
			.unwrap()["status"],
		"PASS"
	);
	// An interrupted final releases, is not complete, and keeps bounded diagnostics.
	assert_eq!(
		(
			r["complete_final"].clone(),
			r["publication_eligible"].clone()
		),
		(json!(false), json!(false))
	);
	assert!(
		std::fs::metadata(root.join("final/DIAGNOSTICS.json"))
			.unwrap()
			.len() <= 4 * 1024 * 1024
	);
	rnx_final::evidence::abandon_diagnostics(&root.join("final")).unwrap();
	assert!(!root.join("final/DIAGNOSTICS.json").exists());

	// Signals that arrive during the terminal release are ignored: it completes.
	struct SignalledRelease;
	impl Hooks for SignalledRelease {
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
				json!([["quick", ["python3", "test_quick.py"]]]),
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
		fn release(&self, owner: &mut rnx_final::evidence::Owners) -> Result<Value> {
			// SAFETY: signals to this process, which the controller handles.
			unsafe {
				libc::kill(libc::getpid(), libc::SIGTERM);
				libc::kill(libc::getpid(), libc::SIGINT);
			}
			std::thread::sleep(Duration::from_millis(200));
			owner.release()
		}
	}
	std::fs::write(root.join("test_quick.py"), "pass\n").unwrap();
	let mut quiet = settings.clone();
	quiet.out = root.join("final-quiet");
	quiet.names = Some(vec!["quick".into()]);
	quiet.hooks = Arc::new(SignalledRelease);
	let r = controller::run(&quiet).unwrap();
	assert_eq!(r["status"], "PASS", "{r}");
	assert_eq!(
		serde_json::from_slice::<Value>(
			&std::fs::read(root.join("final-quiet/RELEASE.json")).unwrap()
		)
		.unwrap()["status"],
		"PASS"
	);
	// A signal while a wide stage is refused its tokens: the running stage
	// stops, the wide one never starts, and every token is back at close
	// (a lease still out would be reported as the run's error).
	struct Refused;
	impl Hooks for Refused {
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
				json!([
					["a", ["python3", "test_a.py"]],
					["wide", ["python3", "test_quick.py"]]
				]),
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
		fn weight(&self, name: &str, _: &[String], _: &Path) -> Result<usize> {
			Ok(if name == "wide" { 4 } else { 1 })
		}
		fn tools_identity(&self, _: &Settings) -> Result<Value> {
			Ok(json!({}))
		}
		fn compact(
			&self,
			_: &Value,
			_: &Map<String, Value>,
			_: &rnx_final::evidence::Owners,
			_: &Value,
		) -> Result<Value> {
			Ok(json!({"schema": 1, "candidate": "c", "roots": []}))
		}
	}
	let mut refused = settings.clone();
	refused.out = root.join("final-refused");
	refused.names = Some(vec!["a".into(), "wide".into()]);
	refused.hooks = Arc::new(Refused);
	refused.tokens = Some(4);
	let started = Instant::now();
	let signaller = std::thread::spawn(|| {
		std::thread::sleep(Duration::from_secs(2));
		// SAFETY: a signal to this process, which the controller handles.
		unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
	});
	let r = controller::run(&refused).unwrap();
	signaller.join().unwrap();
	assert!(
		started.elapsed() < Duration::from_secs(20),
		"the refused final did not stop"
	);
	let error = r["error"].as_str().unwrap();
	assert!(
		error.contains("interrupted by signal 15") && !error.contains("tokens"),
		"{error}"
	);
	let names: Vec<&str> = r["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| c["name"].as_str().unwrap())
		.collect();
	assert_eq!(names, ["a"], "the refused stage started");
	assert!(!root.join("final-refused/jobserver.fifo").exists());
	let _ = std::fs::remove_dir_all(&root);
}
