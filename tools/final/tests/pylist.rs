//! The Python list's planted negatives, in a throwaway history.
use rnx_final::pylist::{LIST, check};
use rnx_final::receipt::invocation_id;
use std::path::{Path, PathBuf};

struct Repo(PathBuf, String);

impl Repo {
	/// A baseline commit with two Python files and no list.
	fn new() -> Self {
		let root = std::env::temp_dir().join(format!("rnx-final-pylist-{}", invocation_id()));
		std::fs::create_dir(&root).unwrap();
		let mut repo = Self(root, String::new());
		repo.git(&["init", "-q"]);
		repo.write("tools/a.py", "");
		repo.write("probes/b.py", "");
		repo.commit("base");
		repo.1 = repo.head();
		repo
	}
	fn git(&self, args: &[&str]) -> String {
		let output = std::process::Command::new("git")
			.arg("-C")
			.arg(&self.0)
			.args([
				"-c",
				"user.name=t",
				"-c",
				"user.email=t@invalid",
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
	fn write(&self, path: &str, text: &str) {
		let p = self.0.join(path);
		std::fs::create_dir_all(p.parent().unwrap()).unwrap();
		std::fs::write(p, text).unwrap();
	}
	fn commit(&self, message: &str) {
		self.git(&["add", "-A"]);
		self.git(&["commit", "-q", "--allow-empty", "-m", message]);
	}
	fn head(&self) -> String {
		self.git(&["rev-parse", "HEAD"])
	}
	fn list(&self, rows: &[(&str, &str)]) {
		let text: String = rows.iter().map(|(p, d)| format!("{p}\t{d}\n")).collect();
		self.write(LIST, &format!("# list\n{text}"));
	}
	fn check(&self) -> Result<serde_json::Value, String> {
		check(&self.0, &self.1).map_err(|e| e.to_string())
	}
}

impl Drop for Repo {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

#[test]
fn the_first_list_holds_exactly_the_baseline() {
	let repo = Repo::new();
	repo.list(&[("tools/a.py", "port"), ("probes/b.py", "delete")]);
	repo.commit("list");
	assert_eq!(repo.check().unwrap()["files"], 2);
}

#[test]
fn a_tracked_file_missing_from_the_list_fails() {
	let repo = Repo::new();
	repo.list(&[("tools/a.py", "port")]);
	repo.commit("list");
	assert!(repo.check().unwrap_err().contains("not listed"));
}

#[test]
fn a_listed_file_that_is_not_tracked_fails() {
	let repo = Repo::new();
	repo.list(&[("tools/a.py", "port"), ("probes/b.py", "delete")]);
	repo.commit("list");
	repo.git(&["rm", "-q", "probes/b.py"]);
	repo.commit("delete without unlisting");
	assert!(repo.check().unwrap_err().contains("not tracked"));
	// Unlisting it with the deletion passes: the list shrank.
	repo.list(&[("tools/a.py", "port")]);
	repo.commit("unlist");
	assert_eq!(repo.check().unwrap()["removed"], 1);
}

#[test]
fn a_list_that_gains_a_file_fails() {
	let repo = Repo::new();
	repo.list(&[("tools/a.py", "port"), ("probes/b.py", "delete")]);
	repo.commit("list");
	repo.write("tools/new.py", "");
	repo.list(&[
		("tools/a.py", "port"),
		("probes/b.py", "delete"),
		("tools/new.py", "port"),
	]);
	repo.commit("new python");
	assert!(repo.check().unwrap_err().contains("may only shrink"));
}

#[test]
fn a_rename_counts_as_an_addition() {
	let repo = Repo::new();
	repo.list(&[("tools/a.py", "port"), ("probes/b.py", "delete")]);
	repo.commit("list");
	repo.git(&["mv", "tools/a.py", "tools/renamed.py"]);
	repo.list(&[("tools/renamed.py", "port"), ("probes/b.py", "delete")]);
	repo.commit("rename");
	assert!(repo.check().unwrap_err().contains("tools/renamed.py"));
}

#[test]
fn a_first_list_beyond_the_baseline_fails() {
	let repo = Repo::new();
	repo.write("tools/later.py", "");
	repo.list(&[
		("tools/a.py", "port"),
		("probes/b.py", "delete"),
		("tools/later.py", "port"),
	]);
	repo.commit("list with a file the baseline lacked");
	assert!(repo.check().unwrap_err().contains("tools/later.py"));
}

#[test]
fn an_unknown_disposition_fails() {
	let repo = Repo::new();
	repo.list(&[("tools/a.py", "keep"), ("probes/b.py", "delete")]);
	repo.commit("list");
	assert!(repo.check().unwrap_err().contains("unknown disposition"));
}

#[test]
fn this_checkout_passes() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	check(&root, rnx_final::pylist::BASELINE).unwrap();
}
