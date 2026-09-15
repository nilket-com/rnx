//! Linux lifecycle ownership. One reaper; no unrelated children in the server.
use crate::transport::Res;
use std::{
	collections::BTreeSet,
	os::fd::{AsRawFd, FromRawFd, OwnedFd},
};
use tokio::time::{Duration, Instant};
pub fn initialize() -> Res<()> {
	if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0 {
		return Err(std::io::Error::last_os_error().into());
	}
	// Fail before spawning the worker if discovery or stable-identity signalling
	// is unavailable on this host. Signal zero only checks the pidfd capability.
	children()?;
	let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) };
	if fd < 0 {
		return Err(std::io::Error::last_os_error().into());
	}
	let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
	if unsafe {
		libc::syscall(
			libc::SYS_pidfd_send_signal,
			fd.as_raw_fd(),
			0,
			std::ptr::null::<libc::siginfo_t>(),
			0,
		)
	} < 0
	{
		return Err(std::io::Error::last_os_error().into());
	}
	Ok(())
}
fn children() -> Res<BTreeSet<i32>> {
	let mut children = BTreeSet::new();
	for task in std::fs::read_dir("/proc/self/task")? {
		let file = task?.path().join("children");
		match std::fs::read_to_string(file) {
			Ok(text) => {
				for pid in text.split_whitespace() {
					children.insert(pid.parse()?);
				}
			}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
			Err(e) => return Err(e.into()),
		}
	}
	Ok(children)
}
pub fn signal(pid: i32, signal: i32) -> Res<()> {
	if unsafe { libc::kill(pid, signal) } < 0 {
		let e = std::io::Error::last_os_error();
		if e.raw_os_error() != Some(libc::ESRCH) {
			return Err(e.into());
		}
	}
	Ok(())
}
/// No other thread waits for children during this sweep. pidfds plus a fresh
/// parenthood check bind signals to owned identities, even while adoption changes.
pub async fn sweep(worker: i32, end: Instant) -> Res<()> {
	if worker > 0 {
		signal(-worker, libc::SIGKILL)?;
	}
	loop {
		if Instant::now() >= end {
			return Err("descendant cleanup exceeded shutdown deadline".into());
		}
		let current = children()?;
		if current.is_empty() {
			return Ok(());
		}
		for pid in current {
			let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
			if fd < 0 {
				let e = std::io::Error::last_os_error();
				if e.raw_os_error() == Some(libc::ESRCH) {
					continue;
				}
				return Err(e.into());
			}
			let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
			if children()?.contains(&pid)
				&& unsafe {
					libc::syscall(
						libc::SYS_pidfd_send_signal,
						fd.as_raw_fd(),
						libc::SIGKILL,
						std::ptr::null::<libc::siginfo_t>(),
						0,
					)
				} < 0
			{
				let e = std::io::Error::last_os_error();
				if e.raw_os_error() != Some(libc::ESRCH) {
					return Err(e.into());
				}
			}
			let r = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
			if r < 0 {
				let e = std::io::Error::last_os_error();
				if e.raw_os_error() != Some(libc::ECHILD) {
					return Err(e.into());
				}
			}
		}
		tokio::time::sleep(Duration::from_millis(5)).await;
	}
}
