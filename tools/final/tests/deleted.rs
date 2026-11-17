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
