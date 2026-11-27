//! The fixed-route policy, scoring and anchor cache (ADR 0296) against
//! witnesses from the published Python at 96a809fe. (ADR 0298 retired the
//! Python cache, and with it 0296's two-way handoff control.)
use rnx_final::child::Raised;
use rnx_final::fixed_route::{self as fr, Cache, Point};
use rnx_final::pyjson::identity;
use rnx_final::receipt::digest;
use serde_json::{Map, Value, json};
use std::cell::{Cell, RefCell};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap()
}

fn witness(name: &str) -> Value {
	let path = root().join("tools/final/tests/witness").join(name);
	let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
	assert_eq!(value["witness"], "96a809fe09fed4f9a19487854f4d4de5c3784c92");
	value["expected"].clone()
}

fn policy() -> Value {
	let mut p: Value =
		serde_json::from_str(&std::fs::read_to_string(root().join(fr::POLICY)).unwrap()).unwrap();
	p["schema"] = json!(1);
	p.as_object_mut().unwrap().remove("source_transitions");
	p.as_object_mut().unwrap().remove("native_setup");
	p.as_object_mut().unwrap().remove("installed_setup");
	p.as_object_mut().unwrap().remove("meter");
	p.as_object_mut().unwrap().remove("relative_relocations");
	p
}

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-fixed-route-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

/// A case's outcome, as the witness records Python's.
fn outcome<T: serde::Serialize>(result: anyhow::Result<T>) -> Value {
	match result {
		Ok(v) => json!({"ok": v}),
		Err(e) => match e.downcast_ref::<Raised>() {
			Some(r) => {
				let class = r.class.rsplit('.').next().unwrap_or(r.class);
				json!({"error": class, "message": r.message, "repr": r.repr})
			}
			None => json!({"error": "unexpected", "message": format!("{e:#}")}),
		},
	}
}

/// Equal to the witness case, `repr` compared only where it was recorded.
fn same(label: &str, got: &Value, want: &Value) {
	let mut got = got.clone();
	if want.get("repr").is_none()
		&& let Some(o) = got.as_object_mut()
	{
		o.remove("repr");
	}
	assert_eq!(&got, want, "{label}");
}

/// A labeled change to a planted value.
type Change<'a> = (&'a str, Box<dyn Fn(&mut Value)>);
/// One cache scenario's body: scratch root, steps, source, binary,
/// receipt and harness.
type Scenario<'a> = &'a dyn Fn(&Path, &mut Steps, &Path, &Path, &Value, &Harness);

// --- The band. ---

fn rows(policy: &Value, low: u64, high: u64, dynamic: u64) -> Vec<Value> {
	let mut out = Vec::new();
	for k in policy["kernels"].as_array().unwrap() {
		for m in policy["modes"].as_array().unwrap() {
			for (n, value) in [(1, low), (5, high)] {
				out.push(json!({"status": "PASS", "kind": k, "policy": m, "n": n, "stdout_sha256": "output", "fixture_sha256": "fixture",
					"accounting": {"rnx_Ir": value, "other_Ir": 0, "dynamic_Ir": dynamic, "total_Ir": value + dynamic}}));
			}
		}
	}
	out
}

fn base_rows(policy: &Value) -> Vec<Value> {
	rows(policy, 100000, 500000, 0)
}

#[test]
fn the_band_answers_as_the_published_python() {
	let policy = policy();
	let expected = witness("fixed-route-band.json");
	let mut cases: Vec<(String, Value)> = Vec::new();
	let mut case = |label: &str, value: Value| cases.push((label.to_owned(), value));
	case(
		"validate: the live policy",
		outcome(fr::validate_policy(&policy)),
	);
	let mutated = |change: &dyn Fn(&mut Value)| {
		let mut p = policy.clone();
		change(&mut p);
		p
	};
	let k = policy["kernels"].as_array().unwrap().clone();
	let changes: Vec<Change> = vec![
		("schema 2", Box::new(|p| p["schema"] = json!(2))),
		(
			"a kernel dropped",
			Box::new(move |p| p["kernels"] = json!(k[..k.len() - 1])),
		),
		(
			"a kernel added",
			Box::new(|p| p["kernels"].as_array_mut().unwrap().push(json!("extra"))),
		),
		(
			"kernels reordered",
			Box::new(|p| p["kernels"].as_array_mut().unwrap().reverse()),
		),
		("modes", Box::new(|p| p["modes"] = json!(["on", "off"]))),
		("sizes", Box::new(|p| p["sizes"] = json!([1, 4]))),
		(
			"per record limit",
			Box::new(|p| p["per_record_per_mille"] = json!(6)),
		),
		(
			"cumulative limit",
			Box::new(|p| p["cumulative_per_mille"] = json!(11)),
		),
		("empty inventory", Box::new(|p| p["inventory"] = json!([]))),
		(
			"duplicate inventory",
			Box::new(|p| {
				let items = p["inventory"].as_array().unwrap().clone();
				p["inventory"].as_array_mut().unwrap().extend(items);
			}),
		),
		(
			"extra selector key",
			Box::new(|p| p["inventory"][0]["extra"] = json!(1)),
		),
		(
			"bad kind",
			Box::new(|p| p["inventory"][0]["kind"] = json!("trait")),
		),
		(
			"absolute file",
			Box::new(|p| p["inventory"][0]["file"] = json!("/etc/passwd")),
		),
		(
			"parent file",
			Box::new(|p| p["inventory"][0]["file"] = json!("rune/../x.rs")),
		),
	];
	for (label, change) in &changes {
		case(
			&format!("validate: {label}"),
			outcome(fr::validate_policy(&mutated(change.as_ref()))),
		);
	}
	for (label, value) in [
		("inventory a string", json!("abc")),
		("inventory a repeated string", json!("aa")),
		("inventory a number", json!(5)),
		("inventory an empty object", json!({})),
		("inventory an object", json!({"file": 1})),
		("inventory null", Value::Null),
		(
			"a selector given as a list",
			json!([["file", "kind", "receiver", "name"]]),
		),
		(
			"a numeric file",
			json!([{"file": 5, "kind": "fn", "receiver": "", "name": "x"}]),
		),
		(
			"a numeric receiver and name",
			json!([{"file": "src/lib.rs", "kind": "fn", "receiver": 5, "name": 6}]),
		),
	] {
		let mut p = policy.clone();
		p["inventory"] = value;
		case(
			&format!("validate: {label}"),
			outcome(fr::validate_policy(&p)),
		);
	}
	// Transitions.
	let i = fr::INITIAL_ANCHOR;
	let (b40, c40, d40) = ("b".repeat(40), "c".repeat(40), "d".repeat(40));
	let a = json!({"anchor": i, "anchor_transition": null});
	let bt = json!({"old": i, "new": b40, "plan": "1".repeat(40), "path": "plans/adr/9998.md"});
	let b = json!({"anchor": b40, "anchor_transition": bt});
	let ct = json!({"old": b40, "new": c40, "plan": "2".repeat(40), "path": "plans/adr/9999.md"});
	let c = json!({"anchor": c40, "anchor_transition": ct});
	let with = |base: &Value, key: &str, v: Value| {
		let mut x = base.clone();
		x[key] = v;
		x
	};
	let transitions: Vec<(&str, Value, Option<Value>)> = vec![
		("bootstrap", a.clone(), None),
		(
			"bootstrap with an approval",
			with(&a, "anchor_transition", bt.clone()),
			None,
		),
		("first link", b.clone(), Some(a.clone())),
		("second link", c.clone(), Some(b.clone())),
		("unchanged", c.clone(), Some(c.clone())),
		("skipping", c.clone(), Some(a.clone())),
		(
			"unchanged with a changed approval",
			with(&c, "anchor_transition", bt.clone()),
			Some(c.clone()),
		),
		(
			"wrong old",
			with(&c, "anchor_transition", with(&ct, "old", json!(i))),
			Some(b.clone()),
		),
		(
			"wrong new",
			with(&c, "anchor_transition", with(&ct, "new", json!(d40))),
			Some(b.clone()),
		),
		(
			"unstructured approval",
			with(&c, "anchor_transition", json!("approved")),
			Some(b.clone()),
		),
		(
			"approval with an extra key",
			with(&c, "anchor_transition", with(&ct, "extra", json!(1))),
			Some(b.clone()),
		),
		(
			"new anchor from bootstrap without approval",
			json!({"anchor": b40, "anchor_transition": null}),
			None,
		),
	];
	for (label, p, prev) in &transitions {
		case(
			&format!("transition: {label}"),
			outcome(fr::anchor_transition(p, prev.as_ref())),
		);
	}
	// Comparisons.
	let cmp = |a: Vec<Value>, b: Vec<Value>, limit: i128| {
		outcome(fr::compare(&json!(a), &json!(b), &policy, limit))
	};
	for (low, high) in [
		(100500, 502500),
		(100500, 502501),
		(100000, 502000),
		(100000, 502001),
		(90000, 450000),
		(100000, 500000),
		(99000, 505000),
		(110000, 502500),
	] {
		case(
			&format!("compare: {low} {high}"),
			cmp(base_rows(&policy), rows(&policy, low, high, 0), 5),
		);
	}
	let (big1, big5): (u64, u64) = (1 << 60, 1 << 62);
	case(
		"compare: high counts at the limit",
		cmp(
			rows(&policy, big1, big5, 0),
			rows(&policy, big1 + big1 / 200, big5 + 5 * (big5 / 1000), 0),
			5,
		),
	);
	case(
		"compare: high counts one past",
		cmp(
			rows(&policy, big1, big5, 0),
			rows(&policy, big1, big5 + 5 * (big5 / 1000) + 1, 0),
			5,
		),
	);
	case(
		"compare: high counts decreasing",
		cmp(
			rows(&policy, big1, big5, 0),
			rows(&policy, big1 / 2, big5 / 3, 0),
			5,
		),
	);
	case(
		"compare: decreasing growth below the base",
		cmp(base_rows(&policy), rows(&policy, 300000, 400000, 0), 5),
	);
	case(
		"compare: dynamic growth",
		cmp(base_rows(&policy), rows(&policy, 100000, 500000, 3000), 5),
	);
	case(
		"compare: limit 10",
		cmp(base_rows(&policy), rows(&policy, 101000, 505000, 0), 10),
	);
	for mutation in [
		"missing",
		"duplicate",
		"output",
		"fixture",
		"count",
		"negative",
		"float",
		"bool",
		"failed",
		"no stdout",
		"extraneous",
	] {
		let mut x = base_rows(&policy);
		match mutation {
			"missing" => {
				x.pop();
			}
			"duplicate" => x.push(x[0].clone()),
			"output" => x[0]["stdout_sha256"] = json!("different"),
			"fixture" => x[0]["fixture_sha256"] = json!("different"),
			"count" => x[0]["accounting"]["total_Ir"] = json!(500001),
			"negative" => {
				x[0]["accounting"]["other_Ir"] = json!(-1);
				x[0]["accounting"]["total_Ir"] = json!(99999);
			}
			"float" => x[0]["accounting"]["rnx_Ir"] = json!(100000.0),
			"bool" => x[0]["accounting"]["other_Ir"] = json!(false),
			"failed" => x[0]["status"] = json!("FAIL"),
			"no stdout" => x[0]["stdout_sha256"] = json!(""),
			_ => {
				let mut extra = x[0].clone();
				extra["kind"] = json!("extra");
				x.push(extra);
			}
		}
		case(
			&format!("compare refuses: {mutation}"),
			cmp(base_rows(&policy), x, 5),
		);
	}
	case(
		"compare refuses: denominator",
		cmp(rows(&policy, 5, 5, 0), base_rows(&policy), 5),
	);
	case(
		"compare refuses: equal sizes",
		cmp(rows(&policy, 500000, 500000, 0), base_rows(&policy), 5),
	);
	// Recipes.
	let recipe = json!({"cpu": ["cpu"], "environment": {"e": 1}, "run_environment": {"PATH": "/bin"}, "valgrind": "v", "fixtures": {"f": "a"},
		"policy": {"p": 1}, "tooling": {"tools/column/paired_build.py": "old", "capture.py": "same"}, "build_environment": "old", "toolchain": "old"});
	case(
		"recipe: measurement inputs",
		outcome(fr::measurement_recipe(&recipe)),
	);
	let mut missing = recipe.clone();
	missing.as_object_mut().unwrap().remove("valgrind");
	case(
		"recipe: missing key",
		outcome(fr::measurement_recipe(&missing)),
	);
	// Scores.
	let receipt = |r: Vec<Value>, revision: &Value| {
		json!({"status": "PASS", "revision": revision, "recipe": {"CPU": "a", "toolchain": "b", "environment": "c"},
			"policy_sha256": identity(&policy).unwrap(), "records": r})
	};
	let cand = json!("candidate");
	let anchor = receipt(base_rows(&policy), &policy["anchor"]);
	let base = receipt(rows(&policy, 100800, 504000, 0), &cand);
	let candidate = receipt(rows(&policy, 101200, 506000, 0), &cand);
	case(
		"score: cap",
		outcome(fr::score(&base, &candidate, Some(&anchor), &policy)),
	);
	case(
		"score: identity pass",
		outcome(fr::score(
			&receipt(base_rows(&policy), &cand),
			&receipt(base_rows(&policy), &cand),
			Some(&anchor),
			&policy,
		)),
	);
	for mutation in [
		"missing",
		"policy",
		"CPU",
		"toolchain",
		"environment",
		"anchor",
		"failed anchor",
		"failed base",
		"stale base policy",
	] {
		let (mut a, mut bb) = (anchor.clone(), base.clone());
		match mutation {
			"policy" => a["policy_sha256"] = json!("stale"),
			"anchor" => a["revision"] = json!("wrong"),
			"failed anchor" => a["status"] = json!("FAIL"),
			"failed base" => bb["status"] = json!("FAIL"),
			"stale base policy" => bb["policy_sha256"] = json!("stale"),
			"missing" => {}
			other => a["recipe"][other] = json!("changed"),
		}
		let anchor = (mutation != "missing").then_some(&a);
		case(
			&format!("score refuses: {mutation}"),
			outcome(fr::score(&bb, &candidate, anchor, &policy)),
		);
	}
	let r = json!({"cpu": ["cpu"], "environment": {}, "run_environment": {}, "valgrind": "v", "fixtures": {}, "policy": {}, "tooling": {}, "build_environment": "old"});
	let mut ra = receipt(base_rows(&policy), &policy["anchor"]);
	ra["recipe"] = r.clone();
	let mut rc = receipt(base_rows(&policy), &cand);
	rc["recipe"] = with(&r, "build_environment", json!("new"));
	let rb = rc.clone();
	ra["binary_sha256"] = json!("a".repeat(64));
	ra["reuse_proof"] = json!({"current_recipe": rc["recipe"], "binary_sha256": "a".repeat(64), "current_build": {"sha256": "a".repeat(64)}});
	case(
		"score: anchor rebuild proof",
		outcome(fr::score(&rb, &rc, Some(&ra), &policy)),
	);
	let proofs: Vec<Change> = vec![
		(
			"proof with changed cpu",
			Box::new(|x| x["recipe"]["cpu"] = json!(["other"])),
		),
		(
			"proof with another build",
			Box::new(|x| x["reuse_proof"]["current_build"]["sha256"] = json!("b".repeat(64))),
		),
		(
			"proof for another recipe",
			Box::new(|x| x["reuse_proof"]["current_recipe"] = json!({})),
		),
		(
			"proof without a binary",
			Box::new(|x| x["binary_sha256"] = json!("")),
		),
		(
			"proof binary differs",
			Box::new(|x| x["reuse_proof"]["binary_sha256"] = json!("c".repeat(64))),
		),
	];
	for (label, change) in &proofs {
		let mut x = ra.clone();
		change(&mut x);
		case(
			&format!("score refuses: {label}"),
			outcome(fr::score(&rb, &rc, Some(&x), &policy)),
		);
	}
	let mut x = rb.clone();
	x["recipe"] = r.clone();
	case(
		"score refuses: base recipe differs (no proof applies)",
		outcome(fr::score(&x, &rc, Some(&ra), &policy)),
	);
	let expected = expected.as_object().unwrap();
	for (label, got) in &cases {
		same(label, got, &expected[label]);
	}
	assert_eq!(cases.len(), expected.len());
}

// --- The git-backed checks. ---

fn git(repo: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.arg("-C")
		.arg(repo)
		.args(args)
		.env("GIT_CONFIG_NOSYSTEM", "1")
		.env("GIT_CONFIG_GLOBAL", "/dev/null")
		.output()
		.unwrap();
	assert!(output.status.success(), "git {args:?}: {output:?}");
	String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn write(path: &Path, text: &str) {
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(path, text).unwrap();
}

/// The witness's planted repository, signed with an ephemeral key.
#[test]
fn approval_and_source_answer_as_the_published_python() {
	let s = scratch();
	let repo = s.0.join("repo");
	std::fs::create_dir(&repo).unwrap();
	let key = s.0.join("key");
	assert!(
		Command::new("ssh-keygen")
			.args(["-q", "-t", "ed25519", "-N", "", "-f"])
			.arg(&key)
			.status()
			.unwrap()
			.success()
	);
	let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();
	std::fs::write(
		s.0.join("allowed"),
		format!("fixture@example.invalid {public}"),
	)
	.unwrap();
	git(&repo, &["init", "-q", "-b", "main"]);
	let allowed = s.0.join("allowed").to_string_lossy().into_owned();
	let key_text = key.to_string_lossy().into_owned();
	for (name, value) in [
		("user.name", "Fixture"),
		("user.email", "fixture@example.invalid"),
		("gpg.format", "ssh"),
		("user.signingkey", key_text.as_str()),
		("gpg.ssh.allowedSignersFile", allowed.as_str()),
		("commit.gpgsign", "true"),
	] {
		git(&repo, &["config", name, value]);
	}
	let guard = s.0.join("guard");
	write(
		&guard,
		"#!/bin/sh\n[ \"$5\" = gamma ] && exit 3\ngrep -- \"$5\" \"$2\"\n",
	);
	std::fs::set_permissions(&guard, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
	let commit = |message: &str, signed: bool| {
		git(&repo, &["add", "-A"]);
		let sign = format!("commit.gpgsign={signed}");
		git(
			&repo,
			&["-c", &sign, "commit", "-q", "--allow-empty", "-m", message],
		);
		git(&repo, &["rev-parse", "HEAD"])
	};
	let live = policy();
	let items = json!([{"file": "src/lib.rs", "kind": "fn", "receiver": "", "name": "alpha"}, {"file": "src/lib.rs", "kind": "fn", "receiver": "", "name": "beta"}]);
	let planted = |anchor: &str, transition: Value, inventory: Value| {
		let mut p = Map::new();
		for k in [
			"schema",
			"per_record_per_mille",
			"cumulative_per_mille",
			"kernels",
			"modes",
			"sizes",
		] {
			p.insert(k.into(), live[k].clone());
		}
		p.insert("anchor".into(), json!(anchor));
		p.insert("anchor_transition".into(), transition);
		p.insert("inventory".into(), inventory);
		Value::Object(p)
	};
	let policy_path = repo.join(fr::POLICY);
	write(
		&repo.join("src/lib.rs"),
		"fn alpha() { 1 }\nfn beta() { 2 }\n",
	);
	let x = commit("base", true);
	let p1 = planted(&x, Value::Null, items.clone());
	write(&policy_path, &serde_json::to_string_pretty(&p1).unwrap());
	let b = commit("policy", true);
	write(&repo.join("src/other.rs"), "fn other() {}\n");
	let y = commit("anchor work", true);
	let declaration =
		format!("User-approved anchor transition\nOld anchor: {x}\nNew anchor: {y}\n");
	write(
		&repo.join("plans/adr/9001_anchor.md"),
		&format!("# ADR 9001\n\n{declaration}"),
	);
	let plan = commit("plan: adr 9001 anchor transition", true);
	write(
		&repo.join("plans/adr/9002_unsigned.md"),
		&format!("# ADR 9002\n\n{declaration}"),
	);
	let unsigned = commit("plan: adr 9002 unsigned transition", false);
	let approved = json!({"old": x, "new": y, "plan": plan, "path": "plans/adr/9001_anchor.md"});
	let p2 = planted(&y, approved.clone(), items.clone());
	write(&policy_path, &serde_json::to_string_pretty(&p2).unwrap());
	let c = commit("impl: adr 9001 move the anchor", true);
	write(
		&repo.join("src/lib.rs"),
		"fn alpha() { 10 }\nfn beta() { 2 }\n",
	);
	let d = commit("impl: change a fixed item", true);
	git(&repo, &["checkout", "-q", "-b", "side", &b]);
	write(&repo.join("plans/adr/9003_side.md"), &declaration);
	let side = commit("plan: adr 9003 side transition", true);
	git(&repo, &["checkout", "-q", "main"]);
	let gamma = json!([{"file": "src/lib.rs", "kind": "fn", "receiver": "", "name": "gamma"}]);
	let mut pg = p2.clone();
	pg["inventory"] = gamma.clone();
	write(&policy_path, &serde_json::to_string_pretty(&pg).unwrap());
	let g = commit("policy with a failing item", true);
	let mut numeric = p2.clone();
	numeric["inventory"] =
		json!([{"file": "src/lib.rs", "kind": "fn", "receiver": 5, "name": "alpha"}]);
	write(
		&policy_path,
		&serde_json::to_string_pretty(&numeric).unwrap(),
	);
	let n = commit("policy with a numeric receiver", true);
	let transition = |change: &[(&str, &str)]| {
		let mut p = p2.clone();
		for (k, v) in change {
			p["anchor_transition"][*k] = json!(v);
		}
		p
	};
	let mut cases: Vec<(String, Value)> = Vec::new();
	let approvals: Vec<(&str, &str, &str, Value)> = vec![
		("a signed, declared, ancestral plan", &b, &c, p2.clone()),
		("an unchanged anchor", &b, &c, p1.clone()),
		("a short anchor", &b, &c, with_anchor(&p2, "abc")),
		(
			"an unknown full anchor",
			&b,
			&c,
			with_anchor(&p2, &"0".repeat(40)),
		),
		("a baseline without a policy", &x, &c, p2.clone()),
		(
			"a transition skipping the baseline",
			&b,
			&c,
			transition(&[("old", &"f".repeat(40))]),
		),
		("a short plan", &b, &c, transition(&[("plan", "abc")])),
		(
			"a plan that is not an ancestor",
			&b,
			&c,
			transition(&[("plan", &side), ("path", "plans/adr/9003_side.md")]),
		),
		(
			"a plan that is not a plan commit",
			&b,
			&c,
			transition(&[("plan", &y)]),
		),
		(
			"an unsigned plan",
			&b,
			&c,
			transition(&[("plan", &unsigned), ("path", "plans/adr/9002_unsigned.md")]),
		),
		(
			"a plan without the declaration",
			&b,
			&c,
			transition(&[("path", "src/lib.rs")]),
		),
	];
	for (label, baseline, candidate, p) in &approvals {
		cases.push((
			format!("approval: {label}"),
			outcome(fr::anchor_approval(&repo, baseline, candidate, p)),
		));
	}
	let mut more = items.as_array().unwrap().clone();
	more.push(json!({"file": "src/other.rs", "kind": "fn", "receiver": "", "name": "other"}));
	let mut inventory = p2.clone();
	inventory["inventory"] = json!(more);
	let mut reordered = p2.clone();
	reordered["kernels"].as_array_mut().unwrap().reverse();
	let sources: Vec<(&str, &str, &str, Value)> = vec![
		("unchanged items", &b, &c, p2.clone()),
		("a changed item", &b, &d, p2.clone()),
		("a changed inventory", &b, &c, inventory),
		("reordered kernels", &b, &c, reordered),
		("a baseline without a policy", &x, &c, p2.clone()),
		("a failing guard", &g, &g, pg.clone()),
		("an inventory string", &b, &c, {
			let mut p = p2.clone();
			p["inventory"] = json!("abc");
			p
		}),
		("a numeric receiver", &n, &n, numeric.clone()),
	];
	for (label, baseline, candidate, p) in &sources {
		cases.push((
			format!("source: {label}"),
			outcome(fr::source_condition(&repo, baseline, candidate, p, &guard)),
		));
	}
	// A changed canonical item needs one exact declaration. The candidate
	// receipt carries the same hashes an independent reviewer can recompute.
	use sha2::{Digest as _, Sha256};
	let hash = |text: &str| rnx_final::pyjson::hex(&Sha256::digest(text.as_bytes()));
	let entry = json!({"selector":items[0],"baseline":b,"adr":"plans/adr/9001_anchor.md",
		"old_sha256":hash("fn alpha() { 1 }"),"new_sha256":hash("fn alpha() { 10 }")});
	let mut declared = p2.clone();
	declared["source_transitions"] = json!([entry]);
	let check = |candidate: &str, p: &Value| fr::source_condition(&repo, &b, candidate, p, &guard);
	let report = check(&d, &declared).unwrap();
	assert_eq!(report["items"][0]["transition"], entry);
	assert!(report["items"][1].get("transition").is_none());
	assert!(check(&d, &p2).is_err());
	assert!(check(&c, &declared).is_err());
	for (key, wrong) in [
		("old_sha256", hash("another old item")),
		("new_sha256", hash("another new item")),
		("baseline", c.clone()),
		("adr", "plans/adr/absent.md".into()),
	] {
		let mut p = declared.clone();
		p["source_transitions"][0][key] = json!(wrong);
		assert!(check(&d, &p).is_err(), "{key}");
	}
	// The next unchanged comparison can inherit a previous declaration.
	assert!(fr::source_condition(&repo, &d, &d, &declared, &guard).is_ok());
	let mut text = serde_json::to_string(&cases).unwrap();
	let mut names = vec![
		(x, "<X>"),
		(b.clone(), "<B>"),
		(y, "<Y>"),
		(plan, "<PLAN>"),
		(unsigned, "<UNSIGNED>"),
		(c.clone(), "<C>"),
		(d, "<D>"),
		(side, "<SIDE>"),
		(g, "<G>"),
		(n, "<N>"),
	];
	names.sort_by_key(|(v, _)| std::cmp::Reverse(v.len()));
	for (value, mark) in names {
		text = text.replace(&value, mark);
	}
	text = text
		.replace(&*repo.to_string_lossy(), "<REPO>")
		.replace(&*guard.to_string_lossy(), "<GUARD>");
	let source = regex::Regex::new(r#"/[^"']*/fixed-source-[^/"']+/source\.rs"#).unwrap();
	text = source.replace_all(&text, "<SOURCE>").into_owned();
	let cases: Vec<(String, Value)> = serde_json::from_str(&text).unwrap();
	let expected = witness("fixed-route-git.json");
	for (label, got) in &cases {
		same(label, got, &expected[label]);
	}
	assert_eq!(cases.len(), expected.as_object().unwrap().len());
}

fn with_anchor(p: &Value, anchor: &str) -> Value {
	let mut p = p.clone();
	p["anchor"] = json!(anchor);
	p
}

// --- The anchor cache. ---

/// The witness's source bundle under `tmp/source`, and its receipt.
fn source(tmp: &Path, policy: &Value) -> (PathBuf, PathBuf, Value) {
	let src = tmp.join("source");
	std::fs::create_dir(&src).unwrap();
	let binary = src.join("rnx");
	std::fs::write(&binary, "binary").unwrap();
	let mut artifacts = Map::new();
	artifacts.insert(
		binary.to_string_lossy().into_owned(),
		json!(digest(&binary).unwrap()),
	);
	let mut records = Vec::new();
	for k in policy["kernels"].as_array().unwrap() {
		for m in policy["modes"].as_array().unwrap() {
			for n in policy["sizes"].as_array().unwrap() {
				let p = src.join(format!(
					"{}-{}-{}",
					k.as_str().unwrap(),
					m.as_str().unwrap(),
					n
				));
				std::fs::create_dir(&p).unwrap();
				for name in ["profile", "program.rn", "program.stdout"] {
					std::fs::write(p.join(name), name).unwrap();
					artifacts.insert(
						p.join(name).to_string_lossy().into_owned(),
						json!(digest(&p.join(name)).unwrap()),
					);
				}
				let size = n.as_u64().unwrap();
				records.push(json!({"status": "PASS", "kind": k, "policy": m, "n": n,
					"profile": p.join("profile").to_string_lossy(), "profile_sha256": digest(&p.join("profile")).unwrap(),
					"fixture_sha256": digest(&p.join("program.rn")).unwrap(), "stdout_sha256": digest(&p.join("program.stdout")).unwrap(),
					"accounting": {"rnx_Ir": size * 100000, "other_Ir": 0, "dynamic_Ir": 10, "total_Ir": size * 100000 + 10}}));
			}
		}
	}
	let receipt = json!({"status": "PASS", "revision": policy["anchor"], "recipe": {}, "policy_sha256": identity(policy).unwrap(),
		"records": records, "artifacts": artifacts, "binary_sha256": digest(&binary).unwrap(), "capture_key": "key1"});
	(src, binary, receipt)
}

fn tree(root: &Path) -> Vec<String> {
	fn walk(root: &Path, dir: &Path, out: &mut Vec<(Vec<String>, String)>) {
		let Ok(entries) = std::fs::read_dir(dir) else {
			return;
		};
		for entry in entries {
			let p = entry.unwrap().path();
			let meta = p.symlink_metadata().unwrap();
			let kind = if meta.file_type().is_symlink() {
				"l"
			} else if p.is_dir() {
				"d"
			} else {
				"f"
			};
			let relative = p.strip_prefix(root).unwrap();
			let parts = relative
				.components()
				.map(|c| match c {
					Component::Normal(n) => n.to_string_lossy().into_owned(),
					_ => String::new(),
				})
				.collect();
			out.push((parts, format!("{kind} {}", relative.display())));
			if kind == "d" {
				walk(root, &p, out);
			}
		}
	}
	let mut out = Vec::new();
	if root.exists() {
		walk(root, root, &mut out);
	}
	out.sort();
	out.into_iter().map(|(_, line)| line).collect()
}

fn normalize(value: &Value, tmp: &Path) -> Value {
	let escaped = serde_json::to_string(&tmp.to_string_lossy()).unwrap();
	let text = serde_json::to_string(value)
		.unwrap()
		.replace(&escaped[1..escaped.len() - 1], "<TMP>");
	let mut v: Value = serde_json::from_str(&text).unwrap();
	fn walk(x: &mut Value) {
		match x {
			Value::Object(o) => {
				for (k, v) in o.iter_mut() {
					match k.as_str() {
						"parent_device" | "parent_inode" | "device" | "inode" => {
							*v = json!(format!("<{}>", k.to_uppercase()))
						}
						"receipt_sha256" if v.as_str().is_some_and(|s| s.len() == 64) => {
							*v = json!("<ANCHOR-DIGEST>")
						}
						_ => walk(v),
					}
				}
			}
			Value::Array(a) => a.iter_mut().for_each(walk),
			_ => {}
		}
	}
	walk(&mut v);
	v
}

/// The faults a scenario arms, and the identifier sequence it shares.
#[derive(Clone, Default)]
struct Harness {
	armed: Rc<RefCell<Vec<(Point, &'static str)>>>,
	next: Rc<Cell<u32>>,
}

impl Harness {
	fn open(&self, root: &Path) -> anyhow::Result<Cache> {
		let next = self.next.clone();
		let armed = self.armed.clone();
		Cache::with(
			root,
			Box::new(move || {
				next.set(next.get() + 1);
				format!("{:032x}", next.get())
			}),
			Box::new(
				move |point| match armed.borrow().iter().find(|(p, _)| *p == point) {
					Some((_, message)) => Err(std::io::Error::other(*message)),
					None => Ok(()),
				},
			),
		)
	}

	fn arm(&self, faults: &[(Point, &'static str)]) {
		*self.armed.borrow_mut() = faults.to_vec();
	}
}

struct Steps<'a> {
	tmp: &'a Path,
	cache_root: PathBuf,
	rows: Vec<Value>,
}

impl Steps<'_> {
	fn record(&mut self, label: &str, mut result: Value, cache: Option<&Cache>) -> Value {
		// The cache witness keeps the class and message, not the repr.
		if let Some(o) = result.as_object_mut() {
			o.remove("repr");
		}
		let journal = match cache {
			Some(c) => c.state.clone(),
			None => std::fs::read_to_string(self.cache_root.join("OWNERS.json"))
				.ok()
				.and_then(|t| serde_json::from_str(&t).ok())
				.unwrap_or(Value::Null),
		};
		self.rows.push(
			json!({"step": label, "result": normalize(&result, self.tmp),
			"journal": normalize(&journal, self.tmp), "tree": tree(&self.cache_root)}),
		);
		result.get("ok").cloned().unwrap_or(Value::Null)
	}
}

fn lookup(c: &mut Cache, key: &str) -> anyhow::Result<Value> {
	let (bundle, why) = c.lookup(key)?;
	Ok(json!([bundle, why]))
}

#[test]
fn the_cache_answers_as_the_published_python() {
	let expected = witness("fixed-route-anchor-cache.json");
	let policy = policy();
	let mut seen = 0;
	let mut check = |name: &str, body: Scenario| {
		let s = scratch();
		let (src, binary, receipt) = source(&s.0, &policy);
		let mut steps = Steps {
			tmp: &s.0,
			cache_root: s.0.join("cache"),
			rows: vec![],
		};
		body(
			&s.0,
			&mut steps,
			&src,
			&binary,
			&receipt,
			&Harness::default(),
		);
		let want = expected[name]
			.as_array()
			.unwrap_or_else(|| panic!("{name}"));
		for (i, (got, want)) in steps.rows.iter().zip(want).enumerate() {
			assert_eq!(got, want, "{name}, step {i}");
		}
		assert_eq!(steps.rows.len(), want.len(), "{name}");
		seen += 1;
	};
	check(
		"publish, then hit with the source gone",
		&|_, st, src, binary, receipt, h| {
			let root = st.cache_root.clone();
			{
				let mut c = h.open(&root).unwrap();
				let r = outcome(lookup(&mut c, "key1"));
				st.record("lookup key1 (empty)", r, Some(&c));
				let r =
					outcome(c.publish(receipt, binary, &json!({"receipt_sha256": "provenance"})));
				st.record("publish key1", r, Some(&c));
			}
			std::fs::remove_dir_all(src).unwrap();
			let mut c = h.open(&root).unwrap();
			let r = outcome((|| -> anyhow::Result<Value> {
				let first = c.lookup("key1")?.0.is_some();
				Ok(json!([first, c.lookup("key1")?.1]))
			})());
			st.record("lookup key1 after the source is gone", r, Some(&c));
			let r = outcome((|| -> anyhow::Result<Value> {
				let bundle = c.lookup("key1")?.0;
				Ok(json!(
					!serde_json::to_string(&bundle)?.contains(&*src.to_string_lossy())
				))
			})());
			st.record("the hit names only the cache", r, Some(&c));
		},
	);
	for mutation in ["missing", "changed", "metadata", "whole-root"] {
		check(
			&format!("damaged bundle: {mutation}"),
			&|_, st, _, binary, receipt, h| {
				let mut c = h.open(&st.cache_root.clone()).unwrap();
				let r = outcome(c.publish(receipt, binary, &json!({})));
				let published = st.record("publish key1", r, Some(&c));
				let entry = PathBuf::from(c.state["entries"][0]["path"].as_str().unwrap());
				let profile = PathBuf::from(published["records"][0]["profile"].as_str().unwrap());
				match mutation {
					"missing" => std::fs::remove_file(&profile).unwrap(),
					"changed" => std::fs::write(&profile, "changed").unwrap(),
					"metadata" => {
						let q = entry.join("ANCHOR.json");
						let mut v: Value =
							serde_json::from_slice(&std::fs::read(&q).unwrap()).unwrap();
						let ir = v["records"][0]["accounting"]["rnx_Ir"].as_u64().unwrap();
						v["records"][0]["accounting"]["rnx_Ir"] = json!(ir + 1);
						rnx_final::receipt::atomic_json(&q, &v).unwrap();
					}
					_ => std::fs::remove_dir_all(&entry).unwrap(),
				}
				st.record(
					&format!("damage: {mutation}"),
					json!({"ok": null}),
					Some(&c),
				);
				let r = outcome(lookup(&mut c, "key1"));
				st.record("lookup key1", r, Some(&c));
				let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
				st.record("republish key1", r, Some(&c));
				let r = outcome(c.lookup("key1").map(|l| l.1));
				st.record("lookup key1 again", r, Some(&c));
			},
		);
	}
	check(
		"one entry, a failed replacement keeps the previous",
		&|_, st, _, binary, receipt, h| {
			let mut c = h.open(&st.cache_root.clone()).unwrap();
			let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
			st.record("publish key1", r, Some(&c));
			let mut second = receipt.clone();
			second["capture_key"] = json!("key2");
			let r = outcome(lookup(&mut c, "key2"));
			st.record("lookup key2", r, Some(&c));
			h.arm(&[(Point::Copy, "disk")]);
			let r = outcome(c.publish(&second, binary, &json!({})));
			h.arm(&[]);
			st.record("publish key2 with a failing copy", r, Some(&c));
			let r = outcome(c.lookup("key1").map(|l| l.1));
			st.record("lookup key1 kept", r, Some(&c));
			let r = outcome(c.publish(&second, binary, &json!({})).map(|_| true));
			st.record("publish key2", r, Some(&c));
			let r = outcome(lookup(&mut c, "key1"));
			st.record("lookup key1 replaced", r, Some(&c));
		},
	);
	check(
		"a replaced or linked entry root is refused",
		&|_, st, _, binary, receipt, h| {
			let mut c = h.open(&st.cache_root.clone()).unwrap();
			let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
			st.record("publish key1", r, Some(&c));
			let p = PathBuf::from(c.state["entries"][0]["path"].as_str().unwrap());
			let old = p.with_file_name("saved");
			std::fs::rename(&p, &old).unwrap();
			std::fs::create_dir(&p).unwrap();
			st.record("replace the entry root", json!({"ok": null}), Some(&c));
			let r = outcome(lookup(&mut c, "key1"));
			st.record("lookup key1", r, Some(&c));
			std::fs::remove_dir(&p).unwrap();
			std::os::unix::fs::symlink(&old, &p).unwrap();
			st.record("link the entry root", json!({"ok": null}), Some(&c));
			let r = outcome(lookup(&mut c, "key1"));
			st.record("lookup key1 through the link", r, Some(&c));
		},
	);
	for (name, point, label, error) in [
		(
			"publication interrupted before the rename",
			Point::Rename,
			"publish key2, interrupted at the rename",
			"interrupted",
		),
		(
			"publication interrupted after the rename",
			Point::Renamed,
			"publish key2, interrupted after the rename",
			"interrupted",
		),
		(
			"publication interrupted after creating its pending directory",
			Point::Created,
			"publish key2, interrupted after creating pending",
			"interrupted",
		),
	] {
		check(name, &|_, st, src, binary, receipt, h| {
			let root = st.cache_root.clone();
			{
				let mut c = h.open(&root).unwrap();
				let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
				st.record("publish key1", r, Some(&c));
				let mut second = receipt.clone();
				second["capture_key"] = json!("key2");
				h.arm(&[(point, error), (Point::Cleanup, "killed")]);
				let r = outcome(c.publish(&second, binary, &json!({})));
				h.arm(&[]);
				st.record(label, r, Some(&c));
			}
			if point == Point::Rename {
				std::fs::remove_dir_all(src).unwrap();
			}
			let mut c = h.open(&root).unwrap();
			let r = outcome(c.lookup("key1").map(|l| l.1));
			st.record("recovered on the next lock", r, Some(&c));
		});
	}
	check(
		"an unknown journal schema is refused",
		&|_, st, _, _, _, h| {
			std::fs::create_dir(&st.cache_root).unwrap();
			rnx_final::receipt::atomic_json(
				&st.cache_root.join("OWNERS.json"),
				&json!({"schema": 2, "current": null, "entries": []}),
			)
			.unwrap();
			let r = outcome(h.open(&st.cache_root.clone()).map(|_| Value::Null));
			st.record("enter with an unknown journal schema", r, None);
		},
	);
	for (label, journal) in [
		(
			"entries null",
			json!({"schema": 1, "current": null, "entries": null}),
		),
		(
			"entries a number",
			json!({"schema": 1, "current": null, "entries": 5}),
		),
		(
			"an entry a number",
			json!({"schema": 1, "current": null, "entries": [5]}),
		),
		("no current", json!({"schema": 1, "entries": []})),
		(
			"an entry without an id",
			json!({"schema": 1, "current": null, "entries": [{"state": "complete"}]}),
		),
	] {
		check(
			&format!("a malformed journal: {label}"),
			&|_, st, _, _, _, h| {
				std::fs::create_dir(&st.cache_root).unwrap();
				let journal_path = st.cache_root.join("OWNERS.json");
				rnx_final::receipt::atomic_json(&journal_path, &journal).unwrap();
				let before = digest(&journal_path).unwrap();
				let r = outcome(h.open(&st.cache_root.clone()).map(|_| Value::Null));
				st.record("enter", r, None);
				let same = digest(&journal_path).unwrap() == before;
				st.record("the journal is unchanged", json!({"ok": same}), None);
			},
		);
	}
	for (label, records) in [
		("records null", Value::Null),
		("records a number", json!(5)),
		("records empty", json!([])),
		("a record a number", json!([5])),
	] {
		check(
			&format!("a malformed bundle: {label}"),
			&|_, st, _, binary, receipt, h| {
				let mut c = h.open(&st.cache_root.clone()).unwrap();
				let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
				st.record("publish key1", r, Some(&c));
				let entry = PathBuf::from(c.state["entries"][0]["path"].as_str().unwrap());
				let mut v: Value =
					serde_json::from_slice(&std::fs::read(entry.join("ANCHOR.json")).unwrap())
						.unwrap();
				v["records"] = records.clone();
				rnx_final::receipt::atomic_json(&entry.join("ANCHOR.json"), &v).unwrap();
				let r = outcome(Cache::verify(&entry, "key1").map(|_| true));
				st.record("verify the edited bundle", r, Some(&c));
			},
		);
	}
	check(
		"a stale row without a state",
		&|_, st, _, binary, receipt, h| {
			let entry = {
				let mut c = h.open(&st.cache_root.clone()).unwrap();
				let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
				st.record("publish key1", r, Some(&c));
				PathBuf::from(c.state["entries"][0]["path"].as_str().unwrap())
			};
			let journal_path = st.cache_root.join("OWNERS.json");
			let mut state: Value =
				serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
			state["current"] = Value::Null;
			state["entries"][0].as_object_mut().unwrap().remove("state");
			rnx_final::receipt::atomic_json(&journal_path, &state).unwrap();
			let before = digest(&journal_path).unwrap();
			let r = outcome(h.open(&st.cache_root.clone()).map(|_| Value::Null));
			st.record("enter with a stale row without a state", r, None);
			let same = digest(&journal_path).unwrap() == before;
			st.record("the journal is unchanged", json!({"ok": same}), None);
			st.record("the entry survives", json!({"ok": entry.is_dir()}), None);
		},
	);
	check(
		"a populated journal without a current",
		&|_, st, _, binary, receipt, h| {
			let entry = {
				let mut c = h.open(&st.cache_root.clone()).unwrap();
				let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
				st.record("publish key1", r, Some(&c));
				PathBuf::from(c.state["entries"][0]["path"].as_str().unwrap())
			};
			let journal_path = st.cache_root.join("OWNERS.json");
			let mut state: Value =
				serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
			state.as_object_mut().unwrap().remove("current");
			rnx_final::receipt::atomic_json(&journal_path, &state).unwrap();
			let before = digest(&journal_path).unwrap();
			let r = outcome(h.open(&st.cache_root.clone()).map(|_| Value::Null));
			st.record("enter with entries and no current", r, None);
			let same = digest(&journal_path).unwrap() == before;
			st.record("the journal is unchanged", json!({"ok": same}), None);
			st.record("the entry survives", json!({"ok": entry.is_dir()}), None);
		},
	);
	check(
		"a parent component in an ownership path",
		&|tmp, st, _, binary, receipt, h| {
			let r = outcome(h.open(&tmp.join("missing/../cache")).map(|_| Value::Null));
			st.record("open through missing/..", r, None);
			let fresh = tmp.join("fresh/cache");
			let r = outcome(
				h.open(&fresh)
					.map(|_| json!([true, fresh.join("LOCK").exists()])),
			);
			st.record("open a fresh nonexistent root", r, None);
			let mut c = h.open(&st.cache_root.clone()).unwrap();
			let r = outcome(c.publish(receipt, binary, &json!({})).map(|_| true));
			st.record("publish key1", r, Some(&c));
			let entry = PathBuf::from(c.state["entries"][0]["path"].as_str().unwrap());
			let twisted = entry
				.parent()
				.unwrap()
				.join("missing/..")
				.join(entry.file_name().unwrap());
			c.state["entries"][0]["path"] = json!(twisted.to_string_lossy());
			let journal_path = st.cache_root.join("OWNERS.json");
			rnx_final::receipt::atomic_json(&journal_path, &c.state).unwrap();
			let before = digest(&journal_path).unwrap();
			let r = outcome(lookup(&mut c, "key1"));
			st.record("lookup through missing/..", r, Some(&c));
			let same = digest(&journal_path).unwrap() == before;
			st.record("the journal is unchanged", json!({"ok": same}), Some(&c));
			let r = outcome(Cache::verify(&twisted, "key1").map(|_| true));
			st.record("verify through missing/..", r, Some(&c));
		},
	);
	assert_eq!(seen, expected.as_object().unwrap().len());
}

#[test]
fn split_capture_key_refuses_the_whole_map_cache_entry() {
	let temp = scratch();
	let (_, binary, mut receipt) = source(&temp.0, &policy());
	let sha = digest(&binary).unwrap();
	let mut recipe = json!({
		"cpu": ["test"], "environment": {}, "run_environment": {},
		"valgrind": "test", "fixtures": {}, "tooling": {}, "policy": {"schema": 1}
	});
	let key = |r: &Value| rnx_final::fixed_route_stage::capture_key(&sha, r).unwrap();
	let whole_key = key(&recipe);
	receipt["recipe"] = recipe.clone();
	receipt["capture_key"] = json!(whole_key);
	recipe["policy"]["schema"] = json!(2);
	let split_key = key(&recipe);
	assert_ne!(whole_key, split_key);

	let mut cache = Cache::open(&temp.0.join("cache")).unwrap();
	assert_eq!(cache.lookup(&split_key).unwrap(), (None, "absent"));
	let cached = cache.publish(&receipt, &binary, &json!({})).unwrap();
	assert_eq!(cache.lookup(&whole_key).unwrap().1, "hit");
	assert_eq!(cache.lookup(&split_key).unwrap(), (None, "key differs"));

	let profile = cached["records"][0]["profile"].as_str().unwrap();
	std::fs::write(profile, "changed profile").unwrap();
	assert_eq!(
		cache.lookup(&whole_key).unwrap(),
		(None, "verification failed")
	);
}

#[test]
fn canonical_cache_key_tracks_measured_launch_and_preserves_mount_provenance() {
	use rnx_final::fixed_route_stage::contextual_key;
	let sha = "a".repeat(64);
	let recipe = json!({"cpu":[],"environment":{},"run_environment":{},"valgrind":"v","fixtures":{},"tooling":{},"policy":{"schema":2}});
	let original = json!({"binary":"/build/rnx","vectors":"/original/vectors","cwd":"/checkout","valgrind_root":"/opt/valgrind","role":"baseline","launch_sha256":{"on":"original-digest"}});
	let key = contextual_key(&sha, &recipe, &original).unwrap();
	let mut reordered = original.clone();
	reordered["launch_sha256"]["on"] = json!("reordered-env-digest");
	assert_ne!(contextual_key(&sha, &recipe, &reordered).unwrap(), key);
	for (field, change) in [
		("binary", "/other/rnx"),
		("vectors", "/other/vectors"),
		("cwd", "/other"),
		("valgrind_root", "/other/valgrind"),
		("role", "candidate"),
	] {
		let mut paths = original.clone();
		paths[field] = json!(change);
		assert_eq!(contextual_key(&sha, &recipe, &paths).unwrap(), key);
		paths.as_object_mut().unwrap().remove(field);
		assert!(contextual_key(&sha, &recipe, &paths).is_err());
	}
}

#[test]
fn same_image_at_another_launch_configuration_is_incomparable_before_scoring() {
	use rnx_final::fixed_route_stage::contextual_key;
	let policy: Value =
		serde_json::from_slice(&std::fs::read(root().join(fr::POLICY)).unwrap()).unwrap();
	let expected = &policy["native_setup"]["launch_sha256"];
	let recipe = json!({"policy":{"schema":2,"launch_sha256":expected,"meter":policy["meter"],"outer_host_inputs":policy["installed_setup"]["outer_reference"]["host_inputs"]},"cpu":[],"environment":{},"run_environment":{},"valgrind":"v","fixtures":{},"tooling":{}});
	let mut paths = json!({"binary":"/test/rnx","vectors":"/test/vectors","role":"candidate","cwd":"/test","valgrind_root":"/test/vg","launch_sha256":expected});
	let image = "a".repeat(64);
	let mut r = json!({"status":"PASS","policy_sha256":identity(&policy).unwrap(),"revision":policy["anchor"],"recipe":recipe,"binary_sha256":image,"capture_paths":paths,"capture_key":contextual_key(&image,&recipe,&paths).unwrap()});
	// The reference binding is valid; absence of the capture matrix is detected later.
	let original = r.clone();
	assert!(
		fr::score(&r, &r, Some(&r), &policy)
			.unwrap_err()
			.to_string()
			.contains("case matrix")
	);
	for field in ["meter", "outer_host_inputs"] {
		let mut stale = original.clone();
		stale["recipe"]["policy"]
			.as_object_mut()
			.unwrap()
			.remove(field);
		stale["capture_key"] = json!(contextual_key(&image, &stale["recipe"], &paths).unwrap());
		assert_ne!(stale["capture_key"], original["capture_key"]);
		let error = fr::score(&stale, &r, Some(&r), &policy).unwrap_err();
		assert!(
			error.to_string().contains("frozen guest and host inputs"),
			"{error}"
		);
	}
	paths["vectors"] = json!("/other/vectors");
	paths["launch_sha256"]["on"] = json!("b".repeat(64));
	r["capture_paths"] = paths.clone();
	r["capture_key"] = json!(contextual_key(&image, &recipe, &paths).unwrap());
	assert_ne!(r["capture_key"], original["capture_key"]);
	let err = fr::score(&original, &r, Some(&original), &policy).unwrap_err();
	assert!(
		err.to_string()
			.contains("incomparable native setup capture"),
		"{err}"
	);
}

#[test]
fn a_cached_pair_retains_both_files_and_misses_on_either_digest_change() {
	use rnx_final::fixed_route_stage::contextual_key;
	let temp = scratch();
	let d = &temp.0;
	let runner = d.join("runner");
	let engine = d.join("shared.so");
	std::fs::write(&runner, "runner").unwrap();
	std::fs::write(&engine, "engine").unwrap();
	let sha = digest(&runner).unwrap();
	let recipe = json!({"cpu":[],"environment":{},"run_environment":{},"valgrind":"v","fixtures":{},"tooling":{},"policy":{"schema":2}});
	let paths = json!({"binary":runner,"vectors":d,"cwd":d,"valgrind_root":d,"role":"baseline","launch_sha256":{"on":"fixture"},
		"measured":{"version":1,"target":"x86_64-unknown-linux-gnu","runner":{"path":"/run/rnx-route/rnx","sha256":sha},
		"engine":{"path":"/run/rnx-route/libengine_image.so","sha256":digest(&engine).unwrap()}}});
	let key = contextual_key(&sha, &recipe, &paths).unwrap();
	let receipt = json!({"status":"PASS","records":[],"artifacts":{},"binary_sha256":sha,"engine_binary":engine,"capture_paths":paths,"capture_key":key,"recipe":recipe});
	let mut cache = Cache::open(&d.join("cache")).unwrap();
	let stored = cache.publish(&receipt, &runner, &json!({})).unwrap();
	std::fs::remove_file(engine).unwrap();
	assert_eq!(cache.lookup(&key).unwrap().1, "hit");
	for role in ["runner", "engine"] {
		let mut other = paths.clone();
		other["measured"][role]["sha256"] = json!("a".repeat(64));
		let changed = contextual_key(
			other["measured"]["runner"]["sha256"].as_str().unwrap(),
			&recipe,
			&other,
		)
		.unwrap();
		assert_eq!(cache.lookup(&changed).unwrap(), (None, "key differs"));
	}
	std::fs::write(stored["engine_binary"].as_str().unwrap(), "wrong").unwrap();
	assert_eq!(cache.lookup(&key).unwrap(), (None, "verification failed"));
}

#[test]
fn reused_anchor_accepts_declaration_and_band_changes_and_refuses_measurement_changes() {
	let policy: Value =
		serde_json::from_slice(&std::fs::read(root().join(fr::POLICY)).unwrap()).unwrap();
	let meas_sha = identity(&fr::measurement_policy(&policy).unwrap()).unwrap();
	let full_sha = identity(&policy).unwrap();
	let cand = json!("candidate");
	let base_recipe = json!({
		"cpu": ["cpu"],
		"environment": {},
		"run_environment": {},
		"valgrind": "v",
		"fixtures": {},
		"policy": {
			"schema": policy["schema"],
			"launch_sha256": policy["native_setup"]["launch_sha256"],
			"outer_host_inputs": policy["installed_setup"]["outer_reference"]["host_inputs"],
			"meter": policy["meter"],
			"per_record_per_mille": policy["per_record_per_mille"],
			"cumulative_per_mille": policy["cumulative_per_mille"]
		},
		"tooling": {},
		"build_environment": "old"
	});
	let sha = "a".repeat(64);
	let paths = json!({
		"binary": "/bin/rnx",
		"vectors": "/tmp/v",
		"cwd": "/tmp",
		"valgrind_root": "/tmp/vg",
		"role": "baseline",
		"launch_sha256": policy["native_setup"]["launch_sha256"]
	});
	let key = rnx_final::fixed_route_stage::contextual_key(&sha, &base_recipe, &paths).unwrap();
	let make_receipt = |rev: &Value, p_sha: &str, m_sha: &str, rec: &Value| {
		let mut rows = base_rows(&policy);
		for r in &mut rows {
			r["binary_sha256"] = json!(sha);
		}
		json!({
			"status": "PASS",
			"revision": rev,
			"recipe": rec,
			"policy_sha256": p_sha,
			"measurement_policy_sha256": m_sha,
			"binary_sha256": sha,
			"capture_paths": paths,
			"capture_key": key,
			"records": rows
		})
	};
	let mut anchor = make_receipt(&policy["anchor"], &full_sha, &meas_sha, &base_recipe);
	let mut next_policy = policy.clone();
	next_policy["source_transitions"] = json!([]);
	let mut extra_item = next_policy["inventory"][0].clone();
	extra_item["name"] = json!("extra_tracked_item");
	next_policy["inventory"]
		.as_array_mut()
		.unwrap()
		.push(extra_item);
	next_policy["anchor_transition"]["plan"] = json!("b".repeat(40));
	let next_full_sha = identity(&next_policy).unwrap();
	assert_ne!(next_full_sha, full_sha);
	assert_eq!(
		identity(&fr::measurement_policy(&next_policy).unwrap()).unwrap(),
		meas_sha
	);
	let base = make_receipt(&cand, &next_full_sha, &meas_sha, &base_recipe);
	let candidate = make_receipt(&cand, &next_full_sha, &meas_sha, &base_recipe);
	// Without reuse_proof, a mismatched full policy_sha256 is refused.
	let err = fr::score(&base, &candidate, Some(&anchor), &next_policy).unwrap_err();
	assert!(err.to_string().contains("stale policy"), "{err}");
	anchor["reuse_proof"] = json!({
		"current_recipe": base_recipe,
		"binary_sha256": sha,
		"current_build": {"sha256": sha}
	});
	// With reuse_proof and matching measurement_policy_sha256, policy validation
	// passes (scoring proceeds to the synthetic row observer check) while anchor
	// keeps recording the earlier full policy_sha256.
	assert_eq!(anchor["policy_sha256"], json!(full_sha));
	let err = fr::score(&base, &candidate, Some(&anchor), &next_policy).unwrap_err();
	assert!(
		err.to_string().contains("guest input observer"),
		"expected to pass policy/reuse checks; got: {err}"
	);

	// On a schema-1 policy (where base_rows are complete), score() returns Ok.
	let mut s1_policy = policy.clone();
	s1_policy["schema"] = json!(1);
	for k in [
		"meter",
		"native_setup",
		"installed_setup",
		"relative_relocations",
	] {
		s1_policy.as_object_mut().unwrap().remove(k);
	}
	let s1_full = identity(&s1_policy).unwrap();
	let s1_meas = identity(&fr::measurement_policy(&s1_policy).unwrap()).unwrap();
	let mut s1_next = s1_policy.clone();
	s1_next["source_transitions"] = json!([]);
	s1_next["anchor_transition"]["plan"] = json!("b".repeat(40));
	let s1_next_full = identity(&s1_next).unwrap();
	assert_ne!(s1_next_full, s1_full);
	let mut s1_anchor = make_receipt(&s1_policy["anchor"], &s1_full, &s1_meas, &base_recipe);
	s1_anchor["reuse_proof"] = anchor["reuse_proof"].clone();
	let s1_base = make_receipt(&cand, &s1_next_full, &s1_meas, &base_recipe);
	let s1_cand = make_receipt(&cand, &s1_next_full, &s1_meas, &base_recipe);
	fr::score(&s1_base, &s1_cand, Some(&s1_anchor), &s1_next).unwrap();

	// Mutating a measurement field (meter) must refuse reuse even with reuse_proof.
	let mut bad_policy = next_policy.clone();
	bad_policy["meter"]["tick_ns"] = json!(999);
	assert!(fr::score(&base, &candidate, Some(&anchor), &bad_policy).is_err());
}
