//! Each controller revalidates the other's final as it stands: a passed one
//! passes; a failed one, a tampered one and one whose release is incomplete
//! are refused by both.
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

fn column() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../tools/column")
		.canonicalize()
		.unwrap()
}

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

/// A final by the Python controller, its stage faked as its own controls do.
fn python_final(root: &Path, fail: bool) -> PathBuf {
	let script = r#"
import sys, json
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import child_result as child, final, stage_fakes
root, fail = Path(sys.argv[1]), sys.argv[2] == '1'
s = {'schema': 1, 'scope': 'required', 'publication_eligible': True, 'record': {'candidate': 'c', 'baseline': 'b', 'plan': 'p', 'record': '9999', 'phase': 'impl', 'paths': [], 'required': []}, 'source': 's', 'policy': 'p', 'root': '/fixture', 'out': '/fixture/final', 'stages': [['stage', ['command']]]}
finalization = {'schema': 1, 'status': 'PASS', 'candidate': 'c', 'scope': 'required', 'publication_eligible': True, 'source': 's', 'selection': 'x', 'evidence': {}, 'durations': {}}
def execute(command, **kw):
    p = Path(kw['env']['RNX_FINAL_ROUTING_ROOT']); (p/'payload').write_text('data')
    d = kw['directory']; d.mkdir()
    child.atomic_json(d/'RESULT.json', dict(schema=1, status='FAIL' if fail else 'PASS', command=['command'], process_exit=int(fail), seconds=0.0, cleanup_seconds=0, cleanup=[], context={'schema': 1, 'invocation': 'i', 'root': '/fixture', 'cwd': '/fixture', 'command': ['command'], 'candidate': 'c', 'source': 's', 'environment': {'keys': [], 'sha256': 'x'}, 'kind': 'leaf'}, artifacts={str(p/'payload'): child.digest(p/'payload')}))
    return SimpleNamespace(returncode=int(fail), validation={'path': str(d/'RESULT.json'), 'sha256': child.digest(d/'RESULT.json')})
with patch.object(final, 'disk_check'), patch.object(final, 'tools_identity', return_value={'controller': 'python'}), patch.object(final.child, 'head', return_value='c'), \
        patch.object(final.gate, 'select', return_value=s), patch.object(final.gate, 'check_selection'), patch.object(final.gate, 'finalize', return_value=finalization), \
        patch.object(final.child, 'run', side_effect=execute), stage_fakes.inline(final):
    final.run(root, root/'final')
"#;
	let output = std::process::Command::new("python3")
		.args(["-B", "-c", script])
		.arg(root)
		.arg(if fail { "1" } else { "0" })
		.current_dir(column())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	root.join("final")
}

fn python_revalidate(out: &Path) -> Result<Value, String> {
	let script = "import sys, json; from pathlib import Path; from final_evidence import revalidate\ntry: print(json.dumps(revalidate(Path(sys.argv[1]))))\nexcept Exception as e: print(json.dumps({'refused': repr(e)}))";
	let output = std::process::Command::new("python3")
		.args(["-B", "-c", script])
		.arg(out)
		.current_dir(column())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let value: Value = serde_json::from_slice(&output.stdout).unwrap();
	match value.get("refused") {
		Some(r) => Err(r.as_str().unwrap().to_owned()),
		None => Ok(value),
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
fn each_controller_revalidates_the_others_final() {
	for (writer, make) in [
		("rust", rust_final as fn(&Path, bool) -> PathBuf),
		("python", python_final),
	] {
		let d = dir();
		let out = make(&d.0, false);
		let (by_python, by_rust) = (python_revalidate(&out), rust_revalidate(&out));
		assert_eq!(
			by_python.as_ref().map(|v| v["status"].clone()),
			Ok(json!("PASS")),
			"{writer} final, Python revalidation"
		);
		assert_eq!(
			by_rust.as_ref().map(|v| v["status"].clone()),
			Ok(json!("PASS")),
			"{writer} final, Rust revalidation"
		);
		assert_eq!(
			by_python.unwrap(),
			by_rust.unwrap(),
			"{writer}: the two revalidations differ"
		);
		// The writer's own identity stays in the manifest; nothing is relabeled.
		let manifest: Value =
			serde_json::from_slice(&std::fs::read(out.join("MANIFEST.json")).unwrap()).unwrap();
		assert_eq!(manifest["tools"]["controller"], writer);
	}
}

#[test]
fn both_controllers_refuse_failed_tampered_and_unreleased_finals() {
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
				python_revalidate(&out).is_err(),
				"{writer} {case} final passed Python revalidation"
			);
			assert!(
				rust_revalidate(&out).is_err(),
				"{writer} {case} final passed Rust revalidation"
			);
		}
	}
}
