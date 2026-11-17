//! The disk admission has two implementations while the Python launchers
//! remain: the controller's (`controller::disk_check`) and
//! `ci/disk_space.py`. Through each one's own reading seam (a function
//! here, `shutil.disk_usage` there), on the same readings, they give the
//! same verdict and the same record, byte for byte: for given paths, and
//! from each entry point with its own path collection, `CARGO_TARGET_DIR`
//! included. A planted change to either floor, or a verdict that disagrees
//! with its record, is caught.
use rnx_final::controller::{DISK_FLOOR_BYTES, disk_check_with, disk_observation};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PYTHON: &str = r#"
import json, sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
import disk_space
frees, paths, minimum = json.loads(sys.argv[2]), json.loads(sys.argv[3]), json.loads(sys.argv[4])
with patch('disk_space.shutil.disk_usage', side_effect=lambda p: SimpleNamespace(free=frees[str(p)])):
    result = disk_space.observe(paths) if minimum is None else disk_space.observe(paths, minimum)
print(json.dumps(result, indent=2))
"#;

fn python(paths: &[PathBuf], frees: &BTreeMap<String, u64>, minimum: Option<u64>) -> String {
	let ci = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci");
	let output = std::process::Command::new("python3")
		.args(["-I", "-B", "-c", PYTHON])
		.arg(&ci)
		.arg(json!(frees).to_string())
		.arg(json!(paths).to_string())
		.arg(json!(minimum).to_string())
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).unwrap()
}

fn reader(frees: &BTreeMap<String, u64>) -> impl Fn(&Path) -> anyhow::Result<u64> + '_ {
	move |p: &Path| {
		frees
			.get(&*p.to_string_lossy())
			.copied()
			.ok_or_else(|| anyhow::anyhow!("no reading for {}", p.display()))
	}
}

/// The record, after checking that the verdict the controller acts on
/// agrees with the status it records.
fn rust(paths: &[PathBuf], frees: &BTreeMap<String, u64>, floor: u64) -> String {
	let (pass, text) = disk_observation(paths, floor, &reader(frees)).unwrap();
	agree(pass, &text).unwrap();
	text
}

fn agree(pass: bool, text: &str) -> anyhow::Result<()> {
	let status: Value = serde_json::from_str(text)?;
	anyhow::ensure!(
		status["status"] == if pass { "PASS" } else { "FAIL" },
		"the verdict {pass} disagrees with the record {}",
		status["status"]
	);
	Ok(())
}

/// A case: its name, the paths, each filesystem's free bytes, the verdict.
type Case<'a> = (&'a str, Vec<PathBuf>, BTreeMap<String, u64>, &'a str);

struct Dir(PathBuf);
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

#[test]
fn both_rules_agree_and_a_changed_floor_is_caught() {
	let dir = {
		let d = std::env::temp_dir().join(format!(
			"rnx-final-disk-{}",
			rnx_final::receipt::invocation_id()
		));
		std::fs::create_dir_all(&d).unwrap();
		Dir(d.canonicalize().unwrap())
	};
	let (a, b) = (dir.0.join("root"), dir.0.join("out"));
	std::fs::create_dir_all(&a).unwrap();
	std::fs::create_dir_all(&b).unwrap();
	// A target directory not made yet is judged by its nearest existing ancestor.
	let missing = dir.0.join("target/deep/not-yet");
	let key = |p: &Path| p.to_string_lossy().into_owned();
	let floor = DISK_FLOOR_BYTES;
	let cases: Vec<Case> = vec![
		(
			"every path has room",
			vec![a.clone(), b.clone()],
			[(key(&a), floor * 2), (key(&b), floor + 1)].into(),
			"PASS",
		),
		(
			"exactly the floor passes",
			vec![a.clone()],
			[(key(&a), floor)].into(),
			"PASS",
		),
		(
			"one byte short fails",
			vec![a.clone(), b.clone()],
			[(key(&a), floor * 2), (key(&b), floor - 1)].into(),
			"FAIL",
		),
		(
			"a missing target directory counts at its ancestor",
			vec![a.clone(), missing.clone()],
			[(key(&a), floor), (key(&dir.0), floor - 1)].into(),
			"FAIL",
		),
	];
	for (name, paths, frees, status) in &cases {
		let (r, p) = (rust(paths, frees, floor), python(paths, frees, None));
		assert_eq!(r, p, "{name}");
		let v: Value = serde_json::from_str(&r).unwrap();
		assert_eq!(v["status"], *status, "{name}");
		assert_eq!(v["filesystems"].as_array().unwrap().len(), paths.len());
	}
	// The ancestor is what was read.
	let v: Value = serde_json::from_str(&rust(&cases[3].1, &cases[3].2, floor)).unwrap();
	assert_eq!(v["filesystems"][1]["path"], json!(key(&dir.0)));
	// A floor changed on either side no longer agrees at the boundary.
	let (_, paths, frees, _) = &cases[1];
	assert_ne!(rust(paths, frees, floor + 1), python(paths, frees, None));
	assert_ne!(
		rust(paths, frees, floor),
		python(paths, frees, Some(floor + 1))
	);
	// A verdict that disagrees with its record is caught.
	let (pass, text) = disk_observation(paths, floor, &reader(frees)).unwrap();
	assert!(agree(!pass, &text).is_err());
}

const PYTHON_MAIN: &str = r#"
import contextlib, io, json, os, sys
from types import SimpleNamespace
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
import disk_space
frees, root, out, target = json.loads(sys.argv[2]), sys.argv[3], sys.argv[4], json.loads(sys.argv[5])
environ = {k: v for k, v in os.environ.items() if k != 'CARGO_TARGET_DIR'}
if target is not None: environ['CARGO_TARGET_DIR'] = target
stdout = io.StringIO()
with patch.dict(os.environ, environ, clear=True), patch.object(sys, 'argv', ['disk_space.py', '--path', root, '--path', out]), \
		patch('disk_space.shutil.disk_usage', side_effect=lambda p: SimpleNamespace(free=frees[str(p)])), contextlib.redirect_stdout(stdout):
	code = disk_space.main()
print(json.dumps({'code': code, 'record': stdout.getvalue()}))
"#;

/// Each entry point collects its own paths: the controller's `disk_check`
/// (root, final directory, `CARGO_TARGET_DIR`) and `ci/disk_space.py`'s
/// `main` (its `--path`s, `CARGO_TARGET_DIR`). A failing target directory
/// fails both; an absent or empty one is not counted by either.
#[test]
fn both_entry_points_count_the_target_directory() {
	let dir = {
		let d = std::env::temp_dir().join(format!(
			"rnx-final-disk-entry-{}",
			rnx_final::receipt::invocation_id()
		));
		std::fs::create_dir_all(&d).unwrap();
		Dir(d.canonicalize().unwrap())
	};
	let (root, target) = (dir.0.join("root"), dir.0.join("target"));
	std::fs::create_dir_all(&root).unwrap();
	std::fs::create_dir_all(&target).unwrap();
	let key = |p: &Path| p.to_string_lossy().into_owned();
	let floor = DISK_FLOOR_BYTES;
	for (case, value, verdict) in [
		("absent", None, "PASS"),
		("empty", Some(String::new()), "PASS"),
		("a full target directory", Some(key(&target)), "FAIL"),
	] {
		let out = dir.0.join(format!("out-{}", case.replace(' ', "-")));
		std::fs::create_dir_all(&out).unwrap();
		let frees: BTreeMap<String, u64> = [
			(key(&root), floor * 2),
			(key(&out), floor * 2),
			(key(&target), floor - 1),
		]
		.into();
		let environment: rnx_final::failure::Environment = value
			.clone()
			.map(|v| [("CARGO_TARGET_DIR".to_string(), v)].into())
			.unwrap_or_default();
		let admitted = disk_check_with(&root, &out, &environment, &reader(&frees)).is_ok();
		let record = std::fs::read_to_string(out.join("disk-admission.json")).unwrap();
		agree(admitted, &record).unwrap();
		let ci = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci");
		let output = std::process::Command::new("python3")
			.args(["-I", "-B", "-c", PYTHON_MAIN])
			.arg(&ci)
			.arg(json!(frees).to_string())
			.arg(&root)
			.arg(&out)
			.arg(json!(value).to_string())
			.output()
			.unwrap();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		let python: Value = serde_json::from_slice(&output.stdout).unwrap();
		assert_eq!(python["record"], json!(record), "{case}");
		assert_eq!(python["code"] == 0, admitted, "{case}");
		let v: Value = serde_json::from_str(&record).unwrap();
		assert_eq!(v["status"], verdict, "{case}");
	}
}
