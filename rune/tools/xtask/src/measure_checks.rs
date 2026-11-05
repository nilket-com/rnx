//! Pure controls for the measurement protocol helpers. No measurements are run.
use crate::common::{hash_file, Context};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{fs, process::Command};

const TESTS: &[(&str, usize)] = &[
	("test_protocol.py", 2),
	("test_append_diagnostic.py", 4),
	("test_call_diagnostic.py", 4),
	("test_n5_qualification.py", 9),
	("test_bootstrap.py", 2),
	("test_callgrind.py", 4),
	("test_callgrind_cell.py", 2),
	("test_callgrind_rules.py", 5),
	("test_callgrind_guard.py", 3),
	("test_callgrind_packet.py", 3),
	("test_callgrind_trend.py", 2),
	("test_callgrind_controller.py", 4),
	("test_control_proof.py", 3),
	("test_qualification_policy.py", 7),
	("test_perf_parse.py", 2),
	("test_counter_assess.py", 2),
	("test_counter_schedule.py", 2),
	("test_measurement_runtime.py", 4),
	("test_measurement_readiness.py", 2),
	("test_counter_cell.py", 2),
	("test_counter_packet.py", 2),
	("test_counter_native_mappings.py", 4),
	("test_counter_controller.py", 4),
	("test_k1.py", 2),
	("test_k1_analysis.py", 2),
	("test_k1_packet.py", 2),
	("test_k1_controller.py", 3),
	("test_source_export.py", 3),
	("test_build_recipe.py", 3),
	("test_build_capsule.py", 2),
	("test_build_worker.py", 4),
	("test_cargo_cache.py", 2),
	("test_role_evidence.py", 4),
	("test_seal_evidence.py", 7),
	("test_seal_store.py", 2),
	("test_input_closure.py", 3),
	("test_input_verification.py", 2),
	("test_analysis_sandbox.py", 2),
	("test_analysis_runtime.py", 2),
	("test_analysis_package.py", 2),
	("test_ledger_transition.py", 2),
	("test_ledger_store.py", 3),
	("test_milestone_registry.py", 2),
	("test_record_policy.py", 2),
	("test_record_store.py", 3),
	("test_tool_identity.py", 5),
];

pub fn check_manifest(ctx: &Context) -> Result<Value> {
	// N5's new qualification helpers must follow the repository's tab style.
	// Check before the expensive suites, alongside the selected control manifest.
	for name in [
		"n5_qualification.py",
		"test_n5_qualification.py",
		"callgrind_rules.py",
		"counter_controller.py",
		"counter_packet.py",
		"test_counter_native_mappings.py",
		"seal_evidence.py",
		"test_seal_evidence.py",
	] {
		let source = fs::read_to_string(ctx.repo.join("tools/xtask/measure").join(name))?;
		ensure!(
			!source.lines().any(|line| line.starts_with("    ")),
			"{name} must use tabs for indentation"
		);
	}
	let mut files = Vec::new();
	for (name, count) in TESTS {
		let path = ctx.repo.join("tools/xtask/measure").join(name);
		ensure!(
			path.is_file(),
			"measurement control is missing: {}",
			path.display()
		);
		files.push(json!({"name":name, "tests":count, "sha256":hash_file(&path)?}));
	}
	Ok(json!({"controls":files,"external_files":0}))
}

pub fn run(ctx: &Context) -> Result<Value> {
	check_manifest(ctx)?;
	let mut results = Vec::new();
	for (name, count) in TESTS {
		let path = ctx.repo.join("tools/xtask/measure").join(name);
		let label = name.trim_end_matches(".py").replace('_', "-");
		let mut command = Command::new("/usr/bin/python3");
		command
			.args(["-I", "-B", "-S"])
			.arg(&path)
			.current_dir(&ctx.repo);
		let result = ctx.checked(&label, &mut command, 120)?;
		let output = fs::read_to_string(&result.stderr)?;
		ensure!(
			output.contains(&format!("Ran {count} test")),
			"{name} did not run its selected controls"
		);
		ensure!(
			output.lines().any(|line| line == "OK"),
			"{name} did not report success"
		);
		results.push(json!({"name":name,"tests":count,"receipt":result.receipt}));
	}
	Ok(json!({"status":"PASS","controls":results}))
}
