//! A standalone fixed-route stage stops on SIGTERM (ADR 0298), as its
//! Python gate's try/finally did: the running child's whole group is
//! drained, the gate sees `producer interrupted by signal 15` (and so
//! publishes its FAIL receipt), and no further child starts. Its own test
//! binary, since it installs the process's signal handlers.
use rnx_final::fixed_route_stage::{Host, Live};
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn sigterm_drains_the_running_child_and_refuses_the_next() {
	let tmp = std::env::temp_dir().join(format!(
		"rnx-final-fixed-interrupt-{}",
		rnx_final::receipt::invocation_id()
	));
	std::fs::create_dir(&tmp).unwrap();
	let groups = tmp.join("groups");
	std::fs::create_dir(&groups).unwrap();
	let fifo = tmp.join("ready");
	let c = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
	// SAFETY: mkfifo on a NUL-terminated path in the scratch directory.
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
	let sleeper = tmp.join("sleeper");
	let mut live = Live {
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		groups,
		env: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
		stop: Some(rnx_final::cross::interruptible().unwrap()),
	};
	let script = format!(
		"sleep 300 & echo $! > {sleeper}; echo ready > {fifo}; wait",
		sleeper = sleeper.display(),
		fifo = fifo.display()
	);
	let command: Vec<String> = ["/bin/sh", "-c", &script].map(String::from).into();
	// Readiness, then the signal: no timing guess.
	let reader = fifo.clone();
	let signaller = std::thread::spawn(move || {
		let ready = std::fs::read_to_string(reader).unwrap();
		assert_eq!(ready, "ready\n");
		// SAFETY: signalling this test's own process.
		unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
	});
	let log = std::fs::File::create(tmp.join("child.log")).unwrap();
	let result = live.child(&command, &tmp, log, Duration::from_secs(120));
	signaller.join().unwrap();
	let error = result.expect_err("the interrupted child must not pass");
	assert_eq!(
		rnx_final::child::message(&error),
		"producer interrupted by signal 15"
	);
	let pid: i32 = std::fs::read_to_string(&sleeper)
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	let alive = std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
		.is_ok_and(|c| c.starts_with("sleep"));
	assert!(!alive, "the interrupted child's group was not drained");
	// No further child starts once interrupted.
	let log = std::fs::File::create(tmp.join("next.log")).unwrap();
	let next = live.child(
		&["/bin/true".to_string()],
		&tmp,
		log,
		Duration::from_secs(10),
	);
	assert_eq!(
		rnx_final::child::message(&next.unwrap_err()),
		"producer interrupted by signal 15"
	);
	let _ = std::fs::remove_dir_all(&tmp);
}
