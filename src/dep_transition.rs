//! Preparation runs outside the session VM and never polls or drains its runtime.
use std::cell::RefCell;
thread_local! {static INSTALLED:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};}
pub(crate) fn installed(mut names: Vec<String>) {
	names.sort();
	INSTALLED.with(|v| *v.borrow_mut() = names);
}
#[cfg(not(unix))]
pub(crate) fn prepare(_: &str, _: impl FnOnce() -> bool) -> Result<(), String> {
	Err("dependency transitions are not supported on this platform".into())
}
#[cfg(unix)]
pub(crate) fn prepare(input: &str, consent: impl FnOnce() -> bool) -> Result<(), String> {
	unix::prepare(input, consent)
}
#[cfg(unix)]
mod unix {
	use super::*;
	use crate::dep_wire as wire;
	use std::{
		io::{IsTerminal, Read, Write},
		os::{
			fd::AsRawFd,
			unix::{net::UnixStream, process::CommandExt},
		},
		path::PathBuf,
		process::{Child, Command, Stdio},
		time::{Duration, Instant},
	};
	fn err(e: impl std::fmt::Display) -> String {
		e.to_string()
	}
	fn nonblock(fd: i32) -> Result<(), String> {
		unsafe {
			let flags = libc::fcntl(fd, libc::F_GETFL);
			if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
				return Err(err(std::io::Error::last_os_error()));
			}
		}
		Ok(())
	}
	struct Helper {
		child: Child,
		socket: UnixStream,
		output: Vec<Box<dyn Read>>,
		frame: Vec<u8>,
		eof: bool,
		total: usize,
		deadline: Instant,
		done: bool,
	}
	impl Helper {
		fn spawn(tool: PathBuf) -> Result<Self, String> {
			let (parent, child) = UnixStream::pair().map_err(err)?;
			let fd = child.as_raw_fd();
			let mut command = Command::new(tool);
			command
				.env("RNX_INTERNAL_DEP_FD", fd.to_string())
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
			let spawned = command.spawn().map_err(err)?;
			drop(child);
			let mut h = Self {
				child: spawned,
				socket: parent,
				output: Vec::new(),
				frame: Vec::new(),
				eof: false,
				total: 0,
				deadline: Instant::now() + Duration::from_secs(5),
				done: false,
			};
			h.socket.set_nonblocking(true).map_err(err)?;
			let out = h.child.stdout.take().unwrap();
			let errors = h.child.stderr.take().unwrap();
			nonblock(out.as_raw_fd())?;
			nonblock(errors.as_raw_fd())?;
			h.output.push(Box::new(out));
			h.output.push(Box::new(errors));
			Ok(h)
		}
		fn check(&self) -> Result<(), String> {
			if crate::host::interrupted() {
				return Err("interrupted; old session unchanged".into());
			}
			if Instant::now() >= self.deadline {
				return Err("dependency tool response deadline exceeded".into());
			}
			Ok(())
		}
		fn output(&mut self) -> Result<(), String> {
			let mut b = [0; 4096];
			for stream in &mut self.output {
				// Bound work per stream so a writer cannot starve cancellation.
				for _ in 0..16 {
					match stream.read(&mut b) {
						Ok(0) => break,
						Ok(n) => {
							self.total += n;
							if self.total > 8 * 1024 * 1024 {
								return Err("preparation output exceeds 8 MiB".into());
							}
							eprint!(
								"{}",
								crate::format::terminal_safe(&String::from_utf8_lossy(&b[..n]))
							);
						}
						Err(e)
							if matches!(
								e.kind(),
								std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
							) =>
						{
							break;
						}
						Err(e) => return Err(err(e)),
					}
				}
			}
			Ok(())
		}
		fn send(&mut self, kind: u8, f: &wire::Fields) -> Result<(), String> {
			let bytes = wire::encode(kind, f)?;
			let mut at = 0;
			while at < bytes.len() {
				self.check()?;
				self.output()?;
				match self.socket.write(&bytes[at..]) {
					Ok(0) => return Err("tool closed control channel".into()),
					Ok(n) => at += n,
					Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
						std::thread::sleep(Duration::from_millis(10))
					}
					Err(e) => return Err(err(e)),
				}
			}
			Ok(())
		}
		fn receive(&mut self) -> Result<(u8, wire::Fields), String> {
			loop {
				self.check()?;
				self.output()?;
				if self.frame.len() >= 4 {
					let size = u32::from_be_bytes(self.frame[..4].try_into().unwrap()) as usize;
					if !(2..=wire::LIMIT).contains(&size) {
						return Err("tool frame length refused".into());
					}
					if self.frame.len() >= size + 4 {
						let result = wire::decode(&self.frame[4..size + 4])?;
						self.frame.drain(..size + 4);
						return Ok(result);
					}
				}
				let mut b = [0; 4096];
				match self.socket.read(&mut b) {
					Ok(0) => return Err("tool closed or truncated transition protocol".into()),
					Ok(n) => {
						if self.frame.len() + n > wire::LIMIT + 4 {
							return Err("tool control buffer exceeded".into());
						}
						self.frame.extend_from_slice(&b[..n]);
					}
					Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
						std::thread::sleep(Duration::from_millis(10))
					}
					Err(e) => return Err(err(e)),
				}
			}
		}
		fn finish(&mut self) -> Result<(), String> {
			if !self.frame.is_empty() {
				return Err("trailing tool control data".into());
			}
			loop {
				self.check()?;
				self.output()?;
				let mut b = [0];
				match self.socket.read(&mut b) {
					Ok(0) => self.eof = true,
					Ok(_) => return Err("trailing tool control data".into()),
					Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
					Err(e) => return Err(err(e)),
				}
				if let Some(status) = self.child.try_wait().map_err(err)? {
					self.done = true;
					self.output()?;
					if !self.eof {
						continue;
					}
					return if status.success() {
						Ok(())
					} else {
						Err(format!("preparation tool failed: {status}"))
					};
				}
				std::thread::sleep(Duration::from_millis(10));
			}
		}
	}
	impl Drop for Helper {
		fn drop(&mut self) {
			if !self.done {
				// Give the tool its signal-handler path so it reaps the Cargo group.
				unsafe {
					libc::kill(self.child.id() as i32, libc::SIGTERM);
				}
				let until = Instant::now() + Duration::from_secs(5);
				while Instant::now() < until {
					let _ = self.output();
					if self.child.try_wait().ok().flatten().is_some() {
						self.done = true;
						break;
					}
					std::thread::sleep(Duration::from_millis(10));
				}
				if !self.done {
					unsafe {
						libc::kill(-(self.child.id() as i32), libc::SIGKILL);
					}
					let _ = self.child.kill();
					let _ = self.child.wait();
				}
			}
		}
	}
	pub(super) fn prepare(input: &str, consent: impl FnOnce() -> bool) -> Result<(), String> {
		if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
			return Err(":dep requires a terminal; use rnx-project add, lock, build and session --manifest FILE".into());
		}
		let association = std::env::var("RNX_INTERNAL_SESSION_V1").unwrap_or_default();
		let installed = INSTALLED.with(|v| v.borrow().join("\n"));
		let tool = if !association.is_empty() {
			PathBuf::from(&wire::capsule(&association)?[&2])
		} else if let Some(path) = std::env::var_os("RNX_PROJECT_TOOL") {
			PathBuf::from(path)
		} else {
			std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
				.map(|p| p.join("rnx-project"))
				.find(|p| p.is_file())
				.ok_or("cannot locate rnx-project; set RNX_PROJECT_TOOL to its absolute path")?
				.canonicalize()
				.map_err(err)?
		};
		if !tool.is_absolute() {
			return Err("RNX_PROJECT_TOOL must be absolute".into());
		}
		let mut offline = false;
		let mut names = Vec::new();
		for word in input.split_whitespace().skip(1) {
			if word == "--offline" && !offline {
				offline = true
			} else if word.starts_with('-') {
				return Err("unknown or duplicate :dep option".into());
			} else {
				names.push(word)
			}
		}
		crate::host::clear_interrupt();
		let mut helper = Helper::spawn(tool)?;
		helper.send(
			1,
			&wire::Fields::from([
				(1, names.join("\n")),
				(2, association),
				(
					3,
					std::env::current_exe()
						.map_err(err)?
						.to_str()
						.ok_or("executable path is not Unicode")?
						.into(),
				),
				(4, installed),
				(5, if offline { "offline" } else { "online" }.into()),
			]),
		)?;
		let (kind, description) = helper.receive()?;
		if kind == 8 {
			wire::exact(&description, &[1])?;
			helper.finish()?;
			return Err(description[&1].clone());
		}
		if kind != 2 {
			return Err("incompatible tool transition protocol".into());
		}
		wire::exact(&description, &[1, 2, 3, 4, 5, 6, 7])?;
		helper.deadline = Instant::now() + Duration::from_secs(1800);
		if description[&5].is_empty() {
			helper.send(3, &wire::Fields::new())?;
			helper
				.socket
				.shutdown(std::net::Shutdown::Write)
				.map_err(err)?;
			helper.finish()?;
			println!("already installed; session unchanged");
			return Ok(());
		}
		println!("{}", crate::format::terminal_safe(&description[&2]));
		if description[&5].is_empty() || !consent() {
			helper.send(3, &wire::Fields::new())?;
			helper
				.socket
				.shutdown(std::net::Shutdown::Write)
				.map_err(err)?;
			helper.finish()?;
			return Ok(());
		}
		helper.send(4, &wire::Fields::from([(1, description[&1].clone())]))?;
		helper
			.socket
			.shutdown(std::net::Shutdown::Write)
			.map_err(err)?;
		let (kind, result) = helper.receive()?;
		if kind != 8 {
			return Err("unprobed replacement refused; startup gate not enabled".into());
		}
		wire::exact(&result, &[1])?;
		helper.finish()?;
		Err(result[&1].clone())
	}
}
