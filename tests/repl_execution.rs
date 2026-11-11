//! Opt-in persistent CLI contract; no engine fallback or transcript replay.
#![cfg(unix)]
use std::{
	io::Write,
	process::{Command, Output, Stdio},
};
fn repl(engine: Option<&str>, input: &str) -> Output {
	let mut command = Command::new("timeout");
	command.args([
		"--kill-after=2s",
		"30s",
		env!("CARGO_BIN_EXE_rnx"),
		"--no-splash",
		"repl",
	]);
	if let Some(engine) = engine {
		command.arg(format!("--engine={engine}"));
	}
	let mut child = command
		.env("RNX_CONFIG", "")
		.env("RNX_HISTORY", "/dev/null")
		.env("NO_COLOR", "1")
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
	child.wait_with_output().unwrap()
}
#[test]
fn default_stays_legacy_and_execution_session_is_sticky_across_reset() {
	let input = "let n=7;let c=||n;let shared=[1];0\nshared.push(2);c()\nshared.len()\n:vars\n:help shared\n:renumber\nc()\n:reset\n7\n:quit\n";
	let old = repl(Some("legacy"), input);
	for engine in [None, Some("auto"), Some("execution")] {
		let actual = repl(engine, input);
		assert!(actual.status.success(), "{:?}", actual);
		assert_eq!(actual.stdout, old.stdout, "{engine:?}");
		assert_eq!(actual.stderr, old.stderr, "{engine:?}");
	}
	let actual = repl(
		Some("execution"),
		"let a=[1];0\na.push(2);async {1}.await\na.len()\n:dep polars\na.len()\n:reset\nasync {1}.await\n:quit\n",
	);
	let mut out = String::from_utf8(actual.stdout).unwrap();
	for number in 1..=8 {
		out = out
			.replace(&format!("[{number}] > "), "")
			.replace(&format!("[{number}] "), "");
	}
	let err = String::from_utf8(actual.stderr).unwrap();
	assert!(actual.status.success());
	assert_eq!(out, "0\n1\n1\nsession reset\n");
	assert!(err.contains("dependency context changes"), "{err}");
	assert_eq!(
		err.matches("async input is not available").count(),
		2,
		"{err}"
	);
}
#[test]
fn execution_spelling_is_repl_only() {
	for entry in ["eval", "run"] {
		let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
			.args([entry, "--engine=execution", "7"])
			.env("RNX_CONFIG", "")
			.env("NO_COLOR", "1")
			.output()
			.unwrap();
		assert!(!output.status.success());
		let message = String::from_utf8(output.stderr).unwrap();
		assert!(
			message.contains("available only with rnx repl"),
			"{message}"
		);
	}
}
#[test]
fn ctrl_c_after_output_stops_a_pure_loop_and_session_remains_usable() {
	let script = r#"
import os,re,signal,subprocess,sys
p=subprocess.Popen([sys.argv[1],'--no-splash','repl','--engine='+sys.argv[2]],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=dict(os.environ,RNX_CONFIG='',RNX_HISTORY='/dev/null',NO_COLOR='1'))
try:
 p.stdin.write(b'println!("ready"); loop {}\n');p.stdin.flush()
 ready=p.stdout.readline();assert ready.endswith(b'ready\n'),ready
 p.send_signal(signal.SIGINT)
 assert p.stderr.readline()==b'interrupted\n'
 p.stdin.write(b'7\n:quit\n');p.stdin.flush()
 out,err=p.communicate(timeout=15)
 out=re.sub(rb'\[\d+\] (?:> )?',b'',out)
 assert p.returncode==0 and out==b'7\n' and err==b'',(p.returncode,out,err)
finally:
 if p.returncode is None: p.kill();p.wait()
"#;
	for engine in ["legacy", "execution"] {
		let output = Command::new("timeout")
			.args([
				"--kill-after=2s",
				"25s",
				"python3",
				"-c",
				script,
				env!("CARGO_BIN_EXE_rnx"),
				engine,
			])
			.output()
			.unwrap();
		assert!(output.status.success(), "{engine}: {output:?}");
	}
}
