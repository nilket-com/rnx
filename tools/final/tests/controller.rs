//! Ports of the Python controller's order, failure, release and wall-budget
//! controls (test_final_evidence.FinalOrderControls, ReviewControls,
//! test_final_successor.WallBudget, test_failure_detail exercise_release),
//! with fake stages behind the same seams the Python controls patch.
mod common;
use anyhow::Result;
use rnx_final::child::Done;
use rnx_final::controller::{self, Hooks, Settings, StageCall};
use rnx_final::evidence::{self, Owners};
use rnx_final::failure::{self, Environment};
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::scheduler::{Resources, Targets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Dir(PathBuf);
impl Dir {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!("rnx-final-controller-{}", invocation_id()));
		std::fs::create_dir(&path).unwrap();
		Self(path.canonicalize().unwrap())
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

type StageFn = dyn Fn(&StageCall) -> Result<Done> + Send + Sync;

#[derive(Default)]
struct Fake {
	stages: Vec<&'static str>,
	scope: &'static str,
	stage: Option<Box<StageFn>>,
	finalize: Option<Box<dyn Fn() -> Result<Value> + Send + Sync>>,
	fail_release: bool,
	check_failures: Mutex<Vec<bool>>,
	clock: Option<Arc<Mutex<f64>>>,
	release_seconds: f64,
	events: Mutex<Vec<String>>,
}

impl Hooks for Fake {
	fn head(&self, _: &Path) -> Option<String> {
		Some("c".into())
	}
	fn disk_check(&self, _: &Path, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn select(
		&self,
		_: &Path,
		_: &Path,
		_: Option<&[String]>,
		scope: &str,
		_: &Environment,
	) -> Result<Value> {
		let stages: Vec<Value> = self
			.stages
			.iter()
			.map(|s| json!([s, ["command"]]))
			.collect();
		Ok(common::selection(
			if self.scope.is_empty() {
				scope
			} else {
				self.scope
			},
			json!("c"),
			json!(stages),
		))
	}
	fn check_selection(&self, _: &Value, _: &Path, _: &Environment) -> Result<()> {
		let mut script = self.check_failures.lock().unwrap();
		if !script.is_empty() && script.remove(0) {
			anyhow::bail!(rnx_final::child::value_error("dirty after stage"));
		}
		Ok(())
	}
	fn finalize(
		&self,
		_: &Value,
		_: &Map<String, Value>,
		_: &Path,
		_: &Environment,
	) -> Result<Value> {
		self.events.lock().unwrap().push("validated".into());
		match &self.finalize {
			Some(f) => f(),
			None => Ok(common::finalization()),
		}
	}
	fn resources(&self, _: &str, _: &[String], _: &Path, _: &Targets) -> Result<Resources> {
		Ok(["fake".to_string()].into())
	}
	fn weight(&self, _: &str, _: &[String], _: &Path) -> Result<usize> {
		Ok(1)
	}
	fn tools_identity(&self, _: &Settings) -> Result<Value> {
		Ok(json!({}))
	}
	fn release(&self, owner: &mut Owners) -> Result<Value> {
		assert!(
			owner.out.join("ACCEPTANCE.json").exists() && owner.out.join("MANIFEST.json").exists()
		);
		self.events.lock().unwrap().push("release".into());
		if let Some(clock) = &self.clock {
			*clock.lock().unwrap() += self.release_seconds;
		}
		if self.fail_release {
			anyhow::bail!("release failed");
		}
		owner.release()
	}
	fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
		if let Some(clock) = &self.clock {
			*clock.lock().unwrap() = 59.0 * 60.0;
		}
		self.stage.as_ref().map(|f| f(call))
	}
	fn elapsed(&self, started: Instant) -> f64 {
		match &self.clock {
			Some(clock) => *clock.lock().unwrap(),
			None => started.elapsed().as_secs_f64(),
		}
	}
}

fn settings(
	root: &Path,
	scope: &str,
	names: Option<Vec<String>>,
	hooks: Arc<dyn Hooks>,
	started: Instant,
) -> Settings {
	Settings {
		root: root.to_owned(),
		out: root.join("final"),
		names,
		scope: scope.into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started,
		bootstrap: None,
		unit: None,
		hooks,
		tokens: Some(2),
	}
}

/// A stage that leaves a payload in its routing root and a receipt.
fn passing_stage(fail: bool) -> Box<StageFn> {
	Box::new(move |call: &StageCall| {
		let routing = PathBuf::from(&call.environment["RNX_FINAL_ROUTING_ROOT"]);
		std::fs::write(routing.join("payload"), "data")?;
		std::fs::create_dir(&call.directory)?;
		let receipt = call.directory.join("RESULT.json");
		let payload = routing.join("payload");
		atomic_json(
			&receipt,
			&common::stage_receipt(
				!fail,
				fail as i32,
				json!([]),
				json!({payload.to_string_lossy(): digest(&payload)?}),
			),
		)?;
		Ok(Done {
			returncode: fail as i32,
			validation: json!({"path": receipt, "sha256": digest(&receipt)?}),
			declared_output: None,
		})
	})
}

fn read(path: &Path) -> Value {
	serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn exercise(fail_stage: bool, fail_release: bool, names: Option<Vec<String>>) {
	let dir = Dir::new();
	let fake = Arc::new(Fake {
		stages: vec!["stage"],
		scope: if names.is_some() {
			"explicit"
		} else {
			"required"
		},
		stage: Some(passing_stage(fail_stage)),
		fail_release,
		..Default::default()
	});
	let scope = if names.is_some() {
		"explicit"
	} else {
		"required"
	};
	let r = controller::run(&settings(
		&dir.0,
		scope,
		names.clone(),
		fake.clone(),
		Instant::now(),
	))
	.unwrap();
	let out = dir.0.join("final");
	assert_eq!(r["complete_final"], false);
	assert_eq!(
		r["publication_eligible"],
		json!(names.is_none() && !(fail_stage || fail_release)),
		"{r}"
	);
	assert_eq!(
		r["checks"],
		json!([{"name": "stage", "exit": fail_stage as i32}])
	);
	assert_eq!(
		r["status"],
		if fail_stage || fail_release {
			"FAIL"
		} else {
			"PASS"
		}
	);
	if !fail_release {
		assert_eq!(
			std::fs::read_dir(dir.0.join("target/routing-cli"))
				.unwrap()
				.count(),
			0,
			"routing roots released"
		);
	}
	if fail_stage {
		assert_eq!(
			r["error"],
			"producer returned FAIL without an error message"
		);
		assert!(
			std::fs::metadata(out.join("DIAGNOSTICS.json"))
				.unwrap()
				.len() <= 4 * 1024 * 1024
		);
		evidence::abandon_diagnostics(&out).unwrap();
		assert!(!out.join("DIAGNOSTICS.json").exists());
	} else {
		assert_eq!(*fake.events.lock().unwrap(), vec!["validated", "release"]);
	}
	if r["status"] == "PASS" {
		assert_eq!(evidence::revalidate(&out).unwrap()["status"], "PASS");
	}
}

#[test]
fn pass_order() {
	exercise(false, false, None);
}
#[test]
fn a_stage_failure_comes_first() {
	exercise(true, true, None);
}
#[test]
fn a_failure_releases() {
	exercise(true, false, None);
}
#[test]
fn a_release_failure_fails_a_pass() {
	exercise(false, true, None);
}
#[test]
fn a_selected_scope_is_not_a_complete_final() {
	exercise(false, false, Some(vec!["stage".into()]));
}

#[test]
fn snapshot_errors_do_not_skip_release_and_a_passing_manifest_failure_keeps_bulk() {
	struct Broken;
	impl Hooks for Broken {
		fn compact(
			&self,
			_: &Value,
			_: &Map<String, Value>,
			_: &Owners,
			_: &Value,
		) -> Result<Value> {
			anyhow::bail!("KeyError('seconds')")
		}
		fn diagnostics(
			&self,
			_: &Path,
			_: &[evidence::Root],
			_: &str,
			_: &Environment,
		) -> Result<()> {
			anyhow::bail!("OSError('snapshot')")
		}
	}
	for status in ["FAIL", "PASS"] {
		let dir = Dir::new();
		let mut owner = Owners::new(&dir.0.join("registry"), &dir.0.join("out")).unwrap();
		let p = owner.assign(&dir.0, "stage", None).unwrap();
		let mut s = settings(&dir.0, "required", None, Arc::new(Broken), Instant::now());
		s.out = dir.0.join("out");
		let mut acceptance = json!({"status": status, "candidate": "c", "error": "original failure", "acceptance_scope": "required", "mode": "ordinary", "checks": [], "seconds": 0.0, "memory": {"tokens": "2", "events_before": null, "events_after": null}});
		let r = controller::terminal(
			&s,
			Some(&mut owner),
			Some(&json!({"selected": true})),
			&Map::new(),
			&json!({}),
			&mut acceptance,
		);
		if status == "FAIL" {
			assert_eq!(r["status"], "PASS");
			assert!(!p.exists());
			let stored = read(&dir.0.join("out/ACCEPTANCE.json"));
			assert_eq!(stored["error"], "original failure");
			assert!(
				stored.get("manifest_error").is_some() && stored.get("diagnostics_error").is_some()
			);
		} else {
			assert_eq!(r["status"], "FAIL");
			assert!(
				p.exists(),
				"a passing final without its manifest keeps its bulk"
			);
		}
	}
}

#[test]
fn diagnostics_redact_values_and_share_a_stage_budget() {
	let dir = Dir::new();
	let secret = "VERY_PRIVATE_ENV_SENTINEL_0260";
	let mut entries = Vec::new();
	for name in ["one", "two"] {
		let p = dir.0.join(name);
		std::fs::create_dir(&p).unwrap();
		for i in 0..20 {
			std::fs::write(
				p.join(format!("{i}.stderr")),
				format!("{secret}{}\n", " x".repeat(1000)).repeat(20),
			)
			.unwrap();
		}
		entries.push(evidence::Root {
			path: p.to_string_lossy().into_owned(),
			parent: String::new(),
			stage: "same-stage".into(),
			state: "owned".into(),
			parent_device: 0,
			parent_inode: 0,
			device: Default::default(),
			inode: Default::default(),
			creation: Default::default(),
		});
	}
	let environment: Environment = [("RNX_SECRET".to_string(), secret.to_string())].into();
	controller::diagnostics(&dir.0, &entries, secret, &environment).unwrap();
	let text = std::fs::read_to_string(dir.0.join("DIAGNOSTICS.json")).unwrap();
	assert!(!text.contains(secret));
	let files = read(&dir.0.join("DIAGNOSTICS.json"))["files"]
		.as_array()
		.unwrap()
		.clone();
	// The Python controller computes the same general diagnostics.
	let script = "import json, sys; sys.path.insert(0, sys.argv[3]); import final; a = json.loads(sys.argv[1]); from pathlib import Path; final.diagnostics(Path(sys.argv[2]), a, a and 'VERY_PRIVATE_ENV_SENTINEL_0260', {'RNX_SECRET': 'VERY_PRIVATE_ENV_SENTINEL_0260'})";
	let python_out = dir.0.join("python");
	std::fs::create_dir(&python_out).unwrap();
	let column = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/column");
	let rows: Vec<Value> = entries
		.iter()
		.map(|e| json!({"path": e.path, "stage": e.stage}))
		.collect();
	let output = std::process::Command::new("python3")
		.args(["-B", "-c", script, &json!(rows).to_string()])
		.arg(&python_out)
		.arg(&column)
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		std::fs::read(python_out.join("DIAGNOSTICS.json")).unwrap(),
		std::fs::read(dir.0.join("DIAGNOSTICS.json")).unwrap()
	);
	assert!(files.len() > 1);
}

#[test]
fn a_clean_check_failing_after_a_stage_fails_the_final() {
	let dir = Dir::new();
	// The first check passes before the stage; the one after it finds the tree dirty.
	let fake = Arc::new(Fake {
		stages: vec!["stage"],
		stage: Some(passing_stage(false)),
		check_failures: Mutex::new(vec![false, false, true]),
		..Default::default()
	});
	let r = controller::run(&settings(&dir.0, "required", None, fake, Instant::now())).unwrap();
	assert_eq!(r["status"], "FAIL");
	assert!(
		r["error"].as_str().unwrap().contains("dirty after stage"),
		"{}",
		r["error"]
	);
}

#[test]
fn the_wall_budget_counts_release_and_fails_only_a_complete_final() {
	assert!(
		controller::wall_budget_error("complete", 61.0 * 60.0)
			.unwrap()
			.contains("took 61.0 minutes; its budget is 60")
	);
	assert!(controller::wall_budget_error("complete", 59.0 * 60.0).is_none());
	for scope in ["required", "explicit"] {
		assert!(controller::wall_budget_error(scope, 10.0 * 3600.0).is_none());
	}
	for (release, pass) in [(0.0, true), (120.0, false)] {
		let dir = Dir::new();
		// The stage ends at 59 minutes; release takes `release` seconds more.
		let fake = Arc::new(Fake {
			stages: vec!["only"],
			scope: "complete",
			stage: Some(passing_stage(false)),
			clock: Some(Arc::new(Mutex::new(0.0))),
			release_seconds: release,
			..Default::default()
		});
		let s = settings(&dir.0, "complete", None, fake, Instant::now());
		let r = controller::run(&s).unwrap();
		assert_eq!(r["status"] == "PASS", pass, "{r}");
		assert_eq!(r["complete_final"] == true, pass);
		if !pass {
			assert_eq!(r["publication_eligible"], false);
			assert!(
				r["wall_budget_error"].as_str().unwrap().contains("took 61"),
				"{r}"
			);
			let projection = failure::projection(r.as_object().unwrap(), &Environment::new());
			assert!(projection["error"].as_str().unwrap().contains("took 61"));
		}
	}
}

#[test]
fn a_failed_stage_cause_survives_release() {
	for nested in [false, true] {
		let dir = Dir::new();
		let stage: Box<StageFn> = Box::new(move |call: &StageCall| {
			let bulk = PathBuf::from(&call.environment["RNX_FINAL_ROUTING_ROOT"]);
			let case = bulk.join("case");
			std::fs::create_dir(&case)?;
			atomic_json(
				&case.join("RESULT.json"),
				&json!({"status": "FAIL", "error": "THE ACTUAL CHILD CAUSE"}),
			)?;
			std::fs::write(case.join("child.log"), "THE FAILING CHILD LOG\n")?;
			let semantic = bulk.join("RESULT.json");
			if nested {
				atomic_json(
					&semantic,
					&json!({"status": "FAIL", "error": "wrapper failed", "records": [case.join("RESULT.json")]}),
				)?;
				let reference = failure::retain(
					&bulk,
					"capture",
					Some(&semantic),
					&[],
					std::slice::from_ref(&bulk),
					&Environment::new(),
					"stage failed",
				)?;
				atomic_json(
					&semantic,
					&json!({"status": "FAIL", "failure": reference, "error": "wrapper failed"}),
				)?;
			} else {
				atomic_json(
					&semantic,
					&json!({"status": "FAIL", "error": "THE ACTUAL CHILD CAUSE"}),
				)?;
			}
			std::fs::write(&call.log, "THE FAILING CHILD LOG\n")?;
			std::fs::create_dir(&call.directory)?;
			atomic_json(
				&call.directory.join("CHILD.json"),
				&json!({"receipt": semantic}),
			)?;
			let result = call.directory.join("RESULT.json");
			let mut row = common::stage_receipt(false, 1, json!([]), json!({}));
			row["error"] = json!("child exit is nonzero");
			atomic_json(&result, &row)?;
			Ok(Done {
				returncode: 1,
				validation: json!({"path": result, "sha256": digest(&result)?}),
				declared_output: None,
			})
		});
		struct NoCompact(Fake);
		impl Hooks for NoCompact {
			fn head(&self, r: &Path) -> Option<String> {
				self.0.head(r)
			}
			fn disk_check(&self, a: &Path, b: &Path, c: &Environment) -> Result<()> {
				self.0.disk_check(a, b, c)
			}
			fn select(
				&self,
				a: &Path,
				b: &Path,
				c: Option<&[String]>,
				d: &str,
				e: &Environment,
			) -> Result<Value> {
				self.0.select(a, b, c, d, e)
			}
			fn check_selection(&self, a: &Value, b: &Path, c: &Environment) -> Result<()> {
				self.0.check_selection(a, b, c)
			}
			fn resources(&self, a: &str, b: &[String], c: &Path, d: &Targets) -> Result<Resources> {
				self.0.resources(a, b, c, d)
			}
			fn weight(&self, a: &str, b: &[String], c: &Path) -> Result<usize> {
				self.0.weight(a, b, c)
			}
			fn tools_identity(&self, s: &Settings) -> Result<Value> {
				self.0.tools_identity(s)
			}
			fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
				self.0.stage(call)
			}
			fn compact(
				&self,
				_: &Value,
				_: &Map<String, Value>,
				_: &Owners,
				_: &Value,
			) -> Result<Value> {
				Ok(json!({"schema": 1, "candidate": "c", "roots": []}))
			}
		}
		let fake = Arc::new(NoCompact(Fake {
			stages: vec!["broken"],
			scope: "explicit",
			stage: Some(stage),
			..Default::default()
		}));
		let r = controller::run(&settings(
			&dir.0,
			"explicit",
			Some(vec!["broken".into()]),
			fake,
			Instant::now(),
		))
		.unwrap();
		let out = dir.0.join("final");
		assert_eq!(
			(r["status"].clone(), r["failed_stage"].clone()),
			(json!("FAIL"), json!("broken"))
		);
		assert_eq!(
			(
				r["publication_eligible"].clone(),
				r["complete_final"].clone()
			),
			(json!(false), json!(false))
		);
		assert_eq!(
			std::fs::read_dir(dir.0.join("target/routing-cli"))
				.unwrap()
				.count(),
			0
		);
		assert!(
			out.join("FAILURE.json").is_file(),
			"failure evidence was released"
		);
		assert_eq!(r["error"], "THE ACTUAL CHILD CAUSE");
		assert_eq!(read(&out.join("ACCEPTANCE.json"))["failed_stage"], "broken");
		let text: String = std::fs::read_dir(out.join("failure"))
			.unwrap()
			.flatten()
			.map(|e| std::fs::read_to_string(e.path()).unwrap())
			.collect();
		assert!(text.contains("THE FAILING CHILD LOG"));
		assert_eq!(read(&out.join("RELEASE.json"))["status"], "PASS");
	}
}
