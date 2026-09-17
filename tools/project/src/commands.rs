//! Bounded Cargo output and Unix process-group ownership for tool subprocesses.
use std::{
	io::Read,
	process::{Command, Stdio},
	sync::atomic::{AtomicI32, Ordering},
	time::Duration,
};
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
	if interrupted() {
		Err("interrupted".into())
	} else {
		Ok(())
	}
}
pub(crate) fn run(mut command: Command, capture: bool) -> Result<Vec<u8>, String> {
	check()?;
	command.stdin(Stdio::null()).stderr(Stdio::inherit());
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
	#[cfg(unix)]
	let pid = child.id();
	let mut reader = child.stdout.take().map(|stdout| {
		std::thread::spawn(move || {
			let mut bytes = vec![];
			stdout
				.take(crate::input::DOCUMENT_LIMIT as u64 + 1)
				.read_to_end(&mut bytes)
				.map_err(|e| e.to_string())?;
			if bytes.len() > crate::input::DOCUMENT_LIMIT {
				return Err("command output exceeds 16 MiB".into());
			}
			Ok(bytes)
		})
	});
	let kill = || {
		#[cfg(unix)]
		unsafe {
			libc::kill(-(pid as i32), libc::SIGKILL);
		}
	};
	let mut captured = None;
	let result = loop {
		if interrupted() {
			kill();
			let _ = child.kill();
			let _ = child.wait();
			break Err("interrupted".into());
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

		std::thread::sleep(Duration::from_millis(10));
	};
	let output = match reader {
		Some(r) => r.join().map_err(|_| "command output reader panicked")?,
		None => Ok(captured.unwrap_or_default()),
	};
	result?;
	check()?;
	output
}
