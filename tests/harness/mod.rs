//! A harness whose every wait has a bound, for gates about things that hang.
//!
//! Shared by more than one test binary, and each of them uses part of it, so
//! what is unused in one is live in another: the dead-code warning would be
//! about the file's readers rather than about the file.
//!
//! A test of a hang cannot be allowed to hang: a blocking wait in the harness
//! turns a regression into a stopped suite, and one that runs before cleanup
//! turns it into a stopped suite with processes left behind. So every step
//! here — the handshakes, the receive, the reap, the output — is bounded, and
//! nothing is judged until the teardown has run.
//!
//! Shared by more than one test binary, and each of them uses part of it, so
//! what is unused in one is live in another: the dead-code warning would be
//! about the file's readers rather than about the file.
#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A directory of this test's own, for the handshake files.
pub fn scratch(tag: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-{tag}-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// A child whose escaped descendant takes the read end of standard input,
/// says so, holds it until released, and says when it has let go.
///
/// Three handshakes, no delays. `ready` means the descriptor is held — which
/// needs `exec 3<&0`, because a non-interactive shell gives a backgrounded
/// command `/dev/null` and a descendant that never held the pipe proves
/// nothing. `release` is the test's signal to let go. `closed` means the pipe is
/// **already released**, and it takes closing **two** descriptors to mean
/// that: `<&3` makes the descendant's standard input a duplicate, and it
/// inherits fd 3 itself as well, so closing standard input alone left the
/// pipe held by a descriptor nobody was looking at — an acknowledgment that
/// was not true. Both go before `closed` is written.
///
/// The descendant also gives up on its own after about thirty seconds. If a
/// test dies without releasing it, that is the bound on what it leaves behind.
pub fn holds_stdin_until_released(dir: &Path) -> String {
	let ready = dir.join("ready").display().to_string();
	let release = dir.join("release").display().to_string();
	let closed = dir.join("closed").display().to_string();
	format!(
		"exec 3<&0; \
		 setsid sh -c 'touch {ready}; i=0; \
		 while [ ! -e {release} ] && [ $i -lt 1500 ]; do sleep 0.02; i=$((i+1)); done; \
		 exec 0<&- 3<&-; touch {closed}' <&3 >/dev/null 2>&1 & \
		 while [ ! -e {ready} ]; do sleep 0.02; done"
	)
}

/// The same, and the direct child stays too: it waits to be released rather
/// than exiting once the descendant has the descriptor. That is what leaves a
/// deadline or a cancellation something to end, with the descendant still
/// holding the read end after the process group is killed.
pub fn holds_stdin_and_stays(dir: &Path) -> String {
	let ready = dir.join("ready").display().to_string();
	let release = dir.join("release").display().to_string();
	let closed = dir.join("closed").display().to_string();
	format!(
		"exec 3<&0; \
		 setsid sh -c 'touch {ready}; i=0; \
		 while [ ! -e {release} ] && [ $i -lt 1500 ]; do sleep 0.02; i=$((i+1)); done; \
		 exec 0<&- 3<&-; touch {closed}' <&3 >/dev/null 2>&1 & \
		 while [ ! -e {ready} ]; do sleep 0.02; done; \
		 i=0; while [ ! -e {release} ] && [ $i -lt 1500 ]; do sleep 0.02; i=$((i+1)); done"
	)
}

/// Wait for a handshake file, or give up.
pub fn appeared(path: &Path, within: Duration) -> bool {
	let deadline = Instant::now() + within;
	while Instant::now() < deadline {
		if path.exists() {
			return true;
		}
		std::thread::sleep(Duration::from_millis(10));
	}
	false
}

/// The first line a reader produces, waited for with a bound. `BufRead::lines`
/// has none, which is why this exists.
pub fn line_within(
	from: impl std::io::Read + Send + 'static,
	within: Duration,
) -> Result<String, std::sync::mpsc::RecvTimeoutError> {
	let (tx, rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		use std::io::BufRead;
		let mut first = String::new();
		let _ = std::io::BufReader::new(from).read_line(&mut first);
		let _ = tx.send(first.trim_end().to_owned());
	});
	rx.recv_timeout(within)
}

/// Everything a reader has to say, with a bound: a pipe a live descendant
/// still holds would otherwise never reach the end of the file.
///
/// A timeout and a read failure are reported rather than returned as nothing.
/// Empty output is a thing a test may assert about, so a stuck pipe must not
/// be able to satisfy that assertion.
pub fn read_within(
	from: impl std::io::Read + Send + 'static,
	within: Duration,
) -> Result<String, String> {
	let (tx, rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let mut all = String::new();
		let mut from = from;
		let outcome = from.read_to_string(&mut all).map(|_| all);
		let _ = tx.send(outcome);
	});
	match rx.recv_timeout(within) {
		Ok(Ok(all)) => Ok(all),
		Ok(Err(e)) => Err(format!("a stream could not be read: {e}")),
		Err(_) => Err(format!("a stream was still open after {within:?}")),
	}
}

/// Wait for a process to exit, with a bound, without blocking on it.
pub fn reaped_within(child: &mut Child, within: Duration) -> Option<Option<i32>> {
	let deadline = Instant::now() + within;
	loop {
		match child.try_wait() {
			Ok(Some(status)) => return Some(status.code()),
			Ok(None) => {}
			Err(_) => return None,
		}
		if Instant::now() >= deadline {
			return None;
		}
		std::thread::sleep(Duration::from_millis(10));
	}
}

/// What a teardown could not take down. Empty is the good case.
pub type Trouble = Vec<String>;

/// How a run ended, once everything has been taken down.
pub struct Ended {
	/// Anything that survived. A caller should fail on it.
	pub trouble: Trouble,
	/// The status it exited with, if it exited on its own rather than being
	/// killed for outstaying its bound.
	pub code: Option<i32>,
	pub out: String,
	pub err: String,
}

/// Take the runner and the handshake down, bounding every step, and report
/// anything that survived so a caller can fail on it rather than leave it
/// unsaid.
///
/// The order matters. The descendant is released and **acknowledged** before
/// the handshake files are removed: deleting them while it is still polling
/// would leave it waiting for a file that can never appear, which is a stray
/// process for as long as its own patience lasts.
pub fn teardown(mut child: Child, dir: &Path) -> Ended {
	let mut trouble = Trouble::new();

	// The run may be waiting on its standard input; let it go.
	drop(child.stdin.take());

	// Let the descendant go, and wait for it to say the descriptor is closed.
	// A run that never started one has nothing to acknowledge, and waiting for
	// a handshake that cannot come is its own kind of unbounded.
	let _ = std::fs::write(dir.join("release"), b"");
	let expecting = dir.join("ready").exists();
	let acknowledged = !expecting || appeared(&dir.join("closed"), Duration::from_secs(5));
	if !acknowledged {
		trouble.push("the descendant never acknowledged closing the read end".to_owned());
	}

	// The runner: a bounded wait, then a kill, then a bounded reap.
	let mut code = None;
	match reaped_within(&mut child, Duration::from_secs(3)) {
		Some(status) => code = status,
		None => {
			let _ = child.kill();
			if reaped_within(&mut child, Duration::from_secs(3)).is_none() {
				trouble.push("the runner did not exit even after being killed".to_owned());
			}
		}
	}

	// Only now is it safe to read to the end: whatever held the pipes is gone,
	// and the read is bounded anyway.
	// A stream that could not be read is trouble, not empty output: a caller
	// asserting emptiness must not be satisfied by a pipe nobody could read.
	let mut collected = |read: Option<Result<String, String>>, which: &str| -> String {
		match read {
			None => String::new(),
			Some(Ok(text)) => text,
			Some(Err(why)) => {
				trouble.push(format!("{which}: {why}"));
				String::new()
			}
		}
	};
	let out = collected(
		child
			.stdout
			.take()
			.map(|p| read_within(p, Duration::from_secs(3))),
		"standard output",
	);
	let err = collected(
		child
			.stderr
			.take()
			.map(|p| read_within(p, Duration::from_secs(3))),
		"standard error",
	);

	if acknowledged {
		let _ = std::fs::remove_dir_all(dir);
	} else {
		trouble.push(format!(
			"left {} in place, so the descendant can still find `release`",
			dir.display()
		));
	}
	Ended {
		trouble,
		code,
		out,
		err,
	}
}
