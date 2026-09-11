//! Where a session's history goes, asked of the program rather than of the
//! function that computes the path.
//!
//! Record 0025 decision 7. The defect this exists to catch was not a wrong
//! path: it was `None`, on Windows, because the code asked for `HOME` and
//! Windows has none. A test that reads `history_path` back would have agreed
//! with the code and passed throughout, and the two checks that first
//! accompanied the fix did exactly that — one restated the join, the other
//! accepted `None`, which is the failure itself. Both were deleted.
//!
//! So these run the binary in an environment of their own and look at the
//! filesystem afterwards. Three rules, each earned rather than assumed:
//!
//! - **Every variable the lookup reads is set here, `HOME` included.** A
//!   regression that ignored `XDG_STATE_HOME` would otherwise append to the
//!   real home directory's history — the developer's own file, edited by a
//!   test that then failed. `HOME` points at scratch space so the worst a
//!   regression can do is write where this test can see it.
//! - **The child is bounded, and a timeout is a failure.** A session that
//!   does not end is killed and collected rather than left to hang the
//!   suite.
//! - **Cleanup runs before assertions.** A panic between them would leave
//!   scratch directories behind on every failing run, so what went wrong is
//!   carried back and asserted last.
//!
//! On Windows, gate 18 also starts a second interactive console session and
//! recalls the saved expression with Up, then checks its evaluated result.

mod harness;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// How long a session that only has to quit may take. Generous, because it
/// bounds a hang rather than measuring anything.
const LIMIT: Duration = Duration::from_secs(20);

/// The input the session is given before it quits, so the file can be asked
/// what it holds rather than only whether it exists.
const MARKER: &str = "40 + 2";

/// A directory of this test's own.
fn scratch(what: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-history-{}-{what}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// The variable this platform keeps per-user state under. The names differ;
/// what is asked of them does not.
#[cfg(windows)]
const STATE_VAR: &str = "LOCALAPPDATA";
#[cfg(unix)]
const STATE_VAR: &str = "XDG_STATE_HOME";

/// Everything the lookup consults, cleared before anything is set, so no
/// variable of the runner's reaches the child.
const CONSULTED: [&str; 4] = ["RNX_HISTORY", "XDG_STATE_HOME", "LOCALAPPDATA", "HOME"];

/// Run a session that evaluates `MARKER` and quits, in an environment built
/// from `env` alone. Answers what went wrong, if anything.
fn session_with(home: &Path, env: &[(&str, &Path)]) -> Result<(), String> {
	let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
	command.arg("repl");
	for name in CONSULTED {
		command.env_remove(name);
	}
	// Always somewhere of this test's own, so a lookup that falls back to it
	// writes where the test can see rather than into a real home directory.
	command.env("HOME", home);
	for (name, value) in env {
		command.env(name, value);
	}

	let mut child = command
		.stdin(Stdio::piped())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.map_err(|e| format!("rnx did not start: {e}"))?;

	{
		let mut stdin = child.stdin.take().expect("no standard input");
		let written = stdin
			.write_all(format!("{MARKER}\n:quit\n").as_bytes())
			.and_then(|()| stdin.flush());
		if let Err(e) = written {
			let survived = harness::killed_within(&mut child, LIMIT);
			return Err(match survived {
				Some(survived) => format!("cannot write to the session: {e}; and {survived}"),
				None => format!("cannot write to the session: {e}"),
			});
		}
	} // dropped, which closes the stream and ends the session's input

	let deadline = Instant::now() + LIMIT;
	loop {
		match child.try_wait() {
			Ok(Some(status)) if status.success() => return Ok(()),
			Ok(Some(status)) => return Err(format!("the session exited with {status}")),
			Ok(None) => {}
			Err(e) => {
				let survived = harness::killed_within(&mut child, LIMIT);
				return Err(match survived {
					Some(survived) => format!("cannot wait for the session: {e}; and {survived}"),
					None => format!("cannot wait for the session: {e}"),
				});
			}
		}
		if Instant::now() >= deadline {
			// Killed and collected **within a bound**, so the failure is the
			// timeout rather than a stuck suite — and a session that survives
			// being killed is said rather than waited on for ever.
			let survived = harness::killed_within(&mut child, LIMIT);
			return Err(match survived {
				Some(survived) => {
					format!("the session did not end within {LIMIT:?}, and {survived}")
				}
				None => format!("the session did not end within {LIMIT:?}"),
			});
		}
		std::thread::sleep(Duration::from_millis(10));
	}
}

/// What a history file at `path` is wrong about, if anything.
fn trouble_with(path: &Path, whose: &str) -> Vec<String> {
	let mut trouble = Vec::new();
	match std::fs::read_to_string(path) {
		Ok(text) if text.contains(MARKER) => {}
		Ok(text) => trouble.push(format!(
			"{whose} does not hold the input that was typed: {text:?}"
		)),
		Err(e) => trouble.push(format!(
			"{whose} could not be read at {}: {e}",
			path.display()
		)),
	}
	trouble
}

/// With only the platform's state directory named, history is written
/// beneath it, and holds what was typed.
///
/// This is the defect, exactly. Before decision 7 the Windows answer was
/// `None` however `LOCALAPPDATA` was set, so no file appeared and a session
/// silently kept nothing.
#[test]
fn history_is_written_under_the_platforms_state_directory() {
	let home = scratch("home");
	let state = scratch("state");

	let mut trouble = Vec::new();
	if let Err(why) = session_with(&home, &[(STATE_VAR, state.as_path())]) {
		trouble.push(why);
	} else {
		trouble.extend(trouble_with(
			&state.join("rnx").join("history"),
			&format!("the history under {STATE_VAR}"),
		));
		// The fallback must not have been what answered.
		if home.join(".local").exists() {
			trouble.push("history was written under HOME as well".to_owned());
		}
	}

	std::fs::remove_dir_all(&home).ok();
	std::fs::remove_dir_all(&state).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// `RNX_HISTORY` names the file itself, and wins over the state directory.
#[test]
fn rnx_history_names_the_file_and_wins() {
	let home = scratch("home");
	let state = scratch("ignored");
	let chosen = scratch("named");
	let named = chosen.join("chosen-history");

	let mut trouble = Vec::new();
	let ran = session_with(
		&home,
		&[
			(STATE_VAR, state.as_path()),
			("RNX_HISTORY", named.as_path()),
		],
	);
	if let Err(why) = ran {
		trouble.push(why);
	} else {
		trouble.extend(trouble_with(&named, "the history it was told to keep"));
		if state.join("rnx").join("history").exists() {
			trouble.push(format!(
				"history was written under {STATE_VAR} as well as where it was told"
			));
		}
	}

	std::fs::remove_dir_all(&home).ok();
	std::fs::remove_dir_all(&state).ok();
	std::fs::remove_dir_all(&chosen).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// With no state variable set, Unix falls back to `HOME/.local/state` — and
/// that is the branch a default Linux install actually takes, because
/// `XDG_STATE_HOME` is unset on most of them. Decision 7 moved this lookup
/// behind the platform seam, so the branch that changed hands is the one with
/// the most users and, until now, no gate.
#[cfg(unix)]
#[test]
fn without_a_state_variable_unix_falls_back_to_home() {
	let home = scratch("home");

	let mut trouble = Vec::new();
	// `session_with` clears every consulted variable and sets `HOME` alone,
	// so this is the fallback and nothing else.
	match session_with(&home, &[]) {
		Err(why) => trouble.push(why),
		Ok(()) => trouble.extend(trouble_with(
			&home
				.join(".local")
				.join("state")
				.join("rnx")
				.join("history"),
			"the history under HOME",
		)),
	}

	std::fs::remove_dir_all(&home).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// The same absence, on Windows, means something else: there is no fallback
/// to fall back to.
///
/// `LOCALAPPDATA` is the only base the platform offers, and `HOME` is not a
/// Windows concept — a session that found one set, by a shell that exports it
/// or by a test like this one, must not start keeping history under it. So
/// the expectation here is the opposite of the Unix one: no file, anywhere,
/// and a session that still runs and quits cleanly without it.
#[cfg(windows)]
#[test]
fn without_a_state_variable_windows_keeps_no_history() {
	let home = scratch("home");

	let mut trouble = Vec::new();
	match session_with(&home, &[]) {
		Err(why) => trouble.push(why),
		Ok(()) => {
			// The Unix location, asked for by name, because that is what a
			// lookup which reached for `HOME` would have produced.
			let under_home = home
				.join(".local")
				.join("state")
				.join("rnx")
				.join("history");
			if under_home.exists() {
				trouble.push(
					"history was written under HOME, which Windows has no rule for".to_owned(),
				);
			}
			if let Ok(entries) = std::fs::read_dir(&home)
				&& entries.count() != 0
			{
				trouble.push(
					"something was written under HOME with no state directory set".to_owned(),
				);
			}
		}
	}

	std::fs::remove_dir_all(&home).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

#[cfg(windows)]
#[path = "harness/console.rs"]
mod console;

#[cfg(windows)]
struct RecallSpace {
	home: PathBuf,
	state: PathBuf,
}

#[cfg(windows)]
impl Drop for RecallSpace {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.home);
		let _ = std::fs::remove_dir_all(&self.state);
	}
}

#[cfg(windows)]
#[test]
fn a_second_windows_session_recalls_saved_history() {
	for explicit in [false, true] {
		let space = RecallSpace {
			home: scratch("recall-home"),
			state: scratch("recall-state"),
		};
		let chosen = space.state.join("chosen-history");
		let mut env = vec![
			("HOME", space.home.as_path()),
			("LOCALAPPDATA", space.state.as_path()),
		];
		if explicit {
			env.push(("RNX_HISTORY", chosen.as_path()));
		}
		{
			let mut first = console::Console::spawn(&["repl"], &env);
			first.expect("rnx>");
			first.send("103000 + 1729\r");
			first.expect("104729");
			first.send(":quit\r");
			assert_eq!(first.finish(), 0);
		}
		{
			let mut second = console::Console::spawn(&["repl"], &env);
			second.expect("rnx>");
			// No expression is typed in this process: two Ups pass :quit and
			// recall the saved expression, then Enter evaluates it. The result
			// is absent from the input, so terminal echo cannot satisfy this assertion.
			second.send("\x1b[A\x1b[A\r");
			second.expect("104729");
			second.send(":quit\r");
			assert_eq!(second.finish(), 0);
		}
	}
}
