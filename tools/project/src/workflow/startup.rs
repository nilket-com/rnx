//! One bounded process, no version-only readiness and no detached output readers.
pub(super) fn flags(text: &str) -> Result<Vec<String>, String> {
	let values: Vec<_> = text.lines().map(str::to_owned).collect();
	let mut color = false;
	let mut splash = false;
	for value in &values {
		match value.as_str() {
			"--no-splash" if !splash => splash = true,
			"--color=auto" | "--color=always" | "--color=never" if !color => color = true,
			_ => return Err("invalid startup presentation flags".into()),
		}
	}
	Ok(values)
}
#[cfg(unix)]
pub(super) fn check(artifact: &crate::artifact::Checked, presentation: &str) -> Result<(), String> {
	use crate::{commands, dep_wire as wire};
	use std::{
		io::Read,
		os::{
			fd::AsRawFd,
			unix::{net::UnixStream, process::CommandExt},
		},
		process::{Command, Stdio},
		time::{Duration, Instant},
	};
	fn nonblock(fd: i32) -> Result<(), String> {
		unsafe {
			let f = libc::fcntl(fd, libc::F_GETFL);
			if f < 0 || libc::fcntl(fd, libc::F_SETFL, f | libc::O_NONBLOCK) < 0 {
				return Err(std::io::Error::last_os_error().to_string());
			}
		}
		Ok(())
	}
	struct Owner(std::process::Child);
	impl Drop for Owner {
		fn drop(&mut self) {
			unsafe {
				libc::kill(-(self.0.id() as i32), libc::SIGKILL);
			}
			let _ = self.0.kill();
			let _ = self.0.wait();
		}
	}
	let deadline = Instant::now() + Duration::from_secs(5);
	artifact.recheck()?;
	let (mut parent, child) = UnixStream::pair().map_err(|e| e.to_string())?;
	parent.set_nonblocking(true).map_err(|e| e.to_string())?;
	let fd = child.as_raw_fd();
	let mut command = Command::new(artifact.path());
	command
		.args(flags(presentation)?)
		.arg("repl")
		.env_remove("RNX_INTERNAL_DEP_FD")
		.env_remove("RNX_INTERNAL_SESSION_V1")
		.env("RNX_INTERNAL_STARTUP_FD", fd.to_string())
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0);
	unsafe {
		command.pre_exec(move || {
			if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
				return Err(std::io::Error::last_os_error());
			}
			Ok(())
		});
	}
	let mut owner = Owner(command.spawn().map_err(|e| format!("startup spawn: {e}"))?);
	drop(child);
	let stdout = owner.0.stdout.take().unwrap();
	let stderr = owner.0.stderr.take().unwrap();
	nonblock(stdout.as_raw_fd())?;
	nonblock(stderr.as_raw_fd())?;
	let mut outputs: Vec<Box<dyn Read>> = vec![Box::new(stdout), Box::new(stderr)];
	let mut tails = [Vec::new(), Vec::new()];
	let mut ended = [false; 2];
	let mut control = Vec::new();
	let mut control_end = false;
	let mut total = 0usize;
	let outcome = (|| {
		loop {
			commands::check()?;
			if Instant::now() >= deadline {
				return Err(
					"startup exceeded five seconds (builders, evaluation and cleanup)".into(),
				);
			}
			let mut buffer = [0; 4096];
			for (index, stream) in outputs.iter_mut().enumerate() {
				for _ in 0..16 {
					match stream.read(&mut buffer) {
						Ok(0) => {
							ended[index] = true;
							break;
						}
						Ok(n) => {
							total += n;
							if total > 8 * 1024 * 1024 {
								return Err("startup output exceeds 8 MiB".into());
							}
							tails[index].extend_from_slice(&buffer[..n]);
							if tails[index].len() > 65536 {
								let discard = tails[index].len() - 65536;
								tails[index].drain(..discard);
							}
						}
						Err(e)
							if matches!(
								e.kind(),
								std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
							) =>
						{
							break;
						}
						Err(e) => return Err(e.to_string()),
					}
				}
			}
			for _ in 0..16 {
				match parent.read(&mut buffer) {
					Ok(0) => {
						control_end = true;
						break;
					}
					Ok(n) => {
						if control.len() + n > wire::LIMIT + 4 {
							return Err("startup control exceeds limit".into());
						}
						control.extend_from_slice(&buffer[..n]);
					}
					Err(e)
						if matches!(
							e.kind(),
							std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
						) =>
					{
						break;
					}
					Err(e) => return Err(e.to_string()),
				}
			}
			if let Some(status) = owner.0.try_wait().map_err(|e| e.to_string())? {
				if !status.success() {
					return Err(format!("startup exited unsuccessfully: {status}"));
				}
				if control_end && ended.iter().all(|v| *v) {
					break;
				}
			}
			std::thread::sleep(Duration::from_millis(5));
		}
		let mut cursor = std::io::Cursor::new(&control);
		let (kind, fields) = wire::read(&mut cursor)
			.map_err(|e| format!("startup readiness missing or malformed: {e}"))?;
		wire::exact(&fields, &[1])?;
		if kind != 6 || fields[&1] != "42" || cursor.position() != control.len() as u64 {
			return Err("startup readiness mismatch or trailing data".into());
		}
		artifact.recheck()?;
		Ok(())
	})();
	// Joining/killing the owned group is part of this call on every path.
	drop(owner);
	if let Err(error) = outcome {
		return Err(format!(
			"{error}\nprobe stdout (tail): {}\nprobe stderr (tail): {}",
			String::from_utf8_lossy(&tails[0][tails[0].len().saturating_sub(8192)..]),
			String::from_utf8_lossy(&tails[1][tails[1].len().saturating_sub(8192)..])
		));
	}
	// Preserve settings warnings without turning the harmless eval into a cell.
	if !tails[1].is_empty() {
		eprintln!("{}", String::from_utf8_lossy(&tails[1]));
	}
	Ok(())
}
