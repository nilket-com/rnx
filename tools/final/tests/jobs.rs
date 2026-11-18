//! `rnx-final jobs` (ADR 0295) over planted finals and a planted user
//! manager, each view checked against its recorded snapshot.
use rnx_final::jobs::{self, Manager, Unit, Units};
use serde_json::json;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}
fn scratch() -> Scratch {
	let d = std::env::temp_dir().join(format!(
		"rnx-final-jobs-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&d).unwrap();
	Scratch(d.canonicalize().unwrap())
}

struct Planted(Units);
impl Manager for Planted {
	fn units(&self) -> Units {
		self.0.clone()
	}
}

/// A fixed "now", so every elapsed time is exact.
fn now() -> SystemTime {
	SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000)
}

fn write(path: &Path, value: &serde_json::Value) {
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

/// A complete final directory selecting `names`, begun `ago` seconds
/// before now.
fn final_dir(dir: &Path, names: &[&str], ago: u64) {
	scoped_dir(dir, "complete", names, ago);
}

/// A final directory of `scope` selecting `names`, begun `ago` seconds
/// before now.
fn scoped_dir(dir: &Path, scope: &str, names: &[&str], ago: u64) {
	let stages: Vec<_> = names.iter().map(|n| json!([n, ["true"]])).collect();
	let selection = dir.join("SELECTION.json");
	write(
		&selection,
		&json!({"schema": 1, "scope": scope, "stages": stages}),
	);
	std::fs::File::options()
		.write(true)
		.open(&selection)
		.unwrap()
		.set_modified(now() - Duration::from_secs(ago))
		.unwrap();
}

/// Re-plant a final's start `ago` seconds before the real clock, for views
/// the binary renders at the real time.
fn began_real(dir: &Path, ago: u64) {
	std::fs::File::options()
		.write(true)
		.open(dir.join("SELECTION.json"))
		.unwrap()
		.set_modified(SystemTime::now() - Duration::from_secs(ago))
		.unwrap();
}

fn finished(dir: &Path, names: &[&str]) {
	for name in names {
		write(
			&dir.join(format!("{name}-child/RESULT.json")),
			&json!({"status": "PASS"}),
		);
	}
}

fn result(dir: &Path, status: &str, complete: bool, seconds: f64, failed: &[&str]) {
	let checks: Vec<_> = ["a", "b"]
		.iter()
		.chain(failed)
		.map(|n| json!({"name": n, "exit": if failed.contains(n) { 1 } else { 0 }}))
		.collect();
	write(
		&dir.join("RESULT.json"),
		&json!({"status": status, "complete_final": complete, "seconds": seconds, "checks": checks}),
	);
}

/// An earlier complete PASS whose schedule ended stages at 100, 500 and
/// 900 seconds.
fn reference(root: &Path) -> PathBuf {
	let dir = root.join("0294/FINAL A");
	final_dir(&dir, &["a", "b", "c"], 10_000);
	finished(&dir, &["a", "b", "c"]);
	result(&dir, "PASS", true, 2640.0, &[]);
	let stage = |name: &str, ended: f64| json!({"name": name, "state": "passed", "ready": 0.0, "waits": [], "started": 0.0, "ended": ended});
	write(
		&dir.join("SCHEDULE.json"),
		&json!({"schema": 1, "stages": [stage("a", 100.0), stage("b", 500.0), stage("c", 900.0)], "controller": []}),
	);
	dir
}

/// The running final, its path holding spaces: two stages done, one
/// running, an unselected stage's child and a stray file ignored.
fn running(root: &Path) -> PathBuf {
	let dir = root.join("0295/FINAL with space");
	final_dir(&dir, &["a", "b", "c", "d", "e"], 600);
	finished(&dir, &["a", "b"]);
	std::fs::create_dir_all(dir.join("c-child")).unwrap();
	std::fs::create_dir_all(dir.join("zz-child")).unwrap();
	std::fs::write(dir.join("notes.txt"), "unrelated").unwrap();
	dir
}

fn unit(name: &str, elapsed: Option<u64>, memory: Option<u64>, out: Option<&Path>) -> Unit {
	Unit {
		name: name.into(),
		elapsed: elapsed.map(Duration::from_secs),
		memory,
		out: out.map(Path::to_path_buf),
	}
}

fn shown(text: String, root: &Path) -> String {
	text.replace(&*root.to_string_lossy(), "<ROOT>")
}

#[test]
fn a_running_final_shows_its_stages_and_a_labeled_estimate() {
	let s = scratch();
	let root = &s.0;
	reference(root);
	let dir = running(root);
	// Unrelated build directories are not finals.
	std::fs::create_dir_all(root.join("debug/deps")).unwrap();
	let manager = Planted(Units::Running(vec![
		unit("rnx-0295-final", Some(600), Some(3 << 30), Some(&dir)),
		unit("rnx-cargo-build", None, None, None),
		unit(
			"rnx-elsewhere",
			Some(61),
			Some(1 << 30),
			Some(&root.join("not-a-final")),
		),
	]));
	assert_eq!(
		shown(jobs::snapshot(&manager, root, now()), root),
		"\
rnx-0295-final  running 10m, memory 3G
  final <ROOT>/0295/FINAL with space
  stages [############------------------] 2/5 done, 1 running: c
  time   [######------------------------] 10m / ~44m est
  against the last complete PASS, <ROOT>/0294/FINAL A (44m00s), which had 2 done by now

rnx-cargo-build  running ?, memory ?
  (not tied to a final)

rnx-elsewhere  running 1m, memory 1G
  (not tied to a final)
"
	);
}

/// Only a failed or incomplete earlier final, or only the final itself:
/// no reference, said plainly.
#[test]
fn without_an_earlier_complete_pass_there_is_no_estimate() {
	let s = scratch();
	let root = &s.0;
	let failed = root.join("0293/FINAL-x");
	final_dir(&failed, &["a"], 20_000);
	result(&failed, "FAIL", true, 100.0, &["a"]);
	let partial = root.join("0293/FINAL-required");
	final_dir(&partial, &["a"], 15_000);
	result(&partial, "PASS", false, 50.0, &[]);
	let dir = running(root);
	let manager = Planted(Units::Running(vec![unit(
		"rnx-run",
		Some(600),
		Some(0),
		Some(&dir),
	)]));
	assert_eq!(
		shown(jobs::snapshot(&manager, root, now()), root),
		"\
rnx-run  running 10m, memory 0G
  final <ROOT>/0295/FINAL with space
  stages [############------------------] 2/5 done, 1 running: c
  time   10m elapsed; no earlier complete PASS to compare with
"
	);
}

/// Nothing running: the latest final's verdict, its failed checks and its
/// receipt; a final that ended without a receipt says so.
#[test]
fn an_idle_view_shows_the_latest_verdict_or_its_absence() {
	let s = scratch();
	let root = &s.0;
	reference(root);
	let failed = root.join("0295/FINAL-v2");
	final_dir(&failed, &["a", "b", "final-controls"], 3000);
	finished(&failed, &["a", "b", "final-controls"]);
	result(&failed, "FAIL", true, 2032.4, &["final-controls"]);
	assert_eq!(
		shown(
			jobs::snapshot(&Planted(Units::Running(vec![])), root, now()),
			root
		),
		"\
no rnx-* services running

latest:
  final <ROOT>/0295/FINAL-v2
  FAIL in 33m52s, 3/3 stages done; failed: final-controls
  receipt <ROOT>/0295/FINAL-v2/RESULT.json
"
	);
	// Early termination: one stage done, one begun and left without a
	// receipt, no terminal receipt. With no active service, nothing is
	// called running.
	let stopped = root.join("0296/FINAL-v1");
	final_dir(&stopped, &["a", "b"], 1200);
	finished(&stopped, &["a"]);
	std::fs::create_dir_all(stopped.join("b-child")).unwrap();
	assert_eq!(
		shown(
			jobs::snapshot(
				&Planted(Units::Unavailable(
					"Failed to connect to bus: No medium found".into()
				)),
				root,
				now()
			),
			root
		),
		"\
user manager unavailable: Failed to connect to bus: No medium found

latest:
  final <ROOT>/0296/FINAL-v1
  stages [###############---------------] 1/2 done, 1 unfinished: b
  no terminal receipt; began 20m ago
"
	);
	let empty = scratch();
	assert_eq!(
		shown(
			jobs::snapshot(&Planted(Units::Running(vec![])), &empty.0, now()),
			&empty.0
		),
		"no rnx-* services running\n\nno finals under <ROOT>\n"
	);
}

/// A malformed selection is reported, never guessed at.
#[test]
fn an_unreadable_selection_is_reported() {
	let s = scratch();
	let dir = s.0.join("0295/FINAL-bad");
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("SELECTION.json"), "{").unwrap();
	assert_eq!(
		jobs::final_view(&dir, &[], now(), false),
		vec![
			format!("final {}", dir.display()),
			"no readable selection receipt".to_string()
		]
	);
}

/// The final's directory comes from argv boundaries, never from text a
/// space could split, and a relative one from the working directory.
#[test]
fn the_output_comes_from_exact_argv() {
	let argv = |a: &[&str]| -> Vec<OsString> { a.iter().map(OsString::from).collect() };
	assert_eq!(
		jobs::out_from_argv(
			&argv(&[
				"/bin/sh",
				"final.sh",
				"--complete",
				"--out",
				"/t/FINAL with space"
			]),
			None
		),
		Some(PathBuf::from("/t/FINAL with space"))
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--out=/t/a b"]), None),
		Some(PathBuf::from("/t/a b"))
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--out", "target/F"]), Some(Path::new("/work"))),
		Some(PathBuf::from("/work/target/F"))
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--out", "target/F"]), None),
		None
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--outside", "y"]), None),
		None
	);
	assert_eq!(jobs::out_from_argv(&argv(&["x", "--out"]), None), None);
	// Empty or conflicting values name nothing; never the cwd, never the next flag.
	assert_eq!(
		jobs::out_from_argv(
			&argv(&["x", "--out", "", "--complete"]),
			Some(Path::new("/work"))
		),
		None
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--out="]), Some(Path::new("/work"))),
		None
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--out", "/a", "--out=/b"]), None),
		None
	);
	assert_eq!(
		jobs::out_from_argv(&argv(&["x", "--out", "/a", "--out=/a"]), None),
		Some(PathBuf::from("/a"))
	);
	// cmdline keeps empty arguments in place.
	assert_eq!(
		jobs::argv_from_cmdline(b"sh\0--out\0\0--complete\0"),
		argv(&["sh", "--out", "", "--complete"])
	);
	assert_eq!(
		jobs::out_from_argv(
			&jobs::argv_from_cmdline(b"sh\0--out\0\0--complete\0"),
			Some(Path::new("/w"))
		),
		None
	);
	assert!(jobs::argv_from_cmdline(b"").is_empty());
}

#[test]
fn systemctl_show_is_read_unit_by_unit() {
	let units = jobs::parse_show(
		"Id=rnx-a.service\nMemoryCurrent=[not set]\nMainPID=0\n\nId=rnx-b.service\nActiveEnterTimestampMonotonic=5\n",
	);
	assert_eq!(units.len(), 2);
	assert_eq!(units[0]["MemoryCurrent"], "[not set]");
	assert_eq!(units[1]["ActiveEnterTimestampMonotonic"], "5");
}

/// The alias is valid shell and runs the built controller, never a build.
#[test]
fn the_alias_runs_the_built_controller() {
	let alias = Path::new(env!("CARGO_MANIFEST_DIR")).join("rnx-jobs");
	let text = std::fs::read_to_string(&alias).unwrap();
	assert!(!text.contains("cargo"), "the alias must not build");
	assert!(text.contains("jobs"));
	let status = std::process::Command::new("sh")
		.arg("-n")
		.arg(&alias)
		.status()
		.unwrap();
	assert!(status.success());
}

/// Bars keep their declared width at none done and past the estimate, and
/// a Cargo build directory is not searched for finals.
#[test]
fn bars_hold_their_width_and_build_caches_are_not_searched() {
	let s = scratch();
	let root = &s.0;
	let quick = root.join("0294/FINAL-quick");
	final_dir(&quick, &["a"], 10_000);
	finished(&quick, &["a"]);
	result(&quick, "PASS", true, 300.0, &[]);
	let dir = root.join("0295/FINAL-slow");
	final_dir(&dir, &["a", "b"], 600);
	let cached = root.join("debug/hidden");
	final_dir(&cached, &["a"], 1);
	std::fs::write(root.join("debug/.cargo-lock"), "").unwrap();
	assert_eq!(jobs::finals(root), vec![dir.clone(), quick.clone()]);
	let view = jobs::final_view(&dir, &jobs::finals(root), now(), true);
	assert_eq!(view[1], "stages [------------------------------] 0/2 done");
	assert_eq!(
		view[2],
		"time   [##############################] 10m / ~5m est"
	);
	for line in &view[1..3] {
		let bar = line.split('[').nth(1).unwrap().split(']').next().unwrap();
		assert_eq!(bar.chars().count(), 30, "{line}");
	}
}

/// The public entry from a fresh checkout: the alias, a prebuilt
/// controller and a scripted `systemctl` first on PATH, for an active
/// service whose main process names a final at a path with spaces, an
/// empty list, and a failed user manager. Nothing is built or started
/// beyond the planted process.
#[test]
fn the_alias_answers_from_a_fresh_checkout_over_a_scripted_manager() {
	let s = scratch();
	let root = &s.0;
	let finals = root.join("finals");
	began_real(&reference(&finals), 10_000);
	let dir = running(&finals);
	// The final's start, relative to the real clock this time.
	std::fs::File::options()
		.write(true)
		.open(dir.join("SELECTION.json"))
		.unwrap()
		.set_modified(SystemTime::now() - Duration::from_secs(600))
		.unwrap();
	let checkout = root.join("fresh checkout");
	let alias = checkout.join("tools/final/rnx-jobs");
	std::fs::create_dir_all(alias.parent().unwrap()).unwrap();
	std::fs::copy(
		Path::new(env!("CARGO_MANIFEST_DIR")).join("rnx-jobs"),
		&alias,
	)
	.unwrap();
	let bin = root.join("bin");
	std::fs::create_dir(&bin).unwrap();
	let systemctl = bin.join("systemctl");
	std::fs::write(
		&systemctl,
		"#!/bin/sh\ncase \"$MANAGER\" in\nfailed) echo 'Failed to connect to bus: No medium found' >&2; exit 1 ;;\nempty) exit 0 ;;\nesac\ncase \"$2\" in\nlist-units) echo 'rnx-0295-final.service loaded active running final' ;;\nshow) printf 'Id=rnx-0295-final.service\\nMemoryCurrent=3221225472\\nMainPID=%s\\n' \"$PLANTED_PID\" ;;\nesac\n",
	)
	.unwrap();
	std::fs::set_permissions(
		&systemctl,
		std::os::unix::fs::PermissionsExt::from_mode(0o755),
	)
	.unwrap();
	let mut process = std::process::Command::new("sh")
		.args(["-c", "sleep 60", "final.sh", "--complete", "--out"])
		.arg(&dir)
		.spawn()
		.unwrap();
	let run = |manager: &str| {
		let output = std::process::Command::new(&alias)
			.env_clear()
			.env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
			.env("RNX_FINAL_BIN", env!("CARGO_BIN_EXE_rnx-final"))
			.env("RNX_FINALS", &finals)
			.env("MANAGER", manager)
			.env("PLANTED_PID", process.id().to_string())
			.output()
			.unwrap();
		assert!(output.status.success(), "{output:?}");
		shown(String::from_utf8(output.stdout).unwrap(), &finals)
	};
	let active = run("active");
	let empty = run("empty");
	let failed = run("failed");
	let _ = process.kill();
	let _ = process.wait();
	assert_eq!(
		active,
		"\
rnx-0295-final  running ?, memory 3G
  final <ROOT>/0295/FINAL with space
  stages [############------------------] 2/5 done, 1 running: c
  time   [######------------------------] 10m / ~44m est
  against the last complete PASS, <ROOT>/0294/FINAL A (44m00s), which had 2 done by now
"
	);
	assert!(
		empty.starts_with("no rnx-* services running\n\nlatest:\n"),
		"{empty}"
	);
	assert!(
		failed.starts_with("user manager unavailable: Failed to connect to bus: No medium found\n"),
		"{failed}"
	);
	// The default path: the controller final.sh builds, in the checkout's
	// own target, which also holds the finals; nothing else is set.
	let built = checkout.join("target/final-controller/release/rnx-final");
	std::fs::create_dir_all(built.parent().unwrap()).unwrap();
	std::fs::copy(env!("CARGO_BIN_EXE_rnx-final"), &built).unwrap();
	let mine = checkout.join("target/0297/FINAL-v1");
	final_dir(&mine, &["a"], 60);
	finished(&mine, &["a"]);
	result(&mine, "PASS", true, 61.0, &[]);
	let default = std::process::Command::new(&alias)
		.env_clear()
		.env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
		.env("MANAGER", "empty")
		.output()
		.unwrap();
	assert!(default.status.success(), "{default:?}");
	assert_eq!(
		String::from_utf8(default.stdout).unwrap(),
		format!(
			"no rnx-* services running\n\nlatest:\n  final {mine}\n  PASS in 1m01s, 1/1 stages done\n  receipt {mine}/RESULT.json\n",
			mine = mine.display()
		)
	);
	std::fs::remove_file(&built).unwrap();
	// With no built controller and none named, the alias refuses; it never builds.
	let refused = std::process::Command::new(&alias)
		.env_clear()
		.env("PATH", "/usr/bin:/bin")
		.output()
		.unwrap();
	assert_eq!(refused.status.code(), Some(1));
	assert!(String::from_utf8_lossy(&refused.stderr).contains("no built rnx-final"));
}

fn fifo(path: &Path) {
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
	// SAFETY: mkfifo on a NUL-terminated path in the scratch directory.
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
}

/// The one-shot `--final` view reads receipts only as regular files: a
/// linked RESULT.json, FIFOs named as receipts and a FIFO selection are
/// unavailable, never followed or opened to block.
#[test]
fn only_regular_receipts_are_read() {
	let s = scratch();
	let root = &s.0;
	let bin = root.join("bin");
	std::fs::create_dir(&bin).unwrap();
	let systemctl = bin.join("systemctl");
	std::fs::write(&systemctl, "#!/bin/sh\nexit 0\n").unwrap();
	std::fs::set_permissions(
		&systemctl,
		std::os::unix::fs::PermissionsExt::from_mode(0o755),
	)
	.unwrap();
	let finals = root.join("finals");
	let reference = finals.join("0294/FINAL-ref");
	final_dir(&reference, &["a"], 10_000);
	began_real(&reference, 10_000);
	result(&reference, "PASS", true, 2640.0, &[]);
	fifo(&reference.join("SCHEDULE.json"));
	let external = root.join("external.json");
	write(
		&external,
		&json!({"status": "PASS", "complete_final": true, "seconds": 1.0, "checks": []}),
	);
	let linked = finals.join("0295/FINAL-linked");
	final_dir(&linked, &["a"], 600);
	std::os::unix::fs::symlink(&external, linked.join("RESULT.json")).unwrap();
	let piped = finals.join("0295/FINAL-piped");
	final_dir(&piped, &["a"], 600);
	fifo(&piped.join("RESULT.json"));
	// The binary reads the real clock: these two began ten minutes ago.
	for dir in [&linked, &piped] {
		std::fs::File::options()
			.write(true)
			.open(dir.join("SELECTION.json"))
			.unwrap()
			.set_modified(SystemTime::now() - Duration::from_secs(600))
			.unwrap();
	}
	let selection = finals.join("0295/FINAL-selection");
	fifo(&selection.join("SELECTION.json"));
	let view = |dir: &Path| {
		let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_rnx-final"))
			.args(["jobs", "--final"])
			.arg(dir)
			.env_clear()
			.env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
			.env("RNX_FINALS", &finals)
			.stdout(std::process::Stdio::piped())
			.spawn()
			.unwrap();
		let deadline = std::time::Instant::now() + Duration::from_secs(20);
		while child.try_wait().unwrap().is_none() {
			if std::time::Instant::now() > deadline {
				let _ = child.kill();
				panic!("the view blocked on {}", dir.display());
			}
			std::thread::sleep(Duration::from_millis(20));
		}
		let output = child.wait_with_output().unwrap();
		assert!(output.status.success());
		String::from_utf8(output.stdout)
			.unwrap()
			.replace(&*finals.to_string_lossy(), "<ROOT>")
	};
	assert_eq!(
		view(&linked),
		"final <ROOT>/0295/FINAL-linked\nstages [------------------------------] 0/1 done\nno terminal receipt; began 10m ago\n"
	);
	assert_eq!(
		view(&piped),
		"final <ROOT>/0295/FINAL-piped\nstages [------------------------------] 0/1 done\nno terminal receipt; began 10m ago\n"
	);
	assert_eq!(
		view(&selection),
		"final <ROOT>/0295/FINAL-selection\nno readable selection receipt\n"
	);
	// The reference's piped schedule gives no pace, and is never opened.
	let live = jobs::final_view(&piped, &jobs::finals(&finals), SystemTime::now(), true);
	assert_eq!(
		live[3],
		format!(
			"against the last complete PASS, {} (44m00s)",
			reference.display()
		)
	);
}

/// Like with like: a required final against the last required PASS of
/// the same stages, never a newer complete PASS or a required PASS of
/// other stages; a complete final against the last complete PASS, never a
/// newer required PASS. With no comparable PASS, no estimate.
#[test]
fn an_estimate_compares_like_with_like() {
	let s = scratch();
	let root = &s.0;
	let six = [
		"commit-discipline",
		"controller-controls",
		"python-list",
		"format-final",
		"clippy-final",
		"final-controls",
	];
	let required = root.join("0295/FINAL-REQUIRED-v1");
	scoped_dir(&required, "required", &six, 9000);
	finished(&required, &six);
	result(&required, "PASS", false, 138.0, &[]);
	let stage = |name: &str, ended: f64| json!({"name": name, "state": "passed", "ready": 0.0, "waits": [], "started": 0.0, "ended": ended});
	write(
		&required.join("SCHEDULE.json"),
		&json!({"schema": 1, "stages": [stage("commit-discipline", 30.0), stage("final-controls", 130.0)], "controller": []}),
	);
	// The same number of stages, one of them different.
	let mut swapped = six;
	swapped[5] = "jupyter-tests";
	let other = root.join("0295/FINAL-REQUIRED-other");
	scoped_dir(&other, "required", &swapped, 8000);
	finished(&other, &swapped);
	result(&other, "PASS", false, 60.0, &[]);
	let complete = reference(root);
	// A newer complete PASS than the required one, to be ignored by it.
	std::fs::File::options()
		.write(true)
		.open(complete.join("SELECTION.json"))
		.unwrap()
		.set_modified(now() - Duration::from_secs(5000))
		.unwrap();
	let running = root.join("0296/FINAL-REQUIRED-v2");
	scoped_dir(&running, "required", &six, 60);
	finished(&running, &six[..4]);
	std::fs::create_dir_all(running.join("final-controls-child")).unwrap();
	let all = jobs::finals(root);
	let view: Vec<String> = jobs::final_view(&running, &all, now(), true)
		.into_iter()
		.map(|l| l.replace(&*root.to_string_lossy(), "<ROOT>"))
		.collect();
	assert_eq!(
		view,
		vec![
			"final <ROOT>/0296/FINAL-REQUIRED-v2",
			"stages [####################----------] 4/6 done, 1 running: final-controls",
			"time   [#############-----------------] 1m / ~2m est",
			"against the last required PASS of the same 6 stages, <ROOT>/0295/FINAL-REQUIRED-v1 (2m18s), which had 1 done by now",
		]
	);
	// Only a required PASS of other stages: no estimate.
	std::fs::remove_dir_all(&required).unwrap();
	let view = jobs::final_view(&running, &jobs::finals(root), now(), true);
	assert_eq!(
		view[2],
		"time   1m elapsed; no earlier required PASS of the same 6 stages to compare with"
	);
	// A complete final ignores the newer required PASS.
	let newer = root.join("0297/FINAL-REQUIRED-v1");
	scoped_dir(&newer, "required", &six, 100);
	finished(&newer, &six);
	result(&newer, "PASS", false, 140.0, &[]);
	let full = root.join("0297/FINAL-RUST-v1");
	scoped_dir(&full, "complete", &["a", "b"], 600);
	let view: Vec<String> = jobs::final_view(&full, &jobs::finals(root), now(), true)
		.into_iter()
		.map(|l| l.replace(&*root.to_string_lossy(), "<ROOT>"))
		.collect();
	assert_eq!(
		view[2..],
		[
			"time   [######------------------------] 10m / ~44m est".to_string(),
			"against the last complete PASS, <ROOT>/0294/FINAL A (44m00s), which had 2 done by now"
				.to_string(),
			"no stage is running".to_string(),
		]
	);
	// Never a later PASS, an unknown scope, or an unusable duration.
	std::fs::remove_dir_all(&newer).unwrap();
	let later = root.join("0297/FINAL-REQUIRED-later");
	scoped_dir(&later, "required", &six, 30);
	finished(&later, &six);
	result(&later, "PASS", false, 90.0, &[]);
	let unknown = root.join("0296/FINAL-unknown");
	scoped_dir(&unknown, "nightly", &six, 7000);
	finished(&unknown, &six);
	result(&unknown, "PASS", false, 80.0, &[]);
	let zero = root.join("0296/FINAL-zero");
	scoped_dir(&zero, "required", &six, 6000);
	finished(&zero, &six);
	result(&zero, "PASS", false, 0.0, &[]);
	let view = jobs::final_view(&running, &jobs::finals(root), now(), true);
	assert_eq!(
		view[2],
		"time   1m elapsed; no earlier required PASS of the same 6 stages to compare with"
	);
	// An unknown scope is never estimated, even against its own kind.
	let unknown_running = root.join("0297/FINAL-unknown-running");
	scoped_dir(&unknown_running, "nightly", &six, 60);
	let view = jobs::final_view(&unknown_running, &jobs::finals(root), now(), true);
	assert_eq!(
		view[2],
		"time   1m elapsed; no earlier comparable PASS to compare with"
	);
}
