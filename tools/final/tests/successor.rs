//! Ports of the Python docs-successor controls (test_final_successor.DocsChange).
mod common;
use anyhow::Result;
use rnx_final::child::Done;
use rnx_final::controller::{self, Hooks, Settings, StageCall, docs_change};
use rnx_final::failure::Environment;
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::scheduler::{Resources, Targets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Repo {
	root: PathBuf,
	passed: String,
}

impl Repo {
	fn new() -> Self {
		let root = std::env::temp_dir().join(format!("rnx-final-successor-{}", invocation_id()));
		std::fs::create_dir(&root).unwrap();
		let mut repo = Self {
			root: root.canonicalize().unwrap(),
			passed: String::new(),
		};
		repo.git(&["init", "-q"]);
		repo.write(".gitignore", "final*/\ntarget/\n");
		repo.write("src/lib.rs", "a");
		repo.write("plans/adr/0001_x.md", "one");
		repo.commit("base");
		repo.write("src/lib.rs", "b");
		repo.write("plans/adr/0001_x.md", "two");
		repo.commit("impl");
		repo.passed = repo.head();
		repo
	}
	fn git(&self, args: &[&str]) -> String {
		let output = std::process::Command::new("git")
			.arg("-C")
			.arg(&self.root)
			.args([
				"-c",
				"user.name=a",
				"-c",
				"user.email=a@b",
				"-c",
				"commit.gpgsign=false",
			])
			.args(args)
			.output()
			.unwrap();
		assert!(
			output.status.success(),
			"{args:?}: {}",
			String::from_utf8_lossy(&output.stderr)
		);
		String::from_utf8(output.stdout).unwrap().trim().to_owned()
	}
	fn write(&self, name: &str, text: &str) {
		let p = self.root.join(name);
		std::fs::create_dir_all(p.parent().unwrap()).unwrap();
		std::fs::write(p, text).unwrap();
	}
	fn commit(&self, message: &str) {
		self.git(&["add", "-A"]);
		self.git(&["commit", "-q", "-m", message]);
	}
	fn head(&self) -> String {
		self.git(&["rev-parse", "HEAD"])
	}
	fn amend(&self, name: &str, text: &str) -> String {
		self.git(&["checkout", "-q", &self.passed]);
		self.write(name, text);
		self.git(&["add", "-A"]);
		self.git(&["commit", "-q", "--amend", "-m", "impl"]);
		self.head()
	}
}

impl Drop for Repo {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.root);
	}
}

#[test]
fn a_plans_only_amend_is_a_docs_change() {
	let repo = Repo::new();
	let amended = repo.amend("plans/adr/0001_x.md", "three");
	assert_eq!(
		docs_change(&repo.root, &repo.passed, &amended).unwrap(),
		["plans/adr/0001_x.md"]
	);
}

#[test]
fn amends_outside_plans_are_refused() {
	let repo = Repo::new();
	for (name, text) in [("src/lib.rs", "c"), ("src/plans/notes.md", "x")] {
		let amended = repo.amend(name, text);
		assert!(
			docs_change(&repo.root, &repo.passed, &amended)
				.unwrap_err()
				.to_string()
				.contains("plans/ only"),
			"{name}"
		);
	}
	assert!(
		docs_change(&repo.root, &repo.passed, &repo.passed)
			.unwrap_err()
			.to_string()
			.contains("plans/ only"),
		"an unchanged commit"
	);
}

#[test]
fn a_different_parent_is_refused() {
	let repo = Repo::new();
	repo.git(&["checkout", "-q", &repo.passed]);
	repo.write("plans/adr/0001_x.md", "later");
	repo.commit("next");
	assert!(
		docs_change(&repo.root, &repo.passed, &repo.head())
			.unwrap_err()
			.to_string()
			.contains("same parent")
	);
}

#[test]
fn moves_across_plans_are_refused_by_both_names() {
	let repo = Repo::new();
	for (from, to) in [
		("src/lib.rs", "plans/lib.rs"),
		("plans/adr/0001_x.md", "src/0001_x.md"),
	] {
		repo.git(&["checkout", "-q", &repo.passed]);
		repo.git(&["mv", from, to]);
		repo.git(&["commit", "-q", "--amend", "-m", "impl"]);
		let outside = if from.starts_with("src") { from } else { to };
		assert!(
			docs_change(&repo.root, &repo.passed, &repo.head())
				.unwrap_err()
				.to_string()
				.contains(outside),
			"{from} -> {to}"
		);
	}
}

/// Fake stages; records the scopes it was asked to select.
struct Fake(Mutex<Vec<String>>);
impl Hooks for Fake {
	fn head(&self, root: &Path) -> Option<String> {
		rnx_final::child::head(root)
	}
	fn disk_check(&self, _: &Path, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn select(
		&self,
		root: &Path,
		_: &Path,
		_: Option<&[String]>,
		scope: &str,
		_: &Environment,
	) -> Result<Value> {
		self.0.lock().unwrap().push(scope.into());
		Ok(common::selection(
			scope,
			json!(rnx_final::child::head(root)),
			json!([["commit-discipline", ["command"]]]),
		))
	}
	fn check_selection(&self, _: &Value, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn finalize(
		&self,
		_: &Value,
		_: &Map<String, Value>,
		_: &Path,
		_: &Environment,
	) -> Result<Value> {
		Ok(common::finalization())
	}
	fn resources(&self, _: &str, _: &[String], _: &Path, _: &Targets) -> Result<Resources> {
		Ok(["fake".to_string()].into())
	}
	fn weight(&self, _: &str, _: &[String], _: &Path) -> Result<usize> {
		Ok(1)
	}
	fn tools_identity(&self, _: &Settings) -> Result<Value> {
		Ok(json!({}))
	}
	fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
		Some((|| {
			std::fs::create_dir(&call.directory)?;
			let receipt = call.directory.join("RESULT.json");
			atomic_json(
				&receipt,
				&common::stage_receipt(true, 0, json!([]), json!({})),
			)?;
			Ok(Done {
				returncode: 0,
				validation: json!({"path": receipt, "sha256": digest(&receipt)?}),
				declared_output: None,
			})
		})())
	}
}

fn settings(root: &Path, out: &Path, scope: &str, hooks: Arc<dyn Hooks>) -> Settings {
	Settings {
		root: root.to_owned(),
		out: out.to_owned(),
		names: None,
		scope: scope.into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks,
		tokens: Some(2),
	}
}

#[test]
fn a_successor_runs_only_the_docs_selection_and_revalidates() {
	let repo = Repo::new();
	repo.git(&["checkout", "-q", &repo.passed]);
	let fake = Arc::new(Fake(Mutex::new(vec![])));
	let prior = repo.root.join("final-prior");
	let earlier = controller::run(&settings(&repo.root, &prior, "complete", fake.clone())).unwrap();
	assert_eq!(earlier["complete_final"], true, "{earlier}");
	let amended = repo.amend("plans/adr/0001_x.md", "three");
	let out = repo.root.join("final-successor");
	let r =
		controller::successor(&settings(&repo.root, &out, "docs", fake.clone()), &prior).unwrap();
	assert_eq!(
		(r["status"].clone(), r["candidate"].clone()),
		(json!("PASS"), json!(amended))
	);
	assert_eq!(
		*fake.0.lock().unwrap(),
		["complete", "docs"],
		"the successor reselected more than the docs scope"
	);
	let revalidated = controller::revalidate_successor(&out, Some(&repo.root)).unwrap();
	assert_eq!(
		(
			revalidated["complete_final"].clone(),
			revalidated["acceptance_scope"].clone()
		),
		(json!(true), json!("complete"))
	);
	// A changed prior final no longer revalidates its successor.
	let mut prior_result: Value =
		serde_json::from_slice(&std::fs::read(prior.join("RESULT.json")).unwrap()).unwrap();
	prior_result["seconds"] = json!(0);
	atomic_json(&prior.join("RESULT.json"), &prior_result).unwrap();
	assert!(
		controller::revalidate_successor(&out, Some(&repo.root))
			.unwrap_err()
			.to_string()
			.contains("prior final changed")
	);
}
