//! Every control of the Python final controller is listed once in
//! CONTROLS.txt, with the Rust test that ports it or a stated reason it
//! stays with the Python stage programs. A row whose Python control was
//! deleted with that controller (ADR 0284) has a third column, `removed: `
//! and why; its Rust pair must still exist and its Python control must be
//! gone. Every Python control still present is listed and not removed.
use std::collections::BTreeSet;
use std::path::Path;

const MODULES: [&str; 11] = [
	"test_final_evidence",
	"test_final_successor",
	"test_stage_scheduler",
	"test_gate_policy",
	"test_child_result",
	"test_owned_build",
	"test_failure_detail",
	"test_stage_outputs",
	"test_host_budget",
	"test_required_stages",
	"test_final_output",
];

fn root() -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

/// `module.Class.test_name` for every test method, read from the source.
fn python_controls() -> BTreeSet<String> {
	let mut controls = BTreeSet::new();
	for module in MODULES {
		let Ok(text) = std::fs::read_to_string(root().join(format!("tools/column/{module}.py")))
		else {
			continue;
		};
		let mut class = None;
		for line in text.lines() {
			if let Some(rest) = line.strip_prefix("class ") {
				class = Some(rest.split(['(', ':']).next().unwrap().trim().to_owned());
			} else if let (Some(class), Some(rest)) = (&class, line.strip_prefix("\tdef test_")) {
				controls.insert(format!(
					"{module}.{class}.test_{}",
					rest.split('(').next().unwrap()
				));
			}
		}
	}
	controls
}

#[test]
fn every_python_control_has_a_rust_pair_or_a_reason() {
	let table = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("CONTROLS.txt"))
		.unwrap();
	let controls = python_controls();
	let mut listed = BTreeSet::new();
	let mut removed = 0;
	for line in table
		.lines()
		.filter(|l| !l.starts_with('#') && !l.trim().is_empty())
	{
		let mut fields = line.split('\t');
		let (control, pair) = (
			fields.next().unwrap(),
			fields.next().expect("control<TAB>pair"),
		);
		let gone = fields.next();
		assert!(fields.next().is_none(), "{control}: too many columns");
		assert!(
			listed.insert(control.to_owned()),
			"{control} is listed twice"
		);
		match gone {
			Some(reason) => {
				assert!(
					reason.starts_with("removed: ") && reason.len() > 20,
					"{control}: a removal states why"
				);
				assert!(
					!controls.contains(control),
					"{control} is marked removed but still exists"
				);
				removed += 1;
			}
			None => assert!(
				controls.contains(control),
				"{control} no longer exists; mark it removed"
			),
		}
		if pair.starts_with("stays: ") || pair.starts_with("not portable: ") {
			assert!(pair.len() > 20, "{control}: a reason, not a label");
			continue;
		}
		let (file, test) = pair
			.split_once("::")
			.unwrap_or_else(|| panic!("{control}: {pair}"));
		let source = std::fs::read_to_string(root().join(file))
			.unwrap_or_else(|_| panic!("{control}: no file {file}"));
		assert!(
			source.contains(&format!("fn {test}()")),
			"{control}: no test {test} in {file}"
		);
	}
	let unpaired: Vec<&String> = controls.difference(&listed).collect();
	assert!(
		unpaired.is_empty(),
		"Python controls without a pair: {unpaired:?}"
	);
	assert!(listed.len() >= 130, "the table lost rows: {}", listed.len());
	assert!(removed > 0 || listed.len() == controls.len());
}
