//! A headless Windows console. ConPTY gives the child real console handles
//! while the fixture sends keystrokes and reads its rendered output through
//! pipes. All observation waits are bounded; Drop ends the child on failure.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{FALSE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON};
use windows_sys::Win32::System::Threading::*;

const LIMIT: Duration = Duration::from_secs(15);

struct PseudoConsole(HPCON);
impl Drop for PseudoConsole {
	fn drop(&mut self) {
		unsafe { ClosePseudoConsole(self.0) };
	}
}

pub struct Console {
	process: OwnedHandle,
	console: Option<PseudoConsole>,
	reader: Option<std::thread::JoinHandle<()>>,
	input: std::io::PipeWriter,
	output: Receiver<Result<Vec<u8>, String>>,
	pub seen: String,
}

impl Console {
	pub fn spawn(args: &[&str], env: &[(&str, &Path)]) -> Self {
		let (input_read, input) = std::io::pipe().unwrap();
		let (mut output_read, output_write) = std::io::pipe().unwrap();
		let mut console = 0;
		// Pipe ends stay owned through creation; ConPTY retains its own references.
		let result = unsafe {
			CreatePseudoConsole(
				COORD { X: 120, Y: 40 },
				input_read.as_raw_handle(),
				output_write.as_raw_handle(),
				0,
				&mut console,
			)
		};
		assert_eq!(result, 0, "CreatePseudoConsole: {result:#x}");
		let console = PseudoConsole(console);
		let (tx, output) = mpsc::channel();
		// Drain continuously, including during ClosePseudoConsole. Keeping this
		// reader active prevents console teardown from blocking on its output.
		let reader = std::thread::spawn(move || {
			let mut bytes = [0; 4096];
			loop {
				match output_read.read(&mut bytes) {
					Ok(0) => break,
					Ok(n) => {
						let _ = tx.send(Ok(bytes[..n].to_vec()));
					}
					Err(e) => {
						let _ = tx.send(Err(e.to_string()));
						break;
					}
				}
			}
		});
		let mut size = 0;
		unsafe {
			InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
		}
		let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
		let attributes = storage.as_mut_ptr().cast();
		let mut info = STARTUPINFOEXW::default();
		info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
		// Explicit null handles select the new console even when Cargo's own
		// standard handles are redirected (Microsoft Terminal discussion 15814).
		info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
		info.lpAttributeList = attributes;
		let mut process = PROCESS_INFORMATION::default();
		// These fixture arguments contain neither quotes nor trailing slashes.
		// Quote each to preserve spaces; reject inputs outside that contract.
		let executable = env!("CARGO_BIN_EXE_rnx");
		let mut command = std::iter::once(executable)
			.chain(args.iter().copied())
			.map(|arg| {
				assert!(!arg.contains('"') && !arg.ends_with('\\'));
				format!("\"{arg}\"")
			})
			.collect::<Vec<_>>()
			.join(" ")
			.encode_utf16()
			.chain(Some(0))
			.collect::<Vec<_>>();
		let mut environment: std::collections::BTreeMap<String, std::ffi::OsString> =
			std::env::vars_os()
				.map(|(k, v)| (k.to_string_lossy().to_uppercase(), v))
				.collect();
		for key in ["RNX_HISTORY", "LOCALAPPDATA", "XDG_STATE_HOME", "HOME"] {
			environment.remove(key);
		}
		environment.insert("TERM".into(), "xterm-256color".into());
		for (key, value) in env {
			environment.insert(key.to_string(), value.as_os_str().to_owned());
		}
		use std::os::windows::ffi::OsStrExt;
		let mut block = Vec::<u16>::new();
		for (key, value) in environment {
			block.extend(key.encode_utf16());
			block.push('=' as u16);
			block.extend(value.encode_wide());
			block.push(0);
		}
		block.push(0);
		let started = unsafe {
			assert_ne!(
				InitializeProcThreadAttributeList(attributes, 1, 0, &mut size),
				FALSE
			);
			assert_ne!(
				UpdateProcThreadAttribute(
					attributes,
					0,
					PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
					console.0 as *const _,
					size_of::<HPCON>(),
					std::ptr::null_mut(),
					std::ptr::null()
				),
				FALSE
			);
			let ok = CreateProcessW(
				std::ptr::null(),
				command.as_mut_ptr(),
				std::ptr::null(),
				std::ptr::null(),
				FALSE,
				EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
				block.as_ptr().cast(),
				std::ptr::null(),
				&info.StartupInfo,
				&mut process,
			);
			let error = std::io::Error::last_os_error();
			DeleteProcThreadAttributeList(attributes);
			if ok == FALSE {
				panic!("CreateProcessW: {error}");
			}
			OwnedHandle::from_raw_handle(process.hProcess)
		};
		drop(unsafe { OwnedHandle::from_raw_handle(process.hThread) });
		drop(input_read);
		drop(output_write);
		Self {
			process: started,
			console: Some(console),
			reader: Some(reader),
			input,
			output,
			seen: String::new(),
		}
	}

	pub fn send(&mut self, text: &str) {
		self.input.write_all(text.as_bytes()).unwrap();
	}

	pub fn expect(&mut self, text: &str) {
		let deadline = Instant::now() + LIMIT;
		while !self.seen.contains(text) {
			let remaining = deadline.saturating_duration_since(Instant::now());
			let bytes = self
				.output
				.recv_timeout(remaining)
				.unwrap_or_else(|e| panic!("waiting for {text:?}: {e}; console: {:?}", self.seen))
				.unwrap();
			self.seen.push_str(&String::from_utf8_lossy(&bytes));
		}
	}

	pub fn finish(&mut self) -> u32 {
		assert_eq!(
			unsafe { WaitForSingleObject(self.process.as_raw_handle(), LIMIT.as_millis() as u32) },
			WAIT_OBJECT_0,
			"console child did not exit: {:?}",
			self.seen
		);
		let mut code = 0;
		assert_ne!(
			unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) },
			FALSE
		);
		code
	}
}

impl Drop for Console {
	fn drop(&mut self) {
		let ended = unsafe {
			TerminateProcess(self.process.as_raw_handle(), 1);
			WaitForSingleObject(self.process.as_raw_handle(), LIMIT.as_millis() as u32)
		};
		drop(self.console.take());
		let reader = self.reader.take().unwrap();
		let deadline = Instant::now() + LIMIT;
		while !reader.is_finished() && Instant::now() < deadline {
			std::thread::sleep(Duration::from_millis(5));
		}
		let collected = reader.is_finished() && reader.join().is_ok();
		if ended != WAIT_OBJECT_0 || !collected {
			if std::thread::panicking() {
				eprintln!(
					"console cleanup failed: process wait={ended}, reader collected={collected}"
				);
			} else {
				panic!(
					"console cleanup failed: process wait={ended}, reader collected={collected}"
				);
			}
		}
	}
}
