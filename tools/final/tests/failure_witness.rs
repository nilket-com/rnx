//! The failure diagnostics agree byte for byte with the Python controller's
//! on the same inputs, including its planted secrets and boundary cases.
use rnx_final::failure::{self, Environment};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

struct Dir(PathBuf);
impl Dir {
	fn new(label: &str) -> Self {
		let path = std::env::temp_dir().join(format!(
			"rnx-final-{label}-{}",
			rnx_final::receipt::invocation_id()
		));
		std::fs::create_dir(&path).unwrap();
		Self(path)
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn python(script: &str, input: &Value) -> Value {
	let output = std::process::Command::new("python3")
		.args(["-B", "-c", script, &input.to_string()])
		.current_dir(root().join("tools/column"))
		.output()
		.expect("python3");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap()
}

fn environment(value: &Value) -> Environment {
	value
		.as_object()
		.unwrap()
		.iter()
		.map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
		.collect()
}

const SANITIZE: &str = "import json, sys, failure_detail as d; a = json.loads(sys.argv[1]); print(json.dumps([d.sanitize(t, a['env']) for t in a['texts']]))";

#[test]
fn sanitize_matches_python() {
	let home = std::env::var("HOME").unwrap_or_else(|_| "/nonexistent-home".into());
	let env = json!({"SHLVL": "1", "X": "0", "HOME": home, "USER": "me", "TOKEN": "s3cr3t-abcdef", "PATH": "/usr/bin:/opt/bin", "TOOL_PATH": "/opt/bin", "COUNT": "123456789", "LC_TEST": "long-locale", "LONGER": "s3cr3t-abcdef-and-more"});
	let mut texts = vec![
		"InsufficientMemory: host_available_bytes=41000000000, workers=0, reserve_bytes=2147483648".to_string(),
		format!("File \"{home}/work/rnx/tools/a.py\", line 1, in x"),
		"exit 1 with rust 1.98.1; run cargo test --locked again; s3cr3t-abcdef and s3cr3t-abcdef-and-more".into(),
		"/opt/bin 123456789 long-locale".into(),
		"argv: [\"child\", \"--token=x\"]\nnext\r\nline\x0bsplit\u{2028}u\n['child', 'x']\ncommand ['a']\n  environment = {}\nfoo: execve(\n".into(),
		"--Token X --api_key=1 --access-token 'a b' --client-secret \"q\\\"r\" --my-auth-flag=v,rest; --keyword=keep --tokens=y".into(),
		String::new(),
		"no newline at end".into(),
	];
	for flag in [
		"--token",
		"--api-key",
		"--password",
		"--client-secret",
		"--auth",
	] {
		for separator in ["=", " ", "\t "] {
			texts.push(format!(
				"failed with {flag}{separator}\"unknown-secret-value\"; run --locked again"
			));
		}
	}
	let expected = python(SANITIZE, &json!({"env": env, "texts": texts}));
	let actual: Vec<String> = texts
		.iter()
		.map(|t| failure::sanitize(t, &environment(&env)))
		.collect();
	assert_eq!(json!(actual), expected);
}

/// Both controllers retain the same failure from the same files.
fn retain_both(
	label: &str,
	setup: impl Fn(&Path) -> (Option<PathBuf>, Vec<PathBuf>, Value, String),
) {
	let dir = Dir::new(label);
	let source = dir.0.join("source");
	std::fs::create_dir(&source).unwrap();
	let (receipt, logs, env, error) = setup(&source);
	let (python_out, rust_out) = (dir.0.join("python"), dir.0.join("rust"));
	std::fs::create_dir(&python_out).unwrap();
	std::fs::create_dir(&rust_out).unwrap();
	let script = "import json, sys, failure_detail as d; from pathlib import Path; a = json.loads(sys.argv[1]); \ntry: r = d.retain(Path(a['out']), 'capture', receipt=a['receipt'] and Path(a['receipt']), logs=[Path(p) for p in a['logs']], roots=[Path(a['root'])], environment=a['env'], error=a['error'])\nexcept ValueError as e: r = {'refused': str(e)}\nprint(json.dumps(r))";
	let expected = python(
		script,
		&json!({"out": python_out, "receipt": receipt, "logs": logs, "root": source, "env": env, "error": error}),
	);
	let actual = match failure::retain(
		&rust_out,
		"capture",
		receipt.as_deref(),
		&logs,
		std::slice::from_ref(&source),
		&environment(&env),
		&error,
	) {
		Ok(reference) => reference,
		Err(refused) => json!({"refused": refused.to_string()}),
	};
	if expected.get("refused").is_some() {
		assert_eq!(actual, expected, "{label}: both must refuse alike");
		return;
	}
	assert_eq!(actual["error"], expected["error"], "{label}");
	assert_eq!(
		actual["sha256"], expected["sha256"],
		"{label}: FAILURE.json differs"
	);
	let names = |out: &Path| -> Vec<(String, Vec<u8>)> {
		let mut rows: Vec<_> = std::fs::read_dir(out.join("failure"))
			.unwrap()
			.map(|e| e.unwrap().path())
			.map(|p| {
				(
					p.file_name().unwrap().to_string_lossy().into_owned(),
					std::fs::read(&p).unwrap(),
				)
			})
			.collect();
		rows.sort();
		rows
	};
	assert_eq!(
		names(&rust_out),
		names(&python_out),
		"{label}: failure files differ"
	);
	assert_eq!(
		std::fs::read(rust_out.join("FAILURE.json")).unwrap(),
		std::fs::read(python_out.join("FAILURE.json")).unwrap()
	);
}

fn write_json(path: &Path, value: &Value) {
	rnx_final::receipt::atomic_json(path, value).unwrap();
}

#[test]
fn large_log_and_planted_secrets_are_retained_alike() {
	retain_both("secrets", |source| {
		let secret = "PLANTED_PRIVATE_VALUE_0271_DO_NOT_KEEP";
		let argv = "PLANTED_ARGV_VALUE_0271_DO_NOT_KEEP";
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "error": format!("real failure {secret}"), "command": ["bad", argv], "environment": {"token": secret}, "accounting": {"big": "x".repeat(100000)}}),
		);
		let line =
			format!("argv: [\"child\", \"--token={argv}\"]\n['child', '{argv}']\n{secret}\n");
		std::fs::write(
			source.join("child.log"),
			line.repeat(50000) + "actual error\n",
		)
		.unwrap();
		(
			Some(source.join("RESULT.json")),
			vec![source.join("child.log")],
			json!({"RNX_SECRET": secret}),
			"stage failed".into(),
		)
	});
}

#[test]
fn a_secret_split_at_an_excerpt_boundary_leaves_no_fragment_alike() {
	retain_both("boundary", |source| {
		let secret = "BOUNDARY_PRIVATE_SENTINEL_0271_NEVER_RETAIN";
		let mut data = b"a\n".repeat(3 * failure::FILE_CAP / 2);
		let len = data.len();
		for offset in [failure::FILE_CAP / 2 - 10, len - failure::FILE_CAP / 2 - 10] {
			data[offset..offset + secret.len()].copy_from_slice(secret.as_bytes());
		}
		std::fs::write(source.join("child.log"), &data).unwrap();
		(
			None,
			vec![source.join("child.log")],
			json!({"TOKEN": secret}),
			"stage failed".into(),
		)
	});
}

#[test]
fn a_nested_failing_leaf_is_chosen_alike() {
	retain_both("nested", |source| {
		let case = source.join("case");
		std::fs::create_dir(&case).unwrap();
		write_json(
			&case.join("RESULT.json"),
			&json!({"status": "FAIL", "kind": "fib", "error": "leaf broke", "configuration": "private"}),
		);
		std::fs::write(case.join("child.log"), "leaf log\n").unwrap();
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "error": "capture failed", "records": [case.join("RESULT.json")]}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![],
			json!({}),
			"stage failed".into(),
		)
	});
}

#[test]
fn failing_cases_declared_files_and_rule_failures_alike() {
	retain_both("cases", |source| {
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "failed_cases": [{"id": "case-7", "message": "comparison differs", "command": ["PRIVATE"], "B1": 999}]}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![],
			json!({}),
			"stage failed".into(),
		)
	});
	retain_both("declared", |source| {
		std::fs::write(
			source.join("named.stderr"),
			"error: explicitly declared cause\n",
		)
		.unwrap();
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "diagnostic_files": ["named.stderr"]}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![],
			json!({}),
			"stage failed".into(),
		)
	});
	retain_both("rules", |source| {
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "per_record": [{"kind": "fib", "mode": "off", "metric": "W", "level_pass": true, "growth_pass": false, "pass_all": false, "B1": 999}], "admission": {"workers": 0, "binding_cgroup": "/x", "reserve_bytes": 5, "other": "drop"}}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![],
			json!({"T": "s3cr3t-abcdef"}),
			"stage failed".into(),
		)
	});
}

#[test]
fn a_missing_receipt_keeps_the_log_alike() {
	retain_both("missing", |source| {
		std::fs::write(source.join("child.log"), "original cause\n").unwrap();
		(
			Some(source.join("missing")),
			vec![source.join("child.log")],
			json!({}),
			"missing receipt".into(),
		)
	});
	retain_both("exit", |source| {
		std::fs::write(
			source.join("child.log"),
			"no recognizable cause here\njust output\n",
		)
		.unwrap();
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "error": "child exit is nonzero"}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![source.join("child.log")],
			json!({}),
			"child exit is nonzero".into(),
		)
	});
	retain_both("long-error", |source| {
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "error": "x".repeat(20000)}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![],
			json!({}),
			"stage failed".into(),
		)
	});
	// Escaped non-ASCII text overflows the index; both refuse it.
	retain_both("escaped-error", |source| {
		write_json(
			&source.join("RESULT.json"),
			&json!({"status": "FAIL", "error": "é".repeat(20000)}),
		);
		(
			Some(source.join("RESULT.json")),
			vec![],
			json!({}),
			"stage failed".into(),
		)
	});
}

#[test]
fn a_nested_test_report_keeps_the_inner_assertion_alike() {
	retain_both("nested-report", |source| {
		let text = "early\n".repeat(20000)
			+ "---- outer stdout ----\nthread outer panicked at outer:1:\nrunning 1 test\nfailures:\nfailures:\n    inner\ntest result: FAILED. 0 passed; 1 failed\nthread inner panicked at inner:2:\nassertion failed: concrete nested cause\nerror: test failed, to rerun pass --lib\n"
			+ &"late\n".repeat(20000);
		std::fs::write(source.join("nested.log"), text).unwrap();
		(
			None,
			vec![source.join("nested.log")],
			json!({}),
			"stage failed".into(),
		)
	});
}

#[test]
fn links_and_outside_paths_are_refused() {
	let dir = Dir::new("links");
	let source = dir.0.join("source");
	std::fs::create_dir(&source).unwrap();
	let outside = dir.0.join("outside");
	std::fs::write(&outside, "PRIVATE").unwrap();
	std::os::unix::fs::symlink(&outside, source.join("linked")).unwrap();
	for path in [outside.clone(), source.join("linked")] {
		assert!(
			failure::checked(&path, std::slice::from_ref(&source)).is_err(),
			"{}",
			path.display()
		);
	}
}

#[test]
fn a_copied_bundle_matches_and_a_tampered_one_is_refused() {
	let dir = Dir::new("copy");
	let source = dir.0.join("source");
	std::fs::create_dir(&source).unwrap();
	write_json(
		&source.join("RESULT.json"),
		&json!({"status": "FAIL", "error": "leaf broke"}),
	);
	let out = dir.0.join("out");
	std::fs::create_dir(&out).unwrap();
	let reference = failure::retain(
		&out,
		"capture",
		Some(&source.join("RESULT.json")),
		&[],
		std::slice::from_ref(&source),
		&Environment::new(),
		"stage failed",
	)
	.unwrap();
	let (python_copy, rust_copy) = (dir.0.join("python-copy"), dir.0.join("rust-copy"));
	std::fs::create_dir(&python_copy).unwrap();
	std::fs::create_dir(&rust_copy).unwrap();
	let script = "import json, sys, failure_detail as d; from pathlib import Path; a = json.loads(sys.argv[1]); print(json.dumps(d.copy_bundle(Path(a['out']), a['reference'], roots=[Path(a['root'])], environment={})))";
	let expected = python(
		script,
		&json!({"out": python_copy, "reference": reference, "root": out}),
	);
	let actual = failure::copy_bundle(
		&rust_copy,
		&reference,
		std::slice::from_ref(&out),
		&Environment::new(),
	)
	.unwrap();
	assert_eq!(
		(actual["sha256"].clone(), actual["error"].clone()),
		(expected["sha256"].clone(), json!("leaf broke"))
	);
	std::fs::write(out.join("failure/00-receipt.json"), "tampered").unwrap();
	let bad = dir.0.join("bad");
	std::fs::create_dir(&bad).unwrap();
	assert!(
		failure::copy_bundle(
			&bad,
			&reference,
			std::slice::from_ref(&out),
			&Environment::new()
		)
		.unwrap_err()
		.to_string()
		.contains("changed")
	);
}

#[test]
fn named_sections_precede_the_tail_and_oversized_lines_are_not_split() {
	let dir = Dir::new("sections");
	let log = dir.0.join("large.log");
	let text = "early\n".repeat(20000)
		+ "---- first stdout ----\nthread first panicked at fixture:1:\n"
		+ &"x".repeat(100000)
		+ "\nfailures:\nfirst\n---- second stdout ----\nthread second panicked at fixture:2:\nfailures:\nsecond\n"
		+ &"late\n".repeat(20000);
	std::fs::write(&log, &text).unwrap();
	let (data, truncated) = failure::cause_excerpt(&log, failure::FILE_CAP).unwrap();
	assert!(truncated && data.len() <= failure::FILE_CAP);
	assert!(data.starts_with(b"---- first stdout ----"));
	assert!(data.windows(23).any(|w| w == b"---- second stdout ----"));
	assert!(!data.windows(100).any(|w| w.iter().all(|b| *b == b'x')));
	retain_both("sections-alike", |source| {
		std::fs::write(source.join("large.log"), &text).unwrap();
		(
			None,
			vec![source.join("large.log")],
			json!({}),
			"stage failed".into(),
		)
	});
}
