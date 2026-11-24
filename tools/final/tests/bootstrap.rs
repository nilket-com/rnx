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

#[test]
fn checkpoint_sources_switch_after_the_baseline_build_and_a_new_recipe_starts_cold() {
	let checkout = Checkout::new(ECHO);
	let root = &checkout.0;
	let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint/state.sh");
	let output = std::process::Command::new("bash")
		.args(["-ceu", r#"
		set -o pipefail
		source "$1"
		cd "$2"
		baseline=$(git rev-parse HEAD)
		candidate_src='fn main() { println!("candidate"); }'
		printf '%s\n' "$candidate_src" >tools/final/src/main.rs
		git add tools/final/src/main.rs
		git -c user.name=test -c user.email=test@invalid -c commit.gpgsign=false commit -qm candidate
		candidate=$(git rev-parse HEAD)
		checkout_revision "$PWD" "$baseline"
		cargo +1.98.1 build --offline --locked
		start=$(uptime_seconds)
		checkout_revision "$PWD" "$candidate"
		cargo +1.98.1 build --offline --locked --message-format=json >target/candidate.jsonl
		end=$(uptime_seconds)
		jq -es 'any(.[]; .reason=="compiler-artifact" and .fresh==false)' target/candidate.jsonl
		test "$(target/debug/rnx-final)" = candidate
		test ! -e target/new-recipe
		cargo +1.98.1 build --offline --locked --target-dir target/new-recipe --message-format=json >target/new.jsonl
		jq -es 'any(.[]; .reason=="compiler-artifact" and .fresh==false)' target/new.jsonl
		awk -v start="$start" -v end="$end" 'BEGIN{exit !(end>start)}'
		"#, "checkpoint-control"])
		.arg(script).arg(root)
		.env_remove("CARGO_TARGET_DIR").env_remove("CARGO_BUILD_BUILD_DIR")
		.output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn a_dirty_checkpoint_or_foreign_payload_cannot_promote_and_a_time_only_verdict_sticks() {
	let checkout = Checkout::new(ECHO);
	let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint/state.sh");
	let output = std::process::Command::new("bash")
		.args(["-ceu", r#"
		set -o pipefail
		source "$1"
		cd "$2"
		state="$PWD/target/state"
		generation="$state/generation"
		root="$generation/source"
		mkdir -p "$state"
		baseline=$(git rev-parse HEAD)
		candidate=$baseline
		choose_generation; test "$mode" = cold
		claim_directory "$generation"
		mkdir -p "$root/target"/{final-caches,final-controller/release,fixed-route-anchor-cache-v2} "$generation/cargo"
		printf payload >"$root/target/final-controller/release/rnx-final"
		last="$PWD/target/passed"
		mkdir "$last"
		snapshot() { checkpoint_identity "$generation"; }
		seal=$(snapshot)
		binary=$(sha256sum <"$root/target/final-controller/release/rnx-final" | cut -d' ' -f1)
		jq -n --arg sha "$candidate" '{candidate:$sha,status:"PASS",complete_final:true,publication_eligible:true}' >"$last/RESULT.json"
		jq -n --arg seal "$seal" '{terminal_state_sha256:$seal}' >"$last/CHECKPOINTRELEASE.json"
		jq -n --arg sha "$binary" '{tools:{controller:{binary_sha256:$sha}}}' >"$last/MANIFEST.json"
		result=$(sha256sum <"$last/RESULT.json" | cut -d' ' -f1)
		jq -n --arg result "$result" --arg seal "$seal" --arg out "$last" --arg sha "$candidate" --arg root "$root" \
			'{result:$result,seal:$seal,out:$out,candidate:$sha,checkout:$root}' >"$state/previous.json"
		choose_generation; test "$mode" = promoted
		(root="$root/other-checkout"; choose_generation; test "$mode" = restored)
		printf running >"$state/in-use"
		choose_generation; test "$mode" = restored
		# A crash can happen after a cache transaction and before a verdict.
		printf '{}' >"$last/CACHE.json"
		choose_generation; test "$mode" = restored
		unlink "$state/in-use"
		printf foreign >"$root/target/fixed-route-anchor-cache-v2/entry"
		choose_generation; test "$mode" = restored
		jq -n --arg out "$last" --arg candidate "$candidate" '{out:$out,candidate:$candidate}' >"$state/attempt.json"
		jq -n --arg candidate "$candidate" '{candidate:$candidate,status:"FAIL",wall_budget_error:"over",error:"over",checks:[{exit:0}]}' >"$last/RESULT.json"
		if (guard_overrun); then exit 1; fi
		test -e "$state/overruns/$candidate"
		unlink "$state/attempt.json"
		if (guard_overrun); then exit 1; fi
		"#, "checkpoint-control"])
		.arg(script).arg(&checkout.0).output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn checkpoint_replay_binds_locked_fetches_and_all_clock_intervals() {
	use rnx_final::checkpoint::{Admission, revalidate};
	let checkout = Checkout::new(ECHO);
	let proof = checkout.0.join("preparation");
	std::fs::create_dir(&proof).unwrap();
	std::fs::write(proof.join("recipes.jsonl"), "{}\n").unwrap();
	std::fs::write(proof.join("fetch.json"), "[]\n").unwrap();
	for file in ["units.jsonl", "locked-sources.txt"] {
		std::fs::write(proof.join(file), "fixture").unwrap();
	}
	std::fs::write(
		proof.join("anchor-result.json"),
		r#"{"status":"PASS","anchor_cache":{"key":"witness"}}"#,
	)
	.unwrap();
	std::fs::write(proof.join("classes.json"), serde_json::to_vec(&json!({
		"baseline":"a".repeat(40), "anchor":"witness", "lane_units":digest(&proof.join("units.jsonl")).unwrap(),
		"dependency_home":digest(&proof.join("locked-sources.txt")).unwrap()
	})).unwrap()).unwrap();
	let state = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint/state.sh");
	let sha = std::process::Command::new("bash")
		.args([
			"-ceu",
			"set -o pipefail; source \"$1\"; recipe_digest \"$2\"",
			"test",
		])
		.arg(state)
		.arg(&proof)
		.output()
		.unwrap();
	assert!(sha.status.success());
	let candidate = checkout.git(&["rev-parse", "HEAD"]);
	let admission = Admission {
		baseline: "a".repeat(40),
		candidate: candidate.clone(),
		boot_id: "00000000-0000-0000-0000-000000000001".into(),
		preparation: "restored".into(),
		recipe_sha256: String::from_utf8(sha.stdout).unwrap().trim().into(),
		toolchains_sha256: "1".repeat(64),
		prepared_state_sha256: digest(&proof.join("classes.json")).unwrap(),
		developer_state_sha256: "3".repeat(64),
		requested: 10.0,
		acquired: 13.0,
		admitted: 29.0,
	};
	std::fs::write(
		proof.join("admission.json"),
		serde_json::to_vec(&admission).unwrap(),
	)
	.unwrap();
	let manifest =
		json!({"tools":{"controller":{"bootstrap":{"started":29.0, "checkpoint":admission}}}});
	let mut timing = admission.timing(11.0);
	assert_eq!(
		(
			timing.lease_wait_seconds,
			timing.preparation_seconds,
			timing.timed_seconds,
			timing.total_seconds
		),
		(3.0, 16.0, 11.0, 30.0)
	);
	revalidate(&checkout.0, &manifest, &timing, &candidate, 11.0).unwrap();
	timing.total_seconds -= timing.lease_wait_seconds;
	assert!(
		revalidate(&checkout.0, &manifest, &timing, &candidate, 11.0)
			.unwrap_err()
			.to_string()
			.contains("durations")
	);
	std::fs::write(proof.join("fetch.json"), "[[]]\n").unwrap();
	assert!(
		revalidate(
			&checkout.0,
			&manifest,
			&admission.timing(11.0),
			&candidate,
			11.0
		)
		.unwrap_err()
		.to_string()
		.contains("recipes")
	);
}

#[test]
fn resetting_a_private_checkout_preserves_recovery_and_drops_unproven_products() {
	let checkout = Checkout::new(ECHO);
	let state = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint/state.sh");
	let output = std::process::Command::new("bash")
		.args(["-ceu", r#"
		set -o pipefail
		source "$1"
		cd "$2"
		mkdir -p target
		gen="$PWD/target/gen"
		claim_directory "$gen"
		git clone --quiet --local "$PWD" "$gen/source"
		cd "$gen/source"
		mkdir -p target/{final-owners,rnx-verify,routing-cli,final-caches,final-controller,project-image} tools/final/target
		for p in target/* tools/final/target; do touch "$p/sentinel"; done
		reset_build_outputs "$gen" invocation
		test ! -e tools/final/target
		test ! -e target/project-image
		test -f target/final-caches/sentinel
		test -f target/final-controller/sentinel
		claim_directory "$gen/cargo"
		touch "$gen/cargo/obsolete-dependency"
		prepare_generation "$gen" promoted
		test ! -e "$gen/cargo"
		test ! -e target/final-controller
		test -f target/final-caches/sentinel
		reset_build_outputs "$gen" all
		test ! -e target/final-caches && test ! -e target/final-controller
		for p in target/{final-owners,rnx-verify,routing-cli}; do test -f "$p/sentinel"; done
		"#, "reset-control"])
		.arg(state).arg(&checkout.0).output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn checkpoint_parent_refuses_inherited_cargo_configuration_and_dangling_links() {
	let scratch = std::env::temp_dir().join(format!("rnx-checkpoint-parent-{}", invocation_id()));
	std::fs::create_dir(&scratch).unwrap();
	let scratch = Checkout(scratch.canonicalize().unwrap());
	let state = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint/state.sh");
	let output = std::process::Command::new("bash")
		.args([
			"-ceu",
			r#"
		source "$1"
		mkdir -m 0700 "$2/child"
		if (export CARGO_BUILD_RUSTC_WRAPPER="$2/unleased"; check_build_environment) 2>"$2/error"; then exit 1; fi
		grep -Fxq 'final refuses CARGO_BUILD_RUSTC_WRAPPER override' "$2/error"
		no_cargo_config "$2/child"
		mkdir "$2/.cargo"
		echo '[alias]' >"$2/.cargo/config.toml"
		if validate_store_root "$2/child" "$(id -u)" 2>"$2/error"; then exit 1; fi
		grep -Fxq "checkpoint would inherit $2/.cargo/config.toml" "$2/error"
		rm "$2/.cargo/config.toml"
		no_cargo_config "$2/child"
		ln -s missing "$2/.cargo/config"
		if no_cargo_config "$2/child" 2>"$2/error"; then exit 1; fi
		grep -Fxq "checkpoint would inherit $2/.cargo/config" "$2/error"
		rm "$2/.cargo/config"
		chmod 0770 "$2"
		if validate_store_root "$2/child" "$(id -u)" 2>"$2/error"; then exit 1; fi
		grep -Fq "unsafe checkpoint ancestor: $2 (" "$2/error"
		"#,
			"parent-control",
		])
		.arg(state)
		.arg(&scratch.0)
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn settle_ignores_other_applications_and_names_changes_to_each_developer_cache() {
	let checkout = Checkout::new(ECHO);
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let output = std::process::Command::new("bash")
		.current_dir(root)
		.args([
			"-ceu",
			r#"
		set -o pipefail
		source tools/final/checkpoint/state.sh
		account="$1/account"; login="$1/login"
		export XDG_CACHE_HOME="$1/xdg" CARGO_HOME="$1/cargo" RNX_PROJECT_CACHE="$1/project"
		roots=("$account/.cargo" "$login/.cargo" "$CARGO_HOME" "$RNX_PROJECT_CACHE")
		for base in "$account/.cache" "$login/.cache" "$XDG_CACHE_HOME"; do
			roots+=("$base/rnx" "$base/rnx-file-images")
		done
		mkdir -p -- "${roots[@]}"
		store="$1/store"; mkdir "$store"
		export RNX_FINAL_CHECKPOINT_STORE="$store"
		claim_directory "$store/generation"
		git init -q "$store/generation/source"
		run="$store/invocation"; claim_directory "$run"
		developer_cache_paths "$account" "$login" >"$run/home-paths"
		record_before() {
			developer_cache_manifest "$run/home-paths" >"$run/home-before"
			jq -n --arg sha "$(sha256sum <"$run/home-before" | cut -d' ' -f1)" \
				'{developer_state_sha256:$sha}' >"$1"
		}
		admission="$1/admission.json"
		for protected in "${roots[@]}"; do
			record_before "$admission"
			file="$protected/"$'new\tentry\ncontinued'
			touch -- "$file"
			if bash tools/final/checkpoint/settle.sh "$run" PASS "$admission" 2>"$1/error"; then exit 1; fi
			name=$(printf '%q' "$file")
			grep -Fxq -- "$name" "$1/error"
			grep -Fxq -- "$name" "$1/developer-cache-changes.txt"
		done
		record_before "$admission"
		mkdir -p "$account/.cache/tracker3/files"
		touch "$account/.cache/tracker3/files/meta.db-wal"
		bash tools/final/checkpoint/settle.sh "$run" PASS "$admission"
		test ! -e "$run"
		"#,
			"settle-control",
		])
		.arg(&checkout.0)
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn a_failed_timed_bootstrap_keeps_the_admission_intervals() {
	let checkout = Checkout::new(ECHO);
	let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint/state.sh");
	let output = std::process::Command::new("bash").args(["-ceu", r#"
		set -o pipefail
		source "$1"
		store="$2/target/state"; out="$2/target/out"
		mkdir -p "$store" "$out/preparation"
		export RNX_FINAL_CHECKPOINT_STORE="$store"
		jq -n --arg out "$out" '{out:$out}' >"$store/attempt.json"
		echo '{"candidate":"c","baseline":"b","requested":1,"acquired":2,"admitted":4}' >"$out/preparation/admission.json"
		exec 7<"$store"
		flock -x 7
		timed_refusal "$out"
		jq -e '.status=="FAIL" and .phase=="candidate" and .lease_wait_seconds==1 and .preparation_seconds==2 and .timed_seconds==(.finished-4) and .total_seconds==(.finished-1)' "$out/PREPARATION-FAIL.json"
		"#, "refusal-control"]).arg(script).arg(&checkout.0).output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
}
#[test]
fn disk_shortfalls_on_either_side_of_preparation_never_start_the_clock() {
	let checkout = Checkout::new(ECHO);
	let mechanism = Path::new(env!("CARGO_MANIFEST_DIR")).join("checkpoint");
	let script = r#"
		set -euo pipefail
		shopt -s inherit_errexit
		mechanism=$1; root=$2
		source "$mechanism/state.sh"
		source "$mechanism/disk.sh"
		out="$root/$3"; proof="$out/preparation"; lanes="$root/lanes"
		mkdir -p "$proof" "$lanes"
		trap refusal EXIT
		admitted=null; requested=1; acquired=2; run=; outcome=refused; candidate=test
		baseline=ee52b405e9cea581c514ec87463faaa34d803b13; mode=cold
		disk_tokens() { echo 13; }
		disk_scratch() { echo "$1"; }
		# The signed cold run's small disk and the first post-preparation
		# refusal are independent observations on the same fixture device.
		stop=$3
		disk_free() {
			if [[ $outcome != "$stop" ]]; then echo 999999999999
			elif [[ $stop == before-preparation ]]; then echo 229807611904
			else echo 117976432640; fi
		}
		prepare_checkpoint() { touch "$out/built"; }
		prepare_and_admit
		touch "$out/timed"
	"#;
	for stop in ["before-preparation", "before-clock", "sufficient"] {
		let result = std::process::Command::new("bash")
			.args(["-c", script, "disk-control"])
			.arg(&mechanism)
			.arg(&checkout.0)
			.arg(stop)
			.output()
			.unwrap();
		let out = checkout.0.join(stop);
		assert_eq!(
			result.status.success(),
			stop == "sufficient",
			"{}",
			String::from_utf8_lossy(&result.stderr)
		);
		assert_eq!(out.join("built").exists(), stop != "before-preparation");
		assert_eq!(out.join("timed").exists(), stop == "sufficient");
		if stop == "sufficient" {
			continue;
		}
		let failure: Value =
			serde_json::from_slice(&std::fs::read(out.join("PREPARATION-FAIL.json")).unwrap())
				.unwrap();
		assert_eq!(failure["phase"], stop);
		assert!(failure["admitted"].is_null());
		assert_eq!(failure["timed_seconds"], 0);
		assert!(!out.join("RESULT.json").exists());
		let record: Value = serde_json::from_slice(
			&std::fs::read(out.join(format!("preparation/{stop}.json"))).unwrap(),
		)
		.unwrap();
		let refusal = &failure["disk"];
		let free = refusal["free_bytes"].as_u64().unwrap();
		assert!(refusal["needed_bytes"].as_u64().unwrap() > free);
		assert_eq!(
			free,
			if stop == "before-preparation" {
				229807611904
			} else {
				117976432640
			}
		);
		if stop == "before-preparation" {
			assert!(
				record["preparation_growth_bytes"].as_u64().unwrap() >= 112738422784 - 271187968
			);
		}
	}
}

#[test]
fn the_bootstrap_declaration_matches_the_complete_controller_catalogue() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let output = std::process::Command::new(env!("CARGO_BIN_EXE_rnx-final"))
		.arg("checkpoint-disk-plan")
		.env_remove("CARGO_BUILD_BUILD_DIR")
		.env_remove("CARGO_TARGET_DIR")
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let checked = std::fs::read(root.join("tools/final/checkpoint/disk-plan.json")).unwrap();
	assert_eq!(
		serde_json::from_slice::<Value>(&output.stdout).unwrap(),
		serde_json::from_slice::<Value>(&checked).unwrap()
	);
}
