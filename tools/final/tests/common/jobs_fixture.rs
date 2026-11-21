use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::RwLock;

/// An overlapping fork would duplicate even a CLOEXEC writer until exec.
/// Only the fork/exec handoff needs a shared guard. Children run concurrently.
static EXECUTABLES: RwLock<()> = RwLock::new(());

fn write_executable<T>(write: impl FnOnce() -> T) -> T {
	let _guard = EXECUTABLES.write().unwrap();
	write()
}

/// Fixture data may be written to a caller-supplied path. Use the same
/// exclusion as executable installation so that reusing a path is safe.
pub(super) fn write_fixture(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
	write_executable(|| std::fs::write(path, bytes))
}

fn fork_fixture<T>(fork: impl FnOnce() -> T) -> T {
	let _guard = EXECUTABLES.read().unwrap();
	fork()
}

pub(super) trait FixtureCommand {
	fn start(&mut self) -> io::Result<Child>;
	fn capture(&mut self) -> io::Result<Output>;
}

impl FixtureCommand for Command {
	fn start(&mut self) -> io::Result<Child> {
		fork_fixture(|| self.spawn())
	}

	fn capture(&mut self) -> io::Result<Output> {
		self.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.start()?
			.wait_with_output()
	}
}

pub(super) enum Executable<'a> {
	Copy(&'a Path),
	Script(&'a str),
}

impl Executable<'_> {
	pub(super) fn install(self, dest: &Path) -> io::Result<()> {
		write_executable(|| {
			match self {
				Self::Copy(source) => {
					std::fs::copy(source, dest)?;
				}
				Self::Script(text) => {
					std::fs::write(dest, text)?;
					std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
				}
			}
			Ok(())
		})
	}
}

/// The snapshot tests must go through these helpers for process launches
/// and for copies or permission changes that could create an executable.
pub(super) fn check_source(source: &str) {
	let direct = regex::Regex::new(
		r"\bfs\s*::\s*write\b|\b(spawn|output|status|fork|copy|set_permissions|set_mode|from_mode|mode|chmod|fchmod)\s*\(",
	)
	.unwrap();
	if let Some(call) = direct.find(source) {
		let line = source[..call.start()].lines().count();
		panic!(
			"jobs.rs:{line}: use the guarded fixture helpers for {}",
			call.as_str()
		);
	}
}

#[test]
fn executable_writers_are_closed_before_another_fixture_forks() {
	use std::os::fd::AsRawFd;
	use std::sync::{TryLockError, mpsc};

	// Keep a forked child in the window before exec. Its inherited CLOEXEC
	// descriptors live until the parent closes the pipe. No sleep or retry.
	let hold_fds = || {
		let (reader, release) = io::pipe().unwrap();
		let pid = fork_fixture(|| {
			assert!(matches!(
				EXECUTABLES.try_write(),
				Err(TryLockError::WouldBlock)
			));
			// SAFETY: the child only closes a descriptor, reads one byte and
			// calls _exit. It never re-enters Rust or runs a destructor.
			unsafe {
				let pid = libc::fork();
				if pid == 0 {
					libc::close(release.as_raw_fd());
					let mut byte = 0u8;
					while libc::read(reader.as_raw_fd(), (&raw mut byte).cast(), 1) < 0
						&& *libc::__errno_location() == libc::EINTR
					{}
					libc::_exit(0);
				}
				pid
			}
		});
		assert!(pid > 0, "fork failed: {}", io::Error::last_os_error());
		(release, pid)
	};
	let release_child = |(release, pid): (io::PipeWriter, libc::pid_t)| {
		drop(release);
		let mut status = 0;
		// SAFETY: pid is our child and status is a writable wait status.
		loop {
			let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
			if waited == pid {
				break;
			}
			assert_eq!(
				io::Error::last_os_error().kind(),
				io::ErrorKind::Interrupted
			);
		}
		assert_eq!(status, 0);
	};

	let s = super::scratch();
	let executable = s.0.join("true");
	Executable::Copy(Path::new("/usr/bin/true"))
		.install(&executable)
		.unwrap();
	let open_writer = || {
		std::fs::OpenOptions::new()
			.write(true)
			.open(&executable)
			.unwrap()
	};
	let writer = open_writer(); // Deliberately omit the exclusive guard.
	let holder = hold_fds();
	drop(writer);
	let refused = Command::new(&executable).capture();
	release_child(holder);
	assert_eq!(refused.unwrap_err().raw_os_error(), Some(libc::ETXTBSY));

	let holder = std::thread::scope(|scope| {
		let (opened, ready) = mpsc::channel();
		let (attempted, attempted_rx) = mpsc::channel();
		let worker = scope.spawn(move || {
			ready.recv().unwrap();
			attempted
				.send(matches!(
					EXECUTABLES.try_read(),
					Err(TryLockError::WouldBlock)
				))
				.unwrap();
			hold_fds()
		});
		write_executable(|| {
			let _writer = open_writer();
			opened.send(()).unwrap();
			assert!(attempted_rx.recv().unwrap());
		});
		worker.join().unwrap()
	});
	let ran = Command::new(&executable).capture();
	release_child(holder);
	assert!(ran.unwrap().status.success());
}
