//! Record 0049: domain registration reaches every ordinary entry point.
use std::io::Write;
use std::process::{Command, Stdio};
mod harness;

#[test]
fn domains_work_in_run_eval_and_a_session_after_reset() {
	let dir = harness::scratch("namespaces");
	let source = r#"io::eprint("")?; json::parse(json::stringify(18446744073709551615u64)?)? == 18446744073709551615u64"#;
	let file = dir.join("domains.rn");
	std::fs::write(&file, format!("pub fn main(_) {{ {source} }}")).unwrap();
	for args in [vec!["run", file.to_str().unwrap()], vec!["eval", source]] {
		let out = Command::new(env!("CARGO_BIN_EXE_rnx"))
			.args(args)
			.output()
			.unwrap();
		assert!(out.status.success(), "{out:?}");
		assert_eq!(out.stdout, b"true\n");
		assert!(out.stderr.is_empty());
	}
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["--color=never", "--no-splash"])
		.env("RNX_CONFIG", dir.join("absent"))
		.env("RNX_HISTORY", dir.join("history"))
		.env("TERM", "xterm")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let input = format!(
		"{source}\n:reset\n{source}\nprocess::exit(0).is_err()\n:help json::parse\n:help json::stringify\n:help io::stdin\n:help io::eprint\n:help process::exit\n:help host::json_parse\n:q\n"
	);
	child
		.stdin
		.take()
		.unwrap()
		.write_all(input.as_bytes())
		.unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(out.status.success(), "{out:?}");
	let text = String::from_utf8(out.stdout).unwrap();
	assert_eq!(text.lines().filter(|s| *s == "true").count(), 3, "{text}");
	for name in [
		"json::parse",
		"json::stringify",
		"io::stdin",
		"io::eprint",
		"process::exit",
	] {
		assert!(text.contains(&format!("{name}: host function")), "{text}");
	}
	assert!(!text.contains("host::json_parse: host function"), "{text}");
	assert!(
		text.contains("no binding, declaration, host function, or command"),
		"{text}"
	);
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rune_printing_coexists_and_an_explicit_io_import_can_be_disambiguated() {
	let dir = harness::scratch("io-shadow");
	let file = dir.join("shadow.rn");
	std::fs::write(&file, r#"use std::io; pub fn main(_) { io::print("a"); print!("b"); println("c"); println!("d"); dbg!(1); ::io::eprint("tail")?; json::parse("7")? }"#).unwrap();
	let out = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&file)
		.output()
		.unwrap();
	std::fs::remove_dir_all(dir).unwrap();

	assert!(out.status.success(), "{out:?}");
	let stdout = String::from_utf8(out.stdout).unwrap();
	assert!(stdout.starts_with("abc\nd\n"), "{stdout}");
	assert!(stdout.ends_with("7\n"), "{stdout}");
	assert!(String::from_utf8(out.stderr).unwrap().ends_with("tail"));
}
