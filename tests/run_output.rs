//! What `rnx run` prints for the value a script returns.
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cases run in parallel, so each gets a directory of its own. Naming one
/// after the source's length collided whenever two sources were the same
/// length, and one case then removed another's script.
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn run(source: &str) -> String {
	let dir = std::env::temp_dir().join(format!(
		"rnx-run-output-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	let mut file = std::fs::File::create(&path).unwrap();
	file.write_all(source.as_bytes()).unwrap();
	drop(file);
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.output()
		.expect("run the script");
	let _ = std::fs::remove_dir_all(&dir);
	// Without this, a script that failed to run at all would satisfy an
	// assertion that it prints nothing.
	assert!(
		output.status.success(),
		"the script did not run: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_script_returning_unit_prints_nothing() {
	assert_eq!(run("pub fn main(args) { }"), "");
	assert_eq!(run("pub fn main(args) { () }"), "");
	assert_eq!(run("pub fn main(args) { Ok(()) }"), "");
}

#[test]
fn every_other_value_still_prints() {
	// `None` serialises to null like unit does, and is not unit: it prints.
	assert_eq!(run("pub fn main(args) { None }"), "null\n");
	assert_eq!(run("pub fn main(args) { 42 }"), "42\n");
	assert_eq!(run("pub fn main(args) { \"text\" }"), "\"text\"\n");
	assert_eq!(run("pub fn main(args) { [1, 2] }"), "[1,2]\n");
	assert_eq!(run("pub fn main(args) { Ok(7) }"), "7\n");
	assert_eq!(run("pub fn main(args) { false }"), "false\n");
}

#[test]
fn a_script_output_goes_to_stdout_and_is_not_disturbed() {
	assert_eq!(
		run("pub fn main(args) { println!(\"line\"); Ok(()) }"),
		"line\n"
	);
}
