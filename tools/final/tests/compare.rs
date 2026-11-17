//! The parity comparison on finals the controller really ran: two runs of
//! the same fake stages agree; a planted change each fails it, made where
//! a real difference would arise, so every binding stays consistent.
mod common;
use anyhow::Result;
use rnx_final::child::Done;
use rnx_final::compare::compare;
use rnx_final::controller::{self, Hooks, Settings, StageCall};
use rnx_final::failure::Environment;
use rnx_final::pyjson;
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::scheduler::{Resources, Targets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// What the second run plants.
#[derive(Clone, Copy, PartialEq)]
enum Plant {
	Nothing,
	ChangedRow,
	ExtraField,
	/// A field excused only for cross, varying in another producer.
	CrossFieldElsewhere,
}

struct Fake(Plant);

impl Hooks for Fake {
	fn head(&self, _: &Path) -> Option<String> {
		Some("c".into())
	}
	fn disk_check(&self, _: &Path, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn select(
		&self,
		root: &Path,
		out: &Path,
		_: Option<&[String]>,
		scope: &str,
		_: &Environment,
	) -> Result<Value> {
		let mut selection = common::selection(
			scope,
			json!("c"),
			json!([
				["rune-suite", ["python3", "suite"]],
				[
					"cross",
					[
						"python3",
						"verify",
						"--out",
						root.join(format!(
							"target/rnx-verify/{}-0123456789abcdef/payload",
							out.file_name().unwrap().to_string_lossy()
						))
					]
				]
			]),
		);
		selection["out"] = json!(out);
		selection["root"] = json!(root);
		Ok(selection)
	}
	fn check_selection(&self, _: &Value, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn finalize(
		&self,
		_: &Value,
		evidence: &Map<String, Value>,
		_: &Path,
		_: &Environment,
	) -> Result<Value> {
		Ok(common::finalization_of(evidence))
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
	fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
		let plant = self.0;
		Some((|| {
			// rune-suite's receipt sits in the final's directory; cross's in an
			// owned root that release removes, so only its retained copy remains.
			let routing = PathBuf::from(&call.environment["RNX_FINAL_ROUTING_ROOT"]);
			let place = if call.name == "cross" {
				routing.clone()
			} else {
				call.log.parent().unwrap().join(&call.name)
			};
			std::fs::create_dir_all(&place)?;
			let mut semantic = json!({"status": "PASS", "seconds": 1.5, "validation_context": invocation_id(), "artifacts": {place.join("0-test.log").to_string_lossy(): invocation_id()},
				"contracts": {"seconds": 2.5, "cases": [{"name": "one", "selected": ["a::one"], "passed": 1, "contract_rows": 3, "status": "PASS", "exit": 0, "seconds": 0.5, "artifacts": {"stdout.log": invocation_id()}}]}});
			if call.name == "rune-suite" && plant == Plant::ChangedRow {
				semantic["contracts"]["cases"][0]["selected"] = json!(["a::other"]);
			}
			// Cross's own build digests vary every run; they are excused for it alone.
			if call.name == "cross" {
				semantic["binding"] = json!({"executable_sha256": invocation_id()});
			}
			if call.name == "rune-suite" && plant == Plant::CrossFieldElsewhere {
				semantic["binding"] = json!({"executable_sha256": invocation_id()});
			}
			if call.name == "cross" && plant == Plant::ExtraField {
				semantic["unexpected"] = json!(true);
			}
			let receipt = place.join("RESULT.json");
			atomic_json(&receipt, &semantic)?;
			std::fs::create_dir(&call.directory)?;
			let mut row = common::stage_receipt(
				true,
				0,
				json!(["CARGO_BUILD_JOBS"]),
				json!({call.log.to_string_lossy(): "x"}),
			);
			row["context"]["kind"] = json!("producer");
			// The declared keys, as child::run computes them for a real stage.
			row["context"]["environment"] = rnx_final::child::environment(&call.environment)?;
			row["context"]["invocation"] = json!(invocation_id());
			row["producer"] = json!({"schema": 1, "context": invocation_id(), "invocation": invocation_id(), "source": "s", "status": "PASS", "receipt": receipt, "sha256": digest(&receipt)?});
			let result = call.directory.join("RESULT.json");
			atomic_json(&result, &row)?;
			Ok(Done {
				returncode: 0,
				validation: json!({"path": result, "sha256": digest(&result)?}),
				declared_output: None,
			})
		})())
	}
}

struct Run(PathBuf);
impl Drop for Run {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// One checkout, as the parity run uses: both finals run in it.
fn checkout() -> Run {
	let root = std::env::temp_dir().join(format!("rnx-final-compare-{}", invocation_id()));
	std::fs::create_dir(&root).unwrap();
	Run(root.canonicalize().unwrap())
}

fn final_run(checkout: &Run, plant: Plant, extra: Option<(&str, &str)>) -> PathBuf {
	let root = checkout.0.clone();
	let mut environment: Environment = std::env::vars().collect();
	environment.remove("RUSTFLAGS");
	if let Some((k, v)) = extra {
		environment.insert(k.into(), v.into());
	}
	let out = root.join(format!("final-{}", invocation_id()));
	let settings = Settings {
		root: root.clone(),
		out: out.clone(),
		names: None,
		scope: "required".into(),
		fresh: false,
		environment,
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: Arc::new(Fake(plant)),
		tokens: Some(2),
	};
	let r = controller::run(&settings).unwrap();
	assert_eq!(r["status"], "PASS", "{r}");
	out
}

fn verdict(a: &Path, b: &Path) -> Value {
	compare(a, b, None).unwrap()
}

#[test]
fn two_runs_that_differ_only_in_run_specific_places_agree() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, None),
	);
	assert!(
		b.join("retained/cross-RESULT.json").is_file(),
		"the owned-root receipt was not retained"
	);
	let r = verdict(&a, &b);
	assert_eq!(r["status"], "PASS", "{r}");
}

#[test]
fn a_changed_row_with_the_same_count_and_pass_fails() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::ChangedRow, None),
	);
	let r = verdict(&a, &b);
	assert_eq!(r["status"], "FAIL");
	assert!(
		r["differences"]
			.to_string()
			.contains("contracts.cases[0].selected"),
		"{r}"
	);
}

#[test]
fn an_unknown_extra_field_fails() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::ExtraField, None),
	);
	let r = verdict(&a, &b);
	assert!(
		r["differences"]
			.to_string()
			.contains("cross receipt.unexpected"),
		"{r}"
	);
}

#[test]
fn a_changed_declared_environment_value_fails() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, Some(("RUSTFLAGS", "-C opt-level=1"))),
	);
	let r = verdict(&a, &b);
	assert!(r["differences"].to_string().contains("RUSTFLAGS"), "{r}");
}

#[test]
fn missing_evidence_after_release_and_a_broken_binding_fail() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, None),
	);
	std::fs::remove_file(b.join("retained/cross-RESULT.json")).unwrap();
	let r = verdict(&a, &b);
	assert!(
		r["differences"]
			.to_string()
			.contains("producer receipt missing after release"),
		"{r}"
	);
	let child = b.join("rune-suite-child/RESULT.json");
	let mut row: Value = serde_json::from_slice(&std::fs::read(&child).unwrap()).unwrap();
	row["test_count"] = json!(9);
	std::fs::write(&child, pyjson::pretty(&row).unwrap()).unwrap();
	let r = verdict(&a, &b);
	assert!(
		r["differences"]
			.to_string()
			.contains("differs from its acceptance binding"),
		"{r}"
	);
}

/// The environment digests match the Python controller's at ADR 0284's
/// witness revision, for the same declared and undeclared variables.
#[test]
fn the_environment_digests_match_the_witness() {
	let fixture = common::witness::expected("environment-digests", &[]);
	let env: Environment = serde_json::from_value(fixture["environment"].clone()).unwrap();
	let rust = controller::environment_digests(
		&env,
		Path::new("/o/final"),
		"0123456789abcdef0123456789abcdef",
		Some(Path::new("/r/target/rnx-verify/final-aa")),
	)
	.unwrap();
	let witness: std::collections::BTreeMap<String, String> =
		serde_json::from_value(fixture["digests"].clone()).unwrap();
	assert_eq!(rust, witness);
	assert!(
		!rust.contains_key("HOME"),
		"an undeclared variable was recorded"
	);
}

#[test]
fn a_cross_only_exclusion_does_not_excuse_another_producer() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::CrossFieldElsewhere, None),
		final_run(&c, Plant::CrossFieldElsewhere, None),
	);
	let r = verdict(&a, &b);
	assert!(
		r["differences"]
			.to_string()
			.contains("rune-suite receipt.binding.executable_sha256"),
		"{r}"
	);
	assert!(
		!r["differences"]
			.to_string()
			.contains("cross receipt.binding"),
		"{r}"
	);
}

fn rebind_environments(out: &Path, edit: impl Fn(&mut Value)) {
	let path = out.join("ENVIRONMENTS.json");
	let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
	edit(&mut value);
	atomic_json(&path, &value).unwrap();
	let result_path = out.join("RESULT.json");
	let mut result: Value = serde_json::from_slice(&std::fs::read(&result_path).unwrap()).unwrap();
	result["records"]["ENVIRONMENTS"] = json!(digest(&path).unwrap());
	atomic_json(&result_path, &result).unwrap();
}

#[test]
fn an_environment_record_edited_after_binding_is_refused() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, None),
	);
	let path = b.join("ENVIRONMENTS.json");
	let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
	value["cross"]["CARGO_BUILD_JOBS"] = json!("0".repeat(64));
	atomic_json(&path, &value).unwrap();
	assert!(
		compare(&a, &b, None)
			.unwrap_err()
			.to_string()
			.contains("ENVIRONMENTS differs from its terminal record")
	);
}

#[test]
fn an_acceptance_edited_after_its_binding_is_refused() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, None),
	);
	let path = b.join("ACCEPTANCE.json");
	let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
	value["mode"] = json!("edited");
	atomic_json(&path, &value).unwrap();
	assert!(
		compare(&a, &b, None)
			.unwrap_err()
			.to_string()
			.contains("ACCEPTANCE differs from its terminal record")
	);
}

#[test]
fn a_stage_or_key_missing_from_both_environment_records_fails() {
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, None),
	);
	for out in [&a, &b] {
		rebind_environments(out, |v| {
			v.as_object_mut().unwrap().remove("cross");
		});
	}
	let r = verdict(&a, &b);
	assert!(
		r["differences"]
			.to_string()
			.contains("ENVIRONMENTS: the first final records"),
		"{r}"
	);
	let c = checkout();
	let (a, b) = (
		final_run(&c, Plant::Nothing, None),
		final_run(&c, Plant::Nothing, None),
	);
	for out in [&a, &b] {
		rebind_environments(out, |v| {
			v["rune-suite"]
				.as_object_mut()
				.unwrap()
				.remove("CARGO_BUILD_JOBS");
		});
	}
	let r = verdict(&a, &b);
	assert!(
		r["differences"]
			.to_string()
			.contains("environment record covers"),
		"{r}"
	);
}
