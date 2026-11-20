//! Reproducibility between two finals of one candidate: two Rust finals,
//! or a Rust final and one the retired Python controller wrote. Every receipt is read as its type and compared whole,
//! after substituting each final's own paths; only the run-specific paths
//! listed here are excluded, each by its exact place in its receipt. A
//! missing receipt, a broken binding or any other difference fails.
use crate::receipt::{Field, digest};
use crate::types::{self, Acceptance, ChildResult, FinalResult, Kind, Manifest, Selection};
use anyhow::{Context as _, Result};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::compare_oracle as oracle;
use crate::compare_route as route;

/// Run-specific places in a stage child's validation receipt.
pub const CHILD_RUN_SPECIFIC: [&str; 12] = [
	"context.invocation",
	// The declared values are compared through ENVIRONMENTS.json instead.
	"context.environment.sha256",
	"context.existing_results",
	"seconds",
	"cleanup_seconds",
	"cleanup[].seconds",
	"cleanup[].sha256",
	"artifacts.*",
	"producer.context",
	"producer.invocation",
	"producer.sha256",
	"failure.sha256",
];

/// Run-specific places in a producer's own semantic receipt: times, log
/// digests and the binding to this run's validation context.
pub const SEMANTIC_RUN_SPECIFIC: [&str; 6] = [
	"validation_context",
	"seconds",
	"artifacts.*",
	"contracts.seconds",
	"contracts.cases[].seconds",
	"contracts.cases[].artifacts.*",
];

/// Further run-specific places in the cross verifier's receipt only:
/// binaries and metadata built in its own directory (not byte-reproducible
/// across directories), its owned build's accounting, and times; found by
/// diffing two runs of one tree. No other producer's fields are excused.
pub const CROSS_RUN_SPECIFIC: [&str; 16] = [
	"binding.artifact.executable_sha256",
	"binding.artifact.stream_sha256",
	"binding.binary_copy_sha256",
	"binding.dependencies.metadata_sha256",
	"binding.executable_sha256",
	"cleanup.allocated_file_bytes",
	"cleanup.logical_file_bytes",
	"cleanup.owner.invocation",
	"cleanup.owner.parent_inode",
	"cleanup.retained.*",
	"cleanup.seconds",
	"results.census.binary_sha256",
	"results.census.runs[].seconds",
	"results.census.seconds",
	"results.enrollment.child_binary_sha256",
	"results.enrollment.seconds",
];

/// Further run-specific places in the fixed-route screen's receipt only:
/// its binaries, profiles and anchor key are built and captured inside the
/// final's own directory, so their digests differ in any two finals; and
/// its anchor build's times.
pub const FIXED_ROUTE_RUN_SPECIFIC: [&str; 19] = [
	"anchor_cache.anchor_build_seconds",
	"anchor_cache.anchor_total_seconds",
	"anchor_cache.key",
	"baseline.artifacts.*",
	"baseline.binary_sha256",
	"baseline.capture_key",
	"baseline.records[].profile_sha256",
	"candidate.artifacts.*",
	"candidate.binary_sha256",
	"candidate.capture_key",
	"candidate.records[].profile_sha256",
	"baseline.records[].boundary.thread",
	"candidate.records[].boundary.thread",
	"baseline.records[].phases.*.profile_sha256",
	"candidate.records[].phases.*.profile_sha256",
	"baseline.records[].boundary_sha256",
	"candidate.records[].boundary_sha256",
	"baseline.records[].stderr_sha256",
	"candidate.records[].stderr_sha256",
];

/// The fixed-route screen's counts and the values derived from them. They
/// are left out of the whole-receipt comparison and checked by `route`
/// instead: each screen's derived values against its own counts, and the
/// counts exactly at one path length, with one named bounded case.
pub const FIXED_ROUTE_COUNTS: [&str; 15] = [
	"baseline.records[].accounting",
	"candidate.records[].accounting",
	"per_record[].B1",
	"per_record[].B5",
	"per_record[].C1",
	"per_record[].C5",
	"cumulative[].B1",
	"cumulative[].B5",
	"cumulative[].C1",
	"cumulative[].C5",
	"dynamic[].Ir",
	"baseline.records[].phases.*.accounting",
	"candidate.records[].phases.*.accounting",
	"per_record[].phases",
	"cumulative[].phases",
];

/// The Jupyter controls' receipt binds each nested check's own receipt by
/// digest; that receipt carries its run's invocation, so the digest is
/// run-specific and the receipts themselves are compared instead
/// (`nested_checks`).
pub const JUPYTER_RUN_SPECIFIC: [&str; 1] = ["checks[].validation.sha256"];

/// The run-specific places of one producer's receipt.
fn semantic_run_specific(stage: &str) -> Vec<&'static str> {
	let mut places = SEMANTIC_RUN_SPECIFIC.to_vec();
	match stage {
		"cross" => places.extend(CROSS_RUN_SPECIFIC),
		"fixed-route" => {
			places.extend(FIXED_ROUTE_RUN_SPECIFIC);
			places.extend(FIXED_ROUTE_COUNTS);
		}
		"jupyter-kernel" => places.extend(JUPYTER_RUN_SPECIFIC),
		_ => {}
	}
	places
}

/// The run-specific places of one stage's validation receipt. The Polars
/// suite's oracle report is compared row by row instead of by digest.
fn child_run_specific(stage: &str) -> Vec<&'static str> {
	let mut places = CHILD_RUN_SPECIFIC.to_vec();
	if stage == "polars-suite" {
		places.push("declared_output.actual_sha256");
	}
	places
}

/// The oracle report, verified against its declared digest, with each
/// unordered row's output in its canonical form (`oracle`).
fn oracle_report(f: &Final, row: &ChildResult) -> Result<Option<Value>> {
	let Some(output) = row.declared_output.value() else {
		return Ok(None);
	};
	let path = PathBuf::from(&output.archive);
	anyhow::ensure!(
		digest(&path)? == output.actual_sha256,
		"polars-suite: oracle report differs from its declared digest"
	);
	let mut report: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
	oracle::canonicalize(&mut report);
	Ok(Some(f.normalize(&report)))
}

/// Run-specific places in the selection: this final's directory and the
/// controller's own policy identity.
pub const SELECTION_RUN_SPECIFIC: [&str; 2] = ["out", "policy"];

/// Run-specific places in the verdict: times, this run's terminal record
/// digests, free space, and the finalization's own bindings to them.
pub const RESULT_RUN_SPECIFIC: [&str; 8] = [
	"seconds",
	"records",
	"free_bytes_before",
	"free_bytes_after",
	"finalization.selection",
	"finalization.durations",
	"finalization.evidence",
	"remote_context",
];

/// One final, with what makes a path its own.
struct Final {
	out: PathBuf,
	selection: Selection,
	result: FinalResult,
	acceptance: Acceptance,
	environments: BTreeMap<String, BTreeMap<String, String>>,
	substitutions: Vec<(String, &'static str)>,
}

impl Final {
	fn load(out: &Path) -> Result<Self> {
		let selection: Selection = types::read(&out.join("SELECTION.json"))?;
		let result: FinalResult = types::read(&out.join("RESULT.json"))?;
		// Everything compared is bound to the verdict's terminal records.
		for name in ["ACCEPTANCE", "MANIFEST", "RELEASE", "ENVIRONMENTS"] {
			let recorded = result.records.get(name).with_context(|| {
				format!("{}: terminal record {name} is not bound", out.display())
			})?;
			anyhow::ensure!(
				digest(&out.join(format!("{name}.json")))? == *recorded,
				"{}: {name} differs from its terminal record",
				out.display()
			);
		}
		if result.status == types::Status::Pass {
			crate::evidence::revalidate(out)?;
		}
		let acceptance: Acceptance = types::read(&out.join("ACCEPTANCE.json"))?;
		// A declared schedule is validated on its own; its times and choices
		// are this run's, not a parity field.
		crate::evidence::check_schedule(out, &result, &acceptance)?;
		let manifest: Manifest = types::read(&out.join("MANIFEST.json"))?;
		let environments = types::read(&out.join("ENVIRONMENTS.json"))?;
		static OWNER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([0-9a-f]{32})-").unwrap());
		let mut substitutions = Vec::new();
		// The cross verifier's directory is derived from the final's; it goes first.
		for (_, command) in &selection.stages {
			if let Some(i) = command.iter().position(|a| a == "--out")
				&& let Some(parent) = command
					.get(i + 1)
					.filter(|o| o.contains("rnx-verify"))
					.and_then(|o| Path::new(o).parent())
			{
				substitutions.push((parent.to_string_lossy().into_owned(), "<CROSS>"));
			}
		}
		substitutions.push((selection.out.clone(), "<OUT>"));
		if let Some(id) = manifest.roots.iter().find_map(|r| {
			Path::new(&r.owner.path).file_name().and_then(|n| {
				OWNER
					.captures(&n.to_string_lossy())
					.map(|c| c[1].to_owned())
			})
		}) {
			substitutions.push((id, "<OWNER>"));
		}
		Ok(Self {
			out: out.to_owned(),
			selection,
			result,
			acceptance,
			environments,
			substitutions,
		})
	}

	fn substitute(&self, text: &str) -> String {
		self.substitutions
			.iter()
			.fold(text.to_owned(), |text, (from, to)| {
				text.replace(from.as_str(), to)
			})
	}

	/// A receipt with this final's paths substituted in every string and key.
	fn normalize(&self, value: &Value) -> Value {
		match value {
			Value::String(text) => Value::String(self.substitute(text)),
			Value::Array(items) => Value::Array(items.iter().map(|v| self.normalize(v)).collect()),
			Value::Object(map) => Value::Object(
				map.iter()
					.map(|(k, v)| (self.substitute(k), self.normalize(v)))
					.collect(),
			),
			other => other.clone(),
		}
	}

	/// The stage's validation receipt, bound to its acceptance row.
	fn stage(&self, name: &str) -> Result<ChildResult> {
		let row = self
			.acceptance
			.checks
			.iter()
			.find(|c| c.name == name)
			.with_context(|| format!("{name}: no acceptance row"))?;
		let validation = row
			.validation
			.as_ref()
			.with_context(|| format!("{name}: no validation receipt"))?;
		let path = PathBuf::from(&validation.path);
		anyhow::ensure!(
			digest(&path).with_context(|| format!("{name}: validation receipt missing"))?
				== validation.sha256,
			"{name}: validation receipt differs from its acceptance binding"
		);
		if let Field::Value(finalization) = &self.acceptance.finalization {
			anyhow::ensure!(
				finalization.evidence.get(name) == Some(validation),
				"{name}: finalization evidence differs from the acceptance row"
			);
		}
		types::read(&path)
	}

	/// The producer's own receipt, from where it ran or as retained before
	/// release, bound to the digest its validation recorded.
	fn semantic(&self, name: &str, row: &ChildResult) -> Result<Option<Value>> {
		let context = row
			.context
			.value()
			.with_context(|| format!("{name}: no context"))?;
		if context.kind != Kind::Producer {
			return Ok(None);
		}
		let binding = row
			.producer
			.value()
			.with_context(|| format!("{name}: producer without a binding"))?;
		let original = PathBuf::from(&binding.receipt);
		let path = if original.is_file() {
			original
		} else {
			self.out
				.join("retained")
				.join(format!("{name}-RESULT.json"))
		};
		anyhow::ensure!(
			digest(&path)
				.with_context(|| format!("{name}: producer receipt missing after release"))?
				== binding.sha256,
			"{name}: producer receipt differs from its binding"
		);
		Ok(Some(serde_json::from_slice(&std::fs::read(&path)?)?))
	}
}

/// Replace every listed place with a marker; keys stay, so a missing or an
/// extra one still differs.
fn exclude(value: &mut Value, patterns: &[&str]) {
	for pattern in patterns {
		let segments: Vec<&str> = pattern.split('.').collect();
		mark(value, &segments);
	}
}

fn mark(value: &mut Value, segments: &[&str]) {
	let Some((first, rest)) = segments.split_first() else {
		*value = json!("<RUN>");
		return;
	};
	let (key, each) = match first.strip_suffix("[]") {
		Some(key) => (key, true),
		None => (*first, false),
	};
	let Value::Object(map) = value else { return };
	let targets: Vec<String> = if key == "*" {
		map.keys().cloned().collect()
	} else if map.contains_key(key) {
		vec![key.to_owned()]
	} else {
		vec![]
	};
	for target in targets {
		let Some(child) = map.get_mut(&target) else {
			continue;
		};
		if each {
			if let Value::Array(items) = child {
				for item in items {
					mark(item, rest);
				}
			}
		} else {
			mark(child, rest);
		}
	}
}

/// Every place two values differ, by path.
fn differ(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
	match (a, b) {
		(Value::Object(x), Value::Object(y)) => {
			let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
			for key in keys {
				match (x.get(key), y.get(key)) {
					(Some(p), Some(q)) => differ(&format!("{path}.{key}"), p, q, out),
					(p, _) => out.push(format!(
						"{path}.{key}: in the {} final only",
						if p.is_some() { "first" } else { "second" }
					)),
				}
			}
		}
		(Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
			for (i, (p, q)) in x.iter().zip(y).enumerate() {
				differ(&format!("{path}[{i}]"), p, q, out);
			}
		}
		_ if a != b => out.push(format!("{path}: {} != {}", brief(a), brief(b))),
		_ => {}
	}
}

fn brief(value: &Value) -> String {
	let text = value.to_string();
	match text.char_indices().nth(300) {
		Some((i, _)) => format!("{}...", &text[..i]),
		None => text,
	}
}

fn comparable<T: serde::Serialize>(final_: &Final, value: &T, patterns: &[&str]) -> Result<Value> {
	let mut value = final_.normalize(&serde_json::to_value(value)?);
	exclude(&mut value, patterns);
	Ok(value)
}

/// Each nested check of the Jupyter controls: its receipt must match its
/// bound digest and read as a stage receipt, and the two finals' receipts
/// are compared as stage receipts are.
fn nested_checks(a: &Final, p: &Value, b: &Final, q: &Value, differences: &mut Vec<String>) {
	let checks = |v: &Value| v["checks"].as_array().cloned().unwrap_or_default();
	let (left, right) = (checks(p), checks(q));
	if left.len() != right.len() {
		differences.push(format!(
			"jupyter-kernel checks: {} != {}",
			left.len(),
			right.len()
		));
		return;
	}
	let read = |check: &Value| -> Result<(ChildResult, String)> {
		let path = PathBuf::from(
			check["validation"]["path"]
				.as_str()
				.context("no validation path")?,
		);
		anyhow::ensure!(
			Value::String(digest(&path)?) == check["validation"]["sha256"],
			"{} differs from its binding",
			path.display()
		);
		Ok((types::read(&path)?, path.to_string_lossy().into_owned()))
	};
	for (i, (l, r)) in left.iter().zip(&right).enumerate() {
		match (read(l), read(r)) {
			(Ok((x, px)), Ok((y, py))) => match (
				comparable(a, &x, &CHILD_RUN_SPECIFIC),
				comparable(b, &y, &CHILD_RUN_SPECIFIC),
			) {
				(Ok(x), Ok(y)) => differ(
					&format!("jupyter-kernel check {i}"),
					&stage_view(x, &child_dir(a, &px)),
					&stage_view(y, &child_dir(b, &py)),
					differences,
				),
				(x, y) => differences.push(format!(
					"jupyter-kernel check {i}: {:?} / {:?}",
					x.err(),
					y.err()
				)),
			},
			(x, y) => differences.push(format!(
				"jupyter-kernel check {i}: {:?} / {:?}",
				x.err().map(|e| format!("{e:#}")),
				y.err().map(|e| format!("{e:#}"))
			)),
		}
	}
}

/// A stage receipt as compared. Its cleanup list follows build completion
/// order, so it is sorted, keeping every entry. Its artifacts become a
/// sorted list of entries, so none can merge with another, and only the
/// cleanup records an owned build writes in this receipt's own directory
/// under a fresh identifier have that identifier normalized.
fn stage_view(mut value: Value, child: &str) -> Value {
	static ID: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"^CLEANUP-[0-9a-f]{32}\.json$").unwrap());
	if let Some(Value::Array(items)) = value.get_mut("cleanup") {
		items.sort_by_key(|item| item.to_string());
	}
	if let Some(Value::Object(map)) = value.get("artifacts").cloned() {
		let mut entries: Vec<Value> = map
			.into_iter()
			.map(|(key, v)| {
				let key = match key.strip_prefix(child).and_then(|r| r.strip_prefix('/')) {
					Some(name) if ID.is_match(name) => format!("{child}/CLEANUP-<ID>.json"),
					_ => key,
				};
				json!([key, v])
			})
			.collect();
		entries.sort_by_key(|e| e.to_string());
		value["artifacts"] = Value::Array(entries);
	}
	value
}

/// The directory of a receipt at `path`, as this final's paths normalize it.
fn child_dir(f: &Final, path: &str) -> String {
	f.substitute(
		&Path::new(path)
			.parent()
			.map(|p| p.to_string_lossy().into_owned())
			.unwrap_or_default(),
	)
}

/// The fixed-route screens' counts (`route`). At one path length they must
/// agree exactly; otherwise a witness must bridge the lengths: a passed
/// final of the same candidate, run by the same controller as one final,
/// whose path has the other final's length, with that other final's
/// receipt and counts.
fn route_parity(
	a: &Final,
	p: &Value,
	b: &Final,
	q: &Value,
	witness: Option<&Final>,
	differences: &mut Vec<String>,
) -> Value {
	let (sa, sb) = match (route::Screen::read(p), route::Screen::read(q)) {
		(Ok(x), Ok(y)) => (x, y),
		(x, y) => {
			for (which, r) in [("first", x.err()), ("second", y.err())] {
				if let Some(error) = r {
					differences.push(format!(
						"fixed-route: the {which} final's screen: {error:#}"
					));
				}
			}
			return json!(null);
		}
	};
	let (la, lb) = (a.selection.out.len(), b.selection.out.len());
	if la == lb {
		differences.extend(
			sa.counts_differ(&sb)
				.into_iter()
				.map(|d| format!("fixed-route counts: {d}")),
		);
		return json!({"rule": "exact at one path length", "path_lengths": [la, lb], "witness": null});
	}
	let Some(w) = witness else {
		differences.push(format!(
			"fixed-route counts: the finals' directory paths differ in length ({la} != {lb}) and no witness is given"
		));
		return json!({"rule": "no witness", "path_lengths": [la, lb], "witness": null});
	};
	let lw = w.selection.out.len();
	let (peer, peer_receipt, peer_screen) = if w.selection.policy == a.selection.policy && lw == lb
	{
		(b, q, &sb)
	} else if w.selection.policy == b.selection.policy && lw == la {
		(a, p, &sa)
	} else {
		differences.push(format!(
			"fixed-route witness {}: it must share one final's controller and the other's path length ({la}, {lb}; witness {lw})",
			w.out.display()
		));
		return json!({"rule": "witness does not bridge", "path_lengths": [la, lb, lw], "witness": w.out});
	};
	if w.result.status != types::Status::Pass || w.result.candidate != peer.result.candidate {
		differences.push(format!(
			"fixed-route witness {}: not a passed final of {:?}",
			w.out.display(),
			peer.result.candidate
		));
	}
	let receipt = w.stage("fixed-route").and_then(|row| {
		w.semantic("fixed-route", &row)?
			.context("no producer receipt")
	});
	match receipt.and_then(|r| Ok((route::Screen::read(&r)?, r))) {
		Ok((screen, r)) => {
			let places = semantic_run_specific("fixed-route");
			match (
				comparable(w, &r, &places),
				comparable(peer, peer_receipt, &places),
			) {
				(Ok(x), Ok(y)) => differ("fixed-route witness receipt", &x, &y, differences),
				(x, y) => differences.push(format!(
					"fixed-route witness receipt: {:?} / {:?}",
					x.err(),
					y.err()
				)),
			}
			differences.extend(
				screen
					.counts_differ(peer_screen)
					.into_iter()
					.map(|d| format!("fixed-route witness counts: {d}")),
			);
		}
		Err(error) => differences.push(format!(
			"fixed-route witness {}: {error:#}",
			w.out.display()
		)),
	}
	json!({"rule": "exact against a path-matched witness", "path_lengths": [la, lb, lw], "witness": w.out})
}

/// Every difference between two finals that the contract does not name.
pub fn compare(a: &Path, b: &Path, witness: Option<&Path>) -> Result<Value> {
	let (a, b) = (Final::load(a)?, Final::load(b)?);
	let witness = witness.map(Final::load).transpose()?;
	let mut fixed_route = json!(null);
	let mut differences = Vec::new();
	differ(
		"SELECTION",
		&comparable(&a, &a.selection, &SELECTION_RUN_SPECIFIC)?,
		&comparable(&b, &b.selection, &SELECTION_RUN_SPECIFIC)?,
		&mut differences,
	);
	differ(
		"RESULT",
		&comparable(&a, &a.result, &RESULT_RUN_SPECIFIC)?,
		&comparable(&b, &b.result, &RESULT_RUN_SPECIFIC)?,
		&mut differences,
	);
	differ(
		"ENVIRONMENTS",
		&json!(a.environments),
		&json!(b.environments),
		&mut differences,
	);
	// Each final's environments cover exactly the stages it started.
	for (which, f) in [("first", &a), ("second", &b)] {
		let ran: std::collections::BTreeSet<&String> =
			f.acceptance.checks.iter().map(|c| &c.name).collect();
		let recorded: std::collections::BTreeSet<&String> = f.environments.keys().collect();
		if ran != recorded {
			differences.push(format!(
				"ENVIRONMENTS: the {which} final records {recorded:?} for stages {ran:?}"
			));
		}
	}
	let names: Vec<String> = a.selection.stages.iter().map(|(n, _)| n.clone()).collect();
	let ran = |f: &Final, name: &str| f.acceptance.checks.iter().any(|c| c.name == name);
	for name in &names {
		if !ran(&a, name) && !ran(&b, name) {
			continue;
		}
		let (x, y) = match (a.stage(name), b.stage(name)) {
			(Ok(x), Ok(y)) => (x, y),
			(x, y) => {
				for (which, r) in [("first", x.err()), ("second", y.err())] {
					if let Some(error) = r {
						differences.push(format!("{name}: {which} final: {error:#}"));
					}
				}
				continue;
			}
		};
		let child = |f: &Final| {
			f.acceptance
				.checks
				.iter()
				.find(|c| c.name == *name)
				.and_then(|c| c.validation.as_ref())
				.map(|v| child_dir(f, &v.path))
				.unwrap_or_default()
		};
		differ(
			name,
			&stage_view(comparable(&a, &x, &child_run_specific(name))?, &child(&a)),
			&stage_view(comparable(&b, &y, &child_run_specific(name))?, &child(&b)),
			&mut differences,
		);
		// And each stage's recorded keys are the ones its context declared.
		for (which, f, row) in [("first", &a, &x), ("second", &b, &y)] {
			let declared: Vec<&String> = row
				.context
				.value()
				.map(|c| c.environment.keys.iter().collect())
				.unwrap_or_default();
			let recorded: Vec<&String> = f
				.environments
				.get(name)
				.map(|m| m.keys().collect())
				.unwrap_or_default();
			if declared != recorded {
				differences.push(format!("{name}: the {which} final's environment record covers {recorded:?}, its context declares {declared:?}"));
			}
		}
		if name == "polars-suite" {
			match (oracle_report(&a, &x), oracle_report(&b, &y)) {
				(Ok(Some(p)), Ok(Some(q))) => {
					differ("polars-suite oracle report", &p, &q, &mut differences)
				}
				(Ok(None), Ok(None)) => {}
				(p, q) => differences.push(format!(
					"polars-suite oracle report: {:?} / {:?}",
					p.map(|v| v.is_some()).map_err(|e| e.to_string()),
					q.map(|v| v.is_some()).map_err(|e| e.to_string())
				)),
			}
		}
		match (a.semantic(name, &x), b.semantic(name, &y)) {
			(Ok(Some(p)), Ok(Some(q))) => {
				differ(
					&format!("{name} receipt"),
					&comparable(&a, &p, &semantic_run_specific(name))?,
					&comparable(&b, &q, &semantic_run_specific(name))?,
					&mut differences,
				);
				if name == "jupyter-kernel" {
					nested_checks(&a, &p, &b, &q, &mut differences);
				}
				if name == "fixed-route" {
					fixed_route = route_parity(&a, &p, &b, &q, witness.as_ref(), &mut differences);
				}
			}
			(Ok(None), Ok(None)) => {}
			(p, q) => {
				for (which, r) in [("first", p), ("second", q)] {
					match r {
						Err(error) => differences.push(format!("{name}: {which} final: {error:#}")),
						Ok(None) => differences
							.push(format!("{name}: the {which} final has no producer receipt")),
						Ok(Some(_)) => {}
					}
				}
			}
		}
	}
	let mut excluded = Map::new();
	for (receipt, places) in [
		("SELECTION", &SELECTION_RUN_SPECIFIC[..]),
		("RESULT", &RESULT_RUN_SPECIFIC[..]),
		("stage RESULT", &CHILD_RUN_SPECIFIC[..]),
		("producer RESULT", &SEMANTIC_RUN_SPECIFIC[..]),
		("cross producer RESULT, also", &CROSS_RUN_SPECIFIC[..]),
		(
			"fixed-route producer RESULT, also",
			&FIXED_ROUTE_RUN_SPECIFIC[..],
		),
		(
			"fixed-route producer RESULT, checked by the count rule",
			&FIXED_ROUTE_COUNTS[..],
		),
		(
			"jupyter-kernel producer RESULT, also",
			&JUPYTER_RUN_SPECIFIC[..],
		),
	] {
		excluded.insert(receipt.into(), json!(places));
	}
	Ok(
		json!({"status": if differences.is_empty() { "PASS" } else { "FAIL" }, "differences": differences, "stages": names.len(), "run_specific": excluded, "fixed_route": fixed_route, "variable_case": {"case": route::VARIABLE_CASE, "bound": route::VARIATION_BOUND}}),
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cleanup_records_keep_their_count_and_only_their_identifier_is_normalized() {
		let id = |n: u8| format!("<OUT>/s-child/CLEANUP-{}.json", format!("{n:x}").repeat(32));
		let receipt = |keys: &[String]| {
			json!({
				"artifacts": keys.iter().map(|k| (k.clone(), json!("<RUN>"))).collect::<Map<_, _>>(),
				"cleanup": [{"path": "b"}, {"path": "a"}, {"path": "a"}],
			})
		};
		let two = stage_view(receipt(&[id(1), id(2)]), "<OUT>/s-child");
		assert_eq!(
			two["artifacts"].as_array().unwrap().len(),
			2,
			"two records stay two"
		);
		assert_eq!(
			two,
			stage_view(receipt(&[id(3), id(4)]), "<OUT>/s-child"),
			"fresh identifiers alone"
		);
		assert_ne!(
			two,
			stage_view(receipt(&[id(3)]), "<OUT>/s-child"),
			"a removed record"
		);
		assert_ne!(
			two,
			stage_view(receipt(&[id(3), id(4), id(5)]), "<OUT>/s-child"),
			"an added record"
		);
		assert_eq!(
			two["cleanup"].as_array().unwrap().len(),
			3,
			"the cleanup list keeps duplicates"
		);
		// A record in another directory keeps its identifier.
		let elsewhere = format!("<OUT>/other/CLEANUP-{}.json", "a".repeat(32));
		let view = stage_view(receipt(std::slice::from_ref(&elsewhere)), "<OUT>/s-child");
		assert_eq!(view["artifacts"][0][0], json!(elsewhere));
	}
}
