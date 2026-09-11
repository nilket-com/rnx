//! What each platform does for the same contract.
//!
//! The contracts are records 0016, 0020, 0022 and 0023's: a child ends at its
//! deadline, an interrupt ends a call while it waits and while it cleans up,
//! a child's descendants go with it, and every worker stops when the call
//! does. The mechanisms differ; the promises do not, and record 0025 says
//! which promise Windows cannot be asked in the same words.
//!
//! Nothing here is claimed for a platform it has not run on. The Windows side
//! type-checks against `x86_64-pc-windows-msvc`; record 0025's gates are what
//! will say whether it works.
use std::io::Result;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set when the person running rnx asks it to stop. Every waiter and every
/// worker reads this; the handler that sets it does nothing else, because a
/// handler runs where it likes.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub fn interrupted() -> bool {
	INTERRUPTED.load(Ordering::Relaxed)
}

pub fn clear_interrupt() {
	INTERRUPTED.store(false, Ordering::Relaxed);
}

/// The per-user, machine-local directory a program may keep state in, or
/// `None` where the platform does not say.
///
/// Record 0025 decision 7. `LOCALAPPDATA` is what `XDG_STATE_HOME` names on
/// Unix — per-user, machine-local, not roamed, set by the system rather than
/// by a shell — so the mechanism differs here and the contract does not.
/// Windows has no `HOME` and no XDG layout, and asking for them there is how
/// a session came to keep no history at all.
#[cfg(windows)]
pub fn state_dir() -> Option<std::path::PathBuf> {
	std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
}

#[cfg(unix)]
pub fn state_dir() -> Option<std::path::PathBuf> {
	std::env::var_os("XDG_STATE_HOME")
		.map(std::path::PathBuf::from)
		.or_else(|| {
			std::env::var_os("HOME")
				.map(|h| std::path::PathBuf::from(h).join(".local").join("state"))
		})
}

/// A stream this platform can be asked about: a descriptor on Unix, a handle
/// on Windows. The bound exists so `capture` can take either without knowing
/// which.
#[cfg(unix)]
pub trait Stream: std::os::fd::AsFd {}
#[cfg(unix)]
impl<T: std::os::fd::AsFd> Stream for T {}

#[cfg(windows)]
pub trait Stream: std::os::windows::io::AsHandle {}
#[cfg(windows)]
impl<T: std::os::windows::io::AsHandle> Stream for T {}

#[cfg(unix)]
mod imp {
	use super::*;

	extern "C" fn interrupt(_: libc::c_int) {
		INTERRUPTED.store(true, Ordering::Relaxed);
	}

	/// Ctrl-C sets the flag and nothing else happens in the handler.
	pub fn watch_for_interrupt() {
		unsafe {
			libc::signal(libc::SIGINT, interrupt as *const () as libc::sighandler_t);
		}
	}

	/// Whether standard input is a terminal, which record 0012 refuses to read
	/// because nobody is going to send an end-of-file.
	pub fn stdin_is_a_terminal() -> bool {
		unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
	}

	/// A child and its descendants, addressable together.
	pub struct Group(i32);

	/// Spawn the child in a process group of its own, so its descendants can
	/// be ended with it.
	pub fn spawn_in_group(command: &mut Command) -> Result<(Child, Group)> {
		use std::os::unix::process::CommandExt;
		let child = command.process_group(0).spawn()?;
		let pid = child.id() as i32;
		Ok((child, Group(pid)))
	}

	impl Group {
		/// End everything in the group. A descendant that left it — `setsid` —
		/// is not reached, and records 0022 and 0023 are why that no longer
		/// extends a call.
		pub fn end(&self) {
			unsafe {
				libc::kill(-self.0, libc::SIGKILL);
			}
		}
	}

	/// Make a stream one that never blocks, so a worker's own flag can stop it.
	pub fn prepare_stream(stream: &impl Stream) -> Result<()> {
		use std::os::fd::AsRawFd;
		let fd = stream.as_fd().as_raw_fd();
		unsafe {
			let flags = libc::fcntl(fd, libc::F_GETFL);
			if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
				return Err(std::io::Error::last_os_error());
			}
		}
		Ok(())
	}

	/// Whether a read may be attempted. A non-blocking descriptor answers
	/// this itself, by returning `WouldBlock`, so there is nothing to ask.
	pub fn readable(_stream: &impl Stream) -> bool {
		true
	}

	/// A stream's handle, as a value the protocol can keep for the length of
	/// one operation.
	pub type Raw = std::os::fd::RawFd;

	pub fn raw(stream: &impl Stream) -> Raw {
		use std::os::fd::AsRawFd;
		stream.as_fd().as_raw_fd()
	}

	/// Reach a worker that is inside an operation. There is nothing to reach
	/// here: a non-blocking descriptor is never inside one for longer than it
	/// takes to return, so the worker's own flag is enough.
	pub fn cancel(_handle: Raw) {}
}

#[cfg(windows)]
mod imp {
	use super::*;
	use std::os::windows::io::AsRawHandle;
	use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE, TRUE};
	#[cfg(feature = "test-support")]
	use windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent;
	use windows_sys::Win32::System::Console::{
		CONSOLE_MODE, CTRL_BREAK_EVENT, CTRL_C_EVENT, GetConsoleMode, GetStdHandle,
		STD_INPUT_HANDLE, SetConsoleCtrlHandler,
	};
	use windows_sys::Win32::System::Diagnostics::ToolHelp::{
		CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
	};
	use windows_sys::Win32::System::IO::CancelIoEx;
	use windows_sys::Win32::System::JobObjects::{
		AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
		JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
		SetInformationJobObject, TerminateJobObject,
	};
	use windows_sys::Win32::System::Pipes::PeekNamedPipe;
	use windows_sys::Win32::System::Threading::{
		CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
	};
	use windows_sys::core::BOOL;

	/// The console calls this on a thread of its own choosing, so it sets the
	/// flag and returns, exactly as the Unix handler does.
	unsafe extern "system" fn interrupt(event: u32) -> BOOL {
		if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
			INTERRUPTED.store(true, Ordering::Relaxed);
			return TRUE;
		}
		FALSE
	}

	pub fn watch_for_interrupt() {
		// Two steps, and the order is load-bearing.
		//
		// A process that ignores Ctrl-C passes that on to its children, and
		// installing a handler does not undo it: the handler is registered
		// and simply never runs. `SetConsoleCtrlHandler(NULL, FALSE)` is what
		// restores delivery.
		//
		// But delivery must be restored **after** the handler is in place,
		// never before. Between restoring and installing, an interrupt would
		// find only the default handler, which Windows documents as calling
		// `ExitProcess` — so the window between the two calls is one where a
		// Ctrl-C kills rnx outright instead of cancelling a call. Doing it
		// the other way round trades a contract that silently does not hold
		// for a process that sometimes dies, which is worse.
		pretend_a_parent_ignored_ctrl_c();
		let installed = unsafe { SetConsoleCtrlHandler(Some(interrupt), TRUE) } != FALSE;
		// Only if the handler is actually there. Enabling delivery after a
		// failed registration is the same defect as enabling it first: an
		// interrupt would find the default handler and end the process. A
		// process that cannot be interrupted keeps running; one that takes
		// `ExitProcess` mid-call does not, and the first is the better
		// failure.
		let delivered = installed && unsafe { SetConsoleCtrlHandler(None, FALSE) } != FALSE;
		ask_for_an_interrupt_later(installed, delivered);
	}

	/// Put this process into the state a parent that ignores Ctrl-C would
	/// have left it in, before anything else runs.
	///
	/// Nothing a script can do produces that state, and whether the process
	/// that started a test run is already in it is not something a fixture
	/// controls — so without this the gate below would establish the fix only
	/// under whatever state it happened to inherit. Same reason
	/// `RNX_TEST_JOB_ASSIGNMENT_FAILS` exists.
	#[cfg(feature = "test-support")]
	fn pretend_a_parent_ignored_ctrl_c() {
		if std::env::var("RNX_TEST_IGNORE_CTRL_C_FIRST").is_ok_and(|v| !v.is_empty()) {
			// SAFETY: NULL with TRUE is the documented way to ask to ignore
			// Ctrl-C, and is what a parent that did so passed on.
			let ignoring = unsafe { SetConsoleCtrlHandler(None, TRUE) } != FALSE;
			// Said, because a control that silently failed to put the process
			// into the state it is controlling for would pass while measuring
			// the ordinary case twice.
			eprintln!("rnx test-support: now ignoring Ctrl-C: ok={ignoring}");
		}
	}

	#[cfg(not(feature = "test-support"))]
	fn pretend_a_parent_ignored_ctrl_c() {}

	/// Raise a Ctrl-C on this process's own console, after a delay the hook
	/// names. Nothing a script can do provokes one, which is why this exists
	/// at all — the same reason `RNX_TEST_JOB_ASSIGNMENT_FAILS` does.
	///
	/// **This is why it is gated.** `GenerateConsoleCtrlEvent(_, 0)` reaches
	/// every process attached to the console, which in an ordinary run
	/// includes the shell that started rnx. Record 0025's risks say an
	/// interrupt on Windows is console-wide and must be established in a
	/// console of its own before it goes near anything a person runs; the
	/// gate that sets this hook gives the process one.
	///
	/// What it establishes is that the handler decision 4 installs is
	/// reached and sets the flag. It is **not** gate 6, which wants an
	/// interrupt during cleanup, with the child already gone and the readers
	/// finishing.
	#[cfg(feature = "test-support")]
	fn ask_for_an_interrupt_later(installed: bool, delivered: bool) {
		let Ok(when) = std::env::var("RNX_TEST_RAISE_INTERRUPT_WHEN") else {
			return;
		};
		std::thread::spawn(move || {
			// A handshake, not a sleep. The **child** makes this file and then
			// stays alive, so the event is raised because a child exists and
			// the call is waiting on it. A marker the script wrote before the
			// call would say only that the script reached that line, and the
			// event could then arrive before any child existed; a fixed delay
			// says less again, since one long enough to be safe on a loaded
			// machine is also long enough to hide a call that never started.
			let waited_from = std::time::Instant::now();
			while !std::path::Path::new(&when).exists() {
				if waited_from.elapsed() > std::time::Duration::from_secs(20) {
					eprintln!("rnx test-support: the script never signalled it was ready");
					return;
				}
				std::thread::sleep(std::time::Duration::from_millis(5));
			}
			// SAFETY: no arguments to get wrong.
			let asked = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) };
			// Said out loud, because a gate that only watches the flag cannot
			// tell "the event was never raised" from "the handler never ran",
			// and those want different answers.
			eprintln!(
				"rnx test-support: handler installed={installed} delivery restored={delivered}; raised a console interrupt: ok={} err={}",
				asked != FALSE,
				std::io::Error::last_os_error()
			);
			// Raising is a request, not a receipt. The handler runs on a
			// thread of the console's choosing, so `ok=true` says only that
			// Windows accepted the ask — and a gate that released its readers
			// here could still have them finish before the flag was ever set.
			// So wait for the flag the handler sets, and publish only then.
			//
			// A flag that never arrives leaves the file unwritten on purpose:
			// whoever was waiting on it then fails for want of it, which is
			// the honest outcome, rather than being released as though the
			// interrupt had landed.
			if let Ok(path) = std::env::var("RNX_TEST_SIGNAL_RAISED_TO") {
				let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
				while !INTERRUPTED.load(Ordering::Relaxed) && std::time::Instant::now() < until {
					std::thread::sleep(std::time::Duration::from_millis(2));
				}
				if INTERRUPTED.load(Ordering::Relaxed) {
					let _ = std::fs::write(&path, "received");
				} else {
					eprintln!(
						"rnx test-support: the interrupt was raised but never received; nothing released"
					);
				}
			}
		});
	}

	#[cfg(not(feature = "test-support"))]
	fn ask_for_an_interrupt_later(_installed: bool, _delivered: bool) {}

	/// Whether standard input is a **console**, which is the thing record 0012
	/// refuses to read because nobody is going to send an end-of-file.
	///
	/// `GetFileType` is not the question: it answers `FILE_TYPE_CHAR` for any
	/// character device, `NUL` among them, so a program run with its input
	/// redirected from `NUL` would have had its empty input refused instead of
	/// read. `GetConsoleMode` succeeds only for a console handle.
	pub fn stdin_is_a_terminal() -> bool {
		let mut mode: CONSOLE_MODE = 0;
		unsafe { GetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), &raw mut mode) != FALSE }
	}

	/// A job object: a child and everything it spawns, addressable together.
	/// Dropping it closes the handle, and `KILL_ON_JOB_CLOSE` means a parent
	/// that dies takes the job with it.
	pub struct Group(HANDLE);

	/// Spawn the child **suspended**, put it in the job while it cannot act,
	/// and only then let it run.
	///
	/// Assigning after the child is running is a race: a descendant spawned in
	/// that window is outside the job and survives the call. Record 0025
	/// decision 2 is why this is the order.
	pub fn spawn_in_group(command: &mut Command) -> Result<(Child, Group)> {
		use std::os::windows::process::CommandExt;
		let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
		if job.is_null() {
			return Err(std::io::Error::last_os_error());
		}
		let group = Group(job);
		let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
		limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
		let set = unsafe {
			SetInformationJobObject(
				job,
				JobObjectExtendedLimitInformation,
				(&raw const limits).cast(),
				size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
			)
		};
		if set == FALSE {
			return Err(std::io::Error::last_os_error());
		}
		let child = command.creation_flags(CREATE_SUSPENDED).spawn()?;
		#[cfg(feature = "test-support")]
		if let Err(why) = observe_before_assignment(&child, job) {
			return Err(abandon(child, why));
		}
		// From here the child exists and is suspended, so every failure has to
		// end it **directly**. Ending the job would not: a child that never
		// joined it is not in it, and would be left suspended for ever.
		if injected_assignment_failure() {
			return Err(abandon(
				child,
				std::io::Error::other("the job assignment was told to fail"),
			));
		}
		let assigned = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) };
		if assigned == FALSE {
			return Err(abandon(child, std::io::Error::last_os_error()));
		}
		if let Err(why) = resume(child.id()) {
			return Err(abandon(child, why));
		}
		Ok((child, group))
	}

	/// Whether to fail the job assignment deliberately. Nothing a script can
	/// do makes `AssignProcessToJobObject` fail, and what record 0025 has to
	/// gate on the failure path is that the suspended child does not survive
	/// it. Off unless `test-support` is built.
	#[cfg(feature = "test-support")]
	fn injected_assignment_failure() -> bool {
		std::env::var("RNX_TEST_JOB_ASSIGNMENT_FAILS").is_ok_and(|v| !v.is_empty())
	}

	/// Let a native fixture own observation handles before assignment or its
	/// injected failure. A suspended child cannot publish its own identity.
	#[cfg(feature = "test-support")]
	fn observe_before_assignment(child: &Child, job: HANDLE) -> Result<()> {
		let Some(dir) = std::env::var_os("RNX_TEST_BEFORE_JOB_ASSIGNMENT") else {
			return Ok(());
		};
		let dir = std::path::PathBuf::from(dir);
		std::fs::write(
			dir.join("created.tmp"),
			format!("{} {}", child.id(), job as usize),
		)?;
		std::fs::rename(dir.join("created.tmp"), dir.join("created"))?;
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
		while !dir.join("assign").exists() {
			if std::time::Instant::now() >= deadline {
				return Err(std::io::Error::other(
					"the assignment observer never released the child",
				));
			}
			std::thread::sleep(std::time::Duration::from_millis(5));
		}
		Ok(())
	}

	#[cfg(not(feature = "test-support"))]
	fn injected_assignment_failure() -> bool {
		false
	}

	/// End a child this call is giving up on, and collect it, so a failure
	/// leaves nothing suspended behind. The original reason is what is
	/// reported; a failure to clean up is not more interesting than it.
	fn abandon(mut child: Child, why: std::io::Error) -> std::io::Error {
		let _ = child.kill();
		let _ = child.wait();
		why
	}

	/// Let a suspended child run.
	///
	/// `std::process::Child` hands over the process handle and not the thread,
	/// so the one thread a just-created process has is found by enumerating
	/// them. Record 0025 names calling `CreateProcessW` directly as the
	/// fallback if this proves to race with creation.
	fn resume(pid: u32) -> Result<()> {
		let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
		if snapshot == INVALID_HANDLE_VALUE {
			return Err(std::io::Error::last_os_error());
		}
		let mut entry = THREADENTRY32 {
			dwSize: size_of::<THREADENTRY32>() as u32,
			..Default::default()
		};
		let mut found = false;
		let mut resumed: Result<()> = Err(std::io::Error::other(
			"the child's thread was opened but never resumed",
		));
		let mut walking = unsafe { Thread32First(snapshot, &raw mut entry) } != FALSE;
		while walking {
			if entry.th32OwnerProcessID == pid {
				let thread =
					unsafe { OpenThread(THREAD_SUSPEND_RESUME, FALSE, entry.th32ThreadID) };
				if !thread.is_null() {
					// Resuming is not established by finding the thread. The call
					// answers the **previous** suspend count: `u32::MAX` is a
					// failure, and any count above one leaves the thread still
					// suspended, so it is resumed until it is running or the call
					// says it cannot be.
					loop {
						let previous = unsafe { ResumeThread(thread) };
						if previous == u32::MAX {
							resumed = Err(std::io::Error::last_os_error());
							break;
						}
						if previous <= 1 {
							resumed = Ok(());
							break;
						}
					}
					unsafe {
						CloseHandle(thread);
					}
					found = true;
					break;
				}
			}
			walking = unsafe { Thread32Next(snapshot, &raw mut entry) } != FALSE;
		}
		unsafe {
			CloseHandle(snapshot);
		}
		if found {
			resumed
		} else {
			Err(std::io::Error::other(
				"the child was created suspended and its thread could not be found to resume it",
			))
		}
	}

	impl Group {
		/// End everything in the job. A child cannot leave one unless the job
		/// permits it, and this one does not, so there is nothing outside to
		/// miss — record 0025 decision 5.
		pub fn end(&self) {
			unsafe {
				TerminateJobObject(self.0, 1);
			}
		}
	}

	impl Drop for Group {
		fn drop(&mut self) {
			unsafe {
				CloseHandle(self.0);
			}
		}
	}

	/// Nothing to prepare: a pipe `std` made is not overlapped, so it cannot
	/// be made non-blocking. `readable` is how a read is kept from blocking.
	pub fn prepare_stream(_stream: &impl Stream) -> Result<()> {
		Ok(())
	}

	/// Whether bytes are waiting. A pipe that has been closed answers no to
	/// the peek, and a read of it is what reports the end of the stream, so a
	/// failed peek says "read it and find out" rather than "there is nothing".
	pub fn readable(stream: &impl Stream) -> bool {
		let handle = stream.as_handle().as_raw_handle() as HANDLE;
		let mut available: u32 = 0;
		let peeked = unsafe {
			PeekNamedPipe(
				handle,
				std::ptr::null_mut(),
				0,
				std::ptr::null_mut(),
				&raw mut available,
				std::ptr::null_mut(),
			)
		};
		peeked == FALSE || available > 0
	}

	/// Reach a worker that is inside an operation.
	///
	/// This is the half `PeekNamedPipe` cannot do. A `WriteFile` into a full
	/// pipe blocks in the kernel, where no flag reaches it, so the operation is
	/// cancelled from outside and the write returns `ERROR_OPERATION_ABORTED`.
	///
	/// The handle belongs to the `Pipe` both sides hold, so it cannot be closed
	/// under this call, and `Pipe` only calls it while an operation is
	/// outstanding — `CancelIoEx` affects operations already begun and reports
	/// `ERROR_NOT_FOUND` when there are none, which is why the protocol and not
	/// a status check decides when to call it.
	pub fn cancel(handle: Raw) {
		unsafe {
			CancelIoEx(handle as HANDLE, std::ptr::null());
		}
	}

	/// A stream's handle, as a value the protocol can keep for the length of
	/// one operation. Kept as an `isize` rather than a `HANDLE` so it can
	/// cross to the thread that cancels it.
	pub type Raw = isize;

	pub fn raw(stream: &impl Stream) -> Raw {
		stream.as_handle().as_raw_handle() as Raw
	}
}

pub use imp::{
	Raw, cancel, prepare_stream, raw, readable, spawn_in_group, stdin_is_a_terminal,
	watch_for_interrupt,
};

/// How long a call waits between asks to reach an operation a worker is
/// inside. The first ask ordinarily lands and the loop ends on the look after
/// it, so the first wait is short; the cap is what keeps a worker that is
/// wedged from costing a core to keep asking.
const FIRST_ASK_WAIT: std::time::Duration = std::time::Duration::from_micros(100);
const LONGEST_ASK_WAIT: std::time::Duration = std::time::Duration::from_millis(5);

/// The protocol that lets a call reach an operation its worker is inside,
/// without either side being able to close a handle under the other, and
/// without a cancellation arriving before there is anything to cancel.
///
/// Three problems this solves, each found in review of a previous draft:
///
/// **A handle must not be used after its owner has let it go.** The first
/// draft kept a copy of a raw handle the worker owned and asked whether the
/// worker had finished before using it. A worker can finish and close the
/// handle immediately after that question is answered, and a closed handle's
/// value may already name something else. Sharing ownership instead was
/// worse: a write end the call still holds is a write end the child never
/// sees the end of, and `cat` waits for ever.
///
/// So the worker keeps its stream, and **publishes the handle for exactly as
/// long as it is inside an operation** — under a lock, and withdrawn by a
/// guard, so it is withdrawn on the way out of a panic too. A call can only
/// ever see a handle whose owner is inside a call on it.
///
/// **An operation must not be declared after a stop.** Checking whether the
/// call still wants the operation and declaring it are one step, under the
/// same lock the call takes to stop, so a worker cannot pass a check and then
/// declare behind a stop that has already been decided.
///
/// **A cancellation must outlast the submission it is aimed at.** This is the
/// one the second review found, and it is why `stop` is a loop. Windows
/// documents `CancelIoEx` as affecting operations already **outstanding**, and
/// as answering `ERROR_NOT_FOUND` when there are none. Declaring an operation
/// and submitting it cannot be one step — the lock cannot be held across a
/// blocking write, or nothing could cancel it — so a single cancellation can
/// land in the gap between them and cancel nothing at all, leaving the write
/// to block afterwards with no-one asking again.
///
/// So a call does not ask once. It asks, and keeps asking while the operation
/// is still declared: whatever the worker was doing when the first ask
/// arrived, a later ask finds the operation outstanding and cancels it, the
/// worker's guard withdraws the handle, and the loop ends. What bounds the
/// loop is `LONGEST_REACH` rather than the argument, because a cleanup path
/// does not get to wait indefinitely on reasoning.
///
/// The lock is never held across an operation, nor across a wait. It guards
/// two assignments.
#[derive(Clone)]
pub struct Pipe(std::sync::Arc<std::sync::Mutex<Progress>>);

struct Progress {
	/// Set by the call when it is finished with this worker.
	stopped: bool,
	/// How many times a call has asked to reach this worker's operation.
	/// Only the gates read it, and it is per-pipe so two of them running at
	/// once cannot answer for each other.
	#[cfg(test)]
	asks: u64,
	/// The handle of the operation in progress, published by the worker for
	/// as long as it is inside one. `None` means there is nothing to reach.
	in_call: Option<Raw>,
}

/// What a call found when it asked to stop a worker.
#[derive(Debug, PartialEq, Eq)]
pub enum Stopped {
	/// The worker was not inside an operation, and cannot now enter one.
	NothingToReach,
	/// The worker was inside an operation, was asked to abandon it, and left
	/// it. There is no third answer: `stop` does not return while an
	/// operation is still declared.
	Reached,
}

/// A worker's declaration that it is inside an operation. While this lives,
/// the operation's handle is published and a call can reach it; when it dies
/// — by being dropped, at the end of the operation or on the way out of a
/// panic — the handle is withdrawn.
///
/// It is a guard rather than a matching `end()` call because a panic between
/// the two would have left the handle published while the stream it names was
/// dropped: exactly the use-after-close the first problem above describes,
/// arriving by the one route a lock cannot cover.
pub struct InCall<'a>(&'a Pipe);

impl Drop for InCall<'_> {
	fn drop(&mut self) {
		self.0.lock().in_call = None;
	}
}

impl Pipe {
	pub fn new() -> Self {
		Self(std::sync::Arc::new(std::sync::Mutex::new(Progress {
			stopped: false,
			in_call: None,
			#[cfg(test)]
			asks: 0,
		})))
	}

	/// Declare an operation on `stream`, or learn that the call is finished
	/// with this worker. One step, so a stop cannot arrive in the middle of
	/// it. The operation lasts as long as the guard.
	pub fn begin(&self, stream: &impl Stream) -> Option<InCall<'_>> {
		let mut progress = self.lock();
		if progress.stopped {
			return None;
		}
		progress.in_call = Some(raw(stream));
		Some(InCall(self))
	}

	/// Tell the worker to stop, and keep asking until the operation it is
	/// inside is over. Answers whether there was one.
	///
	/// **There is no cutoff, and one would reopen the race it exists to close.**
	/// An earlier draft gave up after a second of asking. A worker descheduled
	/// between declaring and submitting can spend that second there: every ask
	/// finds nothing outstanding, the call gives up, and the worker then submits
	/// a write with nobody left to cancel it. So "still inside after a while"
	/// does not mean cancellation is broken — it can simply mean the operation
	/// has not been submitted yet, and no length of timeout distinguishes the
	/// two.
	///
	/// Nor is this an unbounded wait traded for a bounded one. What follows a
	/// `stop` is the worker's collection, which waits for the **same** event —
	/// the operation ending — without asking for it. Giving up early does not
	/// shorten anything; it only removes the one thing that would end the
	/// operation. So the responsibility is held until the operation withdraws.
	pub fn stop(&self) -> Stopped {
		let mut wait = FIRST_ASK_WAIT;
		let mut asked = false;
		loop {
			{
				let mut progress = self.lock();
				progress.stopped = true;
				let inside = progress.in_call;
				match inside {
					// Asked every time round, not only the first: an ask that
					// arrived before the operation was submitted cancelled
					// nothing, and this is what asks again once it has been.
					Some(handle) => {
						cancel(handle);
						#[cfg(test)]
						{
							progress.asks += 1;
						}
						asked = true;
					}
					None if asked => return Stopped::Reached,
					None => return Stopped::NothingToReach,
				}
			}
			std::thread::sleep(wait);
			wait = (wait * 2).min(LONGEST_ASK_WAIT);
		}
	}

	/// How many times this worker's operation has been asked to stop.
	#[cfg(test)]
	fn asks(&self) -> u64 {
		self.lock().asks
	}

	/// Whether the call has asked this worker to stop.
	pub fn stopped(&self) -> bool {
		self.lock().stopped
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, Progress> {
		// A worker that panicked mid-operation withdrew its handle on the way
		// out — that is what `InCall`'s `Drop` is for — so what a poisoned
		// lock guards here is a consistent state: stopped as it was, nothing
		// published. The call's own collection reports the panic.
		self.0.lock().unwrap_or_else(|e| e.into_inner())
	}
}

impl Default for Pipe {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::{Duration, Instant};

	/// A stream to declare operations on. Its contents do not matter: what is
	/// being gated is the protocol around an operation, not the operation.
	fn stream() -> std::io::PipeReader {
		std::io::pipe().expect("a pipe").0
	}

	/// Wait for something to become true, bounded, and say whether it did.
	/// Nothing here waits without a bound, and nothing reports a timeout as a
	/// pass.
	fn within(how_long: Duration, mut yet: impl FnMut() -> bool) -> bool {
		let ends = Instant::now() + how_long;
		while Instant::now() < ends {
			if yet() {
				return true;
			}
			std::thread::sleep(Duration::from_micros(50));
		}
		yet()
	}

	#[test]
	fn an_operation_is_never_declared_after_a_stop() {
		// A worker cannot pass a check, have the call stop behind it, and then
		// declare. There is no check to pass — declaring is the check.
		let pipe = Pipe::new();
		let held = stream();
		let operation = pipe.begin(&held).expect("declared");
		drop(operation);
		assert_eq!(pipe.stop(), Stopped::NothingToReach);
		assert!(
			pipe.begin(&held).is_none(),
			"an operation was declared after the stop"
		);
		assert!(pipe.stopped());
	}

	#[test]
	fn a_stop_reaches_the_operation_a_worker_is_inside() {
		// The worker leaves its operation as the ask arrives, which is the
		// ordinary case: one ask, and the operation is over.
		let pipe = Pipe::new();
		let held = stream();
		let declared = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
		let worker = {
			let pipe = pipe.clone();
			let declared = declared.clone();
			std::thread::spawn(move || {
				let held = stream();
				let operation = pipe.begin(&held).expect("declared");
				declared.store(true, Ordering::Relaxed);
				within(Duration::from_secs(5), || pipe.asks() >= 1);
				drop(operation);
			})
		};
		assert!(within(Duration::from_secs(5), || declared.load(Ordering::Relaxed)));
		assert_eq!(pipe.stop(), Stopped::Reached);
		worker.join().expect("the worker");
		drop(held);
	}

	#[test]
	fn nothing_is_reachable_once_an_operation_is_over() {
		// The other half of the same guarantee: the handle the platform is
		// given is always one whose owner is inside a call on it, so it cannot
		// be one the owner has closed — and a closed handle's value may
		// already name something else.
		let pipe = Pipe::new();
		let held = stream();
		let operation = pipe.begin(&held).expect("declared");
		drop(operation);
		assert_eq!(pipe.stop(), Stopped::NothingToReach);
	}

	#[test]
	fn a_cancellation_that_arrives_before_the_submission_is_asked_again() {
		// The interleaving that a single ask loses, and the reason `stop` is a
		// loop. The worker declares, and is held there until an ask has
		// arrived with nothing submitted — the ask Windows answers
		// `ERROR_NOT_FOUND`. Only then does it submit, and from that moment
		// its operation ends only if a **later** ask reaches it, which is what
		// a write blocked in the kernel does.
		//
		// A single-ask `stop` cannot pass this: the count stops rising while
		// the worker is still inside, and the worker says so.
		let pipe = Pipe::new();
		let declared = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
		let worker = {
			let pipe = pipe.clone();
			let declared = declared.clone();
			std::thread::spawn(move || {
				let held = stream();
				let operation = pipe.begin(&held).expect("declared");
				declared.store(true, Ordering::Relaxed);
				// Not submitted yet. This is the gap.
				let missed = within(Duration::from_secs(5), || pipe.asks() >= 1);
				let before = pipe.asks();
				// Submitted from here, and outstanding until asked again.
				let asked_again = within(Duration::from_secs(5), || pipe.asks() > before);
				drop(operation);
				(missed, asked_again)
			})
		};
		assert!(within(Duration::from_secs(5), || declared.load(Ordering::Relaxed)));
		let stopped = pipe.stop();
		let (missed, asked_again) = worker.join().expect("the worker");
		assert!(missed, "the first ask never arrived");
		assert!(
			asked_again,
			"the ask arrived before the operation was submitted and was never repeated, \
			 so a blocked write would still be blocked"
		);
		assert_eq!(stopped, Stopped::Reached);
	}

	#[test]
	fn asking_does_not_stop_while_the_operation_is_still_declared() {
		// The submission race again, held open past the one-second cutoff an
		// earlier draft gave up after. A worker descheduled between declaring
		// and submitting can spend an arbitrary time there, and every ask that
		// arrives meanwhile finds nothing outstanding: so "still inside after
		// a while" is not evidence that cancellation is broken, and a call
		// that gives up on that evidence leaves the write it was going to
		// cancel with nobody asking. This gate is evidence against that one
		// cutoff; no test can rule out every cutoff, which is why the loop
		// carries the argument for having none.
		//
		// This worker stays between declaring and submitting for well past that
		// second, then submits and requires an ask from **after** that moment. Its own waits are bounded and its
		// teardown does not depend on the ask arriving, so a protocol that
		// gives up fails this rather than hanging it.
		const PAST_THE_FORMER_CUTOFF: Duration = Duration::from_millis(1500);
		let pipe = Pipe::new();
		let declared = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
		let worker = {
			let pipe = pipe.clone();
			let declared = declared.clone();
			std::thread::spawn(move || {
				let held = stream();
				let operation = pipe.begin(&held).expect("declared");
				declared.store(true, Ordering::Relaxed);
				// Nothing submitted for all of this, however much is asked.
				let held_open = Instant::now();
				while held_open.elapsed() < PAST_THE_FORMER_CUTOFF {
					std::thread::sleep(Duration::from_millis(10));
				}
				let asks_while_held = pipe.asks();
				// Submitted from here, and outstanding until asked again.
				let asked_again = within(Duration::from_secs(5), || pipe.asks() > asks_while_held);
				drop(operation);
				(asks_while_held, asked_again)
			})
		};
		assert!(within(Duration::from_secs(5), || declared.load(Ordering::Relaxed)));
		let stopped = pipe.stop();
		let (asks_while_held, asked_again) = worker.join().expect("the worker");
		assert!(
			asks_while_held > 1,
			"only {asks_while_held} asks arrived while the operation was declared"
		);
		assert!(
			asked_again,
			"asking stopped while the operation was still declared, so the write \
			 submitted afterwards would never be cancelled"
		);
		assert_eq!(stopped, Stopped::Reached);
	}

	#[test]
	fn a_panic_inside_an_operation_withdraws_the_handle() {
		// The route a lock cannot cover. The worker dies between declaring and
		// finishing, so its stream is dropped by the unwind; if the handle
		// were still published, the next ask would name a closed handle —
		// which may by then be something else entirely.
		let pipe = Pipe::new();
		let worker = {
			let pipe = pipe.clone();
			std::thread::spawn(move || {
				let held = stream();
				let _operation = pipe.begin(&held).expect("declared");
				panic!("a worker dying inside an operation");
			})
		};
		assert!(worker.join().is_err(), "the worker was supposed to die");
		assert!(
			pipe.lock().in_call.is_none(),
			"the stream is gone and its handle is still reachable"
		);
		assert_eq!(pipe.stop(), Stopped::NothingToReach);
	}

	#[test]
	fn a_worker_never_lets_go_of_a_stream_that_is_still_reachable() {
		// Two hundred races between a worker declaring and finishing
		// operations and a call stopping it. The invariant asserted is the one
		// that makes a use-after-close impossible: at the moment the worker
		// gives up its stream, nothing names it.
		for _ in 0..200 {
			let pipe = Pipe::new();
			let worker = {
				let pipe = pipe.clone();
				std::thread::spawn(move || {
					let held = stream();
					let mut rounds = 0u64;
					let started = Instant::now();
					// Bounded, so a stop that never sticks fails this rather
					// than spinning: which is what a `begin` that ignored the
					// flag did when this gate was controlled against it.
					while let Some(operation) = pipe.begin(&held) {
						rounds += 1;
						drop(operation);
						assert!(
							started.elapsed() < Duration::from_secs(5),
							"the stop did not stop the worker in {rounds} rounds"
						);
					}
					assert!(
						pipe.lock().in_call.is_none(),
						"the stream is about to be dropped and something can still reach it"
					);
					drop(held);
					rounds
				})
			};
			pipe.stop();
			let rounds = worker.join().expect("the worker");
			assert!(
				pipe.stopped(),
				"the stop did not stick after {rounds} rounds"
			);
		}
	}
}
