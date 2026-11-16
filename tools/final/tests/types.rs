//! The receipt types: real receipts round-trip byte for byte; malformed
//! ones are refused at the boundary.
use rnx_final::pyjson;
use rnx_final::types::{self, Acceptance, ChildResult, FinalResult, Manifest, Release, Selection};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Parse into the type and write it back: the stored bytes, exactly.
fn round_trip<T: DeserializeOwned + Serialize>(path: &Path) -> Result<(), String> {
	let typed: T = types::read(path).map_err(|e| format!("{e:#}"))?;
	let again = pyjson::pretty(&serde_json::to_value(&typed).unwrap()).unwrap();
	let stored = std::fs::read_to_string(path).unwrap();
	if again != stored {
		return Err(format!("{} re-encodes differently", path.display()));
	}
	Ok(())
}

fn check_final(out: &Path) -> Vec<String> {
	let mut failures = Vec::new();
	let mut note = |r: Result<(), String>| {
		if let Err(e) = r {
			failures.push(e);
		}
	};
	note(round_trip::<FinalResult>(&out.join("RESULT.json")));
	for (name, check) in [
		(
			"ACCEPTANCE",
			round_trip::<Acceptance> as fn(&Path) -> Result<(), String>,
		),
		("MANIFEST", round_trip::<Manifest>),
		("RELEASE", round_trip::<Release>),
		("SELECTION", round_trip::<Selection>),
	] {
		let path = out.join(format!("{name}.json"));
		if path.exists() {
			note(check(&path));
		}
	}
	for entry in std::fs::read_dir(out).unwrap().flatten() {
		let receipt = entry.path().join("RESULT.json");
		if entry.file_name().to_string_lossy().ends_with("-child") && receipt.exists() {
			note(round_trip::<ChildResult>(&receipt));
		}
	}
	failures
}

/// Every receipt of real finals, when a directory of them is named
/// (RNX_FINAL_RECEIPTS=dir of FINAL-* directories): run locally, never
/// committed, since real receipts hold a machine's paths.
#[test]
#[ignore = "reads real finals named by RNX_FINAL_RECEIPTS"]
fn real_receipts_round_trip() {
	let base = PathBuf::from(std::env::var("RNX_FINAL_RECEIPTS").expect("RNX_FINAL_RECEIPTS"));
	let mut checked = 0;
	let mut failures = Vec::new();
	for entry in std::fs::read_dir(&base).unwrap().flatten() {
		if entry.path().join("RESULT.json").exists() {
			failures.extend(check_final(&entry.path()));
			checked += 1;
		}
	}
	assert!(checked > 0);
	assert!(
		failures.is_empty(),
		"{} failures in {checked} finals:\n{}",
		failures.len(),
		failures.join("\n")
	);
	eprintln!("{checked} finals round-trip");
}

fn written(name: &str, value: Value) -> (PathBuf, impl Drop) {
	struct Gone(PathBuf);
	impl Drop for Gone {
		fn drop(&mut self) {
			let _ = std::fs::remove_dir_all(&self.0);
		}
	}
	let dir = std::env::temp_dir().join(format!(
		"rnx-final-types-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&dir).unwrap();
	let path = dir.join(name);
	rnx_final::receipt::atomic_json(&path, &value).unwrap();
	(path, Gone(dir))
}

fn release() -> Value {
	json!({"status": "PASS", "ledger_sha256": "a", "roots": [], "acceptance_sha256": "b", "manifest_sha256": "c"})
}

#[test]
fn malformed_receipts_are_refused() {
	let (path, _g) = written("RELEASE.json", release());
	types::read::<Release>(&path).unwrap();
	for (label, value) in [
		("unknown field", {
			let mut v = release();
			v["extra"] = json!(1);
			v
		}),
		("wrong type", {
			let mut v = release();
			v["roots"] = json!("none");
			v
		}),
		("unknown status", {
			let mut v = release();
			v["status"] = json!("OK");
			v
		}),
		("bad root", {
			let mut v = release();
			v["roots"] = json!([{"path": "x"}]);
			v
		}),
	] {
		let (path, _g) = written("RELEASE.json", value);
		assert!(
			types::read::<Release>(&path).is_err(),
			"{label} was accepted"
		);
	}
	let result = json!({"status": "PASS", "candidate": "c", "seconds": 1.0, "checks": [], "error": null, "release_error": null, "manifest_error": null, "diagnostics_error": null, "finalization": null, "remote_context": null, "acceptance_scope": "complete", "complete_final": true, "publication_eligible": true, "free_bytes_before": 1, "free_bytes_after": 1, "records": {}});
	let (path, _g) = written("RESULT.json", result.clone());
	types::read::<FinalResult>(&path).unwrap();
	for key in ["status", "checks", "records", "acceptance_scope"] {
		let mut missing = result.clone();
		missing.as_object_mut().unwrap().remove(key);
		let (path, _g) = written("RESULT.json", missing);
		assert!(
			types::read::<FinalResult>(&path).is_err(),
			"a RESULT without {key} was accepted"
		);
	}
	let mut scope = result.clone();
	scope["acceptance_scope"] = json!("complete-final");
	let (path, _g) = written("RESULT.json", scope);
	assert!(
		types::read::<FinalResult>(&path).is_err(),
		"an unknown scope was accepted"
	);
}
