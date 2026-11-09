//! Real one-shot CLI behavior, separate from persistent session routing.
#![cfg(unix)]
use std::process::{Command, Output};

fn run(source: &str, legacy: bool, budget: Option<usize>, debug: bool) -> Output {
	let mut cmd = Command::new("timeout");
	cmd.args(["--kill-after=2s", "30s", env!("CARGO_BIN_EXE_rnx"), "eval"]);
	if legacy {
		cmd.arg("--engine=legacy");
	}
	if debug {
		cmd.arg("--debug-engine");
	}
	if let Some(budget) = budget {
		cmd.args(["--budget", &budget.to_string()]);
	}
	cmd.arg(source)
		.env("RNX_CONFIG", "")
		.env("NO_COLOR", "1")
		.output()
		.unwrap()
}

#[test]
fn one_shot_values_effects_errors_and_async_match_legacy() {
	for source in [
		"42",
		"()",
		"let x=3; x+4",
		"fn f(n){if n==0 {0} else {f(n-1)+1}} f(8)",
		"let v=[1,2]; v[0]=5; v",
		"Ok(7)",
		"Err(\"oops\")",
		"println!(\"once\"); 1/0",
		"let =",
		"async fn f(){7} f().await",
	] {
		let old = run(source, true, None, false);
		let fresh = run(source, false, None, false);
		assert_eq!(fresh.status.code(), old.status.code(), "{source}");
		assert_eq!(fresh.stdout, old.stdout, "{source}");
		assert_eq!(fresh.stderr, old.stderr, "{source}");
	}
}

#[test]
fn one_shot_route_discloses_interpreted_policy_and_legacy_refusal() {
	let fresh = run("42", false, None, true);
	assert!(fresh.status.success());
	let text = String::from_utf8(fresh.stderr).unwrap();
	assert!(text.contains("engine=execution"), "{text}");
	assert!(text.contains("execution=sliced-interpreted"), "{text}");
	assert!(text.contains("native-requested=on"), "{text}");
	let old = run("42", true, None, true);
	assert!(
		String::from_utf8(old.stderr)
			.unwrap()
			.contains("engine=legacy")
	);
	let asynchronous = run("async fn f(){7} f().await", false, None, true);
	assert!(
		String::from_utf8(asynchronous.stderr)
			.unwrap()
			.contains("refusal=Await")
	);
}

#[test]
fn one_shot_budget_is_terminal_and_does_not_repeat_output() {
	for legacy in [false, true] {
		let out = run("println!(\"once\"); loop {}", legacy, Some(100), false);
		let stderr = String::from_utf8(out.stderr).unwrap();
		assert_eq!(
			out.status.code(),
			Some(1),
			"legacy={legacy}, stderr={stderr:?}"
		);
		assert_eq!(
			String::from_utf8(out.stdout).unwrap(),
			"once\n",
			"legacy={legacy}, stderr={stderr:?}"
		);
		assert_eq!(
			stderr, "halted: 100 instructions exceeded\n",
			"legacy={legacy}, stderr={stderr:?}"
		);
	}
}

#[test]
fn one_shot_ctrl_c_stops_after_a_committed_output_boundary() {
	let script = r#"
import os,signal,subprocess,sys
for legacy in (False,True):
 cmd=[sys.argv[1],'eval']+(['--engine=legacy'] if legacy else [])+['println!("ready"); loop {}']
 p=subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=dict(os.environ,RNX_CONFIG='',NO_COLOR='1'))
 try:
  assert p.stdout.readline()==b'ready\n'
  p.send_signal(signal.SIGINT)
  stdout,stderr=p.communicate(timeout=15)
  assert p.returncode==130,(p.returncode,stderr)
  assert stdout==b'',stdout
 finally:
  if p.poll() is None: p.kill();p.wait()
"#;
	let result = Command::new("timeout")
		.args([
			"--kill-after=2s",
			"40s",
			"python3",
			"-c",
			script,
			env!("CARGO_BIN_EXE_rnx"),
		])
		.output()
		.unwrap();
	assert!(
		result.status.success(),
		"{}",
		String::from_utf8_lossy(&result.stderr)
	);
}
