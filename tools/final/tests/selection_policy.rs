//! Ports of the Python selection controls (test_gate_policy.py), against a
//! throwaway record history.
mod common;
use rnx_final::receipt::invocation_id;
use rnx_final::selection::{self, close_prerequisites, finalize_with, select};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Gates(PathBuf, String);

impl Gates {
	fn new() -> Self {
		let root = std::env::temp_dir().join(format!("rnx-final-gates-{}", invocation_id()));
		std::fs::create_dir(&root).unwrap();
		let mut gates = Self(root.canonicalize().unwrap(), String::new());
		gates.git(&["init", "-q"]);
		gates.write(".gitignore", "runs/\n");
		gates.write("src/a.rs", "old");
		gates.commit("baseline");
		gates.1 = gates.git(&["rev-parse", "HEAD"]);
		gates.write("plans/adr/9999.md", "plan");
		gates.commit("plan: adr 9999 controls");
		gates
	}
	fn git(&self, args: &[&str]) -> String {
		let output = std::process::Command::new("git")
			.arg("-C")
			.arg(&self.0)
			.args([
				"-c",
				"user.name=Test",
				"-c",
				"user.email=test@example.invalid",
				"-c",
				"commit.gpgSign=false",
			])
			.args(args)
			.output()
			.unwrap();
		assert!(output.status.success(), "{args:?}");
		String::from_utf8(output.stdout).unwrap().trim().to_owned()
	}
	fn write(&self, name: &str, text: &str) {
		let p = self.0.join(name);
		std::fs::create_dir_all(p.parent().unwrap()).unwrap();
		std::fs::write(p, text).unwrap();
	}
	fn commit(&self, subject: &str) {
		self.git(&["add", "-A"]);
		self.git(&["commit", "-qm", subject]);
	}
	fn runs(&self) -> PathBuf {
		self.0.join("runs")
	}
	/// Tools present, as a final's launch supplies them.
	fn env(&self) -> BTreeMap<String, String> {
		let mut env: BTreeMap<String, String> = std::env::vars().collect();
		let python = self.0.join("runs/python");
		std::fs::create_dir_all(python.parent().unwrap()).unwrap();
		std::fs::write(&python, "").unwrap();
		env.insert(
			"RNX_JUPYTER_PYTHON".into(),
			python.to_string_lossy().into_owned(),
		);
		env.insert(
			"RNX_VALGRIND_ROOT".into(),
			fake_valgrind(&self.0.join("runs"))
				.to_string_lossy()
				.into_owned(),
		);
		env
	}
	fn select(&self, names: Option<&[&str]>, scope: &str) -> anyhow::Result<Value> {
		let names: Option<Vec<String>> = names.map(|n| n.iter().map(|s| s.to_string()).collect());
		select(
			&self.0,
			&self.runs(),
			names.as_deref(),
			"HEAD",
			Some(scope),
			&self.env(),
		)
	}
	fn check(&self, m: &Value) -> anyhow::Result<()> {
		selection::check_selection(m, &self.0, true, &self.env())
	}
}

impl Drop for Gates {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// A Valgrind root as admission sees one: an executable bin/valgrind.
fn fake_valgrind(base: &Path) -> PathBuf {
	use std::os::unix::fs::PermissionsExt as _;
	let root = base.join("valgrind");
	std::fs::create_dir_all(root.join("bin")).unwrap();
	std::fs::write(root.join("bin/valgrind"), "#!/bin/sh\n").unwrap();
	std::fs::set_permissions(
		root.join("bin/valgrind"),
		std::fs::Permissions::from_mode(0o755),
	)
	.unwrap();
	root
}

fn stage_names(m: &Value) -> Vec<String> {
	m["stages"]
		.as_array()
		.unwrap()
		.iter()
		.map(|s| s[0].as_str().unwrap().to_owned())
		.collect()
}

#[test]
fn a_fix_has_its_own_baseline_and_revalidatable_final() {
	use rnx_final::child::{self, Done};
	use rnx_final::controller::{self, Hooks, Settings, StageCall};
	use rnx_final::receipt::{atomic_json, digest};
	use std::sync::Arc;
	use std::time::Instant;

	struct Stages(PathBuf);
	impl Hooks for Stages {
		fn disk_check(
			&self,
			_: &Path,
			_: &Path,
			_: &BTreeMap<String, String>,
		) -> anyhow::Result<()> {
			Ok(())
		}
		fn tools_identity(&self, _: &Settings) -> anyhow::Result<Value> {
			Ok(json!({}))
		}
		fn resources(
			&self,
			name: &str,
			_: &[String],
			_: &Path,
			_: &rnx_final::scheduler::Targets,
		) -> anyhow::Result<rnx_final::scheduler::Resources> {
			Ok([name.to_owned()].into())
		}
		fn stage(&self, call: &StageCall) -> Option<anyhow::Result<Done>> {
			Some((|| {
				std::fs::create_dir(&call.directory)?;
				let receipt = call.directory.join("RESULT.json");
				let mut value = common::stage_receipt(true, 0, json!([]), json!({}));
				value["command"] = json!(call.command);
				value["context"]["command"] = json!(call.command);
				value["context"]["candidate"] = json!(child::head(&self.0));
				value["context"]["source"] = json!(child::source(&self.0)?);
				atomic_json(&receipt, &value)?;
				Ok(Done {
					returncode: 0,
					validation: json!({"path": receipt, "sha256": digest(&receipt)?}),
					declared_output: None,
				})
			})())
		}
	}

	let g = Gates::new();
	assert!(g.check(&g.select(None, "required").unwrap()).is_err());
	g.write(".gitignore", "runs/\ntarget/\n");
	g.write("src/a.rs", "record implementation");
	g.commit("impl: adr 9999 controls");
	let numbered = selection::record(&g.0, "HEAD").unwrap();
	assert_eq!(numbered["baseline"], g.1);
	assert_eq!(numbered["phase"], "impl");

	g.write("README.md", "first fix");
	g.commit("fix: clarify the documentation");
	let parent = g.git(&["rev-parse", "HEAD"]);
	g.write("README.md", "second fix");
	let subject = "fix: clarify the 30 second timeout";
	g.commit(subject);
	let candidate = g.git(&["rev-parse", "HEAD"]);
	let selected = g.select(None, "required").unwrap();
	assert_eq!(selected["record"]["candidate"], candidate);
	assert_eq!(selected["record"]["baseline"], parent);
	assert_eq!(selected["record"]["phase"], "fix");
	assert_eq!(selected["record"]["subject"], subject);
	assert_eq!(selected["record"]["paths"], json!(["README.md"]));
	assert!(selected["record"].get("plan").is_none());
	assert!(selected["record"].get("record").is_none());
	assert!(!stage_names(&selected).iter().any(|n| n == "root-tests"));
	g.check(&selected).unwrap();
	let explicit = g.select(Some(&["commit-discipline"]), "explicit").unwrap();
	assert_eq!(explicit["publication_eligible"], false);
	let mut forged = selected.clone();
	forged["record"]["record"] = json!("9999");
	assert!(g.check(&forged).is_err());

	g.write("dirty.txt", "untracked");
	assert!(g.check(&g.select(None, "required").unwrap()).is_err());
	std::fs::remove_file(g.0.join("dirty.txt")).unwrap();
	let out = g.runs().join("final");
	let result = controller::run(&Settings {
		root: g.0.clone(),
		out: out.clone(),
		names: None,
		scope: "required".into(),
		fresh: false,
		environment: g.env(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: Arc::new(Stages(g.0.clone())),
		tokens: Some(2),
	})
	.unwrap();
	assert_eq!(result["status"], "PASS", "{result}");
	assert_eq!(result["publication_eligible"], true);
	assert_eq!(result["fix_subject"], subject);
	let released = rnx_final::evidence::revalidate(&out).unwrap();
	assert_eq!(released["fix_subject"], subject);

	let receipt = out.join("SELECTION.json");
	let mut selection: Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
	selection["record"]["baseline"] = json!(candidate);
	atomic_json(&receipt, &selection).unwrap();
	assert!(
		rnx_final::evidence::revalidate(&out)
			.unwrap_err()
			.to_string()
			.contains("fix selection differs")
	);
}

#[test]
fn conservative_paths_and_both_rename_sides() {
	let g = Gates::new();
	for name in ["src/new.rs", "tests/a.rs"] {
		let required: BTreeSet<String> = rnx_final::catalogue::required(&[name.into()], None, None)
			.unwrap()
			.into_iter()
			.collect();
		assert!(
			["root-tests", "inventory", "cross"]
				.iter()
				.all(|s| required.contains(*s))
		);
	}
	assert_eq!(
		rnx_final::catalogue::required(&["tools/column/foo.py".into()], None, None).unwrap(),
		["commit-discipline", "controller-controls", "python-list"]
	);
	assert!(rnx_final::catalogue::required(&["Cargo.toml".into()], None, None).is_err());
	std::fs::rename(g.0.join("src/a.rs"), g.0.join("renamed.rs")).unwrap();
	g.commit("impl: adr 9999 controls");
	let record = selection::record(&g.0, "HEAD").unwrap();
	assert_eq!(record["baseline"], g.1);
	let paths: Vec<&str> = record["paths"]
		.as_array()
		.unwrap()
		.iter()
		.map(|p| p.as_str().unwrap())
		.collect();
	assert!(paths.contains(&"src/a.rs") && paths.contains(&"renamed.rs"));
	assert!(
		record["required"]
			.as_array()
			.unwrap()
			.contains(&json!("root-tests"))
	);
}

#[test]
fn cross_is_added_and_omission_or_spoof_is_refused() {
	let g = Gates::new();
	g.write("src/a.rs", "changed");
	g.commit("impl: adr 9999 controls");
	let m = g.select(Some(&["format"]), "explicit").unwrap();
	let names: BTreeSet<String> = stage_names(&m).into_iter().collect();
	assert_eq!(
		names,
		[
			"format",
			"clippy",
			"cross",
			"root-tests",
			"inventory",
			"commit-discipline",
			"controller-controls",
			"python-list"
		]
		.map(String::from)
		.into()
	);
	let cross = m["stages"]
		.as_array()
		.unwrap()
		.iter()
		.find(|s| s[0] == "cross")
		.unwrap()[1]
		.clone();
	let out = cross
		.as_array()
		.unwrap()
		.iter()
		.position(|a| a == "--out")
		.unwrap();
	assert!(Path::new(cross[out + 1].as_str().unwrap()).starts_with(g.0.join("target/rnx-verify")));
	g.check(&m).unwrap();
	for mutation in ["omit", "command", "baseline", "policy", "source"] {
		let mut bad = m.clone();
		match mutation {
			"omit" => bad["stages"]
				.as_array_mut()
				.unwrap()
				.retain(|s| s[0] != "cross"),
			"command" => {
				*bad["stages"]
					.as_array_mut()
					.unwrap()
					.last_mut()
					.unwrap()
					.get_mut(1)
					.unwrap() = json!(["true"])
			}
			"baseline" => bad["record"]["baseline"] = bad["record"]["candidate"].clone(),
			_ => bad[mutation] = json!("bad"),
		}
		assert!(g.check(&bad).is_err(), "{mutation}");
	}
}

#[test]
fn development_is_not_final_and_source_changes_fail() {
	let g = Gates::new();
	let m = g.select(Some(&["format"]), "explicit").unwrap();
	assert!(
		g.check(&m).is_err(),
		"a plan commit is not a final's candidate"
	);
	g.write("tools/a.py", "a");
	g.commit("impl: adr 9999 controls");
	let m = g.select(Some(&["format"]), "explicit").unwrap();
	g.write("src/a.rs", "dirty");
	assert!(g.check(&m).is_err());
}

#[test]
fn missing_wrong_command_and_nonproducer_cross_are_refused() {
	let g = Gates::new();
	g.write("src/a.rs", "changed");
	g.commit("impl: adr 9999 controls");
	let m = g.select(Some(&["cross"]), "explicit").unwrap();
	assert!(selection::finalize(&m, &Map::new(), &g.0, &g.env()).is_err());
	let evidence: Map<String, Value> = stage_names(&m)
		.into_iter()
		.map(|n| (n, json!({})))
		.collect();
	let rows = |kind: &str| -> Mutex<Vec<Value>> {
		Mutex::new(m["stages"].as_array().unwrap().iter().map(|s| json!({"command": s[1], "context": {"command": s[1], "kind": kind, "candidate": m["record"]["candidate"]}})).collect())
	};
	let good = rows("producer");
	let check =
		|_: &Value, _: &Value| -> anyhow::Result<Value> { Ok(good.lock().unwrap().remove(0)) };
	assert_eq!(
		finalize_with(&m, &evidence, &g.0, &g.env(), &check).unwrap()["status"],
		"PASS"
	);
	let leaf = rows("leaf");
	let check =
		|_: &Value, _: &Value| -> anyhow::Result<Value> { Ok(leaf.lock().unwrap().remove(0)) };
	assert!(finalize_with(&m, &evidence, &g.0, &g.env(), &check).is_err());
}

#[test]
fn a_consumer_selection_closes_over_its_producers() {
	let g = Gates::new();
	g.write("tools/a.py", "a");
	g.commit("impl: adr 9999 controls");
	let names = stage_names(&g.select(Some(&["polars-suite"]), "explicit").unwrap());
	assert!(
		names.iter().position(|n| n == "polars-inventories")
			< names.iter().position(|n| n == "polars-suite")
	);
	let commands: Vec<(String, Vec<String>)> =
		rnx_final::catalogue::commands(&g.runs(), &g.0).unwrap();
	let closed =
		close_prerequisites(&commands, ["production-cli-controls".to_string()].into()).unwrap();
	assert!(closed.contains("production-cli-build"));
}

#[test]
fn the_docs_scope_selects_only_the_stages_that_read_plans() {
	let g = Gates::new();
	g.write("adapters/polars/src/example.rs", "change");
	g.commit("impl: adr 9999 controls");
	assert!(
		g.select(None, "required").unwrap()["stages"]
			.as_array()
			.unwrap()
			.len() > rnx_final::catalogue::READS_PLANS.len()
	);
	let docs = g.select(None, "docs").unwrap();
	assert_eq!(stage_names(&docs), rnx_final::catalogue::READS_PLANS);
	assert_eq!(docs["publication_eligible"], false);
	g.check(&docs).unwrap();
}

#[test]
fn required_complete_and_explicit_scopes() {
	let g = Gates::new();
	g.write("adapters/polars/src/example.rs", "change");
	g.commit("impl: adr 9999 controls");
	let required = g.select(None, "required").unwrap();
	let names = stage_names(&required);
	assert!(names.contains(&"polars-inventories".to_string()));
	let refs: Vec<&str> = names.iter().map(String::as_str).collect();
	let explicit = g.select(Some(&refs), "explicit").unwrap();
	assert_eq!(explicit["stages"], required["stages"]);
	assert_eq!(
		(
			explicit["publication_eligible"].clone(),
			required["publication_eligible"].clone()
		),
		(json!(false), json!(true))
	);
	let complete = g.select(None, "complete").unwrap();
	let catalogue: Vec<Value> = rnx_final::catalogue::commands(&g.runs(), &g.0)
		.unwrap()
		.into_iter()
		.map(|(n, c)| json!([n, c]))
		.collect();
	assert_eq!(complete["stages"], json!(catalogue));
	for value in [&required, &explicit, &complete] {
		g.check(value).unwrap();
	}
	let mut bad = required.clone();
	bad["scope"] = json!("explicit");
	assert!(g.check(&bad).is_err());
	let mut bad = required.clone();
	bad["stages"]
		.as_array_mut()
		.unwrap()
		.push(json!(["extra", ["extra"]]));
	assert!(g.check(&bad).is_err());
	assert!(g.select(Some(&refs), "required").is_err());
}

#[test]
fn a_missing_or_late_producer_fails_closed() {
	// The real table: polars-suite needs polars-inventories before it.
	let only_consumer = vec![("polars-suite".to_string(), vec![])];
	assert!(
		close_prerequisites(&only_consumer, ["polars-suite".to_string()].into())
			.unwrap_err()
			.to_string()
			.contains("missing producer")
	);
	let late = vec![
		("polars-suite".to_string(), vec![]),
		("polars-inventories".to_string(), vec![]),
	];
	assert!(
		close_prerequisites(&late, ["polars-suite".to_string()].into())
			.unwrap_err()
			.to_string()
			.contains("producer must precede consumer")
	);
}

#[test]
fn bulk_assignments_cover_producer_calls() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let commands = rnx_final::catalogue::commands(&root.join("target/unused"), &root).unwrap();
	let assigned = regex::Regex::new(r#"assigned_root\(\s*'(RNX_FINAL_[A-Z_]+)'"#).unwrap();
	for (name, command) in &commands {
		let Some(script) = command.iter().find(|a| a.ends_with(".py")) else {
			continue;
		};
		let text = std::fs::read_to_string(root.join(script)).unwrap_or_default();
		let variables: BTreeSet<String> = assigned
			.captures_iter(&text)
			.map(|c| c[1].to_owned())
			.collect();
		let bulk = rnx_final::catalogue::bulk_roots(name);
		for variable in variables {
			let kind = match variable.as_str() {
				"RNX_FINAL_BASELINE_ROOT" => rnx_final::catalogue::Bulk::Baseline,
				"RNX_FINAL_FIXED_ROOT" => rnx_final::catalogue::Bulk::Fixed,
				_ => continue,
			};
			assert!(
				bulk.contains(&kind),
				"{name} reads {variable} without its bulk root"
			);
		}
	}
	assert!(
		rnx_final::catalogue::bulk_roots("generated-projects")
			.contains(&rnx_final::catalogue::Bulk::Project)
	);
	assert!(rnx_final::catalogue::bulk_roots("cross").contains(&rnx_final::catalogue::Bulk::Cross));
}

#[test]
fn prerequisites_follow_the_selected_stages() {
	let g = Gates::new();
	g.write("adapters/polars/src/example.rs", "change");
	g.commit("impl: adr 9999 controls");
	let mut env = g.env();
	env.insert(
		"RNX_JUPYTER_PYTHON".into(),
		g.0.join("absent").to_string_lossy().into_owned(),
	);
	env.insert(
		"RNX_VALGRIND_ROOT".into(),
		g.0.join("absent").to_string_lossy().into_owned(),
	);
	let docs = select(&g.0, &g.runs(), None, "HEAD", Some("docs"), &env).unwrap();
	assert_eq!(stage_names(&docs), ["commit-discipline"]);
	let kernel = select(
		&g.0,
		&g.runs(),
		Some(&["jupyter-kernel".to_string()]),
		"HEAD",
		Some("explicit"),
		&env,
	)
	.unwrap_err();
	assert!(
		kernel.to_string().contains("RNX_JUPYTER_PYTHON"),
		"{kernel}"
	);
	let probe = select(
		&g.0,
		&g.runs(),
		Some(&["package-column-probe".to_string()]),
		"HEAD",
		Some("explicit"),
		&env,
	)
	.unwrap_err();
	assert!(probe.to_string().contains("Valgrind"), "{probe}");
}

/// Every stage finalize requires to be a producer runs a registered producer
/// program, so no complete final can pass every stage and still be refused
/// at finalization (ADR 0283 named a direct test stage `package-*`).
#[test]
fn every_stage_that_must_be_a_producer_runs_one() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	for (name, command) in
		rnx_final::catalogue::commands(&root.join("target/unused"), &root).unwrap()
	{
		if selection::requires_producer(&name) {
			let kind = rnx_final::child::contract(&command, &root, &root).unwrap();
			assert_eq!(
				kind, "producer",
				"{name} must be a producer but runs {command:?}"
			);
		}
	}
	// The Python stage protocol, which checks a Python producer's receipt
	// at run time, classifies each Python-run one as a producer too; the
	// Rust stage modes run under the Rust protocol alone.
	let required: Vec<serde_json::Value> =
		rnx_final::catalogue::commands(&root.join("target/unused"), &root)
			.unwrap()
			.into_iter()
			.filter(|(name, command)| selection::requires_producer(name) && command[0] == "python3")
			.map(|(name, command)| serde_json::json!([name, command]))
			.collect();
	// None: cross (ADR 0294) and fixed-route (ADR 0298) run as Rust stage
	// modes.
	assert!(
		required.is_empty(),
		"{} Python producer stages",
		required.len()
	);
	let script = "import json, sys\nfrom pathlib import Path\nimport child_result as c\nroot = Path(sys.argv[1])\nprint(json.dumps([n for n, command in json.loads(sys.argv[2]) if c.contract(command, root, root) != 'producer']))";
	let output = std::process::Command::new("python3")
		.args(["-B", "-c", script])
		.arg(&root)
		.arg(serde_json::Value::Array(required).to_string())
		.current_dir(root.join("tools/column"))
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
}

/// The finalizer holds the suite's native contract cases to the manifest,
/// then goes on to check the stages after it (formerly the Python control
/// test_native_contracts.Coverage, which finalized through gate_policy).
#[test]
fn the_finalizer_checks_native_contracts_and_the_stages_after_them() {
	let g = Gates::new();
	let real = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.join(selection::NATIVE_MANIFEST);
	let text = std::fs::read_to_string(&real).unwrap();
	g.write(selection::NATIVE_MANIFEST, &text);
	g.write("src/a.rs", "changed");
	g.commit("impl: adr 9999 controls");
	let m = g
		.select(Some(&["rune-suite", "fixed-route"]), "explicit")
		.unwrap();
	let names = stage_names(&m);
	let suite = names.iter().position(|n| n == "rune-suite").unwrap();
	assert!(
		suite + 1 < names.len(),
		"a stage follows the suite: {names:?}"
	);
	let manifest: Value = serde_json::from_str(&text).unwrap();
	let cases: Vec<Value> = manifest["cases"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| {
			json!({"name": c["name"], "specification": rnx_final::pyjson::identity(c).unwrap(), "status": "PASS", "exit": 0,
				"selected": [c["selector"]], "passed": 1, "contract_rows": c.get("rows")})
		})
		.collect();
	let receipt = std::env::temp_dir().join(format!("rnx-final-suite-{}.json", invocation_id()));
	let evidence: Map<String, Value> = names.iter().map(|n| (n.clone(), json!({}))).collect();
	for missing in [false, true] {
		let shown = if missing {
			&cases[..cases.len() - 1]
		} else {
			&cases[..]
		};
		std::fs::write(
			&receipt,
			json!({"contracts": {"manifest_sha256": rnx_final::receipt::digest(&g.0.join(selection::NATIVE_MANIFEST)).unwrap(), "cases": shown}}).to_string(),
		)
		.unwrap();
		let rows: Mutex<Vec<Value>> = Mutex::new(
			m["stages"]
				.as_array()
				.unwrap()
				.iter()
				.map(|s| json!({"command": s[1], "context": {"command": s[1], "kind": "producer", "candidate": m["record"]["candidate"]}, "producer": {"receipt": receipt}}))
				.collect(),
		);
		let check =
			|_: &Value, _: &Value| -> anyhow::Result<Value> { Ok(rows.lock().unwrap().remove(0)) };
		let result = finalize_with(&m, &evidence, &g.0, &g.env(), &check);
		if missing {
			let error = result.unwrap_err().to_string();
			assert!(error.contains("catalogue differs"), "{error}");
		} else {
			let result = result.unwrap();
			assert_eq!(result["status"], "PASS");
			assert!(rows.lock().unwrap().is_empty(), "every stage was checked");
			assert!(result["durations"].get(names.last().unwrap()).is_some());
		}
	}
	let _ = std::fs::remove_file(&receipt);
}
