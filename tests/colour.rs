//! Presentation equality removes ONLY rnx's closed set of SGR sequences.
use std::io::Write;
use std::process::{Command, Output, Stdio};
mod harness;
const SGR: [&str; 8] = [
	"\x1b[0m",
	"\x1b[1m",
	"\x1b[35m",
	"\x1b[32m",
	"\x1b[36m",
	"\x1b[2m",
	"\x1b[1;31m",
	"\x1b[31m",
];
fn strip(bytes: &[u8]) -> String {
	let mut text = String::from_utf8(bytes.to_vec()).unwrap();
	for code in SGR {
		text = text.replace(code, "");
	}
	assert!(!text.contains('\x1b'), "unexpected escape: {text:?}");
	text
}
fn run(mode: &str, args: &[&str], input: &str) -> Output {
	let dir = harness::scratch("colour");
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg(format!("--color={mode}"))
		.args(args)
		.env("TERM", "xterm-256color")
		.env_remove("NO_COLOR")
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
	let result = child.wait_with_output().unwrap();
	std::fs::remove_dir_all(dir).unwrap();
	result
}
fn equivalent(args: &[&str], input: &str) {
	let plain = run("never", args, input);
	let colour = run("always", args, input);
	assert_eq!(colour.status.code(), plain.status.code());
	assert_eq!(strip(&colour.stdout).as_bytes(), plain.stdout);
	assert_eq!(strip(&colour.stderr).as_bytes(), plain.stderr);
	assert!(colour.stdout.contains(&27) || colour.stderr.contains(&27));
	let auto = run("auto", args, input);
	assert_eq!(auto.stdout, plain.stdout);
	assert_eq!(auto.stderr, plain.stderr);
}
#[test]
fn presentation_styles_cannot_change_output_or_bounds() {
	for source in [
		r##"#{"\u{1b}[2J": ["e\u{301}界\t", true, Some(1), ()], plain: -2.5}"##,
		"let v = [1]; v.push(v); [v, || 1]",
		"let 界 = 2; let x = ;",
		"panic!(\"é界\")",
		"Err(#{message: \"\\u{1b}[2J\"})",
	] {
		equivalent(&["eval", source], "");
	}
	equivalent(&["help"], "");
	equivalent(
		&["repl"],
		"let v = []; for n in 0..100 { v.push(n) } v\nlet s = \"x\"; for n in 0..13 { s = s + s; } s\n:vars\n:help s\n:help\n:quit\n",
	);
	let dir = harness::scratch("colour-file");
	let path = dir.join("file.rn");
	for source in [
		"pub fn main(_) { [true, 42, \"界\"] }",
		"pub fn main(_) { let 界 = ; }",
		"pub fn main(_) { panic!(\"bad\") }",
	] {
		std::fs::write(&path, source).unwrap();
		equivalent(&["run", path.to_str().unwrap()], "");
	}
	std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn raw_script_output_is_not_presentation() {
	for mode in ["auto", "always", "never"] {
		let out = run(
			mode,
			&[
				"eval",
				r#"println!("\u{1b}[2J"); host::eprint("\u{1b}[31m")"#,
			],
			"",
		);
		assert_eq!(out.stdout, b"\x1b[2J\n");
		assert_eq!(out.stderr, b"\x1b[31m");
	}
}
#[test]
fn colour_flag_belongs_only_before_the_command() {
	let bad = run("purple", &["eval", "println!(\"must not run\")"], "");
	assert_eq!(bad.status.code(), Some(2));
	assert!(bad.stdout.is_empty());
	assert!(String::from_utf8_lossy(&bad.stderr).contains("auto, always, or never"));
	let dir = harness::scratch("colour-args");
	let file = dir.join("args.rn");
	std::fs::write(&file, "pub fn main(_) { env::args() }").unwrap();
	let out = run(
		"never",
		&["run", file.to_str().unwrap(), "--color=never"],
		"",
	);
	assert_eq!(out.stdout, b"[\"--color=never\"]\n");
	let out = run(
		"never",
		&["run", "--color=never", file.to_str().unwrap()],
		"",
	);
	assert_eq!(out.status.code(), Some(1));
	assert!(String::from_utf8_lossy(&out.stderr).contains("--color=never"));
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn diagnostic_caret_styling_starts_after_the_source_newline() {
	let out = run("always", &["eval", "let x = ;"], "");
	let error = String::from_utf8(out.stderr).unwrap();
	assert!(error.contains("\n\x1b[31m  "), "{error:?}");
	assert!(!error.contains("\x1b[31m\n"), "{error:?}");
}
