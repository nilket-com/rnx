//! Record 0042 pipe controls and global flag placement.
use std::io::Write;
use std::process::{Command, Stdio};
mod harness;
fn run(args: &[&str], input: &str, term: &str) -> std::process::Output {
	let dir = harness::scratch("prompt");
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(args)
		.env("TERM", term)
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
	std::fs::remove_dir_all(dir).unwrap();
	out
}
#[test]
fn clear_never_controls_a_pipe_and_quit_alias_preserves_transcripts() {
	for term in ["xterm", "dumb", "EMACS"] {
		let q = run(&["--color=always"], ":clear\n:q\n", term);
		let quit = run(&["--color=always"], ":clear\n:quit\n", term);
		assert!(q.status.success());
		assert_eq!(q.stdout, quit.stdout);
		assert!(q.stderr.is_empty());
		assert!(!q.stdout.contains(&27), "{:?}", q.stdout);
	}
	let help = run(&[], ":help\n:help :q\n:q\n", "xterm");
	let text = String::from_utf8(help.stdout).unwrap();
	assert!(text.contains(":quit  (:q)"));
	assert!(text.contains(":clear"));
	assert!(text.contains(":quit: session command"));
}
#[test]
fn splash_flags_compose_before_the_command_and_stay_arguments_after_the_path() {
	for args in [
		["--no-splash", "--color=never"],
		["--color=never", "--no-splash"],
	] {
		let out = run(&args, "42\n:q\n", "xterm");
		assert!(out.status.success());
		assert_eq!(out.stdout, b"42\n");
	}
	let dir = harness::scratch("splash_args");
	let file = dir.join("args.rn");
	std::fs::write(&file, "pub fn main(_) { env::args() }").unwrap();
	let out = run(&["run", file.to_str().unwrap(), "--no-splash"], "", "xterm");
	assert!(out.status.success());
	assert_eq!(out.stdout, b"[\"--no-splash\"]\n");
	let out = run(&["run", "--no-splash", file.to_str().unwrap()], "", "xterm");
	assert!(!out.status.success());
	assert!(
		String::from_utf8(out.stderr)
			.unwrap()
			.contains("--no-splash")
	);
	std::fs::remove_dir_all(dir).unwrap();
}
