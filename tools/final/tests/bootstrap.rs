//! The entry and its handoff: the entry refuses before building, the build
//! is bound to the candidate, and the controller believes a record only
//! once, only in order, and only for the binary it is.
use rnx_final::bootstrap::{accept, boottime};
use rnx_final::receipt::{digest, invocation_id};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/final.sh");

struct Checkout(PathBuf);

impl Checkout {
	/// A committed checkout whose rnx-final writes its arguments to a file.
	fn new(source: &str) -> Self {
		let root = std::env::temp_dir().join(format!("rnx-final-bootstrap-{}", invocation_id()));
		std::fs::create_dir_all(root.join("tools/final/src")).unwrap();
		let root = root.canonicalize().unwrap();
		std::fs::copy(SCRIPT, root.join("tools/final/final.sh")).unwrap();
		std::fs::write(
			root.join("Cargo.toml"),
			"[workspace]\nmembers = [\"tools/final\"]\nresolver = \"3\"\n",
		)
		.unwrap();
		std::fs::write(
			root.join("tools/final/Cargo.toml"),
			"[package]\nname = \"rnx-final\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
		)
		.unwrap();
		std::fs::write(root.join("tools/final/src/main.rs"), source).unwrap();
		std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
		let cargo = std::process::Command::new("cargo")
			.args(["+1.98.1", "generate-lockfile", "--offline"])
			.current_dir(&root)
			.output()
			.unwrap();
		assert!(
			cargo.status.success(),
			"{}",
			String::from_utf8_lossy(&cargo.stderr)
		);
		let checkout = Self(root);
		checkout.git(&["init", "-q"]);
		checkout.git(&["add", "-A"]);
		checkout.git(&[
			"-c",
			"user.name=t",
			"-c",
			"user.email=t@invalid",
			"-c",
			"commit.gpgsign=false",
			"commit",
			"-q",
			"-m",
			"impl: adr 9999 x",
		]);
		checkout
	}
	fn git(&self, args: &[&str]) -> String {
		let output = std::process::Command::new("git")
			.arg("-C")
			.arg(&self.0)
			.args(args)
			.output()
			.unwrap();
		assert!(output.status.success(), "{args:?}");
		String::from_utf8(output.stdout).unwrap().trim().to_owned()
	}
	/// The entry, with ample fixture memory unless the test gives its own:
	/// the host's real headroom varies with load (inside a final, its own
	/// unit's), and these controls are about the entry, not the host. The
	/// real sources are the complete final's own path.
	fn entry(&self, env: &[(&str, &str)]) -> std::process::Output {
		self.launch(env, true)
	}

	fn launch(&self, env: &[(&str, &str)], with_memory: bool) -> std::process::Output {
		let mut command = std::process::Command::new("sh");
		command
			.arg(self.0.join("tools/final/final.sh"))
			.args(["--out", "OUT"])
			.env_remove("RNX_BOOTSTRAP_FIXTURE")
			.env_remove("RNX_BOOTSTRAP_DISK_BYTES");
		if with_memory && !env.iter().any(|(k, _)| *k == "RNX_BOOTSTRAP_CGROUP_ROOT") {
			for (k, v) in fixture_memory(&self.0.join("target/ample"), 64 * 1024 * 1024 * 1024) {
				command.env(k, v);
			}
			command.env("RNX_BOOTSTRAP_FIXTURE", "1");
		}
		if with_memory
			&& (env.iter().any(|(k, _)| *k == "RNX_BOOTSTRAP_FIXTURE")
				|| !env.iter().any(|(k, _)| *k == "RNX_BOOTSTRAP_CGROUP_ROOT"))
		{
			command.env("RNX_BOOTSTRAP_DISK_BYTES", "34359738368");
		}
		for (k, v) in env {
			command.env(k, v);
		}
		command.output().unwrap()
	}
}

impl Drop for Checkout {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

const ECHO: &str = "fn main() { let a: Vec<String> = std::env::args().skip(1).collect(); std::fs::write(std::env::current_dir().unwrap().join(\"handed\"), a.join(\"\\n\")).unwrap(); }\n";

#[test]
fn the_entry_builds_and_hands_over_a_bound_record() {
	let checkout = Checkout::new(ECHO);
	let output = checkout.entry(&[]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let handed = std::fs::read_to_string(checkout.0.join("handed")).unwrap();
	let args: Vec<&str> = handed.lines().collect();
	assert_eq!(
		(args[0], &args[2..]),
		("--bootstrap", &["--out", "OUT"][..])
	);
	let record: Value = serde_json::from_slice(&std::fs::read(args[1]).unwrap()).unwrap();
	assert_eq!(record["candidate"], checkout.git(&["rev-parse", "HEAD"]));
	assert_eq!(
		record["cargo_lock_sha256"],
		json!(digest(&checkout.0.join("Cargo.lock")).unwrap())
	);
	assert_eq!(
		record["artifact_sha256"],
		json!(digest(&checkout.0.join("target/final-controller/release/rnx-final")).unwrap())
	);
	// One job; and the handoff says it was admitted from fixture memory,
	// which the controller refuses (the_controller_checks_every_part...).
	assert_eq!(
		(record["jobs"].clone(), record["fixture"].clone()),
		(json!(1), json!(true))
	);
	assert!(record["started"].as_f64().unwrap() <= record["built"].as_f64().unwrap());
}

#[test]
fn bootstrap_preserves_foreign_and_leased_targets() {
	let checkout = Checkout::new(ECHO);
	let target = checkout.0.join("target/final-controller");
	std::fs::create_dir_all(&target).unwrap();
	std::fs::write(target.join("foreign"), b"keep").unwrap();
	let output = checkout.entry(&[]);
	assert!(!output.status.success());
	assert!(String::from_utf8_lossy(&output.stderr).contains("unowned bootstrap target"));
	assert_eq!(std::fs::read(target.join("foreign")).unwrap(), b"keep");
	assert!(!target.join("release").exists());

	let checkout = Checkout::new(ECHO);
	assert!(checkout.entry(&[]).status.success());
	std::fs::remove_file(checkout.0.join("handed")).unwrap();
	assert!(checkout.git(&["status", "--porcelain"]).is_empty());
	let target = checkout.0.join("target/final-controller");
	let lease = std::fs::File::open(&target).unwrap();
	lease.try_lock().unwrap();
	let output = checkout.entry(&[]);
	assert!(!output.status.success());
	assert!(String::from_utf8_lossy(&output.stderr).contains("bootstrap target in use"));
	assert!(!checkout.0.join("handed").exists());
	drop(lease);
	assert!(checkout.entry(&[]).status.success());
}

#[test]
fn a_controller_that_does_not_build_starts_nothing() {
	let checkout = Checkout::new("fn main() { this does not compile }\n");
	let output = checkout.entry(&[]);
	assert!(!output.status.success());
	assert!(String::from_utf8_lossy(&output.stderr).contains("the controller did not build"));
	assert!(
		!checkout.0.join("target/final-controller/handoff").exists(),
		"a record was written"
	);
	assert!(!checkout.0.join("OUT").exists());
}

#[test]
fn a_dirty_checkout_is_refused_before_building() {
	let checkout = Checkout::new(ECHO);
	std::fs::write(checkout.0.join("untracked.rs"), "").unwrap();
	let output = checkout.entry(&[]);
	assert!(String::from_utf8_lossy(&output.stderr).contains("not clean"));
	assert!(
		!checkout.0.join("target/final-controller").exists(),
		"it built anyway"
	);
}

fn fixture_memory(dir: &Path, headroom: u64) -> Vec<(String, String)> {
	let group = dir.join("cgroup/final.service");
	std::fs::create_dir_all(&group).unwrap();
	std::fs::write(group.join("memory.current"), "0").unwrap();
	std::fs::write(group.join("memory.high"), headroom.to_string()).unwrap();
	std::fs::write(group.join("memory.max"), "max").unwrap();
	std::fs::write(dir.join("meminfo"), "MemAvailable: 104857600 kB\n").unwrap();
	std::fs::write(dir.join("membership"), "0::/final.service\n").unwrap();
	vec![
		(
			"RNX_BOOTSTRAP_CGROUP_ROOT".into(),
			dir.join("cgroup").to_string_lossy().into_owned(),
		),
		(
			"RNX_BOOTSTRAP_MEMINFO".into(),
			dir.join("meminfo").to_string_lossy().into_owned(),
		),
		(
			"RNX_BOOTSTRAP_MEMBERSHIP".into(),
			dir.join("membership").to_string_lossy().into_owned(),
		),
	]
}

#[test]
fn too_little_memory_for_one_build_job_is_refused_before_building() {
	let checkout = Checkout::new(ECHO);
	let memory = fixture_memory(&checkout.0.join("target/fixture"), 4 * 1024 * 1024 * 1024);
	let mut env: Vec<(&str, &str)> = memory
		.iter()
		.map(|(k, v)| (k.as_str(), v.as_str()))
		.collect();
	// Without the fixture flag the sources themselves are refused.
	let output = checkout.entry(&env);
	assert!(String::from_utf8_lossy(&output.stderr).contains("fixture memory sources are refused"));
	env.push(("RNX_BOOTSTRAP_FIXTURE", "1"));
	let output = checkout.entry(&env);
	assert!(
		String::from_utf8_lossy(&output.stderr).contains("needed for one build job"),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(
		!checkout.0.join("target/final-controller").exists(),
		"it built anyway"
	);
}

#[test]
fn the_entry_and_the_controller_share_their_constants() {
	let script = std::fs::read_to_string(SCRIPT).unwrap();
	let constant = |name: &str| -> u64 {
		script
			.lines()
			.find_map(|l| l.strip_prefix(&format!("{name}=")))
			.and_then(|v| v.split_whitespace().next())
			.unwrap()
			.parse()
			.unwrap()
	};
	assert_eq!(
		constant("FLOOR_BYTES"),
		rnx_final::controller::DISK_FLOOR_BYTES
	);
	assert_eq!(constant("COLD_BYTES"), rnx_final::scheduler::COLD_BYTES);
	assert_eq!(constant("RESERVE_BYTES"), job_budget::memory::RESERVE_BYTES);
	assert_eq!(
		constant("JOB_MEMORY_BYTES"),
		job_budget::memory::JOB_MEMORY_BYTES
	);
}

/// A record for this test binary, which plays the controller.
fn record(checkout: &Checkout, started: f64, built: f64) -> Value {
	let toolchain = std::process::Command::new("rustc")
		.args(["+1.98.1", "-vV"])
		.output()
		.unwrap()
		.stdout;
	json!({
		"schema": 1, "started": started, "built": built, "candidate": checkout.git(&["rev-parse", "HEAD"]), "clean": true,
		"cargo_lock_sha256": digest(&checkout.0.join("Cargo.lock")).unwrap(),
		"toolchain_sha256": rnx_final::pyjson::hex(&<sha2::Sha256 as sha2::Digest>::digest(&toolchain)),
		"jobs": 1, "artifact_sha256": digest(Path::new("/proc/self/exe")).unwrap(), "fixture": false,
	})
}

fn hand(checkout: &Checkout, value: &Value) -> PathBuf {
	let path = checkout.0.join("target/handoff.json");
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(&path, value.to_string()).unwrap();
	path
}

#[test]
fn the_controller_checks_every_part_of_the_handoff() {
	let checkout = Checkout::new(ECHO);
	let now = boottime();
	let good = record(&checkout, now - 10.0, now - 5.0);
	let path = hand(&checkout, &good);
	accept(&path, &checkout.0).unwrap();
	assert!(
		accept(&path, &checkout.0)
			.unwrap_err()
			.to_string()
			.contains("already used"),
		"a record is single use"
	);
	let mutate = |key: &str, value: Value| {
		let mut bad = good.clone();
		bad[key] = value;
		bad
	};
	for (label, bad) in [
		("out of order", mutate("built", json!(now - 20.0))),
		("future", mutate("built", json!(now + 1000.0))),
		("non-finite", mutate("started", json!("nan"))),
		("digest", mutate("artifact_sha256", json!("0".repeat(64)))),
		("candidate", mutate("candidate", json!("0".repeat(40)))),
		("lock", mutate("cargo_lock_sha256", json!("0".repeat(64)))),
		(
			"toolchain",
			mutate("toolchain_sha256", json!("0".repeat(64))),
		),
		("fixture", mutate("fixture", json!(true))),
		("schema", mutate("schema", json!(2))),
	] {
		let path = hand(&checkout, &bad);
		assert!(accept(&path, &checkout.0).is_err(), "{label} was accepted");
		assert!(!path.exists(), "{label}: the record outlived its one use");
	}
	// A source change after the entry's capture.
	std::fs::write(checkout.0.join("tools/final/src/main.rs"), "fn main() {}\n").unwrap();
	let path = hand(&checkout, &good);
	assert!(
		accept(&path, &checkout.0)
			.unwrap_err()
			.to_string()
			.contains("not clean")
	);
}

#[test]
fn the_build_counts_toward_the_wall_budget() {
	let checkout = Checkout::new(ECHO);
	let now = boottime();
	// The entry started 61 minutes ago; the build took most of it.
	let path = hand(&checkout, &record(&checkout, now - 61.0 * 60.0, now - 1.0));
	let (_, began) = accept(&path, &checkout.0).unwrap();
	let seconds = began.elapsed().as_secs_f64();
	assert!(
		rnx_final::controller::wall_budget_error("complete", seconds)
			.unwrap()
			.contains("took 61"),
		"{seconds}"
	);
}

/// `git status` fails (an unreadable index): its empty output is not a
/// clean tree, for the entry and for the handoff check alike.
#[test]
fn a_failing_git_status_is_not_a_clean_tree() {
	use std::os::unix::fs::PermissionsExt as _;
	let checkout = Checkout::new(ECHO);
	let now = boottime();
	let good = record(&checkout, now - 10.0, now - 5.0);
	let index = checkout.0.join(".git/index");
	std::fs::set_permissions(&index, std::fs::Permissions::from_mode(0o000)).unwrap();
	// Root reads through any mode; then git status succeeds and the control means nothing.
	let readable = std::fs::read(&index).is_ok();
	if !readable {
		let output = checkout.entry(&[]);
		assert!(!output.status.success());
		assert!(
			String::from_utf8_lossy(&output.stderr).contains("git status failed"),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		assert!(
			!checkout.0.join("target/final-controller").exists(),
			"it built anyway"
		);
		let path = hand(&checkout, &good);
		assert!(
			accept(&path, &checkout.0)
				.unwrap_err()
				.to_string()
				.contains("git status failed")
		);
	}
	std::fs::set_permissions(&index, std::fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn disk_only_fixture_is_guarded_and_taints_the_handoff() {
	use std::os::unix::fs::PermissionsExt;
	let checkout = Checkout::new(ECHO);
	let required = 24_u64 << 30;
	let short = (required - 1).to_string();
	let output = checkout.launch(&[("RNX_BOOTSTRAP_DISK_BYTES", &short)], false);
	assert!(String::from_utf8_lossy(&output.stderr).contains("fixture disk bytes are refused"));
	let output = checkout.launch(
		&[
			("RNX_BOOTSTRAP_DISK_BYTES", &short),
			("RNX_BOOTSTRAP_FIXTURE", "1"),
		],
		false,
	);
	let refusal = String::from_utf8_lossy(&output.stderr);
	assert!(
		refusal.contains(checkout.0.join("target/final-controller").to_str().unwrap())
			&& refusal.contains("filesystem ")
			&& refusal.contains(&format!("{short} bytes free, {required} needed")),
		"{refusal}"
	);
	assert!(!checkout.0.join("target/final-controller").exists());

	// Supply deterministic OS readings at the command boundary so this
	// disk-only control also works inside a memory-constrained final.
	let bin = checkout.0.join("target/os");
	std::fs::create_dir_all(&bin).unwrap();
	for (name, source) in [
		(
			"awk",
			"#!/bin/sh\ncase \"$2\" in /proc/meminfo) echo 104857600;; *) exec /usr/bin/awk \"$@\";; esac\n",
		),
		(
			"sed",
			"#!/bin/sh\ncase \"$3\" in /proc/self/cgroup) echo /;; *) exec /usr/bin/sed \"$@\";; esac\n",
		),
	] {
		let path = bin.join(name);
		std::fs::write(&path, source).unwrap();
		std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
	}
	let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
	let output = checkout.launch(
		&[
			("RNX_BOOTSTRAP_FIXTURE", "1"),
			("RNX_BOOTSTRAP_DISK_BYTES", &required.to_string()),
			("PATH", &path),
		],
		false,
	);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let handed = std::fs::read_to_string(checkout.0.join("handed")).unwrap();
	let path = Path::new(handed.lines().nth(1).unwrap());
	assert!(
		accept(path, &checkout.0)
			.unwrap_err()
			.to_string()
			.contains("fixture sources")
	);
}
