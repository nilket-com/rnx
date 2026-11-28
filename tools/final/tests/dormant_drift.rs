//! ADR 0355 controls: reviewed pinned dormant proof (`tools/column/dormant/NNNN.json`
//! pinned in a signed parent plan and committed in a signed impl), structural AST
//! checker veto (module-qualified enum identity, exact `cfg(test)` module edges,
//! expanding attribute rejection, shadowing, multi-root, strict derive allowlist,
//! `--no-renames -z`), reused baseline succession (`controller::docs_change`),
//! full-rescore capture-free revalidation, checker source provenance, and stage-receipt
//! binding in top-level revalidation and docs-only succession.
use rnx_final::fixed_route::{self as fr, drift};
use rnx_final::pyjson::{hex, identity};
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

struct TempDir(PathBuf);
impl TempDir {
	fn new(prefix: &str) -> Self {
		let p = std::env::temp_dir().join(format!("{prefix}-{}", invocation_id()));
		std::fs::create_dir_all(&p).unwrap();
		Self(p.canonicalize().unwrap())
	}
}
impl Drop for TempDir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn build_execute_guard() -> PathBuf {
	use std::sync::OnceLock;
	static GUARD: OnceLock<PathBuf> = OnceLock::new();
	GUARD
		.get_or_init(|| {
			let root = Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../..")
				.canonicalize()
				.unwrap();
			let manifest = root.join("ci/execute_guard/Cargo.toml");
			let status = Command::new("cargo")
				.args(["build", "--locked", "--offline", "--manifest-path"])
				.arg(&manifest)
				.status()
				.unwrap();
			assert!(status.success(), "building execute-guard failed");
			root.join("ci/execute_guard/target/debug/rnx-execute-drift-guard")
		})
		.clone()
}

fn minimal_policy() -> Value {
	let mut policy: Value =
		serde_json::from_str(include_str!("../../column/fixed_route_policy.json")).unwrap();
	policy["schema"] = json!(1);
	let obj = policy.as_object_mut().unwrap();
	obj.remove("meter");
	obj.remove("native_setup");
	obj.remove("installed_setup");
	obj.remove("relative_relocations");
	policy
}

fn make_capture(
	revision: &str,
	policy: &Value,
	binary_sha: &str,
	sieve_off_delta: (i64, i64),
	loops_off_delta: (i64, i64),
) -> Value {
	let policy_sha = identity(policy).unwrap();
	let recipe = json!({"policy": {"schema": 1}, "toolchain": "1.98.1"});
	let mut records = Vec::new();
	for k in policy["kernels"].as_array().unwrap() {
		let kind = k.as_str().unwrap();
		for m in policy["modes"].as_array().unwrap() {
			let mode = m.as_str().unwrap();
			for n in policy["sizes"].as_array().unwrap() {
				let size = n.as_i64().unwrap();
				let (base_ir, delta) = match (kind, mode, size) {
					("awfy_sieve", "off", 1) => (14_246_686_i64, sieve_off_delta.0),
					("awfy_sieve", "off", 5) => (71_233_429_i64, sieve_off_delta.1),
					("loops", "off", 1) => (6_523_505_i64, loops_off_delta.0),
					("loops", "off", 5) => (32_617_529_i64, loops_off_delta.1),
					(_, _, 1) => (10_000_000_i64, 0),
					(_, _, 5) => (50_000_000_i64, 0),
					_ => (10_000_000_i64, 0),
				};
				let rnx_ir = base_ir + delta;
				records.push(json!({
					"kind": kind,
					"policy": mode,
					"n": size,
					"status": "PASS",
					"stdout_sha256": format!("{kind}-{mode}-{size}-out"),
					"fixture_sha256": format!("{kind}-fix"),
					"accounting": {
						"rnx_Ir": rnx_ir,
						"other_Ir": 0,
						"dynamic_Ir": 0,
						"total_Ir": rnx_ir,
					}
				}));
			}
		}
	}
	json!({
		"status": "PASS",
		"revision": revision,
		"policy_sha256": policy_sha,
		"binary_sha256": binary_sha,
		"capture_key": format!("key-{revision}-{binary_sha}"),
		"recipe": recipe,
		"capture_paths": {
			"measured": {
				"version": 1,
				"runner": {"path": "/run/rnx", "sha256": binary_sha},
			}
		},
		"records": records,
	})
}

/// Ephemeral SSH signing key + `allowed_signers` for self-contained signed
/// git fixture repositories (never touches user SSH keys).
struct TestSigningKey {
	_dir: TempDir,
	pub_key: PathBuf,
	allowed_signers: PathBuf,
}

fn test_signing_key() -> &'static TestSigningKey {
	use std::sync::OnceLock;
	static KEY: OnceLock<TestSigningKey> = OnceLock::new();
	KEY.get_or_init(|| {
		let dir = TempDir::new("rnx-0355-sshkey");
		let priv_key = dir.0.join("id_ed25519");
		let status = Command::new("ssh-keygen")
			.args(["-q", "-t", "ed25519", "-N", "", "-C", "t@t", "-f"])
			.arg(&priv_key)
			.status()
			.unwrap();
		assert!(status.success(), "ssh-keygen failed");
		let pub_key = dir.0.join("id_ed25519.pub");
		let pub_text = std::fs::read_to_string(&pub_key).unwrap();
		let allowed_signers = dir.0.join("allowed_signers");
		std::fs::write(&allowed_signers, format!("t@t {}\n", pub_text.trim())).unwrap();
		TestSigningKey {
			_dir: dir,
			pub_key,
			allowed_signers,
		}
	})
}

fn git_raw(dir: &Path, args: &[&str], sign: bool) -> String {
	let mut cmd = Command::new("git");
	cmd.arg("-C").arg(dir).args(args);
	if sign {
		// Clear any outer GIT_CONFIG_* env that forces commit.gpgsign=false.
		cmd.env_remove("GIT_CONFIG_COUNT")
			.env_remove("GIT_CONFIG_KEY_0")
			.env_remove("GIT_CONFIG_VALUE_0");
	} else {
		cmd.env("GIT_CONFIG_COUNT", "1")
			.env("GIT_CONFIG_KEY_0", "commit.gpgsign")
			.env("GIT_CONFIG_VALUE_0", "false");
	}
	let out = cmd.output().unwrap();
	assert!(
		out.status.success(),
		"git {args:?} failed: {}",
		String::from_utf8_lossy(&out.stderr)
	);
	String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn git_in(dir: &Path, args: &[&str]) -> String {
	git_raw(dir, args, false)
}

fn git_signed(dir: &Path, args: &[&str]) -> String {
	git_raw(dir, args, true)
}

fn configure_signed_repo(dir: &Path) {
	let key = test_signing_key();
	git_in(dir, &["config", "user.email", "t@t"]);
	git_in(dir, &["config", "user.name", "t"]);
	git_in(dir, &["config", "gpg.format", "ssh"]);
	git_in(
		dir,
		&["config", "user.signingkey", key.pub_key.to_str().unwrap()],
	);
	git_in(
		dir,
		&[
			"config",
			"gpg.ssh.allowedsignersfile",
			key.allowed_signers.to_str().unwrap(),
		],
	);
	git_in(dir, &["config", "commit.gpgsign", "true"]);
}

fn blob_id_of_bytes(dir: &Path, bytes: &[u8]) -> String {
	use std::io::Write;
	let mut child = Command::new("git")
		.arg("-C")
		.arg(dir)
		.args(["hash-object", "--stdin"])
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.as_mut().unwrap().write_all(bytes).unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(out.status.success());
	String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

struct SignedFixture {
	dir: TempDir,
	base_rev: String,
	pub_rev: String,
	plan_rev: String,
	cand_rev: String,
	proof_path: String,
	proof_sha256: String,
}

#[derive(Clone)]
struct FixtureOpts {
	record: &'static str,
	sign_plan: bool,
	sign_impl: bool,
	include_pin: bool,
	wrong_pin_digest: bool,
	write_proof_file: bool,
	pub_docs_amend: bool,
	added_variants: Vec<&'static str>,
	added_arms: Vec<(&'static str, &'static str)>,
	extra_context: Vec<&'static str>,
}

impl Default for FixtureOpts {
	fn default() -> Self {
		Self {
			record: "0356",
			sign_plan: true,
			sign_impl: true,
			include_pin: true,
			wrong_pin_digest: false,
			write_proof_file: true,
			pub_docs_amend: false,
			added_variants: vec!["heap::Payload::Bytes", "heap::Type::Bytes"],
			added_arms: vec![
				("src/heap.rs", "Self : : Bytes ( _ )"),
				("src/heap.rs", "| Payload : : Bytes ( _ )"),
			],
			extra_context: vec!["src/heap.rs"],
		}
	}
}

/// Build a self-contained signed git repo with:
/// `seed -> base_rev` (and optional same-parent `pub_rev`) `-> plan_rev -> cand_rev`,
/// with `tools/column/dormant/NNNN.json` pinned in `plan_rev` and committed in `cand_rev`.
fn init_signed_repo_with_proof(
	files_base: &[(&str, &str)],
	files_cand: &[(&str, &str)],
	opts: &FixtureOpts,
) -> SignedFixture {
	let dir = TempDir::new("rnx-0355-signed");
	git_in(&dir.0, &["init", "-q"]);
	configure_signed_repo(&dir.0);

	// Seed root commit with minimal ci/execute_guard so `base_rev` has a parent.
	std::fs::create_dir_all(dir.0.join("ci/execute_guard/src")).unwrap();
	std::fs::write(
		dir.0.join("ci/execute_guard/Cargo.toml"),
		"[package]\nname=\"g\"\n",
	)
	.unwrap();
	std::fs::write(dir.0.join("ci/execute_guard/src/main.rs"), "fn main() {}\n").unwrap();
	git_in(&dir.0, &["add", "-A"]);
	git_signed(&dir.0, &["commit", "-qm", "seed"]);

	for (rel, body) in files_base {
		let p = dir.0.join(rel);
		std::fs::create_dir_all(p.parent().unwrap()).unwrap();
		std::fs::write(&p, body).unwrap();
	}
	git_in(&dir.0, &["add", "-A"]);
	git_signed(&dir.0, &["commit", "-qm", "impl: adr 0355 baseline"]);
	let base_rev = git_in(&dir.0, &["rev-parse", "HEAD"]);

	let pub_rev = if opts.pub_docs_amend {
		std::fs::create_dir_all(dir.0.join("plans/impl")).unwrap();
		std::fs::write(
			dir.0.join("plans/impl/0355_results.md"),
			"Baseline results amend.\n",
		)
		.unwrap();
		git_in(&dir.0, &["add", "plans/impl/0355_results.md"]);
		git_signed(
			&dir.0,
			&["commit", "--amend", "-qm", "impl: adr 0355 baseline"],
		);
		git_in(&dir.0, &["rev-parse", "HEAD"])
	} else {
		base_rev.clone()
	};

	// Build delta rows and context blobs from base vs cand maps.
	let base_map: std::collections::BTreeMap<&str, &str> = files_base.iter().copied().collect();
	let cand_map: std::collections::BTreeMap<&str, &str> = files_cand.iter().copied().collect();
	let mut all_paths: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
	all_paths.extend(base_map.keys().copied());
	all_paths.extend(cand_map.keys().copied());

	let mut delta_rows = Vec::new();
	for path in &all_paths {
		if path.starts_with("plans/") {
			continue;
		}
		let old_b = base_map.get(path).copied();
		let new_b = cand_map.get(path).copied();
		if old_b == new_b {
			continue;
		}
		let (old_mode, old_blob) = match old_b {
			Some(s) => (
				json!("100644"),
				json!(blob_id_of_bytes(&dir.0, s.as_bytes())),
			),
			None => (Value::Null, Value::Null),
		};
		let (new_mode, new_blob) = match new_b {
			Some(s) => (
				json!("100644"),
				json!(blob_id_of_bytes(&dir.0, s.as_bytes())),
			),
			None => (Value::Null, Value::Null),
		};
		delta_rows.push(json!({
			"path": path,
			"old_mode": old_mode,
			"new_mode": new_mode,
			"old_blob": old_blob,
			"new_blob": new_blob,
		}));
	}

	let mut context_rows = Vec::new();
	for ctx_path in &opts.extra_context {
		let content = cand_map
			.get(ctx_path)
			.or_else(|| base_map.get(ctx_path))
			.expect("context path must exist in fixture");
		context_rows.push(json!({
			"path": ctx_path,
			"blob": blob_id_of_bytes(&dir.0, content.as_bytes()),
		}));
	}

	let proof_path = format!("tools/column/dormant/{}.json", opts.record);
	let mut proof = json!({
		"schema": 1,
		"record": opts.record,
		"kind": "representation_only",
		"baseline": base_rev,
		"delta": delta_rows,
		"context": context_rows,
		"added_variants": opts.added_variants,
		"added_arms": opts.added_arms.iter().map(|(f, p)| json!({"file": f, "pattern": p})).collect::<Vec<_>>(),
	});
	if opts.pub_docs_amend {
		proof["baseline_publication"] = json!(pub_rev);
	}
	let proof_bytes = serde_json::to_vec_pretty(&proof).unwrap();
	let proof_sha256 = hex(&Sha256::digest(&proof_bytes));

	// Plan parent commit.
	std::fs::create_dir_all(dir.0.join("plans/adr")).unwrap();
	std::fs::create_dir_all(dir.0.join("plans/impl")).unwrap();
	std::fs::write(
		dir.0.join(format!("plans/adr/{}_test.md", opts.record)),
		format!("# ADR {}\n\nStatus: Proposed\n", opts.record),
	)
	.unwrap();
	let mut impl_plan = format!(
		"# Implementation plan {}\n\nStatus: Proposed\n\n",
		opts.record
	);
	if opts.include_pin {
		let pin_digest = if opts.wrong_pin_digest {
			"0".repeat(64)
		} else {
			proof_sha256.clone()
		};
		impl_plan.push_str(&format!(
			"Dormant proof: `{proof_path}`\nDormant proof sha256: `{pin_digest}`\n"
		));
	}
	std::fs::write(
		dir.0.join(format!("plans/impl/{}_test.md", opts.record)),
		impl_plan,
	)
	.unwrap();
	git_in(&dir.0, &["add", "-A"]);
	let plan_msg = format!("plan: adr {} test dormant", opts.record);
	if opts.sign_plan {
		git_signed(&dir.0, &["commit", "-qm", &plan_msg]);
	} else {
		git_in(&dir.0, &["commit", "-qm", &plan_msg]);
	}
	let plan_rev = git_in(&dir.0, &["rev-parse", "HEAD"]);

	// Candidate impl commit.
	for (rel, body) in files_cand {
		let p = dir.0.join(rel);
		std::fs::create_dir_all(p.parent().unwrap()).unwrap();
		std::fs::write(&p, body).unwrap();
	}
	if opts.write_proof_file {
		let p = dir.0.join(&proof_path);
		std::fs::create_dir_all(p.parent().unwrap()).unwrap();
		std::fs::write(&p, &proof_bytes).unwrap();
	}
	git_in(&dir.0, &["add", "-A"]);
	let impl_msg = format!("impl: adr {} test dormant", opts.record);
	if opts.sign_impl {
		git_signed(&dir.0, &["commit", "-qm", &impl_msg]);
	} else {
		git_in(&dir.0, &["commit", "-qm", &impl_msg]);
	}
	let cand_rev = git_in(&dir.0, &["rev-parse", "HEAD"]);

	SignedFixture {
		dir,
		base_rev,
		pub_rev,
		plan_rev,
		cand_rev,
		proof_path,
		proof_sha256,
	}
}

const BASE_HEAP_RS: &str = r#"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type { String, Vec }
pub enum Payload { String(String), Vec(Vec<u8>) }
impl Payload {
	pub fn tag(&self) -> Type {
		match self {
			Self::String(_) => Type::String,
			Self::Vec(_) => Type::Vec,
		}
	}
}
pub fn children(p: &Payload) -> usize {
	match p {
		Payload::String(_) => 0,
		Payload::Vec(v) => v.len(),
	}
}
"#;

const DORMANT_BYTES_HEAP_RS: &str = r#"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type { String, Vec, Bytes }
pub enum Payload {
	String(String),
	Vec(Vec<u8>),
	#[cfg_attr(not(test), allow(dead_code))]
	Bytes(Vec<u8>),
}
impl Payload {
	pub fn tag(&self) -> Type {
		match self {
			Self::String(_) => Type::String,
			Self::Bytes(_) => Type::Bytes,
			Self::Vec(_) => Type::Vec,
		}
	}
}
pub fn children(p: &Payload) -> usize {
	match p {
		Payload::String(_) | Payload::Bytes(_) => 0,
		Payload::Vec(v) => v.len(),
	}
}
"#;

#[test]
fn control1_signed_accepting_path_reused_baseline_and_historical_bytes_fixture() {
	let guard = build_execute_guard();

	// 1a. Historical Bytes payload regression fixture (measured source `56db49ec`, unsigned,
	// no reviewed proof): raw 30/32 per-record and 32/32 cumulative, NOT eligible under live rule.
	let mut hist_policy = minimal_policy();
	let hist_base_rev = "f6ce4a3f00000000000000000000000000000000";
	let hist_cand_rev = "56db49ec00000000000000000000000000000000";
	hist_policy["anchor"] = json!(hist_base_rev);
	let hist_anchor = make_capture(hist_base_rev, &hist_policy, &"a".repeat(64), (0, 0), (0, 0));
	let hist_base = make_capture(hist_base_rev, &hist_policy, &"a".repeat(64), (0, 0), (0, 0));
	let hist_cand = make_capture(
		hist_cand_rev,
		&hist_policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(-50_010, -250_054),
	);
	let hist_raw = fr::score(&hist_base, &hist_cand, Some(&hist_anchor), &hist_policy).unwrap();
	assert_eq!(hist_raw["status"], "FAIL");
	assert_eq!(
		hist_raw["per_record"]
			.as_array()
			.unwrap()
			.iter()
			.filter(|r| r["pass_all"] == true)
			.count(),
		30
	);
	assert_eq!(
		hist_raw["cumulative"]
			.as_array()
			.unwrap()
			.iter()
			.filter(|r| r["pass_all"] == true)
			.count(),
		32
	);

	// 1b. Full accepting path on a small signed fixture repo with ephemeral test SSH key,
	// signed parent plan pinning `tools/column/dormant/0356.json`, and signed impl commit.
	let fx = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts::default(),
	);
	let mut policy = minimal_policy();
	policy["anchor"] = json!(fx.base_rev);
	let anchor = make_capture(&fx.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let baseline = make_capture(&fx.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let candidate = make_capture(
		&fx.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(-50_010, -250_054),
	);

	let without = drift::score_with_witness(
		&baseline,
		&candidate,
		Some(&anchor),
		&policy,
		None,
		&fx.dir.0,
		&guard,
	)
	.unwrap();
	assert_eq!(without["status"], "FAIL");
	assert!(without.get("effective_verdict").is_none());

	let affected = drift::failing_row_ids(without["per_record"].as_array().unwrap());
	let witness = drift::structural_witness(
		&fx.dir.0,
		&fx.pub_rev,
		&fx.cand_rev,
		&guard,
		&baseline,
		&candidate,
		&policy,
		&affected,
	)
	.unwrap();
	assert_eq!(witness["plan_commit"], fx.plan_rev);
	assert_eq!(witness["proof_path"], fx.proof_path);
	assert_eq!(witness["proof_sha256"], fx.proof_sha256);

	let with = drift::score_with_witness(
		&baseline,
		&candidate,
		Some(&anchor),
		&policy,
		Some(&witness),
		&fx.dir.0,
		&guard,
	)
	.unwrap();
	assert_eq!(with["status"], "PASS");
	assert_eq!(with["effective_verdict"], "accepted_layout_drift");
	assert_eq!(
		with["per_record"]
			.as_array()
			.unwrap()
			.iter()
			.filter(|r| r["pass_all"] == true)
			.count(),
		30
	);
	assert_eq!(
		with["cumulative"]
			.as_array()
			.unwrap()
			.iter()
			.filter(|r| r["pass_all"] == true)
			.count(),
		32
	);
	assert_eq!(
		with["layout_drift"]["added_variants"],
		json!(["heap::Payload::Bytes", "heap::Type::Bytes"])
	);

	// 1c (Fix 4 + Fix 12): reused same-parent docs-only publication baseline amend -> PASS,
	// while a child commit on top of `baseline_source` (different parent) -> REFUSED.
	let fx_reused = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts {
			pub_docs_amend: true,
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx_reused.base_rev);
	let r_anchor = make_capture(
		&fx_reused.base_rev,
		&policy,
		&"a".repeat(64),
		(0, 0),
		(0, 0),
	);
	let mut r_base = make_capture(
		&fx_reused.base_rev,
		&policy,
		&"a".repeat(64),
		(0, 0),
		(0, 0),
	);
	r_base["publication_commit"] = json!(fx_reused.pub_rev);
	let r_cand = make_capture(
		&fx_reused.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(-50_010, -250_054),
	);
	let reused_witness = drift::structural_witness(
		&fx_reused.dir.0,
		&fx_reused.pub_rev,
		&fx_reused.cand_rev,
		&guard,
		&r_base,
		&r_cand,
		&policy,
		&affected,
	)
	.unwrap();
	assert_eq!(reused_witness["baseline"], fx_reused.base_rev);
	assert_eq!(reused_witness["baseline_publication"], fx_reused.pub_rev);
	let reused_with = drift::score_with_witness(
		&r_base,
		&r_cand,
		Some(&r_anchor),
		&policy,
		Some(&reused_witness),
		&fx_reused.dir.0,
		&guard,
	)
	.unwrap();
	assert_eq!(reused_with["status"], "PASS");
	assert_eq!(reused_with["effective_verdict"], "accepted_layout_drift");

	// Child commit on top of `base_rev` (different parent) -> refused by `controller::docs_change`.
	git_in(
		&fx.dir.0,
		&["checkout", "-q", "-b", "pub-docs-child", &fx.base_rev],
	);
	std::fs::create_dir_all(fx.dir.0.join("plans/impl")).unwrap();
	std::fs::write(fx.dir.0.join("plans/impl/0354_child.md"), "Child commit.\n").unwrap();
	git_in(&fx.dir.0, &["add", "plans/impl/0354_child.md"]);
	git_signed(&fx.dir.0, &["commit", "-qm", "docs child"]);
	let child_pub_rev = git_in(&fx.dir.0, &["rev-parse", "HEAD"]);
	let mut child_base = baseline.clone();
	child_base["publication_commit"] = json!(child_pub_rev);
	assert!(
		drift::structural_witness(
			&fx.dir.0,
			&child_pub_rev,
			&fx.cand_rev,
			&guard,
			&child_base,
			&candidate,
			&policy,
			&affected,
		)
		.is_err(),
		"child commit on top of baseline_source must be refused by docs_change"
	);
}

#[test]
fn control2_checker_veto_controls_remain_enforced_even_with_signed_pinned_proof() {
	let guard = build_execute_guard();
	let mut policy = minimal_policy();
	policy["anchor"] = json!("base");
	let dummy_base = make_capture("base", &policy, &"a".repeat(64), (0, 0), (0, 0));
	let dummy_cand = make_capture("cand", &policy, &"b".repeat(64), (95_079, 475_411), (0, 0));
	let raw = fr::score(&dummy_base, &dummy_cand, Some(&dummy_base), &policy).unwrap();
	let affected = drift::failing_row_ids(raw["per_record"].as_array().unwrap());

	// 2a. Planted extra tag query on the old Vec path is refused by checker veto.
	let planted = DORMANT_BYTES_HEAP_RS.replace(
		"Payload::Vec(v) => v.len(),",
		"Payload::Vec(v) => { if matches!(p, Payload::Bytes(_)) { return 99; } v.len() },",
	);
	let fx1 = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", &planted)],
		&FixtureOpts::default(),
	);
	policy["anchor"] = json!(fx1.base_rev);
	let b1 = make_capture(&fx1.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c1 = make_capture(
		&fx1.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	assert!(
		drift::structural_witness(
			&fx1.dir.0,
			&fx1.pub_rev,
			&fx1.cand_rev,
			&guard,
			&b1,
			&c1,
			&policy,
			&affected
		)
		.is_err()
	);

	// 2b (Fix 2): cross-module same-named `Payload::Bytes` arm -> refused by checker veto.
	let fx2 = init_signed_repo_with_proof(
		&[
			("src/a.rs", "pub enum Payload { String(String) }\n"),
			(
				"src/b.rs",
				"pub enum Payload { Int, Bytes(u8) }\npub fn h(p: Payload) -> u8 { match p { Payload::Int => 0, _ => 1 } }\n",
			),
		],
		&[
			(
				"src/a.rs",
				"pub enum Payload { String(String), Bytes(Vec<u8>) }\n",
			),
			(
				"src/b.rs",
				"pub enum Payload { Int, Bytes(u8) }\npub fn h(p: Payload) -> u8 { match p { Payload::Int => 0, Payload::Bytes(x) => x + 1, _ => 1 } }\n",
			),
		],
		&FixtureOpts {
			added_variants: vec!["a::Payload::Bytes"],
			added_arms: vec![],
			extra_context: vec!["src/a.rs", "src/b.rs"],
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx2.base_rev);
	let b2 = make_capture(&fx2.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c2 = make_capture(
		&fx2.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let err2 = drift::structural_witness(
		&fx2.dir.0,
		&fx2.pub_rev,
		&fx2.cand_rev,
		&guard,
		&b2,
		&c2,
		&policy,
		&affected,
	)
	.unwrap_err()
	.to_string();
	assert!(err2.contains("src/b.rs"), "{err2}");

	// 2c (Fix 3): nested `mod controls { #[cfg(test)] mod worker; }` does not exempt `src/worker.rs`.
	let fx3 = init_signed_repo_with_proof(
		&[
			(
				"src/lib.rs",
				"mod heap;\nmod worker;\nmod controls {\n\t#[cfg(test)]\n\tmod worker;\n}\n",
			),
			("src/heap.rs", BASE_HEAP_RS),
			("src/worker.rs", "pub fn run() -> usize { 1 }\n"),
		],
		&[
			(
				"src/lib.rs",
				"mod heap;\nmod worker;\nmod controls {\n\t#[cfg(test)]\n\tmod worker;\n}\n",
			),
			("src/heap.rs", DORMANT_BYTES_HEAP_RS),
			("src/worker.rs", "pub fn run() -> usize { 2 }\n"),
		],
		&FixtureOpts {
			extra_context: vec!["src/lib.rs", "src/heap.rs"],
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx3.base_rev);
	let b3 = make_capture(&fx3.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c3 = make_capture(
		&fx3.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let err3 = drift::structural_witness(
		&fx3.dir.0,
		&fx3.pub_rev,
		&fx3.cand_rev,
		&guard,
		&b3,
		&c3,
		&policy,
		&affected,
	)
	.unwrap_err()
	.to_string();
	assert!(err3.contains("src/worker.rs"), "{err3}");

	// 2d (Fix 7): expanding `#[cfg_attr(feature = "serde", derive(Deserialize))]` -> refused.
	let fx4 = init_signed_repo_with_proof(
		&[(
			"src/heap.rs",
			"#[cfg_attr(feature = \"serde\", derive(Deserialize))]\npub enum Payload { String(String) }\n",
		)],
		&[(
			"src/heap.rs",
			"#[cfg_attr(feature = \"serde\", derive(Deserialize))]\npub enum Payload { String(String), Bytes(Vec<u8>) }\n",
		)],
		&FixtureOpts {
			added_variants: vec!["heap::Payload::Bytes"],
			added_arms: vec![],
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx4.base_rev);
	let b4 = make_capture(&fx4.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c4 = make_capture(
		&fx4.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let err4 = drift::structural_witness(
		&fx4.dir.0,
		&fx4.pub_rev,
		&fx4.cand_rev,
		&guard,
		&b4,
		&c4,
		&policy,
		&affected,
	)
	.unwrap_err()
	.to_string();
	assert!(err4.contains("non-inert cfg_attr"), "{err4}");

	// 2e (Fix 8): block-local enum shadowing changed enum (Codex's `shadow` fixture) -> refused.
	let fx5 = init_signed_repo_with_proof(
		&[(
			"src/lib.rs",
			"enum Payload { A }\npub fn f() -> u8 {\n    enum Payload { A, Bytes }\n    match Payload::Bytes { _ => 0 }\n}\n",
		)],
		&[(
			"src/lib.rs",
			"enum Payload { A, Bytes }\npub fn f() -> u8 {\n    enum Payload { A, Bytes }\n    match Payload::Bytes { Payload::Bytes => 1, _ => 0 }\n}\n",
		)],
		&FixtureOpts {
			added_variants: vec!["Payload::Bytes"],
			added_arms: vec![],
			extra_context: vec!["src/lib.rs"],
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx5.base_rev);
	let b5 = make_capture(&fx5.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c5 = make_capture(
		&fx5.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let err5 = drift::structural_witness(
		&fx5.dir.0,
		&fx5.pub_rev,
		&fx5.cand_rev,
		&guard,
		&b5,
		&c5,
		&policy,
		&affected,
	)
	.unwrap_err()
	.to_string();
	assert!(err5.contains("shadows changed enum"), "{err5}");

	// 2f (Fix 9): two crate roots sharing `src/worker.rs` (Codex's `two-roots` fixture) -> refused.
	let fx6 = init_signed_repo_with_proof(
		&[
			("src/lib.rs", "enum P { A }\n#[cfg(test)] mod worker;\n"),
			(
				"src/main.rs",
				"mod worker;\nfn main() { println!(\"{}\", worker::f()); }\n",
			),
			("src/worker.rs", "pub fn f() -> u8 { 0 }\n"),
		],
		&[
			(
				"src/lib.rs",
				"enum P { A, Bytes }\n#[cfg(test)] mod worker;\n",
			),
			(
				"src/main.rs",
				"mod worker;\nfn main() { println!(\"{}\", worker::f()); }\n",
			),
			("src/worker.rs", "pub fn f() -> u8 { 1 }\n"),
		],
		&FixtureOpts {
			added_variants: vec!["P::Bytes"],
			added_arms: vec![],
			extra_context: vec!["src/lib.rs", "src/main.rs"],
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx6.base_rev);
	let b6 = make_capture(&fx6.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c6 = make_capture(
		&fx6.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let err6 = drift::structural_witness(
		&fx6.dir.0,
		&fx6.pub_rev,
		&fx6.cand_rev,
		&guard,
		&b6,
		&c6,
		&policy,
		&affected,
	)
	.unwrap_err()
	.to_string();
	assert!(err6.contains("src/worker.rs"), "{err6}");

	// 2g (Fix 14): qualified custom derive `#[derive(my_crate::Clone)]` -> refused.
	let fx7 = init_signed_repo_with_proof(
		&[(
			"src/heap.rs",
			"#[derive(my_crate::Clone)]\npub enum Payload { String(String) }\n",
		)],
		&[(
			"src/heap.rs",
			"#[derive(my_crate::Clone)]\npub enum Payload { String(String), Bytes(Vec<u8>) }\n",
		)],
		&FixtureOpts {
			added_variants: vec!["heap::Payload::Bytes"],
			added_arms: vec![],
			..FixtureOpts::default()
		},
	);
	policy["anchor"] = json!(fx7.base_rev);
	let b7 = make_capture(&fx7.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c7 = make_capture(
		&fx7.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let err7 = drift::structural_witness(
		&fx7.dir.0,
		&fx7.pub_rev,
		&fx7.cand_rev,
		&guard,
		&b7,
		&c7,
		&policy,
		&affected,
	)
	.unwrap_err()
	.to_string();
	assert!(err7.contains("non-allowlisted derive"), "{err7}");

	// 2h (Fix 15): moving a production file under `plans/` (`git mv src/extra.rs plans/extra.rs`) -> refused.
	let fx8 = init_signed_repo_with_proof(
		&[
			("src/heap.rs", BASE_HEAP_RS),
			("src/extra.rs", "pub fn extra() -> u8 { 1 }\n"),
		],
		&[
			("src/heap.rs", DORMANT_BYTES_HEAP_RS),
			("plans/extra.rs", "pub fn extra() -> u8 { 1 }\n"),
		],
		&FixtureOpts::default(),
	);
	// Delete `src/extra.rs` in candidate (simulating `git mv src/extra.rs plans/extra.rs`).
	git_in(&fx8.dir.0, &["rm", "-q", "src/extra.rs"]);
	git_signed(
		&fx8.dir.0,
		&["commit", "--amend", "-qm", "impl: adr 0356 test dormant"],
	);
	let cand8 = git_in(&fx8.dir.0, &["rev-parse", "HEAD"]);
	policy["anchor"] = json!(fx8.base_rev);
	let b8 = make_capture(&fx8.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let c8 = make_capture(&cand8, &policy, &"b".repeat(64), (95_079, 475_411), (0, 0));
	assert!(
		drift::structural_witness(
			&fx8.dir.0,
			&fx8.pub_rev,
			&cand8,
			&guard,
			&b8,
			&c8,
			&policy,
			&affected,
		)
		.is_err()
	);
}

#[test]
fn control3_and_mixed_failure_on_same_row_stay_fail() {
	let guard = build_execute_guard();
	let fx = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts::default(),
	);
	let mut policy = minimal_policy();
	policy["anchor"] = json!(fx.base_rev);
	let anchor = make_capture(&fx.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let baseline = make_capture(&fx.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let candidate = make_capture(
		&fx.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(0, 0),
	);
	let raw = fr::score(&baseline, &candidate, Some(&anchor), &policy).unwrap();
	let affected = drift::failing_row_ids(raw["per_record"].as_array().unwrap());
	let witness = drift::structural_witness(
		&fx.dir.0,
		&fx.pub_rev,
		&fx.cand_rev,
		&guard,
		&baseline,
		&candidate,
		&policy,
		&affected,
	)
	.unwrap();

	// 3a. Unsupported witness kind -> FAIL.
	let mut unsupported = witness.clone();
	unsupported["kind"] = json!("measured_zero_entries");
	let unsupported = drift::seal_witness(unsupported).unwrap();
	assert_eq!(
		drift::score_with_witness(
			&baseline,
			&candidate,
			Some(&anchor),
			&policy,
			Some(&unsupported),
			&fx.dir.0,
			&guard,
		)
		.unwrap()["status"],
		"FAIL"
	);

	// 3b. Stale witness (wrong candidate revision) -> FAIL.
	let mut stale = witness.clone();
	stale["candidate"] = json!("0".repeat(40));
	let stale = drift::seal_witness(stale).unwrap();
	assert_eq!(
		drift::score_with_witness(
			&baseline,
			&candidate,
			Some(&anchor),
			&policy,
			Some(&stale),
			&fx.dir.0,
			&guard,
		)
		.unwrap()["status"],
		"FAIL"
	);

	// 3c. Cumulative failure (> 10 per mille against anchor) -> FAIL even with valid witness.
	let cand_cum_fail = make_capture(
		&fx.cand_rev,
		&policy,
		&"b".repeat(64),
		(200_000, 1_000_000),
		(0, 0),
	);
	let cum_witness = drift::structural_witness(
		&fx.dir.0,
		&fx.pub_rev,
		&fx.cand_rev,
		&guard,
		&baseline,
		&cand_cum_fail,
		&policy,
		&affected,
	)
	.unwrap();
	assert_eq!(
		drift::score_with_witness(
			&baseline,
			&cand_cum_fail,
			Some(&anchor),
			&policy,
			Some(&cum_witness),
			&fx.dir.0,
			&guard,
		)
		.unwrap()["status"],
		"FAIL"
	);

	// 3d. Mixed failure on the SAME row stays FAIL.
	let mixed_off_row = json!({
		"kind": "awfy_sieve",
		"mode": "off",
		"metric": "W",
		"level_pass": false,
		"growth_pass": false,
		"pass_all": false,
		"dispatch_levels": [
			{"before": 100_000, "after": 100_000, "allowance": 500, "pass": true},
			{"before": 100_000, "after": 105_000, "allowance": 500, "pass": false},
		]
	});
	assert!(!drift::row_execution_only_failure(&mixed_off_row));

	let mixed_on_row = json!({
		"kind": "awfy_sieve",
		"mode": "on",
		"metric": "W",
		"level_pass": false,
		"growth_pass": false,
		"pass_all": false,
		"phases": {
			"setup": [{"axis": "size5", "pass": false}],
			"execution": [{"axis": "size5", "pass": false}],
		}
	});
	assert!(!drift::row_execution_only_failure(&mixed_on_row));
}

#[test]
fn control4_rescore_replay_forged_witness_dropped_row_edited_allowance_and_successor_symmetry() {
	let guard = build_execute_guard();
	let fx = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts::default(),
	);
	let mut policy = minimal_policy();
	policy["anchor"] = json!(fx.base_rev);
	let anchor = make_capture(&fx.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let baseline = make_capture(&fx.base_rev, &policy, &"a".repeat(64), (0, 0), (0, 0));
	let candidate = make_capture(
		&fx.cand_rev,
		&policy,
		&"b".repeat(64),
		(95_079, 475_411),
		(-50_010, -250_054),
	);
	let raw = fr::score(&baseline, &candidate, Some(&anchor), &policy).unwrap();
	let affected = drift::failing_row_ids(raw["per_record"].as_array().unwrap());
	let witness = drift::structural_witness(
		&fx.dir.0,
		&fx.pub_rev,
		&fx.cand_rev,
		&guard,
		&baseline,
		&candidate,
		&policy,
		&affected,
	)
	.unwrap();
	let mut gate = drift::score_with_witness(
		&baseline,
		&candidate,
		Some(&anchor),
		&policy,
		Some(&witness),
		&fx.dir.0,
		&guard,
	)
	.unwrap();
	gate["policy"] = policy.clone();
	gate["baseline"] = baseline.clone();
	gate["candidate"] = candidate.clone();

	let out = TempDir::new("rnx-0355-released");
	let gate_dir = out.0.join("fixed-route");
	std::fs::create_dir_all(&gate_dir).unwrap();
	let anchor_path = gate_dir.join("ANCHOR-EVIDENCE.json");
	atomic_json(&anchor_path, &anchor).unwrap();
	gate["anchor"] = json!(anchor_path.to_string_lossy());
	let receipt_path = gate_dir.join("RESULT.json");
	atomic_json(&receipt_path, &gate).unwrap();
	let reloaded: Value = serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();

	// Omitting repo/guard must fail closed.
	assert!(drift::revalidate_receipt(&reloaded, Some(&gate_dir), None, None).is_err());

	// Valid replay with full rescore + signed pin/proof + checker veto PASSES.
	let replayed =
		drift::revalidate_receipt(&reloaded, Some(&gate_dir), Some(&fx.dir.0), Some(&guard))
			.unwrap();
	assert_eq!(replayed["status"], "PASS");
	assert_eq!(replayed["effective_verdict"], "accepted_layout_drift");
	assert_eq!(replayed["raw_per_record_passed"], 30);
	assert_eq!(replayed["raw_cumulative_passed"], 32);

	// Forged witness WITH recomputed witness_sha256 fails replay.
	let mut forged = reloaded.clone();
	forged["layout_drift"]["added_variants"] = json!(["heap::Payload::Forged"]);
	forged["layout_drift"] = drift::seal_witness(forged["layout_drift"].clone()).unwrap();
	assert!(
		drift::revalidate_receipt(&forged, Some(&gate_dir), Some(&fx.dir.0), Some(&guard)).is_err()
	);

	// Tampered guard_source_sha256 fails replay (Fix 6).
	let mut bad_guard = reloaded.clone();
	bad_guard["layout_drift"]["guard_source_sha256"] = json!("0".repeat(64));
	bad_guard["layout_drift"] = drift::seal_witness(bad_guard["layout_drift"].clone()).unwrap();
	assert!(
		drift::revalidate_receipt(&bad_guard, Some(&gate_dir), Some(&fx.dir.0), Some(&guard))
			.is_err()
	);

	// Tampered proof_sha256 in witness fails replay.
	let mut bad_proof_sha = reloaded.clone();
	bad_proof_sha["layout_drift"]["proof_sha256"] = json!("0".repeat(64));
	bad_proof_sha["layout_drift"] =
		drift::seal_witness(bad_proof_sha["layout_drift"].clone()).unwrap();
	assert!(
		drift::revalidate_receipt(
			&bad_proof_sha,
			Some(&gate_dir),
			Some(&fx.dir.0),
			Some(&guard)
		)
		.is_err()
	);

	// Dropped row fails rescore (Fix 5a).
	let mut dropped = reloaded.clone();
	dropped["per_record"].as_array_mut().unwrap().remove(0);
	dropped["cumulative"].as_array_mut().unwrap().remove(0);
	assert!(
		drift::revalidate_receipt(&dropped, Some(&gate_dir), Some(&fx.dir.0), Some(&guard))
			.is_err()
	);

	// Edited row count fails rescore (Fix 5b).
	let mut edited = reloaded.clone();
	edited["per_record"][0]["C5"] = json!(999_999_999);
	assert!(
		drift::revalidate_receipt(&edited, Some(&gate_dir), Some(&fx.dir.0), Some(&guard)).is_err()
	);

	// Missing fixed-route/RESULT.json on accepted_layout_drift final (Fix 1) +
	// symmetric successor effective_verdict check (Fix 13).
	let final_dir = TempDir::new("rnx-0355-final-missing");
	let acc = json!({
		"status": "PASS", "candidate": fx.cand_rev, "acceptance_scope": "complete", "mode": "ordinary",
		"checks": [], "seconds": 1.0, "memory": {"tokens": "4", "events_before": null, "events_after": null}
	});
	let man = json!({
		"schema": 1, "candidate": fx.cand_rev, "acceptance_scope": "complete", "roots": []
	});
	atomic_json(&final_dir.0.join("ACCEPTANCE.json"), &acc).unwrap();
	atomic_json(&final_dir.0.join("MANIFEST.json"), &man).unwrap();
	let acc_sha = digest(&final_dir.0.join("ACCEPTANCE.json")).unwrap();
	let man_sha = digest(&final_dir.0.join("MANIFEST.json")).unwrap();
	let rel = json!({
		"status": "PASS", "roots": [], "acceptance_sha256": acc_sha, "manifest_sha256": man_sha
	});
	atomic_json(&final_dir.0.join("RELEASE.json"), &rel).unwrap();
	let rel_sha = digest(&final_dir.0.join("RELEASE.json")).unwrap();
	let top = json!({
		"status": "PASS", "candidate": fx.cand_rev, "seconds": 1.0, "checks": [],
		"error": null, "release_error": null, "manifest_error": null, "diagnostics_error": null,
		"finalization": null, "remote_context": null, "acceptance_scope": "complete",
		"complete_final": true, "publication_eligible": true,
		"free_bytes_before": 100, "free_bytes_after": 100,
		"records": {"ACCEPTANCE": acc_sha, "MANIFEST": man_sha, "RELEASE": rel_sha},
		"effective_verdict": "accepted_layout_drift",
		"fixed_route_sha256": digest(&receipt_path).unwrap(),
	});
	atomic_json(&final_dir.0.join("RESULT.json"), &top).unwrap();
	let err = rnx_final::evidence::revalidate(&final_dir.0)
		.unwrap_err()
		.to_string();
	assert!(err.contains("missing fixed-route/RESULT.json"), "{err}");

	let succ_dir = TempDir::new("rnx-0355-succ-mismatch");
	let prior_ord = TempDir::new("rnx-0355-prior-ordinary");
	atomic_json(&prior_ord.0.join("ACCEPTANCE.json"), &acc).unwrap();
	atomic_json(&prior_ord.0.join("MANIFEST.json"), &man).unwrap();
	atomic_json(&prior_ord.0.join("RELEASE.json"), &rel).unwrap();
	let mut top_ord = top.clone();
	top_ord.as_object_mut().unwrap().remove("effective_verdict");
	top_ord
		.as_object_mut()
		.unwrap()
		.remove("fixed_route_sha256");
	atomic_json(&prior_ord.0.join("RESULT.json"), &top_ord).unwrap();
	let succ_forged = json!({
		"status": "PASS",
		"candidate": fx.cand_rev,
		"prior": prior_ord.0.to_string_lossy(),
		"prior_candidate": fx.cand_rev,
		"prior_result_sha256": digest(&prior_ord.0.join("RESULT.json")).unwrap(),
		"plans_final": prior_ord.0.to_string_lossy(),
		"plans_final_result_sha256": digest(&prior_ord.0.join("RESULT.json")).unwrap(),
		"changed": [],
		"reran": [],
		"effective_verdict": "accepted_layout_drift",
	});
	atomic_json(&succ_dir.0.join("SUCCESSOR.json"), &succ_forged).unwrap();
	let err_succ_ord = rnx_final::controller::revalidate_successor(&succ_dir.0, Some(&fx.dir.0))
		.unwrap_err()
		.to_string();
	assert!(
		err_succ_ord.contains("disagrees with prior final"),
		"{err_succ_ord}"
	);

	let succ_missing_stage = json!({
		"status": "PASS",
		"candidate": fx.cand_rev,
		"prior": final_dir.0.to_string_lossy(),
		"prior_candidate": fx.cand_rev,
		"prior_result_sha256": digest(&final_dir.0.join("RESULT.json")).unwrap(),
		"plans_final": prior_ord.0.to_string_lossy(),
		"plans_final_result_sha256": digest(&prior_ord.0.join("RESULT.json")).unwrap(),
		"changed": [],
		"reran": [],
		"effective_verdict": "accepted_layout_drift",
	});
	atomic_json(&succ_dir.0.join("SUCCESSOR.json"), &succ_missing_stage).unwrap();
	let err_succ_missing =
		rnx_final::controller::revalidate_successor(&succ_dir.0, Some(&fx.dir.0))
			.unwrap_err()
			.to_string();
	assert!(
		err_succ_missing.contains("missing fixed-route/RESULT.json"),
		"{err_succ_missing}"
	);
}

#[test]
fn control5_reviewed_proof_pin_and_codex_bin_derive_refused() {
	let guard = build_execute_guard();
	let mut policy = minimal_policy();
	policy["anchor"] = json!("base");
	let dummy_base = make_capture("base", &policy, &"a".repeat(64), (0, 0), (0, 0));
	let dummy_cand = make_capture("cand", &policy, &"b".repeat(64), (95_079, 475_411), (0, 0));
	let raw = fr::score(&dummy_base, &dummy_cand, Some(&dummy_base), &policy).unwrap();
	let affected = drift::failing_row_ids(raw["per_record"].as_array().unwrap());

	let check_refused = |fx: &SignedFixture, cand_override: Option<&str>, needle: &str| {
		let cand = cand_override.unwrap_or(&fx.cand_rev);
		let mut p = policy.clone();
		p["anchor"] = json!(fx.base_rev);
		let b = make_capture(&fx.base_rev, &p, &"a".repeat(64), (0, 0), (0, 0));
		let c = make_capture(cand, &p, &"b".repeat(64), (95_079, 475_411), (0, 0));
		let err =
			drift::structural_witness(&fx.dir.0, &fx.pub_rev, cand, &guard, &b, &c, &p, &affected)
				.unwrap_err()
				.to_string();
		assert!(err.contains(needle), "expected `{needle}` in error: {err}");
	};

	// (1) Pin is missing from signed plan.
	let fx_no_pin = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts {
			include_pin: false,
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_no_pin, None, "missing dormant proof pin");

	// (2) Pin has the wrong digest in signed plan.
	let fx_wrong_pin = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts {
			wrong_pin_digest: true,
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_wrong_pin, None, "disagrees with signed plan pin");

	// (3) Proof is edited ONLY in the candidate (valid impl signature, even with valid JSON)
	// -> disagrees with signed plan pin, leaves ordinary FAIL.
	let fx_cand_edit = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts::default(),
	);
	let proof_file = fx_cand_edit.dir.0.join(&fx_cand_edit.proof_path);
	let mut edited_bytes = std::fs::read(&proof_file).unwrap();
	edited_bytes.push(b'\n');
	std::fs::write(&proof_file, &edited_bytes).unwrap();
	git_in(&fx_cand_edit.dir.0, &["add", &fx_cand_edit.proof_path]);
	git_signed(
		&fx_cand_edit.dir.0,
		&["commit", "--amend", "-qm", "impl: adr 0356 test dormant"],
	);
	let edited_cand = git_in(&fx_cand_edit.dir.0, &["rev-parse", "HEAD"]);
	check_refused(
		&fx_cand_edit,
		Some(&edited_cand),
		"disagrees with signed plan pin",
	);

	// (4a) Impl commit is unsigned (plan is signed).
	let fx_unsigned_impl = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts {
			sign_impl: false,
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_unsigned_impl, None, "failed signature verification");

	// (4b) Plan commit is unsigned (impl is signed).
	let fx_unsigned_plan = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts {
			sign_plan: false,
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_unsigned_plan, None, "failed signature verification");

	// Helper to build a signed fixture where we mutate the proof JSON BEFORE pinning it in the
	// signed plan, so signature & pin checks pass and the specific proof-content check triggers!
	let build_mutated_proof = |files_base: &[(&str, &str)],
	                           files_cand: &[(&str, &str)],
	                           chmod_exec: Option<&str>,
	                           mutate: &dyn Fn(&mut Value)|
	 -> SignedFixture {
		let dir = TempDir::new("rnx-0355-mutated");
		git_in(&dir.0, &["init", "-q"]);
		configure_signed_repo(&dir.0);
		std::fs::create_dir_all(dir.0.join("ci/execute_guard/src")).unwrap();
		std::fs::write(
			dir.0.join("ci/execute_guard/Cargo.toml"),
			"[package]\nname=\"g\"\n",
		)
		.unwrap();
		std::fs::write(dir.0.join("ci/execute_guard/src/main.rs"), "fn main() {}\n").unwrap();
		git_in(&dir.0, &["add", "-A"]);
		git_signed(&dir.0, &["commit", "-qm", "seed"]);
		for (rel, body) in files_base {
			let p = dir.0.join(rel);
			std::fs::create_dir_all(p.parent().unwrap()).unwrap();
			std::fs::write(&p, body).unwrap();
		}
		git_in(&dir.0, &["add", "-A"]);
		git_signed(&dir.0, &["commit", "-qm", "impl: adr 0355 baseline"]);
		let base_rev = git_in(&dir.0, &["rev-parse", "HEAD"]);

		let base_map: std::collections::BTreeMap<&str, &str> = files_base.iter().copied().collect();
		let cand_map: std::collections::BTreeMap<&str, &str> = files_cand.iter().copied().collect();
		let mut delta_rows = Vec::new();
		for (path, new_s) in &cand_map {
			let old_s = base_map.get(path).copied();
			if old_s == Some(*new_s) {
				continue;
			}
			delta_rows.push(json!({
				"path": path,
				"old_mode": old_s.map(|_| "100644"),
				"new_mode": "100644",
				"old_blob": old_s.map(|s| blob_id_of_bytes(&dir.0, s.as_bytes())),
				"new_blob": blob_id_of_bytes(&dir.0, new_s.as_bytes()),
			}));
		}
		let mut proof = json!({
			"schema": 1,
			"record": "0356",
			"kind": "representation_only",
			"baseline": base_rev,
			"delta": delta_rows,
			"context": [{
				"path": "src/heap.rs",
				"blob": blob_id_of_bytes(&dir.0, cand_map["src/heap.rs"].as_bytes()),
			}],
			"added_variants": ["heap::Payload::Bytes", "heap::Type::Bytes"],
			"added_arms": [
				{"file": "src/heap.rs", "pattern": "Self : : Bytes ( _ )"},
				{"file": "src/heap.rs", "pattern": "| Payload : : Bytes ( _ )"},
			],
		});
		mutate(&mut proof);
		let proof_path = "tools/column/dormant/0356.json".to_owned();
		let proof_bytes = serde_json::to_vec_pretty(&proof).unwrap();
		let proof_sha256 = hex(&Sha256::digest(&proof_bytes));

		std::fs::create_dir_all(dir.0.join("plans/adr")).unwrap();
		std::fs::create_dir_all(dir.0.join("plans/impl")).unwrap();
		std::fs::write(
			dir.0.join("plans/adr/0356_test.md"),
			"# ADR 0356\n\nStatus: Proposed\n",
		)
		.unwrap();
		std::fs::write(
			dir.0.join("plans/impl/0356_test.md"),
			format!(
				"# Implementation plan 0356\n\nStatus: Proposed\n\nDormant proof: `{proof_path}`\nDormant proof sha256: `{proof_sha256}`\n"
			),
		)
		.unwrap();
		git_in(&dir.0, &["add", "-A"]);
		git_signed(&dir.0, &["commit", "-qm", "plan: adr 0356 test mutated"]);
		let plan_rev = git_in(&dir.0, &["rev-parse", "HEAD"]);

		for (rel, body) in files_cand {
			let p = dir.0.join(rel);
			std::fs::create_dir_all(p.parent().unwrap()).unwrap();
			std::fs::write(&p, body).unwrap();
		}
		let pp = dir.0.join(&proof_path);
		std::fs::create_dir_all(pp.parent().unwrap()).unwrap();
		std::fs::write(&pp, &proof_bytes).unwrap();
		git_in(&dir.0, &["add", "-A"]);
		if let Some(exec_path) = chmod_exec {
			git_in(&dir.0, &["update-index", "--chmod=+x", exec_path]);
		}
		git_signed(&dir.0, &["commit", "-qm", "impl: adr 0356 test mutated"]);
		let cand_rev = git_in(&dir.0, &["rev-parse", "HEAD"]);

		SignedFixture {
			dir,
			base_rev: base_rev.clone(),
			pub_rev: base_rev,
			plan_rev,
			cand_rev,
			proof_path,
			proof_sha256,
		}
	};

	// (5) A changed path in git diff is missing from `proof.delta`.
	let fx_missing_delta = build_mutated_proof(
		&[
			("src/heap.rs", BASE_HEAP_RS),
			("src/other.rs", "pub fn x() -> u8 { 0 }\n"),
		],
		&[
			("src/heap.rs", DORMANT_BYTES_HEAP_RS),
			("src/other.rs", "pub fn x() -> u8 { 1 }\n"),
		],
		None,
		&|p| {
			p["delta"]
				.as_array_mut()
				.unwrap()
				.retain(|r| r["path"] != "src/other.rs");
		},
	);
	check_refused(
		&fx_missing_delta,
		None,
		"reviewed proof delta does not match exact git diff",
	);

	// (6) An extra/unmodified path is listed in `proof.delta` that did not change in git diff.
	let fx_extra_delta = build_mutated_proof(
		&[
			("src/heap.rs", BASE_HEAP_RS),
			("src/other.rs", "pub fn x() -> u8 { 0 }\n"),
		],
		&[
			("src/heap.rs", DORMANT_BYTES_HEAP_RS),
			("src/other.rs", "pub fn x() -> u8 { 0 }\n"),
		],
		None,
		&|p| {
			p["delta"].as_array_mut().unwrap().push(json!({
				"path": "src/other.rs",
				"old_mode": "100644",
				"new_mode": "100644",
				"old_blob": "1".repeat(40),
				"new_blob": "2".repeat(40),
			}));
		},
	);
	check_refused(
		&fx_extra_delta,
		None,
		"reviewed proof delta does not match exact git diff",
	);

	// (7) Mode differs (`proof.delta` lists `100644` while candidate committed `100755`).
	let fx_mode_diff = build_mutated_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		Some("src/heap.rs"),
		&|_| {},
	);
	check_refused(
		&fx_mode_diff,
		None,
		"reviewed proof delta does not match exact git diff",
	);

	// (8) A path is duplicated in `proof.delta`.
	let fx_dup_delta = build_mutated_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		None,
		&|p| {
			let first = p["delta"][0].clone();
			p["delta"].as_array_mut().unwrap().push(first);
		},
	);
	check_refused(&fx_dup_delta, None, "duplicate path");

	// (9) A context blob is wrong.
	let fx_bad_ctx = build_mutated_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		None,
		&|p| {
			p["context"][0]["blob"] = json!("f".repeat(40));
		},
	);
	check_refused(&fx_bad_ctx, None, "context blob");

	// (10) The baseline in `proof.json` is stale.
	let fx_stale_base = build_mutated_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		None,
		&|p| {
			p["baseline"] = json!("0".repeat(40));
		},
	);
	check_refused(&fx_stale_base, None, "reviewed proof baseline disagrees");

	// (11) The proof file is absent in candidate (even though pinned in signed plan).
	let fx_absent = init_signed_repo_with_proof(
		&[("src/heap.rs", BASE_HEAP_RS)],
		&[("src/heap.rs", DORMANT_BYTES_HEAP_RS)],
		&FixtureOpts {
			write_proof_file: false,
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_absent, None, "missing or non-unique");

	// (12a) Codex's `bin` fixture (`#[path]` module + `#[cfg(test)]` in unrelated bin) has no
	// reviewed proof -> refused!
	let fx_codex_bin = init_signed_repo_with_proof(
		&[
			(
				"src/lib.rs",
				"enum P { A }\n#[path = \"worker.rs\"] mod prod;\n",
			),
			(
				"src/bin/extra.rs",
				"#[cfg(test)] mod worker;\nfn main() {}\n",
			),
			("src/worker.rs", "pub fn f() -> u8 { 0 }\n"),
		],
		&[
			(
				"src/lib.rs",
				"enum P { A, Bytes }\n#[path = \"worker.rs\"] mod prod;\n",
			),
			(
				"src/bin/extra.rs",
				"#[cfg(test)] mod worker;\nfn main() {}\n",
			),
			("src/worker.rs", "pub fn f() -> u8 { 1 }\n"),
		],
		&FixtureOpts {
			include_pin: false,
			write_proof_file: false,
			added_variants: vec!["P::Bytes"],
			added_arms: vec![],
			extra_context: vec!["src/lib.rs", "src/bin/extra.rs"],
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_codex_bin, None, "missing dormant proof pin");

	// (12b) Codex's `derive` fixture (derive alias `use custom::MyDerive as Clone;`) has no
	// reviewed proof -> refused!
	let fx_codex_derive = init_signed_repo_with_proof(
		&[(
			"src/lib.rs",
			"use custom::MyDerive as Clone;\n#[derive(Clone)]\npub enum P { A }\n",
		)],
		&[(
			"src/lib.rs",
			"use custom::MyDerive as Clone;\n#[derive(Clone)]\npub enum P { A, Bytes }\n",
		)],
		&FixtureOpts {
			include_pin: false,
			write_proof_file: false,
			added_variants: vec!["P::Bytes"],
			added_arms: vec![],
			extra_context: vec!["src/lib.rs"],
			..FixtureOpts::default()
		},
	);
	check_refused(&fx_codex_derive, None, "missing dormant proof pin");
}
