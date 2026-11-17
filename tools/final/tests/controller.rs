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

fn sha256_hex(bytes: &[u8]) -> String {
	use sha2::Digest as _;
	rnx_final::pyjson::hex(&sha2::Sha256::digest(bytes))
}

#[test]
fn diagnostics_redact_values_and_share_a_stage_budget() {
	// The excerpts share a byte budget that the roots' path length enters, so
	// the roots sit at the witness's path length: /tmp/rnx-0284-d/ and 8 hex.
	let dir = {
		let path = PathBuf::from(format!("/tmp/rnx-0284-d/{}", &invocation_id()[..8]));
		std::fs::create_dir_all(&path).unwrap();
		Dir(path)
	};
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
	// The Python controller at ADR 0284's witness revision wrote these same
	// bytes for the same roots and failure, the roots' directory aside.
	let witness = common::witness::expected("diagnostics", &[]);
	let normalized = text.replace(&*dir.0.to_string_lossy(), "<DIR>");
	assert_eq!(
		json!({"sha256": sha256_hex(normalized.as_bytes()), "bytes": normalized.len(), "files": files.len()}),
		witness
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

/// The fake with the real compaction replaced, as the release test needs.
struct Released(Fake);
impl Hooks for Released {
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
	fn compact(&self, _: &Value, _: &Map<String, Value>, _: &Owners, _: &Value) -> Result<Value> {
		Ok(json!({"schema": 1, "candidate": "c", "roots": []}))
	}
}

const CENSUS_SECRET: &str = "planted-cross-secret-0292";

/// The first two executed census cases, by the census plan.
fn census_cases() -> (String, String) {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let plan =
		std::fs::read_to_string(root.join("tools/verify/fixtures/census/execution.tsv")).unwrap();
	let ids: Vec<String> = plan
		.lines()
		.skip(1)
		.filter(|l| l.split('\t').nth(1) == Some("execute"))
		.map(|l| l.split('\t').next().unwrap().to_owned())
		.take(2)
		.collect();
	(ids[0].clone(), ids[1].clone())
}

/// A census payload under `payload`: two differing cases, the first with
/// both engines' outputs (a secret in each stderr, `stderr_bytes` long),
/// the second with only its stock run.
fn plant_census(payload: &Path, first: &str, second: &str, stderr_bytes: usize) -> Result<()> {
	let live = payload.join("census/live-execution");
	for (case, engine) in [(first, "stock"), (first, "fresh"), (second, "stock")] {
		let run = live.join(case).join(engine);
		std::fs::create_dir_all(run.join("report"))?;
		std::fs::write(run.join("stdout"), format!("{case} {engine} stdout\n"))?;
		let line = format!("{case} {engine} stderr token={CENSUS_SECRET}\n");
		std::fs::write(
			run.join("stderr"),
			line.repeat(stderr_bytes / line.len() + 1),
		)?;
		std::fs::write(
			run.join("report/result.json"),
			format!("{{\"case\": \"{case}\", \"engine\": \"{engine}\"}}\n"),
		)?;
	}
	let summary = json!({"executed": 71, "admission_only": [], "differences": [
		{"id": first, "reason": "live observations differ"},
		{"id": second, "reason": format!("census execution {second} fresh failed: exit status: 124")},
	]});
	std::fs::write(
		live.join("RESULT.json"),
		serde_json::to_vec_pretty(&summary)?,
	)?;
	Ok(())
}

/// The launcher's receipt for `payload`, written by `verify.terminal_receipt`.
fn census_receipt(payload: &Path) -> Result<PathBuf> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()?;
	let script = format!(
		"import importlib.util, sys\nfrom pathlib import Path\nsys.path.insert(0, {column:?})\nspec = importlib.util.spec_from_file_location('verify_launcher', {verify:?})\nverify = importlib.util.module_from_spec(spec); spec.loader.exec_module(verify)\nimport child_result\nresult = verify.terminal_receipt(Path({payload:?}), RuntimeError('cross verification failed'), {{'status': 'PASS'}})\nchild_result.atomic_json(Path({payload:?}) / 'RESULT.json', result)\n",
		column = root.join("tools/column").to_string_lossy(),
		verify = root.join("tools/verify.py").to_string_lossy(),
		payload = payload.to_string_lossy(),
	);
	anyhow::ensure!(
		std::process::Command::new("/usr/bin/python3")
			.args(["-B", "-c", &script])
			.status()?
			.success()
	);
	Ok(payload.join("RESULT.json"))
}

/// A census failure through the final (ADR 0292): the launcher's real
/// receipt over a planted payload, the child bundle `child::run` builds
/// from it, the controller's handoff to `copy_bundle`, and the owned root's
/// release. What survives names both cases and reasons and holds the first
/// case's labelled stock and fresh evidence, redacted; with oversized
/// stderr, the envelope's caps hold and say what they dropped.
#[test]
fn a_census_failure_keeps_its_cases_through_the_final() {
	for stderr_bytes in [64, 200 * 1024] {
		let dir = Dir::new();
		let (first, second) = census_cases();
		let (f, s) = (first.clone(), second.clone());
		let stage: Box<StageFn> = Box::new(move |call: &StageCall| {
			let bulk = PathBuf::from(&call.environment["RNX_FINAL_ROUTING_ROOT"]);
			let payload = bulk.join("payload");
			std::fs::create_dir(&payload)?;
			plant_census(&payload, &f, &s, stderr_bytes)?;
			let semantic = census_receipt(&payload)?;
			std::fs::write(&call.log, "cross verification failed\n")?;
			std::fs::create_dir(&call.directory)?;
			atomic_json(
				&call.directory.join("CHILD.json"),
				&json!({"receipt": semantic, "sha256": digest(&semantic)?}),
			)?;
			// The child bundle, as child::run builds it for a failed producer.
			let env: Environment = [("CROSS_TOKEN".to_string(), CENSUS_SECRET.to_string())].into();
			let reference = failure::retain(
				&call.directory,
				"cross-child",
				Some(&semantic),
				std::slice::from_ref(&call.log),
				&[call.directory.clone(), bulk.clone()],
				&env,
				"child exit is nonzero",
			)?;
			let result = call.directory.join("RESULT.json");
			let mut row = common::stage_receipt(false, 1, json!([]), json!({}));
			row["error"] = reference["error"].clone();
			row["failure"] = reference;
			atomic_json(&result, &row)?;
			Ok(Done {
				returncode: 1,
				validation: json!({"path": result, "sha256": digest(&result)?}),
				declared_output: None,
			})
		});
		let fake = Arc::new(Released(Fake {
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
		// The owned root is released; the evidence is the final's own.
		assert_eq!(
			std::fs::read_dir(dir.0.join("target/routing-cli"))
				.unwrap()
				.count(),
			0
		);
		assert_eq!(read(&out.join("RELEASE.json"))["status"], "PASS");
		let index = read(&out.join("FAILURE.json"));
		let error = index["error"].as_str().unwrap();
		assert!(
			error.contains("(2 cases)")
				&& error.contains(&format!("{first}: live observations differ")),
			"{error}"
		);
		assert!(
			error.contains(&format!(
				"{second}: census execution {second} fresh failed: exit status: 124"
			)),
			"{error}"
		);
		let files = index["files"].as_array().unwrap();
		let total: u64 = files.iter().map(|f| f["bytes"].as_u64().unwrap()).sum();
		assert!(
			files.len() <= 8 && total as usize <= failure::BUNDLE_CAP,
			"{index}"
		);
		let on_disk = std::fs::read_dir(out.join("failure")).unwrap().count();
		assert_eq!(on_disk, files.len(), "an excerpt without an index row");
		let label = |name: &str| files.iter().find(|f| f["source_name"] == name).cloned();
		let live = "census/live-execution";
		assert!(label(&format!("{live}/RESULT.json")).is_some(), "{index}");
		for engine in ["stock", "fresh"] {
			let row = label(&format!("{live}/{first}/{engine}/report/result.json"))
				.unwrap_or_else(|| panic!("{engine} report: {index}"));
			let text = std::fs::read_to_string(out.join(row["path"].as_str().unwrap())).unwrap();
			assert!(text.contains(&first) && text.contains(engine), "{text}");
		}
		let all: String = std::fs::read_dir(out.join("failure"))
			.unwrap()
			.flatten()
			.map(|e| std::fs::read_to_string(e.path()).unwrap())
			.collect();
		assert!(!all.contains(CENSUS_SECRET), "the secret survived");
		if stderr_bytes < 1024 {
			for engine in ["stock", "fresh"] {
				let row = label(&format!("{live}/{first}/{engine}/stderr"))
					.unwrap_or_else(|| panic!("{engine} stderr: {index}"));
				let text =
					std::fs::read_to_string(out.join(row["path"].as_str().unwrap())).unwrap();
				assert!(text.contains("<redacted>"), "{text}");
			}
			assert!(
				all.contains(&format!(
					"census evidence unavailable: {second} fresh stderr"
				)),
				"the unavailable run was not named"
			);
		} else {
			// Oversized stderr: excerpts cut at the file cap, the rest dropped
			// for the budget, and said so.
			assert!(files.iter().any(|f| f["truncated"] == true), "{index}");
			assert!(
				index["omitted"]
					.as_array()
					.unwrap()
					.iter()
					.any(|o| o == "budget exhausted"),
				"{index}"
			);
		}
	}
}
