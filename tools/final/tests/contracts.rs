//! `rune-suite` and its native contract cases in Rust (ADR 0289), against
//! the Python stage programs they replace at the record's witness
//! revision: the recipes and twins, each case's children, environment and
//! row, the manifest and catalogue verdicts, the build, and the receipt;
//! then the token, process-group and parser controls.
use anyhow::Result;
use job_budget::fifo::Pool;
use rnx_final::contracts::{self, Call, Process};
use rnx_final::failure::{Environment, python_repr};
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::stage::{self, Invoke};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const WITNESS_0289: &str = "837f2b5d4fbd23f123d4a0a607872b21bf584012";

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

/// A fixture of this record, generated at its own witness revision.
fn fixture(name: &str, substitutions: &[(&str, &str)]) -> Value {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("tests/witness")
		.join(format!("{name}.json"));
	let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
	assert_eq!(value["witness"], WITNESS_0289, "{name}");
	assert!(
		value["method"].as_str().is_some_and(|m| m.len() > 40),
		"{name}"
	);
	let mut text = value["expected"].to_string();
	for (mark, real) in substitutions {
		let escaped = serde_json::to_string(real).unwrap();
		text = text.replace(mark, &escaped[1..escaped.len() - 1]);
	}
	serde_json::from_str(&text).unwrap()
}

/// Python's last traceback line, `Class: message`, as the repr the Rust
/// receipts write.
fn as_repr(line: &str) -> String {
	let (class, message) = line.split_once(": ").unwrap();
	// A KeyError's line already shows its key's repr.
	if class == "KeyError" {
		return format!("{class}({message})");
	}
	format!("{class}({})", python_repr(&json!(message)))
}

/// Each call's limit in seconds, as the traces record it, so an integer
/// witness limit and its exact Duration compare.
fn seconds(mut calls: Value) -> Value {
	for call in calls.as_array_mut().unwrap() {
		call["timeout"] = json!(call["timeout"].as_f64().unwrap());
	}
	calls
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!("rnx-final-contracts-{}", invocation_id()));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

/// Answers the suite's children as the generator did: a twin lists and
/// passes its declared count, any other test one.
#[derive(Default)]
struct Suite {
	calls: Vec<Value>,
}

impl Invoke for Suite {
	fn invoke(
		&mut self,
		command: &[String],
		cwd: &Path,
		log: &Path,
		_: &[(String, String)],
	) -> Result<String> {
		self.calls.push(
			json!({"command": command, "cwd": cwd, "log": log.file_name().unwrap().to_string_lossy()}),
		);
		let run = &command[..command.iter().position(|a| a == "--").unwrap()];
		let listing = command.iter().any(|a| a == "--list");
		let text = if run.iter().any(|a| a == "--doc") {
			let selector = run.last().unwrap();
			let n = rnx_final::catalogue::TWINS
				.iter()
				.find(|(s, _)| s == selector)
				.unwrap()
				.1;
			if listing {
				(0..n)
					.map(|i| format!("{selector}::twin{i}: test\n"))
					.collect()
			} else {
				format!("test result: ok. {n} passed; 0 failed;\n")
			}
		} else if listing {
			"module::case: test\n".into()
		} else {
			"test result: ok. 1 passed; 0 failed;\n".into()
		};
		std::fs::write(log, &text)?;
		Ok(text)
	}
	fn archive(&mut self, _: &Path, _: &str, _: &Path) -> Result<()> {
		unreachable!("rune-suite archives nothing")
	}
	fn clippy(
		&mut self,
		_: &[String],
		_: &Path,
		_: &Path,
		_: &Path,
		_: &Path,
		_: usize,
	) -> Result<i32> {
		unreachable!("rune-suite runs no Clippy comparison")
	}
}

#[test]
fn rune_suite_runs_its_recipes_and_twins_as_the_witness_ran_them() {
	let s = scratch();
	let out = s.0.join("out");
	std::fs::create_dir(&out).unwrap();
	let want = fixture(
		"rune-suite-recipes",
		&[
			("<ROOT>", &root().to_string_lossy()),
			("<OUT>", &out.to_string_lossy()),
		],
	);
	let mut trace = Suite::default();
	let rows = stage::suite(
		"rune-suite",
		&out,
		&root(),
		&|| unreachable!(),
		&Environment::new(),
		&mut trace,
	)
	.unwrap();
	assert_eq!(Value::Array(trace.calls), want["calls"]);
	assert_eq!(Value::Array(rows), want["rows"]);
	// A twin short of its count is refused.
	struct Short(Suite);
	impl Invoke for Short {
		fn invoke(
			&mut self,
			command: &[String],
			cwd: &Path,
			log: &Path,
			env: &[(String, String)],
		) -> Result<String> {
			let text = self.0.invoke(command, cwd, log, env)?;
			Ok(if command.iter().any(|a| a == "--doc") {
				text.replace("test result: ok. 4 passed", "test result: ok. 3 passed")
			} else {
				text
			})
		}
		fn archive(&mut self, _: &Path, _: &str, _: &Path) -> Result<()> {
			unreachable!()
		}
		fn clippy(
			&mut self,
			_: &[String],
			_: &Path,
			_: &Path,
			_: &Path,
			_: &Path,
			_: usize,
		) -> Result<i32> {
			unreachable!()
		}
	}
	let short = s.0.join("short");
	std::fs::create_dir(&short).unwrap();
	let error = stage::suite(
		"rune-suite",
		&short,
		&root(),
		&|| unreachable!(),
		&Environment::new(),
		&mut Short(Suite::default()),
	)
	.unwrap_err();
	assert_eq!(
		rnx_final::child::message(&error),
		want["twin_short"].as_str().unwrap()
	);
	// The parsers child_result.py now holds agree with the Rust ones.
	for (label, verdict) in want["parsers"].as_object().unwrap() {
		let (which, text) = label.split_once(' ').unwrap();
		let text = python_literal(text);
		let got = match which {
			"discovered" => rnx_final::child::discovered(&text).map(|v| json!(v)),
			_ => rnx_final::child::executed(&text).map(|v| json!(v)),
		};
		let got =
			got.unwrap_or_else(|e| json!(format!("ValueError: {}", rnx_final::child::message(&e))));
		assert_eq!(&got, verdict, "{label}");
	}
}

/// The text of a Python string literal as `repr` wrote it in the fixture.
fn python_literal(literal: &str) -> String {
	let inner = &literal[1..literal.len() - 1];
	inner.replace("\\n", "\n")
}

/// What each planted case's children print, as the generator planted it.
#[derive(Clone, Default)]
struct Plant {
	listing: Option<&'static str>,
	result: Option<&'static str>,
	rows_marker: Option<u64>,
	stderr: Option<&'static str>,
	differences: Option<Value>,
	code: i32,
	changed: bool,
	groups: bool,
}

fn plants() -> Vec<(&'static str, Plant)> {
	vec![
		("plain", Plant::default()),
		(
			"full",
			Plant {
				groups: true,
				rows_marker: Some(2),
				differences: Some(json!({"rows_checked": 2, "differences": []})),
				..Default::default()
			},
		),
		(
			"discovery-differs",
			Plant {
				listing: Some("fixture::contract: test\nfixture::other: test\n"),
				..Default::default()
			},
		),
		(
			"empty-discovery",
			Plant {
				listing: Some(""),
				..Default::default()
			},
		),
		(
			"count-differs",
			Plant {
				result: Some("test result: ok. 0 passed; 0 failed;\n"),
				..Default::default()
			},
		),
		(
			"two-result-lines",
			Plant {
				result: Some(
					"test result: ok. 1 passed; 0 failed;\ntest result: ok. 1 passed; 0 failed;\n",
				),
				..Default::default()
			},
		),
		(
			"nonzero-exit",
			Plant {
				code: 101,
				..Default::default()
			},
		),
		(
			"rows-differ",
			Plant {
				rows_marker: Some(2),
				..Default::default()
			},
		),
		("rows-missing", Plant::default()),
		(
			"nonempty-stderr",
			Plant {
				stderr: Some("warning\n"),
				..Default::default()
			},
		),
		(
			"stderr-allowed",
			Plant {
				stderr: Some("warning\n"),
				..Default::default()
			},
		),
		(
			"differences",
			Plant {
				differences: Some(
					json!({"rows_checked": 1, "differences": [{"row": 0, "difference": "a != b", "extra": "x"}]}),
				),
				..Default::default()
			},
		),
		(
			"rows-checked-differ",
			Plant {
				rows_marker: Some(2),
				differences: Some(json!({"rows_checked": 1, "differences": []})),
				..Default::default()
			},
		),
		(
			"binary-changed",
			Plant {
				changed: true,
				..Default::default()
			},
		),
		(
			"differences-missing",
			Plant {
				rows_marker: Some(2),
				differences: Some(json!({"rows_checked": 2})),
				..Default::default()
			},
		),
		("fractional-timeout", Plant::default()),
	]
}

/// Records each process as the generator recorded it, printing the plant.
struct Planted {
	plant: Plant,
	calls: Mutex<Vec<Value>>,
}

impl Process for Planted {
	fn run(&self, call: &Call) -> Result<i32> {
		self.calls.lock().unwrap().push(json!({"command": call.command, "cwd": call.cwd, "env": call.env, "timeout": call.timeout.as_secs_f64(), "check": call.check}));
		let p = &self.plant;
		if call.command.iter().any(|a| a == "--list") {
			std::fs::write(
				&call.stdout,
				p.listing.unwrap_or("fixture::contract: test\n"),
			)?;
			return Ok(0);
		}
		let mut text = String::new();
		if let Some(rows) = p.rows_marker {
			text.push_str(&format!("CONTRACT ROWS {rows}\n"));
		}
		text.push_str(p.result.unwrap_or("test result: ok. 1 passed; 0 failed;\n"));
		std::fs::write(&call.stdout, text)?;
		std::fs::write(call.stderr.as_ref().unwrap(), p.stderr.unwrap_or(""))?;
		if let Some(d) = &p.differences {
			atomic_json(
				Path::new(&call.env.as_ref().unwrap()["RNX_CONTRACT_DIFFS"]),
				d,
			)?;
		}
		Ok(p.code)
	}
}

#[test]
fn every_planted_case_runs_as_the_witness_ran_it() {
	let s = scratch();
	let want = fixture("native-cases", &[]);
	let mut seen = Vec::new();
	for (name, plant) in plants() {
		let bulk = s.0.join(name);
		std::fs::create_dir(&bulk).unwrap();
		let groups = s.0.join("groups");
		let case = &want[name];
		let expected: Value = serde_json::from_str(
			&case
				.to_string()
				.replace("<BULK>", &bulk.to_string_lossy())
				.replace("<ROOT>", &root().to_string_lossy())
				.replace("<GROUPS>", &groups.to_string_lossy()),
		)
		.unwrap();
		let binary = bulk.join("rune-tests");
		std::fs::write(&binary, b"fixture binary").unwrap();
		let receipt = bulk.join("BINARY.json");
		atomic_json(
			&receipt,
			&json!({"binary_sha256": digest(&binary).unwrap()}),
		)
		.unwrap();
		assert_eq!(
			json!(digest(&receipt).unwrap()),
			expected["receipt_sha256"],
			"{name}"
		);
		if plant.changed {
			std::fs::write(&binary, b"changed binary").unwrap();
		}
		let mut env: Environment = [
			("PLANTED_SECRET".to_string(), "do-not-copy".to_string()),
			("PATH".to_string(), "/planted".to_string()),
		]
		.into();
		if plant.groups {
			env.insert(
				"RNX_FINAL_GROUPS".into(),
				groups.to_string_lossy().into_owned(),
			);
		}
		let process = Planted {
			plant,
			calls: Mutex::new(Vec::new()),
		};
		let mut row = contracts::run_case(
			&expected["case"],
			&binary,
			&receipt,
			&root(),
			&bulk,
			&env,
			&process,
		)
		.unwrap();
		row.as_object_mut().unwrap().remove("seconds");
		let mut want_row = expected["row"].clone();
		if let Some(error) = want_row.get("error").and_then(Value::as_str) {
			let suffix = want_row
				.get("error_suffix")
				.and_then(Value::as_str)
				.unwrap_or_default()
				.to_owned();
			want_row["error"] = json!(format!("{}{suffix}", as_repr(error)));
			want_row.as_object_mut().unwrap().remove("error_suffix");
		}
		assert_eq!(row, want_row, "{name} row");
		assert_eq!(
			json!(process.calls.into_inner().unwrap()),
			seconds(expected["calls"].clone()),
			"{name} calls"
		);
		let mut files: Vec<String> = std::fs::read_dir(bulk.join("fixture"))
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		files.sort();
		assert_eq!(json!(files), expected["files"], "{name} files");
		seen.push(name);
	}
	assert_eq!(seen.len(), want.as_object().unwrap().len());
}

#[test]
fn the_manifest_and_catalogue_verdicts_match_the_witness() {
	use rnx_final::selection::{NATIVE_MANIFEST, catalogue_check, native_manifest};
	let want = fixture("native-catalogue", &[]);
	let real = native_manifest(&root()).unwrap();
	type Edit = Box<dyn Fn(&mut Value)>;
	let edits: Vec<(&str, Edit)> = vec![
		("real", Box::new(|_| {})),
		("version 2", Box::new(|m| m["version"] = json!(2))),
		("no cases", Box::new(|m| m["cases"] = json!([]))),
		(
			"duplicate",
			Box::new(|m| {
				let c = m["cases"][0].clone();
				m["cases"].as_array_mut().unwrap().push(c)
			}),
		),
		(
			"empty selector",
			Box::new(|m| m["cases"][0]["selector"] = json!("")),
		),
		(
			"zero timeout",
			Box::new(|m| m["cases"][0]["timeout"] = json!(0)),
		),
		(
			"slash name",
			Box::new(|m| m["cases"][0]["name"] = json!("a/b")),
		),
		(
			"dotdot name",
			Box::new(|m| m["cases"][0]["name"] = json!("..")),
		),
		(
			"empty name",
			Box::new(|m| m["cases"][0]["name"] = json!("")),
		),
	];
	assert_eq!(edits.len(), want["manifests"].as_object().unwrap().len());
	for (label, edit) in edits {
		let s = scratch();
		let mut m = real.clone();
		edit(&mut m);
		let path = s.0.join(NATIVE_MANIFEST);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, m.to_string()).unwrap();
		let got =
			native_manifest(&s.0).map_or_else(|e| rnx_final::child::message(&e), |_| "ok".into());
		assert_eq!(json!(got), want["manifests"][label], "{label}");
	}
	let cases = real["cases"].as_array().unwrap().clone();
	let rows: Vec<Value> = cases
		.iter()
		.map(|c| json!({"name": c["name"], "specification": rnx_final::pyjson::identity(c).unwrap(), "status": "PASS", "exit": 0, "selected": [c["selector"]], "passed": 1, "contract_rows": c.get("rows")}))
		.collect();
	let verdict = |m: &Value, rs: &[Value]| -> Value {
		catalogue_check(m, rs)
			.unwrap_or_else(|e| json!(format!("ValueError: {}", rnx_final::child::message(&e))))
	};
	let truthy = |c: &Value| rnx_final::failure::truthy(&c["ignored"]);
	let ignored = cases.iter().position(truthy).unwrap();
	let plain = cases.iter().position(|c| !truthy(c)).unwrap();
	let without = |i: usize| -> Vec<Value> {
		rows.iter()
			.enumerate()
			.filter(|(j, _)| *j != i)
			.map(|(_, r)| r.clone())
			.collect()
	};
	let mut checked = vec![
		("all", verdict(&real, &rows)),
		("missing ignored", verdict(&real, &without(ignored))),
		("missing plain", verdict(&real, &without(plain))),
	];
	let mut extra = rows.clone();
	let mut planted = rows[0].clone();
	planted["name"] = json!("planted");
	extra.push(planted);
	checked.push(("extra", verdict(&real, &extra)));
	let mut added = real.clone();
	let mut case = cases[0].clone();
	case["name"] = json!("planted-contract");
	case["selector"] = json!("planted::contract");
	added["cases"].as_array_mut().unwrap().push(case);
	checked.push(("added to the manifest", verdict(&added, &rows)));
	for (label, key, value) in [
		("exit", "exit", json!(101)),
		("status", "status", json!("FAIL")),
		("selected", "selected", json!(["other"])),
		("passed", "passed", json!(0)),
		("specification", "specification", json!("different")),
		("empty selected", "selected", json!([])),
	] {
		let mut rs = rows.clone();
		rs[0][key] = value;
		checked.push((label, verdict(&real, &rs)));
	}
	let mut rs = rows.clone();
	let i = cases
		.iter()
		.position(|c| !c["rows"].is_null() && c.get("rows").is_some())
		.unwrap();
	rs[i]["contract_rows"] = json!(rs[i]["contract_rows"].as_u64().unwrap() + 1);
	checked.push(("contract_rows", verdict(&real, &rs)));
	let mut rs = rows.clone();
	rs[0]["status"] = json!("FAIL");
	rs[0]["error"] = json!("planted error");
	checked.push(("failed with error", verdict(&real, &rs)));
	assert_eq!(checked.len(), want["verdicts"].as_object().unwrap().len());
	for (label, got) in checked {
		assert_eq!(got, want["verdicts"][label], "{label}");
	}
}

/// The build's Cargo writes the planted executables; readelf prints the
/// planted DWARF.
struct Build {
	executables: Vec<(&'static str, bool)>,
	dwarf: String,
	calls: Mutex<Vec<Value>>,
}

impl Process for Build {
	fn run(&self, call: &Call) -> Result<i32> {
		let mut row = json!({"command": call.command, "cwd": call.cwd, "log": call.stdout.file_name().unwrap().to_string_lossy(), "timeout": call.timeout.as_secs_f64()});
		if let Some(env) = &call.env {
			let target = PathBuf::from(&env["CARGO_TARGET_DIR"]);
			row["env"] = json!({"CARGO_TARGET_DIR": env["CARGO_TARGET_DIR"], "CARGO_BUILD_JOBS": env["CARGO_BUILD_JOBS"]});
			row["target_exists"] = json!(target.exists());
		}
		self.calls.lock().unwrap().push(row);
		let text = if call.command[0] == "cargo" {
			let target = PathBuf::from(&call.env.as_ref().unwrap()["CARGO_TARGET_DIR"]);
			let mut lines = vec![json!({"reason": "build-script-executed"}).to_string()];
			for (i, (name, test)) in self.executables.iter().enumerate() {
				let exe = target.join(format!("exe{i}"));
				std::fs::create_dir_all(&target)?;
				std::fs::write(&exe, format!("binary {i}"))?;
				lines.push(json!({"reason": "compiler-artifact", "target": {"name": name}, "profile": {"test": test}, "executable": exe}).to_string());
			}
			format!("Compiling rune\n{}\n", lines.join("\n"))
		} else {
			self.dwarf.clone()
		};
		std::fs::write(&call.stdout, text)?;
		Ok(0)
	}
}

#[test]
fn the_build_and_its_target_release_match_the_witness() {
	let want = fixture("native-build", &[]);
	let root = root();
	let real = rnx_final::selection::native_manifest(&root).unwrap();
	let good = format!(
		"Compilation Unit @ offset 0x0:\n <0><b>: Abbrev Number: 1 (DW_TAG_compile_unit)\n    <10>   DW_AT_name        : (indirect line string, offset: 0x0): rune/crates/rune/src/lib.rs/@/rune.abc-cgu.0\n    <14>   DW_AT_comp_dir    : (indirect line string, offset: 0x1): {}\nCompilation Unit @ offset 0x100:\n <0><10b>: Abbrev Number: 1 (DW_TAG_compile_unit)\n    <110>   DW_AT_name        : (indirect line string, offset: 0x2): /rustc/library/core/src/lib.rs\n    <114>   DW_AT_comp_dir    : (indirect line string, offset: 0x3): /rustc\n",
		root.display()
	);
	let one = vec![("rune", true), ("rune", false), ("other", true)];
	let mut two = real.clone();
	let library = two["libraries"][0].clone();
	two["libraries"].as_array_mut().unwrap().push({
		let mut l = library;
		l["name"] = json!("second");
		l
	});
	let mut none = real.clone();
	none["libraries"] = json!(
		none["libraries"]
			.as_array()
			.unwrap()
			.iter()
			.filter(|l| {
				let f: Vec<&str> = l["features"].as_str().unwrap().split(',').collect();
				!(f.contains(&"execution")
					&& f.contains(&"serde")
					&& !rnx_final::failure::truthy(&l["selector"]))
			})
			.cloned()
			.collect::<Vec<_>>()
	);
	type Planted = (&'static str, Value, Vec<(&'static str, bool)>, String);
	let builds: Vec<Planted> = vec![
		("good", real.clone(), one.clone(), good.clone()),
		(
			"ambiguous",
			real.clone(),
			vec![("rune", true), ("rune", true)],
			good.clone(),
		),
		(
			"no binary",
			real.clone(),
			vec![("rune", false)],
			good.clone(),
		),
		(
			"other checkout",
			real.clone(),
			one.clone(),
			good.replace(&root.to_string_lossy().into_owned(), "/elsewhere"),
		),
		(
			"no units",
			real.clone(),
			one.clone(),
			good.split("Compilation Unit @ offset 0x100")
				.next()
				.unwrap()
				.replace("rune/crates/rune/src/lib.rs", "other/src/lib.rs"),
		),
		(
			"two dirs",
			real.clone(),
			one.clone(),
			good.replace(
				"    <14>",
				"    <13>   DW_AT_comp_dir    : (x): /a\n    <14>",
			),
		),
		("two recipes", two, one.clone(), good.clone()),
		("no recipe", none, one.clone(), good.clone()),
	];
	assert_eq!(builds.len(), want.as_object().unwrap().len());
	let env: Environment = [(
		"MAKEFLAGS".to_string(),
		"-j7 --jobserver-auth=fifo:/nonexistent".to_string(),
	)]
	.into();
	for (label, m, executables, dwarf) in builds {
		let s = scratch();
		let bulk = s.0.join("bulk");
		std::fs::create_dir(&bulk).unwrap();
		let expected: Value = serde_json::from_str(
			&want[label]
				.to_string()
				.replace("<BULK>", &bulk.to_string_lossy())
				.replace("<ROOT>", &root.to_string_lossy()),
		)
		.unwrap();
		let process = Build {
			executables,
			dwarf,
			calls: Mutex::new(Vec::new()),
		};
		let result = match contracts::build(&root, &bulk, &m, &env, &process) {
			Ok((binary, receipt)) => {
				let mut r: Value =
					serde_json::from_slice(&std::fs::read(receipt).unwrap()).unwrap();
				for key in ["source", "candidate"] {
					if !r[key].is_null() {
						r[key] = json!(format!("<{}>", key.to_uppercase()));
					}
				}
				// The planted DWARF names the checkout, so its digest is the
				// generator's path's: held to this run's log, then masked.
				let log = bulk.join("dwarf.log");
				assert_eq!(r["dwarf_sha256"], json!(digest(&log).unwrap()), "{label}");
				assert_eq!(
					std::fs::read_to_string(&log).unwrap(),
					process.dwarf,
					"{label}"
				);
				r["dwarf_sha256"] = json!("<DWARF>");
				json!({"binary": binary.file_name().unwrap().to_string_lossy(), "receipt": r})
			}
			Err(error) => {
				json!({"error": format!("ValueError: {}", rnx_final::child::message(&error))})
			}
		};
		let mut expected_result = expected["result"].clone();
		if expected_result["receipt"].is_object() {
			expected_result["receipt"]["dwarf_sha256"] = json!("<DWARF>");
		}
		assert_eq!(result, expected_result, "{label} result");
		assert_eq!(
			json!(process.calls.into_inner().unwrap()),
			seconds(expected["calls"].clone()),
			"{label} calls"
		);
		let mut files: Vec<String> = std::fs::read_dir(&bulk)
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		files.sort();
		assert_eq!(json!(files), expected["files"], "{label} files");
		let release = bulk.join("build-target-RELEASE.json");
		let got = if release.exists() {
			let r: Value = serde_json::from_slice(&std::fs::read(&release).unwrap()).unwrap();
			let mut retained: Vec<String> = r["retained"]
				.as_object()
				.unwrap()
				.keys()
				.map(|k| {
					Path::new(k)
						.file_name()
						.unwrap()
						.to_string_lossy()
						.into_owned()
				})
				.collect();
			retained.sort();
			let original = r["original_error"].as_str().map(|e| e.to_owned());
			json!({"status": r["status"], "removed": r.get("removed"), "retained": retained, "original_error": original, "error": r.get("error")})
		} else {
			Value::Null
		};
		assert_eq!(got, expected["release"], "{label} release");
	}
}

#[test]
fn the_rune_suite_receipt_matches_the_witness() {
	let s = scratch();
	let out = s.0.join("out");
	std::fs::create_dir(&out).unwrap();
	let want = fixture("rune-suite-receipt", &[("<OUT>", &out.to_string_lossy())]);
	let rows = || -> Result<Vec<Value>> {
		std::fs::write(out.join("0-test.log"), "log")?;
		Ok(vec![
			json!({"command": ["planted"], "discovered": ["a"], "passed": 1}),
		])
	};
	let pass = stage::receipt("rune-suite", &out, rows, || {
		Ok(json!({"status": "PASS", "catalogue": {"status": "PASS"}}))
	})
	.unwrap();
	assert_eq!(pass, want["pass_"]["value"]);
	std::fs::remove_file(out.join("0-test.log")).unwrap();
	let fail = stage::receipt("rune-suite", &out, rows, || {
		Ok(json!({"status": "FAIL", "error": "planted contract failure"}))
	})
	.unwrap();
	let mut expected = want["fail"]["value"].clone();
	expected["error"] = json!(as_repr(expected["error"].as_str().unwrap()));
	assert_eq!(fail, expected);
}

/// The stage's environment inside a final with `pool`, holding `held`.
fn final_env(pool: &Pool, held: usize) -> Environment {
	[
		("MAKEFLAGS".to_string(), pool.flags()),
		("RNX_FINAL_STAGE_TOKENS".to_string(), held.to_string()),
	]
	.into()
}

/// The tokens free in the pool now, all put back.
fn free(pool: &Pool) -> usize {
	let mut leases = Vec::new();
	while let Some(lease) = pool.take(1).unwrap() {
		leases.push(lease);
	}
	let n = leases.len();
	for lease in leases {
		pool.give(lease).unwrap();
	}
	n
}

/// An exclusive case holds every token its stage does not, so nothing
/// else runs beside it; afterwards every token is back.
#[test]
fn an_exclusive_case_holds_every_token_its_stage_does_not() {
	let s = scratch();
	let pool = Pool::create(&s.0.join("jobserver.fifo"), 4).unwrap();
	let stage = pool.take(1).unwrap().unwrap();
	let env = final_env(&pool, 1);
	let seen = Mutex::new(Vec::new());
	let cases = vec![
		json!({"name": "exclusive", "exclusive": true}),
		json!({"name": "shared"}),
	];
	let rows = contracts::run_cases(
		&cases,
		&|c| {
			if c["name"] == "exclusive" {
				seen.lock().unwrap().push(pool.take(1).unwrap().is_none());
			}
			Ok(json!({"name": c["name"]}))
		},
		&env,
	)
	.unwrap();
	assert_eq!(
		rows,
		vec![json!({"name": "exclusive"}), json!({"name": "shared"})]
	);
	assert_eq!(
		seen.into_inner().unwrap(),
		vec![true],
		"a token was free while the exclusive case ran"
	);
	pool.give(stage).unwrap();
	assert_eq!(free(&pool), 4);
	pool.close().unwrap();
}

/// With the pool's only token held by its stage, every case still runs,
/// on the stage's own slot.
#[test]
fn a_one_token_pool_still_finishes_every_case() {
	let s = scratch();
	let pool = Pool::create(&s.0.join("jobserver.fifo"), 1).unwrap();
	let stage = pool.take(1).unwrap().unwrap();
	let cases: Vec<Value> = (0..4).map(|i| json!({"name": format!("c{i}")})).collect();
	let rows = contracts::run_cases(
		&cases,
		&|c| Ok(json!({"name": c["name"]})),
		&final_env(&pool, 1),
	)
	.unwrap();
	assert_eq!(rows.len(), 4);
	pool.give(stage).unwrap();
	assert_eq!(free(&pool), 1);
	pool.close().unwrap();
}

/// Shared cases overlap, at most the case width (half the jobserver), on
/// the stage's own slots and pool tokens; one failing case stops none.
#[test]
fn shared_cases_overlap_within_the_width_and_all_run() {
	let s = scratch();
	let pool = Pool::create(&s.0.join("jobserver.fifo"), 4).unwrap();
	let stage = pool.take(1).unwrap().unwrap();
	let env = final_env(&pool, 1);
	let width = 2;
	let active = Mutex::new((0usize, 0usize));
	let changed = std::sync::Condvar::new();
	let cases: Vec<Value> = (0..6).map(|i| json!({"name": format!("c{i}")})).collect();
	let rows = contracts::run_cases(
		&cases,
		&|c| {
			let mut state = active.lock().unwrap();
			state.0 += 1;
			state.1 = state.1.max(state.0);
			changed.notify_all();
			// Each case waits, bounded, for a neighbor to overlap it.
			let (mut state, _) = changed
				.wait_timeout_while(state, Duration::from_secs(10), |s| {
					s.0 < width && s.1 < width
				})
				.unwrap();
			state.0 -= 1;
			changed.notify_all();
			Ok(
				json!({"name": c["name"], "status": if c["name"] == "c3" { "FAIL" } else { "PASS" }}),
			)
		},
		&env,
	)
	.unwrap();
	assert_eq!(active.into_inner().unwrap().1, width);
	assert_eq!(rows.iter().filter(|r| r["status"] == "PASS").count(), 5);
	assert_eq!(
		rows.iter().map(|r| r["name"].clone()).collect::<Vec<_>>(),
		cases.iter().map(|c| c["name"].clone()).collect::<Vec<_>>()
	);
	pool.give(stage).unwrap();
	assert_eq!(free(&pool), 4);
	pool.close().unwrap();
}

/// Outside a final, fake cases never wait on a pool they did not join,
/// even one whose every token is held.
#[test]
fn cases_outside_a_final_ignore_an_ambient_pool() {
	let s = scratch();
	let pool = Pool::create(&s.0.join("jobserver.fifo"), 1).unwrap();
	let held = pool.take(1).unwrap().unwrap();
	let cases = vec![
		json!({"name": "exclusive", "exclusive": true}),
		json!({"name": "shared"}),
	];
	let env: Environment = [("MAKEFLAGS".to_string(), "-j1".to_string())].into();
	let rows = contracts::run_cases(&cases, &|c| Ok(json!({"name": c["name"]})), &env).unwrap();
	assert_eq!(rows.len(), 2);
	pool.give(held).unwrap();
	pool.close().unwrap();
}

/// A contract process's group is drained before its call returns: a
/// background child it left is gone, and a process past its limit is
/// terminated with its group.
#[test]
fn a_contract_process_leaves_nothing_running() {
	let s = scratch();
	let pidfile = s.0.join("pid");
	let live = contracts::Live {
		env: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
	};
	let call = |command: &str, timeout: u64, log: &str| Call {
		command: vec!["sh".into(), "-c".into(), command.into()],
		cwd: s.0.clone(),
		env: None,
		timeout: Duration::from_secs(timeout),
		check: false,
		stdout: s.0.join(log),
		stderr: None,
	};
	let code = live
		.run(&call(
			&format!("sleep 300 & echo $! > {}; exit 0", pidfile.display()),
			60,
			"a.log",
		))
		.unwrap();
	assert_eq!(code, 0);
	let pid: i32 = std::fs::read_to_string(&pidfile)
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	assert!(
		!Path::new(&format!("/proc/{pid}")).exists()
			|| std::fs::read_to_string(format!("/proc/{pid}/stat"))
				.unwrap()
				.contains(") Z "),
		"the background child outlived its call"
	);
	let error = live.run(&call("sleep 300", 1, "b.log")).unwrap_err();
	assert!(format!("{error}").starts_with("TimeoutExpired("), "{error}");
	let failed = live
		.run(&Call {
			check: true,
			..call("exit 3", 60, "c.log")
		})
		.unwrap_err();
	assert!(
		format!("{failed}").contains("returned non-zero exit status 3"),
		"{failed}"
	);
}

/// The stage's children read the baseline they were given while it
/// exists, and cannot once it is deleted (the control formerly in
/// test_anchor_cache.py over suite_stage.invoke).
#[test]
fn a_stage_child_reads_its_baseline_until_it_is_deleted() {
	let s = scratch();
	let source = s.0.join("source");
	std::fs::create_dir(&source).unwrap();
	std::fs::write(source.join("Cargo.toml"), "baseline source").unwrap();
	let mut live = stage::Live {
		env: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
	};
	let command: Vec<String> = ["cat", "Cargo.toml"]
		.iter()
		.map(|s| s.to_string())
		.collect();
	let text = live
		.invoke(&command, &source, &s.0.join("before.log"), &[])
		.unwrap();
	assert!(text.contains("baseline source"));
	std::fs::remove_dir_all(&source).unwrap();
	assert!(
		live.invoke(&command, &source, &s.0.join("after.log"), &[])
			.is_err()
	);
}

/// child_result.py's parsers, which moved there from suite_stage.py, give
/// the Rust parsers' verdicts over the same texts.
#[test]
fn the_python_parsers_agree_with_the_rust_ones() {
	let texts = [
		"",
		"a::b: test\n",
		"a::b: test\nc: test\nnot a test\n",
		" x: test\n",
		"test result: ok. 0 passed;",
		"test result: ok. 3 passed;\ntest result: ok. 2 passed;",
		"test result: FAILED. 3 passed;",
		"x\r: test\n",
	];
	let script = "import json, sys\nimport child_result as c\nout = []\nfor t in json.loads(sys.stdin.read()):\n    row = []\n    for f in (c.discovered, c.executed):\n        try: row.append(f(t))\n        except ValueError as e: row.append('ValueError: ' + str(e))\n    out.append(row)\nprint(json.dumps(out))\n";
	let mut child = std::process::Command::new("/usr/bin/python3")
		.args([
			"-I",
			"-B",
			"-S",
			"-c",
			&format!(
				"import sys; sys.path.insert(0, {:?})\n{script}",
				root().join("tools/column").to_string_lossy()
			),
		])
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.spawn()
		.unwrap();
	std::io::Write::write_all(
		child.stdin.as_mut().unwrap(),
		json!(texts).to_string().as_bytes(),
	)
	.unwrap();
	let output = child.wait_with_output().unwrap();
	assert!(output.status.success());
	let python: Vec<Vec<Value>> = serde_json::from_slice(&output.stdout).unwrap();
	for (text, verdicts) in texts.iter().zip(python) {
		let message =
			|e: anyhow::Error| json!(format!("ValueError: {}", rnx_final::child::message(&e)));
		let rust = vec![
			rnx_final::child::discovered(text)
				.map(|v| json!(v))
				.unwrap_or_else(message),
			rnx_final::child::executed(text)
				.map(|v| json!(v))
				.unwrap_or_else(message),
		];
		assert_eq!(rust, verdicts, "{text:?}");
	}
}

/// A manifest's fractional limit is kept, not truncated: a command that
/// outlives the whole seconds of its limit but not the limit passes, and
/// a half-second limit is half a second, not none.
#[test]
fn a_fractional_limit_is_kept() {
	let s = scratch();
	let live = contracts::Live {
		env: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
	};
	let call = |command: &str, seconds: f64, log: &str| Call {
		command: vec!["sh".into(), "-c".into(), command.into()],
		cwd: s.0.clone(),
		env: None,
		timeout: Duration::from_secs_f64(seconds),
		check: true,
		stdout: s.0.join(log),
		stderr: None,
	};
	assert_eq!(live.run(&call("sleep 1.3", 1.9, "a.log")).unwrap(), 0);
	assert_eq!(live.run(&call("true", 0.5, "b.log")).unwrap(), 0);
	let error = live.run(&call("sleep 300", 0.5, "c.log")).unwrap_err();
	assert!(
		format!("{error}").contains("timed out after 0.5 seconds"),
		"{error}"
	);
}

/// A case whose differences file omits its list fails, as the Python
/// runner's KeyError did, and the cases after it still run.
#[test]
fn a_case_without_its_differences_list_fails_and_the_rest_run() {
	let s = scratch();
	let bulk = s.0.join("bulk");
	std::fs::create_dir(&bulk).unwrap();
	let binary = bulk.join("rune-tests");
	std::fs::write(&binary, b"fixture binary").unwrap();
	let receipt = bulk.join("BINARY.json");
	atomic_json(
		&receipt,
		&json!({"binary_sha256": digest(&binary).unwrap()}),
	)
	.unwrap();
	let case = |name: &str| json!({"name": name, "selector": "fixture::contract", "exact": false, "ignored": false, "timeout": 17, "rows": 2, "sandbox": false, "empty_stderr": false, "env": {}});
	let cases = vec![case("missing"), case("after")];
	let missing = Planted {
		plant: Plant {
			rows_marker: Some(2),
			differences: Some(json!({"rows_checked": 2})),
			..Default::default()
		},
		calls: Mutex::new(Vec::new()),
	};
	let complete = Planted {
		plant: Plant {
			rows_marker: Some(2),
			differences: Some(json!({"rows_checked": 2, "differences": []})),
			..Default::default()
		},
		calls: Mutex::new(Vec::new()),
	};
	let env = Environment::new();
	let rows = contracts::run_cases(
		&cases,
		&|c| {
			let process: &dyn Process = if c["name"] == "missing" {
				&missing
			} else {
				&complete
			};
			contracts::run_case(c, &binary, &receipt, &root(), &bulk, &env, process)
		},
		&env,
	)
	.unwrap();
	assert_eq!(rows[0]["status"], "FAIL");
	assert_eq!(rows[0]["error"], "KeyError('differences')");
	assert_eq!(rows[1]["status"], "PASS");
	let receipt: Value =
		serde_json::from_slice(&std::fs::read(bulk.join("missing/RESULT.json")).unwrap()).unwrap();
	assert_eq!(receipt["status"], "FAIL");
}
