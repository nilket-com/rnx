//! The entry points a Python test's route names, read from their active
//! declarations: preflight's unittest commands, the measurement suite's
//! `TESTS` list, the supplemental workflow's discovery, the catalogue's
//! stages, and a stage script's command lists. Comments are removed first,
//! so a commented-out call is no declaration; any other form is not
//! recognized. Each `TESTS` count is held to the cases unittest discovers
//! in that file, found once for the whole family.
use crate::catalogue::{self, Stage};
use anyhow::{Context as _, Result};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

pub const PREFLIGHT: &str = "ci/preflight.py";
pub const MEASURE_CHECKS: &str = "rune/tools/xtask/src/measure_checks.rs";
pub const MEASURE: &str = "rune/tools/xtask/measure";
pub const SUPPLEMENTAL: &str = "rune/tools/xtask/supplemental";

/// Every entry point's active declarations, read once.
pub struct Entrypoints {
	pub root: std::path::PathBuf,
	pub commands: Vec<Stage>,
	/// Test files preflight runs by `unittest discover -s ci -p NAME`.
	pub preflight: BTreeSet<String>,
	/// `TESTS`: file name -> declared case count.
	pub tests: BTreeMap<String, u64>,
	/// The case count unittest discovers in each `TESTS` file, or why not.
	pub discovered: BTreeMap<String, std::result::Result<u64, String>>,
	/// The pattern of the supplemental workflow's own discovery, if declared.
	pub supplemental: Option<String>,
}

/// Python source without its comments: whole-line and trailing `#`
/// comments outside string literals.
pub fn python_active(text: &str) -> String {
	strip(text, "#")
}

/// Rust source without its `//` comments outside string literals.
pub fn rust_active(text: &str) -> String {
	strip(text, "//")
}

fn strip(text: &str, marker: &str) -> String {
	let mut out = String::new();
	for line in text.lines() {
		let mut quote: Option<char> = None;
		let mut escaped = false;
		let mut end = line.len();
		for (i, c) in line.char_indices() {
			match quote {
				Some(q) => {
					if escaped {
						escaped = false;
					} else if c == '\\' {
						escaped = true;
					} else if c == q {
						quote = None;
					}
				}
				None if c == '"' || c == '\'' => quote = Some(c),
				None if line[i..].starts_with(marker) => {
					end = i;
					break;
				}
				None => {}
			}
		}
		out.push_str(&line[..end]);
		out.push('\n');
	}
	out
}

static PREFLIGHT_RUN: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"subprocess\.run\(\[sys\.executable,\s*"-B",\s*"-m",\s*"unittest",\s*"discover",\s*"-s",\s*str\(root\s*/\s*"ci"\),\s*"-p",\s*"(test_\w+\.py)"\]"#).unwrap()
});
static TESTS_BLOCK: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"(?s)const TESTS: &\[\(&str, usize\)\] = &\[(.*?)\];"#).unwrap());
static TESTS_ENTRY: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"\(\s*"(test_\w+\.py)",\s*(\d+)\s*\)"#).unwrap());
/// A wrapper's `commands = [ ... ]` declaration, one command per line,
/// closed by `]` on a line of its own.
static COMMANDS_BLOCK: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?ms)^\s*commands\s*=\s*\[\s*$(.*?)^\s*\]\s*$").unwrap());
/// The loop that runs that declaration.
static COMMANDS_LOOP: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"for\s+[\w, ]+\s+in\s+(enumerate\(commands\)|commands)\s*:").unwrap()
});
static SUPPLEMENTAL_RUN: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"execute\(\[sys\.executable,\s*'-m',\s*'unittest',\s*'discover',\s*'-s',\s*str\(HERE\),\s*'-p',\s*'([\w*.]+)'\]"#).unwrap()
});

impl Entrypoints {
	/// Read every entry point under `root`; discover the `TESTS` files'
	/// cases only when `measure` is set.
	pub fn load(root: &Path, measure: bool) -> Result<Self> {
		let commands = catalogue::commands(&root.join("target/unused"), root)?;
		Self::load_with(root, commands, measure)
	}

	/// `load` with the catalogue given, as a control plants entry points.
	pub fn load_with(root: &Path, commands: Vec<Stage>, measure: bool) -> Result<Self> {
		let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_default();
		let preflight = PREFLIGHT_RUN
			.captures_iter(&python_active(&read(PREFLIGHT)))
			.map(|c| c[1].to_owned())
			.collect();
		let checks = rust_active(&read(MEASURE_CHECKS));
		let tests: BTreeMap<String, u64> = TESTS_BLOCK
			.captures(&checks)
			.map(|block| {
				TESTS_ENTRY
					.captures_iter(&block[1])
					.map(|c| (c[1].to_owned(), c[2].parse().unwrap_or(0)))
					.collect()
			})
			.unwrap_or_default();
		let discovered = if measure {
			discover(
				&root.join(MEASURE),
				&tests.keys().cloned().collect::<Vec<_>>(),
			)?
		} else {
			BTreeMap::new()
		};
		let supplemental = SUPPLEMENTAL_RUN
			.captures(&python_active(&read(&format!("{SUPPLEMENTAL}/run.py"))))
			.map(|c| c[1].to_owned());
		Ok(Self {
			root: root.to_owned(),
			commands,
			preflight,
			tests,
			discovered,
			supplemental,
		})
	}

	/// Why a test file's route does not hold, if it does not. A deferred
	/// route runs nowhere and must say why.
	pub fn error(&self, path: &str, route: &str) -> Option<String> {
		let file = Path::new(path);
		let parent = file
			.parent()
			.map(|p| p.to_string_lossy().into_owned())
			.unwrap_or_default();
		let name = file
			.file_name()
			.map(|n| n.to_string_lossy().into_owned())
			.unwrap_or_default();
		if let Some(reason) = route.strip_prefix("deferred: ") {
			return (reason.trim().len() < 30)
				.then(|| format!("{path}: a deferred route states why"));
		}
		match route {
			"preflight" => {
				let runs = parent == "ci" && self.preflight.contains(&name);
				return (!runs).then(|| format!("{path}: ci/preflight.py does not run it"));
			}
			"xtask measure-checks" => {
				let Some(declared) = self.tests.get(&name).filter(|_| parent == MEASURE) else {
					return Some(format!("{path}: measure_checks.rs TESTS does not list it"));
				};
				return match self.discovered.get(&name) {
					Some(Ok(found)) if found == declared => None,
					Some(Ok(found)) => Some(format!(
						"{path}: TESTS declares {declared} cases, discovery finds {found}"
					)),
					Some(Err(error)) => Some(format!("{path}: discovery failed: {error}")),
					None => Some(format!("{path}: not discovered")),
				};
			}
			"workflow supplemental" => {
				let runs = parent == SUPPLEMENTAL
					&& self.supplemental.as_deref().is_some_and(|p| glob(p, &name));
				return (!runs)
					.then(|| format!("{path}: supplemental/run.py does not discover it"));
			}
			_ => {}
		}
		let Some(rest) = route.strip_prefix("final ") else {
			return Some(format!("{path}: unknown route {route}"));
		};
		let (stage, via) = match rest.split_once(" via ") {
			Some((s, v)) => (s, Some(v)),
			None => (rest, None),
		};
		let Some((_, command)) = self.commands.iter().find(|(n, _)| n == stage) else {
			return Some(format!("{path}: no stage {stage}"));
		};
		let runs = match via {
			// The script's `commands` declaration, which it iterates, names
			// the file as an element; a list it does not run is no route.
			Some(script) => {
				let active = python_active(
					&std::fs::read_to_string(self.root.join(script)).unwrap_or_default(),
				);
				let element = Regex::new(&format!(
					r#"[\[,]\s*['"]{}['"]\s*[,\]]"#,
					regex::escape(path)
				))
				.ok();
				let declared = COMMANDS_BLOCK
					.captures(&active)
					.is_some_and(|block| element.is_some_and(|e| e.is_match(&block[1])));
				command.iter().any(|a| a == script) && declared && COMMANDS_LOOP.is_match(&active)
			}
			None => {
				command.iter().any(|a| a == path)
					|| (command.iter().any(|a| a == "discover")
						&& flag(command, "-s") == Some(parent.as_str())
						&& flag(command, "-p").is_some_and(|p| glob(p, &name)))
			}
		};
		(!runs).then(|| format!("{path}: stage {stage} does not run it"))
	}
}

/// The cases unittest's loader finds in each file, without running them,
/// in one interpreter for the family; an import or loader failure is an
/// error, never zero.
pub fn discover(
	directory: &Path,
	names: &[String],
) -> Result<BTreeMap<String, std::result::Result<u64, String>>> {
	discover_with(directory, names, &[])
}

/// The interpreter and isolation `measure_checks.rs` runs these files
/// with: `/usr/bin/python3 -I -B -S`, so neither `PYTHONPATH` nor site
/// setup can substitute a module; the family's directory is the one path
/// the loader adds.
pub const MEASURE_PYTHON: [&str; 4] = ["/usr/bin/python3", "-I", "-B", "-S"];

/// `discover` with extra environment, as a control plants some.
pub fn discover_with(
	directory: &Path,
	names: &[String],
	environment: &[(&str, &str)],
) -> Result<BTreeMap<String, std::result::Result<u64, String>>> {
	const SCRIPT: &str = "import json, sys, unittest\nsys.path.insert(0, sys.argv[1])\nout = {}\nfor name in json.loads(sys.argv[2]):\n    try:\n        suite = unittest.defaultTestLoader.loadTestsFromName(name[:-3])\n        def ids(s):\n            for t in s:\n                yield from (ids(t) if isinstance(t, unittest.TestSuite) else [t.id()])\n        found = list(ids(suite))\n        failed = [i for i in found if 'FailedTest' in i or 'ModuleImportFailure' in i]\n        out[name] = {'error': failed[0]} if failed else {'cases': len(found)}\n    except Exception as e:\n        out[name] = {'error': repr(e)}\nprint(json.dumps(out))";
	let output = std::process::Command::new(MEASURE_PYTHON[0])
		.args(&MEASURE_PYTHON[1..])
		.args(["-c", SCRIPT])
		.arg(directory)
		.arg(serde_json::to_string(names)?)
		.envs(environment.iter().copied())
		.current_dir(directory)
		.output()
		.context(MEASURE_PYTHON[0])?;
	anyhow::ensure!(
		output.status.success(),
		"discovery: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let found: BTreeMap<String, serde_json::Value> = serde_json::from_slice(&output.stdout)?;
	Ok(found
		.into_iter()
		.map(|(name, v)| {
			let result = match v["cases"].as_u64() {
				Some(n) => Ok(n),
				None => Err(v["error"].as_str().unwrap_or("no result").to_owned()),
			};
			(name, result)
		})
		.collect())
}

fn flag<'a>(command: &'a [String], name: &str) -> Option<&'a str> {
	let i = command.iter().position(|a| a == name)?;
	command.get(i + 1).map(String::as_str)
}

/// A discovery pattern with at most one `*`, as `test_*.py`.
fn glob(pattern: &str, name: &str) -> bool {
	match pattern.split_once('*') {
		Some((head, tail)) => {
			name.len() >= head.len() + tail.len() && name.starts_with(head) && name.ends_with(tail)
		}
		None => pattern == name,
	}
}
