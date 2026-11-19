//! Ports of the Python required-stage controls (test_required_stages.py).
use rnx_final::catalogue::{
	Bulk, Graph, affected, bulk_roots, commands, formats, lints, package_checks, packages, quality,
	recipes, required,
};
use rnx_final::child::{discovered, executed};
use std::collections::BTreeSet;
use std::path::Path;

fn set(stages: Vec<String>) -> BTreeSet<String> {
	stages.into_iter().collect()
}

fn s(list: &[&str]) -> Vec<String> {
	list.iter().map(|x| x.to_string()).collect()
}

fn graph(rows: &[(&str, &[&str])]) -> Graph {
	rows.iter().map(|(k, v)| (k.to_string(), s(v))).collect()
}

fn root() -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

#[test]
fn a_jupyter_only_diff_requires_its_package_and_kernel() {
	let stages = set(required(&s(&["jupyter/src/worker.rs"]), None, None).unwrap());
	assert!(
		[
			"jupyter-format",
			"jupyter-clippy",
			"jupyter-tests",
			"jupyter-kernel"
		]
		.iter()
		.all(|x| stages.contains(*x))
	);
}

#[test]
fn the_before_graph_retains_removed_dependents() {
	let old = graph(&[
		("Cargo.toml", &["tools/project/Cargo.toml"]),
		("tools/project/Cargo.toml", &[]),
	]);
	let new = graph(&[("Cargo.toml", &[])]);
	let both = [old.clone(), new];
	assert_eq!(
		affected(&s(&["tools/project/Cargo.toml"]), &both),
		old.keys().cloned().collect()
	);
	let stages = set(required(&s(&["tools/project/Cargo.toml"]), Some(&both), None).unwrap());
	assert!(
		["root-tests", "inventory", "project-suite", "cross"]
			.iter()
			.all(|x| stages.contains(*x))
	);
}

#[test]
fn runtime_and_adapter() {
	assert!(
		required(&s(&["rune/crates/rune/src/lib.rs"]), None, None)
			.unwrap()
			.contains(&"fixed-route".to_string())
	);
	assert!(
		!required(&s(&["rune/crates/rune/README.md"]), None, None)
			.unwrap()
			.contains(&"fixed-route".to_string())
	);
	assert!(
		required(&s(&["adapters/postgres/src/lib.rs"]), None, None)
			.unwrap()
			.contains(&"suite-adapter-postgres".to_string())
	);
}

#[test]
fn the_async_test_binary_selects_the_stage_that_runs_it() {
	// root-tests builds without `test-support`, so it compiles
	// tests/async_execution.rs out; async-test-support is what runs it.
	for path in ["tests/async_execution.rs", "tests/harness/scratch.rs"] {
		assert!(
			required(&s(&[path]), None, None)
				.unwrap()
				.contains(&"async-test-support".to_string()),
			"{path}"
		);
	}
	// A neighboring test file is not that binary.
	assert!(
		!required(&s(&["tests/repl.rs"]), None, None)
			.unwrap()
			.contains(&"async-test-support".to_string())
	);
}

#[test]
fn empty_discovery_and_execution_are_refused() {
	assert!(discovered("0 tests").is_err());
	assert!(executed("test result: ok. 0 passed;").is_err());
	assert_eq!(discovered("name: test\n").unwrap(), ["name"]);
	assert_eq!(executed("test result: ok. 1 passed;").unwrap(), 1);
}

#[test]
fn the_root_manifest_keeps_all_new_packages_and_an_unknown_one_fails_closed() {
	let nodes: Graph = package_checks()
		.into_iter()
		.map(|(m, _, _)| (m.to_string(), vec![]))
		.collect();
	let selected = set(required(&s(&["Cargo.toml"]), Some(&[nodes.clone(), nodes]), None).unwrap());
	assert!(
		package_checks()
			.iter()
			.all(|(_, name, _)| selected.contains(*name))
	);
	let unknown = graph(&[("unregistered/Cargo.toml", &[])]);
	assert!(
		required(&s(&["Cargo.toml"]), Some(&[unknown]), None)
			.unwrap_err()
			.to_string()
			.contains("package unregistered/Cargo.toml: declare")
	);
}

#[test]
fn active_tools_have_real_graph_selected_quality_recipes() {
	let canonical: BTreeSet<String> = commands(Path::new("output"), &root())
		.unwrap()
		.into_iter()
		.map(|(n, _)| n)
		.collect();
	for manifest in [
		"rune/tools/xtask/column/probe/Cargo.toml",
		"rune/tools/xtask/measure/harness/Cargo.toml",
		"rune/tools/xtask/Cargo.toml",
	] {
		let g = graph(&[(manifest, &[])]);
		let selected = set(required(&s(&[manifest]), Some(&[g.clone(), g]), None).unwrap());
		let (tests, formatting, lint) = quality(manifest).unwrap();
		let all: Vec<String> = tests.iter().cloned().chain([formatting, lint]).collect();
		assert!(all.iter().all(|x| selected.contains(x)), "{manifest}");
		for stage in &all {
			assert!(canonical.contains(stage), "{stage}");
			assert!(
				recipes(stage, &root())
					.unwrap()
					.iter()
					.all(|c| c.iter().any(|a| a == manifest)),
				"{stage}"
			);
		}
		for command in recipes(&tests[0], &root()).unwrap() {
			let verb = if manifest == "rune/tools/xtask/Cargo.toml" {
				"test"
			} else {
				"check"
			};
			assert_eq!(&command[..3], ["cargo", "+1.98.1", verb]);
			assert!(
				["--locked", "--all-targets", "--all-features"]
					.iter()
					.all(|f| command.iter().any(|a| a == f))
			);
		}
	}
}

#[test]
fn every_registered_package_source_has_format_and_lint() {
	let registered: Vec<String> = packages().into_keys().collect();
	for manifest in registered.iter().cloned().chain(s(&[
		"rune/crates/rune/Cargo.toml",
		"rune/crates/rune-alloc/Cargo.toml",
	])) {
		let path = Path::new(&manifest)
			.parent()
			.unwrap()
			.join("src/lib.rs")
			.to_string_lossy()
			.into_owned();
		let mut manifests = registered.clone();
		manifests.push(manifest.clone());
		for source in [
			path.clone(),
			path.replace("src/lib.rs", "tests/control.rs"),
			path.replace("src/lib.rs", "build.rs"),
		] {
			let stages =
				set(required(std::slice::from_ref(&source), None, Some(&manifests)).unwrap());
			let (tests, formatting, lint) = quality(&manifest).unwrap();
			assert!(
				tests
					.iter()
					.chain([&formatting, &lint])
					.all(|x| stages.contains(x)),
				"{manifest} {source}"
			);
		}
	}
}

#[test]
fn the_longest_owner_and_an_unregistered_source_fail_closed() {
	let manifests = s(&[
		"Cargo.toml",
		"rune/tools/unregistered/Cargo.toml",
		"rune/tools/xtask/measure/unregistered/Cargo.toml",
		"ci/unregistered_guard/Cargo.toml",
	]);
	for (path, package) in [
		(
			"rune/tools/unregistered/src/main.rs",
			"rune/tools/unregistered/Cargo.toml",
		),
		(
			"rune/tools/xtask/measure/unregistered/src/main.rs",
			"rune/tools/xtask/measure/unregistered/Cargo.toml",
		),
		(
			"ci/unregistered_guard/src/main.rs",
			"ci/unregistered_guard/Cargo.toml",
		),
	] {
		let error = required(&s(&[path]), None, Some(&manifests))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains(&format!("{package}: declare")) && error.contains("format and lint"),
			"{error}"
		);
	}
}

#[test]
fn documentation_does_not_gain_quality_stages() {
	let base = ["commit-discipline", "controller-controls", "python-list"];
	assert_eq!(
		required(&s(&["README.md", "plans/adr/example.md"]), None, None).unwrap(),
		base
	);
	let stages = required(&s(&["rune/crates/rune/README.md"]), None, None).unwrap();
	assert!(!stages.contains(&"format".to_string()) && stages.contains(&"rune-clippy".to_string()));
	assert_eq!(
		required(
			&s(&["ci/unregistered_guard/README.md"]),
			None,
			Some(&s(&["Cargo.toml", "ci/unregistered_guard/Cargo.toml"]))
		)
		.unwrap(),
		base
	);
}

#[test]
fn graph_selection_is_closed_under_quality_and_a_removed_owner() {
	for manifest in packages().into_keys() {
		let selected = set(required(
			std::slice::from_ref(&manifest),
			Some(&[graph(&[(&manifest, &[])]), Graph::new()]),
			None,
		)
		.unwrap());
		let (tests, formatting, lint) = quality(&manifest).unwrap();
		assert!(
			tests
				.iter()
				.chain([&formatting, &lint])
				.all(|x| selected.contains(x)),
			"{manifest}"
		);
	}
	let g = graph(&[("ci/unregistered_guard/Cargo.toml", &[])]);
	assert!(
		required(
			&s(&["ci/unregistered_guard/Cargo.toml"]),
			Some(&[g, Graph::new()]),
			None
		)
		.unwrap_err()
		.to_string()
		.contains("ci/unregistered_guard/Cargo.toml: declare")
	);
}

#[test]
fn new_quality_commands_are_canonical_and_formats_precede_compilation() {
	let names: Vec<String> = commands(Path::new("output"), &root())
		.unwrap()
		.into_iter()
		.map(|(n, _)| n)
		.collect();
	let position = |n: &str| names.iter().position(|x| x == n).unwrap();
	for (name, _) in formats() {
		assert!(position(&name) < position("clippy"), "{name}");
	}
	for (name, _) in lints() {
		assert!(names.contains(&name));
		assert_eq!(bulk_roots(&name), [Bulk::Baseline], "{name}");
	}
}
