//! Gate 10: a child cannot leave rnx's job, **because rnx's job forbids it**.
//!
//! Record 0025 decision 5. The escaped-descendant cases records 0022 and 0023
//! were built around use `setsid`; with `JOB_OBJECT_LIMIT_BREAKAWAY_OK` unset
//! — the default, and what decision 5 keeps — a descendant cannot leave the
//! job. Decision 5 calls that a **stronger** guarantee and is explicit that it
//! must not be reported as a passing gate for the same test. So the refusal
//! has to be evidenced, and it is what licenses marking those cases
//! inapplicable rather than skipping them.
//!
//! **A refusal is not evidence of a policy.** A first version spawned with
//! `CREATE_BREAKAWAY_FROM_JOB`, saw `ERROR_ACCESS_DENIED`, and stopped there.
//! That establishes only that one call failed: the same error would appear if
//! the executable were missing, if the spawn were broken for an unrelated
//! reason, or if some enclosing job forbade it regardless of rnx. Four things
//! are needed together, and this asks for all four:
//!
//! 1. the child **is** in a job at all (`IsProcessInJob`),
//! 2. that job's limits **lack** `JOB_OBJECT_LIMIT_BREAKAWAY_OK`,
//! 3. the same executable, same arguments, spawns **fine without the flag** —
//!    so the refusal is the flag's doing and not the spawn's,
//! 4. and only then, that spawning **with** the flag is refused.
//!
//! Plus a control from outside: this test process attempts the same breakaway
//! before running rnx at all. If it succeeds there, the flag plainly works on
//! this machine and rnx's job is what refuses it. If this runner's own
//! breakaway fails, that control cannot be drawn and the gate says so rather
//! than quietly passing on four-of-five. Being in a job is not itself blocking:
//! the outside control can succeed in a job that permits breakaway.
//!
//! On the measured Windows runner, `cargo test` blocks the control while
//! running the compiled test binary directly succeeds. Section G of record
//! 0025's evidence gives a PowerShell recipe to locate that binary without
//! hardcoding its hash.

#![cfg(windows)]

mod harness;

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::JobObjects::{
	IsProcessInJob, JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
	JobObjectExtendedLimitInformation, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::core::BOOL;

/// Documented to make a process that is not part of its parent's job.
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// `ERROR_ACCESS_DENIED`, which a job that forbids breakaway answers.
const ACCESS_DENIED: i32 = 5;

const LIMIT: Duration = Duration::from_secs(40);

/// The probe child is `cmd /c exit 0`. It has nothing to do, so this is long
/// enough that only a wedged process reaches it.
const PROBE_LIMIT: Duration = Duration::from_secs(10);

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!("rnx-breakaway-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// Whether this process is inside a job at all.
fn in_a_job() -> bool {
	let mut answer: BOOL = FALSE;
	// SAFETY: a null job handle asks about the current process's own job, and
	// the out parameter is a BOOL this frame owns.
	unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &raw mut answer) };
	answer != FALSE
}

/// Whether the job this process is in permits breakaway. `None` when the job
/// cannot be queried, which is a different answer from "it does not permit".
fn breakaway_permitted() -> Option<bool> {
	let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
	let size = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
	// SAFETY: a null job handle asks about the current process's own job; the
	// buffer and its length agree and are owned by this frame.
	let ok = unsafe {
		QueryInformationJobObject(
			std::ptr::null_mut(),
			JobObjectExtendedLimitInformation,
			(&raw mut info).cast(),
			size,
			std::ptr::null_mut(),
		)
	};
	if ok == FALSE {
		return None;
	}
	Some(info.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_BREAKAWAY_OK != 0)
}

/// What a probe spawn answered.
///
/// Three outcomes, and keeping the third apart from the first is the point. A
/// **refusal is evidence** — `ERROR_ACCESS_DENIED` is a job saying no, which
/// is the whole of what this gate asks — while a probe that could not answer
/// is the absence of evidence. An earlier version of this file collapsed the
/// third into success: a child that outlived its bound, or survived being
/// killed, was reported as having run, and supplied a passing control for a
/// question nothing had asked.
#[derive(Debug)]
enum Attempt {
	/// Created, and collected within its bound.
	Ran,
	/// The spawn itself was refused, with the raw OS error.
	Refused(Option<i32>),
	/// The probe could not answer: the child outlived its bound or could not
	/// be waited for, and any failure to clean it up is carried along.
	Failed(String),
}

/// Spawn `program` with `args`, with or without asking to leave the job, and
/// collect it within `within`.
///
/// The program is a parameter because the control below must not go through a
/// shell: killing `cmd` collects `cmd`, and whatever it started keeps running.
fn attempt(program: &str, args: &[&str], breaking_away: bool, within: Duration) -> Attempt {
	let mut command = Command::new(program);
	command
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	if breaking_away {
		command.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
	}
	let mut child = match command.spawn() {
		Ok(child) => child,
		Err(e) => return Attempt::Refused(e.raw_os_error()),
	};
	match harness::reaped_within(&mut child, within) {
		Some(_) => Attempt::Ran,
		None => {
			// Either it outlived the bound or it could not be waited for; both
			// leave the probe with nothing to say. Cleanup is still bounded,
			// and a child that survives it is worse news than one that did
			// not — so that is kept rather than replacing the first failure.
			let survived = harness::killed_within(&mut child, within);
			Attempt::Failed(match survived {
				Some(survived) => {
					format!("the probe child did not end within {within:?}, and {survived}")
				}
				None => format!("the probe child did not end within {within:?}"),
			})
		}
	}
}

/// The probe this gate uses: a child with nothing to do, so only a wedged one
/// reaches the bound.
fn spawn_attempt(breaking_away: bool) -> Attempt {
	// `cmd` is the child here deliberately: what is being asked is whether a
	// child of this process can leave the job, and this one exits at once, so
	// nothing outlives it to be left behind.
	attempt("cmd", &["/c", "exit", "0"], breaking_away, PROBE_LIMIT)
}

#[test]
fn a_child_cannot_break_away_because_the_job_forbids_it() {
	// The inner role: inside rnx's job, report all four observations.
	if std::env::var_os("RNX_TEST_TRY_BREAKAWAY").is_some() {
		println!("IN_JOB {}", in_a_job());
		println!(
			"BREAKAWAY_OK {}",
			match breakaway_permitted() {
				Some(true) => "permitted",
				Some(false) => "forbidden",
				None => "unknown",
			}
		);
		println!(
			"PLAIN {}",
			match spawn_attempt(false) {
				Attempt::Ran => "spawned".to_owned(),
				Attempt::Refused(e) => format!("failed {e:?}"),
				Attempt::Failed(why) => format!("probe-failed {why}"),
			}
		);
		println!(
			"BREAKAWAY {}",
			match spawn_attempt(true) {
				Attempt::Ran => "spawned".to_owned(),
				Attempt::Refused(e) => format!("refused {e:?}"),
				Attempt::Failed(why) => format!("probe-failed {why}"),
			}
		);
		return;
	}

	let mut trouble = Vec::new();

	// The control, from outside any job of rnx's: does the flag work here at
	// all? Drawable only if this runner is not itself inside a job that
	// already forbids it.
	let runner_in_job = in_a_job();
	let runner_permits = breakaway_permitted();
	let runner_attempt = spawn_attempt(true);

	let dir = scratch();
	let exe = std::env::current_exe().expect("no test binary");
	let quoted = |s: &str| serde_json::to_string(s).expect("a string is serialisable");
	// The call's own outcome is reported too: stdout alone would read the same
	// whether the helper ran and answered or the call was deadlined short.
	let script = format!(
		"pub fn main(_) {{ let r = host::process({}, [\"--exact\", \
		 \"a_child_cannot_break_away_because_the_job_forbids_it\", \"--nocapture\"], 20000)?; \
		 println!(\"CALL code={{:?}} timed_out={{}} cancelled={{}}\", r.code, r.timed_out, r.cancelled); \
		 print!(\"{{}}\", r.stdout); Ok(()) }}",
		quoted(&exe.display().to_string())
	);
	let path = dir.join("breakaway.rn");
	std::fs::write(&path, &script).expect("cannot write the script");

	let out_path = dir.join("stdout");
	let out = std::fs::File::create(&out_path).expect("cannot make a stdout file");
	let err = out.try_clone().expect("cannot share the stdout file");

	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.env("RNX_TEST_TRY_BREAKAWAY", "1")
		.stdin(Stdio::null())
		.stdout(Stdio::from(out))
		.stderr(Stdio::from(err))
		.spawn()
		.expect("rnx did not start");

	let deadline = Instant::now() + LIMIT;
	let status = loop {
		match child.try_wait() {
			Ok(Some(status)) => break Some(status),
			Ok(None) => {}
			Err(e) => {
				trouble.push(format!("cannot wait for rnx: {e}"));
				break None;
			}
		}
		if Instant::now() >= deadline {
			trouble.push(format!("rnx did not end within {LIMIT:?}"));
			break None;
		}
		std::thread::sleep(Duration::from_millis(10));
	};
	if status.is_none()
		&& let Some(survived) = harness::killed_within(&mut child, LIMIT)
	{
		trouble.push(survived);
	}

	// An unreadable file is trouble, not silence: every check below asks
	// whether a line is present, and absence is what they would conclude.
	let said = match harness::wrote(&out_path) {
		Ok(said) => said,
		Err(e) => {
			trouble.push(e);
			String::new()
		}
	};
	let has = |needle: &str| said.lines().any(|l| l.trim() == needle);

	if let Some(status) = status {
		if status.code() != Some(0) {
			trouble.push(format!("rnx exited {:?}", status.code()));
		}
		// The call must have run the helper to completion, not been deadlined
		// or cancelled into looking like a refusal.
		if !has("CALL code=0 timed_out=false cancelled=false") {
			trouble.push(format!(
				"the helper did not run to a clean finish: {:?}",
				said.lines()
					.find(|l| l.starts_with("CALL"))
					.unwrap_or("(no CALL line)")
			));
		}
		// 1. in a job at all
		if !has("IN_JOB true") {
			trouble.push("the child was not in a job, so nothing was being asked".to_owned());
		}
		// 2. whose limits forbid breakaway
		if !has("BREAKAWAY_OK forbidden") {
			trouble.push(format!(
				"the job's limits do not say breakaway is forbidden: {:?}",
				said.lines()
					.find(|l| l.starts_with("BREAKAWAY_OK"))
					.unwrap_or("(none)")
			));
		}
		// A probe inside rnx that could not answer is its own failure, named
		// before the checks that would otherwise report it as a refusal or an
		// absence.
		for line in said.lines().filter(|l| l.contains("probe-failed")) {
			trouble.push(format!(
				"a probe inside rnx could not answer: {}",
				line.trim()
			));
		}
		// 3. the same spawn works without the flag
		if !has("PLAIN spawned") {
			trouble.push(format!(
				"the same child could not be spawned without the flag, so a refusal proves nothing: {:?}",
				said.lines()
					.find(|l| l.starts_with("PLAIN"))
					.unwrap_or("(none)")
			));
		}
		// 4. and is refused with it
		if has("BREAKAWAY spawned") {
			trouble.push(
				"a child left the job: decision 5's guarantee is narrower than the record claims"
					.to_owned(),
			);
		} else if !has(&format!("BREAKAWAY refused Some({ACCESS_DENIED})")) {
			trouble.push(format!(
				"the attempt did not report the expected refusal: {:?}",
				said.lines()
					.find(|l| l.starts_with("BREAKAWAY "))
					.unwrap_or("(none)")
			));
		}
	}

	// The outside control, reported either way — and a probe that could not
	// answer is not the same as one that was refused. A refusal here is the
	// measured environment saying no, which blocks attribution; a probe
	// failure is this test being broken, and must not be reported as the
	// former or absorbed into a pass.
	if let Attempt::Failed(why) = &runner_attempt {
		trouble.push(format!(
			"the outside control could not be attempted, so nothing here is attributable either way: {why}"
		));
	} else if matches!(runner_attempt, Attempt::Refused(_)) {
		// The refusal inside rnx is then unattributable: it happens here
		// anyway. Measured on this machine as `in_job=true permits=Some(false)
		// attempt=Err(Some(5))` -- the same error, with no rnx involved. Passing
		// on the four inner observations alone would credit rnx's job with a
		// refusal the environment was already making.
		trouble.push(format!(
			"the control cannot be drawn here: this runner is in_job={runner_in_job} permits={runner_permits:?} and its own breakaway is {runner_attempt:?}, so the refusal inside rnx is not attributable to rnx's job. On the measured Windows runner, cargo test introduces the restrictive job. Run the compiled test directly from PowerShell: & '{}' --nocapture . See section G of plans/0025_the_same_contracts_on_windows_evidence.md for the hash-free build recipe. If direct execution also blocks the control, an enclosing job still prevents attribution.",
			exe.display().to_string().replace('\'', "''")
		));
	}

	std::fs::remove_dir_all(&dir).ok();
	assert!(trouble.is_empty(), "{}", trouble.join("; "));
}

/// A probe child that does not return must fail, and must fail **after** its
/// cleanup rather than by waiting for it.
///
/// This is the control for `Attempt`'s third arm, and for the defect that arm
/// exists to prevent: a `spawn_attempt` that discarded a timeout and answered
/// `Ok(())` gave this gate a passing outside control while asking nothing at
/// all. Nothing about a job is being tested here — only that a probe which
/// cannot answer says so.
///
/// `ping -n 30` is the child because it ignores its standard input: `pause`
/// reads a redirected `NUL` as a keypress and returns at once, which would
/// make this control agree with a broken probe.
///
/// It is spawned **directly**, not as `cmd /c ping`. A shell in between makes
/// the shell the child: killing it collects the shell, `ping` keeps running,
/// and the control would report a clean cleanup while leaving a process behind
/// on every run — the same defect one layer down. The Unix half of this is
/// measured in `tests/child_input.rs`, where `sh -c` was found to fork rather
/// than exec.
#[test]
fn a_probe_child_that_never_returns_fails_the_probe() {
	const CONTROL_LIMIT: Duration = Duration::from_secs(2);
	let began = Instant::now();
	let outcome = attempt("ping", &["-n", "30", "127.0.0.1"], false, CONTROL_LIMIT);
	let took = began.elapsed();

	match outcome {
		Attempt::Failed(why) => {
			// Bounded cleanup, and it worked: the child that would not end on
			// its own did end when it was killed. A `Failed` that also says
			// the child survived is a leak this control would rather report
			// than let pass silently.
			assert!(
				!why.contains("still running"),
				"the probe child survived its cleanup: {why}"
			);
		}
		other => panic!("a child that never returns was reported as {other:?}"),
	}
	// Twice the bound covers the wait and the kill; anything beyond it means
	// something waited without one.
	assert!(
		took < CONTROL_LIMIT * 3,
		"the probe took {took:?} to give up on a {CONTROL_LIMIT:?} bound"
	);
}
