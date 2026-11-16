//! Ports of the Python scheduling controls (test_stage_scheduler.Scheduling).
mod common;
use anyhow::Result;
use rnx_final::child::Done;
use rnx_final::controller::{self, Hooks, Settings, StageCall};
use rnx_final::failure::Environment;
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::scheduler::{ALONE, FIRST, Resources, Targets};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Dir(PathBuf);
impl Dir {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!("rnx-final-scheduling-{}", invocation_id()));
		std::fs::create_dir(&path).unwrap();
		Self(path.canonicalize().unwrap())
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// Each fake stage records when it ran, and sleeps or fails as told.
struct Fake {
	plan: Vec<(&'static str, Vec<&'static str>)>,
	behavior: BTreeMap<&'static str, (f64, i32)>,
	spans: Mutex<BTreeMap<String, (Instant, Instant)>>,
	weight: usize,
	check_delay: Duration,
	/// Free bytes each admission sees, in turn; the last one repeats.
	free: Mutex<Vec<u64>>,
	environments: Mutex<Vec<(Option<String>, Option<String>)>>,
	/// A stage that borrows tokens from the pool, as Cargo's jobs do, and
	/// returns them while it keeps running: (stage, tokens, held seconds).
	borrow: Option<(&'static str, usize, f64)>,
	/// A stage whose unchanged Python client runs an exclusive case.
	exclusive: Option<&'static str>,
	weights: BTreeMap<&'static str, usize>,
}

impl Fake {
	fn new(
		plan: Vec<(&'static str, Vec<&'static str>)>,
		behavior: &[(&'static str, f64, i32)],
	) -> Self {
		Self {
			plan,
			behavior: behavior.iter().map(|(n, s, c)| (*n, (*s, *c))).collect(),
			spans: Mutex::new(BTreeMap::new()),
			weight: 1,
			check_delay: Duration::ZERO,
			free: Mutex::new(vec![]),
			environments: Mutex::new(vec![]),
			borrow: None,
			exclusive: None,
			weights: BTreeMap::new(),
		}
	}
	fn span(&self, name: &str) -> Option<(Instant, Instant)> {
		self.spans.lock().unwrap().get(name).copied()
	}
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
		_: &str,
		_: &Environment,
	) -> Result<Value> {
		let stages: Vec<Value> = self.plan.iter().map(|(n, _)| json!([n, [n]])).collect();
		Ok(common::selection("explicit", json!("c"), json!(stages)))
	}
	fn check_selection(&self, _: &Value, _: &Path, _: &Environment) -> Result<()> {
		std::thread::sleep(self.check_delay);
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
		Ok(self
			.plan
			.iter()
			.find(|(n, _)| *n == name)
			.unwrap()
			.1
			.iter()
			.map(|s| s.to_string())
			.collect())
	}
	fn weight(&self, name: &str, _: &[String], _: &Path) -> Result<usize> {
		Ok(self.weights.get(name).copied().unwrap_or(self.weight))
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
	fn disk_free(&self, path: &Path) -> Result<u64> {
		let mut free = self.free.lock().unwrap();
		match free.len() {
			0 => controller::disk_free(path),
			1 => Ok(free[0]),
			_ => Ok(free.remove(0)),
		}
	}
	fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
		self.environments.lock().unwrap().push((
			std::env::var("GIT_OPTIONAL_LOCKS").ok(),
			call.environment.get("GIT_OPTIONAL_LOCKS").cloned(),
		));
		let (seconds, code) = self
			.behavior
			.get(call.name.as_str())
			.copied()
			.unwrap_or((0.3, 0));
		let start = Instant::now();
		if let Some((_, count, hold)) = self.borrow.filter(|(stage, _, _)| *stage == call.name) {
			// Borrow tokens as a Cargo job does, and return them mid-stage.
			let flags = &call.environment["MAKEFLAGS"];
			let path = flags
				.split("fifo:")
				.nth(1)
				.unwrap()
				.split_whitespace()
				.next()
				.unwrap();
			let mut fifo = std::fs::OpenOptions::new()
				.read(true)
				.write(true)
				.open(path)
				.unwrap();
			let mut tokens = vec![0u8; count];
			std::io::Read::read_exact(&mut fifo, &mut tokens).unwrap();
			std::thread::sleep(Duration::from_secs_f64(hold));
			std::io::Write::write_all(&mut fifo, &tokens).unwrap();
		}
		if self.exclusive == Some(call.name.as_str()) {
			let script = "import time, native_contracts\nfrom unittest.mock import patch\ndef case(*a): time.sleep(0.5); return {'status': 'PASS'}\nwith patch.object(native_contracts, 'run_case', side_effect=case):\n    native_contracts.with_tokens(3, {'name': 'exclusive'}, None, None, None, None)\n";
			let column = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/column");
			let status = std::process::Command::new("timeout")
				.args(["30", "python3", "-B", "-c", script])
				.env_clear()
				.envs(&call.environment)
				.current_dir(column)
				.status();
			if !status.as_ref().is_ok_and(|s| s.success()) {
				return Some(Err(anyhow::anyhow!(
					"the exclusive case never got its tokens: {status:?}"
				)));
			}
		}
		std::thread::sleep(Duration::from_secs_f64(seconds));
		self.spans
			.lock()
			.unwrap()
			.insert(call.name.clone(), (start, Instant::now()));
		Some((|| {
			std::fs::create_dir(&call.directory)?;
			let receipt = call.directory.join("RESULT.json");
			let mut row = common::stage_receipt(code == 0, code, json!([]), json!({}));
			if code != 0 {
				row["error"] = json!("planted failure");
			}
			atomic_json(&receipt, &row)?;
			Ok(Done {
				returncode: code,
				validation: json!({"path": receipt, "sha256": digest(&receipt)?}),
				declared_output: None,
			})
		})())
	}
}

fn run(fake: Arc<Fake>, tokens: usize) -> (Value, Dir) {
	let dir = Dir::new();
	let names: Vec<String> = fake.plan.iter().map(|(n, _)| n.to_string()).collect();
	let settings = Settings {
		root: dir.0.clone(),
		out: dir.0.join("final"),
		names: Some(names),
		scope: "explicit".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: fake,
		tokens: Some(tokens),
	};
	(controller::run(&settings).unwrap(), dir)
}

fn overlap(a: (Instant, Instant), b: (Instant, Instant)) -> bool {
	a.0 < b.1 && b.0 < a.1
}

fn names(r: &Value) -> Vec<String> {
	r["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| c["name"].as_str().unwrap().to_owned())
		.collect()
}

#[test]
fn disjoint_stages_overlap_shared_ones_keep_order_and_alone_overlaps_nothing() {
	let fake = Arc::new(Fake::new(
		vec![
			("a", vec!["x"]),
			("b", vec!["y"]),
			("c", vec!["x"]),
			("fixed-route", vec![ALONE]),
			("d", vec!["z"]),
		],
		&[],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	let span = |n| fake.span(n).unwrap();
	assert!(overlap(span("a"), span("b")));
	assert!(!overlap(span("a"), span("c")) && span("a").1 <= span("c").0);
	for name in ["a", "b", "c", "d"] {
		assert!(!overlap(span("fixed-route"), span(name)), "{name}");
	}
	assert_eq!(names(&r), ["a", "b", "c", "fixed-route", "d"]);
}

#[test]
fn first_stages_run_before_everything() {
	let fake = Arc::new(Fake::new(
		vec![
			("commit-discipline", vec![FIRST]),
			("a", vec!["x"]),
			("b", vec!["y"]),
		],
		&[],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS");
	assert!(
		fake.span("commit-discipline").unwrap().1
			<= fake.span("a").unwrap().0.min(fake.span("b").unwrap().0)
	);
}

#[test]
fn a_failure_stops_admission_and_running_stages_keep_receipts() {
	let fake = Arc::new(Fake::new(
		vec![
			("slow", vec!["y"]),
			("broken", vec!["x"]),
			("after", vec!["x"]),
			("other", vec!["z"]),
		],
		&[("slow", 1.0, 0), ("broken", 0.1, 1), ("other", 0.1, 0)],
	));
	let (r, _dir) = run(fake.clone(), 2);
	assert_eq!(
		(r["status"].clone(), r["failed_stage"].clone()),
		(json!("FAIL"), json!("broken"))
	);
	assert!(fake.span("after").is_none() && fake.span("other").is_none());
	let exits: BTreeMap<String, i64> = r["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| {
			(
				c["name"].as_str().unwrap().to_owned(),
				c["exit"].as_i64().unwrap(),
			)
		})
		.collect();
	assert_eq!(
		exits,
		[("slow".to_string(), 0), ("broken".to_string(), 1)].into()
	);
}

#[test]
fn a_declared_producer_finishes_before_its_consumer() {
	// The catalogue's own producer pair.
	let fake = Arc::new(Fake::new(
		vec![
			("polars-inventories", vec!["x"]),
			("polars-suite", vec!["y"]),
		],
		&[("polars-inventories", 1.0, 0), ("polars-suite", 0.1, 0)],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS");
	assert!(fake.span("polars-inventories").unwrap().1 <= fake.span("polars-suite").unwrap().0);
}

#[test]
fn a_failed_producer_never_runs_its_consumer() {
	let fake = Arc::new(Fake::new(
		vec![
			("polars-inventories", vec!["x"]),
			("polars-suite", vec!["y"]),
		],
		&[("polars-inventories", 0.1, 1)],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["failed_stage"], "polars-inventories");
	assert!(fake.span("polars-suite").is_none());
}

#[test]
fn a_failure_that_already_returned_stops_the_next_admission() {
	let mut fake = Fake::new(
		vec![("broken", vec!["x"]), ("b", vec!["y"]), ("c", vec!["z"])],
		&[("broken", 0.0, 1), ("b", 0.1, 0), ("c", 0.1, 0)],
	);
	// Each admission rechecks the selection, slowly.
	fake.check_delay = Duration::from_millis(500);
	let fake = Arc::new(fake);
	let (r, _dir) = run(fake.clone(), 8);
	assert_eq!(r["failed_stage"], "broken");
	assert!(fake.span("c").is_none());
}

#[test]
fn a_test_width_above_the_budget_is_refused_before_any_stage() {
	for tokens in [1, 2] {
		let mut fake = Fake::new(vec![("a", vec!["x"])], &[]);
		fake.weight = 4;
		let fake = Arc::new(fake);
		let (r, _dir) = run(fake.clone(), tokens);
		assert!(
			r["error"].as_str().unwrap().contains(&format!(
				"budget of {tokens} tokens is below the test width of a (4)"
			)),
			"{r}"
		);
		assert!(fake.span("a").is_none());
	}
	let mut fake = Fake::new(vec![("a", vec!["x"])], &[]);
	fake.weight = 4;
	assert_eq!(run(Arc::new(fake), 4).0["status"], "PASS");
}

#[test]
fn tokens_bound_running_stages() {
	let fake = Arc::new(Fake::new(
		vec![("a", vec!["x"]), ("b", vec!["y"]), ("c", vec!["z"])],
		&[],
	));
	let (r, _dir) = run(fake.clone(), 1);
	assert_eq!(r["status"], "PASS");
	let mut spans: Vec<(Instant, Instant)> = ["a", "b", "c"]
		.iter()
		.map(|n| fake.span(n).unwrap())
		.collect();
	spans.sort();
	assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0));
}

const GIB: u64 = 1024 * 1024 * 1024;

#[test]
fn disk_short_of_the_estimate_or_the_remaining_estimate_is_refused() {
	let floor = controller::DISK_FLOOR_BYTES;
	let bulk = rnx_final::scheduler::declared_bulk("clippy");
	let fake = Fake::new(vec![("a", vec!["x"]), ("clippy", vec!["x"])], &[]);
	*fake.free.lock().unwrap() = vec![floor + bulk - 1];
	let fake = Arc::new(fake);
	let (r, dir) = run(fake.clone(), 2);
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("disk admission before a"),
		"{r}"
	);
	assert!(fake.span("a").is_none());
	let _ = dir;
	// Enough for both at first; the second stage then finds too little.
	let fake = Fake::new(vec![("a", vec!["x"]), ("clippy", vec!["x"])], &[]);
	*fake.free.lock().unwrap() = vec![floor + bulk, floor + bulk - 1];
	let (r, dir) = run(Arc::new(fake), 2);
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("disk admission before clippy"),
		"{r}"
	);
	let acceptance: Value =
		serde_json::from_slice(&std::fs::read(dir.0.join("final/ACCEPTANCE.json")).unwrap())
			.unwrap();
	// "x" is outside the checkout, so it adds no growth.
	assert_eq!(acceptance["disk"]["estimate_bytes"], json!(floor + bulk));
	assert_eq!(
		acceptance["disk"]["lowest_free_bytes"],
		json!(floor + bulk - 1)
	);
}

#[test]
fn a_cold_target_directory_counts_its_growth() {
	let dir = Dir::new();
	let cold: &'static str = Box::leak(
		dir.0
			.join("adapters/large/target/debug")
			.to_string_lossy()
			.into_owned()
			.into_boxed_str(),
	);
	// The run's root is its own temporary checkout, so the lane is inside it.
	let fake = Fake::new(vec![("suite", vec![cold])], &[]);
	*fake.free.lock().unwrap() =
		vec![controller::DISK_FLOOR_BYTES + rnx_final::scheduler::COLD_BYTES - 1];
	let fake = Arc::new(fake);
	let settings = Settings {
		root: dir.0.clone(),
		out: dir.0.join("final"),
		names: Some(vec!["suite".into()]),
		scope: "explicit".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: fake.clone(),
		tokens: Some(2),
	};
	let r = controller::run(&settings).unwrap();
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("disk admission before suite"),
		"{r}"
	);
	assert!(fake.span("suite").is_none());
	let acceptance: Value =
		serde_json::from_slice(&std::fs::read(dir.0.join("final/ACCEPTANCE.json")).unwrap())
			.unwrap();
	assert_eq!(
		acceptance["disk"]["estimate_bytes"],
		json!(controller::DISK_FLOOR_BYTES + rnx_final::scheduler::COLD_BYTES)
	);
}

#[test]
fn target_growth_is_declared_warm_or_cold() {
	let dir = Dir::new();
	let warm = dir.0.join("warm/debug");
	std::fs::create_dir_all(&warm).unwrap();
	std::fs::write(warm.join("artifact"), vec![b'x'; 1000]).unwrap();
	assert_eq!(
		rnx_final::scheduler::growth(&warm.to_string_lossy(), &dir.0),
		GIB
	);
	assert_eq!(
		rnx_final::scheduler::growth(&dir.0.join("missing/debug").to_string_lossy(), &dir.0),
		rnx_final::scheduler::COLD_BYTES
	);
	assert_eq!(rnx_final::scheduler::growth(ALONE, &dir.0), 0);
}

#[test]
fn every_selected_stage_is_declared_and_resolved() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let targets = Targets::new(&root);
	let commands = rnx_final::catalogue::commands(&root.join("target/unused"), &root).unwrap();
	let plan: BTreeMap<String, Resources> = commands
		.iter()
		.map(|(n, c)| {
			(
				n.clone(),
				rnx_final::scheduler::resources(n, c, &root, &targets).unwrap(),
			)
		})
		.collect();
	let target = |a: &str, b: &str| -> String {
		if b.is_empty() {
			root.join(a).to_string_lossy().into_owned()
		} else {
			root.join(a).join(b).to_string_lossy().into_owned()
		}
	};
	assert_eq!(plan["rune-suite"], [target("target", "engine-dev")].into());
	assert_eq!(
		plan["polars-suite"],
		[
			target("adapters/polars/target", "debug"),
			target("target/0073", "")
		]
		.into()
	);
	assert!(plan["polars-inventories"].contains(&target("target/0072", "")));
	assert_eq!(
		plan["production-cli-build"],
		plan["production-cli-controls"]
	);
	assert_eq!(
		plan["production-cli-build"],
		[target("adapters/polars/target", "release")].into()
	);
	assert!(plan["package-hegel"].contains(&target("rune/hegel_tests/target", "debug")));
	assert!(plan["format"].is_empty());
	assert_eq!(plan["fixed-route"], [ALONE.to_string()].into());
	assert_eq!(plan["fetch-Cargo.toml"], [FIRST.to_string()].into());
	assert_eq!(
		plan["clippy"],
		[
			target("target/column-clippy-published", "debug"),
			target("target/column-clippy-candidate", "debug")
		]
		.into()
	);
	assert!(
		rnx_final::scheduler::declared(
			"new-stage",
			&[
				"python3".into(),
				"-B".into(),
				"tools/column/new_stage.py".into()
			]
		)
		.unwrap_err()
		.to_string()
		.contains("not declared")
	);
	let by_name: BTreeMap<String, Vec<String>> = commands.into_iter().collect();
	let weights: BTreeMap<&str, usize> = [
		"rune-suite",
		"root-tests",
		"generated-projects",
		"clippy",
		"cross",
		"package-hegel",
		"format",
		"column-host",
		"fixed-route",
	]
	.iter()
	.map(|n| {
		(
			*n,
			rnx_final::scheduler::weight(n, &by_name[*n], &root).unwrap(),
		)
	})
	.collect();
	assert_eq!(
		weights,
		[
			("rune-suite", 4),
			("root-tests", 4),
			("generated-projects", 1),
			("clippy", 1),
			("cross", 4),
			("package-hegel", 4),
			("format", 1),
			("column-host", 4),
			("fixed-route", 1)
		]
		.into()
	);
}

#[test]
fn stages_run_git_without_optional_locks() {
	let fake = Arc::new(Fake::new(vec![("a", vec!["x"])], &[]));
	let (r, _dir) = run(fake.clone(), 2);
	assert_eq!(r["status"], "PASS");
	// The stage's own environment carries it; the test process is untouched.
	assert_eq!(fake.environments.lock().unwrap()[0].1.as_deref(), Some("0"));
}

#[test]
fn python_suites_and_the_screen_get_no_jobserver() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let commands: BTreeMap<String, Vec<String>> =
		rnx_final::catalogue::commands(&root.join("target/unused"), &root)
			.unwrap()
			.into_iter()
			.collect();
	let dir = Dir::new();
	let pool = job_budget::fifo::Pool::create(&dir.0.join("jobserver.fifo"), 4).unwrap();
	for name in [
		"controller-controls",
		"benchmark-protocol",
		"pin-controls",
		"fixed-route",
	] {
		assert!(
			rnx_final::scheduler::stage_environment(name, &commands[name], &pool, 1).is_empty(),
			"{name}"
		);
	}
	let env: BTreeMap<String, String> =
		rnx_final::scheduler::stage_environment("rune-suite", &commands["rune-suite"], &pool, 4)
			.into_iter()
			.collect();
	assert!(env["MAKEFLAGS"].contains("--jobserver-auth=fifo:"));
	assert_eq!(
		(
			env["RNX_FINAL_STAGE_TOKENS"].as_str(),
			env["RUST_TEST_THREADS"].as_str()
		),
		("4", "4")
	);
	pool.close().unwrap();
}

/// The cycle a partial reservation would create: a running stage's
/// unchanged Python client starts an exclusive case and needs every token
/// its stage does not hold, while a four-wide stage waits beside it. With
/// whole-width admission the wide stage holds nothing while it waits, so the
/// exclusive case runs, its stage ends, and the wide stage follows. The
/// client runs under a 30 s timeout, so a deadlock fails instead of hanging.
#[test]
fn a_waiting_wide_stage_never_starves_a_running_exclusive_case() {
	let mut fake = Fake::new(
		vec![("holder", vec!["x"]), ("wide", vec!["y"])],
		&[("holder", 0.0, 0), ("wide", 0.1, 0)],
	);
	fake.exclusive = Some("holder");
	fake.weights = [("holder", 1), ("wide", 4)].into();
	let fake = Arc::new(fake);
	let started = Instant::now();
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	assert!(
		started.elapsed() < Duration::from_secs(25),
		"the exclusive case waited on tokens the wide stage held"
	);
	let (holder, wide) = (fake.span("holder").unwrap(), fake.span("wide").unwrap());
	assert!(
		wide.0 >= holder.1,
		"the wide stage ran beside a stage holding a token"
	);
}
