//! ADR 0284 deleted the Python final controller and retired remote
//! acceptance (`accept_remote`). No current file names one
//! of its programs as a path or imports one, and every script a stage runs
//! exists. History is exempt: `plans/` and `probes/` describe what ran
//! then, the witness fixtures (`tools/final/tests/witness`) record how the
//! deleted controller computed their expected values, and CONTROLS.txt
//! keeps its controls' names as tombstones. The producer registries keep
//! the deleted programs' bare names so receipts they wrote stay readable;
//! a bare name is neither a path nor an import.
use regex::Regex;
use std::path::{Path, PathBuf};

const DELETED: [&str; 9] = [
	"final",
	"accept",
	"required_stages",
	"stage_outputs",
	"stage_fakes",
	"chain",
	"resume",
	"recover_cross",
	"accept_remote",
];

const HISTORY: [&str; 4] = [
	"plans/",
	"probes/",
	"tools/final/tests/witness/",
	"tools/final/CONTROLS.txt",
];

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

fn tracked(root: &Path) -> Vec<String> {
	let output = std::process::Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["ls-files", "-z"])
		.output()
		.unwrap();
	assert!(output.status.success());
	output
		.stdout
		.split(|b| *b == 0)
		.filter(|p| !p.is_empty())
		.map(|p| String::from_utf8_lossy(p).into_owned())
		.collect()
}

/// Every place a current file names a deleted program, by file.
fn references(root: &Path, files: &[String]) -> Vec<String> {
	// A path to a deleted program anywhere; an import of one in Python, or
	// in the Python a Rust test embeds (the vendored engine has none).
	let paths: Vec<Regex> = DELETED
		.iter()
		.map(|d| {
			// Bare, or under tools/column; another directory's program of the
			// same name (probes/0116/final.py) is not this one.
			let d = regex::escape(d);
			Regex::new(&format!(r"(?m)(^|[^/\w]|tools/column/){d}\.py\b")).unwrap()
		})
		.collect();
	let imports: Vec<Regex> = DELETED
		.iter()
		.map(|d| {
			let d = regex::escape(d);
			Regex::new(&format!(
				r"(?m)(^|[;\s\x22])(import\s+([\w.]+(\s+as\s+\w+)?\s*,\s*)*{d}\b|from\s+{d}\s+import\b)"
			))
			.unwrap()
		})
		.collect();
	let mut found = Vec::new();
	for file in files {
		if HISTORY.iter().any(|h| file.starts_with(h)) || file == "tools/final/tests/deleted.rs" {
			continue;
		}
		let Ok(text) = std::fs::read_to_string(root.join(file)) else {
			continue;
		};
		let python =
			file.ends_with(".py") || (file.ends_with(".rs") && !file.starts_with("rune/crates/"));
		let applicable = paths.iter().chain(imports.iter().filter(|_| python));
		for pattern in applicable {
			if let Some(m) = pattern.find(&text) {
				found.push(format!("{file}: {}", m.as_str().trim()));
			}
		}
	}
	found
}

#[test]
fn no_current_file_names_a_deleted_program() {
	let root = root();
	for d in DELETED {
		assert!(
			!root.join(format!("tools/column/{d}.py")).exists(),
			"tools/column/{d}.py exists"
		);
	}
	let found = references(&root, &tracked(&root));
	assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_planted_reference_is_found() {
	let root = std::env::temp_dir().join(format!(
		"rnx-final-deleted-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir_all(root.join("plans")).unwrap();
	std::fs::create_dir_all(root.join("rune/crates")).unwrap();
	let cases = [
		("a.py", "import accept\n", true),
		("b.py", "import json, gate_policy as g, final\n", true),
		("c.py", "from stage_outputs import finish_stage\n", true),
		(
			"d.rs",
			"let s = \"import json; import final; final.run()\";\n",
			true,
		),
		("e.md", "Run `tools/column/final.py --complete`.\n", true),
		("f.sh", "python3 tools/column/resume.py --prior x\n", true),
		("plans/g.md", "tools/column/final.py ran then.\n", false),
		("h.py", "PRODUCERS = ('final', 'chain')\n", false),
		(
			"i.py",
			"import finality\nfrom final_evidence import canonical\n",
			false,
		),
		("j.rs", "// the final's controller; final.sh\n", false),
		("rune/crates/k.rs", "/// a step in an import chain\n", false),
		("l.txt", "probes/0116/final.py\tdelete\n", false),
		("m.md", "see `chain.py` for the driver\n", true),
	];
	let mut files = Vec::new();
	for (name, text, _) in cases {
		std::fs::write(root.join(name), text).unwrap();
		files.push(name.to_string());
	}
	let found = references(&root, &files);
	for (name, _, planted) in cases {
		assert_eq!(
			found.iter().any(|f| f.starts_with(&format!("{name}:"))),
			planted,
			"{name}: {found:?}"
		);
	}
	let _ = std::fs::remove_dir_all(&root);
}

/// Every Python script a stage of the complete final runs is a tracked file.
#[test]
fn every_stage_script_exists() {
	let root = root();
	let files = tracked(&root);
	for (name, command) in
		rnx_final::catalogue::commands(&root.join("target/unused"), &root).unwrap()
	{
		// A unittest discovery pattern is not a file.
		for arg in command
			.iter()
			.filter(|a| a.ends_with(".py") && !a.contains('*'))
		{
			assert!(
				files.iter().any(|f| f == arg),
				"{name} runs {arg}, which is not a tracked file"
			);
		}
	}
}

/// The probe scripts ADR 0285 deleted, as its committed tombstone lists them.
fn retired(root: &Path) -> Vec<String> {
	std::fs::read_to_string(root.join("tools/final/tests/retired/probes-0285.txt"))
		.unwrap()
		.lines()
		.filter(|l| !l.starts_with('#') && !l.trim().is_empty())
		.map(str::to_owned)
		.collect()
}

/// The reviewed history that names a retired script: each file, an exact
/// line, and how many times that line occurs. Nothing else may name one.
const HISTORY_LINES: [(&str, &str, usize); 4] = [
	(
		"tools/polars-gen/README.md",
		"mismatch. Record 0093's census (`probes/0093/census.py` at 107e2bde,",
		1,
	),
	(
		"adapters/candle/tests/interchange.rs",
		"//! (`tests/data/0143`, written by `probes/0143/fixtures.py` at 107e2bde with",
		1,
	),
	(
		"tools/polars-gen/releases/0.55.2-joins.toml",
		"cite = \"record 0118 stage 4: the in-memory reader over script Bytes (Cursor<Vec<u8>>); census path in probes/0118/census.py\"",
		3,
	),
	(
		"tools/polars-gen/releases/0.55.2-joins.toml",
		"cite = \"record 0118 stage 4: the in-memory writer into a polars::Sink; census path in probes/0118/census.py\"",
		5,
	),
];

/// Every line of a current file that names a retired script, by its path
/// or as a dotted module (`probes.0143.fixtures`, or `from probes.0143
/// import fixtures`), other than the reviewed history lines; and every
/// reviewed line whose count changed.
fn retired_references(root: &Path, files: &[String], retired: &[String]) -> Vec<String> {
	let dotted: Vec<(String, Regex)> = retired
		.iter()
		.map(|p| {
			let stem = p.trim_end_matches(".py");
			let (package, module) = stem.rsplit_once('/').unwrap();
			let (package, module) = (package.replace('/', "."), module.to_owned());
			let (p_, m_) = (regex::escape(&package), regex::escape(&module));
			let pattern = format!(r"\b{p_}\.{m_}\b|\bfrom\s+{p_}\s+import\s+([\w]+\s*,\s*)*{m_}\b");
			(p.clone(), Regex::new(&pattern).unwrap())
		})
		.collect();
	let mut found = Vec::new();
	let mut seen: std::collections::BTreeMap<(&str, &str), usize> = Default::default();
	for file in files {
		if file.starts_with("plans/")
			|| file.starts_with("probes/")
			|| file.starts_with("tools/final/tests/retired/")
			|| file == "tools/final/tests/deleted.rs"
		{
			continue;
		}
		let Ok(text) = std::fs::read_to_string(root.join(file)) else {
			continue;
		};
		if !text.contains("probes/") && !text.contains("probes.") {
			continue;
		}
		for line in text.lines() {
			let named = retired
				.iter()
				.find(|p| line.contains(p.as_str()))
				.cloned()
				.or_else(|| {
					dotted
						.iter()
						.find(|(_, r)| r.is_match(line))
						.map(|(p, _)| p.clone())
				});
			let Some(path) = named else { continue };
			match HISTORY_LINES
				.iter()
				.find(|(f, l, _)| f == file && *l == line)
			{
				Some((f, l, _)) => *seen.entry((f, l)).or_default() += 1,
				None => found.push(format!("{file}: {path}: {}", line.trim())),
			}
		}
	}
	for (f, l, count) in HISTORY_LINES {
		let got = seen.get(&(f, l)).copied().unwrap_or(0);
		if files.iter().any(|x| x == f) && got != count {
			found.push(format!(
				"{f}: reviewed history line occurs {got} times, not {count}"
			));
		}
	}
	found
}

#[test]
fn no_retired_probe_script_is_tracked_or_named() {
	let root = root();
	let files = tracked(&root);
	let retired = retired(&root);
	assert_eq!(retired.len(), 151);
	let still: Vec<&String> = retired.iter().filter(|p| files.contains(p)).collect();
	assert!(still.is_empty(), "still tracked: {still:?}");
	let found = retired_references(&root, &files, &retired);
	assert!(found.is_empty(), "{found:#?}");
	// Every reviewed history file is current.
	for (f, _, _) in HISTORY_LINES {
		assert!(files.iter().any(|x| x == f), "{f}");
	}
}

#[test]
fn only_the_reviewed_history_names_a_retired_script() {
	let root = std::env::temp_dir().join(format!(
		"rnx-final-retired-{}",
		rnx_final::receipt::invocation_id()
	));
	for d in ["tools/polars-gen/releases", "adapters/candle/tests"] {
		std::fs::create_dir_all(root.join(d)).unwrap();
	}
	let retired = vec![
		"probes/0143/fixtures.py".to_string(),
		"probes/0118/census.py".to_string(),
		"probes/0093/census.py".to_string(),
	];
	let toml = "tools/polars-gen/releases/0.55.2-joins.toml";
	let cites = format!(
		"{}\n{}\n{}\n",
		HISTORY_LINES[2].1, HISTORY_LINES[2].1, HISTORY_LINES[2].1
	) + &format!("{}\n", HISTORY_LINES[3].1).repeat(5);
	// The reviewed files, each exactly as reviewed, pass.
	let reviewed = [
		(
			"tools/polars-gen/README.md".to_string(),
			format!("{}\n", HISTORY_LINES[0].1),
		),
		(
			"adapters/candle/tests/interchange.rs".to_string(),
			format!("{}\n", HISTORY_LINES[1].1),
		),
		(toml.to_string(), cites.clone()),
	];
	let write = |files: &[(String, String)]| -> Vec<String> {
		for (name, text) in files {
			std::fs::write(root.join(name), text).unwrap();
		}
		files.iter().map(|(n, _)| n.clone()).collect()
	};
	let names = write(&reviewed);
	assert!(retired_references(&root, &names, &retired).is_empty());
	// Each of these is flagged.
	let planted: [(&str, String); 11] = [
		(
			"a.py",
			"importlib.import_module(\"probes.0143.fixtures\")\n".into(),
		),
		("b.py", "from probes.0143 import fixtures\n".into()),
		(
			"c.rs",
			"//! written by `probes/0143/fixtures.py` at 107e2bde\n".into(),
		),
		(
			"d.md",
			"Record 0143's generator, `probes/0143/fixtures.py` at 107e2bde.\n".into(),
		),
		(
			"e.md",
			"python3 probes/0143/fixtures.py # 107e2bde\n".into(),
		),
		(
			"f.md",
			"~~~sh\npython3 probes/0143/fixtures.py\n~~~\n".into(),
		),
		("g.md", "    python3 probes/0143/fixtures.py\n".into()),
		(
			"h.rs",
			"let c = [\"python3\", \"probes/0143/fixtures.py\"]; // at 107e2bde\n".into(),
		),
		(
			toml,
			cites.clone() + "cite = \"record 0118 stage 4: probes/0143/fixtures.py\"\n",
		),
		(toml, cites.replace("Cursor<Vec<u8>>", "Cursor<u8>")),
		(
			"adapters/candle/tests/interchange.rs",
			format!("{}\n{}\n", HISTORY_LINES[1].1, HISTORY_LINES[1].1),
		),
	];
	for (name, text) in planted {
		let mut files = reviewed.to_vec();
		files.retain(|(n, _)| n != name);
		files.push((name.to_string(), text));
		let names = write(&files);
		let found = retired_references(&root, &names, &retired);
		assert!(!found.is_empty(), "{name}: not flagged");
		write(&reviewed);
	}
	let _ = std::fs::remove_dir_all(&root);
}

/// The finished-record instruments and data ADR 0286 retired.
fn retired_0286(root: &Path) -> Vec<String> {
	std::fs::read_to_string(root.join("tools/final/tests/retired/tools-0286.txt"))
		.unwrap()
		.lines()
		.filter(|l| !l.starts_with('#') && !l.trim().is_empty())
		.map(str::to_owned)
		.collect()
}

/// Every current line that names a file ADR 0286 retired: by its path, by
/// its path inside the Rune tree (without `rune/`), or, for a module, by an
/// import of its name. Plans of either tree are history.
fn references_0286(root: &Path, files: &[String], retired: &[String]) -> Vec<String> {
	let mut patterns = Vec::new();
	for path in retired {
		patterns.push((path.clone(), Regex::new(&regex::escape(path)).unwrap()));
		if let Some(inner) = path.strip_prefix("rune/") {
			patterns.push((
				path.clone(),
				Regex::new(&format!(r"(^|[^/\w]){}", regex::escape(inner))).unwrap(),
			));
		}
		if let Some(stem) = Path::new(path)
			.file_name()
			.and_then(|n| n.to_str())
			.and_then(|n| n.strip_suffix(".py"))
		{
			let s = regex::escape(stem);
			patterns.push((
				path.clone(),
				Regex::new(&format!(
					r"(?m)(^|[;\s\x22])(import\s+([\w.]+\s*,\s*)*{s}\b|from\s+{s}\s+import\b)"
				))
				.unwrap(),
			));
		}
	}
	let mut found = Vec::new();
	for file in files {
		if file.starts_with("plans/")
			|| file.starts_with("probes/")
			|| file.starts_with("rune/plans/")
			|| file.starts_with("tools/final/tests/retired/")
			|| file == "tools/final/tests/deleted.rs"
		{
			continue;
		}
		let Ok(text) = std::fs::read_to_string(root.join(file)) else {
			continue;
		};
		for (path, pattern) in &patterns {
			if let Some(m) = pattern.find(&text) {
				found.push(format!("{file}: {path}: {}", m.as_str().trim()));
			}
		}
	}
	found
}

#[test]
fn no_file_retired_in_0286_is_tracked_or_named() {
	let root = root();
	let files = tracked(&root);
	let retired = retired_0286(&root);
	assert_eq!(retired.len(), 9);
	let still: Vec<&String> = retired.iter().filter(|p| files.contains(p)).collect();
	assert!(still.is_empty(), "still tracked: {still:?}");
	let found = references_0286(&root, &files, &retired);
	assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_reference_to_a_0286_retired_file_is_found() {
	let root = std::env::temp_dir().join(format!(
		"rnx-final-retired-0286-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir_all(root.join("rune/plans")).unwrap();
	let retired = vec![
		"rune/tools/xtask/measure/n3_qualification.py".to_string(),
		"rune/tools/xtask/measure/qualifications/e1a.json".to_string(),
	];
	let cases = [
		("a.py", "import n3_qualification\n", true),
		("b.py", "from n3_qualification import main\n", true),
		(
			"c.rs",
			"\"tools/xtask/measure/qualifications/e1a.json\"\n",
			true,
		),
		(
			"d.md",
			"rune/tools/xtask/measure/n3_qualification.py\n",
			true,
		),
		(
			"rune/plans/e.txt",
			"tools/xtask/measure/qualifications/e1a.json\n",
			false,
		),
		("f.py", "import n3_qualification_v2\n", false),
		(
			"g.rs",
			"\"mytools/xtask/measure/qualifications/e1a.json\"\n",
			false,
		),
	];
	let mut files = Vec::new();
	for (name, text, _) in cases {
		std::fs::write(root.join(name), text).unwrap();
		files.push(name.to_string());
	}
	let found = references_0286(&root, &files, &retired);
	for (name, _, flagged) in cases {
		assert_eq!(
			found.iter().any(|f| f.starts_with(&format!("{name}:"))),
			flagged,
			"{name}: {found:?}"
		);
	}
	let _ = std::fs::remove_dir_all(&root);
}
