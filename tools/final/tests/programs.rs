//! The CLI and Jupyter stage modes (ADR 0291) against the Python stage
//! programs they replace, at the record's witness revision: the CLI
//! checker's verdicts on its planted evidence, each run's commands,
//! outputs and receipt, and the Jupyter stage's children, environment and
//! receipt; then its declaration and its standalone build width.
use anyhow::Result;
use rnx_final::child::Done;
use rnx_final::failure::{Environment, python_repr};
use rnx_final::stage::{self, ChildRun, CliRun};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

const WITNESS_0291: &str = "427d0e3242b794698957773c421970905f68a6c7";

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

fn fixture(name: &str, substitutions: &[(&str, &str)]) -> Value {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("tests/witness")
		.join(format!("{name}.json"));
	let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
	assert_eq!(value["witness"], WITNESS_0291, "{name}");
	assert!(
		value["method"].as_str().is_some_and(|m| m.len() > 40),
		"{name}"
	);
	let mut text = value["expected"].to_string();
	for (mark, real) in substitutions {
		let escaped = serde_json::to_string(real).unwrap();
		text = text.replace(mark, &escaped[1..escaped.len() - 1]);
	}
	serde_json::from_str(&text).unwrap()
}

/// Python's last traceback line, `Class: message`, as the repr the Rust
/// receipts write.
fn as_repr(line: &str) -> String {
	let (class, message) = line.split_once(": ").unwrap();
	format!("{class}({})", python_repr(&json!(message)))
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-programs-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

#[test]
fn the_cli_checker_judges_its_planted_evidence_as_the_witness_did() {
	let want = fixture("cli-controls", &[]);
	stage::cli_controls().unwrap();
	let verdicts = want["verdicts"].as_array().unwrap();
	assert_eq!(verdicts.len(), 8);
	for v in verdicts {
		let got = match stage::validate_cli(
			v["code"].as_i64().unwrap() as i32,
			v["stdout"].as_str().unwrap(),
			v["stderr"].as_str().unwrap(),
			v["exhausted"].as_bool().unwrap(),
			v["stock"].as_bool().unwrap(),
		) {
			Ok(()) => "pass".to_string(),
			Err(e) => format!("AssertionError: {}", rnx_final::child::message(&e)),
		};
		assert_eq!(json!(got), v["verdict"], "{v}");
	}
}

/// Answers each run as the generator did.
struct Answers {
	scenario: &'static str,
	binary: PathBuf,
	calls: Vec<Value>,
}

impl CliRun for Answers {
	fn run(
		&mut self,
		command: &[String],
		cwd: &Path,
		limit: Duration,
	) -> Result<(i32, String, String)> {
		self.calls
			.push(json!({"command": command, "cwd": cwd, "timeout": limit.as_secs()}));
		let n = self.calls.len();
		let stock = command.last().unwrap().ends_with("cli-stock.rn");
		let work = &command[command.iter().position(|a| a == "--budget").unwrap() + 1];
		let legacy = "rnx: engine=legacy refusal=HostContract\n";
		let (code, mut stdout, stderr) = if work == "2000" {
			(
				1,
				"COLUMN_ENTER\n".to_string(),
				format!("{legacy}rnx: 2000 work units exceeded; --budget N raises it\n"),
			)
		} else if stock {
			(0, "COLUMN_ENTER\nCOLUMN_OK path=scalar-fallback; completed=true; rows=10000; valid_rows=10000; attempts=0; replays=0; refusal=None\n".to_string(), legacy.to_string())
		} else {
			(0, "COLUMN_ENTER\nCOLUMN_OK path=avx2; completed=true; rows=10000; valid_rows=10000; attempts=1; replays=0; refusal=None\n".to_string(), legacy.to_string())
		};
		if self.scenario == "bad-case" && n == 2 {
			stdout = stdout.replace("COLUMN_OK", "SKIPPED");
		}
		if self.scenario == "binary-changed" && n == 8 {
			std::fs::write(&self.binary, b"changed binary")?;
		}
		Ok((code, stdout, stderr))
	}
}

#[test]
fn every_cli_run_matches_the_witness() {
	for scenario in ["pass", "bad-case", "binary-changed"] {
		let s = scratch();
		let binary = s.0.join("rnx-polars");
		std::fs::write(&binary, b"planted binary").unwrap();
		let out = s.0.join("out");
		std::fs::create_dir(&out).unwrap();
		let want = fixture(
			"cli-controls",
			&[
				("<BINARY>", &binary.to_string_lossy()),
				("<OUT>", &out.to_string_lossy()),
				("<ROOT>", &root().to_string_lossy()),
			],
		)["runs"][scenario]
			.clone();
		let mut answers = Answers {
			scenario,
			binary: binary.clone(),
			calls: Vec::new(),
		};
		let receipt = stage::cli(&binary, &out, &root(), &mut answers).unwrap();
		let calls: Vec<Value> = want["calls"]
			.as_array()
			.unwrap()
			.iter()
			.map(|c| json!({"command": c["command"], "cwd": c["cwd"], "timeout": c["timeout"]}))
			.collect();
		assert_eq!(json!(answers.calls), json!(calls), "{scenario} calls");
		assert_eq!(receipt, want["receipt"], "{scenario} receipt");
		assert_eq!(
			receipt["status"] == "PASS",
			want["exit"] == 0,
			"{scenario} exit"
		);
		let mut files: Vec<String> = std::fs::read_dir(&out)
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		files.sort();
		assert_eq!(json!(files), want["files"], "{scenario} files");
		for (name, text) in want["texts"].as_object().unwrap() {
			assert_eq!(
				json!(std::fs::read_to_string(out.join(name)).unwrap()),
				*text,
				"{scenario} {name}"
			);
		}
		let last = std::fs::read_to_string(out.join("TRACEBACK.txt"))
			.ok()
			.map(|t| t.trim().lines().last().unwrap().to_owned());
		assert_eq!(json!(last), want["traceback_last"], "{scenario} traceback");
	}
}

/// Records each child as the generator recorded it.
struct Children {
	fail_second: bool,
	calls: Vec<Value>,
}

impl ChildRun for Children {
	fn run(
		&mut self,
		command: &[String],
		root: &Path,
		env: &Environment,
		log: &Path,
		directory: &Path,
		limit: Duration,
	) -> Result<Done> {
		std::fs::File::create_new(log)?;
		let shown: serde_json::Map<String, Value> = [
			"PYTHONDONTWRITEBYTECODE",
			"CARGO_BUILD_JOBS",
			"MAKEFLAGS",
			"PATH",
			"RNX_JUPYTER_PYTHON",
		]
		.iter()
		.map(|k| (k.to_string(), env.get(*k).map_or(Value::Null, |v| json!(v))))
		.collect();
		let keys: Vec<&String> = env.keys().collect();
		self.calls.push(json!({"command": command, "root": root, "cwd": root, "timeout": limit.as_secs(),
			"directory": directory.file_name().unwrap().to_string_lossy(), "log": log.file_name().unwrap().to_string_lossy(),
			"stderr_is_stdout": true, "env": shown, "env_keys": keys}));
		let n = self.calls.len();
		Ok(Done {
			returncode: if self.fail_second && n == 2 { 1 } else { 0 },
			validation: json!({"planted": n}),
			declared_output: None,
		})
	}
}

#[test]
fn the_jupyter_stage_runs_its_children_as_the_witness_ran_them() {
	for scenario in ["inherited", "second-fails", "no-client"] {
		let s = scratch();
		let python = s.0.join("client/bin/python");
		std::fs::create_dir_all(python.parent().unwrap()).unwrap();
		std::fs::write(&python, "").unwrap();
		let out = s.0.join("out");
		std::fs::create_dir(&out).unwrap();
		let want = fixture(
			"jupyter-controls",
			&[
				("<PYTHON>", &python.to_string_lossy()),
				("<OUT>", &out.to_string_lossy()),
				("<ROOT>", &root().to_string_lossy()),
				("<TMP>", &s.0.to_string_lossy()),
			],
		)[scenario]
			.clone();
		let client = if scenario == "no-client" {
			s.0.join("missing/python")
		} else {
			python.clone()
		};
		let env: Environment = [
			("PATH".to_string(), "/planted/bin".to_string()),
			(
				"MAKEFLAGS".to_string(),
				"-j7 --jobserver-auth=fifo:/nonexistent".to_string(),
			),
			(
				"RNX_JUPYTER_PYTHON".to_string(),
				client.to_string_lossy().into_owned(),
			),
		]
		.into();
		let mut children = Children {
			fail_second: scenario == "second-fails",
			calls: Vec::new(),
		};
		let receipt = stage::jupyter(&out, &root(), &env, &stage::build_jobs, &mut children);
		assert_eq!(json!(children.calls), want["calls"], "{scenario} calls");
		let mut expected = want["receipt"].clone();
		if let Some(error) = expected.get("error").and_then(Value::as_str) {
			expected["error"] = json!(as_repr(error));
		}
		assert_eq!(receipt, expected, "{scenario} receipt");
		assert_eq!(
			receipt["status"] == "PASS",
			want["exit"] == false,
			"{scenario} exit"
		);
		let mut files: Vec<String> = std::fs::read_dir(&out)
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		files.sort();
		assert_eq!(json!(files), want["files"], "{scenario} files");
		// The runner's children are its declaration's.
		if scenario == "inherited" {
			let declared = stage::jupyter_children(&python.to_string_lossy(), &root());
			let run: Vec<&Value> = children.calls.iter().map(|c| &c["command"]).collect();
			assert_eq!(json!(run), json!(declared));
		}
	}
}

/// The stage makes one budget decision and every child reuses it; the
/// rule is the one given (production passes the shared
/// `stage::build_jobs`, whose inherited width the witness holds).
#[test]
fn every_jupyter_child_reuses_one_budget_decision() {
	let s = scratch();
	let python = s.0.join("python");
	std::fs::write(&python, "").unwrap();
	let out = s.0.join("out");
	std::fs::create_dir(&out).unwrap();
	let env: Environment = [(
		"RNX_JUPYTER_PYTHON".to_string(),
		python.to_string_lossy().into_owned(),
	)]
	.into();
	let asked = std::cell::Cell::new(0);
	let rule = |e: &Environment| -> Result<usize> {
		assert!(e.get("MAKEFLAGS").is_none());
		asked.set(asked.get() + 1);
		Ok(5)
	};
	let mut children = Children {
		fail_second: false,
		calls: Vec::new(),
	};
	let receipt = stage::jupyter(&out, &root(), &env, &rule, &mut children);
	assert_eq!(receipt["status"], "PASS", "{receipt}");
	assert_eq!(asked.get(), 1);
	assert_eq!(children.calls.len(), 3);
	for call in &children.calls {
		assert_eq!(call["env"]["CARGO_BUILD_JOBS"], "5");
		assert_eq!(call["env"]["PYTHONDONTWRITEBYTECODE"], "1");
	}
}

/// The CLI's output is read as Python's text mode read it: an invalid
/// UTF-8 byte fails the run, even beside every success marker, and
/// newlines are universal.
#[test]
fn cli_output_is_decoded_strictly_with_universal_newlines() {
	let s = scratch();
	let mut live = stage::LiveCli {
		env: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
		out: s.0.clone(),
	};
	let sh = |script: &str| -> Vec<String> { vec!["sh".into(), "-c".into(), script.into()] };
	let good = "COLUMN_ENTER\\nCOLUMN_OK path=avx2; completed=true; rows=10000; valid_rows=10000; attempts=1; replays=0; refusal=None\\n";
	let error = live
		.run(
			&sh(&format!(
				"printf '{good}\\377'; printf 'rnx: engine=legacy' >&2"
			)),
			&s.0,
			Duration::from_secs(60),
		)
		.unwrap_err();
	assert!(
		format!("{error}").starts_with("UnicodeDecodeError("),
		"{error}"
	);
	let (code, stdout, stderr) = live
		.run(
			&sh("printf 'a\\r\\nb\\rc'; printf 'x\\r\\n' >&2"),
			&s.0,
			Duration::from_secs(60),
		)
		.unwrap();
	assert_eq!(
		(code, stdout.as_str(), stderr.as_str()),
		(0, "a\nb\nc", "x\n")
	);
	assert!(stage::text_mode(b"ok\xff").is_err());
}
