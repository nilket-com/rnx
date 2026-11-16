//! Each stage runs under a guardian: a process of this binary outside the
//! stage's process group that starts the stage, and when the stage ends, is
//! stopped, or the controller dies, terminates every descendant of the
//! stage, detached ones included, then records that it did.
//!
//! The controller spawns the guardian in a new session, records it, and only
//! then releases it through a pipe; the guardian records the stage's process
//! before releasing the stage the same way. A record therefore never names a
//! process that has not been identified by its start time.
use crate::receipt::{Field, atomic_json, invocation_id};
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::ffi::{CString, OsString};
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a stopped stage has to exit before it is killed.
pub const GRACE: Duration = Duration::from_secs(30);
/// How long the controller waits for a stopped guardian to finish draining.
pub const DRAIN_MARGIN: Duration = Duration::from_secs(30);
/// Exit status of a guardian that was never released.
pub const UNRELEASED: i32 = 125;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Process {
	pub pid: i32,
	/// Start time in clock ticks after boot (/proc/PID/stat field 22).
	pub started: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Completion {
	/// The stage's exit status, or 128 + its signal.
	pub status: i32,
	/// Descendants killed after the stage itself ended.
	pub killed: u64,
}

/// GUARD-<id>.json in the final's group registry. Distinct from the Python
/// controller's GROUP records, which its recovery reads; this schema is
/// read only by this controller.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Record {
	pub schema: u64,
	pub invocation: String,
	pub guardian: Process,
	/// Null until the guardian has identified the stage, before it runs.
	pub stage: Field<Process>,
	/// Written only after every descendant has been reaped.
	pub completion: Field<Completion>,
}

pub fn read_record(path: &Path) -> Result<Record> {
	Ok(serde_json::from_slice(
		&std::fs::read(path).with_context(|| format!("read {}", path.display()))?,
	)?)
}

/// A process's start time, or None when it no longer exists.
pub fn start_ticks(pid: i32) -> Option<u64> {
	let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
	let fields: Vec<&str> = text[text.rfind(')')? + 2..].split_whitespace().collect();
	fields.get(19)?.parse().ok()
}

/// True while the process recorded as `process` still exists as itself.
pub fn alive(process: &Process) -> bool {
	start_ticks(process.pid) == Some(process.started)
}

fn last_error(what: &str) -> anyhow::Error {
	anyhow::Error::new(std::io::Error::last_os_error()).context(what.to_owned())
}

fn pipe() -> Result<(OwnedFd, OwnedFd)> {
	let mut fds = [0; 2];
	// SAFETY: fds has room for two descriptors.
	if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
		return Err(last_error("pipe2"));
	}
	// SAFETY: pipe2 returned two new descriptors this function now owns.
	Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn pidfd(pid: i32) -> Result<OwnedFd> {
	// SAFETY: pidfd_open takes a pid and flags and returns a new descriptor.
	let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
	if fd < 0 {
		return Err(last_error("pidfd_open"));
	}
	// SAFETY: a new descriptor this function owns.
	Ok(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

/// Wait for a process to exit, at most `limit`; true when it exited. A
/// kernel exit notification, never a sampling loop.
pub fn wait_exit(pidfd: &OwnedFd, limit: Option<Duration>) -> Result<bool> {
	let deadline = limit.map(|limit| std::time::Instant::now() + limit);
	loop {
		let timeout = match deadline {
			None => -1,
			Some(deadline) => deadline
				.saturating_duration_since(std::time::Instant::now())
				.as_millis()
				.min(i32::MAX as u128) as i32,
		};
		let mut ready = libc::pollfd {
			fd: pidfd.as_raw_fd(),
			events: libc::POLLIN,
			revents: 0,
		};
		// SAFETY: one valid pollfd.
		match unsafe { libc::poll(&mut ready, 1, timeout) } {
			1 => return Ok(true),
			0 => return Ok(false),
			_ => {
				let error = std::io::Error::last_os_error();
				if error.kind() != std::io::ErrorKind::Interrupted {
					return Err(error.into());
				}
			}
		}
	}
}

/// A stop request another thread raises; a guarded wait wakes on it.
pub struct StopSignal {
	read: OwnedFd,
	write: OwnedFd,
}

impl StopSignal {
	pub fn new() -> Result<Self> {
		let (read, write) = pipe()?;
		for fd in [&read, &write] {
			// SAFETY: setting O_NONBLOCK on a descriptor this value owns.
			let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
			if flags < 0
				|| unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
					< 0
			{
				return Err(last_error("O_NONBLOCK"));
			}
		}
		Ok(Self { read, write })
	}

	/// Raise the request; it stays raised.
	pub fn raise(&self) {
		// SAFETY: one byte to a nonblocking pipe; a full pipe is already raised.
		unsafe { libc::write(self.write.as_raw_fd(), b"!".as_ptr().cast(), 1) };
	}
}

enum Woke {
	Exited,
	Stopped,
	TimedOut,
}

/// The process exits, the stop is raised, or the time runs out: whichever
/// comes first, by kernel notification.
fn wait_either(
	pidfd: &OwnedFd,
	stop: Option<&StopSignal>,
	limit: Option<Duration>,
) -> Result<Woke> {
	let deadline = limit.map(|limit| std::time::Instant::now() + limit);
	loop {
		let timeout = match deadline {
			None => -1,
			Some(deadline) => deadline
				.saturating_duration_since(std::time::Instant::now())
				.as_millis()
				.min(i32::MAX as u128) as i32,
		};
		let mut fds = vec![libc::pollfd {
			fd: pidfd.as_raw_fd(),
			events: libc::POLLIN,
			revents: 0,
		}];
		if let Some(stop) = stop {
			fds.push(libc::pollfd {
				fd: stop.read.as_raw_fd(),
				events: libc::POLLIN,
				revents: 0,
			});
		}
		// SAFETY: a valid array of pollfds.
		match unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) } {
			0 => return Ok(Woke::TimedOut),
			n if n > 0 => {
				if fds[0].revents != 0 {
					return Ok(Woke::Exited);
				}
				return Ok(Woke::Stopped);
			}
			_ => {
				let error = std::io::Error::last_os_error();
				if error.kind() != std::io::ErrorKind::Interrupted {
					return Err(error.into());
				}
			}
		}
	}
}

/// Signal a recorded process only while it is still the one recorded.
pub fn signal(process: &Process, signal: i32) -> Result<bool> {
	let fd = match pidfd(process.pid) {
		Ok(fd) => fd,
		Err(_) => return Ok(false),
	};
	// The pidfd pins the identity: checked after opening, it cannot change.
	if !alive(process) {
		return Ok(false);
	}
	// SAFETY: pidfd_send_signal with a valid pidfd and no siginfo.
	let sent = unsafe {
		libc::syscall(
			libc::SYS_pidfd_send_signal,
			fd.as_raw_fd(),
			signal,
			std::ptr::null::<libc::siginfo_t>(),
			0,
		)
	};
	Ok(sent == 0)
}

/// Every process in group `pgid` whose start is not before `since`, SIGKILLed
/// and waited for. The leader's start time bounds membership, so a reused
/// group id cannot reach older processes.
fn drain_group(pgid: i32, since: u64) -> Result<()> {
	let mut fds = Vec::new();
	for entry in std::fs::read_dir("/proc")?.flatten() {
		let Some(pid) = entry
			.file_name()
			.to_str()
			.and_then(|n| n.parse::<i32>().ok())
		else {
			continue;
		};
		let Ok(fd) = pidfd(pid) else { continue };
		let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
		let Some(close) = stat.rfind(')') else {
			continue;
		};
		let fields: Vec<&str> = stat[close + 2..].split_whitespace().collect();
		let (group, started) = (
			fields.get(2).and_then(|g| g.parse::<i32>().ok()),
			fields.get(19).and_then(|s| s.parse::<u64>().ok()),
		);
		if group == Some(pgid) && started.is_some_and(|s| s >= since) {
			// SAFETY: pidfd_send_signal with a valid pidfd.
			unsafe {
				libc::syscall(
					libc::SYS_pidfd_send_signal,
					fd.as_raw_fd(),
					libc::SIGKILL,
					std::ptr::null::<libc::siginfo_t>(),
					0,
				)
			};
			fds.push(fd);
		}
	}
	for fd in fds {
		wait_exit(&fd, None)?;
	}
	Ok(())
}

/// What became of a guarded stage.
#[derive(Debug)]
pub struct Outcome {
	pub status: i32,
	pub record: PathBuf,
	/// Why the stage did not end normally, if it did not.
	pub stopped: Option<String>,
}

/// One guarded stage, from the controller's side.
pub struct Guarded {
	pub record: PathBuf,
	/// How long a stopped guardian has to finish draining.
	pub drain_deadline: Duration,
	child: std::process::Child,
	pidfd: OwnedFd,
	guardian: Process,
}

impl Guarded {
	pub fn guardian(&self) -> &Process {
		&self.guardian
	}

	/// Ask the guardian to stop the stage; it drains and records.
	pub fn stop(&self) -> Result<()> {
		signal(&self.guardian, libc::SIGTERM)?;
		Ok(())
	}

	/// Block until the guardian exits, stopping the stage after `timeout`.
	pub fn wait(self, timeout: Option<Duration>) -> Result<Outcome> {
		self.wait_until(timeout, None)
	}

	/// Block until the guardian exits. After `timeout`, or as soon as `stop`
	/// is raised, the guardian is stopped and has `drain_deadline` to finish
	/// draining; past it, it and its group are killed and the drain is
	/// reported incomplete.
	pub fn wait_until(
		mut self,
		timeout: Option<Duration>,
		stop: Option<&StopSignal>,
	) -> Result<Outcome> {
		let mut stopped = None;
		match wait_either(&self.pidfd, stop, timeout)? {
			Woke::Exited => {}
			woke => {
				stopped = Some(match woke {
					Woke::Stopped => "stage stopped by the controller".to_owned(),
					_ => format!(
						"stage timed out after {} seconds",
						timeout.unwrap_or_default().as_secs()
					),
				});
				self.stop()?;
				if !wait_exit(&self.pidfd, Some(self.drain_deadline))? {
					// The guardian did not finish draining: kill it, its own
					// group and its stage group, and report the drain incomplete.
					signal(&self.guardian, libc::SIGKILL)?;
					wait_exit(&self.pidfd, None)?;
					drain_group(self.guardian.pid, self.guardian.started)?;
					if let Ok(Record {
						stage: Field::Value(stage),
						..
					}) = read_record(&self.record)
					{
						drain_group(stage.pid, stage.started)?;
					}
					stopped = Some("stage did not drain after it was stopped".into());
				}
			}
		}
		let status = self.child.wait()?;
		let status = status.code().unwrap_or_else(|| {
			128 + std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0)
		});
		let record = read_record(&self.record)?;
		if record.completion.is_absent() || matches!(record.completion, Field::Null) {
			stopped
				.get_or_insert_with(|| "stage guardian ended without a completion record".into());
		}
		Ok(Outcome {
			status,
			record: self.record,
			stopped,
		})
	}
}

/// Start `command` under a guardian. The guardian is identified and
/// recorded in `groups` before it may start the stage.
pub fn spawn(
	guardian_exe: &Path,
	groups: &Path,
	command: &[String],
	cwd: &Path,
	environment: &[(OsString, OsString)],
	stdout: std::fs::File,
	stderr: std::fs::File,
) -> Result<Guarded> {
	ensure!(!command.is_empty(), "empty stage command");
	let invocation = invocation_id();
	let record = groups.join(format!("GUARD-{invocation}.json"));
	let (release_read, release_write) = pipe()?;
	let read_fd = release_read.as_raw_fd();
	let mut launcher = std::process::Command::new(guardian_exe);
	launcher
		.arg("guardian")
		.arg("--release-fd")
		.arg(read_fd.to_string())
		.arg("--parent")
		.arg(std::process::id().to_string())
		.arg("--record")
		.arg(&record)
		.arg("--")
		.args(command)
		.current_dir(cwd)
		.env_clear()
		.envs(environment.iter().map(|(k, v)| (k, v)))
		.stdin(std::process::Stdio::null())
		.stdout(stdout)
		.stderr(stderr);
	// SAFETY: only async-signal-safe calls between fork and exec.
	unsafe {
		launcher.pre_exec(move || {
			if libc::setsid() < 0 {
				return Err(std::io::Error::last_os_error());
			}
			// The read end alone crosses exec; the write end stays CLOEXEC,
			// so the guardian sees EOF if the controller dies first.
			if libc::fcntl(read_fd, libc::F_SETFD, 0) < 0 {
				return Err(std::io::Error::last_os_error());
			}
			Ok(())
		});
	}
	let child = launcher.spawn().context("start stage guardian")?;
	drop(release_read);
	let pid = child.id() as i32;
	let fd = pidfd(pid)?;
	let started = start_ticks(pid).context("stage guardian exited before it was recorded")?;
	let guardian = Process { pid, started };
	let row = Record {
		schema: 1,
		invocation,
		guardian: guardian.clone(),
		stage: Field::Null,
		completion: Field::Null,
	};
	let mut guarded = Guarded {
		record: record.clone(),
		drain_deadline: GRACE + DRAIN_MARGIN,
		child,
		pidfd: fd,
		guardian,
	};
	if let Err(error) = atomic_json(&record, &row) {
		// Never release an unrecorded guardian: EOF makes it exit unrun.
		drop(release_write);
		let _ = guarded.child.wait();
		return Err(error.context("record stage guardian"));
	}
	let mut release = std::fs::File::from(release_write);
	std::io::Write::write_all(&mut release, b"!").context("release stage guardian")?;
	Ok(guarded)
}

/// `rnx-final guardian`: the guardian process itself.
pub fn main(arguments: &[OsString]) -> Result<i32> {
	let mut iter = arguments.iter();
	let (mut release_fd, mut parent, mut record) = (None, None, None);
	let mut command = Vec::new();
	while let Some(argument) = iter.next() {
		match argument.to_str() {
			Some("--release-fd") => {
				release_fd = iter.next().and_then(|v| v.to_str()?.parse::<RawFd>().ok())
			}
			Some("--parent") => parent = iter.next().and_then(|v| v.to_str()?.parse::<i32>().ok()),
			Some("--record") => record = iter.next().map(PathBuf::from),
			Some("--") => {
				command = iter.by_ref().cloned().collect();
				break;
			}
			_ => bail!("guardian: unknown argument {argument:?}"),
		}
	}
	let (release_fd, parent, record) = (
		release_fd.context("guardian: --release-fd")?,
		parent.context("guardian: --parent")?,
		record.context("guardian: --record")?,
	);
	ensure!(!command.is_empty(), "guardian: no command");
	// SAFETY: the descriptor was passed to this process to own.
	let release = unsafe { OwnedFd::from_raw_fd(release_fd) };

	// Signals arrive only through sigtimedwait below: none is lost between
	// these setup steps and the wait.
	let mut blocked: libc::sigset_t = unsafe { std::mem::zeroed() };
	// SAFETY: sigset operations on a local set; sigprocmask on this thread,
	// which is the guardian's only thread.
	unsafe {
		libc::sigemptyset(&mut blocked);
		for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP, libc::SIGCHLD] {
			libc::sigaddset(&mut blocked, signal);
		}
		if libc::sigprocmask(libc::SIG_BLOCK, &blocked, std::ptr::null_mut()) != 0 {
			return Err(last_error("sigprocmask"));
		}
		if libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0 {
			return Err(last_error("PR_SET_CHILD_SUBREAPER"));
		}
		if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0) != 0 {
			return Err(last_error("PR_SET_PDEATHSIG"));
		}
	}
	// The controller may have died before PDEATHSIG was armed.
	if unsafe { libc::getppid() } != parent {
		return Ok(UNRELEASED);
	}
	let mut byte = [0u8; 1];
	let released = loop {
		// SAFETY: reading one byte into a local buffer.
		match unsafe { libc::read(release.as_raw_fd(), byte.as_mut_ptr().cast(), 1) } {
			1 => break byte[0] == b'!',
			0 => break false,
			_ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => {
				continue;
			}
			_ => break false,
		}
	};
	drop(release);
	if !released {
		return Ok(UNRELEASED);
	}
	run(&record, &command, &blocked)
}

fn run(record_path: &Path, command: &[OsString], blocked: &libc::sigset_t) -> Result<i32> {
	let mut record = read_record(record_path)?;
	let program = CString::new(command[0].as_bytes())?;
	let argv: Vec<CString> = command
		.iter()
		.map(|a| CString::new(a.as_bytes()))
		.collect::<Result<_, _>>()?;
	let mut argv_ptrs: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
	argv_ptrs.push(std::ptr::null());
	let (gate_read, gate_write) = pipe()?;
	// SAFETY: this process has one thread, so the child may run ordinary code
	// until exec; it still uses only async-signal-safe calls.
	let pid = unsafe { libc::fork() };
	if pid < 0 {
		return Err(last_error("fork"));
	}
	if pid == 0 {
		// The child owns only the read end, so the guardian's death is EOF.
		unsafe {
			libc::close(gate_write.as_raw_fd());
			if libc::setpgid(0, 0) != 0 {
				libc::_exit(UNRELEASED);
			}
			let mut byte = 0u8;
			let ready = libc::read(gate_read.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) == 1
				&& byte == b'!';
			libc::close(gate_read.as_raw_fd());
			if !ready {
				libc::_exit(UNRELEASED);
			}
			// As Python's subprocess restores them: the stage gets default
			// dispositions for the signals a Rust or Python parent ignores.
			for signal in [libc::SIGPIPE, libc::SIGXFSZ] {
				if libc::signal(signal, libc::SIG_DFL) == libc::SIG_ERR {
					libc::_exit(UNRELEASED);
				}
			}
			if libc::sigprocmask(libc::SIG_UNBLOCK, blocked, std::ptr::null_mut()) != 0 {
				libc::_exit(UNRELEASED);
			}
			libc::execvp(program.as_ptr(), argv_ptrs.as_ptr());
			libc::_exit(127);
		}
	}
	drop(gate_read);
	// The stage's group exists once either side has set it; a race with
	// the child's own setpgid is the only acceptable failure (EACCES after
	// its exec, which cannot happen before the gate opens).
	if unsafe { libc::setpgid(pid, pid) } != 0
		&& std::io::Error::last_os_error().raw_os_error() != Some(libc::EACCES)
	{
		drop(gate_write);
		drain(pid)?;
		return Err(last_error("setpgid for the stage"));
	}
	let stage = Process {
		pid,
		started: start_ticks(pid).context("stage exited before it was recorded")?,
	};
	record.stage = Field::Value(stage.clone());
	if atomic_json(record_path, &record).is_err() {
		drop(gate_write);
		drain(pid)?;
		return Ok(UNRELEASED);
	}
	// A planted fault for the controls: the guardian dies with its stage
	// recorded and unreleased. Never honored inside a final.
	if std::env::var_os("RNX_FINAL").is_none()
		&& std::env::var("RNX_GUARDIAN_FAULT").as_deref() == Ok("die-before-stage-release")
	{
		std::process::exit(UNRELEASED + 1);
	}
	let mut gate = std::fs::File::from(gate_write);
	std::io::Write::write_all(&mut gate, b"!")?;
	drop(gate);

	let mut status = None;
	let mut stopping: Option<std::time::Instant> = None;
	while status.is_none() {
		let timeout = stopping.map(|since| GRACE.saturating_sub(since.elapsed()));
		if timeout == Some(Duration::ZERO) {
			break;
		}
		let spec = timeout.map(|t| libc::timespec {
			tv_sec: t.as_secs() as libc::time_t,
			tv_nsec: t.subsec_nanos() as libc::c_long,
		});
		// SAFETY: waits on this thread's blocked signals.
		let signal = unsafe {
			libc::sigtimedwait(
				blocked,
				std::ptr::null_mut(),
				spec.as_ref().map_or(std::ptr::null(), |s| s as *const _),
			)
		};
		match signal {
			libc::SIGCHLD => status = reap(pid),
			libc::SIGTERM | libc::SIGINT | libc::SIGHUP if stopping.is_none() => {
				unsafe { libc::kill(-pid, libc::SIGTERM) };
				stopping = Some(std::time::Instant::now());
			}
			_ => {}
		}
	}
	let killed = drain(pid)?;
	let status = status.unwrap_or(128 + libc::SIGKILL);
	record.completion = Field::Value(Completion { status, killed });
	atomic_json(record_path, &record)?;
	Ok(status)
}

/// Reap whatever has exited; the stage's own status when it is among them.
fn reap(stage: i32) -> Option<i32> {
	let mut found = None;
	loop {
		let mut status = 0;
		// SAFETY: waitpid on any child, without blocking.
		let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
		if pid <= 0 {
			return found;
		}
		if pid == stage {
			found = Some(if libc::WIFEXITED(status) {
				libc::WEXITSTATUS(status)
			} else {
				128 + libc::WTERMSIG(status)
			});
		}
	}
}

/// This process's children, zombies included: every process whose parent
/// is this one. (The kernel's per-task children lists are not always built.)
fn children() -> Result<Vec<i32>> {
	let me = std::process::id() as i32;
	let mut pids = Vec::new();
	for entry in std::fs::read_dir("/proc")?.flatten() {
		let Some(pid) = entry
			.file_name()
			.to_str()
			.and_then(|n| n.parse::<i32>().ok())
		else {
			continue;
		};
		let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
			continue;
		};
		let Some(close) = stat.rfind(')') else {
			continue;
		};
		if stat[close + 2..]
			.split_whitespace()
			.nth(1)
			.and_then(|p| p.parse::<i32>().ok())
			== Some(me)
		{
			pids.push(pid);
		}
	}
	Ok(pids)
}

/// Kill the stage's group and then every descendant, which as orphans
/// become this subreaper's children, until none is left; reap them all.
/// Each pass kills every current child before blocking on one exit, so a
/// child adopted during the wait is killed on the next pass.
fn drain(stage_pid: i32) -> Result<u64> {
	// SAFETY: the group was created by this guardian's child.
	unsafe { libc::kill(-stage_pid, libc::SIGKILL) };
	let mut killed = 0;
	loop {
		let pids = children()?;
		if pids.is_empty() {
			return Ok(killed);
		}
		for pid in &pids {
			// An unreaped child's pid cannot be reused, so it is safe to signal.
			unsafe { libc::kill(*pid, libc::SIGKILL) };
		}
		let mut status = 0;
		// SAFETY: blocking reap of any child; at least one was just killed.
		let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
		if pid > 0 && pid != stage_pid {
			killed += 1;
		}
	}
}

/// The control group of this process's own systemd unit, when the unit
/// holds nothing but this final: an invocation-specific containment.
pub fn own_unit() -> Result<String> {
	let membership = std::fs::read_to_string("/proc/self/cgroup")?;
	let path = membership
		.lines()
		.find_map(|l| l.strip_prefix("0::"))
		.context("no unified control group")?
		.to_owned();
	let leaf = path.rsplit('/').next().unwrap_or_default();
	ensure!(
		leaf.ends_with(".service") && !leaf.starts_with("user@"),
		"a complete final runs in its own systemd unit; this process is in {path}"
	);
	let procs = std::fs::read_to_string(format!("/sys/fs/cgroup{path}/cgroup.procs"))?;
	let me = std::process::id().to_string();
	ensure!(
		procs.split_whitespace().all(|p| p == me),
		"the final's systemd unit {path} holds other processes"
	);
	Ok(path)
}

/// Whether a unit's control group has ended: it says it holds no process,
/// or the group itself is positively gone. Any other observation, an
/// unreadable or malformed one included, is an error, never an ending.
pub fn unit_ended(path: &str) -> Result<bool> {
	unit_ended_under(Path::new("/sys/fs/cgroup"), path)
}

pub fn unit_ended_under(cgroup_root: &Path, path: &str) -> Result<bool> {
	ensure!(
		path.starts_with('/') && !path.split('/').any(|p| p == ".." || p == "."),
		"malformed unit control group: {path}"
	);
	let group = cgroup_root.join(path.trim_start_matches('/'));
	match std::fs::read_to_string(group.join("cgroup.events")) {
		Ok(events) => match events.lines().find_map(|l| l.strip_prefix("populated ")) {
			Some("0") => Ok(true),
			Some("1") => Ok(false),
			_ => bail!("unit control group {path} reports no populated state"),
		},
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
			match std::fs::symlink_metadata(&group) {
				Err(gone) if gone.kind() == std::io::ErrorKind::NotFound => Ok(true),
				Err(other) => {
					Err(anyhow::Error::new(other).context(format!("unit control group {path}")))
				}
				Ok(_) => bail!("unit control group {path} exists without its accounting"),
			}
		}
		Err(error) => Err(anyhow::Error::new(error).context(format!("unit control group {path}"))),
	}
}

/// Python stage programs' own GROUP records (child_result's run_process),
/// drained as its recovery drains them: by recorded leader identity.
pub fn drain_python_groups(groups: &Path) -> Result<()> {
	let mut records: Vec<PathBuf> = std::fs::read_dir(groups)?
		.flatten()
		.map(|e| e.path())
		.filter(|p| {
			p.file_name().is_some_and(|n| {
				n.to_string_lossy().starts_with("GROUP-") && n.to_string_lossy().ends_with(".json")
			})
		})
		.collect();
	records.sort();
	for path in records {
		let mut row: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
		if row
			.get("drained")
			.is_some_and(|d| d.as_bool() == Some(true))
		{
			continue;
		}
		let pid = row["pid"].as_i64().context("group record without pid")? as i32;
		let started = row["started"]
			.as_u64()
			.context("group record without start")?;
		match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
			Ok(stat) => {
				let fields: Vec<&str> = stat[stat.rfind(')').map_or(0, |i| i + 2)..]
					.split_whitespace()
					.collect();
				let group = fields.get(2).and_then(|g| g.parse::<i32>().ok());
				let start = fields.get(19).and_then(|s| s.parse::<u64>().ok());
				ensure!(
					group == Some(pid) && start == Some(started),
					"producer pid reused"
				);
			}
			Err(_) => {
				// Members cannot predate their recorded leader: refuse ambiguity.
				for entry in std::fs::read_dir("/proc")?.flatten() {
					let Some(other) = entry
						.file_name()
						.to_str()
						.and_then(|n| n.parse::<i32>().ok())
					else {
						continue;
					};
					let Ok(stat) = std::fs::read_to_string(format!("/proc/{other}/stat")) else {
						continue;
					};
					let fields: Vec<&str> = stat[stat.rfind(')').map_or(0, |i| i + 2)..]
						.split_whitespace()
						.collect();
					if fields.get(2).and_then(|g| g.parse::<i32>().ok()) == Some(pid)
						&& fields
							.get(19)
							.and_then(|s| s.parse::<u64>().ok())
							.is_some_and(|s| s < started)
					{
						bail!("producer group identity ambiguous");
					}
				}
			}
		}
		drain_group(pid, started)?;
		row["drained"] = serde_json::json!(true);
		atomic_json(&path, &row)?;
	}
	Ok(())
}

/// Every guardian of a final finished draining, or is made to; a guardian
/// that died without its completion record is accepted only once the
/// final's own unit has ended. Otherwise release must not proceed.
pub fn drain_guardians(groups: &Path, unit: Option<&str>) -> Result<()> {
	let mut records: Vec<PathBuf> = std::fs::read_dir(groups)?
		.flatten()
		.map(|e| e.path())
		.filter(|p| {
			p.file_name().is_some_and(|n| {
				n.to_string_lossy().starts_with("GUARD-") && n.to_string_lossy().ends_with(".json")
			})
		})
		.collect();
	records.sort();
	let mut incomplete = Vec::new();
	for path in records {
		let mut record = read_record(&path)?;
		if record.completion.value().is_none() && alive(&record.guardian) {
			if let Ok(fd) = pidfd(record.guardian.pid)
				&& alive(&record.guardian)
			{
				signal(&record.guardian, libc::SIGTERM)?;
				wait_exit(&fd, Some(GRACE + DRAIN_MARGIN))?;
			}
			record = read_record(&path)?;
		}
		if record.completion.value().is_some() {
			continue;
		}
		if let Field::Value(stage) = &record.stage {
			drain_group(stage.pid, stage.started)?;
		}
		let ended = match unit {
			Some(unit) => unit_ended(unit).with_context(|| {
				format!("recovery cannot establish that {} drained", path.display())
			})?,
			None => false,
		};
		if !ended {
			incomplete.push(
				path.file_name()
					.unwrap_or_default()
					.to_string_lossy()
					.into_owned(),
			);
		}
	}
	ensure!(
		incomplete.is_empty(),
		"stage drain incomplete, roots kept until the final's unit has ended: {}",
		incomplete.join(", ")
	);
	Ok(())
}

/// Recovery's drain for a final's group registry: the stage programs'
/// groups, then every guardian.
pub fn drain_registry(groups: &Path, ledger: &crate::evidence::Ledger) -> Result<()> {
	drain_python_groups(groups)?;
	drain_guardians(groups, ledger.unit.value().map(String::as_str))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn tree() -> PathBuf {
		let root = std::env::temp_dir().join(format!(
			"rnx-final-unit-{}",
			crate::receipt::invocation_id()
		));
		std::fs::create_dir_all(root.join("user.slice/final.service")).unwrap();
		root
	}

	#[test]
	fn only_an_empty_or_vanished_unit_has_ended() {
		let root = tree();
		let group = root.join("user.slice/final.service");
		std::fs::write(group.join("cgroup.events"), "populated 1\nfrozen 0\n").unwrap();
		assert!(!unit_ended_under(&root, "/user.slice/final.service").unwrap());
		std::fs::write(group.join("cgroup.events"), "populated 0\nfrozen 0\n").unwrap();
		assert!(unit_ended_under(&root, "/user.slice/final.service").unwrap());
		assert!(
			unit_ended_under(&root, "/user.slice/gone.service").unwrap(),
			"a vanished group has ended"
		);
		std::fs::write(group.join("cgroup.events"), "frozen 0\n").unwrap();
		assert!(
			unit_ended_under(&root, "/user.slice/final.service").is_err(),
			"no populated state"
		);
		std::fs::remove_file(group.join("cgroup.events")).unwrap();
		assert!(
			unit_ended_under(&root, "/user.slice/final.service").is_err(),
			"an existing group without accounting"
		);
		assert!(unit_ended_under(&root, "user.slice/final.service").is_err());
		assert!(unit_ended_under(&root, "/user.slice/../final.service").is_err());
		std::fs::remove_dir_all(&root).unwrap();
	}

	#[test]
	fn an_unreadable_group_is_not_an_ended_one() {
		use std::os::unix::fs::PermissionsExt as _;
		let root = tree();
		let group = root.join("user.slice/final.service");
		std::fs::write(group.join("cgroup.events"), "populated 0\n").unwrap();
		std::fs::set_permissions(
			group.join("cgroup.events"),
			std::fs::Permissions::from_mode(0o000),
		)
		.unwrap();
		// Root reads through any mode; the control means nothing there.
		if std::fs::read_to_string(group.join("cgroup.events")).is_err() {
			assert!(
				unit_ended_under(&root, "/user.slice/final.service").is_err(),
				"EACCES authorized release"
			);
		}
		std::fs::set_permissions(
			group.join("cgroup.events"),
			std::fs::Permissions::from_mode(0o600),
		)
		.unwrap();
		std::fs::remove_dir_all(&root).unwrap();
	}
}
