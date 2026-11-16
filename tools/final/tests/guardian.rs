//! Guardian controls: the release barriers, draining at normal end, on
//! timeout and on controller death, and identity before every signal.
use rnx_final::guardian::{self, Process, read_record, spawn, start_ticks};
use rnx_final::receipt::{Field, invocation_id};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const GUARDIAN: &str = env!("CARGO_BIN_EXE_rnx-final");

struct Dir(PathBuf);
impl Dir {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!("rnx-final-guardian-{}", invocation_id()));
		std::fs::create_dir(&path).unwrap();
		Self(path)
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		use std::os::unix::fs::PermissionsExt as _;
		let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn environment() -> Vec<(OsString, OsString)> {
	std::env::vars_os().collect()
}

fn log(dir: &Path) -> (std::fs::File, std::fs::File) {
	let file = std::fs::File::create(dir.join("stage.log")).unwrap();
	(file.try_clone().unwrap(), file)
}

fn sh(script: &str) -> Vec<String> {
	vec!["sh".into(), "-c".into(), script.into()]
}

fn read_pid(path: &Path) -> i32 {
	std::fs::read_to_string(path)
		.unwrap()
		.trim()
		.parse()
		.unwrap()
}

/// A recorded process is gone: it no longer exists with its start time.
fn gone(process: &Process) -> bool {
	!guardian::alive(process)
}

/// A recorded process has exited: gone, or a zombie its new parent has not
/// reaped yet.
fn exited(process: &Process) -> bool {
	gone(process)
		|| std::fs::read_to_string(format!("/proc/{}/stat", process.pid))
			.ok()
			.and_then(|s| {
				s.rsplit_once(')')
					.and_then(|(_, r)| r.split_whitespace().next().map(|x| x == "Z"))
			})
			.unwrap_or(true)
}

fn process(pid: i32) -> Process {
	Process {
		pid,
		started: start_ticks(pid).expect("process exists"),
	}
}

/// Wait until `path` exists, by the stage's own writes; bounded.
fn wait_for(path: &Path) {
	for _ in 0..500 {
		if path.exists() && std::fs::metadata(path).unwrap().len() > 0 {
			return;
		}
		std::thread::sleep(Duration::from_millis(10));
	}
	panic!("{} never appeared", path.display());
}

#[test]
fn a_stage_runs_once_released_and_its_status_is_kept() {
	let dir = Dir::new();
	let (out, err) = log(&dir.0);
	let marker = dir.0.join("ran");
	let guarded = spawn(
		Path::new(GUARDIAN),
		&dir.0,
		&sh(&format!("echo ran > {}; exit 3", marker.display())),
		&dir.0,
		&environment(),
		out,
		err,
	)
	.unwrap();
	let outcome = guarded.wait(None).unwrap();
	assert_eq!((outcome.status, outcome.stopped), (3, None));
	let record = read_record(&outcome.record).unwrap();
	assert!(matches!(record.stage, Field::Value(_)));
	assert_eq!(record.completion.value().unwrap().status, 3);
	assert!(marker.exists());
}

#[test]
fn an_unrecorded_guardian_never_starts_its_stage() {
	use std::os::unix::fs::PermissionsExt as _;
	let dir = Dir::new();
	let groups = dir.0.join("groups");
	std::fs::create_dir(&groups).unwrap();
	std::fs::set_permissions(&groups, std::fs::Permissions::from_mode(0o500)).unwrap();
	let marker = dir.0.join("ran");
	let (out, err) = log(&dir.0);
	let error = spawn(
		Path::new(GUARDIAN),
		&groups,
		&sh(&format!("echo ran > {}", marker.display())),
		&dir.0,
		&environment(),
		out,
		err,
	)
	.err()
	.expect("registration fails");
	assert!(
		format!("{error:#}").contains("record stage guardian"),
		"{error:#}"
	);
	assert!(!marker.exists(), "the stage ran without a record");
}

#[test]
fn a_guardian_whose_controller_never_releases_it_exits_unrun() {
	let dir = Dir::new();
	let marker = dir.0.join("ran");
	let record = dir.0.join("GUARD-x.json");
	// The write end is closed without a byte, as when the controller dies.
	let mut fds = [0; 2];
	assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
	let (read, write) = (fds[0], fds[1]);
	unsafe { libc::close(write) };
	let status = std::process::Command::new(GUARDIAN)
		.args([
			"guardian",
			"--release-fd",
			&read.to_string(),
			"--parent",
			&std::process::id().to_string(),
			"--record",
		])
		.arg(&record)
		.arg("--")
		.args(sh(&format!("echo ran > {}", marker.display())))
		.status()
		.unwrap();
	unsafe { libc::close(read) };
	assert_eq!(status.code(), Some(guardian::UNRELEASED));
	assert!(!marker.exists());
	// A guardian whose parent is not the one named also refuses.
	let status = std::process::Command::new(GUARDIAN)
		.args(["guardian", "--release-fd", "0", "--parent", "1", "--record"])
		.arg(&record)
		.arg("--")
		.args(sh(&format!("echo ran > {}", marker.display())))
		.status()
		.unwrap();
	assert_eq!(status.code(), Some(guardian::UNRELEASED));
	assert!(!marker.exists());
}

#[test]
fn a_detached_grandchild_is_gone_when_its_stage_ends() {
	let dir = Dir::new();
	let pidfile = dir.0.join("pid");
	let (out, err) = log(&dir.0);
	let script = format!(
		"setsid sleep 1000 & echo $! > {}; exit 0",
		pidfile.display()
	);
	let guarded = spawn(
		Path::new(GUARDIAN),
		&dir.0,
		&sh(&script),
		&dir.0,
		&environment(),
		out,
		err,
	)
	.unwrap();
	let outcome = guarded.wait(None).unwrap();
	assert_eq!(outcome.status, 0);
	let grandchild = read_pid(&pidfile);
	assert!(
		start_ticks(grandchild).is_none(),
		"detached grandchild {grandchild} survived its stage"
	);
	assert!(
		read_record(&outcome.record)
			.unwrap()
			.completion
			.value()
			.unwrap()
			.killed >= 1
	);
}

#[test]
fn a_timeout_stops_the_stage_and_drains_child_and_grandchild() {
	let dir = Dir::new();
	let pidfile = dir.0.join("pid");
	let (out, err) = log(&dir.0);
	let script = format!("sleep 1000 & echo $! > {}; sleep 1000", pidfile.display());
	let guarded = spawn(
		Path::new(GUARDIAN),
		&dir.0,
		&sh(&script),
		&dir.0,
		&environment(),
		out,
		err,
	)
	.unwrap();
	wait_for(&pidfile);
	let grandchild = process(read_pid(&pidfile));
	let outcome = guarded.wait(Some(Duration::from_millis(300))).unwrap();
	assert!(
		outcome.stopped.as_deref().unwrap().contains("timed out"),
		"{:?}",
		outcome.stopped
	);
	assert_eq!(outcome.status, 128 + libc::SIGTERM);
	assert!(gone(&grandchild));
}

#[test]
fn a_stopped_guardian_drains_and_records() {
	let dir = Dir::new();
	let pidfile = dir.0.join("pid");
	let (out, err) = log(&dir.0);
	let script = format!(
		"setsid sleep 1000 & echo $! > {}; sleep 1000",
		pidfile.display()
	);
	let guarded = spawn(
		Path::new(GUARDIAN),
		&dir.0,
		&sh(&script),
		&dir.0,
		&environment(),
		out,
		err,
	)
	.unwrap();
	wait_for(&pidfile);
	let grandchild = process(read_pid(&pidfile));
	guarded.stop().unwrap();
	let outcome = guarded.wait(None).unwrap();
	assert_eq!(outcome.status, 128 + libc::SIGTERM);
	assert!(gone(&grandchild));
	assert!(
		read_record(&outcome.record)
			.unwrap()
			.completion
			.value()
			.is_some()
	);
}

/// The controller is this test binary re-run as a child; it is SIGKILLed
/// while its stage and a detached grandchild run.
#[test]
fn a_killed_controller_leaves_nothing_running() {
	if let Ok(dir) = std::env::var("RNX_GUARDIAN_CONTROLLER") {
		let dir = PathBuf::from(dir);
		let (out, err) = log(&dir);
		let script = format!(
			"setsid sleep 1000 & echo $! > {}; sleep 1000",
			dir.join("pid").display()
		);
		let guarded = spawn(
			Path::new(GUARDIAN),
			&dir,
			&sh(&script),
			&dir,
			&environment(),
			out,
			err,
		)
		.unwrap();
		std::fs::write(
			dir.join("record"),
			guarded.record.to_string_lossy().as_bytes(),
		)
		.unwrap();
		let _ = guarded.wait(None);
		return;
	}
	let dir = Dir::new();
	let mut controller = std::process::Command::new(std::env::current_exe().unwrap())
		.args([
			"--exact",
			"a_killed_controller_leaves_nothing_running",
			"--nocapture",
			"--test-threads=1",
		])
		.env("RNX_GUARDIAN_CONTROLLER", &dir.0)
		.spawn()
		.unwrap();
	wait_for(&dir.0.join("pid"));
	wait_for(&dir.0.join("record"));
	let record_path = PathBuf::from(std::fs::read_to_string(dir.0.join("record")).unwrap());
	let grandchild = process(read_pid(&dir.0.join("pid")));
	let record = read_record(&record_path).unwrap();
	let guardian_process = record.guardian.clone();
	controller.kill().unwrap();
	controller.wait().unwrap();
	// The guardian drains on its parent's death; wait on it by pidfd.
	let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, guardian_process.pid, 0) } as i32;
	if fd >= 0 && guardian::alive(&guardian_process) {
		let owned = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
		assert!(
			guardian::wait_exit(&owned, Some(Duration::from_secs(60))).unwrap(),
			"guardian did not finish"
		);
	}
	assert!(
		gone(&grandchild),
		"detached grandchild survived the controller"
	);
	let record = read_record(&record_path).unwrap();
	assert!(
		record.completion.value().is_some(),
		"no completion record: {record:?}"
	);
}

/// The guardian dies after recording its stage but before releasing it:
/// the stage sees EOF and exits without running.
#[test]
fn a_guardian_that_dies_before_releasing_its_stage_leaves_it_unrun() {
	let dir = Dir::new();
	let marker = dir.0.join("ran");
	let record = dir.0.join("GUARD-fault.json");
	let row = rnx_final::guardian::Record {
		schema: 1,
		invocation: "fault".into(),
		guardian: Process { pid: 0, started: 0 },
		stage: Field::Null,
		completion: Field::Null,
	};
	rnx_final::receipt::atomic_json(&record, &row).unwrap();
	let mut fds = [0; 2];
	assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
	assert_eq!(unsafe { libc::write(fds[1], b"!".as_ptr().cast(), 1) }, 1);
	unsafe { libc::close(fds[1]) };
	let status = std::process::Command::new(GUARDIAN)
		.args([
			"guardian",
			"--release-fd",
			&fds[0].to_string(),
			"--parent",
			&std::process::id().to_string(),
			"--record",
		])
		.arg(&record)
		.arg("--")
		.args(sh(&format!("echo ran > {}", marker.display())))
		.env_remove("RNX_FINAL")
		.env("RNX_GUARDIAN_FAULT", "die-before-stage-release")
		.status()
		.unwrap();
	unsafe { libc::close(fds[0]) };
	assert_eq!(status.code(), Some(guardian::UNRELEASED + 1));
	let stage = read_record(&record)
		.unwrap()
		.stage
		.value()
		.cloned()
		.expect("the stage was recorded");
	// Its only writer died, so it reads EOF and exits; wait on it by pidfd.
	let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, stage.pid, 0) } as i32;
	if fd >= 0 {
		let owned = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
		assert!(
			guardian::wait_exit(&owned, Some(Duration::from_secs(10))).unwrap(),
			"the gated stage never saw EOF"
		);
	}
	// Exited: gone, or a zombie its new parent has not reaped yet.
	let state = std::fs::read_to_string(format!("/proc/{}/stat", stage.pid))
		.ok()
		.and_then(|s| {
			s.rsplit_once(')')
				.and_then(|(_, rest)| rest.split_whitespace().next().map(str::to_owned))
		});
	assert!(
		gone(&stage) || state.as_deref() == Some("Z"),
		"the stage is still running: {state:?}"
	);
	assert!(!marker.exists(), "the stage ran without its release");
}

/// A guardian that ignores its stop and never drains is killed, with its
/// own group, once the drain deadline after the stop request has passed;
/// the stage's three-hour timeout does not hold the controller.
#[test]
fn an_externally_stopped_guardian_that_hangs_is_killed_within_its_deadline() {
	use std::os::unix::fs::PermissionsExt as _;
	let dir = Dir::new();
	let fake = dir.0.join("hung-guardian");
	let pidfile = dir.0.join("child");
	std::fs::write(
		&fake,
		format!(
			"#!/bin/sh\ntrap '' TERM INT HUP\nsleep 1000 &\necho $! > {}\nwait\nwait\n",
			pidfile.display()
		),
	)
	.unwrap();
	std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
	let (out, err) = log(&dir.0);
	let mut guarded = spawn(&fake, &dir.0, &sh("true"), &dir.0, &environment(), out, err).unwrap();
	guarded.drain_deadline = Duration::from_millis(500);
	wait_for(&pidfile);
	let child = process(read_pid(&pidfile));
	let hung = guarded.guardian().clone();
	let stop = rnx_final::guardian::StopSignal::new().unwrap();
	let started = std::time::Instant::now();
	let outcome = std::thread::scope(|scope| {
		scope.spawn(|| {
			std::thread::sleep(Duration::from_millis(200));
			stop.raise();
		});
		guarded
			.wait_until(Some(Duration::from_secs(3 * 3600)), Some(&stop))
			.unwrap()
	});
	assert!(
		started.elapsed() < Duration::from_secs(10),
		"the hung guardian held the wait for {:?}",
		started.elapsed()
	);
	assert!(
		outcome
			.stopped
			.as_deref()
			.unwrap()
			.contains("did not drain"),
		"{:?}",
		outcome.stopped
	);
	assert!(exited(&hung), "the hung guardian survived");
	assert!(exited(&child), "the hung guardian's own child survived");
}
