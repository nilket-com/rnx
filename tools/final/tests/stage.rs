//! The Rust stage modes against the Python stage programs they replace, at
//! ADR 0287's witness revision: every converted stage's sequence of
//! children (command, directory, log) and rows, both Clippy comparisons'
//! children, and the diagnostic identity on planted Cargo lines; then the
//! ported controls.
mod common;
use rnx_final::failure::Environment;
use rnx_final::stage::{self, Invoke, diagnostics};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const WITNESS_0287: &str = "f0a6663697538651432537c442ed6c60791b424a";
const BASELINE: &str = "0000000000000000000000000000000000000001";

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
	assert_eq!(value["witness"], WITNESS_0287, "{name}");
	let mut text = value["expected"].to_string();
	for (mark, real) in substitutions {
		let escaped = serde_json::to_string(real).unwrap();
		text = text.replace(mark, &escaped[1..escaped.len() - 1]);
	}
	serde_json::from_str(&text).unwrap()
}

fn executable_file(path: &Path) {
	use std::os::unix::fs::PermissionsExt as _;
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(path, "#!/bin/sh\n").unwrap();
	std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-stage-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir_all(d.join("valgrind/include/valgrind")).unwrap();
	std::fs::write(d.join("valgrind/include/valgrind/callgrind.h"), "").unwrap();
	executable_file(&d.join("valgrind/bin/valgrind"));
	Scratch(d.canonicalize().unwrap())
}

/// Records each child as the generator recorded it, with its fake output.
#[derive(Default)]
struct Trace {
	calls: Vec<Value>,
	fail_package: bool,
}

fn empty_tar(path: &Path) {
	let mut builder = tar::Builder::new(std::fs::File::create(path).unwrap());
	builder.finish().unwrap();
}

impl Invoke for Trace {
	fn invoke(
		&mut self,
		command: &[String],
		cwd: &Path,
		log: &Path,
		_: &[(String, String)],
	) -> anyhow::Result<String> {
		self.calls.push(json!({"command": command, "cwd": cwd, "env": null, "log": log.file_name().unwrap().to_string_lossy()}));
		if self.fail_package && log.to_string_lossy().ends_with("-package.log") {
			anyhow::bail!("Command {command:?} returned non-zero exit status 101.");
		}
		let text = if command.iter().any(|a| a == "--list") {
			"module::case: test\n"
		} else if command.iter().any(|a| a == "--test-threads=4") {
			"test result: ok. 1 passed; 0 failed\n"
		} else {
			""
		};
		std::fs::write(log, text)?;
		Ok(text.into())
	}
	fn archive(&mut self, root: &Path, commit: &str, archive: &Path) -> anyhow::Result<()> {
		self.calls
			.push(json!({"command": ["git", "archive", commit], "cwd": root, "subprocess": true}));
		empty_tar(archive);
		Ok(())
	}
	fn clippy(
		&mut self,
		command: &[String],
		cwd: &Path,
		stdout: &Path,
		stderr: &Path,
		target: &Path,
		_: usize,
	) -> anyhow::Result<i32> {
		self.calls.push(
			json!({"command": command, "cwd": cwd, "CARGO_TARGET_DIR": target, "CARGO_BUILD_JOBS": "jobs"}),
		);
		std::fs::write(stdout, "")?;
		std::fs::write(stderr, "")?;
		Ok(0)
	}
}

fn environment(valgrind: &Path) -> Environment {
	[(
		"RNX_VALGRIND_ROOT".to_string(),
		valgrind.to_string_lossy().into_owned(),
	)]
	.into()
}

#[test]
fn every_converted_suite_stage_runs_as_the_witness_ran_it() {
	let s = scratch();
	let root = root();
	let want = fixture(
		"suite-recipes",
		&[
			("<ROOT>", &root.to_string_lossy()),
			("<SCRATCH>", &s.0.to_string_lossy()),
		],
	);
	assert_eq!(want["record_baseline"], BASELINE);
	let stages = want["stages"].as_object().unwrap();
	assert_eq!(stages.len(), 53);
	// Exactly the catalogue's Rust suite stages, by name.
	let catalogue: Vec<String> = rnx_final::catalogue::commands(&root.join("target/unused"), &root)
		.unwrap()
		.into_iter()
		.filter(|(n, c)| {
			c.get(2).map(String::as_str) == Some("suite")
				&& c[0] == stage::PROGRAM
				&& n != "rune-suite"
		})
		.map(|(n, _)| n)
		.collect();
	assert_eq!(
		catalogue.iter().collect::<std::collections::BTreeSet<_>>(),
		stages.keys().collect()
	);
	let env = environment(&s.0.join("valgrind"));
	for (name, case) in stages {
		let out = s.0.join(format!("out-{name}"));
		std::fs::create_dir(&out).unwrap();
		let mut trace = Trace::default();
		let rows =
			stage::suite(name, &out, &root, &|| Ok(BASELINE.into()), &env, &mut trace).unwrap();
		assert_eq!(Value::Array(trace.calls), case["calls"], "{name} children");
		assert_eq!(Value::Array(rows), case["rows"], "{name} rows");
	}
}

#[test]
fn both_clippy_comparisons_run_as_the_witness_ran_them() {
	let s = scratch();
	let root = root();
	let want = fixture(
		"clippy-recipes",
		&[
			("<ROOT>", &root.to_string_lossy()),
			("<SCRATCH>", &s.0.to_string_lossy()),
		],
	);
	for (name, adapter) in [("clippy", false), ("adapter-clippy", true)] {
		assert_eq!(want[name]["base"], stage::CLIPPY_BASE);
		assert_eq!(
			want[name]["polars_lint_args"],
			json!(stage::POLARS_LINT_ARGS)
		);
		let out = s.0.join(if adapter {
			"clippy-adapter"
		} else {
			"clippy-host"
		});
		std::fs::create_dir(&out).unwrap();
		let mut trace = Trace::default();
		let result = stage::clippy(adapter, &out, &root, &Environment::new(), &mut trace).unwrap();
		assert_eq!(result["status"], "PASS");
		// The archive's call carries no target or jobs.
		let mut calls = trace.calls;
		calls[0] = json!({"CARGO_BUILD_JOBS": null, "CARGO_TARGET_DIR": null, "command": calls[0]["command"], "cwd": calls[0]["cwd"]});
		assert_eq!(Value::Array(calls), want[name]["calls"], "{name}");
	}
}

fn rendered(found: &std::collections::BTreeMap<stage::Identity, u64>) -> Vec<String> {
	let mut rows: Vec<String> = stage::render(found)
		.as_array()
		.unwrap()
		.iter()
		.map(Value::to_string)
		.collect();
	rows.sort();
	rows
}

#[test]
fn diagnostics_are_identified_as_the_witness_identified_them() {
	let s = scratch();
	let droot = s.0.join("diag-root");
	std::fs::create_dir(&droot).unwrap();
	let want = fixture(
		"clippy-diagnostics",
		&[("<SCRATCH>", &s.0.to_string_lossy())],
	);
	let cases = want.as_object().unwrap();
	assert_eq!(cases.len(), 9);
	for (name, case) in cases {
		let text: String = case["lines"]
			.as_array()
			.unwrap()
			.iter()
			.map(|l| match l {
				Value::String(raw) => format!("{raw}\n"),
				other => format!("{other}\n"),
			})
			.collect();
		let mut expected: Vec<String> = case["found"]
			.as_array()
			.unwrap()
			.iter()
			.map(Value::to_string)
			.collect();
		expected.sort();
		// This historical mixed-stream witness contains a literal non-JSON
		// line. The clean stdout contract rejects it; its JSON notes and
		// build events still contribute no warnings.
		let text = if name == "ignored lines" {
			assert!(diagnostics(&text, &droot).is_err());
			text.strip_suffix("not json\n").unwrap()
		} else {
			&text
		};
		assert_eq!(
			rendered(&diagnostics(text, &droot).unwrap()),
			expected,
			"{name}"
		);
	}
}

fn line(text: &str, file: &str) -> String {
	json!({"reason": "compiler-message", "target": {"name": "rnx"}, "message": {"level": "warning", "code": {"code": "clippy::x"},
		"message": text, "spans": [{"file_name": file, "is_primary": true, "line_start": 1}]}})
	.to_string()
		+ "\n"
}

/// The comparison is by identity and count: a moved line and an absolute
/// path pass, a second occurrence and a new message each fail.
#[test]
fn a_new_or_repeated_diagnostic_is_added_and_a_moved_one_is_not() {
	let root = Path::new("/r");
	let base = diagnostics(&line("example", "src/a.rs"), root).unwrap();
	assert!(
		stage::added(
			&base,
			&diagnostics(&line("example", "/r/src/a.rs"), root).unwrap()
		)
		.is_empty()
	);
	assert!(
		!stage::added(
			&base,
			&diagnostics(&(line("example", "src/a.rs").repeat(2)), root).unwrap()
		)
		.is_empty()
	);
	assert!(
		!stage::added(
			&base,
			&diagnostics(&line("changed", "src/a.rs"), root).unwrap()
		)
		.is_empty()
	);
	// An existing diagnostic still present is allowed; one gone is too.
	assert!(stage::added(&base, &base).is_empty());
	assert!(stage::added(&base, &diagnostics("", root).unwrap()).is_empty());
}

/// The Polars comparison keeps the preview diagnostic at warning level
/// rather than letting it abort the baseline.
#[test]
fn the_polars_comparison_keeps_the_existing_diagnostic_as_a_warning() {
	let s = scratch();
	let out = s.0.join("out");
	std::fs::create_dir(&out).unwrap();
	let mut trace = Trace::default();
	stage::suite(
		"clippy-adapter-polars",
		&out,
		&root(),
		&|| Ok(BASELINE.into()),
		&Environment::new(),
		&mut trace,
	)
	.unwrap();
	for call in trace.calls.iter().filter(|c| c["subprocess"].is_null()) {
		let command: Vec<String> = serde_json::from_value(call["command"].clone()).unwrap();
		assert!(
			command.ends_with(&["--".into(), "-Wclippy::erasing_op".into()]),
			"{command:?}"
		);
		assert!(!command.iter().any(|a| a == "-D"), "{command:?}");
	}
}

/// The column probe's build and lint need Valgrind's header and refuse
/// before Cargo without it; its format check needs neither.
#[test]
fn the_probe_needs_its_header_and_its_format_check_does_not() {
	let s = scratch();
	for (name, needs) in [
		("package-column-probe", true),
		("lint-package-column-probe", true),
		("format-package-column-probe", false),
	] {
		// An executable Valgrind whose installation lacks the header.
		let missing = s.0.join("no-valgrind");
		if !missing.exists() {
			executable_file(&missing.join("bin/valgrind"));
		}
		let out = s.0.join(format!("out-{name}"));
		std::fs::create_dir(&out).unwrap();
		let mut trace = Trace::default();
		let result = stage::suite(
			name,
			&out,
			&root(),
			&|| Ok(BASELINE.into()),
			&environment(&missing),
			&mut trace,
		);
		if needs {
			let error = result.unwrap_err().to_string();
			assert!(error.contains("callgrind.h"), "{name}: {error}");
			assert!(
				trace.calls.iter().all(|c| !c["command"][0]
					.as_str()
					.unwrap_or_default()
					.ends_with("cargo")),
				"{name} ran Cargo"
			);
		} else {
			result.unwrap();
		}
	}
}

/// A failing package build fails its stage, for every registered recipe.
#[test]
fn every_registered_package_recipe_propagates_a_failed_build() {
	let s = scratch();
	let env = environment(&s.0.join("valgrind"));
	for (_, name, _) in rnx_final::catalogue::package_checks() {
		let out = s.0.join(format!("out-{name}"));
		std::fs::create_dir(&out).unwrap();
		let mut trace = Trace {
			fail_package: true,
			..Default::default()
		};
		let result = stage::suite(
			name,
			&out,
			&root(),
			&|| Ok(BASELINE.into()),
			&env,
			&mut trace,
		);
		// A test recipe does not reach the package branch.
		if trace.calls.iter().any(|c| {
			c["log"]
				.as_str()
				.is_some_and(|l| l.ends_with("-package.log"))
		}) {
			assert!(result.is_err(), "{name} passed a failed build");
		}
	}
}

/// An unknown stage is refused.
#[test]
fn an_unknown_stage_is_refused() {
	let s = scratch();
	let out = s.0.join("out");
	std::fs::create_dir(&out).unwrap();
	{
		let name = "no-such-stage";
		assert!(
			stage::suite(
				name,
				&out,
				&root(),
				&|| Ok(BASELINE.into()),
				&Environment::new(),
				&mut Trace::default()
			)
			.is_err(),
			"{name}"
		);
	}
}

/// Valgrind resolves as `fixed_route_gate.valgrind_root` did: a configured
/// root needs an executable `bin/valgrind`, and the PATH search skips a
/// file it cannot execute.
#[test]
fn valgrind_resolution_needs_an_executable_binary() {
	use std::os::unix::fs::PermissionsExt as _;
	let s = scratch();
	let env = |pairs: &[(&str, &Path)]| -> Environment {
		pairs
			.iter()
			.map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
			.collect()
	};
	assert_eq!(
		stage::valgrind_root(&env(&[("RNX_VALGRIND_ROOT", &s.0.join("valgrind"))])).unwrap(),
		s.0.join("valgrind")
	);
	// A header but no binary.
	let header_only = s.0.join("header-only");
	std::fs::create_dir_all(header_only.join("include/valgrind")).unwrap();
	std::fs::write(header_only.join("include/valgrind/callgrind.h"), "").unwrap();
	assert!(stage::valgrind_root(&env(&[("RNX_VALGRIND_ROOT", &header_only)])).is_err());
	// A binary that cannot execute.
	let plain = s.0.join("plain");
	std::fs::create_dir_all(plain.join("bin")).unwrap();
	std::fs::write(plain.join("bin/valgrind"), "").unwrap();
	std::fs::set_permissions(
		plain.join("bin/valgrind"),
		std::fs::Permissions::from_mode(0o644),
	)
	.unwrap();
	assert!(stage::valgrind_root(&env(&[("RNX_VALGRIND_ROOT", &plain)])).is_err());
	// On PATH, a nonexecutable first entry is skipped for a later valid one.
	let path = std::env::join_paths([plain.join("bin"), s.0.join("valgrind/bin")]).unwrap();
	let found =
		stage::valgrind_root(&[("PATH".to_string(), path.to_string_lossy().into_owned())].into())
			.unwrap();
	assert_eq!(found, s.0.join("valgrind"));
	// The probe still needs its header.
	let out = s.0.join("out-probe");
	std::fs::create_dir(&out).unwrap();
	let no_header = s.0.join("no-header");
	executable_file(&no_header.join("bin/valgrind"));
	let error = stage::suite(
		"package-column-probe",
		&out,
		&root(),
		&|| Ok(BASELINE.into()),
		&environment(&no_header),
		&mut Trace::default(),
	)
	.unwrap_err();
	assert!(error.to_string().contains("callgrind.h"), "{error}");
}

/// A child past its limit is killed through its pinned identity and the
/// wait returns, bounded.
#[test]
fn a_child_past_its_limit_is_killed() {
	let started = std::time::Instant::now();
	let mut command = std::process::Command::new("sleep");
	command.arg("30");
	let error = stage::wait(command, std::time::Duration::from_millis(300)).unwrap_err();
	assert!(error.to_string().contains("timed out"), "{error}");
	assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[test]
fn cargo_progress_cannot_split_or_conceal_a_lint_record() {
	let tmp = scratch();
	let warning = line("planted", "src/lib.rs");
	for format in ["json", "json-render-diagnostics"] {
		let log = tmp.0.join(format!("{format}.log"));
		let flag = format!("--message-format={format}");
		let command = [
			"/bin/sh",
			"-c",
			r#"printf '    Finished' >&2; printf '%s' "$1"; printf ' profile\n' >&2"#,
			"sh",
			&warning,
			&flag,
		]
		.map(str::to_owned);
		let text = stage::Live {
			env: Environment::new(),
		}
		.invoke(&command, &tmp.0, &log, &[])
		.unwrap();
		assert_eq!(text, warning);
		assert_eq!(
			std::fs::read_to_string(log.with_extension("stderr.log")).unwrap(),
			"    Finished profile\n"
		);
		assert_eq!(diagnostics(&text, &tmp.0).unwrap().values().sum::<u64>(), 1);
	}
	assert!(diagnostics(&format!("    Finished{warning}"), &tmp.0).is_err());
}

#[test]
fn identical_crates_have_the_same_lints_on_independent_cold_and_warm_runs() {
	let tmp = scratch();
	let mut env: Environment = std::env::vars().collect();
	for key in [
		"RNX_FINAL_CACHE_ROUTES",
		"CARGO_TARGET_DIR",
		"CARGO_BUILD_BUILD_DIR",
	] {
		env.remove(key);
	}
	let mut live = stage::Live { env };
	let mut expected = None;
	for role in ["baseline", "candidate"] {
		let tree = tmp.0.join(role);
		let target = tree.join("target");
		std::fs::create_dir_all(tree.join("src")).unwrap();
		std::fs::write(
			tree.join("Cargo.toml"),
			"[workspace]\n[package]\nname='lint-witness'\nversion='0.0.0'\nedition='2021'\n",
		)
		.unwrap();
		std::fs::write(
			tree.join("src/lib.rs"),
			"#![warn(clippy::len_zero)]\npub fn empty(s: &str) -> bool { s.len() == 0 }",
		)
		.unwrap();
		let command = [
			"cargo",
			"+1.98.1",
			"clippy",
			"--offline",
			"--lib",
			"--tests",
			"--target-dir",
			target.to_str().unwrap(),
			"--message-format=json",
		]
		.map(str::to_owned);
		for (pass, fresh) in [("cold", false), ("warm", true)] {
			let log = tmp.0.join(format!("{role}-{pass}.log"));
			let text = live.invoke(&command, &tree, &log, &[]).unwrap();
			let counts = diagnostics(&text, &tree).unwrap();
			assert_eq!(counts.len(), 1);
			assert_eq!(counts.values().sum::<u64>(), 2);
			assert_eq!(expected.get_or_insert_with(|| counts.clone()), &counts);
			let artifacts: Vec<Value> = text
				.lines()
				.map(|l| serde_json::from_str::<Value>(l).unwrap())
				.filter(|m| {
					m["reason"] == "compiler-artifact" && m["target"]["name"] == "lint_witness"
				})
				.collect();
			assert_eq!(artifacts.len(), 2);
			for artifact in artifacts {
				assert_eq!(artifact["fresh"], fresh);
				for path in artifact["filenames"].as_array().unwrap() {
					assert!(Path::new(path.as_str().unwrap()).starts_with(&target));
				}
			}
		}
	}
}
