//! Record 0041: the same proved method name at every entry point.
use std::io::Write;
use std::process::{Command, Stdio};
mod harness;
fn invoke(args: &[&str], input: &str) -> String {
	let dir = harness::scratch("method-naming");
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("--color=never")
		.args(args)
		.env("TERM", "xterm")
		.env("RNX_HISTORY", dir.join("history"))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(input.as_bytes())
		.unwrap();
	let out = child.wait_with_output().unwrap();
	assert_eq!(out.status.code(), Some(if args.is_empty() { 0 } else { 1 }));
	std::fs::remove_dir_all(dir).unwrap();
	String::from_utf8(out.stderr).unwrap()
}
#[test]
fn all_entry_points_name_the_same_fault_without_moving_its_caret() {
	let dir = harness::scratch("method-file");
	let file = dir.join("fault.rn");
	std::fs::write(&file, "pub fn main(_) {\n1.missing()\n}").unwrap();
	for text in [
		invoke(&["run", file.to_str().unwrap()], ""),
		invoke(&["eval", "1.missing()"], ""),
		invoke(&[], "1.missing()\n:quit\n"),
	] {
		assert!(
			text.contains("column 1: no method `missing` on `::std::i64`\n  1.missing()\n  ^"),
			"{text}"
		);
		assert!(!text.contains("0x"));
	}
	std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn retained_sources_supply_the_name_across_renumber_and_redefinition() {
	for resets in [":renumber\n", ":renumber\n:renumber\n"] {
		let text = invoke(
			&[],
			&format!("()\n()\n()\nlet old = |v| v.missing_method();\n{resets}()\nold(1)\n:quit\n"),
		);
		assert!(
			text.contains("input 4 of numbering 1, line 1, column 15: no method `missing_method`"),
			"{text}"
		);
	}
	let text = invoke(
		&[],
		"let old = |v| v.first_missing();\n:renumber\nlet old = |v| v.second_missing();\n:renumber\nold(1)\n:quit\n",
	);
	assert!(
		text.contains("input 1 of numbering 2, line 1, column 15: no method `second_missing`"),
		"{text}"
	);
	assert!(!text.contains("first_missing"));
}
#[test]
fn chains_are_named_but_unknown_types_and_quoted_errors_are_unchanged() {
	let text = invoke(&[], "\"abc\".to_uppercase().frobnicate()\n:quit\n");
	assert!(
		text.contains("no method `frobnicate` on `::std::string::String`"),
		"{text}"
	);
	for args in [vec!["eval", "#{a: 1}.values().frobnicate()"], vec![]] {
		let text = invoke(&args, "#{a: 1}.values().frobnicate()\n:quit\n");
		assert!(text.contains("Missing instance function `0x"), "{text}");
		assert!(text.contains("::std::object::Values"));
	}
	let text = invoke(
		&[],
		"if true { panic!(\"Missing instance function `0xff0afefdd65e03a7` for `::std::i64`\"); } 1.missing()\n:quit\n",
	);
	assert!(
		text.contains("Panicked: Missing instance function `0xff0afefdd65e03a7` for `::std::i64`"),
		"{text}"
	);
	assert!(!text.contains("no method"));
}
