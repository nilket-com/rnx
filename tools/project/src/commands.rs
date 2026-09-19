//! Bounded Cargo output and Unix process-group ownership for tool subprocesses.
use std::{
	io::Read,
	process::{Command, Stdio},
	sync::atomic::{AtomicI32, Ordering},
	time::Duration,
};
static DEADLINE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
#[allow(dead_code)]
pub(crate) fn preparation_deadline() {
	let _ = DEADLINE.set(std::time::Instant::now() + Duration::from_secs(1800));
}
static INTERRUPTED: AtomicI32 = AtomicI32::new(0);
#[cfg(unix)]
extern "C" fn interrupt(signal: libc::c_int) {
	INTERRUPTED.store(signal, Ordering::Relaxed);
}
pub(crate) fn install_signals() -> Result<(), String> {
	#[cfg(unix)]
	{
		// The project tool is a standalone host. No rnx context or existing handler
		// is installed here; async-signal-safe work is only this atomic store.
		unsafe {
			for signal in [libc::SIGINT, libc::SIGTERM] {
				let mut action: libc::sigaction = std::mem::zeroed();
				action.sa_sigaction = interrupt as *const () as usize;
				libc::sigemptyset(&mut action.sa_mask);
				if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
					return Err(std::io::Error::last_os_error().to_string());
				}
			}
		}
		Ok(())
	}
	#[cfg(not(unix))]
	{
		Err("project commands require Unix process supervision in this release".into())
	}
}
pub(crate) fn interrupted() -> bool {
	INTERRUPTED.load(Ordering::Relaxed) != 0
}
pub(crate) fn signal_status() -> i32 {
	128 + INTERRUPTED.load(Ordering::Relaxed)
}
pub(crate) fn check() -> Result<(), String> {
	if DEADLINE
		.get()
		.is_some_and(|d| std::time::Instant::now() >= *d)
	{
		Err("preparation exceeded its 30-minute deadline".into())
	} else if interrupted() {
		Err("interrupted".into())
	} else {
		Ok(())
	}
}
pub(crate) fn run(command: Command, capture: bool) -> Result<Vec<u8>, String> {
	run_inner(command, capture, false, None, crate::input::DOCUMENT_LIMIT)
}
/// Installer Git must bound stderr too; ordinary Cargo output keeps its existing stream.
#[allow(dead_code)]
pub(crate) fn run_bounded(command: Command) -> Result<Vec<u8>, String> {
	run_inner(command, true, true, None, crate::input::DOCUMENT_LIMIT)
}
pub(crate) fn run_input(command: Command, input: Vec<u8>, limit: usize) -> Result<Vec<u8>, String> {
	run_inner(command, true, true, Some(input), limit)
}
fn run_inner(
	mut command: Command,
	capture: bool,
	bounded_stderr: bool,
	input: Option<Vec<u8>>,
	limit: usize,
) -> Result<Vec<u8>, String> {
	check()?;
	command
		.env_remove("RNX_INTERNAL_DEP_FD")
		.env_remove("RNX_INTERNAL_SESSION_V1");
	command
		.stdin(if input.is_some() {
			Stdio::piped()
		} else {
			Stdio::null()
		})
		.stderr(if bounded_stderr {
			Stdio::piped()
		} else {
			Stdio::inherit()
		});
	if capture {
		command.stdout(Stdio::piped());
	} else {
		command.stdout(std::io::stderr());
	}
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		command.process_group(0);
	}
	let label = format!("{:?}", command.get_program());
	let mut child = command.spawn().map_err(|e| format!("{label}: {e}"))?;
	let writer = input.map(|bytes| {
		let mut stdin = child.stdin.take().unwrap();
		std::thread::spawn(move || {
			use std::io::Write;
			stdin.write_all(&bytes).map_err(|e| e.to_string())
		})
	});
	#[cfg(unix)]
	let pid = child.id();
	let mut reader = child.stdout.take().map(|stdout| {
		std::thread::spawn(move || {
			let mut bytes = vec![];
			stdout
				.take(limit as u64 + 1)
				.read_to_end(&mut bytes)
				.map_err(|e| e.to_string())?;
			if bytes.len() > limit {
				return Err("command output exceeds allowance".into());
			}
			Ok(bytes)
		})
	});
	let mut errors = child.stderr.take().map(|stderr| {
		std::thread::spawn(move || {
			let mut bytes = vec![];
			stderr
				.take(crate::input::DOCUMENT_LIMIT as u64 + 1)
				.read_to_end(&mut bytes)
				.map_err(|e| e.to_string())?;
			if bytes.len() > crate::input::DOCUMENT_LIMIT {
				return Err("command stderr exceeds 16 MiB".into());
			}
			Ok(bytes)
		})
	});
	let mut error_bytes = vec![];
	let kill = || {
		#[cfg(unix)]
		unsafe {
			libc::kill(-(pid as i32), libc::SIGKILL);
		}
	};
	let mut captured = None;
	let result = loop {
		if let Err(error) = check() {
			kill();
			let _ = child.kill();
			let _ = child.wait();
			break Err(error);
		}
		match child.try_wait() {
			Ok(Some(status)) => {
				kill();
				break if status.success() {
					Ok(())
				} else {
					Err(format!("{label} failed: {status}"))
				};
			}
			Err(e) => {
				kill();
				let _ = child.kill();
				let _ = child.wait();
				break Err(e.to_string());
			}
			Ok(None) => {}
		}

		if reader.as_ref().is_some_and(|r| r.is_finished()) {
			let output = reader
				.take()
				.unwrap()
				.join()
				.unwrap_or_else(|_| Err("command output reader panicked".into()));
			match output {
				Ok(bytes) => captured = Some(bytes),
				Err(e) => {
					kill();
					let _ = child.kill();
					let _ = child.wait();
					break Err(e);
				}
			}
		}

		if errors.as_ref().is_some_and(|r| r.is_finished()) {
			match errors
				.take()
				.unwrap()
				.join()
				.unwrap_or_else(|_| Err("command stderr reader panicked".into()))
			{
				Ok(bytes) => error_bytes = bytes,
				Err(e) => {
					kill();
					let _ = child.kill();
					let _ = child.wait();
					break Err(e);
				}
			}
		}
		std::thread::sleep(Duration::from_millis(10));
	};
	let output = match reader {
		Some(r) => r.join().map_err(|_| "command output reader panicked")?,
		None => Ok(captured.unwrap_or_default()),
	};
	if let Some(reader) = errors {
		error_bytes = reader
			.join()
			.map_err(|_| "command stderr reader panicked")??;
	}
	let written = writer
		.map(|w| {
			w.join()
				.map_err(|_| "command input writer panicked".to_owned())
				.and_then(|r| r)
		})
		.transpose();
	result.map_err(|e| {
		if error_bytes.is_empty() {
			e
		} else {
			format!("{e}: {}", String::from_utf8_lossy(&error_bytes))
		}
	})?;
	written?;
	check()?;
	output
}
