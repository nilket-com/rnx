//! What `rnx eval` does with what an expression returns, and whether `run`
//! agrees with it.
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
}

fn finish(output: std::process::Output) -> Ran {
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}

fn evaluated(source: &str) -> Ran {
	finish(
		Command::new(env!("CARGO_BIN_EXE_rnx"))
			.arg("eval")
			.arg(source)
			.output()
			.unwrap(),
	)
}

/// The same expression as a script's `main`, so the two can be compared.
fn ran(source: &str) -> Ran {
	let dir = std::env::temp_dir().join(format!(
		"rnx-eval-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	let path = dir.join("script.rn");
	std::fs::write(&path, format!("pub fn main(_) {{ {source} }}\n")).unwrap();
	let out = finish(
		Command::new(env!("CARGO_BIN_EXE_rnx"))
			.arg("run")
			.arg(&path)
			.output()
			.unwrap(),
	);
	let _ = std::fs::remove_dir_all(&dir);
	out
}

#[test]
fn a_returned_error_fails() {
	// The case that motivated the record: a read that did not happen, whose
	// status said it had.
	let ran = evaluated("host::read(\"no-such-file-24680\")");
	assert_eq!(ran.code, 1, "a failed read reported success");
	assert_eq!(ran.stdout, "");
	assert!(
		ran.stderr
			.starts_with("error: cannot read no-such-file-24680"),
		"{}",
		ran.stderr
	);
}

#[test]
fn a_string_error_prints_bare() {
	let ran = evaluated("Err(\"plain words\")");
	assert_eq!(ran.code, 1);
	assert_eq!(ran.stderr, "error: plain words\n");
	assert_eq!(ran.stdout, "");
}

#[test]
fn a_returned_ok_is_unwrapped() {
	assert_eq!(evaluated("Ok(5)").stdout, "5\n");
	assert_eq!(evaluated("Ok(5)").code, 0);
}

#[test]
fn a_value_that_is_not_a_result_is_unchanged() {
	let ran = evaluated("1 + 1");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "2\n");
	assert_eq!(ran.stderr, "");
}

#[test]
fn a_child_that_failed_is_still_a_success() {
	// The call worked and the child did not. Only the shape of the returned
	// value decides, so nothing inside it turns this into a failure.
	let ran = evaluated("host::process(\"false\", [], 5000).map(|r| r.code)");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "1\n");
	assert_eq!(ran.stderr, "");
}

/// What the two entry points do with one expression's returned value. Where
/// they differ, the difference is written out rather than described, because
/// "they differ" is not a contract and these are.
enum Agreement {
	/// Identical standard output.
	Same,
	/// Different only in the spaces JSON omits.
	Spacing,
	/// Different in how the value is spelled: `eval`'s text, then `run`'s.
	/// JSON has no form for an option, a tuple, or a character.
	Spelled(&'static str, &'static str),
	/// `run` has no rendering for the value at all, so it reports a failure
	/// where `eval` shows the value. Decision 5; the text is `eval`'s.
	RunCannotShow(&'static str),
}

#[test]
fn the_two_entry_points_agree_on_the_status_and_on_what_they_say_went_wrong() {
	use Agreement::{RunCannotShow, Same, Spacing, Spelled};
	// Not every shape a returned value can have: the shapes this cut
	// measured, each with what the two entry points do with it. Decision 4
	// keeps each renderer as it was, so standard output agrees only where the
	// two happen to spell a value the same way, and that is asserted case by
	// case instead of claimed in general.
	let cases = [
		// A failure. Standard output is empty from both.
		("Err(\"plain words\")", Same),
		("host::read(\"no-such-file-13579\")", Same),
		("Err(#{code: 7})", Same),
		("Err([1, 2, 3])", Same),
		// A value both spell alike.
		("Ok(5)", Same),
		("1 + 1", Same),
		("\"hi\"", Same),
		("()", Same),
		("host::process(\"false\", [], 5000).map(|r| r.code)", Same),
		// A value they space differently.
		("[1, 2, 3]", Spacing),
		("#{a: 1}", Spacing),
		// A value they spell differently, which is more than spacing.
		("None", Spelled("None\n", "null\n")),
		("Some(1)", Spelled("Some(1)\n", "1\n")),
		("(1, 2)", Spelled("(1, 2)\n", "[1,2]\n")),
		("'x'", Spelled("'x'\n", "\"x\"\n")),
		// A value `run` cannot render at all.
		(
			"struct P { code } P { code: 7 }",
			RunCannotShow("P {code: 7}\n"),
		),
		("enum E { A } E::A", RunCannotShow("A\n")),
		("|| 1", RunCannotShow("<function>\n")),
		("0..3", RunCannotShow("<::std::ops::Range>\n")),
	];
	for (source, agreement) in cases {
		let e = evaluated(source);
		let r = ran(source);
		if let RunCannotShow(eval_stdout) = agreement {
			// The one shape whose status does not agree, and deliberately:
			// `run` exiting 0 here would report a failure to show a value as
			// a value shown, which is the defect this record removes.
			assert_eq!(e.code, 0, "{source}: {}", e.stderr);
			assert_eq!(e.stdout, eval_stdout, "{source}");
			assert_eq!(e.stderr, "", "{source}");
			assert_eq!(r.code, 1, "{source}: run reported success");
			assert_eq!(r.stdout, "", "{source}: run printed on standard output");
			let said = "error: the value the script returned cannot be shown: ";
			assert!(r.stderr.starts_with(said), "{source}: {}", r.stderr);
			assert!(
				r.stderr.trim_end().len() > said.len(),
				"{source}: no reason given"
			);
			continue;
		}
		// Every other shape agrees on the status and on standard error.
		assert_eq!(e.code, r.code, "{source}: eval {} run {}", e.code, r.code);
		assert_eq!(e.stderr, r.stderr, "{source}");
		match agreement {
			Same => assert_eq!(e.stdout, r.stdout, "{source}"),
			Spacing => {
				// This holds the difference to spacing, so the day it becomes
				// more than that a test says the record is out of date.
				assert_ne!(
					e.stdout, r.stdout,
					"{source} now agrees; update record 0018"
				);
				assert_eq!(
					e.stdout.replace([' ', '\n'], ""),
					r.stdout.replace([' ', '\n'], ""),
					"{source} differs by more than spacing"
				);
			}
			Spelled(eval_stdout, run_stdout) => {
				assert_eq!(e.stdout, eval_stdout, "{source}: eval");
				assert_eq!(r.stdout, run_stdout, "{source}: run");
			}
			RunCannotShow(_) => unreachable!("handled above"),
		}
	}
}

#[test]
fn a_struct_error_names_its_fields_only_where_they_were_declared() {
	// Both report the failure and both exit 1, and the text is not the same.
	// The bounded renderer takes field names from the session's table of
	// declarations, which `run` has no equivalent of, so it falls back to
	// positions. Retained rather than fixed: giving `run` field names means
	// parsing its file's declarations, which belongs to the renderer question
	// decision 4 defers. Asserted so the difference cannot drift unnoticed.
	let source = "struct Problem { code } Err(Problem { code: 7 })";
	let e = evaluated(source);
	let r = ran(source);
	assert_eq!(e.code, 1, "{}", e.stderr);
	assert_eq!(r.code, 1, "{}", r.stderr);
	assert_eq!(e.stderr, "error: Problem {code: 7}\n");
	assert_eq!(r.stderr, "error: Problem {0: 7}\n");
	assert_eq!(e.stdout, "");
	assert_eq!(r.stdout, "");
}

#[test]
fn a_value_run_cannot_show_is_not_a_success() {
	// Every shape found to have no JSON form. Before this cut each of these
	// printed `<serialization error: ...>` on standard output and exited 0,
	// so a shell saw success and a reader saw something that read like a
	// value. The reason is still reported, on standard error, with a status.
	for source in [
		"struct P { code } P { code: 7 }",
		"enum E { A } E::A",
		"|| 1",
		"fn h() {} h",
		"0..3",
	] {
		let r = ran(source);
		assert_eq!(r.code, 1, "{source} reported success");
		assert_eq!(r.stdout, "", "{source} printed on standard output");
		assert!(
			r.stderr
				.starts_with("error: the value the script returned cannot be shown: "),
			"{source}: {}",
			r.stderr
		);
		assert!(
			!r.stderr.contains("serialization error:"),
			"{source}: the old text is still being printed"
		);
	}
	// A value that does have one is unaffected, and nothing is printed for a
	// script that returns nothing at all.
	assert_eq!(ran("#{a: 1}").stdout, "{\"a\":1}\n");
	assert_eq!(ran("#{a: 1}").code, 0);
	assert_eq!(ran("()").stdout, "");
	assert_eq!(ran("()").code, 0);
}

#[test]
fn nothing_else_about_eval_changed() {
	// Its diagnostics are what records 0002 and 0009 gave it.
	let ran = evaluated("let x = ;");
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("line 1, column 9"), "{}", ran.stderr);
	let ran = evaluated("let v = []; v[7]");
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("runtime error at"), "{}", ran.stderr);
	// And `host::exit` still chooses the status, as record 0013 decided.
	let ran = evaluated("host::exit(5)");
	assert_eq!(ran.code, 5);
	assert_eq!(ran.stderr, "");
}
