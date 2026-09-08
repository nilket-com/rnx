//! What `rnx run` says when something goes wrong, and where it says it.
use std::io::Write;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
	path: String,
}

fn run_with(source: &str, before: &[&str], after: &[&str]) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-run-diagnostics-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	let mut file = std::fs::File::create(&path).unwrap();
	file.write_all(source.as_bytes()).unwrap();
	drop(file);
	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command.arg("run");
	for flag in before {
		command.arg(flag);
	}
	command.arg(&path);
	for argument in after {
		command.arg(argument);
	}
	let Output {
		status,
		stdout,
		stderr,
	} = command.output().expect("run the script");
	let ran = Ran {
		stdout: String::from_utf8_lossy(&stdout).into_owned(),
		stderr: String::from_utf8_lossy(&stderr).into_owned(),
		code: status.code().unwrap_or(-1),
		path: path.to_string_lossy().into_owned(),
	};
	let _ = std::fs::remove_dir_all(&dir);
	ran
}
fn run(source: &str) -> Ran {
	run_with(source, &[], &[])
}

#[test]
fn a_compile_error_names_the_file_line_column_and_marks_the_column() {
	let ran = run("pub fn main(args) {\n\tlet a = 1;\n\tlet x = ;\n}\n");
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stdout, "");
	let expected = format!("error at {}, line 3, column 10:", ran.path);
	assert!(ran.stderr.contains(&expected), "{}", ran.stderr);
	assert!(ran.stderr.contains("let x = ;"), "{}", ran.stderr);
	assert!(ran.stderr.contains("^"), "{}", ran.stderr);
	// The wall of text the first port complained about is gone.
	assert!(!ran.stderr.contains("Diagnostics {"), "{}", ran.stderr);
	assert!(!ran.stderr.contains("pub fn main"), "{}", ran.stderr);
}

#[test]
fn a_runtime_error_names_the_place_it_happened() {
	let ran = run("pub fn main(args) {\n\tlet v = [];\n\tv[7]\n}\n");
	assert_eq!(ran.code, 1);
	let expected = format!("runtime error at {}, line 3, column 2:", ran.path);
	assert!(ran.stderr.contains(&expected), "{}", ran.stderr);
	assert!(ran.stderr.contains("v[7]"), "{}", ran.stderr);
}

#[test]
fn an_error_inside_a_called_function_reports_that_function_not_the_call() {
	let ran = run(
		"fn inner() {\n\tlet v = [];\n\tv[7]\n}\nfn outer() { inner() }\npub fn main(args) {\n\touter()\n}\n",
	);
	assert_eq!(ran.code, 1);
	// Line 3 is inside `inner`; lines 5 and 7 are the two calls.
	assert!(ran.stderr.contains(", line 3, "), "{}", ran.stderr);
	assert!(!ran.stderr.contains(", line 5, "), "{}", ran.stderr);
	assert!(!ran.stderr.contains(", line 7, "), "{}", ran.stderr);
}

#[test]
fn the_streams_stay_separated() {
	let ran = run("pub fn main(args) {\n\tprintln!(\"mine\");\n\tlet v = [];\n\tv[7]\n}\n");
	assert_eq!(ran.stdout, "mine\n");
	assert!(ran.stderr.contains("runtime error at"), "{}", ran.stderr);
	assert!(!ran.stdout.contains("runtime error"), "{}", ran.stdout);
}

#[test]
fn a_returned_error_prints_plainly_and_claims_no_position() {
	let ran = run("pub fn main(args) { Err(\"plain words\") }");
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stderr, "error: plain words\n");
	// No wrapper, no escaping, and nothing that looks like a place.
	assert!(!ran.stderr.contains("script returned"), "{}", ran.stderr);
	assert!(!ran.stderr.contains('\\'), "{}", ran.stderr);
	assert!(!ran.stderr.contains(", line "), "{}", ran.stderr);
	assert!(!ran.stderr.contains('^'), "{}", ran.stderr);
}

#[test]
fn a_returned_error_that_is_not_a_string_prints_through_the_bounded_renderer() {
	let ran = run("pub fn main(args) { Err([1, 2, 3]) }");
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stderr, "error: [1, 2, 3]\n");
	let ran = run("pub fn main(args) { Err(#{code: 7}) }");
	assert_eq!(ran.stderr, "error: {\"code\": 7}\n");
	let ran = run("struct Fault { code }\npub fn main(args) { Err(Fault { code: 2 }) }");
	assert!(ran.stderr.starts_with("error: Fault {"), "{}", ran.stderr);
	// A long value is elided by the renderer's limits rather than printed whole.
	let ran =
		run("pub fn main(args) {\n\tlet v = [];\n\tfor i in 0..5000 { v.push(i) }\n\tErr(v)\n}\n");
	assert!(ran.stderr.contains("more)"), "{}", ran.stderr);
	assert!(ran.stderr.len() < 4096, "{}", ran.stderr.len());
}

#[test]
fn the_source_dump_is_asked_for_and_cannot_be_triggered_by_an_argument() {
	let source = "pub fn main(args) {\n\tlet x = ;\n}\n";
	// Without the flag, no output carries the source.
	let ran = run(source);
	assert!(!ran.stderr.contains("pub fn main"), "{}", ran.stderr);
	// Before the path, it prints the compiled source, to standard error.
	let ran = run_with(source, &["--debug-source"], &[]);
	assert!(ran.stderr.contains("compiled source of"), "{}", ran.stderr);
	assert!(ran.stderr.contains("pub fn main"), "{}", ran.stderr);
	assert_eq!(ran.stdout, "");
	// After the path it is the script's argument and changes nothing.
	let ran = run_with(
		"pub fn main(args) { println!(\"{}\", args[0]); Ok(()) }",
		&[],
		&["--debug-source"],
	);
	assert_eq!(ran.stdout, "--debug-source\n");
	assert!(!ran.stderr.contains("compiled source of"), "{}", ran.stderr);
}

#[test]
fn a_file_that_cannot_be_read_names_it_without_inventing_a_place() {
	let missing = std::env::temp_dir().join("rnx-no-such-script-12345.rn");
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&missing)
		.output()
		.unwrap();
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert_eq!(output.status.code(), Some(1));
	assert!(stderr.contains("rnx-no-such-script-12345.rn"), "{stderr}");
	assert!(!stderr.contains(", line "), "{stderr}");
	assert!(!stderr.contains('^'), "{stderr}");
}

#[test]
fn a_column_counts_characters_through_tabs_and_beyond_ascii() {
	// A tab is one character, as the session counts it.
	let ran = run("pub fn main(args) {\n\t\tlet x = ;\n}\n");
	assert!(
		ran.stderr.contains(", line 2, column 11:"),
		"{}",
		ran.stderr
	);
	// Characters outside ASCII count as one each, not as their bytes.
	let ran = run("pub fn main(args) {\n\tlet café = ;\n}\n");
	assert!(
		ran.stderr.contains(", line 2, column 13:"),
		"{}",
		ran.stderr
	);
}

#[test]
fn the_error_that_took_five_rounds_of_printing_to_find_now_names_its_line() {
	// The first port's mixed arithmetic: a counter defaulting to an integer
	// zero, then added to a float. Nothing said where this happened.
	let ran = run(
		"fn get(counter, name) {\n\tif counter.contains_key(name) { counter[name] } else { 0 }\n}\npub fn main(args) {\n\tlet counter = #{};\n\tcounter[\"a\"] = get(counter, \"a\") + 1.5;\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("runtime error at"), "{}", ran.stderr);
	assert!(ran.stderr.contains(", line 6, "), "{}", ran.stderr);
	assert!(ran.stderr.contains("counter[\"a\"] ="), "{}", ran.stderr);
}

#[test]
fn a_script_that_loops_for_ever_is_halted_by_the_budget() {
	// The runner has always had an instruction budget, and nothing can
	// interrupt a file the way Ctrl-C interrupts a session input.
	let start = std::time::Instant::now();
	let ran = run("pub fn main(args) {\n\twhile true {}\n}\n");
	let elapsed = start.elapsed();
	assert_eq!(ran.code, 1);
	assert!(
		ran.stderr.contains("halted") && ran.stderr.contains("instructions exceeded"),
		"{}",
		ran.stderr
	);
	assert!(elapsed < std::time::Duration::from_secs(20), "{elapsed:?}");
	// A loop that ends well inside the budget is untouched.
	let ran = run("pub fn main(args) {\n\tlet n = 0;\n\twhile n < 1000 { n += 1 }\n\tn\n}\n");
	assert_eq!(ran.code, 0);
	assert_eq!(ran.stdout, "1000\n");
}

#[test]
fn a_fault_with_no_resolvable_place_says_so() {
	// A file with no `main` fails before any instruction runs, so there is no
	// instruction to point at.
	let ran = run("fn helper() { 1 }\n");
	assert_eq!(ran.code, 1);
	assert!(
		ran.stderr.contains("no source position is available"),
		"{}",
		ran.stderr
	);
	assert!(ran.stderr.contains("runtime error"), "{}", ran.stderr);

	// The two things that never had a place do not claim one was sought.
	let ran = run("pub fn main(args) { Err(\"plain words\") }");
	assert_eq!(ran.stderr, "error: plain words\n");
	let missing = std::env::temp_dir().join("rnx-absent-script-98765.rn");
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&missing)
		.output()
		.unwrap();
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		!stderr.contains("no source position is available"),
		"{stderr}"
	);
}
