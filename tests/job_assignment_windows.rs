//! Gates 4 and 15: observe the child before assignment, then ask Windows
//! about exact process/job objects. The observer hook is test-support only.
#![cfg(all(windows, feature = "test-support"))]
mod harness;

use std::fs::File;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Threading::*;

const LIMIT: Duration = Duration::from_secs(15);

struct Run {
	dir: PathBuf,
	rnx: Child,
	child: Option<OwnedHandle>,
	leaf: Option<OwnedHandle>,
	job: Option<OwnedHandle>,
}

fn opened(pid: u32) -> OwnedHandle {
	let handle = unsafe {
		OpenProcess(
			PROCESS_SYNCHRONIZE | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
			FALSE,
			pid,
		)
	};
	assert!(
		!handle.is_null(),
		"OpenProcess {pid}: {}",
		std::io::Error::last_os_error()
	);
	unsafe { OwnedHandle::from_raw_handle(handle) }
}

impl Run {
	fn start(fail: bool) -> Self {
		let dir = harness::scratch("job-assignment");
		let exe =
			serde_json::to_string(std::env::current_exe().unwrap().to_str().unwrap()).unwrap();
		let args =
			serde_json::to_string(&["--exact", "immediate_spawner_helper", "--nocapture"]).unwrap();
		let script = dir.join("run.rn");
		std::fs::write(&script, format!("pub fn main(_) {{ let r = host::process({exe}, {args}, 3000)?; println!(\"CALL timed_out={{}} cancelled={{}}\", r.timed_out, r.cancelled); Ok(()) }}")).unwrap();
		let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
		command
			.arg("run")
			.arg(script)
			.env("RNX_TEST_BEFORE_JOB_ASSIGNMENT", &dir)
			.env("RNX_ASSIGNMENT_HELPER_DIR", &dir)
			.env_remove("RNX_TEST_JOB_ASSIGNMENT_FAILS")
			.stdin(Stdio::null())
			.stdout(File::create(dir.join("stdout")).unwrap())
			.stderr(File::create(dir.join("stderr")).unwrap());
		if fail {
			command.env("RNX_TEST_JOB_ASSIGNMENT_FAILS", "1");
		}
		let rnx = command.spawn().unwrap();
		let mut run = Self {
			dir,
			rnx,
			child: None,
			leaf: None,
			job: None,
		};
		assert!(
			harness::appeared(&run.dir.join("created"), LIMIT),
			"child was not published"
		);
		// The hook renames complete contents into place; own both observation
		// handles before releasing assignment.
		let text = harness::wrote(&run.dir.join("created")).unwrap();
		let values = text
			.split_whitespace()
			.map(str::parse::<usize>)
			.collect::<Result<Vec<_>, _>>()
			.unwrap();
		assert_eq!(values.len(), 2);
		run.child = Some(opened(values[0] as u32));
		let mut job = std::ptr::null_mut();
		assert_ne!(
			unsafe {
				DuplicateHandle(
					run.rnx.as_raw_handle(),
					values[1] as HANDLE,
					GetCurrentProcess(),
					&mut job,
					0,
					FALSE,
					DUPLICATE_SAME_ACCESS,
				)
			},
			FALSE,
			"duplicate exact rnx job: {}",
			std::io::Error::last_os_error()
		);
		run.job = Some(unsafe { OwnedHandle::from_raw_handle(job) });
		run
	}

	fn release(&self) {
		std::fs::write(self.dir.join("assign"), b"go").unwrap();
	}

	fn belongs(&self, process: &OwnedHandle) -> bool {
		let mut answer = FALSE;
		assert_ne!(
			unsafe {
				IsProcessInJob(
					process.as_raw_handle(),
					self.job.as_ref().unwrap().as_raw_handle(),
					&mut answer,
				)
			},
			FALSE
		);
		answer != FALSE
	}
}

impl Drop for Run {
	fn drop(&mut self) {
		let mut trouble = Vec::new();
		if let Some(job) = &self.job {
			unsafe {
				TerminateJobObject(job.as_raw_handle(), 1);
			}
		}
		for handle in [&self.leaf, &self.child].into_iter().flatten() {
			unsafe {
				TerminateProcess(handle.as_raw_handle(), 1);
			}
			let ended = unsafe { WaitForSingleObject(handle.as_raw_handle(), 15000) };
			if ended != WAIT_OBJECT_0 {
				trouble.push(format!(
					"assignment fixture process survived cleanup: {ended}"
				));
			}
		}
		trouble.extend(harness::killed_within(&mut self.rnx, LIMIT));
		let _ = std::fs::remove_dir_all(&self.dir);
		if !std::thread::panicking() {
			assert!(trouble.is_empty(), "{trouble:?}");
		} else if !trouble.is_empty() {
			eprintln!("assignment fixture cleanup: {trouble:?}");
		}
	}
}

// A suspend-count observation is deterministic: SuspendThread returns the
// previous count. Immediately undo our increment, preserving rnx's state.
fn suspension_counts(pid: u32) -> Vec<u32> {
	let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
	assert_ne!(snapshot, INVALID_HANDLE_VALUE);
	let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
	let mut entry = THREADENTRY32 {
		dwSize: size_of::<THREADENTRY32>() as u32,
		..Default::default()
	};
	let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
	let mut counts = Vec::new();
	while found != FALSE {
		if entry.th32OwnerProcessID == pid {
			let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, FALSE, entry.th32ThreadID) };
			assert!(!thread.is_null());
			let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
			let count = unsafe { SuspendThread(thread.as_raw_handle()) };
			assert_ne!(count, u32::MAX);
			assert_ne!(unsafe { ResumeThread(thread.as_raw_handle()) }, u32::MAX);
			counts.push(count);
		}
		found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
	}
	counts
}

#[test]
fn assignment_precedes_the_first_descendant() {
	let mut run = Run::start(false);
	let child = run.child.as_ref().unwrap();
	assert!(!run.belongs(child), "observer must run before assignment");
	let counts = suspension_counts(unsafe { GetProcessId(child.as_raw_handle()) });
	assert!(
		!counts.is_empty() && counts.iter().all(|count| *count > 0),
		"child was able to execute before assignment: {counts:?}"
	);
	run.release();
	assert!(
		harness::appeared(&run.dir.join("leaf"), LIMIT),
		"first descendant did not start"
	);
	let pid: u32 = harness::wrote(&run.dir.join("leaf"))
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	run.leaf = Some(opened(pid));
	assert!(
		run.belongs(run.child.as_ref().unwrap()),
		"direct child is outside the exact rnx job"
	);
	assert!(
		run.belongs(run.leaf.as_ref().unwrap()),
		"immediate descendant is outside the exact rnx job"
	);
	assert_eq!(harness::reaped_within(&mut run.rnx, LIMIT), Some(Some(0)));
	assert!(
		harness::wrote(&run.dir.join("stdout"))
			.unwrap()
			.contains("CALL timed_out=true cancelled=false")
	);
	for handle in [&run.child, &run.leaf].into_iter().flatten() {
		assert_eq!(
			unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) },
			WAIT_OBJECT_0,
			"deadline left a process alive"
		);
	}
}

#[test]
fn failed_assignment_collects_the_suspended_child() {
	let mut run = Run::start(true);
	assert!(!run.belongs(run.child.as_ref().unwrap()));
	assert_eq!(
		unsafe { WaitForSingleObject(run.child.as_ref().unwrap().as_raw_handle(), 0) },
		WAIT_TIMEOUT
	);
	run.release();
	assert_eq!(harness::reaped_within(&mut run.rnx, LIMIT), Some(Some(1)));
	assert!(
		harness::wrote(&run.dir.join("stderr"))
			.unwrap()
			.contains("the job assignment was told to fail")
	);
	assert_eq!(
		unsafe { WaitForSingleObject(run.child.as_ref().unwrap().as_raw_handle(), 0) },
		WAIT_OBJECT_0,
		"failed assignment left a suspended child"
	);
	assert!(
		!run.dir.join("leaf").exists(),
		"failed assignment resumed the child"
	);
}

#[test]
fn immediate_spawner_helper() {
	let Some(dir) = std::env::var_os("RNX_ASSIGNMENT_HELPER_DIR") else {
		return;
	};
	let mut leaf = Command::new(std::env::current_exe().unwrap())
		.args(["--exact", "leaf_helper", "--nocapture"])
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.unwrap();
	let dir = PathBuf::from(dir);
	// Rename publishes complete contents so the observer can parse immediately.
	std::fs::write(dir.join("leaf.tmp"), leaf.id().to_string()).unwrap();
	std::fs::rename(dir.join("leaf.tmp"), dir.join("leaf")).unwrap();
	std::thread::sleep(Duration::from_secs(30));
	assert!(harness::killed_within(&mut leaf, LIMIT).is_none());
}

#[test]
fn leaf_helper() {
	if std::env::var_os("RNX_ASSIGNMENT_HELPER_DIR").is_some() {
		std::thread::sleep(Duration::from_secs(30));
	}
}
