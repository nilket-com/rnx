//! The fixed-route measurement policy (ADR 0296), as `drift_band.py` and
//! `anchor_cache.py` keep it: the policy's validation, the anchor's
//! approved lineage, the unchanged-fixed-source condition, the integer
//! per-mille bands and the score, and the one owned anchor bundle under
//! its lock. Refusals, receipts, the ownership journal and the bundle
//! keep their shapes, so either implementation reads what the other wrote.
use crate::child::{raise, value_error};
use crate::pyjson::identity;
use crate::receipt::{atomic_json, digest, sync_directory};
use crate::stage::called_process_error;
use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

#[path = "fixed_route_phases.rs"]
mod phases;
pub(crate) use phases::validate_setup_context;

/// The policy, under the checkout.
pub const POLICY: &str = "tools/column/fixed_route_policy.json";
pub const INITIAL_ANCHOR: &str = "3fe44bf9245c780cf784554293e0a1b1e7c00c26";
/// `old_path_programs.KERNELS`.
pub const KERNELS: [&str; 8] = [
	"fib",
	"loops",
	"numeric_calls",
	"shared_rows",
	"awfy_sieve",
	"awfy_permute",
	"vec_iter_i64",
	"vec_iter_bool",
];

/// `value[key]`, or Python's `KeyError`.
fn at<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
	value
		.get(key)
		.ok_or_else(|| raise("KeyError", &format!("'{key}'")))
}

/// Python's truth value of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
	match value {
		None | Some(Value::Null) => false,
		Some(Value::Bool(b)) => *b,
		Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
		Some(Value::String(s)) => !s.is_empty(),
		Some(Value::Array(a)) => !a.is_empty(),
		Some(Value::Object(o)) => !o.is_empty(),
	}
}

/// Python's type name for a JSON value.
fn py_type(value: &Value) -> &'static str {
	match value {
		Value::Null => "NoneType",
		Value::Bool(_) => "bool",
		Value::Number(n) if n.is_f64() => "float",
		Value::Number(_) => "int",
		Value::String(_) => "str",
		Value::Array(_) => "list",
		Value::Object(_) => "dict",
	}
}

/// Python's iteration of a JSON value: a list's items, a string's
/// characters, a dict's keys; anything else is not iterable.
fn iterate(value: &Value) -> Result<Vec<Value>> {
	match value {
		Value::Array(a) => Ok(a.clone()),
		Value::String(s) => Ok(s.chars().map(|c| json!(c.to_string())).collect()),
		Value::Object(o) => Ok(o.keys().map(|k| json!(k)).collect()),
		other => Err(raise(
			"TypeError",
			&format!("'{}' object is not iterable", py_type(other)),
		)),
	}
}

/// Python's `set(value)`, as comparable JSON text.
fn key_set(value: &Value) -> Result<BTreeSet<String>> {
	Ok(iterate(value)?.iter().map(Value::to_string).collect())
}

pub fn validate_policy(policy: &Value) -> Result<()> {
	let kernels: BTreeSet<String> = match at(policy, "kernels")? {
		Value::Array(a) => a.iter().map(|v| v.to_string()).collect(),
		_ => BTreeSet::new(),
	};
	let expected: BTreeSet<String> = KERNELS.iter().map(|k| json!(k).to_string()).collect();
	let version = at(policy, "schema")?;
	let reviewed = (*version == 1 && policy.get("native_setup").is_none())
		|| (*version == 2
			&& policy
				.get("native_setup")
				.map(identity)
				.transpose()?
				.as_deref() == Some(phases::TABLE_DIGEST));
	if let Some(installed) = policy.get("installed_setup") {
		anyhow::ensure!(
			*version == 2
				&& identity(installed)?
					== "4e42594e994ad239a0494ca2c3b5b8297ec5b7346ed3f47386b88afa9fa4c34f",
			"unreviewed installed setup reference"
		);
	}
	if !reviewed || kernels != expected {
		bail!(value_error(
			"policy schema/kernel set changed: review required"
		));
	}
	if *at(policy, "modes")? != json!(["off", "on"]) || *at(policy, "sizes")? != json!([1, 5]) {
		bail!(value_error("policy matrix changed: review required"));
	}
	if *at(policy, "per_record_per_mille")? != json!(5)
		|| *at(policy, "cumulative_per_mille")? != json!(10)
	{
		bail!(value_error("policy limits changed: review required"));
	}
	// As Python iterates the inventory: a string by its characters, a
	// dict by its keys, so only a list of selector dicts can pass.
	let items = at(policy, "inventory")?;
	if !truthy(Some(items)) {
		bail!(value_error("empty/duplicate source inventory"));
	}
	let list = iterate(items)?;
	let distinct: BTreeSet<String> = list.iter().map(identity).collect::<Result<_>>()?;
	if distinct.len() != list.len() {
		bail!(value_error("empty/duplicate source inventory"));
	}
	let selector: BTreeSet<String> = ["file", "kind", "receiver", "name"]
		.map(|k| json!(k).to_string())
		.into();
	for item in &list {
		if key_set(item)? != selector {
			bail!(value_error("invalid source selector"));
		}
		let Value::Object(fields) = item else {
			bail!(raise(
				"TypeError",
				&format!(
					"{} indices must be integers or slices, not str",
					py_type(item)
				),
			));
		};
		if !matches!(fields["kind"].as_str(), Some("struct" | "method" | "fn")) {
			bail!(value_error("invalid source selector"));
		}
		let Some(file) = fields["file"].as_str() else {
			bail!(raise(
				"TypeError",
				&format!(
					"argument should be a str or an os.PathLike object where __fspath__ returns a str, not '{}'",
					py_type(&fields["file"])
				),
			));
		};
		let file = Path::new(file);
		if file.is_absolute() || file.components().any(|c| c == Component::ParentDir) {
			bail!(value_error("source outside repository"));
		}
	}
	Ok(())
}

/// The anchor's link from the baseline's, validated before its proof:
/// None when the anchor is unchanged, else its structured approval.
pub fn anchor_transition(policy: &Value, previous: Option<&Value>) -> Result<Option<Value>> {
	let old = match previous {
		None => json!(INITIAL_ANCHOR),
		Some(p) => at(p, "anchor")?.clone(),
	};
	let new = at(policy, "anchor")?;
	let approval = at(policy, "anchor_transition")?;
	if *new == old {
		match previous {
			None if !approval.is_null() => bail!(value_error("unexpected bootstrap transition")),
			Some(p) if approval != at(p, "anchor_transition")? => {
				bail!(value_error("unchanged anchor changed its approval history"))
			}
			_ => return Ok(None),
		}
	}
	let structured: BTreeSet<String> = ["old", "new", "plan", "path"]
		.map(|k| json!(k).to_string())
		.into();
	if !approval.is_object() || key_set(approval)? != structured {
		bail!(value_error("structured approval required"));
	}
	if approval["old"] != old || approval["new"] != *new {
		bail!(value_error("anchor transition skips baseline lineage"));
	}
	Ok(Some(approval.clone()))
}

/// `git -C root ARGS`: its standard output, or `CalledProcessError`.
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
	let mut command = vec![
		"git".to_string(),
		"-C".into(),
		root.to_string_lossy().into_owned(),
	];
	command.extend(args.iter().map(|a| a.to_string()));
	let output = crate::child::git_command()
		.arg("-C")
		.arg(root)
		.args(args)
		.stderr(std::process::Stdio::null())
		.output()
		.with_context(|| format!("spawn git {}", args.first().copied().unwrap_or_default()))?;
	if !output.status.success() {
		bail!(called_process_error(
			&command,
			output.status.code().unwrap_or(-1)
		));
	}
	Ok(output.stdout)
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
	Ok(String::from_utf8(git(root, args)?)?)
}

/// The baseline's policy, when the baseline has one.
fn baseline_policy(root: &Path, baseline: &str) -> Result<Option<Value>> {
	let present = git_text(root, &["ls-tree", "--name-only", baseline, POLICY])?;
	if present.lines().next().is_none() {
		return Ok(None);
	}
	Ok(Some(serde_json::from_slice(&git(
		root,
		&["show", &format!("{baseline}:{POLICY}")],
	)?)?))
}

/// A changed anchor's proof: a full anchor commit, the baseline's policy
/// (or the reviewed bootstrap), and an approval whose plan commit is an
/// ancestor of the candidate, a signed `plan: adr` commit, and declares
/// the transition.
pub fn anchor_approval(root: &Path, baseline: &str, candidate: &str, policy: &Value) -> Result<()> {
	let anchor = at(policy, "anchor")?.as_str().unwrap_or_default();
	let resolved = git_text(root, &["rev-parse", &format!("{anchor}^{{commit}}")]);
	if anchor.len() != 40 || resolved?.trim() != anchor {
		bail!(value_error("anchor must be a full commit"));
	}
	let previous = baseline_policy(root, baseline)?;
	if previous.is_none() && baseline != INITIAL_ANCHOR {
		bail!(value_error("unreviewed anchor bootstrap"));
	}
	let Some(approval) = anchor_transition(policy, previous.as_ref())? else {
		return Ok(());
	};
	let plan = approval["plan"].as_str().unwrap_or_default();
	if plan.len() != 40 {
		bail!(value_error("approval plan must be full commit"));
	}
	git(root, &["merge-base", "--is-ancestor", plan, candidate])?;
	if !git_text(root, &["show", "-s", "--format=%s", plan])?.starts_with("plan: adr ") {
		bail!(value_error("approval is not a plan commit"));
	}
	git(root, &["verify-commit", plan])?;
	let path = approval["path"].as_str().unwrap_or_default();
	let text = git_text(root, &["show", &format!("{plan}:{path}")])?;
	let old = approval["old"].as_str().unwrap_or_default();
	for declaration in [
		"User-approved anchor transition".to_string(),
		format!("Old anchor: {old}"),
		format!("New anchor: {anchor}"),
	] {
		if !text.contains(&declaration) {
			bail!(value_error("approval declaration absent"));
		}
	}
	Ok(())
}

/// Every fixed-source item's canonical text, from `guard`, unchanged
/// between baseline and candidate; the policy itself unchanged in its
/// inventory and limits, or at the reviewed bootstrap.
pub fn source_condition(
	root: &Path,
	baseline: &str,
	candidate: &str,
	policy: &Value,
	guard: &Path,
) -> Result<Value> {
	validate_policy(policy)?;
	match baseline_policy(root, baseline)? {
		Some(old) => {
			if old["schema"] == 2 && policy["schema"] != 2 {
				bail!(value_error("phase policy downgrade"));
			}
			for field in [
				"inventory",
				"kernels",
				"modes",
				"sizes",
				"per_record_per_mille",
				"cumulative_per_mille",
			] {
				if at(&old, field)? != at(policy, field)? {
					bail!(value_error(&format!(
						"policy inventory/limit change: STOP {field}"
					)));
				}
			}
		}
		None if baseline != INITIAL_ANCHOR => bail!(value_error(
			"missing baseline policy outside reviewed bootstrap"
		)),
		None => {}
	}
	let temp =
		std::env::temp_dir().join(format!("fixed-source-{}", crate::receipt::invocation_id()));
	std::fs::create_dir(&temp)?;
	let rows = (|| -> Result<Vec<Value>> {
		let mut rows = Vec::new();
		for item in at(policy, "inventory")?.as_array().into_iter().flatten() {
			// Every guard argument is a string, as subprocess requires:
			// never an empty one standing in for a malformed value.
			let field = |k: &str| item[k].as_str().unwrap_or_default().to_owned();
			for k in ["kind", "receiver", "name"] {
				if !item[k].is_string() {
					bail!(raise(
						"TypeError",
						&format!(
							"expected str, bytes or os.PathLike object, not {}",
							py_type(&item[k])
						),
					));
				}
			}
			let mut values = Vec::new();
			for rev in [baseline, candidate] {
				let blob = git(root, &["show", &format!("{rev}:{}", field("file"))])?;
				let path = temp.join("source.rs");
				std::fs::write(&path, blob)?;
				let mut command = vec![
					guard.to_string_lossy().into_owned(),
					"--item".into(),
					path.to_string_lossy().into_owned(),
				];
				command.extend([field("kind"), field("receiver"), field("name")]);
				let output = std::process::Command::new(guard)
					.args(&command[1..])
					.stderr(std::process::Stdio::inherit())
					.output()?;
				if !output.status.success() {
					bail!(called_process_error(
						&command,
						output.status.code().unwrap_or(-1)
					));
				}
				values.push(String::from_utf8(output.stdout)?.trim().to_owned());
			}
			if values[0] != values[1] {
				bail!(value_error(&format!(
					"fixed source changed: {}:{}",
					field("file"),
					field("name")
				)));
			}
			rows.push(json!({"selector": item, "sha256": crate::pyjson::hex(&Sha256::digest(values[0].as_bytes())), "canonical": values[0]}));
		}
		Ok(rows)
	})();
	let _ = std::fs::remove_dir_all(&temp);
	Ok(
		json!({"status": "PASS", "baseline": baseline, "candidate": candidate,
		"inventory": identity(at(policy, "inventory")?)?, "items": rows?}),
	)
}

/// A nonnegative integer count, never a float or a boolean.
fn count(value: &Value) -> Option<i128> {
	match value {
		Value::Number(n) if n.is_i64() || n.is_u64() => n
			.as_i64()
			.map(i128::from)
			.or_else(|| n.as_u64().map(i128::from))
			.filter(|v| *v >= 0),
		_ => None,
	}
}

/// One measured matrix, by (kind, mode, size): every case once, passed,
/// with reconciling counts and its output and fixture identities.
fn index(records: &Value, policy: &Value) -> Result<BTreeMap<String, Value>> {
	let mut result = BTreeMap::new();
	for row in records.as_array().into_iter().flatten() {
		let key = json!([at(row, "kind")?, at(row, "policy")?, at(row, "n")?]).to_string();
		if result.contains_key(&key) || *at(row, "status")? != json!("PASS") {
			bail!(value_error("duplicate/failed case"));
		}
		let a = at(row, "accounting")?;
		let mut counts = [0i128; 4];
		for (slot, k) in counts
			.iter_mut()
			.zip(["rnx_Ir", "other_Ir", "dynamic_Ir", "total_Ir"])
		{
			*slot = count(at(a, k)?).ok_or_else(|| value_error("invalid count"))?;
		}
		if counts[3] != counts[0] + counts[1] + counts[2] {
			bail!(value_error("partitions do not reconcile"));
		}
		if !truthy(row.get("stdout_sha256")) || !truthy(row.get("fixture_sha256")) {
			bail!(value_error("missing output/fixture"));
		}
		if policy["schema"] == 2 {
			phases::validate_row(row, policy)?;
		}
		result.insert(key, row.clone());
	}
	let mut expected = BTreeSet::new();
	for k in at(policy, "kernels")?.as_array().into_iter().flatten() {
		for m in at(policy, "modes")?.as_array().into_iter().flatten() {
			for n in at(policy, "sizes")?.as_array().into_iter().flatten() {
				expected.insert(json!([k, m, n]).to_string());
			}
		}
	}
	if result.keys().cloned().collect::<BTreeSet<_>>() != expected {
		bail!(value_error("incomplete or extraneous case matrix"));
	}
	Ok(result)
}

pub(crate) fn validate_capture_matrix(records: &Value, policy: &Value) -> Result<()> {
	index(records, policy).map(|_| ())
}

/// The level and growth bands, in per mille, per kernel, mode and metric
/// (W: rnx and other; T: the raw total), in exact integer arithmetic.
pub fn compare(left: &Value, right: &Value, policy: &Value, limit: i128) -> Result<Vec<Value>> {
	let indices = [index(left, policy)?, index(right, policy)?];
	let mut rows = Vec::new();
	let sizes: Vec<&Value> = at(policy, "sizes")?
		.as_array()
		.into_iter()
		.flatten()
		.collect();
	for kind in at(policy, "kernels")?.as_array().into_iter().flatten() {
		for mode in at(policy, "modes")?.as_array().into_iter().flatten() {
			let case = |i: usize, n: &Value| -> &Value {
				&indices[i][&json!([kind, mode, n]).to_string()]
			};
			for n in &sizes {
				let (a, b) = (case(0, n), case(1, n));
				if a["stdout_sha256"] != b["stdout_sha256"]
					|| a["fixture_sha256"] != b["fixture_sha256"]
				{
					bail!(value_error("output/fixture mismatch"));
				}
			}
			for metric in ["W", "T"] {
				let measure = |i: usize, n: i64| -> i128 {
					let a = &case(i, &json!(n))["accounting"];
					let c = |k: &str| count(&a[k]).unwrap_or_default();
					if metric == "T" {
						c("total_Ir")
					} else {
						c("rnx_Ir") + c("other_Ir")
					}
				};
				let (b1, b5, c1, c5) = (measure(0, 1), measure(0, 5), measure(1, 1), measure(1, 5));
				if b5 <= 0 || b5 <= b1 {
					bail!(value_error("invalid baseline denominator"));
				}
				let level = 1000 * (c5 - b5) <= limit * b5;
				let growth = 1000 * ((c5 - c1) - (b5 - b1)) <= limit * (b5 - b1);
				let n = |v: i128| -> Value {
					serde_json::from_str(&v.to_string()).unwrap_or(Value::Null)
				};
				let mut row = json!({"kind": kind, "mode": mode, "metric": metric,
					"B1": n(b1), "B5": n(b5), "C1": n(c1), "C5": n(c5),
					"level_pass": level, "growth_pass": growth, "pass_all": level && growth});
				if policy["schema"] == 2 && mode == "on" {
					phases::score_row(
						&mut row,
						[
							[case(0, sizes[0]), case(0, sizes[1])],
							[case(1, sizes[0]), case(1, sizes[1])],
						],
						policy,
						limit,
					)?;
				}
				if mode == "off" && case(1, sizes[0]).get("dispatch").is_some() {
					phases::score_off_dispatch(
						&mut row,
						[case(0, sizes[0]), case(0, sizes[1])],
						[case(1, sizes[0]), case(1, sizes[1])],
						policy,
						limit,
					)?;
				}
				rows.push(row);
			}
		}
	}
	Ok(rows)
}

/// What determines counts once the rebuilt bytes match.
pub fn measurement_recipe(current: &Value) -> Result<Value> {
	let mut out = Map::new();
	for k in [
		"cpu",
		"environment",
		"run_environment",
		"valgrind",
		"fixtures",
		"policy",
	] {
		out.insert(k.into(), at(current, k)?.clone());
	}
	let tooling: Map<String, Value> = at(current, "tooling")?
		.as_object()
		.context("tooling is not an object")?
		.iter()
		.filter(|(k, _)| *k != "tools/column/paired_build.py")
		.map(|(k, v)| (k.clone(), v.clone()))
		.collect();
	out.insert("tooling".into(), Value::Object(tooling));
	Ok(Value::Object(out))
}

/// The record's bands against its baseline and, cumulatively, against the
/// approved anchor; anchor evidence is required even on an identity PASS.
pub fn score(
	baseline: &Value,
	candidate: &Value,
	anchor: Option<&Value>,
	policy: &Value,
) -> Result<Value> {
	validate_policy(policy)?;
	let Some(anchor) = anchor else {
		bail!(value_error(
			"anchor evidence required even on identity PASS"
		));
	};
	anyhow::ensure!(
		baseline["capture_paths"]["measured"]
			.get("installed")
			.is_none()
			|| candidate["capture_paths"]["measured"]
				.get("installed")
				.is_some(),
		"installed file route disappeared"
	);
	if candidate["capture_paths"]["measured"]
		.get("installed")
		.is_some()
		&& baseline["capture_paths"]["measured"]
			.get("installed")
			.is_none()
	{
		anyhow::ensure!(
			baseline["revision"] == policy["installed_setup"]["previous"],
			"unapproved installed setup transition"
		);
	}
	let policy_sha = identity(policy)?;
	for (i, receipt) in [baseline, candidate, anchor].into_iter().enumerate() {
		if *at(receipt, "status")? != json!("PASS") {
			bail!(value_error("failed capture"));
		}
		if policy["schema"] == 2 {
			anyhow::ensure!(
				receipt["capture_paths"]["launch_sha256"]
					== policy[if receipt["capture_paths"]["measured"]
						.get("installed")
						.is_some()
					{
						"installed_setup"
					} else {
						"native_setup"
					}]["launch_sha256"]
					&& receipt["recipe"]["policy"]["launch_sha256"]
						== policy["native_setup"]["launch_sha256"],
				"incomparable native setup capture: measured argv paths or ordered environment differ from the setup reference"
			);
			anyhow::ensure!(
				receipt["recipe"]["policy"]["schema"] == 2
					&& receipt["capture_key"]
						== crate::fixed_route_stage::contextual_key(
							receipt["binary_sha256"]
								.as_str()
								.context("missing measured binary identity")?,
							at(receipt, "recipe")?,
							at(receipt, "capture_paths")?
						)?,
				"split capture recipe/binary binding"
			);
		}
		if *at(receipt, "policy_sha256")? != json!(policy_sha) {
			bail!(value_error("stale policy"));
		}
		if let Some(desc) = receipt["capture_paths"].get("measured") {
			for row in receipt["records"].as_array().context("missing records")? {
				crate::fixed_route_images::validate_row(row, desc)?;
			}
		}
		if at(receipt, "recipe")? != at(candidate, "recipe")? {
			let empty = json!({});
			let proof = receipt.get("reuse_proof").unwrap_or(&empty);
			if i != 2 || proof.get("current_recipe") != Some(at(candidate, "recipe")?) {
				bail!(value_error("recipe/toolchain/CPU/environment mismatch"));
			}
			if measurement_recipe(at(receipt, "recipe")?)?
				!= measurement_recipe(at(candidate, "recipe")?)?
			{
				bail!(value_error("anchor measurement inputs mismatch"));
			}
			let binary = receipt.get("binary_sha256");
			if !truthy(binary)
				|| proof.get("binary_sha256") != binary
				|| proof.get("current_build").and_then(|b| b.get("sha256")) != binary
				|| proof["current_build"].get("measured")
					!= receipt["capture_paths"].get("measured")
			{
				bail!(value_error("anchor reuse lacks identical current binary"));
			}
		}
	}
	if at(anchor, "revision")? != at(policy, "anchor")? {
		bail!(value_error("wrong anchor"));
	}
	let limit = |k: &str| -> Result<i128> {
		count(at(policy, k)?).ok_or_else(|| value_error("policy limits changed: review required"))
	};
	let per_record = compare(
		&baseline["records"],
		&candidate["records"],
		policy,
		limit("per_record_per_mille")?,
	)?;
	let cumulative = compare(
		&anchor["records"],
		&candidate["records"],
		policy,
		limit("cumulative_per_mille")?,
	)?;
	let pass = per_record
		.iter()
		.chain(&cumulative)
		.all(|r| r["pass_all"] == true);
	let mut dynamic = Vec::new();
	for (role, receipt) in [
		("baseline", baseline),
		("candidate", candidate),
		("anchor", anchor),
	] {
		for r in receipt["records"].as_array().into_iter().flatten() {
			dynamic.push(json!({"role": role, "kind": r["kind"], "mode": r["policy"], "n": r["n"], "Ir": r["accounting"]["dynamic_Ir"]}));
		}
	}
	Ok(
		json!({"status": if pass { "PASS" } else { "FAIL" }, "per_record": per_record, "cumulative": cumulative, "dynamic": dynamic}),
	)
}

// --- The anchor cache. ---

/// Where a control may fail a publication or its cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
	/// The first directory sync, after the pending directory is made.
	Created,
	/// Each artifact copy.
	Copy,
	/// The rename of pending onto the entry.
	Rename,
	/// After the rename, before the bundle is verified.
	Renamed,
	/// The cleanup of a failed publication.
	Cleanup,
}

type Hook = Box<dyn FnMut(Point) -> std::io::Result<()>>;
type Ids = Box<dyn FnMut() -> String>;

/// An identity-checked absolute path: no link, no `.` or `..`.
/// A path with a parent component is refused before it is resolved, as
/// Python's `canonical` refuses it.
fn canon(path: &Path) -> Result<PathBuf> {
	if path.components().any(|c| c == Component::ParentDir) {
		bail!(value_error("symlink or noncanonical owner path"));
	}
	crate::evidence::canonical(path).map_err(|_| value_error("symlink or noncanonical owner path"))
}

/// A journal's structure, checked before any recovery or mutation: an
/// entries list of objects, each with an id and a state, and a current
/// id whenever there is an entry.
fn check_journal(state: &Value) -> Result<()> {
	let entries = &state["entries"];
	let Value::Array(rows) = entries else {
		bail!(raise(
			"TypeError",
			&format!("'{}' object is not iterable", py_type(entries)),
		));
	};
	for row in rows {
		let Value::Object(fields) = row else {
			bail!(raise(
				"TypeError",
				&format!("'{}' object is not subscriptable", py_type(row)),
			));
		};
		// Recovery reads each row's id, then the current id, then the row's
		// state, as Python does; none is removed without all three.
		at(row, "id")?;
		at(state, "current")?;
		if !fields.contains_key("state") {
			bail!(raise("KeyError", "'state'"));
		}
	}
	Ok(())
}

fn exists_or_link(path: &Path) -> bool {
	path.exists() || path.symlink_metadata().is_ok()
}

/// The one owned anchor bundle: the consumer holds the lock until it drops
/// the cache, through scoring.
pub struct Cache {
	pub root: PathBuf,
	journal: PathBuf,
	_lock: std::fs::File,
	pub state: Value,
	ids: Ids,
	hook: Hook,
}

impl Cache {
	/// Lock `root` and recover every entry that is not the complete current
	/// one, as `with Cache(root)` does.
	pub fn open(root: &Path) -> Result<Self> {
		Self::with(root, Box::new(uuid_hex), Box::new(|_| Ok(())))
	}

	/// The same, with the identifiers and fault points a control chooses.
	pub fn with(root: &Path, ids: Ids, hook: Hook) -> Result<Self> {
		let root = canon(root)?;
		std::fs::create_dir_all(&root)?;
		let lock_path = canon(&root.join("LOCK"))?;
		let lock = std::fs::OpenOptions::new()
			.read(true)
			.append(true)
			.create(true)
			.open(&lock_path)?;
		// SAFETY: flock on a descriptor this value owns.
		if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_EX) } != 0 {
			return Err(std::io::Error::last_os_error().into());
		}
		let journal = canon(&root.join("OWNERS.json"))?;
		let state = if journal.exists() {
			serde_json::from_slice(&std::fs::read(&journal)?)?
		} else {
			json!({"schema": 1, "current": null, "entries": []})
		};
		if state.get("schema") != Some(&json!(1)) {
			bail!(value_error("unknown cache ownership schema"));
		}
		check_journal(&state)?;
		let mut cache = Self {
			root,
			journal,
			_lock: lock,
			state,
			ids,
			hook,
		};
		let ids: Vec<Value> = cache.entries().iter().map(|r| r["id"].clone()).collect();
		for id in ids {
			let row = cache.row(&id).context("journal entry vanished")?;
			if row["id"] != cache.state["current"] || row["state"] != "complete" {
				cache.remove(&id)?;
			}
		}
		Ok(cache)
	}

	fn entries(&self) -> Vec<Value> {
		self.state["entries"]
			.as_array()
			.cloned()
			.unwrap_or_default()
	}

	fn row(&self, id: &Value) -> Option<Value> {
		self.entries().into_iter().find(|r| r["id"] == *id)
	}

	fn update(&mut self, id: &Value, change: impl FnOnce(&mut Map<String, Value>)) {
		if let Some(row) = self.state["entries"]
			.as_array_mut()
			.and_then(|e| e.iter_mut().find(|r| r["id"] == *id))
			.and_then(Value::as_object_mut)
		{
			change(row);
		}
	}

	fn forget(&mut self, id: &Value) {
		if let Some(entries) = self.state["entries"].as_array_mut() {
			entries.retain(|r| r["id"] != *id);
		}
		if self.state["current"] == *id {
			self.state["current"] = Value::Null;
		}
	}

	fn save(&self) -> Result<()> {
		atomic_json(&self.journal, &self.state)
	}

	fn path(&self, row: &Value) -> Result<PathBuf> {
		let p = canon(Path::new(row["path"].as_str().unwrap_or_default()))?;
		if p.parent() != Some(self.root.as_path()) {
			bail!(value_error("cache entry outside owner"));
		}
		let meta = std::fs::metadata(&self.root)?;
		if json!(meta.dev()) != row["parent_device"] || json!(meta.ino()) != row["parent_inode"] {
			bail!(value_error("cache parent replaced"));
		}
		Ok(p)
	}

	fn check(row: &Value, p: &Path) -> Result<()> {
		canon(p)?;
		let meta = p.symlink_metadata()?;
		if !meta.is_dir() || json!(meta.dev()) != row["device"] || json!(meta.ino()) != row["inode"]
		{
			bail!(value_error("cache root replaced"));
		}
		Ok(())
	}

	fn remove(&mut self, id: &Value) -> Result<()> {
		let row = self.row(id).context("no such cache entry")?;
		let mut p = self.path(&row)?;
		if row["state"] == "publishing" {
			let old = canon(Path::new(row["rename_from"].as_str().unwrap_or_default()))?;
			if old.parent() != Some(self.root.as_path()) {
				bail!(value_error("outside pending root"));
			}
			if old.exists() && p.exists() {
				bail!(value_error("ambiguous cache publication"));
			}
			if old.exists() {
				p = old;
			}
		}
		if row["state"] == "creating" {
			// No recorded inode: empty-only, never recursive.
			if p.exists() {
				std::fs::remove_dir(&p)?;
			}
		} else if exists_or_link(&p) {
			Self::check(&row, &p)?;
			let shown = p.to_string_lossy().into_owned();
			self.update(id, |r| {
				r.insert("path".into(), json!(shown));
				r.insert("state".into(), json!("releasing"));
			});
			self.save()?;
			std::fs::remove_dir_all(&p)?;
		} else if row["state"] != "releasing" {
			bail!(value_error("owned cache root disappeared"));
		}
		sync_directory(&self.root)?;
		self.forget(id);
		self.save()
	}

	/// The current bundle for `key` and why: hit, absent, key differs, or
	/// verification failed (the entry then removed).
	pub fn lookup(&mut self, key: &str) -> Result<(Option<Value>, &'static str)> {
		let current = self.state["current"].clone();
		let Some(row) = self.row(&current).filter(|_| !current.is_null()) else {
			return Ok((None, "absent"));
		};
		let p = self.path(&row)?;
		if !exists_or_link(&p) {
			self.forget(&current);
			self.save()?;
			return Ok((None, "verification failed"));
		}
		Self::check(&row, &p)?;
		if row["key"] != json!(key) {
			return Ok((None, "key differs"));
		}
		let verified = (|| -> Result<Value> {
			if json!(digest(&p.join("ANCHOR.json"))?) != row["receipt_sha256"] {
				bail!(value_error("anchor metadata differs"));
			}
			Self::verify(&p, key)
		})();
		match verified {
			Ok(r) => Ok((Some(r), "hit")),
			Err(_) => {
				// Delete only the known, identity-checked root.
				self.remove(&current)?;
				Ok((None, "verification failed"))
			}
		}
	}

	/// A bundle whose every artifact is inside it and has its digest, with
	/// its binary and every row's files bound.
	pub fn verify(path: &Path, key: &str) -> Result<Value> {
		let path = canon(path)?;
		let r: Value = serde_json::from_slice(&std::fs::read(canon(&path.join("ANCHOR.json"))?)?)?;
		let split = r["recipe"]["policy"]["schema"] == 2;
		if r.get("schema") != Some(&json!(if split { 3 } else { 2 }))
			|| r.get("status") != Some(&json!("PASS"))
			|| r.get("capture_key") != Some(&json!(key))
			|| !truthy(r.get("artifacts"))
		{
			bail!(value_error("invalid anchor bundle"));
		}
		let artifacts = r["artifacts"]
			.as_object()
			.context("artifacts is not an object")?;
		for (name, sha) in artifacts {
			let p = canon(Path::new(name))?;
			if !p.starts_with(&path) || !p.is_file() || json!(digest(&p)?) != *sha {
				bail!(value_error("cache artifact differs"));
			}
		}
		let binary = at(&r, "binary")?.as_str().unwrap_or_default();
		if artifacts.get(binary) != Some(at(&r, "binary_sha256")?) {
			bail!(value_error("unbound anchor binary"));
		}
		if let Some(desc) = r["capture_paths"].get("measured") {
			crate::fixed_route_images::validate(desc, &r["binary_sha256"])?;
			anyhow::ensure!(
				crate::fixed_route_stage::contextual_key(
					r["binary_sha256"].as_str().unwrap_or_default(),
					&r["recipe"],
					&r["capture_paths"]
				)? == key,
				"unbound artifact identity"
			);
			if let Some(outer) = desc.get("installed") {
				let file = r["installed_binary"]
					.as_str()
					.context("missing cached installed binary")?;
				anyhow::ensure!(
					artifacts.get(file) == outer.get("sha256"),
					"unbound cached installed binary"
				);
			}
			if let Some(engine) = desc.get("engine") {
				let file = r["engine_binary"]
					.as_str()
					.context("missing cached engine")?;
				anyhow::ensure!(
					artifacts.get(file) == engine.get("sha256"),
					"unbound cached engine"
				);
			}
		}
		let records = at(&r, "records")?;
		let Value::Array(records) = records else {
			bail!(raise(
				"TypeError",
				&format!("'{}' object is not iterable", py_type(records)),
			));
		};
		for record in records {
			if !record.is_object() {
				bail!(raise(
					"TypeError",
					&format!("'{}' object is not subscriptable", py_type(record)),
				));
			}
			if let Some(desc) = r["capture_paths"].get("measured") {
				crate::fixed_route_images::validate_row(record, desc)?;
			}
			for file in case_files(record, split)? {
				if artifacts.get(&*file.source.to_string_lossy()) != Some(&file.sha) {
					bail!(value_error("unbound anchor row"));
				}
				if file.name == "GDB.json" {
					let saved: Value = serde_json::from_slice(&std::fs::read(&file.source)?)?;
					let boundary = if record["policy"] == "on" {
						"boundary"
					} else {
						"image_boundary"
					};
					anyhow::ensure!(record[boundary] == saved, "unbound invocation boundary");
				}
			}
		}
		Ok(r)
	}

	fn fault(&mut self, point: Point) -> Result<()> {
		(self.hook)(point).map_err(|e| raise("OSError", &e.to_string()))
	}

	/// Copy a verified receipt's binary and case files into a new entry,
	/// through pending, publishing and complete, and make it the one entry.
	pub fn publish(&mut self, receipt: &Value, binary: &Path, provenance: &Value) -> Result<Value> {
		let ident = (self.ids)();
		let id = json!(ident);
		let pending = self.root.join(format!("pending-{ident}"));
		let dest = self.root.join(format!("entry-{ident}"));
		let parent = std::fs::metadata(&self.root)?;
		let row = json!({"id": ident, "path": pending.to_string_lossy(), "key": at(receipt, "capture_key")?,
			"state": "creating", "parent_device": parent.dev(), "parent_inode": parent.ino()});
		self.state["entries"]
			.as_array_mut()
			.context("journal entries")?
			.push(row);
		self.save()?;
		std::fs::create_dir(&pending)?;
		self.fault(Point::Created)?;
		sync_directory(&self.root)?;
		let made = std::fs::metadata(&pending)?;
		self.update(&id, |r| {
			r.insert("state".into(), json!("building"));
			r.insert("device".into(), json!(made.dev()));
			r.insert("inode".into(), json!(made.ino()));
		});
		self.save()?;
		match self.build(receipt, binary, provenance, &id, &pending, &dest) {
			Ok(r) => Ok(r),
			Err(error) => {
				// The journal survives a failed cleanup, so a later lock
				// holder can recover.
				if self.row(&id).is_some_and(|r| r["state"] != "complete") {
					self.fault(Point::Cleanup)?;
					self.remove(&id)?;
				}
				Err(error)
			}
		}
	}

	fn build(
		&mut self,
		receipt: &Value,
		binary: &Path,
		provenance: &Value,
		id: &Value,
		pending: &Path,
		dest: &Path,
	) -> Result<Value> {
		let split = receipt["recipe"]["policy"]["schema"] == 2;
		let mut r = receipt.clone();
		let object = r.as_object_mut().context("receipt is not an object")?;
		object.remove("reuse_proof");
		object.insert("schema".into(), json!(if split { 3 } else { 2 }));
		object.insert("provenance".into(), provenance.clone());
		object.insert("artifacts".into(), json!({}));
		object.insert(
			"binary".into(),
			json!(dest.join("rnx-anchor").to_string_lossy()),
		);
		let mut copies: Vec<(PathBuf, PathBuf, Value)> = vec![(
			binary.to_path_buf(),
			PathBuf::from("rnx-anchor"),
			at(&r, "binary_sha256")?.clone(),
		)];
		if let Some(outer) = receipt["capture_paths"]["measured"].get("installed") {
			let source = receipt["installed_binary"]
				.as_str()
				.context("missing installed binary source")?;
			copies.push((
				PathBuf::from(source),
				PathBuf::from("installed-cli"),
				outer["sha256"].clone(),
			));
			r["installed_binary"] = json!(dest.join("installed-cli"));
		}
		if let Some(engine) = receipt["capture_paths"]["measured"].get("engine") {
			let source = receipt["engine_binary"]
				.as_str()
				.context("missing engine source")?;
			copies.push((
				PathBuf::from(source),
				PathBuf::from("engine.so"),
				engine["sha256"].clone(),
			));
			r["engine_binary"] = json!(dest.join("engine.so"));
		}
		let records = r["records"].as_array_mut().context("receipt records")?;
		for (i, record) in records.iter_mut().enumerate() {
			let base = PathBuf::from("cases").join(format!("{i:02}"));
			for f in case_files(record, split)? {
				let relative = base.join(f.name);
				let final_path = json!(dest.join(&relative).to_string_lossy());
				match f.name {
					"setup.callgrind" => record["phases"]["setup"]["profile"] = final_path,
					"execution.callgrind" => record["phases"]["execution"]["profile"] = final_path,
					"profile" => record["profile"] = final_path,
					"dispatch.callgrind" => record["dispatch"]["profile"] = final_path,
					"post-exec.callgrind" => record["dispatch"]["inner"] = final_path,
					"DISPATCH.json" => record["dispatch"]["proof"] = final_path,
					_ => {}
				}
				copies.push((f.source, relative, f.sha));
			}
		}
		let mut artifacts = Map::new();
		for (source, relative, sha) in copies {
			if json!(digest(&source)?) != sha {
				bail!(value_error("anchor source changed"));
			}
			let p = pending.join(&relative);
			std::fs::create_dir_all(p.parent().context("copy without directory")?)?;
			self.fault(Point::Copy)?;
			std::fs::copy(&source, &p)?;
			if json!(digest(&p)?) != sha {
				bail!(value_error("anchor copy differs"));
			}
			std::fs::File::open(&p)?.sync_all()?;
			artifacts.insert(dest.join(&relative).to_string_lossy().into_owned(), sha);
		}
		r["artifacts"] = Value::Object(artifacts);
		atomic_json(&pending.join("ANCHOR.json"), &r)?;
		sync_tree(pending)?;
		let shown = (
			dest.to_string_lossy().into_owned(),
			pending.to_string_lossy().into_owned(),
		);
		self.update(id, |row| {
			row.insert("state".into(), json!("publishing"));
			row.insert("path".into(), json!(shown.0));
			row.insert("rename_from".into(), json!(shown.1));
		});
		self.save()?;
		self.fault(Point::Rename)?;
		std::fs::rename(pending, dest)?;
		sync_directory(&self.root)?;
		self.fault(Point::Renamed)?;
		Self::verify(dest, at(&r, "capture_key")?.as_str().unwrap_or_default())?;
		let sha = digest(&dest.join("ANCHOR.json"))?;
		self.update(id, |row| {
			row.insert("state".into(), json!("complete"));
			row.insert("receipt_sha256".into(), json!(sha));
			row.remove("rename_from");
		});
		self.state["current"] = id.clone();
		self.save()?;
		let others: Vec<Value> = self
			.entries()
			.into_iter()
			.map(|e| e["id"].clone())
			.filter(|other| other != id)
			.collect();
		for other in others {
			self.remove(&other)?;
		}
		Ok(r)
	}
}

/// Every directory under `root`, deepest first, then `root`, synced.
fn sync_tree(root: &Path) -> Result<()> {
	for entry in std::fs::read_dir(root)? {
		let entry = entry?;
		if entry.file_type()?.is_dir() {
			sync_tree(&entry.path())?;
		}
	}
	sync_directory(root)
}

/// A random 128-bit identifier as 32 hex digits, as `uuid4().hex` is.
fn uuid_hex() -> String {
	let mut bytes = [0u8; 16];
	if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
		let _ = std::io::Read::read_exact(&mut f, &mut bytes);
	}
	crate::pyjson::hex(&bytes)
}

/// The names in an owned case bundle are fixed. Only the source paths
/// move when the stage publishes a cache entry.
pub(crate) struct CaseFile {
	pub source: PathBuf,
	pub name: &'static str,
	pub sha: Value,
}

pub(crate) fn case_files(row: &Value, split: bool) -> Result<Vec<CaseFile>> {
	let phased = split && row["policy"] == "on";
	let mut files = Vec::new();
	let mut profile = |part: &Value, name| -> Result<PathBuf> {
		let path = PathBuf::from(
			at(part, "profile")?
				.as_str()
				.context("invalid profile path")?,
		);
		let dir = path
			.parent()
			.context("profile has no directory")?
			.to_owned();
		files.push(CaseFile {
			source: path,
			name,
			sha: at(part, "profile_sha256")?.clone(),
		});
		Ok(dir)
	};
	let dir = if phased {
		let setup = profile(&row["phases"]["setup"], "setup.callgrind")?;
		let execution = profile(&row["phases"]["execution"], "execution.callgrind")?;
		anyhow::ensure!(
			setup == execution && files[0].source != files[1].source,
			"phase profile paths"
		);
		execution
	} else {
		profile(row, "profile")?
	};
	if let Some(dispatch) = row.get("dispatch") {
		for (name, path_key, sha_key) in [
			("dispatch.callgrind", "profile", "profile_sha256"),
			("post-exec.callgrind", "inner", "inner_sha256"),
			("DISPATCH.json", "proof", "proof_sha256"),
		] {
			files.push(CaseFile {
				source: PathBuf::from(
					dispatch[path_key]
						.as_str()
						.context("installed dispatch file")?,
				),
				name,
				sha: at(dispatch, sha_key)?.clone(),
			});
		}
	}
	for (name, key) in [
		("program.rn", "fixture_sha256"),
		("program.stdout", "stdout_sha256"),
	] {
		files.push(CaseFile {
			source: dir.join(name),
			name,
			sha: at(row, key)?.clone(),
		});
	}
	if split {
		for (name, key) in [
			("CONFIG.json", "config_sha256"),
			("LAUNCH.json", "launch_sha256"),
			("program.stderr", "stderr_sha256"),
		] {
			files.push(CaseFile {
				source: dir.join(name),
				name,
				sha: at(row, key)?.clone(),
			});
		}
	}
	if phased || row.get("image_boundary").is_some() {
		files.push(CaseFile {
			source: dir.join("GDB.json"),
			name: "GDB.json",
			sha: at(row, "boundary_sha256")?.clone(),
		});
	}
	Ok(files)
}
