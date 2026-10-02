//! Record 0142: a forked child that holds copies of every descriptor this
//! process had open at the fork, until the parent releases it. It stands in,
//! deterministically, for the moment a concurrent test's spawn has forked
//! and not yet called exec.
//!
//! The child runs only async-signal-safe calls (`close`, `read`, `_exit`):
//! it was forked from a multithreaded process. The parent decides when it
//! exits, by writing a release byte: not by closing the pipe, since another
//! forked child may hold a copy of the write end, and EOF would then wait
//! for that child. The reap is bounded, with a kill fallback, and a guard
//! kills and reaps the child if the parent never releases it.

pub(crate) struct Held {
	pid: libc::pid_t,
	release: libc::c_int,
}

impl Held {
	/// Fork the child. It inherits every descriptor open now.
	pub(crate) fn fork() -> Held {
		let mut fds = [0 as libc::c_int; 2];
		assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
		let pid = unsafe { libc::fork() };
		assert!(pid >= 0, "fork failed");
		if pid == 0 {
			// the child: raw calls only, then _exit; it returns on the release
			// byte (or EOF, should the parent die first)
			unsafe {
				libc::close(fds[1]);
				let mut byte = 0u8;
				while libc::read(fds[0], (&raw mut byte).cast(), 1) < 0
					&& *libc::__errno_location() == libc::EINTR
				{}
				libc::_exit(0);
			}
		}
		unsafe { libc::close(fds[0]) };
		Held {
			pid,
			release: fds[1],
		}
	}

	/// Let the child exit, then reap it: its inherited copies are gone
	/// when this returns. True when it exited on the release byte within
	/// the bound; false when it had to be killed.
	pub(crate) fn release(mut self) -> bool {
		self.finish(false)
	}

	fn finish(&mut self, kill: bool) -> bool {
		if self.pid == 0 {
			return true;
		}
		let mut released = false;
		unsafe {
			if !kill {
				let byte = 1u8;
				released = libc::write(self.release, (&raw const byte).cast(), 1) == 1;
			}
			libc::close(self.release);
			let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
			let mut status = 0;
			let mut reaped = false;
			while released && std::time::Instant::now() < deadline {
				if libc::waitpid(self.pid, &mut status, libc::WNOHANG) == self.pid {
					reaped = true;
					break;
				}
				std::thread::sleep(std::time::Duration::from_millis(1));
			}
			if !reaped {
				released = false;
				libc::kill(self.pid, libc::SIGKILL);
				while libc::waitpid(self.pid, &mut status, 0) < 0
					&& *libc::__errno_location() == libc::EINTR
				{}
			}
		}
		self.pid = 0;
		released
	}
}

/// Codex's reproducer for 0142 round 1, kept as the control: releasing one
/// child returns while another descriptor-holding child is still alive
/// (it inherited the first's release writer).
#[test]
fn a_release_does_not_wait_for_another_held_child() {
	let first = Held::fork();
	let second = Held::fork();
	let start = std::time::Instant::now();
	assert!(
		first.release(),
		"the first child exited on its release byte"
	);
	assert!(start.elapsed() < std::time::Duration::from_secs(2));
	assert!(second.release());
}

impl Drop for Held {
	fn drop(&mut self) {
		// not released (an assertion failed first): kill and reap it
		self.finish(true);
	}
}
