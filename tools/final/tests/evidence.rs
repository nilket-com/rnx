//! Ports of the Python evidence controls (test_final_evidence.py).
mod common;
use rnx_final::evidence::{Owners, compact, recover, release, revalidate, tree};
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// A child forked by another test thread holds every open descriptor until
/// it execs, a lock included; tests that fork or read a lock take turns.
static FORKS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn forks() -> std::sync::MutexGuard<'static, ()> {
	FORKS
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Fixture {
	root: PathBuf,
	out: PathBuf,
	registry: PathBuf,
}

impl Fixture {
	fn new() -> (Self, Owners) {
		let root = std::env::temp_dir().join(format!("rnx-final-evidence-{}", invocation_id()));
		std::fs::create_dir(&root).unwrap();
		let fixture = Self {
			out: root.join("final"),
			registry: root.join("owners"),
			root,
		};
		let owner = Owners::new(&fixture.registry, &fixture.out).unwrap();
		(fixture, owner)
	}
}

impl Drop for Fixture {
	fn drop(&mut self) {
		writable(&self.root);
		let _ = std::fs::remove_dir_all(&self.root);
	}
}

fn set_mode(path: &Path, mode: u32) {
	use std::os::unix::fs::PermissionsExt as _;
	std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn writable(path: &Path) {
	if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
		set_mode(path, 0o700);
		for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
			writable(&entry.path());
		}
	}
}

fn no_drain(_: &Path, _: &rnx_final::evidence::Ledger) -> anyhow::Result<()> {
	Ok(())
}

fn ledger(owner: &Owners) -> Value {
	serde_json::from_slice(&std::fs::read(&owner.path).unwrap()).unwrap()
}

#[test]
fn identity_and_interior_link() {
	let (f, mut owner) = Fixture::new();
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	std::fs::write(p.join("data"), "abc").unwrap();
	let outside = f.root.join("outside");
	std::fs::write(&outside, "keep").unwrap();
	std::os::unix::fs::symlink(&outside, p.join("link")).unwrap();
	assert_eq!(tree(&p, &Map::new()).unwrap()["entries"], 2);
	assert_eq!(owner.release().unwrap()["status"], "PASS");
	assert!(!p.exists());
	assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep");
	assert_eq!(release(&owner.path).unwrap()["status"], "PASS");
}

#[test]
fn refusals() {
	let (f, mut owner) = Fixture::new();
	assert!(owner.assign(&f.root, "x", Some("../escape")).is_err());
	std::fs::create_dir(f.root.join("exists")).unwrap();
	assert!(owner.assign(&f.root, "x", Some("exists")).is_err());
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	std::fs::rename(&p, f.root.join("saved")).unwrap();
	std::os::unix::fs::symlink(f.root.join("saved"), &p).unwrap();
	assert!(owner.release().is_err(), "a link in place of the root");
	std::fs::remove_file(&p).unwrap();
	std::fs::create_dir(&p).unwrap();
	assert!(
		owner.release().is_err(),
		"a different directory in place of the root"
	);
}

#[test]
fn ancestor_and_incomplete() {
	let (f, mut owner) = Fixture::new();
	let parent = f.root.join("parent");
	std::fs::create_dir(&parent).unwrap();
	let p = owner.assign(&parent, "bulk", None).unwrap();
	std::fs::rename(&parent, f.root.join("renamed")).unwrap();
	std::os::unix::fs::symlink(f.root.join("renamed"), &parent).unwrap();
	assert!(owner.release().is_err(), "a linked ancestor");
	std::fs::remove_file(&parent).unwrap();
	std::fs::rename(f.root.join("renamed"), &parent).unwrap();
	std::fs::write(p.join("nonempty"), "keep").unwrap();
	let mut row = ledger(&owner);
	row["roots"][0]["state"] = json!("creating");
	atomic_json(&owner.path, &row).unwrap();
	assert!(
		owner.release().is_err(),
		"an incomplete root that is not empty"
	);
}

#[test]
fn release_failure_and_retry_after_removal() {
	let (f, mut owner) = Fixture::new();
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	// A read-only interior directory makes removal fail part way.
	std::fs::create_dir(p.join("locked")).unwrap();
	std::fs::write(p.join("locked/file"), "x").unwrap();
	set_mode(&p.join("locked"), 0o500);
	assert!(owner.release().is_err());
	assert!(p.exists());
	// Removed by hand, as a crash after removal but before the journal.
	set_mode(&p.join("locked"), 0o700);
	std::fs::remove_dir_all(&p).unwrap();
	assert_eq!(owner.release().unwrap()["status"], "PASS");
}

#[test]
fn live_owner_and_killed_owner() {
	let _turn = forks();
	let (f, mut owner) = Fixture::new();
	assert!(
		recover(&f.registry, no_drain)
			.unwrap_err()
			.to_string()
			.contains("live final")
	);
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	owner.close();
	recover(&f.registry, no_drain).unwrap();
	assert!(!p.exists());
	let names: Vec<String> = std::fs::read_dir(&f.registry)
		.unwrap()
		.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
		.collect();
	assert!(
		names.iter().any(|n| n.ends_with("-RECOVERY.json")),
		"{names:?}"
	);
	assert!(
		!names
			.iter()
			.any(|n| n.ends_with(".owner.json") || n.ends_with(".lock")),
		"{names:?}"
	);
}

#[test]
fn compact_and_revalidate() {
	let (f, mut owner) = Fixture::new();
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	std::fs::write(p.join("data"), "secret payload").unwrap();
	let stage = f.out.join("stage.json");
	let data = p.join("data");
	atomic_json(
		&stage,
		&common::stage_receipt(
			true,
			0,
			json!(["RNX_SECRET"]),
			json!({data.to_string_lossy(): digest(&data).unwrap()}),
		),
	)
	.unwrap();
	let selection = common::selection("required", json!("c"), json!([["x", ["declared-command"]]]));
	let mut evidence = Map::new();
	evidence.insert(
		"x".into(),
		json!({"path": stage, "sha256": digest(&stage).unwrap()}),
	);
	let manifest = compact(
		&selection,
		&evidence,
		&owner,
		&json!({"compiler": "version"}),
	)
	.unwrap();
	let text = manifest.to_string();
	assert!(
		!text.contains("secret payload")
			&& !text.contains("secret env")
			&& text.contains("RNX_SECRET")
	);
	assert_eq!(manifest["roots"][0]["payload"]["reused_file_digests"], 1);
	atomic_json(&f.out.join("MANIFEST.json"), &manifest).unwrap();
	atomic_json(
		&f.out.join("ACCEPTANCE.json"),
		&json!({"status": "PASS", "candidate": "c", "acceptance_scope": "required", "mode": "ordinary", "checks": [], "seconds": 0.0, "memory": {"tokens": "2", "events_before": null, "events_after": null}}),
	)
	.unwrap();
	let mut r = owner.release().unwrap();
	r["acceptance_sha256"] = json!(digest(&f.out.join("ACCEPTANCE.json")).unwrap());
	r["manifest_sha256"] = json!(digest(&f.out.join("MANIFEST.json")).unwrap());
	atomic_json(&f.out.join("RELEASE.json"), &r).unwrap();
	let records: Map<String, Value> = ["ACCEPTANCE", "MANIFEST", "RELEASE"]
		.iter()
		.map(|n| {
			(
				n.to_string(),
				json!(digest(&f.out.join(format!("{n}.json"))).unwrap()),
			)
		})
		.collect();
	atomic_json(&f.out.join("RESULT.json"), &json!({"status": "PASS", "candidate": "c", "seconds": 1.0, "checks": [], "error": null, "release_error": null, "manifest_error": null, "diagnostics_error": null, "finalization": null, "remote_context": null, "acceptance_scope": "required", "complete_final": false, "publication_eligible": true, "free_bytes_before": 1, "free_bytes_after": 1, "records": records})).unwrap();
	assert_eq!(
		revalidate(&f.out).unwrap()["bulk"],
		"released; not rehashed"
	);
	// The same receipt schema remains readable for old finals. When the
	// bootstrap declares a clean run, the terminal cleanup must be bound.
	let result_path = f.out.join("RESULT.json");
	let mut result: Value = serde_json::from_slice(&std::fs::read(&result_path).unwrap()).unwrap();
	let mut live = manifest;
	live["tools"]["controller"]["bootstrap"] =
		json!({"clean_run":{"directory":"/owned","baseline":"base"}});
	let manifest_path = f.out.join("MANIFEST.json");
	let release_path = f.out.join("RELEASE.json");
	atomic_json(&manifest_path, &live).unwrap();
	r["manifest_sha256"] = json!(digest(&manifest_path).unwrap());
	atomic_json(&release_path, &r).unwrap();
	result["records"]["MANIFEST"] = r["manifest_sha256"].clone();
	result["records"]["RELEASE"] = json!(digest(&release_path).unwrap());
	atomic_json(&result_path, &result).unwrap();
	assert!(
		revalidate(&f.out)
			.unwrap_err()
			.to_string()
			.contains("missing cleanup evidence")
	);
	let cleanup_path = f.out.join("CLEANUP.json");
	atomic_json(&cleanup_path,&json!({"candidate":"c","baseline":"base","started":1,"finished":2,"directory":"/owned","removed":true,"targets_cleaned":1,"peak_bytes":{"build":4}})).unwrap();
	result["records"]["CLEANUP"] = json!(digest(&cleanup_path).unwrap());
	atomic_json(&result_path, &result).unwrap();
	revalidate(&f.out).unwrap();
	std::fs::remove_file(f.out.join("RELEASE.json")).unwrap();
	assert!(revalidate(&f.out).is_err());
}

#[test]
fn the_manifest_cap_keeps_the_bulk() {
	let (f, mut owner) = Fixture::new();
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	let selection = common::selection("required", json!("c"), json!([]));
	assert!(
		compact(
			&selection,
			&Map::new(),
			&owner,
			&json!({"too_big": "x".repeat(300000)})
		)
		.is_err()
	);
	assert!(p.exists());
}

#[test]
fn creating_absent_and_empty() {
	for absent in [false, true] {
		let (f, mut owner) = Fixture::new();
		let p = owner.assign(&f.root, "bulk", None).unwrap();
		if absent {
			std::fs::remove_dir(&p).unwrap();
		}
		let mut row = ledger(&owner);
		row["roots"][0]["state"] = json!("creating");
		row["roots"][0].as_object_mut().unwrap().remove("inode");
		row["roots"][0].as_object_mut().unwrap().remove("device");
		atomic_json(&owner.path, &row).unwrap();
		assert_eq!(owner.release().unwrap()["status"], "PASS");
		assert!(!p.exists());
		owner.retire().unwrap();
		assert_eq!(std::fs::read_dir(&f.registry).unwrap().count(), 0);
	}
}

#[test]
fn an_interrupted_retirement_is_recoverable() {
	let (f, mut owner) = Fixture::new();
	owner.assign(&f.root, "bulk", None).unwrap();
	owner.release().unwrap();
	let mut row = ledger(&owner);
	row["status"] = json!("retiring");
	atomic_json(&owner.path, &row).unwrap();
	std::fs::remove_dir_all(&owner.groups).unwrap();
	let lock = owner.path.with_extension("lock");
	owner.close();
	std::fs::remove_file(lock).unwrap();
	recover(&f.registry, no_drain).unwrap();
	assert_eq!(std::fs::read_dir(&f.registry).unwrap().count(), 0);
}

/// The payload identity is the one the Python controller's tree() computed
/// at ADR 0284's witness revision for the same payload.
#[test]
fn tree_identity_matches_the_witness() {
	let _turn = forks();
	let (f, mut owner) = Fixture::new();
	let p = owner.assign(&f.root, "bulk", None).unwrap();
	std::fs::create_dir_all(p.join("a/b")).unwrap();
	std::fs::write(p.join("a/b/file"), "contents").unwrap();
	std::fs::write(p.join("é"), "").unwrap();
	std::fs::write(p.join("Z"), "z").unwrap();
	std::os::unix::fs::symlink("../elsewhere", p.join("a/link")).unwrap();
	let mut actual = tree(&p, &Map::new()).unwrap();
	actual.as_object_mut().unwrap().remove("seconds");
	// Directory sizes and allocated blocks depend on the backing filesystem.
	use std::os::unix::fs::MetadataExt;
	let entries = ["a", "a/b", "a/b/file", "é", "Z", "a/link"]
		.map(|name| std::fs::symlink_metadata(p.join(name)).unwrap());
	let mut expected = common::witness::expected("tree", &[]);
	expected["logical_bytes"] = json!(entries.iter().map(|m| m.size()).sum::<u64>());
	expected["allocated_bytes"] = json!(entries.iter().map(|m| m.blocks() * 512).sum::<u64>());
	assert_eq!(actual, expected);
}

/// A Python stage program's own process group, recorded through its
/// run_process, is drained by recovery after its driver was killed.
#[test]
fn a_killed_driver_has_its_registered_producer_drained() {
	let _turn = forks();
	let (f, mut owner) = Fixture::new();
	let column = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../tools/column")
		.canonicalize()
		.unwrap();
	let script = f.root.join("driver.py");
	std::fs::write(&script, format!("import sys, os\nfrom owned_build import run_process\nrun_process([sys.executable, '-c', 'import time; print(\"ready\", flush=True); time.sleep(120)'], env=dict(os.environ, RNX_FINAL_GROUPS={:?}))\n", owner.groups.to_string_lossy())).unwrap();
	let mut driver = std::process::Command::new("python3")
		.arg(&script)
		.env("PYTHONPATH", &column)
		.stdout(std::process::Stdio::piped())
		.spawn()
		.unwrap();
	let mut line = String::new();
	std::io::BufRead::read_line(
		&mut std::io::BufReader::new(driver.stdout.as_mut().unwrap()),
		&mut line,
	)
	.unwrap();
	assert_eq!(line.trim(), "ready");
	driver.kill().unwrap();
	driver.wait().unwrap();
	owner.assign(&f.root, "bulk", None).unwrap();
	owner.close();
	std::fs::remove_dir_all(&f.out).unwrap();
	recover(&f.registry, rnx_final::guardian::drain_registry).unwrap();
	let receipt = std::fs::read_dir(&f.registry)
		.unwrap()
		.flatten()
		.find(|e| e.file_name().to_string_lossy().ends_with("-RECOVERY.json"))
		.unwrap();
	let rows: Value = serde_json::from_slice(&std::fs::read(receipt.path()).unwrap()).unwrap();
	let groups = rows["producer_groups"].as_array().unwrap();
	assert!(
		!groups.is_empty() && groups.iter().all(|g| g["drained"] == true),
		"{rows}"
	);
}

#[test]
fn fresh_targets_and_a_target_override_are_refused() {
	let (f, _owner) = Fixture::new();
	std::fs::create_dir(f.root.join("target")).unwrap();
	let empty = rnx_final::failure::Environment::new();
	assert!(rnx_final::controller::fresh_violation(&f.root, false, &empty).is_none());
	assert!(
		rnx_final::controller::fresh_violation(&f.root, true, &empty)
			.unwrap()
			.contains("preexisting")
	);
	let overridden: rnx_final::failure::Environment =
		[("CARGO_TARGET_DIR".to_string(), "override".to_string())].into();
	assert!(
		rnx_final::controller::fresh_violation(&f.root, false, &overridden)
			.unwrap()
			.contains("CARGO_TARGET_DIR")
	);
}

#[test]
fn the_disk_check_records_each_filesystem() {
	let (f, _owner) = Fixture::new();
	let out = f.root.join("checked");
	std::fs::create_dir(&out).unwrap();
	let result =
		rnx_final::controller::disk_check(&f.root, &out, &rnx_final::failure::Environment::new());
	let text = std::fs::read_to_string(out.join("disk-admission.json")).unwrap();
	let row: Value = serde_json::from_str(&text).unwrap();
	assert_eq!(row["filesystems"].as_array().unwrap().len(), 2);
	assert_eq!(row["status"] == "PASS", result.is_ok());
	assert_eq!(
		row["filesystems"][0]["minimum_free_bytes"],
		json!(rnx_final::controller::DISK_FLOOR_BYTES)
	);
}

#[test]
fn named_admission_numbers_survive_a_real_payload_release() {
	let (f, mut owner) = Fixture::new();
	let observation = job_budget::memory::Observation {
		cpu_ids: vec![0],
		mem_available_bytes: 41000000000,
		ancestor_headroom_bytes: 1073741824,
		binding_cgroup: "/fixture/capture".into(),
		binding_limit: "memory.high".into(),
		groups: vec![],
	};
	let error = job_budget::memory::capacity(
		4,
		&observation,
		2 * 1024 * 1024 * 1024,
		2 * 1024 * 1024 * 1024,
	)
	.unwrap_err()
	.to_string();
	let numbers = json!({"host_available_bytes": 41000000000u64, "cgroup_headroom_bytes": 1073741824, "binding_cgroup": "/fixture/capture", "binding_limit": "memory.high", "reserve_bytes": 2147483648u64, "per_worker_bytes": 2147483648u64, "workers": 0});
	let bulk = owner.assign(&f.root, "capture", None).unwrap();
	atomic_json(
		&bulk.join("RESULT.json"),
		&json!({"status": "FAIL", "error": error, "admission": numbers}),
	)
	.unwrap();
	std::fs::write(bulk.join("child.log"), format!("{error}\n")).unwrap();
	let env: rnx_final::failure::Environment =
		[("SHLVL", "1"), ("X", "0"), ("TOKEN", "s3cr3t-abcdef")]
			.iter()
			.map(|(k, v)| (k.to_string(), v.to_string()))
			.collect();
	rnx_final::failure::retain(
		&f.out,
		"capture",
		Some(&bulk.join("RESULT.json")),
		&[],
		std::slice::from_ref(&bulk),
		&env,
		"stage failed",
	)
	.unwrap();
	assert_eq!(owner.release().unwrap()["status"], "PASS");
	assert!(!bulk.exists());
	let index: Value =
		serde_json::from_slice(&std::fs::read(f.out.join("FAILURE.json")).unwrap()).unwrap();
	for (key, number) in numbers.as_object().unwrap() {
		let shown = number
			.as_str()
			.map(str::to_owned)
			.unwrap_or_else(|| number.to_string());
		assert!(
			index["error"]
				.as_str()
				.unwrap()
				.contains(&format!("{key}={shown}")),
			"{key}"
		);
	}
	let projection: Value = serde_json::from_slice(
		&std::fs::read(f.out.join(index["files"][0]["path"].as_str().unwrap())).unwrap(),
	)
	.unwrap();
	assert_eq!(projection["admission"], numbers);
}

/// A ledger the Python controller wrote (schema 1, as ADR 0284's witness
/// fixture records its shape) for an owner that died is still recovered:
/// its root is released and the ledger retired.
#[test]
fn a_python_schema_one_ledger_is_recovered() {
	use std::os::unix::fs::MetadataExt as _;
	let _turn = forks();
	let root = std::env::temp_dir().join(format!("rnx-final-schema-one-{}", invocation_id()));
	let (registry, out, parent) = (root.join("registry"), root.join("out"), root.join("parent"));
	let id = &invocation_id()[..32];
	for d in [&registry, &out, &parent] {
		std::fs::create_dir_all(d).unwrap();
	}
	std::fs::create_dir(registry.join(format!("{id}.groups"))).unwrap();
	let bulk = parent.join(format!("{id}-stage"));
	std::fs::create_dir(&bulk).unwrap();
	std::fs::write(bulk.join("payload"), "data").unwrap();
	let (b, p) = (
		std::fs::metadata(&bulk).unwrap(),
		std::fs::metadata(&parent).unwrap(),
	);
	let template = common::witness::expected(
		"ledger-schema-1",
		&[
			("<REGISTRY>", &registry.to_string_lossy()),
			("<OUT>", &out.to_string_lossy()),
			("<PARENT>", &parent.to_string_lossy()),
			("<ID>", id),
		],
	);
	let mut ledger = template.clone();
	for (key, value) in [
		("device", b.dev()),
		("inode", b.ino()),
		("parent_device", p.dev()),
		("parent_inode", p.ino()),
	] {
		ledger["roots"][0][key] = json!(value);
	}
	assert_eq!(ledger["schema"], 1);
	let path = registry.join(format!("{id}.owner.json"));
	atomic_json(&path, &ledger).unwrap();
	// The Python owner held this lock while alive; it is gone.
	std::fs::write(registry.join(format!("{id}.owner.lock")), "").unwrap();
	recover(&registry, no_drain).unwrap();
	assert!(!bulk.exists(), "the schema-1 root was released");
	assert!(!path.exists(), "the schema-1 ledger was retired");
	let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn interrupted_stage_scratch_is_recovered_and_a_foreign_candidate_survives() {
	let (f, mut owner) = Fixture::new();
	let foreign = f.root.join("t0");
	std::fs::create_dir(&foreign).unwrap();
	std::fs::write(foreign.join("sentinel"), "keep").unwrap();
	assert!(
		owner
			.assign(&f.root, "stage-temporaries", Some("t0"))
			.is_err()
	);
	let owned = owner
		.assign(&f.root, "stage-temporaries", Some("t1"))
		.unwrap();
	std::fs::write(owned.join("partial"), "interrupted").unwrap();
	owner.close();
	recover(&f.registry, no_drain).unwrap();
	assert!(!owned.exists());
	assert_eq!(std::fs::read(foreign.join("sentinel")).unwrap(), b"keep");
}
