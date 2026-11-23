//! Private, same-process transition between two independently linked file runners.
use crate::program::{Loader, handoff::Journal};
use serde::{Deserialize, Serialize};
use std::{
	cell::RefCell,
	fs::File,
	io,
	os::{
		fd::{AsRawFd, FromRawFd, RawFd},
		unix::fs::MetadataExt,
	},
};

#[cfg(feature = "engine-image")]
use std::{
	io::{Seek, Write},
	os::unix::process::CommandExt,
	process::Command,
};

const MARKER: &str = "--rnx-resume-project-file-v1";
const SEALS: i32 = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;

#[derive(Serialize, Deserialize)]
struct Invocation {
	version: u32,
	pid: u32,
	device: u64,
	inode: u64,
	argv: Vec<String>,
	budget: usize,
	route: serde_json::Value,
	unblock: bool,
	title: bool,
	sources: Journal,
}

pub(crate) struct Resume {
	pub(crate) sources: rune::Sources,
	pub(crate) loader: Loader,
	pub(crate) budget: usize,
}
thread_local! {
	static RESUME: RefCell<Option<Resume>> = const { RefCell::new(None) };
}

pub(crate) fn take_resume() -> Option<Resume> {
	RESUME.take()
}

fn interrupt_mask(how: libc::c_int) -> io::Result<bool> {
	// Both signal sets are fully initialized before they are inspected.
	unsafe {
		let mut set = std::mem::zeroed();
		let mut old = std::mem::zeroed();
		libc::sigemptyset(&mut set);
		libc::sigaddset(&mut set, libc::SIGINT);
		let error = libc::pthread_sigmask(how, &set, &mut old);
		if error != 0 {
			return Err(io::Error::from_raw_os_error(error));
		}
		Ok(libc::sigismember(&old, libc::SIGINT) == 1)
	}
}

#[cfg(feature = "engine-image")]
pub(crate) fn block_interrupt() -> io::Result<bool> {
	let already_blocked = interrupt_mask(libc::SIG_BLOCK)?;
	// A signal handled before this block is no longer pending in the kernel.
	if crate::platform::redirect_project_interrupt() && unsafe { libc::raise(libc::SIGINT) } != 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(!already_blocked)
}

fn duplicate(fd: RawFd) -> io::Result<File> {
	let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
	if copy < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(unsafe { File::from_raw_fd(copy) })
}

#[cfg(feature = "engine-image")]
fn memory_file(name: &std::ffi::CStr, flags: u32) -> io::Result<File> {
	let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | flags) };
	if fd < 0 {
		return Err(io::Error::last_os_error());
	}
	let file = unsafe { File::from_raw_fd(fd) };
	if fd < 3 { duplicate(fd) } else { Ok(file) }
}

#[cfg(feature = "engine-image")]
pub(crate) fn owned_target(fd: RawFd) -> io::Result<File> {
	if fd < 3 {
		return Err(io::Error::other("invalid project executable descriptor"));
	}
	duplicate(fd)
}

#[cfg(feature = "engine-image")]
pub(crate) fn forward(
	exe: &File,
	route: serde_json::Value,
	unblock: bool,
	loader: Loader,
	budget: usize,
) -> crate::Result<std::convert::Infallible> {
	let meta = exe.metadata()?;
	let argv = crate::env::command_line(std::env::args_os().skip(1))?;
	let invocation = Invocation {
		version: 1,
		pid: std::process::id(),
		device: meta.dev(),
		inode: meta.ino(),
		argv,
		budget,
		route,
		unblock,
		title: crate::terminal::handoff_title(None),
		sources: Journal::take(loader)?,
	};
	let mut file = memory_file(c"rnx-project-file", libc::MFD_ALLOW_SEALING)?;
	let fd = file.as_raw_fd();
	serde_json::to_writer(&mut file, &invocation)?;
	file.rewind()?;
	if unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, SEALS) } < 0 {
		return Err(io::Error::last_os_error().into());
	}
	let mut command = Command::new(format!("/proc/self/fd/{}", exe.as_raw_fd()));
	command.arg(format!("{MARKER}={fd}")).args(&invocation.argv);
	// Only the sealed journal is inherited by the compatibility executable.
	unsafe {
		command.pre_exec(move || {
			if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
				return Err(io::Error::last_os_error());
			}
			Ok(())
		});
	}
	assert_empty_output()?;
	Err(command.exec().into())
}

// Rust's stdout exposes flush but not its buffered length. Drain under the two
// stream locks into a private file. Bytes pending at the boundary are a fault.
#[cfg(feature = "engine-image")]
fn assert_empty_output() -> io::Result<()> {
	let mut stdout = io::stdout().lock();
	let mut stderr = io::stderr().lock();
	let sink = memory_file(c"rnx-stdio-check", 0)?;
	fn inspect(stream: &mut impl Write, fd: RawFd, sink: &File) -> io::Result<()> {
		let saved = duplicate(fd);
		if saved
			.as_ref()
			.is_err_and(|e| e.raw_os_error() == Some(libc::EBADF))
		{
			// A closed descriptor still reports an error if data is pending.
			return stream.flush();
		}
		let saved = saved?;
		let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
		if flags < 0 || unsafe { libc::dup2(sink.as_raw_fd(), fd) } < 0 {
			return Err(io::Error::last_os_error());
		}
		let flushed = stream.flush();
		if unsafe {
			libc::dup3(
				saved.as_raw_fd(),
				fd,
				if flags & libc::FD_CLOEXEC != 0 {
					libc::O_CLOEXEC
				} else {
					0
				},
			)
		} < 0
		{
			return Err(io::Error::last_os_error());
		}
		flushed
	}
	inspect(&mut stdout, libc::STDOUT_FILENO, &sink)?;
	inspect(&mut stderr, libc::STDERR_FILENO, &sink)?;
	if sink.metadata()?.len() != 0 {
		return Err(io::Error::other(
			"buffered output before project continuation",
		));
	}
	Ok(())
}

pub(crate) fn resume_requested() -> crate::Result<bool> {
	let Some(arg) = std::env::args_os().nth(1) else {
		return Ok(false);
	};
	let Some(value) = arg
		.to_str()
		.and_then(|s| s.strip_prefix(MARKER))
		.and_then(|s| s.strip_prefix('='))
	else {
		return Ok(false);
	};
	let fd: RawFd = value.parse()?;
	if fd < 3 {
		return Err("invalid project handoff descriptor".into());
	}
	let file = duplicate(fd)?;
	let seals = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GET_SEALS) };
	if seals < 0 || seals & SEALS != SEALS {
		return Err("unsealed project handoff".into());
	}
	let invocation: Invocation = serde_json::from_reader(file)?;
	let meta = std::fs::metadata("/proc/self/exe")?;
	if invocation.version != 1
		|| invocation.pid != std::process::id()
		|| invocation.device != meta.dev()
		|| invocation.inode != meta.ino()
		|| invocation.argv != crate::env::command_line(std::env::args_os().skip(1))?
		|| invocation.route.get("route").and_then(|v| v.as_str()) != Some("legacy_refusal")
	{
		return Err("invalid project handoff identity".into());
	}
	unsafe {
		libc::close(fd);
	}
	crate::terminal::handoff_title(Some(invocation.title));
	let (sources, loader) = invocation.sources.restore()?;
	RESUME.set(Some(Resume {
		sources,
		loader,
		budget: invocation.budget,
	}));
	crate::platform::watch_for_interrupt();
	if invocation.unblock {
		interrupt_mask(libc::SIG_UNBLOCK)?;
	}
	Ok(true)
}
