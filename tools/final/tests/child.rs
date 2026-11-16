//! Ports of the Python supervision controls (test_child_result.py). The
//! producer is a real Python stage program binding its receipt through the
//! unchanged child_result.publish, so each pass also shows the context and
//! source identities this controller writes are the ones Python computes.
use rnx_final::child::{self, Done, Run, contract, evidence, validate};
use rnx_final::failure::Environment;
use rnx_final::receipt::invocation_id;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

const GUARDIAN: &str = env!("CARGO_BIN_EXE_rnx-final");

fn rnx() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

struct Fixture {
	root: PathBuf,
	out: PathBuf,
	script: PathBuf,
}

const PRODUCER: &str = "import sys\nsys.path.insert(0, COLUMN)\nimport child_result as c\nfrom pathlib import Path\nc.publish(Path(sys.argv[sys.argv.index('--out')+1])/'RESULT.json', {'status':'PASS'})\n";

impl Fixture {
	fn new() -> Self {
		let root = std::env::temp_dir().join(format!("rnx-final-child-{}", invocation_id()));
		std::fs::create_dir(&root).unwrap();
		let root = root.canonicalize().unwrap();
		let git = |args: &[&str]| {
			assert!(
				std::process::Command::new("git")
					.arg("-C")
					.arg(&root)
					.args(args)
					.status()
					.unwrap()
					.success()
			)
		};
		git(&["init", "-q"]);
		std::fs::write(root.join(".gitignore"), "runs/\n").unwrap();
		git(&["add", ".gitignore"]);
		let out = root.join("runs");
		std::fs::create_dir(&out).unwrap();
		// A registered producer path, so no registration is patched.
		std::fs::create_dir_all(root.join("tools/column")).unwrap();
		let fixture = Self {
			script: root.join("tools/column/suite_stage.py"),
			root,
			out,
		};
		fixture.write(PRODUCER);
		fixture
	}

	fn write(&self, code: &str) {
		let column = rnx().join("tools/column");
		std::fs::write(
			&self.script,
			code.replace("COLUMN", &format!("{:?}", column.to_string_lossy())),
		)
		.unwrap();
	}

	fn env(&self) -> Environment {
		std::env::vars().collect()
	}

	fn run(
		&self,
		command: &[String],
		name: &str,
		env: &Environment,
		timeout: Option<Duration>,
	) -> anyhow::Result<Done> {
		child::run(Run {
			command,
			cwd: &self.root,
			root: &self.root,
			env,
			log: &self.out.join(format!("{name}.log")),
			timeout,
			directory: &self.out.join(name),
			groups: &self.out,
			guardian: Path::new(GUARDIAN),
			after: None,
			stop: None,
		})
	}

	fn invoke(&self, name: &str) -> Done {
		let result_dir = self.out.join(format!("{name}-producer"));
		std::fs::create_dir_all(&result_dir).unwrap();
		let command = vec![
			"python3".into(),
			self.script.to_string_lossy().into_owned(),
			"--out".into(),
			result_dir.to_string_lossy().into_owned(),
		];
		self.run(&command, name, &self.env(), None).unwrap()
	}
}

impl Drop for Fixture {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.root);
	}
}

fn receipt(done: &Done) -> Value {
	serde_json::from_slice(&std::fs::read(done.validation["path"].as_str().unwrap()).unwrap())
		.unwrap()
}

#[test]
fn a_real_producer_and_a_raw_leaf_pass() {
	let f = Fixture::new();
	let done = f.invoke("attempt");
	assert_eq!(done.returncode, 0, "{}", receipt(&done));
	let row = evidence(&done.validation, &json!(child::source(&f.root).unwrap())).unwrap();
	assert_eq!(row["context"]["kind"], "producer");
	let leaf = f
		.run(
			&["python3".into(), "--version".into()],
			"leaf",
			&f.env(),
			None,
		)
		.unwrap();
	assert_eq!(leaf.returncode, 0);
}

#[test]
fn a_zero_exit_without_a_receipt_fails() {
	let f = Fixture::new();
	f.write("pass\n");
	let done = f.invoke("attempt");
	assert_ne!(done.returncode, 0);
	let row = receipt(&done);
	assert_eq!(
		(
			row["process_exit"].clone(),
			row["status"].clone(),
			row["cleanup_seconds"].clone()
		),
		(json!(0), json!("FAIL"), json!(0))
	);
	assert!(row["seconds"].as_f64().unwrap() >= 0.0);
}

#[test]
fn a_fail_receipt_with_zero_exit_and_a_pass_with_nonzero_exit_both_fail() {
	let f = Fixture::new();
	f.write(&PRODUCER.replace("'PASS'", "'FAIL'"));
	assert_ne!(f.invoke("fail").returncode, 0);
	f.write(&format!("{PRODUCER}raise SystemExit(7)\n"));
	let done = f.invoke("nonzero");
	assert_eq!(done.returncode, 7);
	assert!(
		receipt(&done)["error"]
			.as_str()
			.unwrap()
			.contains("child exit is nonzero")
	);
}

#[test]
fn a_wrong_context_source_or_malformed_receipt_is_refused() {
	let f = Fixture::new();
	let done = f.invoke("attempt");
	let directory = Path::new(done.validation["path"].as_str().unwrap())
		.parent()
		.unwrap()
		.to_owned();
	let ctx: Value =
		serde_json::from_slice(&std::fs::read(directory.join("CONTEXT.json")).unwrap()).unwrap();
	let original: Value =
		serde_json::from_slice(&std::fs::read(directory.join("CHILD.json")).unwrap()).unwrap();
	for (key, value) in [
		("invocation", json!("stale")),
		("source", json!("wrong")),
		("context", json!("wrong")),
		("status", json!("FAIL")),
		("schema", json!(99)),
	] {
		let mut row = original.clone();
		row[key] = value;
		std::fs::write(directory.join("CHILD.json"), row.to_string()).unwrap();
		assert!(validate(&directory, &ctx, 0).is_err(), "{key}");
	}
	std::fs::write(directory.join("CHILD.json"), "{").unwrap();
	assert!(validate(&directory, &ctx, 0).is_err());
	std::fs::write(directory.join("CHILD.json"), original.to_string()).unwrap();
	assert!(validate(&directory, &ctx, 1).is_err());
	validate(&directory, &ctx, 0).unwrap();
	let changed = json!(
		ctx.as_object()
			.unwrap()
			.clone()
			.into_iter()
			.map(|(k, v)| if k == "environment" {
				(k, json!("wrong"))
			} else {
				(k, v)
			})
			.collect::<serde_json::Map<_, _>>()
	);
	assert!(
		validate(&directory, &changed, 0).is_err(),
		"a changed recipe"
	);
	std::fs::write(
		original["receipt"].as_str().unwrap(),
		r#"{"status":"FAIL"}"#,
	)
	.unwrap();
	assert!(validate(&directory, &ctx, 0).is_err());
}

#[test]
fn a_preexisting_attempt_and_unregistered_children_fail_closed() {
	let f = Fixture::new();
	f.invoke("attempt");
	assert!(
		f.run(
			&["cargo".into(), "--version".into()],
			"attempt",
			&f.env(),
			None
		)
		.is_err()
	);
	let stray = f.root.join("stray.py");
	std::fs::write(&stray, "pass\n").unwrap();
	assert!(
		contract(
			&["python3".into(), stray.to_string_lossy().into_owned()],
			&f.root,
			&f.root
		)
		.is_err()
	);
	assert!(
		contract(
			&["python3".into(), "-m".into(), "tools.column.verify".into()],
			&f.root,
			&f.root
		)
		.is_err()
	);
	assert!(
		contract(
			&["bash".into(), "-c".into(), "true".into()],
			&f.root,
			&f.root
		)
		.is_err()
	);
	assert_eq!(
		contract(
			&["python3".into(), "-m".into(), "unittest".into()],
			&f.root,
			&f.root
		)
		.unwrap(),
		"leaf"
	);
}

#[test]
fn a_source_change_during_the_child_fails() {
	let f = Fixture::new();
	f.write(&PRODUCER.replace(
		"c.publish(",
		"Path('changed').write_text('changed')\nc.publish(",
	));
	let done = f.invoke("attempt");
	assert_ne!(done.returncode, 0);
}

#[test]
fn a_resume_archives_the_prior_result_and_rechecks_artifacts() {
	let f = Fixture::new();
	let first = f.invoke("first");
	assert_eq!(first.returncode, 0);
	let producer = f.out.join("first-producer");
	let old = std::fs::read(producer.join("RESULT.json")).unwrap();
	let command = vec![
		"python3".into(),
		f.script.to_string_lossy().into_owned(),
		"--out".into(),
		producer.to_string_lossy().into_owned(),
		"--resume".into(),
	];
	let second = f.run(&command, "second", &f.env(), None).unwrap();
	assert_eq!(second.returncode, 0, "{}", receipt(&second));
	let prior = std::fs::read_dir(f.out.join("second"))
		.unwrap()
		.flatten()
		.find(|e| e.file_name().to_string_lossy().starts_with("PRIOR-"))
		.unwrap();
	assert_eq!(std::fs::read(prior.path()).unwrap(), old);
	let source = json!(child::source(&f.root).unwrap());
	evidence(&second.validation, &source).unwrap();
	std::fs::write(f.out.join("second.log"), "corrupt").unwrap();
	assert!(evidence(&second.validation, &source).is_err());
}

#[test]
fn a_completed_failure_cannot_be_resumed() {
	let f = Fixture::new();
	let producer = f.out.join("producer");
	std::fs::create_dir(&producer).unwrap();
	std::fs::write(producer.join("RESULT.json"), r#"{"status":"FAIL"}"#).unwrap();
	let command = vec![
		"python3".into(),
		f.script.to_string_lossy().into_owned(),
		"--out".into(),
		producer.to_string_lossy().into_owned(),
		"--resume".into(),
	];
	let done = f.run(&command, "failed", &f.env(), None).unwrap();
	assert_ne!(done.returncode, 0);
	assert_eq!(
		std::fs::read_to_string(producer.join("RESULT.json")).unwrap(),
		r#"{"status":"FAIL"}"#
	);
}

#[test]
fn only_declared_environment_names_reach_receipts() {
	let base: Environment = [
		("RUSTFLAGS", "private-recipe-value"),
		("CARGO_BUILD_JOBS", "4"),
		("RNX_MODE", "private-mode"),
	]
	.into_iter()
	.map(|(k, v)| (k.into(), v.into()))
	.collect();
	let bound = child::environment(&base).unwrap();
	assert_eq!(
		bound["keys"],
		json!(["CARGO_BUILD_JOBS", "RNX_MODE", "RUSTFLAGS"])
	);
	let mut wider = base.clone();
	for (k, v) in [
		("TERM", "other"),
		("PWD", "/elsewhere"),
		("SSH_AUTH_SOCK", "socket"),
		("UNDECLARED_SECRET", "secret"),
	] {
		wider.insert(k.into(), v.into());
	}
	assert_eq!(child::environment(&wider).unwrap(), bound);
	let mut changed = base.clone();
	changed.insert("RUSTFLAGS".into(), "changed".into());
	assert_ne!(child::environment(&changed).unwrap(), bound);
	// The same identity Python computes.
	let script = "import json, sys; sys.path.insert(0, sys.argv[2]); import child_result as c; print(json.dumps(c.environment(json.loads(sys.argv[1]))))";
	let output = std::process::Command::new("python3")
		.args([
			"-B",
			"-c",
			script,
			&json!(wider).to_string(),
			&rnx().join("tools/column").to_string_lossy(),
		])
		.output()
		.unwrap();
	assert_eq!(
		serde_json::from_slice::<Value>(&output.stdout).unwrap(),
		bound
	);
	let f = Fixture::new();
	let mut env = f.env();
	env.extend(base);
	env.insert("UNDECLARED_SECRET".into(), "unique-secret-marker".into());
	let result_dir = f.out.join("producer");
	std::fs::create_dir(&result_dir).unwrap();
	let command = vec![
		"python3".into(),
		f.script.to_string_lossy().into_owned(),
		"--out".into(),
		result_dir.to_string_lossy().into_owned(),
	];
	assert_eq!(
		f.run(&command, "attempt", &env, None).unwrap().returncode,
		0
	);
	for entry in walk(&f.out) {
		if entry.extension().is_some_and(|e| e == "json") {
			let text = std::fs::read_to_string(&entry).unwrap();
			for forbidden in [
				"private-recipe-value",
				"private-mode",
				"unique-secret-marker",
				"UNDECLARED_SECRET",
			] {
				assert!(
					!text.contains(forbidden),
					"{forbidden} in {}",
					entry.display()
				);
			}
		}
	}
}

fn walk(path: &Path) -> Vec<PathBuf> {
	let mut files = Vec::new();
	for entry in std::fs::read_dir(path).unwrap().flatten() {
		let p = entry.path();
		if p.is_dir() {
			files.extend(walk(&p))
		} else {
			files.push(p)
		}
	}
	files
}

#[test]
fn a_timeout_drains_child_and_grandchild() {
	let f = Fixture::new();
	let pids = f.out.join("pids");
	let grandchild = format!(
		"import os,signal;open({:?},'a').write(str(os.getpid())+'\\n');signal.pause()",
		pids.to_string_lossy()
	);
	let code = format!(
		"import os,subprocess,sys,signal;open({:?},'a').write(str(os.getpid())+'\\n');subprocess.Popen([sys.executable,'-c',{grandchild:?}]);signal.pause()",
		pids.to_string_lossy()
	);
	// A test_ script is a registered leaf, as the Python control's patch made it.
	let script = f.root.join("test_timeout.py");
	std::fs::write(&script, code).unwrap();
	let done = f
		.run(
			&["python3".into(), script.to_string_lossy().into_owned()],
			"timeout",
			&f.env(),
			Some(Duration::from_secs(1)),
		)
		.unwrap();
	assert_ne!(done.returncode, 0);
	let row = receipt(&done);
	assert_eq!(
		(row["status"].clone(), row["error_type"].clone()),
		(json!("FAIL"), json!("TimeoutExpired"))
	);
	let ids: Vec<i32> = std::fs::read_to_string(&pids)
		.unwrap()
		.lines()
		.map(|l| l.parse().unwrap())
		.collect();
	assert_eq!(ids.len(), 2);
	for pid in ids {
		assert!(
			rnx_final::guardian::start_ticks(pid).is_none(),
			"{pid} survived the timeout"
		);
	}
}

#[test]
fn owned_build_cleanup_is_timed_and_its_receipt_revalidates() {
	let f = Fixture::new();
	f.write(&PRODUCER.replace("c.publish(", "from owned_build import OwnedBuild\nwith OwnedBuild(Path(sys.argv[-1]), 'target') as owned:\n owned.path.mkdir()\n (owned.path/'object').write_text('build')\nc.publish("));
	let done = f.invoke("attempt");
	assert_eq!(done.returncode, 0, "{}", receipt(&done));
	let source = json!(child::source(&f.root).unwrap());
	let row = evidence(&done.validation, &source).unwrap();
	assert!(row["seconds"].as_f64().unwrap() >= row["cleanup_seconds"].as_f64().unwrap());
	assert!(row["cleanup_seconds"].as_f64().unwrap() > 0.0);
	assert!(!f.out.join("attempt-producer/target").exists());
	std::fs::write(row["cleanup"][0]["path"].as_str().unwrap(), "tampered").unwrap();
	assert!(evidence(&done.validation, &source).is_err());
}

#[test]
fn a_spawn_failure_is_a_failed_receipt() {
	let f = Fixture::new();
	let mut env = f.env();
	env.insert("PATH".into(), "/nonexistent".into());
	let done = f.run(&["cargo".into(), "--version".into()], "spawn", &env, None);
	match done {
		Ok(done) => assert_eq!(receipt(&done)["status"], "FAIL"),
		Err(error) => panic!("a spawn failure must still write a receipt: {error:#}"),
	}
}

#[test]
fn a_stage_writing_tracked_source_fails_its_neighbor() {
	let f = Fixture::new();
	std::fs::write(f.root.join("tracked.txt"), "original").unwrap();
	assert!(
		std::process::Command::new("git")
			.arg("-C")
			.arg(&f.root)
			.args(["add", "tracked.txt"])
			.status()
			.unwrap()
			.success()
	);
	assert!(
		std::process::Command::new("git")
			.arg("-C")
			.arg(&f.root)
			.args([
				"-c",
				"user.name=T",
				"-c",
				"user.email=t@invalid",
				"-c",
				"commit.gpgsign=false",
				"commit",
				"-qm",
				"f"
			])
			.status()
			.unwrap()
			.success()
	);
	// Ignored test scripts: registered leaf children of this checkout.
	std::fs::write(
		f.out.join("test_neighbor.py"),
		"import time; time.sleep(1.5)",
	)
	.unwrap();
	std::fs::write(
		f.out.join("test_writer.py"),
		"open('tracked.txt','w').write('changed')",
	)
	.unwrap();
	let neighbor = std::thread::scope(|scope| {
		let neighbor = scope.spawn(|| {
			f.run(
				&[
					"python3".into(),
					f.out
						.join("test_neighbor.py")
						.to_string_lossy()
						.into_owned(),
				],
				"neighbor",
				&f.env(),
				Some(Duration::from_secs(60)),
			)
			.unwrap()
		});
		std::thread::sleep(Duration::from_millis(500));
		f.run(
			&[
				"python3".into(),
				f.out.join("test_writer.py").to_string_lossy().into_owned(),
			],
			"writer",
			&f.env(),
			Some(Duration::from_secs(60)),
		)
		.unwrap();
		neighbor.join().unwrap()
	});
	assert_eq!(neighbor.returncode, 1);
	assert!(
		receipt(&neighbor)["error"]
			.as_str()
			.unwrap()
			.contains("source changed during child")
	);
}

#[test]
fn a_lock_free_status_leaves_the_index_alone() {
	let f = Fixture::new();
	std::fs::write(f.root.join("tracked.txt"), "same").unwrap();
	let git = |args: &[&str], locks: bool| {
		let mut command = if locks {
			std::process::Command::new("git")
		} else {
			child::git_command()
		};
		if locks {
			command.env_remove("GIT_OPTIONAL_LOCKS");
		}
		assert!(
			command
				.arg("-C")
				.arg(&f.root)
				.args([
					"-c",
					"user.name=T",
					"-c",
					"user.email=t@invalid",
					"-c",
					"commit.gpgsign=false"
				])
				.args(args)
				.output()
				.unwrap()
				.status
				.success()
		);
	};
	git(&["add", "."], true);
	git(&["commit", "-qm", "f"], true);
	let stamp = || {
		std::fs::metadata(f.root.join(".git/index"))
			.unwrap()
			.modified()
			.unwrap()
	};
	// Stale stat data, same content.
	let file = std::fs::File::options()
		.write(true)
		.open(f.root.join("tracked.txt"))
		.unwrap();
	file.set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1))
		.unwrap();
	file.set_modified(std::time::SystemTime::now()).unwrap();
	let before = stamp();
	git(&["status", "--porcelain"], false);
	assert_eq!(stamp(), before, "a lock-free status rewrote the index");
	std::thread::sleep(Duration::from_millis(20));
	git(&["status", "--porcelain"], true);
	assert_ne!(stamp(), before, "the hazard: a default status rewrites it");
}

/// A real producer's cause survives the common runner and release, and no
/// planted secret reaches the retained bundle.
#[test]
fn a_real_cause_survives_the_runner_and_release() {
	for producer in [false, true] {
		let f = Fixture::new();
		// Under the checkout's ignored directory, so the producer's writes
		// are not source changes.
		let mut owner =
			rnx_final::evidence::Owners::new(&f.out.join("owners"), &f.out.join("final")).unwrap();
		let bulk = owner.assign(&f.out, "payload", None).unwrap();
		let secret = "PLANTED_MIDDLE_CAUSE_SECRET_0276";
		let (command, expected) = if producer {
			f.write("import os,sys\nsys.path.insert(0, COLUMN)\nimport child_result as c\nfrom pathlib import Path\np=Path(sys.argv[-1])\n(p/'controller.stderr').write_text('cross verification: census fixture identity mismatch: src/time.rs\\n'+os.environ['RNX_FIXTURE_SECRET']+'\\n')\nc.publish(p/'RESULT.json',{'status':'FAIL'})\nraise SystemExit(1)\n");
			(
				vec![
					"python3".into(),
					f.script.to_string_lossy().into_owned(),
					"--out".into(),
					bulk.to_string_lossy().into_owned(),
				],
				"cross verification: census fixture identity mismatch: src/time.rs",
			)
		} else {
			let section = f.root.join("section.txt");
			std::fs::write(&section, "---- middle_failure stdout ----\n\nthread \"middle_failure\" panicked at src/fixture.rs:12:3:\nassertion failed ").unwrap();
			let script = f.root.join("test_fixture.py");
			std::fs::write(&script, format!("import os\nprint('before\\n'*30000)\nprint(open({:?}).read()+os.environ['RNX_FIXTURE_SECRET']+'\\n\\nfailures:\\n    middle_failure\\n')\nprint('after\\n'*30000)\nraise SystemExit(1)\n", section.to_string_lossy())).unwrap();
			(
				vec!["python3".into(), script.to_string_lossy().into_owned()],
				"thread \"middle_failure\" panicked at src/fixture.rs:12:3:",
			)
		};
		let mut env = f.env();
		env.insert("RNX_FIXTURE_SECRET".into(), secret.into());
		let done = f.run(&command, "attempt", &env, None).unwrap();
		let row = receipt(&done);
		assert_eq!(row["status"], "FAIL");
		assert_ne!(done.returncode, 0);
		assert_eq!(owner.release().unwrap()["status"], "PASS");
		assert!(!bulk.exists());
		let failure: Value = serde_json::from_slice(
			&std::fs::read(row["failure"]["path"].as_str().unwrap()).unwrap(),
		)
		.unwrap();
		assert_eq!(
			(
				failure["error"].as_str().unwrap(),
				row["error"].as_str().unwrap()
			),
			(expected, expected)
		);
		let text: String = std::fs::read_dir(f.out.join("attempt/failure"))
			.unwrap()
			.flatten()
			.map(|e| std::fs::read_to_string(e.path()).unwrap())
			.collect();
		assert!(text.contains(expected) && !text.contains(secret));
	}
}
