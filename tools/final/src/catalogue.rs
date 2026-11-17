//! The final's stages, the package recipes behind them, and which stages a
//! change requires.
use anyhow::{Context as _, Result, bail, ensure};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub type Command = Vec<String>;
pub type Stage = (String, Command);

pub const MANIFESTS: [&str; 8] = [
	"Cargo.toml",
	"tools/project/Cargo.toml",
	"adapters/candle/Cargo.toml",
	"adapters/polars/Cargo.toml",
	"adapters/postgres/Cargo.toml",
	"demos/candle/Cargo.toml",
	"demos/polars/Cargo.toml",
	"servers/http-postgres/Cargo.toml",
];

/// Inputs produced by another stage, not supplied by the checkout. Selection
/// closes over this relation and keeps command order.
pub const PREREQUISITES: [(&str, &[&str]); 2] = [
	("polars-suite", &["polars-inventories"]),
	("production-cli-controls", &["production-cli-build"]),
];

pub fn prerequisites(stage: &str) -> &'static [&'static str] {
	PREREQUISITES
		.iter()
		.find(|(name, _)| *name == stage)
		.map_or(&[], |(_, producers)| producers)
}

/// Only these stages read the plans; every other stage's inputs exclude them.
pub const READS_PLANS: [&str; 1] = ["commit-discipline"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bulk {
	Baseline,
	Fixed,
	Project,
	Cross,
	Contracts,
}

/// Final-owned bulk roots; producers refuse a missing assignment in a final.
pub fn bulk_roots(stage: &str) -> Vec<Bulk> {
	match stage {
		"clippy"
		| "adapter-clippy"
		| "project-clippy"
		| "clippy-adapter-candle"
		| "clippy-adapter-polars"
		| "clippy-adapter-postgres" => vec![Bulk::Baseline],
		"fixed-route" => vec![Bulk::Fixed],
		"generated-projects" => vec![Bulk::Project],
		"cross" => vec![Bulk::Cross],
		"rune-suite" => vec![Bulk::Contracts],
		_ if lints().iter().any(|(name, _)| name == stage) => vec![Bulk::Baseline],
		_ => vec![],
	}
}

fn strings(parts: &[&str]) -> Command {
	parts.iter().map(|s| s.to_string()).collect()
}

fn cargo(rest: &[&str]) -> Command {
	let mut command = strings(&["cargo", "+1.98.1"]);
	command.extend(rest.iter().map(|s| s.to_string()));
	command
}

/// manifest, stage, commands: the existing package recipes, in order.
pub fn package_checks() -> Vec<(&'static str, &'static str, Vec<Command>)> {
	let mut rows = vec![
		(
			"rune/no-std/Cargo.toml",
			"package-no-std",
			vec![strings(&[
				"cargo",
				"+nightly-2026-09-01",
				"run",
				"--locked",
				"--manifest-path",
				"rune/no-std/Cargo.toml",
				"--example",
				"minimal",
			])],
		),
		(
			"rune/examples/Cargo.toml",
			"package-examples",
			vec![cargo(&[
				"check",
				"--locked",
				"--manifest-path",
				"rune/examples/Cargo.toml",
				"--all-targets",
			])],
		),
		(
			"rune/benches/Cargo.toml",
			"package-benches",
			vec![cargo(&[
				"check",
				"--locked",
				"--manifest-path",
				"rune/benches/Cargo.toml",
				"--benches",
			])],
		),
		(
			"rune/hegel_tests/Cargo.toml",
			"package-hegel",
			vec![cargo(&[
				"test",
				"--manifest-path",
				"rune/hegel_tests/Cargo.toml",
			])],
		),
		(
			"rune/tools/xtask/column/probe/Cargo.toml",
			"package-column-probe",
			vec![cargo(&[
				"check",
				"--locked",
				"--all-targets",
				"--all-features",
				"--manifest-path",
				"rune/tools/xtask/column/probe/Cargo.toml",
			])],
		),
		(
			"rune/tools/xtask/measure/harness/Cargo.toml",
			"package-measure-harness",
			vec![cargo(&[
				"check",
				"--locked",
				"--all-targets",
				"--all-features",
				"--manifest-path",
				"rune/tools/xtask/measure/harness/Cargo.toml",
			])],
		),
		(
			"rune/tools/xtask/Cargo.toml",
			"package-xtask",
			vec![cargo(&[
				"test",
				"--locked",
				"--all-targets",
				"--all-features",
				"--manifest-path",
				"rune/tools/xtask/Cargo.toml",
			])],
		),
		(
			"tools/polars-gen/Cargo.toml",
			"package-polars-gen",
			vec![cargo(&[
				"check",
				"--locked",
				"--manifest-path",
				"tools/polars-gen/Cargo.toml",
				"--all-targets",
			])],
		),
	];
	// Packages a root lock change reaches, registered in ADR 0283.
	for (manifest, stage) in [
		("ci/execute_guard/Cargo.toml", "package-execute-guard"),
		("rune/tools/builder/Cargo.toml", "package-builder"),
		("rune/tools/generate/Cargo.toml", "package-generate"),
		("rune/tools/site/Cargo.toml", "package-site"),
	] {
		rows.push((
			manifest,
			stage,
			vec![cargo(&[
				"check",
				"--locked",
				"--manifest-path",
				manifest,
				"--all-targets",
			])],
		));
	}
	rows
}

pub const DEMOS: [&str; 3] = [
	"demos/candle/Cargo.toml",
	"demos/polars/Cargo.toml",
	"servers/http-postgres/Cargo.toml",
];

pub fn demo_checks() -> Vec<(&'static str, String, Vec<Command>)> {
	DEMOS
		.iter()
		.map(|m| {
			(
				*m,
				format!("check-{}", m.replace('/', "-")),
				vec![cargo(&[
					"check",
					"--locked",
					"--all-targets",
					"--manifest-path",
					m,
				])],
			)
		})
		.collect()
}

fn checks() -> Vec<(String, String, Vec<Command>)> {
	let mut rows: Vec<(String, String, Vec<Command>)> = package_checks()
		.into_iter()
		.map(|(m, s, c)| (m.to_string(), s.to_string(), c))
		.collect();
	rows.extend(
		demo_checks()
			.into_iter()
			.map(|(m, s, c)| (m.to_string(), s, c)),
	);
	rows
}

/// Format stages, in their order.
pub fn formats() -> Vec<(String, Command)> {
	let mut rows = vec![(
		"project-format".to_string(),
		cargo(&[
			"fmt",
			"--manifest-path",
			"tools/project/Cargo.toml",
			"--all",
			"--check",
		]),
	)];
	for adapter in ["candle", "postgres"] {
		let manifest = format!("adapters/{adapter}/Cargo.toml");
		rows.push((
			format!("format-adapter-{adapter}"),
			cargo(&["fmt", "--manifest-path", &manifest, "--all", "--check"]),
		));
	}
	for (manifest, stage, _) in checks() {
		rows.push((
			format!("format-{stage}"),
			cargo(&["fmt", "--manifest-path", &manifest, "--all", "--check"]),
		));
	}
	rows
}

/// Lint stages: each package check's commands as Clippy.
pub fn lints() -> Vec<(String, Vec<Command>)> {
	checks()
		.into_iter()
		.map(|(_, stage, commands)| {
			let lints = commands
				.into_iter()
				.map(|command| {
					let mut lint = command.clone();
					lint[2] = "clippy".into();
					if command[2] == "test" {
						lint.push("--tests".into());
					}
					lint
				})
				.collect();
			(format!("lint-{stage}"), lints)
		})
		.collect()
}

/// A package's test or check stages, its format stage and its lint stage.
pub type Quality = (Vec<String>, String, String);

pub fn packages() -> BTreeMap<String, Quality> {
	let owned = |tests: &[&str], format: &str, lint: &str| {
		(
			tests.iter().map(|s| s.to_string()).collect(),
			format.to_string(),
			lint.to_string(),
		)
	};
	let mut rows = BTreeMap::new();
	rows.insert(
		"Cargo.toml".into(),
		owned(&["root-tests", "inventory", "cross"], "format", "clippy"),
	);
	rows.insert(
		"tools/project/Cargo.toml".into(),
		owned(
			&["project-suite", "cross"],
			"project-format",
			"project-clippy",
		),
	);
	rows.insert(
		"jupyter/Cargo.toml".into(),
		owned(
			&["jupyter-tests", "jupyter-kernel"],
			"jupyter-format",
			"jupyter-clippy",
		),
	);
	for adapter in ["candle", "polars", "postgres"] {
		let suite = if adapter == "polars" {
			"polars-suite".to_string()
		} else {
			format!("suite-adapter-{adapter}")
		};
		let format = if adapter == "polars" {
			"adapter-format".to_string()
		} else {
			format!("format-adapter-{adapter}")
		};
		rows.insert(
			format!("adapters/{adapter}/Cargo.toml"),
			owned(&[&suite], &format, &format!("clippy-adapter-{adapter}")),
		);
	}
	for (manifest, stage, _) in checks() {
		rows.insert(
			manifest,
			(
				vec![stage.clone()],
				format!("format-{stage}"),
				format!("lint-{stage}"),
			),
		);
	}
	// New in ADR 0283: no baseline to compare lints with, so they deny warnings.
	rows.insert(
		"rune/tools/budget/Cargo.toml".into(),
		owned(&["budget-controls"], "format-budget", "clippy-budget"),
	);
	rows.insert(
		"tools/final/Cargo.toml".into(),
		owned(&["final-controls"], "format-final", "clippy-final"),
	);
	rows
}

pub fn quality(manifest: &str) -> Result<Quality> {
	if manifest.starts_with("rune/crates/") {
		// The existing Rune-family feature matrix, not a lint of every
		// upstream workspace member.
		return Ok((
			vec!["rune-suite".into(), "cross".into()],
			"format".into(),
			"rune-clippy".into(),
		));
	}
	packages().remove(manifest).with_context(|| {
		format!(
			"package {manifest}: declare its test/check, format and lint recipes in tools/final/src/catalogue.rs"
		)
	})
}

fn relative_to<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
	path.strip_prefix(base).ok()
}

/// The manifest whose sources `path` is, if any.
pub fn source_owner(path: &str, manifests: &[String]) -> Option<String> {
	let path = Path::new(path);
	let manifest = manifests
		.iter()
		.filter(|m| {
			relative_to(
				path,
				Path::new(m.as_str()).parent().unwrap_or(Path::new("")),
			)
			.is_some()
		})
		.max_by_key(|m| Path::new(m.as_str()).components().count())?;
	let relative = relative_to(
		path,
		Path::new(manifest.as_str())
			.parent()
			.unwrap_or(Path::new("")),
	)?;
	let first = relative.components().next()?.as_os_str().to_str()?;
	let single = relative.components().count() == 1;
	if relative.file_name()? == "build.rs"
		|| ["src", "tests", "examples", "benches"].contains(&first)
		|| (single && relative.extension().is_some_and(|e| e == "rs"))
	{
		return Some(manifest.clone());
	}
	None
}

/// Every stage in its order. `out` is the final's directory.
pub fn commands(out: &Path, root: &Path) -> Result<Vec<Stage>> {
	let out_text = |name: &str| out.join(name).to_string_lossy().into_owned();
	let resolved = std::fs::canonicalize(out).unwrap_or_else(|_| out.to_owned());
	let tag = crate::pyjson::hex(&Sha256::digest(resolved.to_string_lossy().as_bytes()));
	let name = out
		.file_name()
		.map(|n| n.to_string_lossy().into_owned())
		.unwrap_or_default();
	// The verifier only permits ignored in-tree output below this directory.
	let cross_out: PathBuf = root
		.join("target/rnx-verify")
		.join(format!("{name}-{}", &tag[..16]))
		.join("payload");
	let python = |rest: &[&str]| -> Command {
		let mut command = strings(&["python3", "-B"]);
		command.extend(rest.iter().map(|s| s.to_string()));
		command
	};
	// Every suite stage runs as the controller's own stage mode.
	let suite = |stage: &str| -> Stage {
		let mut command = strings(&[crate::stage::PROGRAM, "stage", "suite"]);
		command.extend(strings(&["--stage", stage, "--out", &out_text(stage)]));
		(stage.into(), command)
	};
	let mut steps: Vec<Stage> = vec![("commit-discipline".into(), python(&["ci/preflight.py"]))];
	for manifest in MANIFESTS {
		steps.push((
			format!("fetch-{}", manifest.replace('/', "-")),
			cargo(&["fetch", "--locked", "--manifest-path", manifest]),
		));
	}
	// Every tracked Python file is listed, and the list only shrinks.
	steps.push((
		"python-list".into(),
		cargo(&[
			"run",
			"--locked",
			"--release",
			"-p",
			"rnx-final",
			"--",
			"python-list",
		]),
	));
	steps.extend([
		(
			"controller-controls".into(),
			python(&[
				"-m",
				"unittest",
				"discover",
				"-s",
				"tools/column",
				"-p",
				"test_*.py",
				"-v",
			]),
		),
		(
			"polars-inventory-preflight".into(),
			strings(&[
				crate::stage::PROGRAM,
				"stage",
				"inventories",
				"--preflight",
				"--out",
				&out_text("polars-inventory-preflight"),
			]),
		),
		("format".into(), cargo(&["fmt", "--all", "--check"])),
		(
			"adapter-format".into(),
			cargo(&[
				"fmt",
				"--manifest-path",
				"adapters/polars/Cargo.toml",
				"--",
				"--check",
			]),
		),
		(
			"pin-controls".into(),
			strings(&["python3", "-I", "-B", "tools/test_verify_pins.py"]),
		),
		(
			"engine-source-controls".into(),
			strings(&["python3", "-B", "tools/test_engine_source.py"]),
		),
		(
			"census-refresh-controls".into(),
			strings(&["python3", "-B", "tools/test_refresh_census.py"]),
		),
		(
			"benchmark-protocol".into(),
			python(&[
				"-m",
				"unittest",
				"discover",
				"-s",
				"tools/routing-bench",
				"-p",
				"test_*.py",
				"-v",
			]),
		),
		(
			"clippy".into(),
			strings(&[
				crate::stage::PROGRAM,
				"stage",
				"clippy",
				"--out",
				&out_text("clippy"),
			]),
		),
		(
			"adapter-clippy".into(),
			strings(&[
				crate::stage::PROGRAM,
				"stage",
				"clippy",
				"--adapter",
				"--out",
				&out_text("adapter-clippy"),
			]),
		),
		(
			"inventory".into(),
			cargo(&[
				"test",
				"--locked",
				"--features",
				"project-sources",
				"--lib",
				"routing_inventory",
				"--",
				"--test-threads=4",
			]),
		),
		(
			"column-host".into(),
			cargo(&[
				"test",
				"--locked",
				"--no-default-features",
				"--lib",
				"column::",
			]),
		),
		(
			"column-adapter-unit".into(),
			cargo(&[
				"test",
				"--locked",
				"--manifest-path",
				"adapters/polars/Cargo.toml",
				"--features",
				"test-support",
				"--lib",
				"column::",
			]),
		),
		(
			"column-adapter-integration".into(),
			cargo(&[
				"test",
				"--locked",
				"--manifest-path",
				"adapters/polars/Cargo.toml",
				"--features",
				"test-support",
				"--test",
				"column",
			]),
		),
		(
			"root-tests".into(),
			cargo(&[
				"test",
				"--no-fail-fast",
				"--locked",
				"--features",
				"project-sources",
				"--",
				"--test-threads=4",
			]),
		),
		(
			"async-test-support".into(),
			cargo(&[
				"test",
				"--locked",
				"--features",
				"project-sources,test-support",
				"--test",
				"async_execution",
				"--",
				"--test-threads=4",
			]),
		),
		(
			"generated-projects".into(),
			cargo(&[
				"test",
				"--locked",
				"--features",
				"project-sources",
				"--test",
				"routing_projects",
				"--",
				"--ignored",
				"--test-threads=1",
			]),
		),
		(
			"jupyter-format".into(),
			cargo(&["fmt", "--manifest-path", "jupyter/Cargo.toml", "--check"]),
		),
		(
			"jupyter-clippy".into(),
			cargo(&[
				"clippy",
				"--locked",
				"--manifest-path",
				"jupyter/Cargo.toml",
				"--all-targets",
				"--features",
				"transport-probe",
				"--",
				"-D",
				"warnings",
			]),
		),
		(
			"jupyter-tests".into(),
			cargo(&[
				"test",
				"--locked",
				"--manifest-path",
				"jupyter/Cargo.toml",
				"--features",
				"transport-probe",
			]),
		),
		(
			"jupyter-kernel".into(),
			strings(&[
				crate::stage::PROGRAM,
				"stage",
				"jupyter",
				"--out",
				&out_text("jupyter-kernel"),
			]),
		),
	]);
	for (stage, manifest) in [
		("budget", "rune/tools/budget/Cargo.toml"),
		("final", "tools/final/Cargo.toml"),
	] {
		let tests = if stage == "budget" {
			"budget-controls"
		} else {
			"final-controls"
		};
		steps.push((
			tests.into(),
			cargo(&["test", "--locked", "--manifest-path", manifest]),
		));
		steps.push((
			format!("format-{stage}"),
			cargo(&["fmt", "--manifest-path", manifest, "--check"]),
		));
		steps.push((
			format!("clippy-{stage}"),
			cargo(&[
				"clippy",
				"--locked",
				"--manifest-path",
				manifest,
				"--all-targets",
				"--",
				"-D",
				"warnings",
			]),
		));
	}
	for adapter in ["candle", "polars", "postgres"] {
		let manifest = format!("adapters/{adapter}/Cargo.toml");
		steps.push((
			format!("adapter-{adapter}"),
			cargo(&[
				"test",
				"--locked",
				"--manifest-path",
				&manifest,
				"--test",
				"execution_routing",
				"--",
				"--test-threads=1",
			]),
		));
	}
	steps.push((
		"polars-inventories".into(),
		strings(&[
			crate::stage::PROGRAM,
			"stage",
			"inventories",
			"--out",
			&out_text("polars-inventories"),
		]),
	));
	steps.push((
		"polars-suite".into(),
		cargo(&[
			"test",
			"--locked",
			"--manifest-path",
			"adapters/polars/Cargo.toml",
			"--features",
			"test-support",
			"--",
			"--test-threads=4",
		]),
	));
	for manifest in &MANIFESTS[MANIFESTS.len() - 3..] {
		steps.push((
			format!("check-{}", manifest.replace('/', "-")),
			cargo(&[
				"check",
				"--locked",
				"--all-targets",
				"--manifest-path",
				manifest,
			]),
		));
	}
	steps.push((
		"cross".into(),
		python(&[
			"tools/verify.py",
			"cross",
			"--suite",
			"all",
			"--out",
			&cross_out.to_string_lossy(),
		]),
	));
	steps.push((
		"production-cli-build".into(),
		cargo(&[
			"build",
			"--locked",
			"--release",
			"--manifest-path",
			"adapters/polars/Cargo.toml",
			"--bin",
			"rnx-polars",
		]),
	));
	let binary = root.join("adapters/polars/target/release/rnx-polars");
	steps.push((
		"production-cli-controls".into(),
		strings(&[
			crate::stage::PROGRAM,
			"stage",
			"cli",
			"--binary",
			&binary.to_string_lossy(),
			"--out",
			&out_text("cli"),
		]),
	));
	for stage in [
		"rune-suite",
		"rune-clippy",
		"project-suite",
		"project-clippy",
		"suite-adapter-candle",
		"suite-adapter-postgres",
		"clippy-adapter-candle",
		"clippy-adapter-polars",
		"clippy-adapter-postgres",
	] {
		steps.push(suite(stage));
	}
	for (_, stage, _) in package_checks() {
		steps.push(suite(stage));
	}
	steps.push((
		"fixed-route".into(),
		python(&[
			"tools/column/fixed_route_gate.py",
			"--out",
			&out_text("fixed-route"),
		]),
	));
	// Every package format check precedes compilation, lint and tests.
	let position = steps
		.iter()
		.position(|(name, _)| name == "pin-controls")
		.expect("pin-controls is listed");
	let quality: Vec<Stage> = formats().iter().map(|(name, _)| suite(name)).collect();
	steps.splice(position..position, quality);
	let position = steps
		.iter()
		.position(|(name, _)| name == "inventory")
		.expect("inventory is listed");
	let quality: Vec<Stage> = lints().iter().map(|(name, _)| suite(name)).collect();
	steps.splice(position..position, quality);
	let names: BTreeSet<&String> = steps.iter().map(|(name, _)| name).collect();
	ensure!(names.len() == steps.len(), "duplicate stage definition");
	Ok(steps)
}

pub fn graph_change(paths: &[String]) -> bool {
	paths.iter().any(|p| {
		matches!(
			Path::new(p).file_name().and_then(|n| n.to_str()),
			Some("Cargo.toml" | "Cargo.lock")
		)
	})
}

/// Stages a change requires from its paths alone.
pub fn direct(paths: &[String], manifests: Option<&[String]>) -> Result<BTreeSet<String>> {
	let mut stages: BTreeSet<String> = ["commit-discipline", "controller-controls", "python-list"]
		.map(String::from)
		.into();
	let mut add = |names: &[&str]| stages.extend(names.iter().map(|s| s.to_string()));
	for p in paths {
		if p.starts_with("src/") || p.starts_with("tests/") {
			add(&["root-tests", "inventory", "cross"]);
		}
		if p.starts_with("rune/crates/") {
			add(&["rune-suite", "rune-clippy", "cross"]);
			if p.contains("/src/") {
				add(&["fixed-route"]);
			}
		}
		if let Some(rest) = p.strip_prefix("adapters/") {
			let name = rest.split('/').next().unwrap_or("");
			if !["candle", "polars", "postgres"].contains(&name) {
				quality(&format!("adapters/{name}/Cargo.toml"))?;
			}
			// The Polars package suite is polars-suite; it needs the generated inventories.
			let suite = if name == "polars" {
				"polars-suite".to_string()
			} else {
				format!("suite-adapter-{name}")
			};
			add(&[&suite, &format!("clippy-adapter-{name}")]);
		}
		if p.starts_with("jupyter/") {
			add(&[
				"jupyter-format",
				"jupyter-clippy",
				"jupyter-tests",
				"jupyter-kernel",
			]);
		}
		if p.starts_with("tools/project/") {
			add(&["project-suite", "project-clippy", "cross"]);
		}
		if p.starts_with("tools/verify/") || p == "tools/verify.py" {
			add(&["cross"]);
		}
		if p == "tools/engine_source.py" || p == "tools/test_engine_source.py" {
			add(&["engine-source-controls"]);
		}
		if p == "tools/refresh_census.py" || p == "tools/test_refresh_census.py" {
			add(&["census-refresh-controls"]);
		}
	}
	let manifests: Vec<String> = match manifests {
		Some(m) => m.to_vec(),
		None => {
			let mut m: Vec<String> = packages().into_keys().collect();
			// Python keeps the package table's insertion order; owners do not depend on it.
			for p in paths {
				let parts: Vec<&str> = Path::new(p).iter().filter_map(|c| c.to_str()).collect();
				if p.starts_with("rune/crates/") && parts.len() > 3 {
					m.push(format!("{}/Cargo.toml", parts[..3].join("/")));
				}
			}
			m
		}
	};
	for p in paths {
		if let Some(owner) = source_owner(p, &manifests) {
			let (tests, format, lint) = quality(&owner)?;
			stages.extend(tests);
			stages.insert(format);
			stages.insert(lint);
		}
	}
	Ok(stages)
}

pub type Graph = BTreeMap<String, Vec<String>>;

/// Packages a change reaches, through either side's path-dependency graph.
pub fn affected(paths: &[String], graphs: &[Graph]) -> BTreeSet<String> {
	let mut selected = BTreeSet::new();
	for nodes in graphs {
		let mut seeds: BTreeSet<String> = BTreeSet::new();
		for manifest in nodes.keys() {
			let parent = Path::new(manifest).parent().unwrap_or(Path::new(""));
			for path in paths {
				let path = Path::new(path);
				let name = path.file_name().and_then(|n| n.to_str());
				if matches!(name, Some("Cargo.toml" | "Cargo.lock")) {
					// Ancestor manifests and locks can alter every enclosed package.
					if parent.starts_with(path.parent().unwrap_or(Path::new(""))) {
						seeds.insert(manifest.clone());
					}
				} else if path.starts_with(parent) {
					seeds.insert(manifest.clone());
				}
			}
		}
		loop {
			let next: BTreeSet<String> = seeds
				.iter()
				.cloned()
				.chain(
					nodes
						.iter()
						.filter(|(_, deps)| deps.iter().any(|d| seeds.contains(d)))
						.map(|(p, _)| p.clone()),
				)
				.collect();
			if next == seeds {
				break;
			}
			seeds = next;
		}
		selected.extend(seeds);
	}
	selected
}

/// Every stage a change requires, sorted.
pub fn required(
	paths: &[String],
	graphs: Option<&[Graph]>,
	manifests: Option<&[String]>,
) -> Result<Vec<String>> {
	let derived: Option<Vec<String>> = match (manifests, graphs) {
		(None, Some(graphs)) => Some(
			graphs
				.iter()
				.flat_map(|g| g.keys().cloned())
				.collect::<BTreeSet<_>>()
				.into_iter()
				.collect(),
		),
		_ => None,
	};
	let manifests = manifests.or(derived.as_deref());
	let mut stages = direct(paths, manifests)?;
	if !graph_change(paths) {
		return Ok(stages.into_iter().collect());
	}
	stages.insert("cross".into());
	let Some(graphs) = graphs else {
		bail!("manifest/lock change requires before and after Cargo graphs")
	};
	let checks = package_checks();
	for manifest in affected(paths, graphs) {
		let (tests, format, lint) = quality(&manifest)?;
		stages.extend(tests);
		stages.insert(format);
		stages.insert(lint);
		let mut add = |names: &[&str]| stages.extend(names.iter().map(|s| s.to_string()));
		if manifest == "Cargo.toml" {
			add(&["root-tests", "inventory"]);
		} else if manifest.starts_with("rune/crates/") {
			add(&["rune-suite", "rune-clippy", "fixed-route"]);
		} else if manifest.starts_with("adapters/") {
			let parent = Path::new(&manifest)
				.parent()
				.unwrap_or(Path::new(""))
				.join("src/lib.rs");
			stages.extend(direct(&[parent.to_string_lossy().into_owned()], None)?);
		} else if manifest == "jupyter/Cargo.toml" {
			add(&[
				"jupyter-format",
				"jupyter-clippy",
				"jupyter-tests",
				"jupyter-kernel",
			]);
		} else if manifest == "tools/project/Cargo.toml" {
			add(&["project-suite", "project-clippy"]);
		} else if let Some((_, stage, _)) = checks.iter().find(|(m, _, _)| *m == manifest) {
			add(&[stage]);
		} else if DEMOS.contains(&manifest.as_str()) {
			add(&[&format!("check-{}", manifest.replace('/', "-"))]);
		} else if manifest.starts_with("ci/") || manifest.starts_with("rune/tools/") {
			add(&["controller-controls"]);
		} else if manifest == "tools/final/Cargo.toml" {
			// The final controller: its quality stages, added above.
		} else {
			bail!("affected package needs a registered suite: {manifest}");
		}
	}
	Ok(stages.into_iter().collect())
}

/// Each API boundary's doc tests: a compiling twin and its compile-fail
/// controls, by selector and their exact count.
pub const TWINS: [(&str, usize); 6] = [
	("host_function_owned_output", 4),
	("host_function_string_input", 8),
	("ExecutionRuntime", 2),
	("function::host::HostOutput", 4),
	("opaque_factory", 2),
	("opaque_method", 5),
];

/// The Cargo commands a suite stage runs.
pub fn recipes(stage: &str, root: &Path) -> Result<Vec<Command>> {
	if let Some((_, command)) = formats().into_iter().find(|(name, _)| name == stage) {
		return Ok(vec![command]);
	}
	if let Some((_, commands)) = lints().into_iter().find(|(name, _)| name == stage) {
		return Ok(commands);
	}
	if let Some((_, _, commands)) = package_checks()
		.into_iter()
		.find(|(_, name, _)| *name == stage)
	{
		return Ok(commands);
	}
	if stage == "rune-suite" || stage == "rune-clippy" {
		let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(
			root.join("rune/tools/xtask/manifests/native.json"),
		)?)?;
		if stage == "rune-clippy" {
			return Ok(manifest["clippy_features"]
				.as_array()
				.into_iter()
				.flatten()
				.map(|f| {
					cargo(&[
						"clippy",
						"--locked",
						"-p",
						"rune",
						"--profile",
						"engine-dev",
						"--lib",
						"--tests",
						"--features",
						f.as_str().unwrap_or_default(),
						"--",
						"-D",
						"warnings",
					])
				})
				.collect());
		}
		let mut commands: Vec<Command> = manifest["libraries"]
			.as_array()
			.into_iter()
			.flatten()
			.map(|row| {
				let mut command = cargo(&[
					"test",
					"--locked",
					"-p",
					"rune",
					"--profile",
					"engine-dev",
					"--lib",
					"--features",
					row["features"].as_str().unwrap_or_default(),
				]);
				if let Some(selector) = row
					.get("selector")
					.and_then(|s| s.as_str())
					.filter(|s| !s.is_empty())
				{
					command.push(selector.into());
				}
				command
			})
			.collect();
		for (selector, _) in TWINS {
			commands.push(cargo(&[
				"test",
				"--locked",
				"-p",
				"rune",
				"--profile",
				"engine-dev",
				"--doc",
				"--features",
				"fmt,workspace,execution",
				selector,
			]));
		}
		return Ok(commands);
	}
	let manifest = if stage.starts_with("suite-adapter-") || stage.starts_with("clippy-adapter-") {
		let name = stage.rsplit('-').next().unwrap_or_default();
		ensure!(
			["polars", "postgres", "candle"].contains(&name),
			"unknown adapter"
		);
		ensure!(
			stage != "suite-adapter-polars",
			"the Polars package suite is polars-suite"
		);
		format!("adapters/{name}/Cargo.toml")
	} else if stage == "project-suite" || stage == "project-clippy" {
		"tools/project/Cargo.toml".into()
	} else {
		bail!("unknown suite stage");
	};
	let verb = if stage.contains("clippy") {
		"clippy"
	} else {
		"test"
	};
	Ok(vec![cargo(&[
		verb,
		"--locked",
		"--manifest-path",
		&manifest,
		"--features",
		"test-support",
		"--lib",
		"--tests",
	])])
}

/// The stage list, bulk roots and required stages against the Python
/// controller's at ADR 0284's witness revision.
#[cfg(test)]
mod witness {
	use super::*;
	use crate::witness::expected;
	use serde_json::{Value, json};

	fn root() -> PathBuf {
		Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../..")
			.canonicalize()
			.unwrap()
	}

	#[test]
	fn the_stage_list_matches_the_witness() {
		let out = std::env::temp_dir().join("rnx-final-witness/FINAL-x");
		let tag = crate::pyjson::hex(&Sha256::digest(
			std::fs::canonicalize(&out)
				.unwrap_or_else(|_| out.clone())
				.to_string_lossy()
				.as_bytes(),
		));
		let fixture = expected(
			"catalogue",
			&[
				("<OUT>", &out.to_string_lossy()),
				("<ROOT>", &root().to_string_lossy()),
				("<TAG>", &tag[..16]),
			],
		);
		let actual: Vec<Value> = commands(&out, &root())
			.unwrap()
			.into_iter()
			.map(|(n, c)| json!([n, c]))
			.collect();
		let witness = fixture["stages"].as_array().unwrap();
		for (i, (a, w)) in actual.iter().zip(witness).enumerate() {
			assert_eq!(a, w, "stage {i}");
		}
		assert_eq!(actual.len(), witness.len());
		for (name, _) in commands(&out, &root()).unwrap() {
			let rust_bulk: Vec<&str> = bulk_roots(&name)
				.into_iter()
				.map(|b| match b {
					Bulk::Baseline => "baseline",
					Bulk::Fixed => "fixed",
					Bulk::Project => "project",
					Bulk::Cross => "cross",
					Bulk::Contracts => "contracts",
				})
				.collect();
			assert_eq!(json!(rust_bulk), fixture["bulk"][&name], "{name}");
		}
	}

	#[test]
	fn required_stages_match_the_witness() {
		let fixture = expected("required", &[("<RECIPES>", "tools/final/src/catalogue.rs")]);
		let graph: Graph = serde_json::from_value(fixture["graph"].clone()).unwrap();
		let cases = fixture["cases"].as_array().unwrap();
		assert_eq!(cases.len(), 30);
		for case in cases {
			let paths: Vec<String> = serde_json::from_value(case["paths"].clone()).unwrap();
			assert_eq!(graph_change(&paths), case["graphs"] == true, "{paths:?}");
			let graphs = graph_change(&paths).then(|| vec![graph.clone(), graph.clone()]);
			let actual = match required(&paths, graphs.as_deref(), None) {
				Ok(stages) => json!(stages),
				Err(error) => json!({"error": error.to_string()}),
			};
			assert_eq!(actual, case["expected"], "{paths:?}");
		}
		// An unregistered adapter is refused.
		assert!(
			required(&["adapters/duckdb/src/lib.rs".to_string()], None, None)
				.unwrap_err()
				.to_string()
				.contains("declare its test/check")
		);
	}

	/// The fresh catalogue keeps the inherited gates and orders the early
	/// lints first (formerly a Python control of `accept.commands`).
	#[test]
	fn the_catalogue_keeps_inherited_gates_and_early_lints() {
		let steps = commands(Path::new("/unused/acceptance"), &root()).unwrap();
		let names: Vec<&str> = steps.iter().map(|(n, _)| n.as_str()).collect();
		let index = |n: &str| {
			names
				.iter()
				.position(|x| *x == n)
				.unwrap_or_else(|| panic!("{n}"))
		};
		let unique: std::collections::BTreeSet<&&str> = names.iter().collect();
		assert_eq!(unique.len(), names.len(), "duplicate stage");
		assert_eq!(names[0], "commit-discipline");
		assert!(index("format") < index("clippy"));
		assert!(index("polars-inventories") + 1 == index("polars-suite"));
		for name in [
			"format",
			"clippy",
			"root-tests",
			"async-test-support",
			"generated-projects",
			"cross",
		] {
			index(name);
		}
	}
}
