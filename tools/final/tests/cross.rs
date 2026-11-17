//! The cross launcher (ADR 0294): its engine pin checks and its launches,
//! against witnesses taken from the published Python at 152b0c07, and the
//! real `rnx-final stage cross` sequence over planted programs.
use rnx_final::child::Raised;
use rnx_final::cross::{self, Host, Launched};
use rnx_final::failure::Environment;
use serde_json::{Map, Value, json};
use std::fs::File;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

fn witness(name: &str) -> Value {
	let path = root().join("tools/final/tests/witness").join(name);
	let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
	assert_eq!(value["witness"], "152b0c07861f3c76695091ab6f7ea50fd44b3461");
	value
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-cross-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

/// `value` with each real path replaced by its placeholder, longest first,
/// as the witness generator wrote them.
fn placeholders(value: &Value, marks: &[(&Path, &str)]) -> Value {
	let mut text = serde_json::to_string(value).unwrap();
	let mut marks = marks.to_vec();
	marks.sort_by_key(|(p, _)| std::cmp::Reverse(p.as_os_str().len()));
	for (real, mark) in marks {
		let escaped = serde_json::to_string(&real.to_string_lossy()).unwrap();
		text = text.replace(&escaped[1..escaped.len() - 1], mark);
	}
	serde_json::from_str(&text).unwrap()
}

/// A check's outcome, as the witness records Python's.
fn outcome<T: serde::Serialize>(result: anyhow::Result<T>) -> Value {
	match result {
		Ok(v) => json!({"ok": v}),
		Err(e) => match e.downcast_ref::<Raised>() {
			Some(r) => json!({"error": r.class, "message": r.message}),
			None => json!({"error": "unexpected", "message": format!("{e:#}")}),
		},
	}
}

fn write(path: &Path, text: &str) {
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(path, text).unwrap();
}

const ENTRY: &str = "[[package]]\nname=\"rune\"\nversion=\"0.15.0\"\n";

fn engine_tree(base: &Path, name: &str) -> PathBuf {
	let root = base.join(name);
	write(
		&root.join("rune/crates/rune/Cargo.toml"),
		"[package]\nname=\"rune\"\nversion=\"0.15.0\"\n",
	);
	write(
		&root.join("rune/crates/rune-core/Cargo.toml"),
		"[package]\nname=\"rune-core\"\nversion=\"0.15.0\"\n",
	);
	root
}

fn lock_tree(base: &Path, name: &str, extra: &[(&str, &str)]) -> PathBuf {
	let root = base.join(name);
	for rel in ["Cargo.lock", "tools/project/Cargo.lock"] {
		write(&root.join(rel), ENTRY);
	}
	std::fs::create_dir_all(root.join("rune")).unwrap();
	for (rel, text) in extra {
		write(&root.join(rel), text);
	}
	root
}

/// Every pin check over the planted trees the witness describes.
#[test]
fn the_pin_checks_answer_as_the_published_python() {
	let s = scratch();
	let base = &s.0;
	let mut pins = Map::new();
	let manifests = [
		(
			"canonical path",
			"[dependencies]\nrune={path=\"rune/crates/rune\"}\n",
		),
		(
			"renamed package",
			"[dependencies]\nengine={package=\"rune-core\", path=\"rune/crates/rune-core\"}\n",
		),
		("no engine dependency", "[dependencies]\nserde=\"1\"\n"),
		(
			"git source",
			"[dependencies]\nrune={git=\"https://example.invalid/rune\"}\n",
		),
		(
			"registry source",
			"[dependencies]\nrune={version=\"0.15\", registry=\"other\"}\n",
		),
		(
			"registry-index source",
			"[dependencies]\nrune={version=\"0.15\", registry-index=\"https://example.invalid\"}\n",
		),
		("version string only", "[dependencies]\nrune=\"0.15\"\n"),
		(
			"workspace inherited",
			"[dependencies]\nrune={workspace=true}\n",
		),
		("missing path", "[dependencies]\nrune={version=\"0.15\"}\n"),
		("path escape", "[dependencies]\nrune={path=\"other\"}\n"),
		(
			"dev dependency canonical",
			"[dev-dependencies]\nrune={path=\"rune/crates/rune\"}\n",
		),
		(
			"build dependency git",
			"[build-dependencies]\nrune={git=\"https://example.invalid/rune\"}\n",
		),
		(
			"target table git",
			"[target.\"cfg(unix)\".dependencies]\nrune={git=\"https://example.invalid/rune\"}\n",
		),
		(
			"patch table canonical",
			"[patch.crates-io]\nrune={path=\"rune/crates/rune\"}\n",
		),
		(
			"patch table git",
			"[patch.crates-io]\nrune={git=\"https://example.invalid/rune\"}\n",
		),
		(
			"workspace dependencies git",
			"[workspace.dependencies]\nrune={git=\"https://example.invalid/rune\"}\n",
		),
		(
			"workspace dependencies canonical",
			"[workspace.dependencies]\nrune={path=\"rune/crates/rune\"}\n",
		),
		(
			"wrong engine crate",
			"[dependencies]\nrune={path=\"rune/crates/rune-core\"}\n",
		),
	];
	for (i, (label, text)) in manifests.iter().enumerate() {
		let root = engine_tree(base, &format!("m-{i}"));
		write(&root.join("other/Cargo.toml"), "[package]\nname=\"rune\"\n");
		write(&root.join("Cargo.toml"), text);
		let got = outcome(cross::check_manifest(&root, &root.join("Cargo.toml")));
		pins.insert(
			format!("manifest: {label}"),
			placeholders(&got, &[(&root, "<ROOT>")]),
		);
	}
	let root = engine_tree(base, "link-escape");
	let outside = base.join("outside");
	write(
		&outside.join("crates/rune/Cargo.toml"),
		"[package]\nname=\"rune\"\n",
	);
	std::fs::remove_dir_all(root.join("rune/crates/rune")).unwrap();
	std::os::unix::fs::symlink(outside.join("crates/rune"), root.join("rune/crates/rune")).unwrap();
	write(
		&root.join("Cargo.toml"),
		"[dependencies]\nrune={path=\"rune/crates/rune\"}\n",
	);
	let got = outcome(cross::check_manifest(&root, &root.join("Cargo.toml")));
	pins.insert(
		"manifest: canonical path escapes through a link".into(),
		placeholders(&got, &[(&root, "<ROOT>"), (&outside, "<OUTSIDE>")]),
	);
	let two = format!("{ENTRY}[[package]]\nname=\"rune-core\"\nversion=\"0.15.0\"\n");
	let locks = [
		("canonical", ENTRY.to_string()),
		(
			"external source",
			format!("{ENTRY}source=\"git+external\"\n"),
		),
		("duplicate", format!("{ENTRY}{ENTRY}")),
		(
			"no engine",
			"[[package]]\nname=\"serde\"\nversion=\"1.0.0\"\n".into(),
		),
		("two engines", two),
		("empty", String::new()),
	];
	for (i, (label, text)) in locks.iter().enumerate() {
		let path = base.join(format!("lock-{i}.lock"));
		write(&path, text);
		pins.insert(
			format!("lock: {label}"),
			placeholders(&outcome(cross::check_lock(&path)), &[(&path, "<LOCK>")]),
		);
	}
	let root = engine_tree(base, "metadata");
	let tool = root.join("rune/tools/rune-tool/Cargo.toml");
	write(&tool, "[package]\nname=\"rune-tool\"\n");
	let escape = base.join("escape");
	write(&escape.join("Cargo.toml"), "[package]\nname=\"rune-x\"\n");
	let rune = root.join("rune/crates/rune/Cargo.toml");
	let core = root.join("rune/crates/rune-core/Cargo.toml");
	let package = |name: &str, source: Value, path: &Path| json!({"name": name, "source": source, "manifest_path": path.to_string_lossy()});
	let metas = [
		("canonical", vec![package("rune", Value::Null, &rune)]),
		(
			"with tool outside crates",
			vec![
				package("rune", Value::Null, &rune),
				package("rune-tool", Value::Null, &tool),
			],
		),
		(
			"non-engine ignored",
			vec![
				package("rune", Value::Null, &rune),
				package(
					"serde",
					json!("registry+x"),
					Path::new("/nowhere/Cargo.toml"),
				),
			],
		),
		("external", vec![package("rune", json!("git+x"), &rune)]),
		(
			"duplicate",
			vec![
				package("rune", Value::Null, &rune),
				package("rune", Value::Null, &rune),
			],
		),
		(
			"escapes the subtree",
			vec![
				package("rune", Value::Null, &rune),
				package("rune-x", Value::Null, &escape.join("Cargo.toml")),
			],
		),
		(
			"crate not canonical",
			vec![package("rune", Value::Null, &core)],
		),
		("no rune", vec![package("rune-core", Value::Null, &core)]),
		("empty graph", vec![]),
	];
	for (label, packages) in metas {
		let got = outcome(cross::check_metadata(&root, &json!({"packages": packages})));
		pins.insert(
			format!("metadata: {label}"),
			placeholders(&got, &[(&root, "<ROOT>"), (&escape, "<ESCAPE>")]),
		);
	}
	let bad = format!("{ENTRY}source=\"git+old\"\n");
	let trees: [(&str, Vec<(&str, &str)>); 5] = [
		("required locks only", vec![]),
		(
			"historical exclusion",
			vec![
				("probes/old/Cargo.lock", &bad),
				("provenance/x/Cargo.lock", &bad),
				("target/x/Cargo.lock", &bad),
				(".git/x/Cargo.lock", &bad),
				("a/__pycache__/Cargo.lock", &bad),
			],
		),
		("new external lock", vec![("adapters/new/Cargo.lock", &bad)]),
		(
			"nested probes not excluded",
			vec![("adapters/probes/Cargo.lock", &bad)],
		),
		(
			"manifest checked",
			vec![(
				"adapters/x/Cargo.toml",
				"[dependencies]\nrune={git=\"https://example.invalid\"}\n",
			)],
		),
	];
	for (i, (label, extra)) in trees.iter().enumerate() {
		let root = lock_tree(base, &format!("t-{i}"), extra);
		let got = outcome(cross::check_tree(&root));
		pins.insert(
			format!("tree: {label}"),
			placeholders(&got, &[(&root, "<ROOT>")]),
		);
	}
	let root = base.join("missing-required");
	write(&root.join("Cargo.lock"), ENTRY);
	std::fs::create_dir_all(root.join("rune")).unwrap();
	pins.insert(
		"tree: missing tools/project lock".into(),
		placeholders(&outcome(cross::check_tree(&root)), &[(&root, "<ROOT>")]),
	);
	let external = base.join("external-adapters");
	write(&external.join("x/Cargo.lock"), &bad);
	let root = lock_tree(base, "linked-dir", &[]);
	std::os::unix::fs::symlink(&external, root.join("linked")).unwrap();
	pins.insert(
		"tree: linked directory not followed".into(),
		placeholders(&outcome(cross::check_tree(&root)), &[(&root, "<ROOT>")]),
	);
	pins.insert(
		"tree: the published checkout".into(),
		outcome(cross::check_tree(&self::root())),
	);
	for (label, request) in [
		("missing rune_path", json!({"rune_rev": null})),
		(
			"null rune_path",
			json!({"rune_rev": null, "rune_path": null}),
		),
	] {
		pins.insert(
			format!("request: {label}"),
			outcome(cross::validate_request(&request)),
		);
	}
	let expected = witness("engine-source.json")["expected"].clone();
	for (label, want) in expected.as_object().unwrap() {
		assert_eq!(pins.get(label), Some(want), "{label}");
	}
	assert_eq!(Value::Object(pins), expected);
}

const SHOWN: [&str; 13] = [
	"CARGO_TARGET_DIR",
	"RNX_VERIFY_RUNE_ROOT",
	"CARGO_BUILD_JOBS",
	"CARGO_INCREMENTAL",
	"CARGO_PROFILE_DEV_DEBUG",
	"CARGO_PROFILE_TEST_DEBUG",
	"CARGO_PROFILE_DEV_STRIP",
	"CARGO_PROFILE_TEST_STRIP",
	"RUSTFLAGS",
	"CARGO_ENCODED_RUSTFLAGS",
	"PATH",
	"HOME",
	"MAKEFLAGS",
];

/// The witness generator's interception, answering as it did.
struct Planted {
	scenario: String,
	calls: Vec<Value>,
}

impl Host for Planted {
	fn which(&self, name: &str, _: &Environment) -> Option<String> {
		match name {
			"bwrap" if self.scenario != "no-bwrap" => Some("/usr/bin/bwrap".into()),
			"cargo" if self.scenario != "no-cargo" => Some("/planted/bin/cargo".into()),
			_ => None,
		}
	}
	fn cpus(&self) -> anyhow::Result<usize> {
		Ok(8)
	}
	fn jobs(&self, env: &Environment) -> anyhow::Result<usize> {
		rnx_final::stage::build_jobs(env)
	}
	fn launcher(&self) -> anyhow::Result<Value> {
		Ok(json!({"path": "/planted/rnx-final", "sha256": "1".repeat(64)}))
	}
	fn output(&mut self, command: &[String], _: &Environment) -> anyhow::Result<String> {
		self.calls
			.push(json!({"kind": "check_output", "command": command}));
		Ok("bubblewrap 0.11.0\n".into())
	}
	fn status(&mut self, command: &[String], _: &Environment) -> anyhow::Result<i32> {
		self.calls.push(json!({"kind": "run", "command": command}));
		Ok(i32::from(self.scenario == "not-ignored"))
	}
	fn run(
		&mut self,
		command: &[String],
		env: &Environment,
		stdout: (&Path, File),
		stderr: (&Path, File),
	) -> anyhow::Result<i32> {
		let shown: Map<String, Value> = SHOWN
			.iter()
			.map(|k| (k.to_string(), env.get(*k).map_or(Value::Null, |v| json!(v))))
			.collect();
		self.calls.push(
			json!({"kind": "run_process", "command": command, "check": true,
			"stdout": stdout.0.file_name().unwrap().to_string_lossy(),
			"stderr": stderr.0.file_name().unwrap().to_string_lossy(),
			"env": shown, "env_keys": env.keys().collect::<Vec<_>>()}),
		);
		let mut out = stdout.1;
		let s = self.scenario.as_str();
		let has = |a: &str| command.iter().any(|c| c == a);
		if has("metadata") {
			let manifest = root().join("rune/crates/rune/Cargo.toml");
			std::io::Write::write_all(
				&mut out,
				json!({"packages": [{"name": "rune", "source": null, "manifest_path": manifest}]})
					.to_string()
					.as_bytes(),
			)?;
		} else if has("--no-run") {
			let exe = Path::new(&env["CARGO_TARGET_DIR"]).join("debug/deps/rnx-planted");
			write(&exe, "planted");
			let row = json!({"reason": "compiler-artifact", "target": {"name": "rnx", "kind": ["test"]}, "executable": exe});
			std::io::Write::write_all(&mut out, format!("{row}\n").as_bytes())?;
		} else if has("--controller") {
			let dir = Path::new(command.last().unwrap()).parent().unwrap();
			if !matches!(s, "controller-fails-no-child" | "missing-child") {
				write(&dir.join("bound-verification-child"), "bound");
			}
			if s == "census-failure" {
				let live = dir.join("census/live-execution");
				for engine in ["stock", "fresh"] {
					write(
						&live.join("0002").join(engine).join("stderr"),
						&format!("{engine} stderr\n"),
					);
					write(
						&live.join("0002").join(engine).join("report/result.json"),
						"{}\n",
					);
				}
				write(
					&live.join("RESULT.json"),
					&json!({"differences": [{"id": "0002", "reason": "live observations differ"}]})
						.to_string(),
				);
			}
			if matches!(
				s,
				"controller-fails-bound-child"
					| "controller-fails-no-child"
					| "both-fail" | "census-failure"
			) {
				return Ok(1);
			}
			write(
				&dir.join("RESULT.json"),
				&json!({"status": "PASS", "planted": true}).to_string(),
			);
		} else if has("--host-check") && matches!(s, "both-fail" | "host-check-fails") {
			return Ok(1);
		}
		Ok(0)
	}
}

fn environment(home: &Path, extra: &Value) -> Environment {
	let mut env: Environment = [
		("PATH", "/planted/bin".to_string()),
		("HOME", home.to_string_lossy().into_owned()),
		("MAKEFLAGS", "-j7 --jobserver-auth=fifo:/nonexistent".into()),
		("CARGO_ENCODED_RUSTFLAGS", "-Cplanted".into()),
	]
	.into_iter()
	.map(|(k, v)| (k.to_string(), v))
	.collect();
	for (k, v) in extra.as_object().unwrap() {
		env.insert(k.clone(), v.as_str().unwrap().into());
	}
	env
}

/// Each refusal names what Python's argparse error named, before any
/// output; inside a final, the unassigned output is refused before even
/// the `bwrap --version` query (the reviewed successor: no call where
/// Python made one).
#[test]
fn the_launcher_refuses_as_the_published_python() {
	let s = scratch();
	let home = s.0.join("home");
	std::fs::create_dir_all(home.join(".cargo")).unwrap();
	let root = root();
	let marks = [
		(s.0.as_path(), "<TMP>"),
		(root.as_path(), "<ROOT>"),
		(home.as_path(), "<HOME>"),
	];
	let expected = witness("cross-launcher.json")["expected"]["refusals"].clone();
	for (label, want) in expected.as_object().unwrap() {
		let argv: Vec<String> = want["argv"]
			.as_array()
			.unwrap()
			.iter()
			.map(|a| {
				a.as_str()
					.unwrap()
					.replace("<TMP>", &s.0.to_string_lossy())
					.replace("<ROOT>", &root.to_string_lossy())
			})
			.collect();
		let scenario = if ["no-bwrap", "no-cargo", "not-ignored"].contains(&label.as_str()) {
			label.clone()
		} else {
			"refusal".into()
		};
		let mut host = Planted {
			scenario,
			calls: vec![],
		};
		let env = environment(&home, &want["env"]);
		let launched = cross::launch(&argv, &root, &root, &env, &mut host).unwrap();
		let Launched::Refused(refusal) = launched else {
			panic!("{label}: not refused: {launched:?}");
		};
		let error = want["error"].as_str().unwrap();
		assert_eq!(
			format!(
				"verify.py: error: {}",
				placeholders(&json!(refusal), &marks).as_str().unwrap()
			),
			error,
			"{label}"
		);
		assert_eq!(want["exit"], 2, "{label}");
		let calls = placeholders(&json!(host.calls), &marks);
		if label.starts_with("final ") {
			assert_eq!(want["calls"].as_array().unwrap().len(), 1, "{label}");
			assert_eq!(calls, json!([]), "{label}");
		} else {
			assert_eq!(calls, want["calls"], "{label}");
		}
		assert!(
			!s.0.join(
				argv[1]
					.trim_start_matches(&*s.0.to_string_lossy())
					.trim_start_matches('/')
			)
			.exists() || !argv[1].starts_with(&*s.0.to_string_lossy()),
			"{label}: a refusal created its output"
		);
	}
}

/// Each run's calls, environment, request, receipt and files are Python's;
/// the request also names the launcher, which Python's did not.
#[test]
fn the_launcher_runs_as_the_published_python() {
	let root = root();
	let expected = witness("cross-launcher.json")["expected"]["runs"].clone();
	for (scenario, want) in expected.as_object().unwrap() {
		let s = scratch();
		let home = s.0.join("home");
		std::fs::create_dir_all(home.join(".cargo")).unwrap();
		let out = s.0.join(format!("run-{scenario}"));
		let mut host = Planted {
			scenario: scenario.clone(),
			calls: vec![],
		};
		let env = environment(&home, &json!({}));
		let argv: Vec<String> = ["--out", &out.to_string_lossy(), "--suite", "all"]
			.map(String::from)
			.into();
		let launched = cross::launch(&argv, &root, &root, &env, &mut host).unwrap();
		let Launched::Ran {
			output,
			passed,
			failure,
		} = launched
		else {
			panic!("{scenario}: refused: {launched:?}");
		};
		assert_eq!(output, out);
		assert_eq!(passed, scenario == "pass", "{scenario}");
		let marks = [
			(out.as_path(), "<OUT>"),
			(root.as_path(), "<ROOT>"),
			(home.as_path(), "<HOME>"),
			(s.0.as_path(), "<TMP>"),
		];
		assert_eq!(
			placeholders(&json!(host.calls), &marks),
			want["calls"],
			"{scenario}"
		);
		let raised = failure.as_ref().map(|f| f.repr.clone());
		assert_eq!(
			placeholders(&json!(raised), &marks),
			want["raised"],
			"{scenario}"
		);
		let mut receipt: Value =
			serde_json::from_slice(&std::fs::read(out.join("RESULT.json")).unwrap()).unwrap();
		let cleanup = receipt.as_object_mut().unwrap().remove("cleanup").unwrap();
		let mut retained: Vec<String> = cleanup["retained"]
			.as_object()
			.unwrap()
			.keys()
			.map(|p| {
				Path::new(p)
					.file_name()
					.unwrap()
					.to_string_lossy()
					.into_owned()
			})
			.collect();
		retained.sort();
		receipt["cleanup"] = json!({"status": cleanup["status"], "removed": cleanup["removed"], "retained": retained});
		assert_eq!(
			placeholders(&receipt, &marks),
			want["receipt"],
			"{scenario}"
		);
		let mut request: Value =
			serde_json::from_slice(&std::fs::read(out.join("REQUEST.json")).unwrap()).unwrap();
		let launcher = request.as_object_mut().unwrap().remove("launcher");
		assert_eq!(
			launcher,
			Some(json!({"path": "/planted/rnx-final", "sha256": "1".repeat(64)}))
		);
		assert_eq!(
			placeholders(&request, &marks),
			want["request"],
			"{scenario}"
		);
		let pins: Value =
			serde_json::from_slice(&std::fs::read(out.join("PIN-CONSISTENCY.json")).unwrap())
				.unwrap();
		assert_eq!(pins, want["pin_consistency"], "{scenario}");
		let mut files = Vec::new();
		let mut stack = vec![out.clone()];
		while let Some(dir) = stack.pop() {
			for entry in std::fs::read_dir(dir).unwrap() {
				let path = entry.unwrap().path();
				let relative = path.strip_prefix(&out).unwrap().to_path_buf();
				if relative.components().next()
					== Some(std::path::Component::Normal("target".as_ref()))
				{
					continue;
				}
				if path.is_dir() {
					stack.push(path);
				} else {
					files.push(relative.to_string_lossy().into_owned());
				}
			}
		}
		files.sort();
		assert_eq!(json!(files), want["files"], "{scenario}");
		assert!(!out.join("target").exists(), "{scenario}: target kept");
	}
}

/// A `--out` whose `..` follows a missing component and then a link is
/// checked where it really lands: inside the Rune tree or the checkout's
/// unignored target, it is refused, and nothing is created on either side.
#[test]
fn a_linked_out_path_is_refused_where_it_really_lands() {
	let s = scratch();
	let root = lock_tree(&s.0, "planted-root", &[]);
	std::fs::create_dir_all(root.join("target")).unwrap();
	std::fs::create_dir_all(root.join("tools/verify/fixtures")).unwrap();
	// The engine outside the checkout, so its own boundary is the one met.
	let engine = s.0.join("engine");
	std::fs::create_dir(&engine).unwrap();
	std::fs::remove_dir(root.join("rune")).unwrap();
	std::os::unix::fs::symlink(&engine, root.join("rune")).unwrap();
	let home = s.0.join("home");
	std::fs::create_dir_all(home.join(".cargo")).unwrap();
	for (alias, into, refusal) in [
		(
			"into-rune",
			engine.clone(),
			"--out must be outside the Rune source tree",
		),
		(
			"into-target",
			root.join("target"),
			"in-tree output must be under target/rnx-verify",
		),
	] {
		std::os::unix::fs::symlink(&into, s.0.join(alias)).unwrap();
		let out = s.0.join(format!("missing/../{alias}/new"));
		let mut host = Planted {
			scenario: "pass".into(),
			calls: vec![],
		};
		let argv: Vec<String> = ["--out", &out.to_string_lossy()].map(String::from).into();
		let launched = cross::launch(
			&argv,
			&root,
			&root,
			&environment(&home, &json!({})),
			&mut host,
		)
		.unwrap();
		match launched {
			Launched::Refused(message) => assert_eq!(message, refusal, "{alias}"),
			other => panic!("{alias}: {other:?}"),
		}
		assert!(!into.join("new").exists(), "{alias}: created inside");
		assert!(
			!s.0.join("missing").exists(),
			"{alias}: created the missing parent"
		);
	}
	assert_eq!(
		cross::resolve(&s.0.join("missing/../into-rune/new")).unwrap(),
		engine.join("new")
	);
	assert_eq!(
		cross::resolve(&s.0.join("fresh/out")).unwrap(),
		s.0.join("fresh/out")
	);
}

/// The planted programs the real stage runs: `bwrap` runs its command,
/// `cargo` answers metadata and builds the verifier script, and the
/// verifier behaves as `VERIFIER_MODE` says.
struct Stage {
	s: Scratch,
	exe: PathBuf,
	out: PathBuf,
	log: PathBuf,
}

impl Stage {
	fn new() -> Self {
		let s = scratch();
		let bin = s.0.join("bin");
		for (name, text) in [
			(
				"bin/bwrap",
				"#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'bubblewrap planted'; exit 0; fi\nwhile [ \"$1\" != --chdir ]; do shift; done\nshift; cd \"$1\" || exit 9; shift; exec \"$@\"\n",
			),
			(
				"bin/cargo",
				"#!/bin/sh\ncase \"$2\" in\nmetadata) printf '{\"packages\":[{\"name\":\"rune\",\"source\":null,\"manifest_path\":\"%s/rune/crates/rune/Cargo.toml\"}]}\\n' \"$PWD\" ;;\ntest) mkdir -p \"$CARGO_TARGET_DIR/debug/deps\" && cp \"$PLANTED_VERIFIER\" \"$CARGO_TARGET_DIR/debug/deps/rnx-verifier\" && printf '{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"rnx\",\"kind\":[\"test\"]},\"executable\":\"%s\"}\\n' \"$CARGO_TARGET_DIR/debug/deps/rnx-verifier\" ;;\n*) exit 7 ;;\nesac\n",
			),
			(
				"verifier",
				"#!/bin/sh\nout=$(dirname \"$2\")\ncase \"$1\" in\n--controller)\n  echo controller >> \"$SEQUENCE_LOG\"\n  if [ \"$VERIFIER_MODE\" = hang ]; then\n    sleep 300 &\n    echo $! > \"$SEQUENCE_LOG.sleeper\"\n    echo ready > \"$READY_FIFO\"\n    wait\n    exit 0\n  fi\n  cp \"$0\" \"$out/bound-verification-child\"\n  if [ \"$VERIFIER_MODE\" = silent ]; then exit 0; fi\n  if [ \"$VERIFIER_MODE\" = malformed ]; then printf 'not json' > \"$out/RESULT.json\"; exit 0; fi\n  if [ \"$VERIFIER_MODE\" = directory ]; then mkdir \"$out/RESULT.json\"; exit 0; fi\n  if [ \"$VERIFIER_MODE\" = symlink ]; then ln -s \"$EXTERNAL_RESULT\" \"$out/RESULT.json\"; exit 0; fi\n  if [ \"$VERIFIER_MODE\" = dangling ]; then ln -s \"$EXTERNAL_RESULT.missing\" \"$out/RESULT.json\"; exit 0; fi\n  sleep 300 &\n  echo $! > \"$SEQUENCE_LOG.sleeper\"\n  printf '{\"status\":\"FAIL\",\"planted\":true}\\n' > \"$out/RESULT.json\"\n  exit 1 ;;\n--host-check)\n  echo host-check >> \"$SEQUENCE_LOG\"\n  if [ -f \"$out/RESULT.json\" ]; then echo result-unfrozen >> \"$SEQUENCE_LOG\"; fi\n  exit 0 ;;\nesac\nexit 8\n",
			),
		] {
			let path = s.0.join(name);
			write(&path, text);
			std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
				.unwrap();
		}
		std::fs::create_dir(s.0.join("cargo-home")).unwrap();
		let _ = bin;
		Self {
			exe: Path::new(env!("CARGO_BIN_EXE_rnx-final"))
				.canonicalize()
				.unwrap(),
			out: s.0.join("payload"),
			log: s.0.join("sequence"),
			s,
		}
	}

	fn command(&self, mode: &str, args: &[&str]) -> std::process::Command {
		let mut command = std::process::Command::new(&self.exe);
		command
			.arg("stage")
			.arg("cross")
			.args(args)
			.env_clear()
			.env(
				"PATH",
				format!("{}:/usr/bin:/bin", self.s.0.join("bin").display()),
			)
			.env("HOME", &self.s.0)
			.env("CARGO_HOME", self.s.0.join("cargo-home"))
			.env("SEQUENCE_LOG", &self.log)
			.env("PLANTED_VERIFIER", self.s.0.join("verifier"))
			.env("READY_FIFO", self.s.0.join("ready"))
			.env("EXTERNAL_RESULT", self.s.0.join("external.json"))
			.env("VERIFIER_MODE", mode);
		command
	}

	fn run(&self, mode: &str) -> std::process::Output {
		self.command(
			mode,
			&[
				"--suite",
				"all",
				"--jobs",
				"1",
				"--out",
				&self.out.to_string_lossy(),
			],
		)
		.output()
		.unwrap()
	}

	fn receipt(&self) -> Value {
		serde_json::from_slice(&std::fs::read(self.out.join("RESULT.json")).unwrap()).unwrap()
	}

	fn sequence(&self) -> String {
		std::fs::read_to_string(&self.log).unwrap()
	}

	/// The verifier's background grandchild is gone.
	fn drained(&self) -> bool {
		let pid: i32 = std::fs::read_to_string(self.s.0.join("sequence.sleeper"))
			.unwrap()
			.trim()
			.parse()
			.unwrap();
		!std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
			.is_ok_and(|c| c.starts_with("sleep"))
	}

	/// Released, with its evidence and a FAIL receipt kept.
	fn released(&self, receipt: &Value) {
		assert_eq!(receipt["status"], "FAIL", "{receipt}");
		assert_eq!(receipt["cleanup"]["status"], "PASS", "{receipt}");
		assert_eq!(receipt["cleanup"]["removed"], true, "{receipt}");
		assert!(!self.out.join("target").exists());
	}
}

/// The real stage: the verifier fails after binding its child and leaving
/// a grandchild; the host check still runs, after the controller and
/// before its result is frozen; the result survives into the receipt; the
/// grandchild is drained; the private target is released; the request
/// names this launcher. A bad option is refused with argparse's exit 2.
#[test]
fn the_real_stage_runs_its_sequence_through_a_controller_failure() {
	let stage = Stage::new();
	let refused = stage
		.command("fail", &["--suite", "nightly"])
		.output()
		.unwrap();
	assert_eq!(refused.status.code(), Some(2));
	assert!(
		String::from_utf8_lossy(&refused.stderr).contains("argument --suite: invalid choice"),
		"{refused:?}"
	);
	let ran = stage.run("fail");
	assert_eq!(ran.status.code(), Some(1), "{ran:?}");
	assert!(String::from_utf8_lossy(&ran.stdout).starts_with("FAIL "));
	assert_eq!(
		stage.sequence(),
		"controller\nhost-check\nresult-unfrozen\n"
	);
	let receipt = stage.receipt();
	stage.released(&receipt);
	assert_eq!(receipt["planted"], true, "the controller's result survives");
	assert_eq!(
		receipt["launcher_error"],
		format!(
			"CalledProcessError(1, ['{}', '--controller', '{}'])",
			stage.out.join("target/debug/deps/rnx-verifier").display(),
			stage.out.join("REQUEST.json").display()
		)
	);
	assert_eq!(receipt["launcher_notes"], json!([]));
	assert!(stage.out.join("CONTROLLER-RESULT.json").is_file());
	assert!(
		stage.drained(),
		"the controller's grandchild was not drained"
	);
	let request: Value =
		serde_json::from_slice(&std::fs::read(stage.out.join("REQUEST.json")).unwrap()).unwrap();
	assert_eq!(
		request["launcher"]["path"],
		json!(stage.exe.to_string_lossy())
	);
	assert_eq!(
		request["launcher"]["sha256"],
		json!(rnx_final::receipt::digest(&stage.exe).unwrap())
	);
}

/// A controller that binds its child and exits 0 without a PASS result is
/// a failure: a FAIL receipt and a nonzero exit, whatever result it left
/// kept as evidence.
#[test]
fn a_controller_without_a_pass_result_fails_the_stage() {
	let silent = Stage::new();
	let ran = silent.run("silent");
	assert_eq!(ran.status.code(), Some(1), "{ran:?}");
	assert!(String::from_utf8_lossy(&ran.stdout).starts_with("FAIL "));
	assert_eq!(silent.sequence(), "controller\nhost-check\n");
	let receipt = silent.receipt();
	silent.released(&receipt);
	assert_eq!(
		receipt["launcher_error"],
		"RuntimeError('the Rust controller wrote no result; see controller.stderr')"
	);
	assert!(!silent.out.join("CONTROLLER-RESULT.json").exists());
	let malformed = Stage::new();
	let ran = malformed.run("malformed");
	assert_eq!(ran.status.code(), Some(1), "{ran:?}");
	let receipt = malformed.receipt();
	malformed.released(&receipt);
	assert_eq!(
		receipt["launcher_error"],
		"RuntimeError(\"the Rust controller's result is not a PASS; see CONTROLLER-RESULT.json\")"
	);
	assert!(receipt["controller_result_error"].is_string(), "{receipt}");
	assert_eq!(
		std::fs::read_to_string(malformed.out.join("CONTROLLER-RESULT.json")).unwrap(),
		"not json"
	);
	// A directory, a link out of the payload (to a planted PASS), and a
	// dangling link: none is read, each is frozen as it stands.
	for mode in ["directory", "symlink", "dangling"] {
		let stage = Stage::new();
		std::fs::write(
			stage.s.0.join("external.json"),
			r#"{"status": "PASS", "external": true}"#,
		)
		.unwrap();
		let ran = stage.run(mode);
		assert_eq!(ran.status.code(), Some(1), "{mode}: {ran:?}");
		assert!(
			String::from_utf8_lossy(&ran.stdout).starts_with("FAIL "),
			"{mode}"
		);
		let receipt = stage.receipt();
		stage.released(&receipt);
		assert_eq!(
			receipt["launcher_error"],
			"RuntimeError(\"the Rust controller's result is not a regular file; see CONTROLLER-RESULT.json\")",
			"{mode}"
		);
		assert_eq!(
			receipt["controller_result_error"], "not a regular file",
			"{mode}"
		);
		assert!(receipt.get("external").is_none(), "{mode}: {receipt}");
		let frozen = stage
			.out
			.join("CONTROLLER-RESULT.json")
			.symlink_metadata()
			.unwrap();
		assert_eq!(
			(frozen.is_dir(), frozen.file_type().is_symlink()),
			(mode == "directory", mode != "directory"),
			"{mode}"
		);
		assert!(!stage.out.join("RESULT.json.missing").exists());
	}
}

/// SIGTERM while the verifier runs, once it says it is ready: its group is
/// drained, the private target released, and a FAIL receipt published
/// naming the signal, as the Python launcher's supervised wait did.
#[test]
fn an_interrupted_standalone_stage_drains_releases_and_fails() {
	let stage = Stage::new();
	let fifo = stage.s.0.join("ready");
	let c = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
	// SAFETY: mkfifo on a NUL-terminated path in the scratch directory.
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
	let mut child = stage
		.command(
			"hang",
			&[
				"--suite",
				"all",
				"--jobs",
				"1",
				"--out",
				&stage.out.to_string_lossy(),
			],
		)
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.unwrap();
	let (sender, receiver) = std::sync::mpsc::channel();
	let reader = fifo.clone();
	std::thread::spawn(move || {
		let _ = sender.send(std::fs::read_to_string(reader));
	});
	let ready = receiver.recv_timeout(std::time::Duration::from_secs(120));
	if !matches!(&ready, Ok(Ok(line)) if line == "ready\n") {
		let _ = child.kill();
		panic!(
			"the verifier never became ready: {ready:?} {:?}",
			child.wait_with_output()
		);
	}
	// SAFETY: signalling the child this test started.
	assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
	let ran = child.wait_with_output().unwrap();
	assert_eq!(ran.status.code(), Some(1), "{ran:?}");
	let receipt = stage.receipt();
	stage.released(&receipt);
	assert_eq!(
		receipt["launcher_error"],
		"RuntimeError('producer interrupted by signal 15')"
	);
	assert!(
		stage.drained(),
		"the interrupted verifier's group was not drained"
	);
}
