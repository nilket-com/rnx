//! A census failure keeps its cases (ADR 0292): the real `cross` launcher's
//! receipt assembly over a planted census payload, then the final's
//! ordinary failure retention, then the payload's release. What survives
//! names the case and its reason and holds its labelled stock and fresh
//! evidence, redacted, within the existing envelope.
use rnx_final::failure::{self, Environment};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-census-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

/// The first two executed census cases, by the census plan.
fn cases() -> (String, String) {
	let plan =
		std::fs::read_to_string(root().join("tools/verify/fixtures/census/execution.tsv")).unwrap();
	let ids: Vec<String> = plan
		.lines()
		.skip(1)
		.filter(|l| l.split('\t').nth(1) == Some("execute"))
		.map(|l| l.split('\t').next().unwrap().to_owned())
		.take(2)
		.collect();
	(ids[0].clone(), ids[1].clone())
}

const SECRET: &str = "planted-cross-secret-0292";

/// The launcher's receipt for `payload`, written by the stage's own
/// `cross::terminal_receipt`, after a launcher failure (`failed`) or a pass.
fn launcher_receipt(payload: &Path, failed: bool) -> Value {
	let failure = rnx_final::cross::Failure {
		repr: "RuntimeError('cross verification failed')".into(),
		notes: vec![],
	};
	let result = rnx_final::cross::terminal_receipt(
		payload,
		&root().join(rnx_final::cross::CENSUS_PLAN),
		failed.then_some(&failure),
		json!({"status": "PASS"}),
	)
	.unwrap();
	rnx_final::receipt::atomic_json(&payload.join("RESULT.json"), &result).unwrap();
	serde_json::from_slice(&std::fs::read(payload.join("RESULT.json")).unwrap()).unwrap()
}

/// A census payload: the summary with two differing cases; the first with
/// both engines' outputs (a secret in its stock stderr), the second with
/// only its stock run, its fresh run having stopped before writing.
fn plant(payload: &Path, first: &str, second: &str) {
	let live = payload.join("census/live-execution");
	for (case, engine) in [(first, "stock"), (first, "fresh"), (second, "stock")] {
		let run = live.join(case).join(engine);
		std::fs::create_dir_all(run.join("report")).unwrap();
		std::fs::write(run.join("stdout"), format!("{case} {engine} stdout\n")).unwrap();
		std::fs::write(
			run.join("stderr"),
			format!("{case} {engine} stderr token={SECRET}\n"),
		)
		.unwrap();
		std::fs::write(
			run.join("report/result.json"),
			format!("{{\"case\": \"{case}\", \"engine\": \"{engine}\"}}\n"),
		)
		.unwrap();
	}
	let summary = json!({"executed": 71, "admission_only": [], "differences": [
		{"id": first, "reason": "live observations differ"},
		{"id": second, "reason": format!("census execution {second} fresh failed: exit status: 124")},
	]});
	std::fs::write(
		live.join("RESULT.json"),
		serde_json::to_vec_pretty(&summary).unwrap(),
	)
	.unwrap();
	std::fs::write(
		payload.join("controller.stderr"),
		format!("cross verification: unreviewed census execution differences (2 cases): {first}: live observations differ; {second}: census execution {second} fresh failed: exit status: 124\n"),
	)
	.unwrap();
}

#[test]
fn a_census_failure_keeps_its_cases_through_release() {
	let s = scratch();
	let (out, payload) = (s.0.join("final"), s.0.join("payload"));
	std::fs::create_dir_all(&out).unwrap();
	std::fs::create_dir_all(&payload).unwrap();
	let (first, second) = cases();
	plant(&payload, &first, &second);
	let receipt = launcher_receipt(&payload, true);
	assert!(
		receipt["error"]
			.as_str()
			.unwrap()
			.contains(&format!("{first}: live observations differ")),
		"{receipt}"
	);
	assert_eq!(
		receipt["launcher_error"],
		"RuntimeError('cross verification failed')"
	);
	let env: Environment = [("CROSS_TOKEN".to_string(), SECRET.to_string())].into();
	let roots = vec![out.clone(), payload.clone()];
	failure::retain(
		&out,
		"cross",
		Some(&payload.join("RESULT.json")),
		&[out.join("cross.log")],
		&roots,
		&env,
		"child failed",
	)
	.unwrap();
	// The payload is released.
	std::fs::remove_dir_all(&payload).unwrap();
	let index: Value =
		serde_json::from_slice(&std::fs::read(out.join("FAILURE.json")).unwrap()).unwrap();
	let error = index["error"].as_str().unwrap();
	assert!(
		error.contains(&format!("{first}: live observations differ"))
			&& error.contains("(2 cases)"),
		"{error}"
	);
	let files = index["files"].as_array().unwrap();
	assert!(files.len() <= 8);
	let label = |name: &str| files.iter().find(|f| f["source_name"] == name).cloned();
	let live = "census/live-execution";
	assert!(label(&format!("{live}/RESULT.json")).is_some(), "{index}");
	for engine in ["stock", "fresh"] {
		for kind in ["report/result.json", "stderr"] {
			let row = label(&format!("{live}/{first}/{engine}/{kind}"))
				.unwrap_or_else(|| panic!("{engine} {kind}: {index}"));
			let text = std::fs::read_to_string(out.join(row["path"].as_str().unwrap())).unwrap();
			assert!(text.contains(&first) && text.contains(engine), "{text}");
			assert!(!text.contains(SECRET), "the secret survived: {text}");
		}
	}
	let stderr = label(&format!("{live}/{first}/stock/stderr")).unwrap();
	assert!(
		std::fs::read_to_string(out.join(stderr["path"].as_str().unwrap()))
			.unwrap()
			.contains("<redacted>")
	);
	// The second case's fresh run wrote nothing: named unavailable, not lost to a cap.
	let projected = files.iter().find(|f| f["kind"] == "receipt").unwrap();
	let receipt_text =
		std::fs::read_to_string(out.join(projected["path"].as_str().unwrap())).unwrap();
	assert!(
		receipt_text.contains(&format!(
			"census evidence unavailable: {second} fresh stderr"
		)),
		"{receipt_text}"
	);
}

/// A failure with no census summary, and a pass, keep the receipts they had.
#[test]
fn other_failures_and_passes_keep_their_receipts() {
	let s = scratch();
	let payload = s.0.join("payload");
	std::fs::create_dir_all(&payload).unwrap();
	let failed = launcher_receipt(&payload, true);
	assert_eq!(failed["status"], "FAIL");
	assert!(
		failed.get("error").is_none()
			&& failed.get("diagnostic_files").is_none()
			&& failed.get("causes").is_none(),
		"{failed}"
	);
	std::fs::remove_file(payload.join("RESULT.json")).unwrap();
	let passed = launcher_receipt(&payload, false);
	assert!(
		passed.get("launcher_error").is_none() && passed.get("diagnostic_files").is_none(),
		"{passed}"
	);
	// A malformed summary leaves the failure as it was.
	std::fs::create_dir_all(payload.join("census/live-execution")).unwrap();
	std::fs::write(
		payload.join("census/live-execution/RESULT.json"),
		"{not json",
	)
	.unwrap();
	std::fs::remove_file(payload.join("RESULT.json")).unwrap();
	let malformed = launcher_receipt(&payload, true);
	assert!(
		malformed.get("error").is_none()
			&& malformed["launcher_error"] == "RuntimeError('cross verification failed')",
		"{malformed}"
	);
}

/// One unreadable declared file between two good ones is skipped with a
/// note; the next is still kept, at the next ordinal, and no excerpt is
/// left without its index row.
#[test]
fn an_unreadable_diagnostic_is_skipped_without_blocking_the_next() {
	use std::os::unix::fs::PermissionsExt as _;
	let s = scratch();
	let (out, payload) = (s.0.join("final"), s.0.join("payload"));
	std::fs::create_dir_all(&out).unwrap();
	std::fs::create_dir_all(&payload).unwrap();
	for name in ["one.log", "two.log", "three.log"] {
		std::fs::write(payload.join(name), format!("{name} text\n")).unwrap();
	}
	std::fs::set_permissions(
		payload.join("two.log"),
		std::fs::Permissions::from_mode(0o000),
	)
	.unwrap();
	let receipt = payload.join("RESULT.json");
	rnx_final::receipt::atomic_json(&receipt, &json!({"status": "FAIL", "error": "planted", "diagnostic_files": ["one.log", "two.log", "three.log"]})).unwrap();
	let roots = vec![out.clone(), payload.clone()];
	failure::retain(
		&out,
		"cross",
		Some(&receipt),
		&[],
		&roots,
		&Environment::new(),
		"planted",
	)
	.unwrap();
	std::fs::set_permissions(
		payload.join("two.log"),
		std::fs::Permissions::from_mode(0o644),
	)
	.unwrap();
	let index: Value =
		serde_json::from_slice(&std::fs::read(out.join("FAILURE.json")).unwrap()).unwrap();
	let files = index["files"].as_array().unwrap();
	let names: Vec<&str> = files
		.iter()
		.map(|f| f["source_name"].as_str().unwrap())
		.collect();
	assert!(
		names.contains(&"one.log") && names.contains(&"three.log") && !names.contains(&"two.log"),
		"{names:?}"
	);
	assert_eq!(
		std::fs::read_dir(out.join("failure")).unwrap().count(),
		files.len()
	);
	assert!(!index["omitted"].as_array().unwrap().is_empty(), "{index}");
}

/// The launcher's headline comes from the summary alone, bounded in UTF-8
/// bytes with its total and omission marker, whatever stderr holds.
#[test]
fn the_launcher_headline_is_bounded_from_the_summary() {
	let (first, second) = cases();
	for stderr in [None, Some(5 * 1024 * 1024)] {
		for (reasons, total) in [
			(vec!["é".repeat(3000)], 1usize),
			(vec!["differ".to_string(); 9], 9),
		] {
			let s = scratch();
			let payload = s.0.join("payload");
			let live = payload.join("census/live-execution");
			std::fs::create_dir_all(&live).unwrap();
			let differences: Vec<Value> = reasons
				.iter()
				.enumerate()
				.map(|(i, r)| json!({"id": if i % 2 == 0 { &first } else { &second }, "reason": r}))
				.collect();
			std::fs::write(
				live.join("RESULT.json"),
				serde_json::to_vec(&json!({"differences": differences})).unwrap(),
			)
			.unwrap();
			if let Some(bytes) = stderr {
				std::fs::write(payload.join("controller.stderr"), "x".repeat(bytes)).unwrap();
			}
			let receipt = launcher_receipt(&payload, true);
			let error = receipt["error"].as_str().unwrap();
			assert!(error.len() <= 4096, "{}", error.len());
			assert!(
				error.starts_with(&format!(
					"unreviewed census execution differences ({total} cases): "
				)),
				"{error}"
			);
			if total == 9 {
				assert!(error.ends_with(" (+5 more)"), "{error}");
			} else {
				assert!(
					error.contains(&format!("{first}: éé")) && error.ends_with("..."),
					"{error}"
				);
			}
		}
	}
}

/// An addition that cannot create its excerpt, because a file already holds
/// that ordinal, fails without touching that file.
#[test]
fn a_failed_addition_never_removes_a_file_it_did_not_create() {
	let s = scratch();
	let out = s.0.join("final");
	std::fs::create_dir_all(&out).unwrap();
	let source = s.0.join("source.log");
	std::fs::write(&source, "source text\n").unwrap();
	let mut bundle = failure::Bundle::new(&out, "cross", &Environment::new(), "planted").unwrap();
	let existing = out.join("failure/00-child.log");
	std::fs::write(&existing, "already here\n").unwrap();
	let roots = vec![s.0.clone()];
	assert!(bundle.add(&source, failure::Kind::Log, &roots).is_err());
	assert_eq!(
		std::fs::read_to_string(&existing).unwrap(),
		"already here\n"
	);
}

/// Wrong-shape difference rows make the summary malformed: the launcher's
/// original failure receipt stands, as for invalid JSON.
#[test]
fn wrong_shape_census_rows_keep_the_original_receipt() {
	let (first, _) = cases();
	for rows in [
		json!([{}]),
		json!([{"id": "not-a-case", "reason": "x"}]),
		json!([{"id": first, "reason": 7}]),
		json!([first]),
	] {
		let s = scratch();
		let payload = s.0.join("payload");
		let live = payload.join("census/live-execution");
		std::fs::create_dir_all(&live).unwrap();
		std::fs::write(
			live.join("RESULT.json"),
			serde_json::to_vec(&json!({"differences": rows})).unwrap(),
		)
		.unwrap();
		let receipt = launcher_receipt(&payload, true);
		assert!(
			receipt.get("error").is_none() && receipt.get("diagnostic_files").is_none(),
			"{rows}: {receipt}"
		);
		assert_eq!(
			receipt["launcher_error"],
			"RuntimeError('cross verification failed')"
		);
	}
}
