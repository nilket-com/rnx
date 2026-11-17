//! The Rust controller revalidates a final as it stands, its own and one the
//! retired Python controller wrote (committed copies from ADR 0284's witness
//! revision, `tests/witness/python-final-*`): a passed one passes; a failed
//! one, a tampered one and one whose release is incomplete or rootless are
//! refused. The Python controller revalidated Rust finals until ADR 0284;
//! ADR 0283's accepted revalidation in both directions is that direction's
//! last witness.
mod common;
use anyhow::Result;
use rnx_final::child::Done;
use rnx_final::controller::{self, Hooks, Settings, StageCall};
use rnx_final::failure::Environment;
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::scheduler::{Resources, Targets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

struct Dir(PathBuf);
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn dir() -> Dir {
	let path = std::env::temp_dir().join(format!("rnx-final-cross-{}", invocation_id()));
	std::fs::create_dir(&path).unwrap();
	Dir(path.canonicalize().unwrap())
}

struct Fake(bool);
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
		Ok(common::selection(
			scope,
			json!("c"),
			json!([["stage", ["command"]]]),
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
	fn resources(&self, _: &str, _: &[String], _: &Path, _: &Targets) -> Result<Resources> {
		Ok(["fake".to_string()].into())
	}
	fn weight(&self, _: &str, _: &[String], _: &Path) -> Result<usize> {
		Ok(1)
	}
	fn tools_identity(&self, _: &Settings) -> Result<Value> {
		Ok(json!({"controller": "rust"}))
	}
	fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
		let fail = self.0;
		Some((|| {
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
		})())
	}
}

fn rust_final(root: &Path, fail: bool) -> PathBuf {
	let settings = Settings {
		root: root.to_owned(),
		out: root.join("final"),
		names: None,
		scope: "required".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: Arc::new(Fake(fail)),
		tokens: Some(2),
	};
	controller::run(&settings).unwrap();
	root.join("final")
}

/// A final the Python controller wrote at the witness revision, its stage
/// faked as its own controls did; revalidation does not depend on where it
/// lies.
fn python_final(root: &Path, fail: bool) -> PathBuf {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("tests/witness")
		.join(if fail {
			"python-final-fail"
		} else {
			"python-final-pass"
		});
	let out = root.join("final");
	copy_tree(&source, &out);
	out
}

fn copy_tree(from: &Path, to: &Path) {
	std::fs::create_dir_all(to).unwrap();
	for entry in std::fs::read_dir(from).unwrap().flatten() {
		let target = to.join(entry.file_name());
		if entry.file_type().unwrap().is_dir() {
			copy_tree(&entry.path(), &target);
		} else {
			std::fs::copy(entry.path(), &target).unwrap();
		}
	}
}

fn rust_revalidate(out: &Path) -> Result<Value, String> {
	rnx_final::evidence::revalidate(out).map_err(|e| e.to_string())
}

fn tamper(out: &Path) {
	let path = out.join("ACCEPTANCE.json");
	let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
	value["candidate"] = json!("other");
	atomic_json(&path, &value).unwrap();
}

/// The release record without its roots, rebound so only that is wrong.
fn rootless(out: &Path) {
	let path = out.join("RELEASE.json");
	let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
	value.as_object_mut().unwrap().remove("roots");
	atomic_json(&path, &value).unwrap();
	let result_path = out.join("RESULT.json");
	let mut result: Value = serde_json::from_slice(&std::fs::read(&result_path).unwrap()).unwrap();
	result["records"]["RELEASE"] = json!(digest(&path).unwrap());
	atomic_json(&result_path, &result).unwrap();
}

fn unrelease(out: &Path) {
	let path = out.join("RELEASE.json");
	let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
	value["roots"][0]["state"] = json!("owned");
	atomic_json(&path, &value).unwrap();
	// Rebind the record so only the incomplete release is wrong.
	let result_path = out.join("RESULT.json");
	let mut result: Value = serde_json::from_slice(&std::fs::read(&result_path).unwrap()).unwrap();
	result["records"]["RELEASE"] = json!(digest(&path).unwrap());
	atomic_json(&result_path, &result).unwrap();
}

#[test]
fn each_writers_passed_final_revalidates() {
	for (writer, make) in [
		("rust", rust_final as fn(&Path, bool) -> PathBuf),
		("python", python_final),
	] {
		let d = dir();
		let out = make(&d.0, false);
		assert_eq!(
			rust_revalidate(&out).map(|v| v["status"].clone()),
			Ok(json!("PASS")),
			"{writer} final"
		);
		// The writer's own identity stays in the manifest; nothing is relabeled.
		let manifest: Value =
			serde_json::from_slice(&std::fs::read(out.join("MANIFEST.json")).unwrap()).unwrap();
		assert_eq!(manifest["tools"]["controller"], writer);
	}
}

#[test]
fn failed_tampered_unreleased_and_rootless_finals_are_refused() {
	for (writer, make) in [
		("rust", rust_final as fn(&Path, bool) -> PathBuf),
		("python", python_final),
	] {
		for case in ["failed", "tampered", "unreleased", "rootless"] {
			let d = dir();
			let out = make(&d.0, case == "failed");
			match case {
				"tampered" => tamper(&out),
				"unreleased" => unrelease(&out),
				"rootless" => rootless(&out),
				_ => {}
			}
			assert!(
				rust_revalidate(&out).is_err(),
				"{writer} {case} final passed revalidation"
			);
		}
	}
}
