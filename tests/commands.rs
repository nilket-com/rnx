//! Record 0024: what each word after `rnx` does, and what a word that is not
//! a command does.
use std::io::Write;
use std::process::{Command, Stdio};

fn rnx(args: &[&str], input: Option<&str>) -> (String, String, Option<i32>) {
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(args)
		.stdin(if input.is_some() {
			Stdio::piped()
		} else {
			Stdio::null()
		})
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	if let Some(text) = input {
		child
			.stdin
			.take()
			.unwrap()
			.write_all(text.as_bytes())
			.unwrap();
	}
	let done = child.wait_with_output().unwrap();
	(
		String::from_utf8_lossy(&done.stdout).into_owned(),
		String::from_utf8_lossy(&done.stderr).into_owned(),
		done.status.code(),
	)
}

/// The first line the self-check prints. Its absence is how a test says the
/// self-check did not run.
const SELFCHECK_FIRST: &str = "retained closure/function";

#[test]
fn the_bare_command_is_a_session() {
	// What someone typing `rnx` almost always wants. It works on a pipe as
	// well as a terminal, which is what lets this be tested at all.
	let (out, err, code) = rnx(&[], Some("9*9\n"));
	assert_eq!(code, Some(0), "{err}");
	assert!(out.contains("a Rune session"), "{out}");
	assert!(out.contains("81"), "{out}");
	assert!(
		!out.contains(SELFCHECK_FIRST),
		"the bare command still runs the self-check: {out}"
	);
}

#[test]
fn the_self_check_is_asked_for_by_name() {
	let (out, err, code) = rnx(&["selfcheck"], None);
	assert_eq!(code, Some(0), "{err}");
	assert!(out.contains(SELFCHECK_FIRST), "{out}");
	// It asserts what it prints, so reaching the end is the point.
	assert!(out.contains("incremental inputs"), "{out}");
}

#[test]
fn a_word_that_is_not_a_command_is_refused() {
	let (out, err, code) = rnx(&["frobnicate"], None);
	assert_eq!(code, Some(2), "an unknown command was not refused: {out}");
	assert!(err.contains("`frobnicate` is not a command"), "{err}");
	// The commands are named, so the refusal is useful.
	for command in ["repl", "run", "eval", "selfcheck", "help"] {
		assert!(
			err.contains(command),
			"{command} is not in the usage: {err}"
		);
	}
	assert!(
		!out.contains(SELFCHECK_FIRST) && !err.contains(SELFCHECK_FIRST),
		"a typo still ran the self-check"
	);
}

#[test]
fn asking_for_help_is_not_an_error() {
	for spelling in [vec!["help"], vec!["--help"], vec!["-h"]] {
		let (out, err, code) = rnx(&spelling, None);
		assert_eq!(code, Some(0), "{spelling:?}: {err}");
		assert!(out.contains("a Rune scripting environment"), "{out}");
		assert!(
			err.is_empty(),
			"{spelling:?} wrote to standard error: {err}"
		);
	}
}

#[test]
fn the_named_commands_still_do_what_they_did() {
	let (out, err, code) = rnx(&["eval", "7.3*8.75"], None);
	assert_eq!(code, Some(0), "{err}");
	assert_eq!(out, "63.875\n");

	let (out, _, code) = rnx(&["repl"], Some("2+2\n"));
	assert_eq!(code, Some(0));
	assert!(out.contains("4"), "{out}");
}

#[test]
fn a_refused_command_cannot_repaint_the_terminal() {
	// Record 0019's rule: everything rnx prints on its own behalf is escaped,
	// and a mistyped command is text from outside like any other. Before this
	// was fixed, `rnx $'bad\033[2J'` cleared the terminal on its way to
	// saying it did not recognise the word.
	let (out, err, code) = rnx(&["bad\u{1b}[2J"], None);
	assert_eq!(code, Some(2), "{err}");
	assert_eq!(out, "", "a refusal wrote to standard output");
	// Visible, not dropped: the escape is shown rather than acted on.
	assert!(err.contains("bad\\u{1b}[2J"), "{err:?}");
	assert!(
		!err.contains('\u{1b}'),
		"an escape reached the terminal: {err:?}"
	);
	// And a carriage return, which could otherwise overwrite the line.
	let (out, err, code) = rnx(&["bad\rrewritten"], None);
	assert_eq!(code, Some(2));
	assert_eq!(out, "");
	assert!(
		!err.contains('\r'),
		"a carriage return reached the terminal"
	);
	assert!(err.contains("bad\\u{d}rewritten"), "{err:?}");
}
