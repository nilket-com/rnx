//! Dormant layout-drift verdict, structural witness, and full-rescore
//! git-tree revalidation with candidate-bound checker provenance (ADR 0355).
use crate::child::{git_command, value_error};
use crate::pyjson::{hex, identity};
use crate::receipt::digest;
use anyhow::{Context as _, Result, bail, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const VERDICT_PASS: &str = "pass";
pub const VERDICT_ACCEPTED_DRIFT: &str = "accepted_layout_drift";
pub const VERDICT_FAIL: &str = "fail";

/// True iff this per-record row failed ONLY on execution (level/growth or
/// `phases.execution`), while every non-execution check on the row (`phases.setup`
/// on `mode == "on"`, `dispatch_levels` on `mode == "off"`) passed.
pub fn row_execution_only_failure(row: &Value) -> bool {
	if row["pass_all"] == true {
		return false;
	}
	match row["mode"].as_str() {
		Some("on") => {
			let Some(setup) = row["phases"]["setup"].as_array() else {
				return false;
			};
			if setup.is_empty() || !setup.iter().all(|c| c["pass"] == true) {
				return false;
			}
			let exec_failed = row["phases"]["execution"]
				.as_array()
				.is_some_and(|checks| checks.iter().any(|c| c["pass"] == false));
			let growth_failed = row["growth_pass"] == false;
			exec_failed || growth_failed
		}
		Some("off") => {
			if let Some(dispatch) = row.get("dispatch_levels") {
				let Some(checks) = dispatch.as_array() else {
					return false;
				};
				if checks.is_empty() || !checks.iter().all(|c| c["pass"] == true) {
					return false;
				}
			}
			row["level_pass"] == false || row["growth_pass"] == false
		}
		_ => false,
	}
}

/// Canonical list of failing per-record row identifiers.
pub fn failing_row_ids(per_record: &[Value]) -> Vec<Value> {
	per_record
		.iter()
		.filter(|r| r["pass_all"] != true)
		.map(|r| {
			json!({
				"kind": r["kind"],
				"mode": r["policy"].as_str().or_else(|| r["mode"].as_str()),
				"metric": r["metric"],
			})
		})
		.collect()
}

/// Build the per-cell matrix binding tying every `(kind, mode, n)` in `policy`
/// to both the baseline and candidate capture receipts' image identities and keys.
pub fn matrix_binding(baseline: &Value, candidate: &Value, policy: &Value) -> Result<Value> {
	let extract = |receipt: &Value| -> Result<Value> {
		let mut cells = Vec::new();
		let mut seen = BTreeSet::new();
		for row in receipt["records"]
			.as_array()
			.context("missing capture records")?
		{
			let key = (
				row["kind"].as_str().context("cell kind")?.to_owned(),
				row["policy"].as_str().context("cell mode")?.to_owned(),
				row["n"].as_i64().context("cell size")?,
			);
			seen.insert(key.clone());
			cells.push(json!({
				"kind": key.0,
				"mode": key.1,
				"n": key.2,
				"fixture_sha256": row["fixture_sha256"],
				"stdout_sha256": row["stdout_sha256"],
			}));
		}
		let mut expected = BTreeSet::new();
		for k in policy["kernels"].as_array().context("policy kernels")? {
			for m in policy["modes"].as_array().context("policy modes")? {
				for n in policy["sizes"].as_array().context("policy sizes")? {
					expected.insert((
						k.as_str().context("kernel")?.to_owned(),
						m.as_str().context("mode")?.to_owned(),
						n.as_i64().context("size")?,
					));
				}
			}
		}
		ensure!(seen == expected, "incomplete matrix binding");
		cells.sort_by(|a, b| {
			(
				a["kind"].as_str().unwrap_or(""),
				a["mode"].as_str().unwrap_or(""),
				a["n"].as_i64().unwrap_or(0),
			)
				.cmp(&(
					b["kind"].as_str().unwrap_or(""),
					b["mode"].as_str().unwrap_or(""),
					b["n"].as_i64().unwrap_or(0),
				))
		});
		Ok(json!({
			"revision": receipt["revision"],
			"binary_sha256": receipt["binary_sha256"],
			"capture_key": receipt["capture_key"],
			"measured": receipt["capture_paths"]["measured"],
			"cells": cells,
		}))
	};
	Ok(json!({
		"baseline": extract(baseline)?,
		"candidate": extract(candidate)?,
	}))
}

/// Compute the canonical SHA-256 of a witness object (excluding `witness_sha256`).
pub fn witness_digest(witness: &Value) -> Result<String> {
	let mut body = witness
		.as_object()
		.context("witness must be an object")?
		.clone();
	body.remove("witness_sha256");
	identity(&Value::Object(body))
}

/// Attach `witness_sha256` to a witness object.
pub fn seal_witness(mut witness: Value) -> Result<Value> {
	let sha = witness_digest(&witness)?;
	witness
		.as_object_mut()
		.context("witness must be an object")?
		.insert("witness_sha256".into(), json!(sha));
	Ok(witness)
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
	let output = git_command()
		.arg("--no-replace-objects")
		.args(args)
		.current_dir(root)
		.output()?;
	ensure!(
		output.status.success(),
		"git {:?} failed: {}",
		args,
		String::from_utf8_lossy(&output.stderr)
	);
	Ok(output.stdout)
}

/// Verify that `baseline_publication` is either identical to `baseline_source` or
/// a verified same-parent docs-only amend of `baseline_source` via `controller::docs_change` (Fix 12).
pub fn verify_baseline_succession(
	root: &Path,
	baseline_source: &str,
	baseline_publication: &str,
) -> Result<()> {
	if baseline_source == baseline_publication {
		return Ok(());
	}
	crate::controller::docs_change(root, baseline_source, baseline_publication)?;
	Ok(())
}

/// Canonical SHA-256 of the `ci/execute_guard` source tree at git revision `rev`.
pub fn guard_source_sha256(root: &Path, rev: &str) -> Result<String> {
	let tree = git_bytes(root, &["ls-tree", "-r", rev, "ci/execute_guard"])?;
	let text = String::from_utf8(tree)?;
	let mut entries = Vec::new();
	for line in text.lines() {
		if line.is_empty() {
			continue;
		}
		let Some((meta, path)) = line.split_once('\t') else {
			continue;
		};
		if path.starts_with("ci/execute_guard/target/") {
			continue;
		}
		let blob = git_bytes(root, &["show", &format!("{rev}:{path}")])?;
		entries.push(json!({
			"path": path,
			"mode": meta.split_whitespace().next().unwrap_or(""),
			"sha256": hex(&Sha256::digest(&blob)),
		}));
	}
	ensure!(
		!entries.is_empty(),
		"ci/execute_guard missing at revision {rev}"
	);
	entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
	identity(&Value::Array(entries))
}

/// Extract `ci/execute_guard` from `candidate`'s git tree into a temporary directory
/// and build it cleanly (Fix 6: replay never trusts an unreviewed working-dir checker).
pub struct BuiltGuard {
	pub binary: PathBuf,
	dir: PathBuf,
}

impl Drop for BuiltGuard {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.dir);
	}
}

pub fn build_guard_from_revision(root: &Path, candidate: &str) -> Result<BuiltGuard> {
	let dir = std::env::temp_dir().join(format!(
		"rnx-reval-guard-{}",
		crate::receipt::invocation_id()
	));
	let src_dir = dir.join("src");
	let target_dir = dir.join("target");
	std::fs::create_dir_all(&src_dir)?;
	let tree = git_bytes(root, &["ls-tree", "-r", candidate, "ci/execute_guard"])?;
	let text = String::from_utf8(tree)?;
	let mut wrote = false;
	for line in text.lines() {
		let Some((_, path)) = line.split_once('\t') else {
			continue;
		};
		if path.starts_with("ci/execute_guard/target/") {
			continue;
		}
		let blob = git_bytes(root, &["show", &format!("{candidate}:{path}")])?;
		let dest = src_dir.join(path);
		std::fs::create_dir_all(dest.parent().context("guard file parent")?)?;
		std::fs::write(&dest, &blob)?;
		wrote = true;
	}
	ensure!(wrote, "ci/execute_guard missing at candidate {candidate}");
	let manifest = src_dir.join("ci/execute_guard/Cargo.toml");
	let status = Command::new("cargo")
		.args([
			"+1.98.1",
			"build",
			"--locked",
			"--offline",
			"--manifest-path",
		])
		.arg(&manifest)
		.arg("--target-dir")
		.arg(&target_dir)
		.status()?;
	if !status.success() {
		let _ = std::fs::remove_dir_all(&dir);
		bail!(value_error(
			"failed to build ci/execute_guard from bound candidate tree"
		));
	}
	let binary = target_dir.join("debug/rnx-execute-drift-guard");
	ensure!(binary.is_file(), "built execute-guard binary missing");
	Ok(BuiltGuard { binary, dir })
}

/// Collect ancestor and sibling `.rs` module files under `src/` so the AST
/// checker can verify exact parent-module `#[cfg(test)]` edges and `pub use`
/// re-exports (e.g. `experimental_core/error.rs` re-exporting `detail::TypeName`).
fn ancestor_module_dirs(rel_path: &str) -> Vec<String> {
	let src_idx = if rel_path.starts_with("src/") {
		Some(0)
	} else {
		rel_path.find("/src/").map(|i| i + 1)
	};
	let Some(src_start) = src_idx else {
		return Vec::new();
	};
	let src_prefix = &rel_path[..src_start + 4];
	let under_src = &rel_path[src_start + 4..];
	let mut cur = Path::new(under_src).parent().unwrap_or(Path::new(""));
	let mut dirs = Vec::new();
	loop {
		if cur.as_os_str().is_empty() {
			dirs.push(src_prefix.trim_end_matches('/').to_owned());
			break;
		}
		dirs.push(format!("{src_prefix}{}", cur.to_string_lossy()));
		cur = cur.parent().unwrap_or(Path::new(""));
	}
	dirs
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DeltaRow {
	path: String,
	old_mode: Option<String>,
	new_mode: Option<String>,
	old_blob: Option<String>,
	new_blob: Option<String>,
}

struct VerifiedReviewedProof {
	record: String,
	plan_commit: String,
	plan_pin_doc: String,
	proof_path: String,
	proof_sha256: String,
	proof_json: Value,
}

fn is_hex(s: &str, len: usize) -> bool {
	s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn parse_record_subject(subject: &str, expected_kind: &str) -> Result<String> {
	let prefix = format!("{expected_kind}: adr ");
	let rest = subject
		.strip_prefix(&prefix)
		.with_context(|| format!("commit subject `{subject}` does not start with `{prefix}`"))?;
	ensure!(
		rest.len() >= 5 && rest.as_bytes()[4] == b' ',
		"malformed record number in subject `{subject}`"
	);
	let num = &rest[..4];
	ensure!(
		num.bytes().all(|b| b.is_ascii_digit()),
		"non-digit record number in subject `{subject}`"
	);
	Ok(num.to_owned())
}

fn extract_plan_pin(
	root: &Path,
	plan_commit: &str,
	record: &str,
) -> Result<(String, String, String)> {
	let expected_path = format!("tools/column/dormant/{record}.json");
	let tree = git_bytes(
		root,
		&["ls-tree", "-r", "--name-only", plan_commit, "plans"],
	)?;
	let text = String::from_utf8(tree)?;
	let mut pin_doc: Option<String> = None;
	let mut pin_path: Option<String> = None;
	let mut pin_sha: Option<String> = None;

	for rel in text.lines() {
		let is_record_doc = (rel.starts_with(&format!("plans/impl/{record}_"))
			|| rel.starts_with(&format!("plans/impl/{record}-"))
			|| rel.starts_with(&format!("plans/adr/{record}_"))
			|| rel.starts_with(&format!("plans/adr/{record}-")))
			&& rel.ends_with(".md");
		if !is_record_doc {
			continue;
		}
		let bytes = git_bytes(root, &["show", &format!("{plan_commit}:{rel}")])?;
		let doc = String::from_utf8(bytes)?;
		for raw_line in doc.lines() {
			let line = raw_line.trim();
			if let Some(val) = line.strip_prefix("Dormant proof:") {
				let cleaned = val.trim().trim_matches('`').trim();
				ensure!(
					pin_path.is_none(),
					"duplicate `Dormant proof:` pin in plan commit {plan_commit}"
				);
				pin_path = Some(cleaned.to_owned());
				if let Some(prev) = &pin_doc {
					ensure!(
						prev == rel,
						"dormant proof pin split across multiple plan documents"
					);
				} else {
					pin_doc = Some(rel.to_owned());
				}
			} else if let Some(val) = line.strip_prefix("Dormant proof sha256:") {
				let cleaned = val.trim().trim_matches('`').trim();
				ensure!(
					pin_sha.is_none(),
					"duplicate `Dormant proof sha256:` pin in plan commit {plan_commit}"
				);
				ensure!(
					is_hex(cleaned, 64),
					"malformed `Dormant proof sha256:` digest in {rel}"
				);
				pin_sha = Some(cleaned.to_owned());
				if let Some(prev) = &pin_doc {
					ensure!(
						prev == rel,
						"dormant proof pin split across multiple plan documents"
					);
				} else {
					pin_doc = Some(rel.to_owned());
				}
			}
		}
	}

	let doc = pin_doc.context("missing dormant proof pin in signed parent plan")?;
	let path = pin_path.context("missing `Dormant proof:` path in signed parent plan")?;
	let sha = pin_sha.context("missing `Dormant proof sha256:` digest in signed parent plan")?;
	ensure!(
		path == expected_path,
		"plan pin path `{path}` does not match record path `{expected_path}`"
	);
	Ok((doc, path, sha))
}

fn real_git_delta(
	root: &Path,
	baseline: &str,
	candidate: &str,
	proof_path: &str,
) -> Result<BTreeSet<DeltaRow>> {
	let raw = git_bytes(
		root,
		&[
			"diff",
			"--raw",
			"--no-renames",
			"--no-abbrev",
			"-z",
			baseline,
			candidate,
		],
	)?;
	let parts: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
	let mut rows = BTreeSet::new();
	let mut idx = 0usize;
	while idx < parts.len() {
		if parts[idx].is_empty() {
			idx += 1;
			continue;
		}
		ensure!(idx + 1 < parts.len(), "truncated git diff --raw -z record");
		let meta = std::str::from_utf8(parts[idx])?;
		let path = std::str::from_utf8(parts[idx + 1])?.to_owned();
		idx += 2;
		if path == proof_path || path.starts_with("plans/") {
			continue;
		}
		let meta = meta
			.strip_prefix(':')
			.context("malformed git diff --raw header")?;
		let fields: Vec<&str> = meta.split_whitespace().collect();
		ensure!(fields.len() == 5, "unexpected git diff --raw field count");
		let old_mode = (fields[0] != "000000").then(|| fields[0].to_owned());
		let new_mode = (fields[1] != "000000").then(|| fields[1].to_owned());
		let old_blob = (!fields[2].bytes().all(|b| b == b'0')).then(|| fields[2].to_owned());
		let new_blob = (!fields[3].bytes().all(|b| b == b'0')).then(|| fields[3].to_owned());
		rows.insert(DeltaRow {
			path,
			old_mode,
			new_mode,
			old_blob,
			new_blob,
		});
	}
	ensure!(
		!rows.is_empty(),
		"empty production delta between baseline and candidate"
	);
	Ok(rows)
}

fn candidate_blob_at(
	root: &Path,
	candidate: &str,
	path: &str,
	require_100644: bool,
) -> Result<String> {
	let out = git_bytes(root, &["ls-tree", "-z", candidate, "--", path])?;
	let entries: Vec<&[u8]> = out.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
	ensure!(
		entries.len() == 1,
		"path `{path}` missing or non-unique in candidate {candidate}"
	);
	let entry = std::str::from_utf8(entries[0])?;
	let (meta, found_path) = entry
		.split_once('\t')
		.context("malformed git ls-tree entry")?;
	ensure!(found_path == path, "git ls-tree path mismatch for `{path}`");
	let parts: Vec<&str> = meta.split_whitespace().collect();
	ensure!(parts.len() == 3, "malformed git ls-tree metadata");
	let (mode, kind, blob) = (parts[0], parts[1], parts[2]);
	ensure!(kind == "blob", "path `{path}` is not a git blob");
	if require_100644 {
		ensure!(
			mode == "100644",
			"proof file `{path}` must be a regular 100644 file, got mode {mode}"
		);
	} else {
		ensure!(
			mode == "100644" || mode == "100755",
			"context file `{path}` must be a regular file, got mode {mode}"
		);
	}
	Ok(blob.to_owned())
}

/// Verify the signed parent plan pin, signed candidate impl commit, exact
/// `tools/column/dormant/NNNN.json` proof bytes, full `--no-renames -z` delta
/// set equality, and context blob bindings.
fn verify_reviewed_proof_precheck(
	root: &Path,
	baseline_source: &str,
	baseline_publication: &str,
	candidate: &str,
) -> Result<VerifiedReviewedProof> {
	// 1. Candidate impl commit must be signed and carry `impl: adr NNNN ...`.
	git_bytes(root, &["verify-commit", candidate])
		.with_context(|| format!("candidate commit {candidate} failed signature verification"))?;
	let cand_subject =
		String::from_utf8(git_bytes(root, &["show", "-s", "--format=%s", candidate])?)?;
	let record = parse_record_subject(cand_subject.trim(), "impl")?;

	// 2. Candidate must have exactly one parent: the signed `plan: adr NNNN ...` commit.
	let parents_line = String::from_utf8(git_bytes(
		root,
		&["rev-list", "--parents", "-n", "1", candidate],
	)?)?;
	let parents: Vec<&str> = parents_line.split_whitespace().collect();
	ensure!(
		parents.len() == 2 && parents[0] == candidate,
		"candidate {candidate} must have exactly one parent plan commit"
	);
	let plan_commit = parents[1].to_owned();
	git_bytes(root, &["verify-commit", &plan_commit]).with_context(|| {
		format!("parent plan commit {plan_commit} failed signature verification")
	})?;
	let plan_subject = String::from_utf8(git_bytes(
		root,
		&["show", "-s", "--format=%s", &plan_commit],
	)?)?;
	let plan_record = parse_record_subject(plan_subject.trim(), "plan")?;
	ensure!(
		plan_record == record,
		"parent plan record {plan_record} disagrees with candidate impl record {record}"
	);

	// 3. Extract the reviewed pin `(plan_pin_doc, proof_path, pinned_sha256)` from the signed plan.
	let (plan_pin_doc, proof_path, pinned_sha256) = extract_plan_pin(root, &plan_commit, &record)?;

	// 4. Verify the proof file exists in candidate as a regular 100644 file and matches the pin.
	let _ = candidate_blob_at(root, candidate, &proof_path, true)?;
	let proof_bytes = git_bytes(root, &["show", &format!("{candidate}:{proof_path}")])?;
	let actual_sha256 = hex(&Sha256::digest(&proof_bytes));
	ensure!(
		actual_sha256 == pinned_sha256,
		"proof file `{proof_path}` sha256 {actual_sha256} disagrees with signed plan pin {pinned_sha256}"
	);

	// 5. Parse the proof JSON and verify baseline, delta set equality, and context blobs.
	let proof_json: Value = serde_json::from_slice(&proof_bytes)
		.with_context(|| format!("parse reviewed proof `{proof_path}`"))?;
	ensure!(proof_json["schema"] == 1, "unknown reviewed proof schema");
	ensure!(
		proof_json["record"].as_str() == Some(record.as_str()),
		"reviewed proof record disagrees with {record}"
	);
	ensure!(
		proof_json["kind"] == "representation_only",
		"reviewed proof kind must be representation_only"
	);
	ensure!(
		proof_json.get("candidate").is_none(),
		"reviewed proof must not name its own candidate commit"
	);
	ensure!(
		proof_json["baseline"].as_str() == Some(baseline_source),
		"reviewed proof baseline disagrees with measured baseline {baseline_source}"
	);
	let proof_pub = proof_json
		.get("baseline_publication")
		.and_then(Value::as_str)
		.unwrap_or(baseline_source);
	ensure!(
		proof_pub == baseline_publication,
		"reviewed proof baseline_publication disagrees with {baseline_publication}"
	);

	let real_delta = real_git_delta(root, baseline_source, candidate, &proof_path)?;
	let real_delta_by_path: BTreeMap<&str, &DeltaRow> =
		real_delta.iter().map(|r| (r.path.as_str(), r)).collect();

	let delta_arr = proof_json["delta"]
		.as_array()
		.context("reviewed proof missing delta array")?;
	let mut seen_delta_paths = BTreeSet::new();
	let mut proof_delta = BTreeSet::new();
	for row in delta_arr {
		let path = row["path"]
			.as_str()
			.context("delta row missing path")?
			.to_owned();
		ensure!(
			seen_delta_paths.insert(path.clone()),
			"duplicate path `{path}` in reviewed proof delta"
		);
		let opt_str = |key: &str| -> Result<Option<String>> {
			match row.get(key) {
				None | Some(Value::Null) => Ok(None),
				Some(Value::String(s)) => Ok(Some(s.clone())),
				_ => bail!(value_error(&format!(
					"invalid `{key}` in delta row `{path}`"
				))),
			}
		};
		let old_mode = opt_str("old_mode")?;
		let new_mode = opt_str("new_mode")?;
		let old_blob = opt_str("old_blob")?;
		let new_blob = opt_str("new_blob")?;
		proof_delta.insert(DeltaRow {
			path,
			old_mode,
			new_mode,
			old_blob,
			new_blob,
		});
	}
	ensure!(
		proof_delta == real_delta,
		"reviewed proof delta does not match exact git diff between {baseline_source} and {candidate}"
	);

	let context_arr = proof_json["context"]
		.as_array()
		.context("reviewed proof missing context array")?;
	let mut seen_ctx_paths = BTreeSet::new();
	for entry in context_arr {
		let path = entry["path"]
			.as_str()
			.context("context entry missing path")?;
		let listed_blob = entry["blob"]
			.as_str()
			.context("context entry missing blob")?;
		ensure!(
			seen_ctx_paths.insert(path.to_owned()),
			"duplicate path `{path}` in reviewed proof context"
		);
		let cand_blob = candidate_blob_at(root, candidate, path, false)?;
		if let Some(drow) = real_delta_by_path.get(path) {
			ensure!(
				drow.new_blob.as_deref() == Some(cand_blob.as_str()) && listed_blob == cand_blob,
				"context blob for changed file `{path}` disagrees with candidate / delta new_blob"
			);
		} else {
			ensure!(
				cand_blob == listed_blob,
				"context blob for `{path}` disagrees with candidate blob {cand_blob}"
			);
		}
	}

	Ok(VerifiedReviewedProof {
		record,
		plan_commit,
		plan_pin_doc,
		proof_path,
		proof_sha256: pinned_sha256,
		proof_json,
	})
}

fn verify_proof_claims(
	proof_json: &Value,
	checker_variants: &[String],
	checker_arms: &[Value],
) -> Result<()> {
	let mut proof_variants: Vec<String> = proof_json["added_variants"]
		.as_array()
		.context("reviewed proof missing added_variants")?
		.iter()
		.map(|v| v.as_str().map(str::to_owned).context("variant string"))
		.collect::<Result<Vec<_>>>()?;
	proof_variants.sort();
	proof_variants.dedup();
	let mut expected_variants = checker_variants.to_vec();
	expected_variants.sort();
	expected_variants.dedup();
	ensure!(
		!proof_variants.is_empty() && proof_variants == expected_variants,
		"reviewed proof added_variants disagree with structural checker"
	);

	let norm_arms = |arr: &[Value]| -> Result<Vec<(String, String)>> {
		let mut out = Vec::with_capacity(arr.len());
		for a in arr {
			let f = a
				.get("file")
				.or_else(|| a.get("path"))
				.and_then(Value::as_str)
				.context("arm missing file/path")?;
			let p = a["pattern"].as_str().context("arm missing pattern")?;
			out.push((f.to_owned(), p.to_owned()));
		}
		out.sort();
		Ok(out)
	};
	let proof_arms = norm_arms(
		proof_json["added_arms"]
			.as_array()
			.context("reviewed proof missing added_arms")?,
	)?;
	let expected_arms = norm_arms(checker_arms)?;
	ensure!(
		proof_arms == expected_arms,
		"reviewed proof added_arms disagree with structural checker"
	);
	Ok(())
}

struct StructuralCheckOutput {
	prod_files: Vec<Value>,
	diff_sha: String,
	variants: Vec<String>,
	arms: Vec<Value>,
}

fn run_structural_checker(
	root: &Path,
	baseline: &str,
	candidate: &str,
	proof_path: &str,
	guard: &Path,
) -> Result<StructuralCheckOutput> {
	// Fix 15: --no-renames -z so moving a production file under plans/ still lists
	// the deleted production source path and fails closed.
	let diff_out = git_bytes(
		root,
		&[
			"diff",
			"--no-renames",
			"--name-only",
			"-z",
			baseline,
			candidate,
		],
	)?;
	let all_paths: Vec<String> = diff_out
		.split(|b| *b == 0)
		.filter(|s| !s.is_empty())
		.map(|s| String::from_utf8(s.to_vec()))
		.collect::<std::result::Result<Vec<_>, _>>()?
		.into_iter()
		.filter(|rel| rel != proof_path)
		.collect();
	ensure!(
		!all_paths.is_empty(),
		"empty diff between baseline and candidate"
	);

	let temp = std::env::temp_dir().join(format!(
		"rnx-dormant-structural-{}",
		crate::receipt::invocation_id()
	));
	std::fs::create_dir(&temp)?;
	let run = (|| -> Result<_> {
		std::fs::write(temp.join("files.txt"), all_paths.join("\n") + "\n")?;
		let mut prod_files = Vec::new();
		let mut mod_dirs = BTreeSet::new();
		for (idx, rel) in all_paths.iter().enumerate() {
			if rel.starts_with("plans/") {
				continue;
			}
			let base_bytes = git_bytes(root, &["show", &format!("{baseline}:{rel}")])
				.with_context(|| {
					format!("missing baseline file {rel} (new production files are not structural)")
				})?;
			let cand_bytes = git_bytes(root, &["show", &format!("{candidate}:{rel}")])
				.with_context(|| {
					format!(
						"missing candidate file {rel} (deleted production files are not structural)"
					)
				})?;
			let old_sha = hex(&Sha256::digest(&base_bytes));
			let new_sha = hex(&Sha256::digest(&cand_bytes));
			if rel.ends_with(".rs") {
				std::fs::write(temp.join(format!("base-{idx}.rs")), &base_bytes)?;
				std::fs::write(temp.join(format!("cand-{idx}.rs")), &cand_bytes)?;
				for d in ancestor_module_dirs(rel) {
					mod_dirs.insert(d);
				}
			}
			prod_files.push(json!({
				"path": rel,
				"old_sha256": old_sha,
				"new_sha256": new_sha,
			}));
		}
		let mut parent_candidates = BTreeSet::new();
		for d in mod_dirs {
			if let Ok(list) = git_bytes(
				root,
				&["ls-tree", "--name-only", candidate, &format!("{d}/")],
			) {
				for entry in String::from_utf8_lossy(&list).lines() {
					if entry.ends_with(".rs") {
						parent_candidates.insert(entry.to_owned());
					}
				}
			}
			// Fix 9: also collect binary crate roots under `src/bin/**/*.rs` so a
			// module declared in `src/lib.rs` with `#[cfg(test)]` cannot hide a
			// non-test `mod <name>;` in a binary target of the same crate.
			if (d == "src" || d.ends_with("/src"))
				&& let Ok(list) = git_bytes(
					root,
					&[
						"ls-tree",
						"-r",
						"--name-only",
						candidate,
						&format!("{d}/bin/"),
					],
				) {
				for entry in String::from_utf8_lossy(&list).lines() {
					if entry.ends_with(".rs") {
						parent_candidates.insert(entry.to_owned());
					}
				}
			}
		}
		let mut parent_list = Vec::new();
		for parent in parent_candidates {
			if let Ok(bytes) = git_bytes(root, &["show", &format!("{candidate}:{parent}")]) {
				let idx = parent_list.len();
				std::fs::write(temp.join(format!("parent-{idx}.rs")), &bytes)?;
				parent_list.push(parent);
			}
		}
		if !parent_list.is_empty() {
			std::fs::write(temp.join("parents.txt"), parent_list.join("\n") + "\n")?;
		}
		prod_files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
		let diff_sha = identity(&Value::Array(prod_files.clone()))?;

		let output = Command::new(guard)
			.arg("--dormant-structural")
			.arg(&temp)
			.output()
			.with_context(|| format!("invoke {}", guard.display()))?;
		if !output.status.success() {
			bail!(value_error(&format!(
				"structural dormant proof rejected: {}",
				String::from_utf8_lossy(&output.stderr).trim()
			)));
		}
		let stdout = String::from_utf8(output.stdout)?;
		let mut lines = stdout.lines();
		ensure!(
			lines.next() == Some("PASS"),
			"unexpected structural checker output"
		);
		let mut variants = Vec::new();
		let mut arms = Vec::new();
		for line in lines {
			let parts: Vec<&str> = line.split('\t').collect();
			match parts.as_slice() {
				["VARIANT", v] => variants.push((*v).to_owned()),
				["ARM", file, pat] => arms.push(json!({"file": file, "pattern": pat})),
				_ => bail!(value_error("malformed structural checker line")),
			}
		}
		Ok(StructuralCheckOutput {
			prod_files,
			diff_sha,
			variants,
			arms,
		})
	})();
	let _ = std::fs::remove_dir_all(&temp);
	run
}

/// Produce a sealed structural dormant-feature witness for a representation-only
/// production delta between the measured `baseline_source` (`base_capture["revision"]`)
/// and `candidate`. When the baseline was reused from a docs-only publication
/// (`baseline_publication != baseline_source`), verifies docs-only succession (Fix 4).
#[allow(clippy::too_many_arguments)]
pub fn structural_witness(
	root: &Path,
	baseline_publication: &str,
	candidate: &str,
	guard: &Path,
	base_capture: &Value,
	cand_capture: &Value,
	policy: &Value,
	affected_rows: &[Value],
) -> Result<Value> {
	let baseline_source = base_capture["revision"]
		.as_str()
		.context("missing baseline capture revision")?;
	if let Some(pub_rev) = base_capture
		.get("publication_commit")
		.and_then(Value::as_str)
	{
		ensure!(
			pub_rev == baseline_publication,
			"baseline capture publication_commit disagrees with baseline_publication"
		);
	}
	ensure!(
		cand_capture["revision"] == candidate,
		"candidate capture revision does not match measured candidate"
	);
	verify_baseline_succession(root, baseline_source, baseline_publication)?;
	let reviewed =
		verify_reviewed_proof_precheck(root, baseline_source, baseline_publication, candidate)?;
	let StructuralCheckOutput {
		prod_files,
		diff_sha,
		variants,
		arms,
	} = run_structural_checker(
		root,
		baseline_source,
		candidate,
		&reviewed.proof_path,
		guard,
	)?;
	verify_proof_claims(&reviewed.proof_json, &variants, &arms)?;
	let binding = matrix_binding(base_capture, cand_capture, policy)?;
	let guard_src_sha = guard_source_sha256(root, candidate)?;
	seal_witness(json!({
		"schema": 1,
		"kind": "representation_only",
		"record": reviewed.record,
		"baseline": baseline_source,
		"baseline_publication": baseline_publication,
		"candidate": candidate,
		"plan_commit": reviewed.plan_commit,
		"plan_pin_doc": reviewed.plan_pin_doc,
		"proof_path": reviewed.proof_path,
		"proof_sha256": reviewed.proof_sha256,
		"production_diff_sha256": diff_sha,
		"production_files": prod_files,
		"added_variants": variants,
		"added_arms": arms,
		"guard_sha256": digest(guard)?,
		"guard_source_sha256": guard_src_sha,
		"matrix_binding": binding,
		"affected_rows": affected_rows,
	}))
}

/// Verify a representation-only dormant-feature witness by checking its digest,
/// bindings, baseline publication succession (Fix 4), checker source provenance
/// (Fix 6), AND re-running the structural AST checker on the git trees of the
/// bound `(baseline, candidate)` pair.
pub fn verify_witness(
	witness: &Value,
	base_capture: &Value,
	cand_capture: &Value,
	policy: &Value,
	affected_rows: &[Value],
	root: &Path,
	guard: &Path,
) -> Result<()> {
	ensure!(witness["schema"] == 1, "unknown dormant witness schema");
	ensure!(
		witness["kind"] == "representation_only",
		"unsupported dormant witness kind (only representation_only is implemented)"
	);
	let recorded_sha = witness["witness_sha256"]
		.as_str()
		.context("missing witness_sha256")?;
	ensure!(
		recorded_sha == witness_digest(witness)?,
		"dormant witness digest mismatch (tampered witness)"
	);
	let baseline = witness["baseline"].as_str().context("witness baseline")?;
	let baseline_pub = witness["baseline_publication"]
		.as_str()
		.context("witness baseline_publication")?;
	let candidate = witness["candidate"].as_str().context("witness candidate")?;
	ensure!(
		base_capture["revision"] == baseline && cand_capture["revision"] == candidate,
		"stale dormant witness: revisions do not match measured baseline/candidate"
	);
	if let Some(cap_pub) = base_capture
		.get("publication_commit")
		.and_then(Value::as_str)
	{
		ensure!(
			cap_pub == baseline_pub,
			"dormant witness baseline_publication disagrees with baseline capture publication_commit"
		);
	}
	verify_baseline_succession(root, baseline, baseline_pub)?;
	let reviewed = verify_reviewed_proof_precheck(root, baseline, baseline_pub, candidate)?;
	ensure!(
		witness["record"] == reviewed.record
			&& witness["plan_commit"] == reviewed.plan_commit
			&& witness["plan_pin_doc"] == reviewed.plan_pin_doc
			&& witness["proof_path"] == reviewed.proof_path
			&& witness["proof_sha256"] == reviewed.proof_sha256,
		"witness reviewed-proof pin metadata disagrees with signed plan/proof"
	);
	ensure!(
		witness["affected_rows"] == Value::Array(affected_rows.to_vec()),
		"dormant witness affected_rows differ from failing per-record rows"
	);
	let expected_binding = matrix_binding(base_capture, cand_capture, policy)?;
	ensure!(
		witness["matrix_binding"] == expected_binding,
		"dormant witness matrix_binding does not match measured captures"
	);
	let files = witness["production_files"]
		.as_array()
		.context("missing production_files")?;
	ensure!(!files.is_empty(), "empty production_files");
	ensure!(
		witness["production_diff_sha256"] == identity(&Value::Array(files.clone()))?,
		"production_diff_sha256 mismatch"
	);
	let variants = witness["added_variants"]
		.as_array()
		.context("missing added_variants")?;
	ensure!(!variants.is_empty(), "empty added_variants");
	let expected_guard_src = guard_source_sha256(root, candidate)?;
	ensure!(
		witness["guard_source_sha256"] == expected_guard_src,
		"guard_source_sha256 disagrees with ci/execute_guard at candidate {candidate}"
	);
	let StructuralCheckOutput {
		prod_files,
		diff_sha,
		variants: added_variants,
		arms: added_arms,
	} = run_structural_checker(root, baseline, candidate, &reviewed.proof_path, guard)?;
	verify_proof_claims(&reviewed.proof_json, &added_variants, &added_arms)?;
	ensure!(
		witness["production_diff_sha256"] == diff_sha
			&& witness["production_files"] == Value::Array(prod_files)
			&& witness["added_variants"] == json!(added_variants)
			&& witness["added_arms"] == Value::Array(added_arms),
		"structural replay on git trees disagrees with witness"
	);
	Ok(())
}

/// Score the fixed-route gate, preserving raw `per_record` and `cumulative` rows
/// unchanged and applying ADR 0355's `accepted_layout_drift` verdict only when
/// every non-execution check and every cumulative row pass and a verified
/// representation-only structural witness is present.
pub fn score_with_witness(
	baseline: &Value,
	candidate: &Value,
	anchor: Option<&Value>,
	policy: &Value,
	witness: Option<&Value>,
	root: &Path,
	guard: &Path,
) -> Result<Value> {
	let mut scored = super::score(baseline, candidate, anchor, policy)?;
	if scored["status"] == "PASS" {
		return Ok(scored);
	}
	let per_record = scored["per_record"]
		.as_array()
		.context("missing per_record")?;
	let cumulative = scored["cumulative"]
		.as_array()
		.context("missing cumulative")?;
	let all_cumulative_pass = cumulative.iter().all(|r| r["pass_all"] == true);
	let failing: Vec<&Value> = per_record
		.iter()
		.filter(|r| r["pass_all"] != true)
		.collect();
	let all_exec_only =
		!failing.is_empty() && failing.iter().all(|r| row_execution_only_failure(r));
	if all_cumulative_pass
		&& all_exec_only
		&& let Some(w) = witness
	{
		let affected = failing_row_ids(per_record);
		if verify_witness(w, baseline, candidate, policy, &affected, root, guard).is_ok() {
			scored["status"] = json!("PASS");
			scored["effective_verdict"] = json!(VERDICT_ACCEPTED_DRIFT);
			scored["layout_drift"] = w.clone();
			return Ok(scored);
		}
	}
	Ok(scored)
}

/// Replay and revalidate a retained `fixed-route/RESULT.json` without needing
/// raw captures or deleted payloads (Fix 5 & Fix 6):
/// - Re-runs `fixed_route::score` on the retained `baseline`, `candidate`,
///   adjacent `ANCHOR-EVIDENCE.json`, and `policy`, asserting exact equality of
///   the full `per_record` and `cumulative` arrays (catches dropped rows and
///   edited allowances).
/// - For `accepted_layout_drift`, builds `ci/execute_guard` from the bound
///   `candidate`'s git tree (when `guard` is `None`) and replays the structural
///   AST proof on the git trees of `(baseline, candidate)`.
pub fn revalidate_receipt(
	gate: &Value,
	gate_dir: Option<&Path>,
	root: Option<&Path>,
	guard: Option<&Path>,
) -> Result<Value> {
	let policy = gate.get("policy").context("gate receipt missing policy")?;
	let baseline = gate
		.get("baseline")
		.context("gate receipt missing baseline")?;
	let candidate = gate
		.get("candidate")
		.context("gate receipt missing candidate")?;

	let anchor_path = gate_dir
		.map(|d| d.join("ANCHOR-EVIDENCE.json"))
		.filter(|p| p.is_file())
		.or_else(|| gate["anchor"].as_str().map(PathBuf::from))
		.context("gate receipt missing ANCHOR-EVIDENCE.json path")?;
	let anchor: Value = serde_json::from_slice(
		&std::fs::read(&anchor_path).with_context(|| format!("read {}", anchor_path.display()))?,
	)?;

	let rescored = super::score(baseline, candidate, Some(&anchor), policy)?;
	ensure!(
		rescored["per_record"] == gate["per_record"]
			&& rescored["cumulative"] == gate["cumulative"],
		"rescored per_record/cumulative arrays disagree with retained gate receipt"
	);

	let per_record = rescored["per_record"]
		.as_array()
		.context("rescored per_record")?;
	let cumulative = rescored["cumulative"]
		.as_array()
		.context("rescored cumulative")?;
	let raw_per_pass = per_record.iter().all(|r| r["pass_all"] == true);
	let raw_cum_pass = cumulative.iter().all(|r| r["pass_all"] == true);
	let verdict = gate
		.get("effective_verdict")
		.and_then(Value::as_str)
		.unwrap_or(if gate["status"] == "PASS" {
			VERDICT_PASS
		} else {
			VERDICT_FAIL
		});

	match verdict {
		VERDICT_PASS => {
			ensure!(
				gate["status"] == "PASS" && raw_per_pass && raw_cum_pass,
				"ordinary pass verdict disagrees with rescored rows"
			);
			Ok(json!({
				"status": "PASS",
				"effective_verdict": VERDICT_PASS,
				"raw_per_record_passed": per_record.len(),
				"raw_per_record_total": per_record.len(),
				"raw_cumulative_passed": cumulative.len(),
				"raw_cumulative_total": cumulative.len(),
			}))
		}
		VERDICT_ACCEPTED_DRIFT => {
			ensure!(
				gate["status"] == "PASS",
				"accepted_layout_drift must have status PASS"
			);
			ensure!(
				raw_cum_pass,
				"accepted_layout_drift refused: cumulative band failed"
			);
			let failing: Vec<&Value> = per_record
				.iter()
				.filter(|r| r["pass_all"] != true)
				.collect();
			ensure!(
				!failing.is_empty(),
				"accepted_layout_drift without any failing per-record row"
			);
			ensure!(
				failing.iter().all(|r| row_execution_only_failure(r)),
				"accepted_layout_drift refused: non-execution failure on failing per-record row"
			);
			let witness = gate
				.get("layout_drift")
				.context("accepted_layout_drift missing layout_drift witness")?;
			let root =
				root.context("revalidating accepted_layout_drift requires repository root")?;
			let cand_rev = candidate["revision"]
				.as_str()
				.context("candidate capture missing revision")?;
			let built;
			let guard_path = match guard {
				Some(g) => g,
				None => {
					built = build_guard_from_revision(root, cand_rev)?;
					&built.binary
				}
			};
			let affected = failing_row_ids(per_record);
			verify_witness(
				witness, baseline, candidate, policy, &affected, root, guard_path,
			)?;
			let mut summary = Map::new();
			summary.insert("status".into(), json!("PASS"));
			summary.insert("effective_verdict".into(), json!(VERDICT_ACCEPTED_DRIFT));
			summary.insert(
				"raw_per_record_passed".into(),
				json!(per_record.len() - failing.len()),
			);
			summary.insert("raw_per_record_total".into(), json!(per_record.len()));
			summary.insert("raw_cumulative_passed".into(), json!(cumulative.len()));
			summary.insert("raw_cumulative_total".into(), json!(cumulative.len()));
			summary.insert("witness_sha256".into(), witness["witness_sha256"].clone());
			summary.insert(
				"baseline_publication".into(),
				witness["baseline_publication"].clone(),
			);
			summary.insert("candidate".into(), witness["candidate"].clone());
			Ok(Value::Object(summary))
		}
		_ => bail!(value_error("gate receipt verdict is FAIL")),
	}
}
