//! Record 0040's pipe and unsupported-terminal transcript contracts.
use std::io::Write;
use std::process::{Command, Output, Stdio};
mod harness;
fn session(term: &str, colour: &str, input: &str) -> (Output, String) {
	let dir = harness::scratch("numbering");
	let history = dir.join("history");
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg(format!("--color={colour}"))
		.env("TERM", term)
		.env_remove("NO_COLOR")
		.env("RNX_HISTORY", &history)
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
	let history = std::fs::read_to_string(history).unwrap_or_default();
	std::fs::remove_dir_all(dir).unwrap();
	(out, history)
}
#[test]
fn marker_visibility_tracks_the_actual_reader_including_uppercase_unsupported_modes() {
	let input = "let x = 1;\nx + 1\n()\n:quit\n";
	let (plain, _) = session("xterm-256color", "never", input);
	assert_eq!(
		String::from_utf8(plain.stdout).unwrap(),
		"rnx: a Rune session. :help lists the commands, :quit ends it.\n2\n"
	);
	for term in ["dumb", "EMACS", "cons25"] {
		let (out, _) = session(term, "never", input);
		assert!(out.status.success());
		assert!(out.stderr.is_empty());
		assert_eq!(
			String::from_utf8(out.stdout).unwrap(),
			"rnx: a Rune session. :help lists the commands, :quit ends it.\n[1] > [2] > [2] 2\n[3] > [4] > "
		);
	}
}
#[test]
fn renumbering_keeps_history_and_old_origins_while_reset_clears_origins() {
	let input = "let x = 7;\nlet v = [x];\nfn kept(n) { n + 1 }\nlet c = |v| v.missing_method();\n:debug\n:renumber\n:debug\n:vars\nc(1)\nlet broken = ;\n:renumber\n:renumber\nlet broken = ;\n:reset\n:vars\nx\n:quit\n";
	let (out, history) = session("dumb", "never", input);
	let stdout = String::from_utf8(out.stdout).unwrap();
	let stderr = String::from_utf8(out.stderr).unwrap();
	assert!(stderr.contains("input 4 of numbering 1"), "{stderr}");
	assert!(stderr.contains("input 2,"), "{stderr}");
	assert_eq!(stderr.matches("input 1,").count(), 2, "{stderr}");
	assert!(stdout.contains("x: i64 = 7"), "{stdout}");
	assert!(stdout.contains("no bindings;"), "{stdout}");
	assert!(stdout.contains("session reset\n[1] > "), "{stdout}");
	for line in input.lines().take(4).chain([":renumber"]) {
		assert!(history.contains(line), "{history}");
	}
	let prefix = "[5] > ";
	let before = stdout
		.split_once(prefix)
		.unwrap()
		.1
		.split_once("[5] > ")
		.unwrap()
		.0;
	let after = stdout.split_once("[1] > ").unwrap().1;
	// Source preservation is also checked structurally in the session unit
	// gate; the transcript must contain the exact debug block a second time.
	assert!(after.contains(before), "{stdout}");
}
#[test]
fn colours_are_separable_and_commands_are_not_marked_as_results() {
	let input = "1\n:renumber\n2\n:help :renumber\n:quit\n";
	let (plain, _) = session("dumb", "never", input);
	let (styled, _) = session("dumb", "always", input);
	let mut text = String::from_utf8(styled.stdout).unwrap();
	for sgr in ["\x1b[0m", "\x1b[1m", "\x1b[36m"] {
		text = text.replace(sgr, "");
	}
	assert_eq!(text.as_bytes(), plain.stdout);
	assert!(text.contains("[1] 1\n[2] > [1] > [1] 2\n"), "{text}");
	assert!(
		text.contains("[2] > :renumber: session command\n"),
		"{text}"
	);
	assert!(!text.contains("[2] :renumber:"));
}
