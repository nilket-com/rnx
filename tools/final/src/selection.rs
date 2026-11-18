//! Record-bound selection: which stages a final runs, checked again
//! before each stage and at finalization.
use crate::catalogue::{self, Graph};
use crate::child::{self, value_error};
use crate::evidence::publication_eligible;
use crate::pyjson;
use crate::receipt::digest;
use crate::scheduler;
use anyhow::{Context as _, Result, bail, ensure};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
	let output = crate::child::git_command()
		.arg("-C")
		.arg(root)
		.args(arguments)
		.output()?;
	ensure!(
		output.status.success(),
		"git {arguments:?}: {}",
		String::from_utf8_lossy(&output.stderr).trim()
	);
	Ok(output.stdout)
}

fn line(root: &Path, arguments: &[&str]) -> Result<String> {
	Ok(String::from_utf8(git(root, arguments)?)?.trim().to_owned())
}

fn nul_paths(bytes: &[u8]) -> Result<Vec<String>> {
	bytes
		.split(|b| *b == 0)
		.filter(|p| !p.is_empty())
		.map(|p| {
			Ok(std::str::from_utf8(p)
				.context("a path is not UTF-8")?
				.to_owned())
		})
		.collect()
}

/// Every package manifest at either revision, outside the archived probes.
pub fn package_manifests(root: &Path, revisions: &[&str]) -> Result<Vec<String>> {
	let mut set: BTreeSet<String> = ["Cargo.toml".to_string()].into();
	for revision in revisions {
		for p in nul_paths(&git(
			root,
			&["ls-tree", "-r", "--name-only", "-z", revision],
		)?)? {
			if (p == "Cargo.toml" || p.ends_with("/Cargo.toml")) && !p.starts_with("probes/") {
				set.insert(p);
			}
		}
	}
	Ok(set.into_iter().collect())
}

/// Cargo's path-dependency graph at `revision`, from an archive of it.
pub fn graph(root: &Path, revision: &str) -> Result<Graph> {
	let directory = std::env::temp_dir().join(format!(
		"required-graph-{}",
		crate::receipt::invocation_id()
	));
	std::fs::create_dir(&directory)?;
	let result = (|| -> Result<Graph> {
		let archive = crate::child::git_command()
			.arg("-C")
			.arg(root)
			.args(["archive", revision])
			.output()?;
		ensure!(archive.status.success(), "git archive {revision} failed");
		let mut tar = std::process::Command::new("tar")
			.args(["-x", "-C"])
			.arg(&directory)
			.stdin(std::process::Stdio::piped())
			.spawn()?;
		std::io::Write::write_all(tar.stdin.as_mut().context("tar stdin")?, &archive.stdout)?;
		drop(tar.stdin.take());
		ensure!(tar.wait()?.success(), "tar failed");
		let base = std::fs::canonicalize(&directory)?;
		let mut manifests = Vec::new();
		collect_manifests(&base, &base, &mut manifests)?;
		manifests.sort();
		let mut nodes = Graph::new();
		for relative in manifests {
			if relative.starts_with("probes/") || nodes.contains_key(&relative) {
				continue;
			}
			let output = std::process::Command::new("cargo")
				.args([
					"+1.98.1",
					"metadata",
					"--offline",
					"--no-deps",
					"--format-version=1",
					"--manifest-path",
				])
				.arg(base.join(&relative))
				.current_dir(&base)
				.output()?;
			ensure!(
				output.status.success(),
				"cargo metadata {relative}: {}",
				String::from_utf8_lossy(&output.stderr).trim()
			);
			let data: Value = serde_json::from_slice(&output.stdout)?;
			for package in data["packages"].as_array().into_iter().flatten() {
				let path =
					std::fs::canonicalize(package["manifest_path"].as_str().unwrap_or_default())?;
				let key = path.strip_prefix(&base)?.to_string_lossy().into_owned();
				let mut deps = BTreeSet::new();
				for dep in package["dependencies"].as_array().into_iter().flatten() {
					if let Some(dep_path) = dep.get("path").and_then(Value::as_str) {
						let p = lexical(&Path::new(dep_path).join("Cargo.toml"));
						let Ok(relative) = p.strip_prefix(&base) else {
							bail!("external graph input")
						};
						deps.insert(relative.to_string_lossy().into_owned());
					}
				}
				nodes.insert(key, deps.into_iter().collect());
			}
		}
		Ok(nodes)
	})();
	let _ = std::fs::remove_dir_all(&directory);
	result
}

fn lexical(path: &Path) -> PathBuf {
	std::fs::canonicalize(path).unwrap_or_else(|_| match (path.parent(), path.file_name()) {
		(Some(parent), Some(name)) => lexical(parent).join(name),
		_ => path.to_owned(),
	})
}

/// Python's rglob order is irrelevant here: the list is sorted after.
fn collect_manifests(base: &Path, directory: &Path, out: &mut Vec<String>) -> Result<()> {
	for entry in std::fs::read_dir(directory)?.flatten() {
		let path = entry.path();
		let kind = entry.file_type()?;
		if kind.is_dir() {
			collect_manifests(base, &path, out)?;
		} else if entry.file_name() == "Cargo.toml" {
			out.push(path.strip_prefix(base)?.to_string_lossy().into_owned());
		}
	}
	Ok(())
}

pub(crate) fn fix_subject(subject: &str) -> bool {
	use std::sync::LazyLock;
	static FORM: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"\Afix: [a-z0-9][ -~]*\z").unwrap());
	static LABELS: LazyLock<Regex> = LazyLock::new(|| {
		Regex::new(
			r"(?i)\b(?:cut [a-z][0-9]*|e[0-9]+(?:[._-][0-9]+)*|(?:crypto|pd|f-own|f-ctr)[-_][0-9]+)\b",
		)
		.unwrap()
	});
	static ADR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\Afix: adr [0-9]{4}\b").unwrap());
	FORM.is_match(subject)
		&& !subject.contains(['(', ')'])
		&& !LABELS.is_match(subject)
		&& !ADR.is_match(subject)
}

/// The candidate, its baseline and the stages its changed paths require.
pub fn record(root: &Path, candidate: &str) -> Result<Value> {
	use crate::types::{Candidate, Fix, FixPhase, Phase, Record};

	let candidate = line(
		root,
		&["rev-parse", "--verify", &format!("{candidate}^{{commit}}")],
	)?;
	let subject = line(root, &["show", "-s", "--format=%s", &candidate])?;
	let pattern = Regex::new(r"^(plan|impl): adr ([0-9]{4}) .+$").unwrap();
	let (baseline, numbered) = if fix_subject(&subject) {
		let parents = line(root, &["show", "-s", "--format=%P", &candidate])?;
		let mut parents = parents.split_whitespace();
		let parent = parents
			.next()
			.filter(|_| parents.next().is_none())
			.ok_or_else(|| value_error("a fix candidate must have a single parent"))?;
		let previous = line(root, &["show", "-s", "--format=%s", parent])?;
		if !fix_subject(&previous) && !pattern.captures(&previous).is_some_and(|m| &m[1] == "impl")
		{
			return Err(value_error(
				"fix must follow a closed record or another fix",
			));
		}
		(parent.to_owned(), None)
	} else {
		let m = pattern
			.captures(&subject)
			.ok_or_else(|| value_error("candidate is neither a record nor a fix commit"))?;
		let number = m[2].to_owned();
		let (phase, plan) = if &m[1] == "plan" {
			(Phase::Plan, candidate.clone())
		} else {
			(
				Phase::Impl,
				line(root, &["rev-parse", &format!("{candidate}^")])?,
			)
		};
		let plan_subject = line(root, &["show", "-s", "--format=%s", &plan])?;
		if !plan_subject.starts_with(&format!("plan: adr {number} ")) {
			return Err(value_error(
				"candidate does not immediately follow its plan",
			));
		}
		(
			line(root, &["rev-parse", &format!("{plan}^")])?,
			Some((plan, number, phase)),
		)
	};
	// No rename folding: both removed and added names count.
	let paths = nul_paths(&git(
		root,
		&[
			"diff",
			"--no-renames",
			"--name-only",
			"-z",
			&baseline,
			&candidate,
		],
	)?)?;
	if numbered.is_none()
		&& paths.iter().any(|p| {
			["plans", "rune/plans"]
				.iter()
				.any(|root| Path::new(p).starts_with(root))
		}) {
		return Err(value_error("fix changes a protected plan path"));
	}
	let graphs = if catalogue::graph_change(&paths) {
		Some(vec![graph(root, &baseline)?, graph(root, &candidate)?])
	} else {
		None
	};
	let manifests = package_manifests(root, &[&baseline, &candidate])?;
	let required = catalogue::required(&paths, graphs.as_deref(), Some(&manifests))?;
	let info = match numbered {
		Some((plan, record, phase)) => Candidate::Numbered(Record {
			candidate,
			baseline,
			plan,
			record,
			phase,
			paths,
			required,
		}),
		None => Candidate::Fix(Fix {
			candidate,
			baseline,
			subject,
			phase: FixPhase::Fix,
			paths,
			required,
		}),
	};
	Ok(serde_json::to_value(info)?)
}

/// The controller's policy identity: the digest of its own source.
pub fn policy_digest() -> Result<String> {
	let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
	let mut rows = Map::new();
	let mut names: Vec<PathBuf> = std::fs::read_dir(&source)
		.map(|d| d.flatten().map(|e| e.path()).collect())
		.unwrap_or_default();
	names.sort();
	for path in names {
		rows.insert(
			path.file_name()
				.unwrap_or_default()
				.to_string_lossy()
				.into_owned(),
			json!(digest(&path)?),
		);
	}
	pyjson::identity(&Value::Object(rows))
}

/// Add every declared producer of a selected stage, transitively.
pub fn close_prerequisites(
	commands: &[catalogue::Stage],
	selected: BTreeSet<String>,
) -> Result<BTreeSet<String>> {
	let order: BTreeMap<&str, usize> = commands
		.iter()
		.enumerate()
		.map(|(i, (n, _))| (n.as_str(), i))
		.collect();
	ensure!(order.len() == commands.len(), "duplicate stage definition");
	let mut selected = selected;
	let mut pending: Vec<String> = selected.iter().cloned().collect();
	while let Some(consumer) = pending.pop() {
		let Some(&position) = order.get(consumer.as_str()) else {
			return Err(value_error(&format!("missing stage: {consumer}")));
		};
		for producer in catalogue::prerequisites(&consumer) {
			let Some(&at) = order.get(producer) else {
				return Err(value_error(&format!(
					"{consumer}: missing producer {producer}"
				)));
			};
			if at >= position {
				return Err(value_error(&format!(
					"{consumer}: producer must precede consumer: {producer}"
				)));
			}
			if selected.insert(producer.to_string()) {
				pending.push(producer.to_string());
			}
		}
	}
	Ok(selected)
}

pub fn valgrind_root(environment: &BTreeMap<String, String>) -> Result<PathBuf> {
	let root = match environment
		.get("RNX_VALGRIND_ROOT")
		.filter(|v| !v.is_empty())
	{
		Some(configured) => lexical(Path::new(configured)),
		None => {
			let path = environment.get("PATH").cloned().unwrap_or_default();
			let binary = path
				.split(':')
				.map(|d| Path::new(d).join("valgrind"))
				.find(|p| p.is_file())
				.ok_or_else(|| {
					value_error("Valgrind missing: set RNX_VALGRIND_ROOT or put valgrind on PATH")
				})?;
			std::fs::canonicalize(binary)?
				.parent()
				.and_then(Path::parent)
				.map(Path::to_owned)
				.unwrap_or_default()
		}
	};
	let binary = root.join("bin/valgrind");
	let executable = std::fs::metadata(&binary).is_ok_and(|m| m.is_file())
		&& std::ffi::CString::new(binary.as_os_str().as_encoded_bytes())
			.is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0);
	if !executable {
		return Err(value_error(
			"Valgrind root must contain executable bin/valgrind",
		));
	}
	Ok(root)
}

pub fn client_python(root: &Path, environment: &BTreeMap<String, String>) -> Result<PathBuf> {
	let python = environment
		.get("RNX_JUPYTER_PYTHON")
		.map(PathBuf::from)
		.unwrap_or_else(|| root.join("target/jupyter-client/bin/python"));
	if !python.is_file() {
		return Err(value_error(
			"set RNX_JUPYTER_PYTHON to the pinned Jupyter client environment",
		));
	}
	Ok(python)
}

/// The selection a final runs, as SELECTION.json records it.
pub fn select(
	root: &Path,
	out: &Path,
	names: Option<&[String]>,
	candidate: &str,
	scope: Option<&str>,
	environment: &BTreeMap<String, String>,
) -> Result<Value> {
	let root = std::fs::canonicalize(root)?;
	let out = lexical(out);
	let info = record(&root, candidate)?;
	let commands = catalogue::commands(&out, &root)?;
	let scope = scope.unwrap_or(if names.is_none() {
		"complete"
	} else {
		"explicit"
	});
	publication_eligible(scope)?;
	if scope != "explicit" && names.is_some() {
		return Err(value_error("named list requires explicit scope"));
	}
	let names: Vec<String> = match scope {
		"complete" => commands.iter().map(|(n, _)| n.clone()).collect(),
		"required" => vec![],
		"docs" => catalogue::READS_PLANS
			.iter()
			.map(|s| s.to_string())
			.collect(),
		_ => names.map(<[String]>::to_vec).unwrap_or_default(),
	};
	let unique: BTreeSet<String> = names.iter().cloned().collect();
	if unique.len() != names.len() {
		return Err(value_error("duplicate stage selection"));
	}
	if unique.iter().any(|n| !commands.iter().any(|(c, _)| c == n)) {
		return Err(value_error("unknown selected stage"));
	}
	// A docs successor's diff touches only plans, so the record's required
	// stages are already proved by the final it extends.
	let mut wanted = unique;
	if scope != "docs" {
		wanted.extend(
			info["required"]
				.as_array()
				.into_iter()
				.flatten()
				.filter_map(Value::as_str)
				.map(str::to_owned),
		);
	}
	let selected = close_prerequisites(&commands, wanted)?;
	// A missing tool is refused before any expensive stage, and only for
	// the stages that use it.
	if [
		"fixed-route",
		"package-column-probe",
		"lint-package-column-probe",
	]
	.iter()
	.any(|s| selected.contains(*s))
	{
		valgrind_root(environment)?;
	}
	if selected.contains("jupyter-kernel") {
		client_python(&root, environment)?;
	}
	for (name, command) in &commands {
		if selected.contains(name) {
			scheduler::declared(name, command)?;
		}
	}
	let stages: Vec<Value> = commands
		.iter()
		.filter(|(n, _)| selected.contains(n))
		.map(|(n, c)| json!([n, c]))
		.collect();
	Ok(json!({
		"schema": 1, "scope": scope, "publication_eligible": publication_eligible(scope)?, "record": info, "policy": policy_digest()?,
		"source": child::source(&root)?, "root": root.to_string_lossy(), "out": out.to_string_lossy(), "stages": stages,
	}))
}

/// The selection still holds: the same stages, source and policy, at the
/// checkout's commit; in a final, on a clean implementation commit.
pub fn check_selection(
	manifest: &Value,
	root: &Path,
	final_: bool,
	environment: &BTreeMap<String, String>,
) -> Result<()> {
	if manifest.get("schema") != Some(&json!(1)) {
		return Err(value_error("unknown selection schema"));
	}
	let selection: crate::types::Selection = crate::types::conform(manifest, "selection")?;
	let scope = manifest["scope"].as_str().unwrap_or_default();
	let names: Option<Vec<String>> = (scope == "explicit").then(|| {
		manifest["stages"]
			.as_array()
			.into_iter()
			.flatten()
			.filter_map(|s| s[0].as_str().map(str::to_owned))
			.collect()
	});
	let candidate = manifest["record"]["candidate"].as_str().unwrap_or_default();
	let actual = select(
		root,
		Path::new(manifest["out"].as_str().unwrap_or_default()),
		names.as_deref(),
		candidate,
		Some(scope),
		environment,
	)?;
	if actual != *manifest {
		return Err(value_error(
			"selection, source, policy or required gates changed",
		));
	}
	if line(root, &["rev-parse", "HEAD"])? != candidate {
		return Err(value_error("selection is not for checkout candidate"));
	}
	if final_
		&& (!selection.record.is_publishable()
			|| !git(root, &["status", "--porcelain", "-z"])?.is_empty())
	{
		return Err(value_error(
			"final acceptance requires a clean implementation or fix commit",
		));
	}
	Ok(())
}

/// The stages whose evidence must be their own program's bound receipt,
/// not only an exit status.
pub fn requires_producer(name: &str) -> bool {
	catalogue::formats().iter().any(|(f, _)| f == name)
		|| catalogue::lints().iter().any(|(l, _)| l == name)
		|| [
			"polars-inventories",
			"cross",
			"fixed-route",
			"rune-suite",
			"rune-clippy",
			"project-suite",
			"project-clippy",
		]
		.contains(&name)
		|| ["suite-adapter-", "clippy-adapter-", "package-"]
			.iter()
			.any(|p| name.starts_with(p))
}

pub const NATIVE_MANIFEST: &str = "rune/tools/xtask/manifests/native.json";

pub fn native_manifest(root: &Path) -> Result<Value> {
	let m: Value = serde_json::from_slice(&std::fs::read(root.join(NATIVE_MANIFEST))?)?;
	let cases = m["cases"].as_array().cloned().unwrap_or_default();
	if m["version"] != 1 || cases.is_empty() {
		return Err(value_error("empty or unknown native manifest"));
	}
	let names: BTreeSet<&str> = cases.iter().filter_map(|c| c["name"].as_str()).collect();
	if names.len() != cases.len() {
		return Err(value_error("duplicate manifest case"));
	}
	for c in &cases {
		let name = c["name"].as_str().unwrap_or_default();
		let timeout_bad = c["timeout"].as_f64().is_none_or(|t| t <= 0.0);
		if !crate::failure::truthy(&c["selector"])
			|| timeout_bad
			|| name.is_empty()
			|| name == "."
			|| name == ".."
			|| name.contains('/')
		{
			return Err(value_error("invalid manifest case"));
		}
	}
	Ok(m)
}

/// A successful suite cannot conceal a missing or differently selected case.
pub fn catalogue_check(m: &Value, rows: &[Value]) -> Result<Value> {
	let expected: BTreeMap<String, &Value> = m["cases"]
		.as_array()
		.into_iter()
		.flatten()
		.map(|c| (c["name"].as_str().unwrap_or_default().to_owned(), c))
		.collect();
	let actual: BTreeMap<String, &Value> = rows
		.iter()
		.map(|r| (r["name"].as_str().unwrap_or_default().to_owned(), r))
		.collect();
	if actual.len() != rows.len() || !actual.keys().eq(expected.keys()) {
		let missing: Vec<&String> = expected
			.keys()
			.filter(|k| !actual.contains_key(*k))
			.collect();
		let extra: Vec<&String> = actual
			.keys()
			.filter(|k| !expected.contains_key(*k))
			.collect();
		return Err(value_error(&format!(
			"native contract catalogue differs: missing={} extra={}",
			crate::failure::python_repr(&json!(missing)),
			crate::failure::python_repr(&json!(extra))
		)));
	}
	for (name, spec) in &expected {
		let r = actual[name];
		if r.get("specification") != Some(&json!(pyjson::identity(spec)?)) {
			return Err(value_error(&format!("{name}: manifest case changed")));
		}
		if r.get("status") != Some(&json!("PASS")) || r.get("exit") != Some(&json!(0)) {
			let error = r
				.get("error")
				.and_then(Value::as_str)
				.unwrap_or("missing successful execution");
			return Err(value_error(&format!("{name}: case failed\n{error}")));
		}
		let selected = r.get("selected").and_then(Value::as_array);
		if !selected.is_some_and(|s| !s.is_empty())
			|| r.get("passed") != Some(&json!(selected.map_or(0, Vec::len)))
		{
			return Err(value_error(&format!("{name}: empty or partial execution")));
		}
		if crate::failure::truthy(&spec["exact"]) && r["selected"] != json!([spec["selector"]]) {
			return Err(value_error(&format!("{name}: exact selector differs")));
		}
		if !spec.get("rows").is_none_or(Value::is_null)
			&& r.get("contract_rows") != spec.get("rows")
		{
			return Err(value_error(&format!("{name}: contract row total differs")));
		}
	}
	let cases = m["cases"].as_array().cloned().unwrap_or_default();
	let selectors: BTreeSet<String> = cases.iter().map(|c| c["selector"].to_string()).collect();
	let ignored: BTreeSet<String> = cases
		.iter()
		.filter(|c| crate::failure::truthy(&c["ignored"]))
		.map(|c| c["selector"].to_string())
		.collect();
	Ok(
		json!({"status": "PASS", "cases": rows.len(), "selectors": selectors.len(), "ignored_selectors": ignored.len()}),
	)
}

/// Every selected stage's evidence, checked once more against the selection.
pub fn finalize(
	manifest: &Value,
	evidence: &Map<String, Value>,
	root: &Path,
	environment: &BTreeMap<String, String>,
) -> Result<Value> {
	finalize_with(manifest, evidence, root, environment, &child::evidence)
}

/// `finalize` with the stage evidence check given, as a control replaces it.
pub fn finalize_with(
	manifest: &Value,
	evidence: &Map<String, Value>,
	root: &Path,
	environment: &BTreeMap<String, String>,
	check: &dyn Fn(&Value, &Value) -> Result<Value>,
) -> Result<Value> {
	check_selection(manifest, root, true, environment)?;
	let names: BTreeSet<String> = manifest["stages"]
		.as_array()
		.into_iter()
		.flatten()
		.filter_map(|s| s[0].as_str().map(str::to_owned))
		.collect();
	if evidence.keys().cloned().collect::<BTreeSet<_>>() != names {
		return Err(value_error("missing or extraneous stage evidence"));
	}
	let mut durations = Map::new();
	for stage in manifest["stages"].as_array().into_iter().flatten() {
		let (name, command) = (stage[0].as_str().unwrap_or_default(), &stage[1]);
		let row = check(&evidence[name], &manifest["source"])?;
		durations.insert(
			name.into(),
			json!({"seconds": row.get("seconds"), "cleanup_seconds": row.get("cleanup_seconds")}),
		);
		if row["context"].get("candidate") != Some(&manifest["record"]["candidate"]) {
			return Err(value_error(&format!("stage candidate differs: {name}")));
		}
		if row["command"] != *command || row["context"]["command"] != *command {
			return Err(value_error(&format!("stage command differs: {name}")));
		}
		let producer_stage = requires_producer(name);
		if producer_stage && row["context"]["kind"] != "producer" {
			return Err(value_error(&format!(
				"stage requires producer receipt: {name}"
			)));
		}
		if name == "rune-suite" {
			let semantic: Value = serde_json::from_slice(&std::fs::read(
				row["producer"]["receipt"].as_str().unwrap_or_default(),
			)?)?;
			let contracts = &semantic["contracts"];
			if contracts.get("manifest_sha256").and_then(Value::as_str)
				!= Some(digest(&root.join(NATIVE_MANIFEST))?.as_str())
			{
				return Err(value_error("native contract manifest changed"));
			}
			catalogue_check(
				&native_manifest(root)?,
				contracts["cases"]
					.as_array()
					.map(Vec::as_slice)
					.unwrap_or_default(),
			)?;
		}
	}
	Ok(json!({
		"schema": 1, "status": "PASS", "candidate": manifest["record"]["candidate"], "scope": manifest["scope"],
		"publication_eligible": publication_eligible(manifest["scope"].as_str().unwrap_or_default())?, "source": manifest["source"],
		"selection": pyjson::identity(manifest)?, "evidence": evidence, "durations": durations,
	}))
}
