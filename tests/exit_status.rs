//! The status a script chooses, and what it can say on the way out.
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
}

fn run_with(source: &str, arguments: &[&str]) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-exit-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.args(arguments)
		.output()
		.unwrap();
	let _ = std::fs::remove_dir_all(&dir);
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}
fn run(source: &str) -> Ran {
	run_with(source, &[])
}

#[test]
fn a_chosen_status_reaches_the_shell() {
	for status in [0, 1, 2, 7, 255] {
		let ran = run(&format!(
			"pub fn main(args) {{\n\thost::exit({status})?;\n\tOk(())\n}}\n"
		));
		assert_eq!(ran.code, status, "{}", ran.stderr);
		assert_eq!(ran.stderr, "", "exit({status}) said something");
	}
}

#[test]
fn a_script_can_fail_quietly() {
	// The case that motivated the cut: a complete report on standard output
	// and a failing status, with nothing on standard error.
	let ran = run(
		"pub fn main(args) {\n\tprintln!(\"the whole report\");\n\thost::exit(1)?;\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stdout, "the whole report\n");
	assert_eq!(ran.stderr, "");
}

#[test]
fn output_with_no_trailing_newline_is_not_lost() {
	// A completed line is flushed on its own; an unterminated one is not, and
	// the process is about to end without unwinding.
	let ran =
		run("pub fn main(args) {\n\tprint!(\"unterminated\");\n\thost::exit(3)?;\n\tOk(())\n}\n");
	assert_eq!(ran.code, 3);
	assert_eq!(ran.stdout, "unterminated");
}

#[test]
fn a_status_outside_the_range_is_refused_rather_than_truncated() {
	// Each of these is what the shell would have seen if the number were
	// passed straight through. The first is the one that matters: 256 would
	// arrive as 0 and turn a failure into a success.
	for (asked, would_become) in [(256, 0), (300, 44), (-1, 255)] {
		let ran = run(&format!(
			"pub fn main(args) {{\n\tprintln!(\"printed\");\n\thost::exit({asked})?;\n\tOk(())\n}}\n"
		));
		assert_ne!(ran.code, would_become, "exit({asked}) was truncated");
		assert_ne!(ran.code, 0, "exit({asked}) reported success");
		assert!(
			ran.stderr.contains(&format!("cannot exit with {asked}")),
			"{}",
			ran.stderr
		);
		// The message says what it would have become, because that is the
		// part a reader cannot work out from the number alone.
		assert!(
			ran.stderr
				.contains(&format!("would reach the shell as {would_become}")),
			"{}",
			ran.stderr
		);
		// What the script printed before asking is still its own.
		assert_eq!(ran.stdout, "printed\n");
	}
}

#[test]
fn nothing_after_the_exit_runs() {
	let ran = run(
		"pub fn main(args) {\n\tprintln!(\"before\");\n\thost::exit(4)?;\n\tprintln!(\"after\");\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 4);
	assert_eq!(ran.stdout, "before\n");
	assert!(!ran.stdout.contains("after"), "{}", ran.stdout);
}

#[test]
fn eprint_writes_exactly_what_it_is_given() {
	let ran = run(
		"pub fn main(args) {\n\thost::eprint(\"one\\n\")?;\n\thost::eprint(\"two, {not a format} \\\\ \\\"quoted\\\"\")?;\n\tprintln!(\"out\");\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	// Verbatim: nothing added, nothing escaped, no newline of its own.
	assert_eq!(ran.stderr, "one\ntwo, {not a format} \\ \"quoted\"");
	// And it does not touch standard output.
	assert_eq!(ran.stdout, "out\n");
}

#[test]
fn eval_may_choose_a_status() {
	// `eval` runs one thing and exits, which is what a status is for.
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("eval")
		.arg("host::exit(5)")
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(5));
	assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn a_script_that_never_asks_keeps_the_status_it_always_had() {
	// Record 0009 decided these, and this cut changes none of them.
	let ran = run("pub fn main(args) { Ok(()) }");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	let ran = run("pub fn main(args) { Err(\"plain words\") }");
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stderr, "error: plain words\n");
	let ran = run("pub fn main(args) {\n\tlet v = [];\n\tv[7]\n}\n");
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("runtime error at"), "{}", ran.stderr);
}

/// Run a script with standard output pointing at a stream that refuses writes.
fn run_writing_to(source: &str) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-exit-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	#[cfg(unix)]
	let out = std::fs::OpenOptions::new()
		.write(true)
		.open("/dev/full")
		.expect("this control needs /dev/full to refuse writes");
	#[cfg(windows)]
	let out = {
		// An open, read-only handle is valid standard output but cannot accept
		// a write. Establish the refusal before handing the handle to rnx.
		use std::io::Write;
		let sink = dir.join("read-only-sink");
		std::fs::write(&sink, b"").unwrap();
		let mut file = std::fs::File::open(&sink).unwrap();
		assert!(
			file.write_all(b"control").is_err(),
			"the sink accepted a write"
		);
		file
	};
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.stdout(std::process::Stdio::from(out))
		.output()
		.unwrap();
	let _ = std::fs::remove_dir_all(&dir);
	Ran {
		stdout: String::new(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}

#[test]
fn an_invalid_status_ends_the_script_even_when_the_result_is_discarded() {
	// The refusal cannot be an ordinary error, because an ordinary error is a
	// value a script can drop, and dropping it would let a script that asked
	// for an impossible status carry on and exit 0.
	let ran =
		run("pub fn main(args) {\n\thost::exit(256);\n\tprintln!(\"continued\");\n\tOk(())\n}\n");
	assert_ne!(ran.code, 0, "an impossible status exited successfully");
	assert_eq!(ran.stdout, "", "the script carried on: {}", ran.stdout);
	assert!(
		ran.stderr.contains("cannot exit with 256"),
		"{}",
		ran.stderr
	);
	// The same with the result bound and ignored rather than dropped.
	let ran = run(
		"pub fn main(args) {\n\tlet ignored = host::exit(300);\n\tprintln!(\"continued\");\n\tOk(())\n}\n",
	);
	assert_ne!(ran.code, 0);
	assert_eq!(ran.stdout, "");
}

#[test]
fn an_invalid_status_from_eval_also_fails() {
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("eval")
		.arg("host::exit(256)")
		.output()
		.unwrap();
	assert_ne!(output.status.code(), Some(0));
	assert!(
		String::from_utf8_lossy(&output.stderr).contains("cannot exit with 256"),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	// It is a failure, not a value printed and shrugged off.
	assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn output_that_could_not_be_written_never_reports_success() {
	// `/dev/full` on Unix, or a read-only handle on Windows, refuses every
	// write, so the report is lost at the flush. A status of 0 there would
	// say the script did what it said, and it did not.
	let ran = run_writing_to(
		"pub fn main(args) {\n\tprint!(\"report\");\n\thost::exit(0)?;\n\tOk(())\n}\n",
	);
	assert_ne!(ran.code, 0, "output was lost and the status said success");
	assert!(
		ran.stderr.contains("cannot write standard output"),
		"{}",
		ran.stderr
	);
}

#[test]
fn a_status_that_was_already_failing_survives_a_failing_stream() {
	// The script had decided it was failing, and that is still true, so the
	// status it chose is kept and the lost output is named beside it.
	let ran = run_writing_to(
		"pub fn main(args) {\n\tprint!(\"report\");\n\thost::exit(2)?;\n\tOk(())\n}\n",
	);
	assert_eq!(ran.code, 2, "{}", ran.stderr);
	assert!(
		ran.stderr.contains("cannot write standard output"),
		"{}",
		ran.stderr
	);
}
