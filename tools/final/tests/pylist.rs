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

/// A test file without a route, and a route on a file that is not a test,
/// each fail; a deferred route must say why.
#[test]
fn a_test_file_names_a_route_and_only_a_test_file_does() {
	let base = |extra: &str| {
		let root = std::env::temp_dir().join(format!("rnx-final-routes-{}", invocation_id()));
		std::fs::create_dir(&root).unwrap();
		let repo = Repo(root, String::new());
		repo.git(&["init", "-q"]);
		repo.write("tools/a.py", "");
		repo.write("tools/test_a.py", "");
		repo.commit("base");
		let mut r = repo;
		r.1 = r.head();
		r.write(LIST, &format!("# list\n{extra}"));
		r.commit("list");
		r
	};
	let reason = "deferred: a planted reason that is long enough to count";
	let ok = base(&format!(
		"tools/a.py\tport\ntools/test_a.py\tport\t{reason}\n"
	));
	ok.check().unwrap();
	let missing = base("tools/a.py\tport\ntools/test_a.py\tport\n");
	assert!(missing.check().unwrap_err().contains("names its route"));
	let extra = base(&format!(
		"tools/a.py\tport\t{reason}\ntools/test_a.py\tport\t{reason}\n"
	));
	assert!(extra.check().unwrap_err().contains("only a test file"));
	let short = base("tools/a.py\tport\ntools/test_a.py\tport\tdeferred: later\n");
	assert!(short.check().unwrap_err().contains("states why"));
	let unknown = base("tools/a.py\tport\ntools/test_a.py\tport\tsomewhere\n");
	assert!(unknown.check().unwrap_err().contains("unknown route"));
}

/// Each route is held to the active declaration of the entry point it
/// names. In a copy of those entry points, a call commented out (its text
/// kept in the comment), a `TESTS` entry removed or given a wrong count,
/// and a command list without the file each break the route.
#[test]
fn a_route_must_be_declared_by_its_entry_point() {
	use rnx_final::routes::{Entrypoints, MEASURE, MEASURE_CHECKS, PREFLIGHT, SUPPLEMENTAL};
	let real = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let commands = rnx_final::catalogue::commands(&real.join("target/unused"), &real).unwrap();
	let jupyter = "tools/column/jupyter_controls.py";
	let run = format!("{SUPPLEMENTAL}/run.py");
	let copy = |edit: &dyn Fn(&str, String) -> String| -> PathBuf {
		let root = std::env::temp_dir().join(format!("rnx-final-routes-{}", invocation_id()));
		for file in [PREFLIGHT, MEASURE_CHECKS, run.as_str(), jupyter] {
			let target = root.join(file);
			std::fs::create_dir_all(target.parent().unwrap()).unwrap();
			let text = std::fs::read_to_string(real.join(file)).unwrap();
			std::fs::write(target, edit(file, text)).unwrap();
		}
		std::fs::create_dir_all(root.join(MEASURE)).unwrap();
		for entry in std::fs::read_dir(real.join(MEASURE)).unwrap().flatten() {
			if entry.file_type().unwrap().is_file() {
				std::fs::copy(entry.path(), root.join(MEASURE).join(entry.file_name())).unwrap();
			}
		}
		root
	};
	let holds = |root: &Path, path: &str, route: &str| {
		let e = Entrypoints::load_with(root, commands.clone(), route == "xtask measure-checks")
			.unwrap();
		e.error(path, route).is_none()
	};
	let cases: [(&str, &str, &str, &str, &str); 8] = [
		(
			PREFLIGHT,
			"ci/test_disk_space.py",
			"preflight",
			"\tsubprocess.run([sys.executable, \"-B\", \"-m\", \"unittest\", \"discover\", \"-s\", str(root / \"ci\"), \"-p\", \"test_disk_space.py\"], cwd=root, check=True)",
			"comment",
		),
		(
			MEASURE_CHECKS,
			"rune/tools/xtask/measure/test_callgrind_n1_cold.py",
			"xtask measure-checks",
			"\t(\"test_callgrind_n1_cold.py\", 10),",
			"comment",
		),
		(
			MEASURE_CHECKS,
			"rune/tools/xtask/measure/test_callgrind_n1_cold.py",
			"xtask measure-checks",
			"(\"test_callgrind_n1_cold.py\", 10)",
			"(\"test_callgrind_n1_cold.py\", 9)",
		),
		(
			MEASURE_CHECKS,
			"rune/tools/xtask/measure/test_callgrind_n1_cold.py",
			"xtask measure-checks",
			"(\"test_callgrind_n1_cold.py\", 10)",
			"(\"test_callgrind_n1_cold.py\", 0)",
		),
		(
			run.as_str(),
			"rune/tools/xtask/supplemental/test_runner.py",
			"workflow supplemental",
			"\tcontrols = execute([sys.executable, '-m', 'unittest', 'discover', '-s', str(HERE), '-p', 'test_*.py'], attempt, env, root, 120, label='controls')",
			"comment",
		),
		(
			jupyter,
			"tests/test_jupyter_execution.py",
			"final jupyter-kernel via tools/column/jupyter_controls.py",
			"\t\t\t[str(python),'-B','tests/test_jupyter_execution.py',str(ROOT/'jupyter/target/debug/rnx-jupyter'),str(ROOT/'target/debug/rnx')],",
			"comment",
		),
		(
			jupyter,
			"tests/test_jupyter_execution.py",
			"final jupyter-kernel via tools/column/jupyter_controls.py",
			"'tests/test_jupyter_execution.py'",
			"'tests/other.py'",
		),
		// The command gone from the declaration the wrapper runs, the path
		// kept in a list it never runs.
		(
			jupyter,
			"tests/test_jupyter_execution.py",
			"final jupyter-kernel via tools/column/jupyter_controls.py",
			"\t\t\t[str(python),'-B','tests/test_jupyter_execution.py',str(ROOT/'jupyter/target/debug/rnx-jupyter'),str(ROOT/'target/debug/rnx')],\n",
			"unused",
		),
	];
	for (file, path, route, old, new) in cases {
		// Unchanged, the route holds in the copy.
		let same = copy(&|_, text| text);
		assert!(holds(&same, path, route), "{path}: {route} before planting");
		let _ = std::fs::remove_dir_all(&same);
		let planted = copy(&|f, text| {
			if f != file {
				return text;
			}
			assert!(text.contains(old), "{f} lacks the planted line: {old}");
			if new == "unused" {
				return text.replacen(old, "", 1)
					+ "\nunused = ['tests/test_jupyter_execution.py']\n";
			}
			let replacement = if new == "comment" {
				let comment = if f.ends_with(".rs") { "//" } else { "#" };
				format!("{comment} {}", old.trim_start())
			} else {
				new.to_owned()
			};
			text.replacen(old, &replacement, 1)
		});
		assert!(
			!holds(&planted, path, route),
			"{path}: {route} survived {new}"
		);
		let _ = std::fs::remove_dir_all(&planted);
	}
	// In this checkout: stages that do not run a file, and the wrong entry points.
	let e = Entrypoints::load(&real, false).unwrap();
	for (path, route) in [
		("tools/test_engine_source.py", "final pin-controls"),
		("tools/test_engine_source.py", "final no-such-stage"),
		(
			"tools/column/compare/test_compare.py",
			"final controller-controls",
		),
		("tools/test_engine_source.py", "final controller-controls"),
		("ci/test_unrun.py", "preflight"),
		("tools/test_engine_source.py", "preflight"),
		(
			"rune/tools/xtask/column/test_cli_cache.py",
			"workflow supplemental",
		),
		(
			"rune/tools/xtask/measure/test_unlisted.py",
			"xtask measure-checks",
		),
	] {
		assert!(e.error(path, route).is_some(), "{path}: {route} held");
	}
	assert!(
		e.error(
			"tools/test_engine_source.py",
			"final engine-source-controls"
		)
		.is_none()
	);
	assert!(
		e.error("tools/column/test_identity.py", "final controller-controls")
			.is_none()
	);
}

/// Count discovery runs as measure-checks runs these files, isolated: a
/// module of the same name on `PYTHONPATH` and a `sitecustomize` there
/// change nothing.
#[test]
fn discovery_ignores_python_path_and_site_setup() {
	use rnx_final::routes::{MEASURE, discover, discover_with};
	let real = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.join(MEASURE);
	let fake = std::env::temp_dir().join(format!("rnx-final-shadow-{}", invocation_id()));
	std::fs::create_dir_all(&fake).unwrap();
	let shadow: String = (0..99)
		.map(|i| format!("\tdef test_{i}(self): pass\n"))
		.collect();
	std::fs::write(
		fake.join("test_callgrind_n1_cold.py"),
		format!("import unittest\nclass S(unittest.TestCase):\n{shadow}"),
	)
	.unwrap();
	std::fs::write(
		fake.join("sitecustomize.py"),
		"raise SystemExit('site setup ran')\n",
	)
	.unwrap();
	let names = vec!["test_callgrind_n1_cold.py".to_string()];
	let plain = discover(&real, &names).unwrap();
	let planted = discover_with(
		&real,
		&names,
		&[
			("PYTHONPATH", &*fake.to_string_lossy()),
			(
				"PYTHONSTARTUP",
				&*fake.join("sitecustomize.py").to_string_lossy(),
			),
		],
	)
	.unwrap();
	let _ = std::fs::remove_dir_all(&fake);
	assert_eq!(plain["test_callgrind_n1_cold.py"], Ok(10));
	assert_eq!(planted, plain);
}
