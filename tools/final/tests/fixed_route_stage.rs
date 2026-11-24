//! The fixed-route stage (ADR 0298) against witnesses from the published
//! Python gate at 7991e30d: the recipe's identities and the gate's whole
//! sequence over a planted host; and the Valgrind root's precedence.
use rnx_final::failure::Environment;
use rnx_final::fixed_route_stage::{self as frs, Host};
use rnx_final::receipt::{atomic_json, digest};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

// ADRs 0316 and 0324 update the capture-script hashes and their derived keys
// under the original schema-1 recipe in both witnesses.
fn witness(name: &str) -> Value {
	let path = root().join("tools/final/tests/witness").join(name);
	let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
	assert_eq!(value["witness"], "7991e30de16863b07289a8423678e8831c7687f4");
	value["expected"].clone()
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-fixed-stage-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

fn outcome<T: serde::Serialize>(result: anyhow::Result<T>) -> Value {
	match result {
		Ok(v) => json!({"ok": v}),
		Err(e) => match e.downcast_ref::<rnx_final::child::Raised>() {
			Some(r) => json!({"error": r.class.rsplit('.').next().unwrap(), "message": r.message}),
			None => json!({"error": "unexpected", "message": format!("{e:#}")}),
		},
	}
}

fn env(pairs: &[(&str, &str)]) -> Environment {
	pairs
		.iter()
		.map(|(k, v)| (k.to_string(), v.to_string()))
		.collect()
}

fn mark(value: &Value, marks: &[(&str, &str)]) -> Value {
	let mut text = serde_json::to_string(value).unwrap();
	for (real, mark) in marks {
		let escaped = serde_json::to_string(real).unwrap();
		text = text.replace(&escaped[1..escaped.len() - 1], mark);
	}
	serde_json::from_str(&text).unwrap()
}

/// The planted host of the recipe witness: rustup and its toolchain under
/// `tmp`, a planted CPU, nothing else.
struct Recipe {
	tmp: PathBuf,
	cpu: String,
	rustup: bool,
}

impl Host for Recipe {
	fn record(&mut self, _: &Path) -> anyhow::Result<Value> {
		unreachable!()
	}
	fn status(&mut self, _: &Path) -> anyhow::Result<Vec<u8>> {
		unreachable!()
	}
	fn source(&mut self, _: &Path) -> anyhow::Result<String> {
		unreachable!()
	}
	fn head(&mut self, _: &Path) -> Option<String> {
		unreachable!()
	}
	fn approval(&mut self, _: &Path, _: &str, _: &str, _: &Value) -> anyhow::Result<()> {
		unreachable!()
	}
	fn condition(
		&mut self,
		_: &Path,
		_: &str,
		_: &str,
		_: &Value,
		_: &Path,
	) -> anyhow::Result<Value> {
		unreachable!()
	}
	fn run(&mut self, _: &[String], _: Option<&Path>, _: File) -> anyhow::Result<i32> {
		unreachable!()
	}
	fn child(&mut self, _: &[String], _: &Path, _: File, _: Duration) -> anyhow::Result<i32> {
		unreachable!()
	}
	fn output(&mut self, command: &[String], _: Option<&Path>) -> anyhow::Result<String> {
		assert_eq!(&command[1..4], ["which", "--toolchain", "1.98.1"]);
		// build_environment's absolute rustup gets fixed strings; the
		// recipe's own rustup gets the real files it digests.
		let dir = if command[0].starts_with('/') {
			PathBuf::from("/planted/toolchain/bin")
		} else {
			self.tmp.join("toolchain/bin")
		};
		Ok(format!("{}\n", dir.join(&command[4]).display()))
	}
	fn cpuinfo(&mut self) -> anyhow::Result<String> {
		Ok(self.cpu.clone())
	}
	fn which(&self, name: &str, _: &Environment) -> Option<PathBuf> {
		(name == "rustup" && self.rustup).then(|| PathBuf::from("/planted/rustup/bin/rustup"))
	}
}

#[test]
fn the_recipe_identities_answer_as_the_published_python() {
	let s = scratch();
	let tmp = &s.0;
	std::fs::create_dir_all(tmp.join("toolchain/bin")).unwrap();
	for (name, text) in [("rustc", "planted rustc"), ("cargo", "planted cargo")] {
		std::fs::write(tmp.join("toolchain/bin").join(name), text).unwrap();
	}
	std::fs::create_dir_all(tmp.join("rustup/bin")).unwrap();
	std::fs::write(tmp.join("rustup/bin/rustup"), "rustup").unwrap();
	let cpu = "processor\t: 0\nvendor_id\t: GenuineIntel\ncpu family\t: 6\nmodel name\t: Planted CPU\nflags\t\t: fpu vme sse2\n\nprocessor\t: 1\nmodel name\t: second\n";
	let mut host = Recipe {
		tmp: tmp.clone(),
		cpu: cpu.into(),
		rustup: true,
	};
	let expected = witness("fixed-route-recipe.json");
	let mut cases: Vec<(String, Value)> = Vec::new();
	let base = [
		("PATH", "/tools"),
		("HOME", "/tmp"),
		("RUSTFLAGS", "-Copt-level=3"),
	];
	let with = |extra: &[(&str, &str)]| {
		let mut e = env(&base);
		for (k, v) in extra {
			e.insert(k.to_string(), v.to_string());
		}
		e
	};
	assert!(!frs::capture_environment().contains_key("TMPDIR"));
	let every: Vec<(String, String)> = frs::BUILD_KEYS
		.iter()
		.map(|k| (k.to_string(), format!("v-{k}")))
		.collect();
	let environments: Vec<(&str, Environment)> = vec![
		("base", with(&[])),
		(
			"unrelated variables",
			with(&[
				("SSH_AUTH_SOCK", "different"),
				("TERM", "different"),
				("PWD", "different"),
			]),
		),
		("another RUSTFLAGS", with(&[("RUSTFLAGS", "-Copt-level=2")])),
		(
			"an unknown override",
			with(&[("RUST_SOMETHING", "unknown")]),
		),
		("an unknown CC_ override", with(&[("CC_x86_64", "gcc")])),
		(
			"a PKG_CONFIG_ override",
			with(&[("PKG_CONFIG_ALLOW_CROSS", "1")]),
		),
		(
			"locales and path one",
			env(&[("PATH", "/one"), ("LANG", "x"), ("LC_CTYPE", "a")]),
		),
		(
			"locales and path two",
			env(&[("PATH", "/two"), ("LANG", "y"), ("LC_ALL", "b")]),
		),
		(
			"cargo variables kept, target dropped",
			with(&[
				("CARGO_HOME", "/h"),
				("CARGO_TARGET_DIR", "/t"),
				("CARGO_BUILD_JOBS", "9"),
				("CARGO_PROFILE_RELEASE_LTO", "fat"),
			]),
		),
		("every declared key", every.into_iter().collect()),
	];
	let marks = [(tmp.to_str().unwrap(), "<TMP>")];
	for (label, e) in &environments {
		cases.push((
			format!("build_environment: {label}"),
			mark(&outcome(frs::build_environment(e, &mut host)), &marks),
		));
	}
	host.rustup = false;
	cases.push((
		"build_environment: no rustup".into(),
		outcome(frs::build_environment(&with(&[]), &mut host)),
	));
	host.rustup = true;
	let mut keys: Vec<&str> = frs::BUILD_KEYS.to_vec();
	keys.sort();
	cases.push((
		"constants".into(),
		json!({"ok": {"BUILD_KEYS": keys, "BUILD_PREFIXES": frs::BUILD_PREFIXES, "capture_environment": frs::capture_environment()}}),
	));
	let policy = legacy_policy();
	let recipe_env = env(&[
		("PATH", "/tools"),
		("HOME", "/planted/home"),
		("RUSTFLAGS", "-Copt-level=3"),
		("CARGO_HOME", "/cargo"),
	]);
	let mut recipe = frs::recipe(&root(), &policy, &recipe_env, &mut host).unwrap();
	cases.push((
		"recipe: planted host".into(),
		mark(&json!({"ok": recipe}), &marks),
	));
	recipe["valgrind"] = json!("v".repeat(64));
	let mut private_env = recipe_env.clone();
	private_env.insert("TMPDIR".into(), "/stage/owned/t0".into());
	let mut private = frs::recipe(&root(), &policy, &private_env, &mut host).unwrap();
	private["valgrind"] = recipe["valgrind"].clone();
	assert_ne!(recipe["build_environment"], private["build_environment"]);
	assert_eq!(
		frs::capture_key("binary", &recipe).unwrap(),
		frs::capture_key("binary", &private).unwrap()
	);
	cases.push((
		"capture_key: the planted recipe".into(),
		json!({"ok": frs::capture_key(&"a".repeat(64), &recipe).unwrap()}),
	));
	let r = json!({"cpu": ["cpu"], "environment": {"digest": "x"}, "run_environment": {"PATH": "/bin"}, "valgrind": "v",
		"fixtures": {"f": "a"}, "policy": {"matrix": "x"}, "tooling": {"tools/column/paired_build.py": "old", "capture.py": "same"},
		"build_environment": "old", "toolchain": "old"});
	let key = |sha: &str, v: &Value| json!({"ok": frs::capture_key(sha, v).unwrap()});
	cases.push(("capture_key: base".into(), key(&"a".repeat(64), &r)));
	let mut changed = r.clone();
	changed["build_environment"] = json!("new");
	changed["toolchain"] = json!("new");
	changed["tooling"]["tools/column/paired_build.py"] = json!("new");
	cases.push((
		"capture_key: build-only changes".into(),
		key(&"a".repeat(64), &changed),
	));
	cases.push((
		"capture_key: another binary".into(),
		key(&"b".repeat(64), &r),
	));
	for k in [
		"cpu",
		"environment",
		"run_environment",
		"valgrind",
		"fixtures",
		"policy",
	] {
		let mut c = r.clone();
		c[k] = json!("changed");
		cases.push((
			format!("capture_key: {k} changed"),
			key(&"a".repeat(64), &c),
		));
	}
	let mut c = r.clone();
	c["tooling"]["capture.py"] = json!("new");
	cases.push((
		"capture_key: other tooling changed".into(),
		key(&"a".repeat(64), &c),
	));
	// reuse_anchor and verified over a planted legacy capture.
	let legacy = tmp.join("legacy");
	let profile = legacy.join("case/profile");
	std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
	std::fs::write(&profile, "counts").unwrap();
	let binary = legacy.join("binary");
	std::fs::write(&binary, "code").unwrap();
	let build = json!({"binary": binary, "sha256": digest(&binary).unwrap()});
	let run_env = frs::capture_environment();
	let rec = json!({"cpu": ["cpu"], "environment": rnx_final::child::environment(&run_env).unwrap(), "valgrind": "v",
		"fixtures": {}, "policy": {}, "tooling": {}, "build_environment": "old"});
	let manifest = legacy.join("MANIFEST.json");
	atomic_json(
		&manifest,
		&json!({"environment": run_env, "binaries": {"baseline": build}}),
	)
	.unwrap();
	let receipt = legacy.join("original.json");
	let mut artifacts = serde_json::Map::new();
	artifacts.insert(
		binary.to_string_lossy().into(),
		json!(digest(&binary).unwrap()),
	);
	artifacts.insert(
		profile.to_string_lossy().into(),
		json!(digest(&profile).unwrap()),
	);
	atomic_json(
		&receipt,
		&json!({"status": "PASS", "revision": "anchor", "recipe": rec, "records": [{"profile": profile, "role": "baseline"}], "artifacts": artifacts}),
	)
	.unwrap();
	let mut current = rec.clone();
	current["build_environment"] = json!("new");
	current["run_environment"] = json!(run_env);
	let anchor = json!({"anchor": "anchor"});
	let digests = |v: Value| -> Value {
		let mut text = serde_json::to_string(&mark(&v, &marks)).unwrap();
		for (path, name) in [
			(&manifest, "<MANIFEST-DIGEST>"),
			(&receipt, "<RECEIPT-DIGEST>"),
		] {
			if path.exists() {
				text = text.replace(&digest(path).unwrap(), name);
			}
		}
		serde_json::from_str(&text).unwrap()
	};
	cases.push((
		"reuse_anchor: the planted capture".into(),
		digests(outcome(frs::reuse_anchor(
			&receipt, &build, &current, &anchor,
		))),
	));
	for k in ["cpu", "valgrind", "run_environment"] {
		let mut c = current.clone();
		c[k] = json!("changed");
		cases.push((
			format!("reuse_anchor: {k} changed"),
			digests(outcome(frs::reuse_anchor(&receipt, &build, &c, &anchor))),
		));
	}
	let other = legacy.join("other");
	std::fs::write(&other, "codf").unwrap();
	let other_build = json!({"binary": other, "sha256": digest(&other).unwrap()});
	cases.push((
		"reuse_anchor: another binary".into(),
		digests(outcome(frs::reuse_anchor(
			&receipt,
			&other_build,
			&current,
			&anchor,
		))),
	));
	let wrong = json!({"anchor": "other"});
	cases.push((
		"reuse_anchor: a wrong revision".into(),
		digests(outcome(frs::reuse_anchor(
			&receipt, &build, &current, &wrong,
		))),
	));
	let reused = frs::reuse_anchor(&receipt, &build, &current, &anchor).unwrap();
	cases.push((
		"verified: the reused anchor".into(),
		json!({"ok": frs::verified(reused.clone()).unwrap() == reused}),
	));
	let mut failed = reused.clone();
	failed["status"] = json!("FAIL");
	cases.push(("verified: not PASS".into(), outcome(frs::verified(failed))));
	let mut empty = reused.clone();
	empty["artifacts"] = json!({});
	cases.push((
		"verified: no artifacts".into(),
		outcome(frs::verified(empty)),
	));
	std::fs::write(&manifest, "tampered").unwrap();
	cases.push((
		"verified: a tampered artifact".into(),
		mark(&outcome(frs::verified(reused)), &marks),
	));
	let expected = expected.as_object().unwrap();
	for (label, got) in &cases {
		assert_eq!(got, &expected[label], "{label}");
	}
	assert_eq!(cases.len(), expected.len());
}

// --- The gate's sequence over a planted host. ---

/// What runs between the first pass and the recorded one: extra arguments.
type Before<'a> = &'a dyn Fn(&Path, &Path) -> Vec<String>;

const CPU: &str =
	"processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Planted CPU\nflags\t\t: fpu sse2\n\n";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CAND: &str = "cccccccccccccccccccccccccccccccccccccccc";
const PYTHON: &str = "/planted/python3";

/// The witness's planted answers, and every call in order.
struct Gate {
	tmp: PathBuf,
	root: PathBuf,
	scenario: String,
	calls: Rc<RefCell<Vec<Value>>>,
	sources: u32,
	policy: Value,
}

impl Gate {
	fn note(&self, kind: &str, command: &[String], extra: Value) {
		let mut row = json!({"kind": kind, "command": command});
		for (k, v) in extra.as_object().into_iter().flatten() {
			row[k] = v.clone();
		}
		self.calls.borrow_mut().push(row);
	}
}

fn cwd(p: Option<&Path>) -> Value {
	json!(p.map_or("None".to_string(), |p| p.to_string_lossy().into_owned()))
}

impl Host for Gate {
	fn record(&mut self, _: &Path) -> anyhow::Result<Value> {
		Ok(json!({"baseline": BASE, "candidate": CAND}))
	}
	fn status(&mut self, root: &Path) -> anyhow::Result<Vec<u8>> {
		self.note(
			"git status",
			&["git", "status", "--porcelain"].map(String::from),
			json!({"cwd": root}),
		);
		Ok(if self.scenario == "a dirty tree" {
			b" M dirty.rs\n".to_vec()
		} else {
			vec![]
		})
	}
	fn source(&mut self, _: &Path) -> anyhow::Result<String> {
		self.sources += 1;
		Ok(
			if self.scenario == "the source changes during the gate" && self.sources > 1 {
				"source-2".into()
			} else {
				"source-1".into()
			},
		)
	}
	fn head(&mut self, _: &Path) -> Option<String> {
		Some(CAND.into())
	}
	fn approval(
		&mut self,
		_: &Path,
		baseline: &str,
		candidate: &str,
		_: &Value,
	) -> anyhow::Result<()> {
		self.note("approval", &[baseline.into(), candidate.into()], json!({}));
		if self.scenario == "an approval refusal" {
			anyhow::bail!(rnx_final::child::value_error("approval declaration absent"));
		}
		Ok(())
	}
	fn condition(
		&mut self,
		_: &Path,
		baseline: &str,
		candidate: &str,
		_: &Value,
		guard: &Path,
	) -> anyhow::Result<Value> {
		self.note(
			"source condition",
			&[
				baseline.into(),
				candidate.into(),
				guard.to_string_lossy().into(),
			],
			json!({}),
		);
		Ok(
			json!({"status": "PASS", "baseline": baseline, "candidate": candidate, "inventory": "planted", "items": []}),
		)
	}
	fn run(&mut self, command: &[String], cwd_: Option<&Path>, _: File) -> anyhow::Result<i32> {
		let identity = command.iter().any(|c| c.ends_with("executor_identity.py"));
		self.note(
			"run",
			command,
			json!({"cwd": cwd(cwd_), "check": !identity}),
		);
		if identity {
			let out =
				PathBuf::from(&command[command.iter().position(|c| c == "--out").unwrap() + 1]);
			let mut ident = json!({"status": "FAIL", "reason": "instruction identity not established: planted"});
			if self.scenario == "an identity tool error" {
				ident["error"] = json!("tool failed");
			}
			match self.scenario.as_str() {
				"an identity receipt of []" => ident = json!([]),
				"an identity receipt of null" => ident = Value::Null,
				_ => {}
			}
			std::fs::write(out, serde_json::to_string(&ident).unwrap()).unwrap();
			return Ok(1);
		}
		Ok(0)
	}
	fn child(
		&mut self,
		command: &[String],
		cwd_: &Path,
		_: File,
		limit: Duration,
	) -> anyhow::Result<i32> {
		self.note(
			"child",
			command,
			json!({"cwd": cwd_, "timeout": limit.as_secs()}),
		);
		let arg = |flag: &str| {
			PathBuf::from(&command[command.iter().position(|c| c == flag).unwrap() + 1])
		};
		let out = arg("--out");
		std::fs::create_dir_all(&out).unwrap();
		if command[2].ends_with("paired_build.py") {
			if self.scenario == "a failing child stage"
				&& out.parent().unwrap().file_name().unwrap() == "record"
			{
				atomic_json(
					&out.join("RESULT.json"),
					&json!({"status": "FAIL", "error": "planted build failure"}),
				)?;
				return Ok(1);
			}
			let which = out
				.parent()
				.unwrap()
				.file_name()
				.unwrap()
				.to_string_lossy()
				.into_owned();
			let one = (self.scenario == "a one-build anchor receipt" && which == "anchor")
				|| (self.scenario == "a one-build record receipt" && which == "record");
			let roles: &[(&str, &str)] = if one {
				&[("baseline", "--baseline")]
			} else {
				&[("baseline", "--baseline"), ("candidate", "--candidate")]
			};
			let mut builds = Vec::new();
			for (role, flag) in roles.iter().copied() {
				let rev = arg(flag).to_string_lossy().into_owned();
				let b = out.join(role).join("rnx");
				std::fs::create_dir_all(b.parent().unwrap()).unwrap();
				std::fs::write(&b, format!("binary {rev}")).unwrap();
				let mut build =
					json!({"role":role,"source_commit":rev,"binary":b,"sha256":digest(&b)?});
				if self.scenario == "a split pair" && which == "record" {
					let engine = b.with_file_name("engine.so");
					std::fs::write(&engine, format!("engine {rev}"))?;
					build["measured"] = json!({"version":1,"target":"x86_64-unknown-linux-gnu",
						"runner":{"path":"/run/rnx-route/rnx","sha256":build["sha256"]},
						"engine":{"path":"/run/rnx-route/libengine_image.so","sha256":digest(&engine)?}});
					build["engine_binary"] = json!(engine);
				}
				builds.push(build);
			}
			atomic_json(
				&out.join("RESULT.json"),
				&json!({"status": "PASS", "builds": builds}),
			)?;
		} else {
			let mut records = Vec::new();
			for role in ["baseline", "candidate"] {
				for k in self.policy["kernels"].as_array().unwrap() {
					for m in self.policy["modes"].as_array().unwrap() {
						for n in self.policy["sizes"].as_array().unwrap() {
							let (k, m, n) = (
								k.as_str().unwrap(),
								m.as_str().unwrap(),
								n.as_u64().unwrap(),
							);
							let case = out.join(role).join(format!("{k}-{m}-{n}"));
							std::fs::create_dir_all(&case).unwrap();
							for name in ["profile", "program.rn", "program.stdout"] {
								std::fs::write(case.join(name), format!("{name} {k} {m} {n}"))
									.unwrap();
							}
							let value = n * 100000;
							let row = json!({"status": "PASS", "role": role, "kind": k, "policy": m, "n": n,
								"profile": case.join("profile"), "profile_sha256": digest(&case.join("profile"))?,
								"fixture_sha256": digest(&case.join("program.rn"))?, "stdout_sha256": digest(&case.join("program.stdout"))?,
								"accounting": {"rnx_Ir": value, "other_Ir": 0, "dynamic_Ir": 7, "total_Ir": value + 7}});
							let p = case.join("ROW.json");
							atomic_json(&p, &row)?;
							records.push(json!(p));
						}
					}
				}
			}
			atomic_json(
				&out.join("RESULT.json"),
				&json!({"status": "PASS", "records": records}),
			)?;
		}
		Ok(0)
	}
	fn output(&mut self, command: &[String], _: Option<&Path>) -> anyhow::Result<String> {
		assert_eq!(
			&command[1..4],
			["which", "--toolchain", "1.98.1"],
			"{command:?}"
		);
		let dir = if command[0].starts_with('/') {
			PathBuf::from("/planted/toolchain")
		} else {
			self.tmp.join("toolchain")
		};
		Ok(format!("{}\n", dir.join(&command[4]).display()))
	}
	fn cpuinfo(&mut self) -> anyhow::Result<String> {
		Ok(CPU.into())
	}
	fn which(&self, name: &str, _: &Environment) -> Option<PathBuf> {
		match name {
			"rustup" => Some(PathBuf::from("/planted/rustup")),
			"python3" if self.scenario == "a relative interpreter" => {
				Some(PathBuf::from("rel/bin/python3"))
			}
			"python3" => Some(PathBuf::from(PYTHON)),
			_ => None,
		}
	}
}

/// The scratch checkout: the real tooling, kernels and policy, and a
/// planted guard.
fn legacy_policy() -> Value {
	let mut policy: Value = serde_json::from_slice(
		&std::fs::read(root().join(rnx_final::fixed_route::POLICY)).unwrap(),
	)
	.unwrap();
	policy["schema"] = json!(1);
	policy.as_object_mut().unwrap().remove("native_setup");
	policy.as_object_mut().unwrap().remove("installed_setup");
	policy.as_object_mut().unwrap().remove("meter");
	policy
}

fn checkout(tmp: &Path) -> PathBuf {
	let root = tmp.join("checkout");
	for rel in frs::TOOLING
		.iter()
		.copied()
		.chain([rnx_final::fixed_route::POLICY])
	{
		let to = root.join(rel);
		std::fs::create_dir_all(to.parent().unwrap()).unwrap();
		std::fs::copy(self::root().join(rel), to).unwrap();
	}
	atomic_json(&root.join(rnx_final::fixed_route::POLICY), &legacy_policy()).unwrap();
	std::fs::create_dir_all(root.join("tools/routing-bench/kernels")).unwrap();
	for entry in std::fs::read_dir(self::root().join("tools/routing-bench/kernels")).unwrap() {
		let p = entry.unwrap().path();
		if p.extension().is_some_and(|x| x == "rn") {
			std::fs::copy(
				&p,
				root.join("tools/routing-bench/kernels")
					.join(p.file_name().unwrap()),
			)
			.unwrap();
		}
	}
	let guard = root.join("ci/execute_guard/target/debug/execute-guard");
	std::fs::create_dir_all(guard.parent().unwrap()).unwrap();
	std::fs::write(guard, "guard").unwrap();
	root
}

fn files(dir: &Path) -> Vec<String> {
	let mut out = Vec::new();
	let mut stack = vec![dir.to_path_buf()];
	while let Some(d) = stack.pop() {
		for e in std::fs::read_dir(&d).unwrap() {
			let p = e.unwrap().path();
			if p.is_dir() {
				stack.push(p);
			} else {
				out.push(p.strip_prefix(dir).unwrap().to_string_lossy().into_owned());
			}
		}
	}
	out.sort();
	out
}

/// A legacy capture of the first pass's anchor, with its manifest; the
/// cache cleared.
fn legacy(tmp: &Path, root: &Path, revision: Option<&str>) -> Vec<String> {
	let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
	let first = read(&tmp.join("first/RESULT.json"));
	let anchor = read(&tmp.join("first/ANCHOR-EVIDENCE.json"));
	std::fs::remove_dir_all(root.join(frs::CACHE)).unwrap();
	let pair = read(&tmp.join("first/anchor/pair/RESULT.json"));
	let binary = &pair["builds"][0];
	let vectors = read(&tmp.join("first/anchor/vectors/RESULT.json"));
	let rows: Vec<Value> = vectors["records"]
		.as_array()
		.unwrap()
		.iter()
		.map(|p| read(Path::new(p.as_str().unwrap())))
		.filter(|r| r["role"] == "baseline")
		.collect();
	let profile = PathBuf::from(rows[0]["profile"].as_str().unwrap());
	let manifest = profile
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.join("MANIFEST.json");
	atomic_json(
		&manifest,
		&json!({"environment": frs::capture_environment(), "binaries": {"baseline": {"binary": binary["binary"], "sha256": binary["sha256"]}}}),
	)
	.unwrap();
	let mut artifacts = serde_json::Map::new();
	artifacts.insert(
		binary["binary"].as_str().unwrap().into(),
		binary["sha256"].clone(),
	);
	for r in &rows {
		let p = PathBuf::from(r["profile"].as_str().unwrap());
		let d = p.parent().unwrap();
		artifacts.insert(p.to_string_lossy().into(), r["profile_sha256"].clone());
		artifacts.insert(
			d.join("program.rn").to_string_lossy().into(),
			r["fixture_sha256"].clone(),
		);
		artifacts.insert(
			d.join("program.stdout").to_string_lossy().into(),
			r["stdout_sha256"].clone(),
		);
	}
	let receipt = tmp.join("legacy/ANCHOR.json");
	std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
	let policy_anchor = anchor["revision"].as_str().unwrap().to_owned();
	atomic_json(
		&receipt,
		&json!({"status": "PASS", "revision": revision.unwrap_or(&policy_anchor), "recipe": first["recipe"],
			"policy_sha256": anchor["policy_sha256"], "records": rows, "artifacts": artifacts}),
	)
	.unwrap();
	vec!["--import-anchor".into(), receipt.to_string_lossy().into()]
}

fn scenario(name: &str, extra: &[&str], before: Option<Before>) -> Value {
	scenario_with(name, extra, before, &[], None, None)
}

/// What a control inspects after the gate, before the scratch is removed:
/// the scratch root and the retained output.
type Inspect<'a> = &'a dyn Fn(&Path, &Path);

/// A scenario with extra environment and, optionally, a launch directory
/// (relative to the scratch root) other than the scratch root itself.
fn scenario_with(
	name: &str,
	extra: &[&str],
	before: Option<Before>,
	more_env: &[(&str, &str)],
	launch: Option<&str>,
	inspect: Option<Inspect>,
) -> Value {
	let s = scratch();
	let tmp = &s.0;
	let root = checkout(tmp);
	std::fs::create_dir_all(tmp.join("toolchain")).unwrap();
	for name in ["rustc", "cargo"] {
		std::fs::write(tmp.join("toolchain").join(name), format!("planted {name}")).unwrap();
	}
	let valgrind = tmp.join("valgrind");
	std::fs::create_dir_all(valgrind.join("bin")).unwrap();
	std::fs::write(valgrind.join("bin/valgrind"), "valgrind").unwrap();
	std::fs::set_permissions(
		valgrind.join("bin/valgrind"),
		std::os::unix::fs::PermissionsExt::from_mode(0o755),
	)
	.unwrap();
	let policy: Value =
		serde_json::from_slice(&std::fs::read(root.join(rnx_final::fixed_route::POLICY)).unwrap())
			.unwrap();
	let calls = Rc::new(RefCell::new(Vec::new()));
	let mut host = Gate {
		tmp: tmp.clone(),
		root: root.clone(),
		scenario: name.into(),
		calls: calls.clone(),
		sources: 0,
		policy,
	};
	let mut e = env(&[
		("PATH", "/planted/bin"),
		("HOME", "/planted/home"),
		("RUSTFLAGS", "-Copt-level=3"),
	]);
	for (k, v) in more_env {
		let v = v.replace("<TMP>", tmp.to_str().unwrap());
		// The final's owner creates the assigned root before the stage runs.
		if *k == "RNX_FINAL_FIXED_ROOT" {
			std::fs::create_dir_all(&v).unwrap();
		}
		e.insert(k.to_string(), v);
	}
	let launch_dir = launch.map_or_else(|| tmp.clone(), |l| tmp.join(l));
	std::fs::create_dir_all(&launch_dir).unwrap();
	let options = |out: &Path, extra: &[String]| {
		let mut args: Vec<String> = [
			"--out",
			&out.to_string_lossy(),
			"--cargo",
			"/planted/bin/cargo",
			"--valgrind-root",
			&valgrind.to_string_lossy(),
		]
		.map(String::from)
		.into();
		args.extend(extra.iter().cloned());
		frs::parse(&args, &launch_dir).unwrap()
	};
	let mut extra: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
	if let Some(before) = before {
		let first = options(&tmp.join("first"), &["--anchor-only".to_string()]);
		assert_eq!(frs::gate(&first, &root, &e, &mut host).unwrap(), 0);
		extra.extend(before(tmp, &root));
		calls.borrow_mut().clear();
		host.sources = 0;
	}
	let out = tmp.join("out");
	let code = frs::gate(&options(&out, &extra), &root, &e, &mut host).unwrap();
	if let Some(inspect) = inspect {
		inspect(tmp, &out);
	}
	let mut receipt: Value =
		serde_json::from_slice(&std::fs::read(out.join("RESULT.json")).unwrap()).unwrap();
	if let Some(a) = receipt
		.get_mut("anchor_cache")
		.and_then(Value::as_object_mut)
	{
		a.remove("anchor_build_seconds");
		a.remove("anchor_total_seconds");
	}
	let journal = root.join(frs::CACHE).join("OWNERS.json");
	let cache = journal.exists().then(|| {
		let j: Value = serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
		json!(
			j["entries"]
				.as_array()
				.unwrap()
				.iter()
				.map(|e| json!({"key": e["key"], "state": e["state"]}))
				.collect::<Vec<_>>()
		)
	});
	let value = json!({"exit": code, "calls": *calls.borrow(), "receipt": receipt, "files": files(&out), "cache": cache});
	let _ = &host.root;
	mark(
		&value,
		&[
			(PYTHON, "<PYTHON>"),
			(out.to_str().unwrap(), "<OUT>"),
			(root.to_str().unwrap(), "<ROOT>"),
			(tmp.to_str().unwrap(), "<TMP>"),
		],
	)
}

#[test]
fn the_gate_runs_as_the_published_python() {
	let expected = witness("fixed-route-gate.json");
	let plain = [
		"a miss captured and published",
		"an identity observation continues",
		"an identity tool error",
		"a dirty tree",
		"an approval refusal",
		"a failing child stage",
		"the source changes during the gate",
	];
	let mut seen = Vec::new();
	let check = |name: &str, got: Value| {
		let want = &expected[name];
		for key in ["exit", "cache", "files", "calls"] {
			assert_eq!(got[key], want[key], "{name}: {key}");
		}
		let (got_r, want_r) = (
			got["receipt"].as_object().unwrap(),
			want["receipt"].as_object().unwrap(),
		);
		for (k, v) in want_r {
			assert_eq!(got_r.get(k), Some(v), "{name}: receipt {k}");
		}
		assert_eq!(got_r.len(), want_r.len(), "{name}: receipt keys");
	};
	for name in plain {
		check(name, scenario(name, &[], None));
		seen.push(name);
	}
	check(
		"anchor only",
		scenario("anchor only", &["--anchor-only"], None),
	);
	check(
		"a cache hit",
		scenario("a cache hit", &[], Some(&|_: &Path, _: &Path| vec![])),
	);
	check(
		"a legacy anchor migrated",
		scenario(
			"a legacy anchor migrated",
			&[],
			Some(&|t: &Path, r: &Path| legacy(t, r, None)),
		),
	);
	check(
		"a legacy anchor that fails",
		scenario(
			"a legacy anchor that fails",
			&[],
			Some(&|t: &Path, r: &Path| legacy(t, r, Some(&"0".repeat(40)))),
		),
	);
	assert_eq!(seen.len() + 4, expected.as_object().unwrap().len());
}

/// Explicit root first, then RNX_VALGRIND_ROOT, then PATH, a linked PATH
/// binary resolved before its root is derived.
#[test]
fn valgrind_resolves_explicit_then_environment_then_a_resolved_path() {
	let s = scratch();
	let install = |name: &str| {
		let root = s.0.join(name);
		std::fs::create_dir_all(root.join("bin")).unwrap();
		std::fs::write(root.join("bin/valgrind"), "valgrind").unwrap();
		std::fs::set_permissions(
			root.join("bin/valgrind"),
			std::os::unix::fs::PermissionsExt::from_mode(0o755),
		)
		.unwrap();
		root
	};
	let (explicit, configured, real) =
		(install("explicit"), install("configured"), install("real"));
	let linked = s.0.join("links/bin");
	std::fs::create_dir_all(&linked).unwrap();
	std::os::unix::fs::symlink(real.join("bin/valgrind"), linked.join("valgrind")).unwrap();
	let both = env(&[
		("RNX_VALGRIND_ROOT", configured.to_str().unwrap()),
		("PATH", linked.to_str().unwrap()),
	]);
	assert_eq!(
		rnx_final::stage::valgrind_root_from(Some(&explicit), &both).unwrap(),
		explicit
	);
	assert_eq!(
		rnx_final::stage::valgrind_root_from(None, &both).unwrap(),
		configured
	);
	let path_only = env(&[("PATH", linked.to_str().unwrap())]);
	assert_eq!(
		rnx_final::stage::valgrind_root_from(None, &path_only).unwrap(),
		real
	);
	assert!(rnx_final::stage::valgrind_root_from(Some(&s.0.join("missing")), &both).is_err());
	// Nowhere at all: refused.
	let empty = s.0.join("empty");
	std::fs::create_dir(&empty).unwrap();
	assert!(
		rnx_final::stage::valgrind_root_from(None, &env(&[("PATH", empty.to_str().unwrap())]))
			.is_err()
	);
}

/// The calls a scenario made, as (kind, the program's script).
fn children(value: &Value) -> Vec<String> {
	value["calls"]
		.as_array()
		.unwrap()
		.iter()
		.filter(|c| c["kind"] == "child" || c["kind"] == "run")
		.map(|c| {
			let command = c["command"].as_array().unwrap();
			command
				.get(2)
				.and_then(Value::as_str)
				.unwrap_or_else(|| command[0].as_str().unwrap())
				.to_string()
		})
		.collect()
}

/// Bad build or identity evidence, or a malformed legacy recipe, ends in a
/// FAIL receipt: no panic, and nothing is captured from it.
#[test]
fn malformed_evidence_fails_the_gate_without_capture() {
	for (name, error, captured) in [
		(
			"a one-build anchor receipt",
			"ValueError: build receipt must name exactly two builds",
			0,
		),
		(
			"a one-build record receipt",
			"ValueError: build receipt must name exactly two builds",
			1,
		),
		(
			"an identity receipt of []",
			"ValueError: identity tool failed, not a measured mismatch",
			1,
		),
		(
			"an identity receipt of null",
			"ValueError: identity tool failed, not a measured mismatch",
			1,
		),
	] {
		let got = scenario(name, &[], None);
		assert_eq!(got["exit"], 1, "{name}");
		assert_eq!(got["receipt"]["status"], "FAIL", "{name}");
		assert_eq!(got["receipt"]["error"], error, "{name}");
		// Only the anchor's own capture, if it got that far: never the record's.
		let vectors = children(&got)
			.iter()
			.filter(|c| c.ends_with("fixed_route_vectors.py"))
			.count();
		assert_eq!(vectors, captured, "{name}: {:?}", children(&got));
	}
	// A scalar legacy recipe: the import refuses it as Python's TypeError,
	// which the gate does not swallow.
	let got = scenario(
		"a scalar legacy recipe",
		&[],
		Some(&|t: &Path, r: &Path| {
			let args = legacy(t, r, None);
			let receipt = PathBuf::from(&args[1]);
			let mut v: Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
			v["recipe"] = json!(5);
			atomic_json(&receipt, &v).unwrap();
			args
		}),
	);
	assert_eq!(got["exit"], 1);
	assert_eq!(
		got["receipt"]["error"],
		"TypeError: the legacy capture's recipe is not a mapping"
	);
	assert!(
		!children(&got)
			.iter()
			.any(|c| c.ends_with("fixed_route_vectors.py")),
		"{:?}",
		children(&got)
	);
}

/// A relative PATH interpreter is spelled against the launch directory,
/// not the checkout the children run in, and its link is not resolved.
#[test]
fn a_relative_interpreter_is_frozen_against_the_launch_directory() {
	let got = scenario_with(
		"a relative interpreter",
		&["--anchor-only"],
		None,
		&[],
		Some("launch"),
		None,
	);
	assert_eq!(got["exit"], 0, "{}", got["receipt"]);
	for call in got["calls"].as_array().unwrap() {
		if call["kind"] == "child" {
			assert_eq!(call["command"][0], "<TMP>/launch/rel/bin/python3", "{call}");
			assert_eq!(call["cwd"], "<ROOT>");
		}
	}
}

/// In a final, the assigned payload is separate from the retained output:
/// a failing child's bundle and the kept logs are copied out, and they
/// still name the cause once the payload is released.
#[test]
fn a_final_failure_keeps_its_evidence_after_the_payload_is_released() {
	let got = scenario_with(
		"a failing child stage",
		&[],
		None,
		&[
			("RNX_FINAL", "1"),
			("RNX_FINAL_FIXED_ROOT", "<TMP>/payload"),
		],
		None,
		Some(&|tmp: &Path, out: &Path| {
			// The payload the stage worked in is released.
			assert!(tmp.join("payload/record/build.log").is_file());
			std::fs::remove_dir_all(tmp.join("payload")).unwrap();
			let read =
				|p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
			let receipt = read(&out.join("RESULT.json"));
			assert_eq!(receipt["failure"]["error"], "planted build failure");
			assert_eq!(receipt["failure"]["stage"], "build");
			let index = out.join("FAILURE.json");
			assert_eq!(receipt["failure"]["path"], json!(index.to_string_lossy()));
			assert_eq!(receipt["failure"]["sha256"], json!(digest(&index).unwrap()));
			let failure = std::fs::read_to_string(&index).unwrap();
			assert!(failure.contains("planted build failure"), "{failure}");
			// The planted cause, in the retained child receipt.
			let child = std::fs::read_to_string(out.join("failure/00-receipt.json")).unwrap();
			assert!(child.contains("planted build failure"), "{child}");
			assert!(out.join("failure/01-child.log").is_file());
			// The kept semantic logs, the failing stage's row among them.
			let row = read(&out.join("record/build-stage.json"));
			assert_eq!(row["status"], "FAIL");
			assert!(out.join("record/build.log").is_file());
			assert!(out.join("guard-build.log").is_file());
		}),
	);
	assert_eq!(got["exit"], 1);
	assert_eq!(
		got["receipt"]["payload_lifetime"],
		"artifact paths valid through live final validation; after release see PAYLOAD-RELEASE.json and final MANIFEST.json"
	);
}

#[test]
fn identity_precheck_compares_the_owning_elf_of_each_revision() {
	let result = scenario("a split pair", &[], None);
	let calls = result["calls"].as_array().unwrap();
	let call = calls
		.iter()
		.find(|c| c.to_string().contains("executor_identity.py"))
		.unwrap();
	let command = call.to_string();
	for role in ["baseline", "candidate"] {
		assert!(command.contains(&format!("/{role}/engine.so")), "{command}");
		assert!(!command.contains(&format!("/{role}/rnx")), "{command}");
	}
}
