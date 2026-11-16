//! Ports of the Python stage-output controls (test_stage_outputs.py).
use rnx_final::outputs::{ORACLE, finish_stage, owned_report, validate_report};
use rnx_final::receipt::invocation_id;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn report() -> Value {
	json!({"cases": 1, "verified": 1, "tally": {"match": 1}, "results": [{"id": "unique", "path": "unique", "policy": "unordered", "unordered": true, "status": "match", "detail": "1,2"}]})
}

fn reordered() -> Value {
	let mut actual = report();
	actual["results"][0]["status"] = json!("row_order_differs");
	actual["results"][0]["detail"] = json!("2,1");
	actual["tally"] = json!({"row_order_differs": 1});
	actual
}

struct Repo(PathBuf);
impl Repo {
	fn new() -> Self {
		let root = std::env::temp_dir().join(format!("rnx-final-outputs-{}", invocation_id()));
		std::fs::create_dir_all(root.join(Path::new(ORACLE).parent().unwrap())).unwrap();
		let repo = Self(root);
		repo.git(&["init", "-q"]);
		std::fs::write(repo.0.join(ORACLE), report().to_string()).unwrap();
		std::fs::write(repo.0.join(".gitignore"), "target/\n").unwrap();
		std::fs::write(repo.0.join("source.rs"), "original").unwrap();
		repo.git(&["add", "."]);
		repo.git(&[
			"-c",
			"user.name=Test",
			"-c",
			"user.email=test@invalid",
			"-c",
			"commit.gpgsign=false",
			"commit",
			"-qm",
			"fixture",
		]);
		std::fs::create_dir(repo.0.join("target")).unwrap();
		repo
	}
	fn git(&self, args: &[&str]) -> Vec<u8> {
		let output = std::process::Command::new("git")
			.args(args)
			.current_dir(&self.0)
			.output()
			.unwrap();
		assert!(output.status.success());
		output.stdout
	}
}
impl Drop for Repo {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

#[test]
fn unapproved_missing_or_reclassified_cases_fail() {
	let original = report().to_string();
	for (key, value) in [
		("status", "mismatch"),
		("id", "different"),
		("policy", "changed"),
	] {
		let mut bad = report();
		bad["results"][0][key] = json!(value);
		assert!(
			validate_report(original.as_bytes(), bad.to_string().as_bytes()).is_err(),
			"{key}"
		);
	}
	let mut bad = report();
	bad["verified"] = json!(0);
	assert!(validate_report(original.as_bytes(), bad.to_string().as_bytes()).is_err());
	validate_report(original.as_bytes(), reordered().to_string().as_bytes()).unwrap();
}

#[test]
fn only_the_declared_report_is_restored_and_its_evidence_kept() {
	let repo = Repo::new();
	let (p, out) = (repo.0.join(ORACLE), repo.0.join("target"));
	let original = report().to_string();
	let encoded = reordered().to_string();
	std::fs::write(&p, &encoded).unwrap();
	assert!(finish_stage(&repo.0, "other", &out).unwrap().is_none());
	assert_eq!(std::fs::read_to_string(&p).unwrap(), encoded);
	std::fs::write(repo.0.join("source.rs"), "changed").unwrap();
	assert!(finish_stage(&repo.0, "polars-suite", &out).is_err());
	assert_eq!(std::fs::read_to_string(&p).unwrap(), encoded);
	std::fs::write(repo.0.join("source.rs"), "original").unwrap();
	let receipt = finish_stage(&repo.0, "polars-suite", &out)
		.unwrap()
		.unwrap();
	assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
	assert_eq!(
		std::fs::read_to_string(receipt["archive"].as_str().unwrap()).unwrap(),
		encoded
	);
	assert!(repo.git(&["status", "--porcelain"]).is_empty());
}

#[test]
fn an_owned_report_leaves_the_tracked_report_untouched() {
	let repo = Repo::new();
	let out = repo.0.join("target");
	let encoded = reordered().to_string();
	std::fs::write(owned_report(&out), &encoded).unwrap();
	let receipt = finish_stage(&repo.0, "polars-suite", &out)
		.unwrap()
		.unwrap();
	assert_eq!(
		std::fs::read_to_string(repo.0.join(ORACLE)).unwrap(),
		report().to_string()
	);
	assert_eq!(receipt["verified"], 1);
	// A tracked edit beside an owned report is a source mutation.
	std::fs::write(repo.0.join(ORACLE), &encoded).unwrap();
	assert!(finish_stage(&repo.0, "polars-suite", &out).is_err());
}
