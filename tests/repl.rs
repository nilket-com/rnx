//! The interactive session, exercised through a pseudo-terminal (Linux).
//! Output is compared after stripping terminal control sequences: each
//! expected fragment must appear, in order, in what the terminal received.
#![cfg(target_os = "linux")]
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Terminal {
	master: std::fs::File,
	child: Child,
	seen: Vec<u8>,
	cursor: usize,
}
impl Terminal {
	fn spawn(history: &std::path::Path) -> Self {
		let mut master = 0;
		let mut slave = 0;
		let mut size = libc::winsize {
			ws_row: 40,
			ws_col: 120,
			ws_xpixel: 0,
			ws_ypixel: 0,
		};
		let rc = unsafe {
			libc::openpty(
				&mut master,
				&mut slave,
				std::ptr::null_mut(),
				std::ptr::null(),
				&mut size,
			)
		};
		assert_eq!(rc, 0, "openpty");
		let slave_fd = unsafe { OwnedFd::from_raw_fd(slave) };
		let stdin = Stdio::from(slave_fd.try_clone().unwrap());
		let stdout = Stdio::from(slave_fd.try_clone().unwrap());
		let stderr = Stdio::from(slave_fd);
		let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
		command
			.arg("repl")
			.env("RNX_HISTORY", history)
			.env("TERM", "xterm")
			.stdin(stdin)
			.stdout(stdout)
			.stderr(stderr);
		unsafe {
			command.pre_exec(move || {
				libc::setsid();
				if libc::ioctl(0, libc::TIOCSCTTY as _, 0) != 0 {
					return Err(std::io::Error::last_os_error());
				}
				Ok(())
			});
		}
		let child = command.spawn().expect("spawn rnx repl");
		let master = unsafe { std::fs::File::from_raw_fd(master) };
		let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
		unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) };
		Self {
			master,
			child,
			seen: Vec::new(),
			cursor: 0,
		}
	}
	fn send(&mut self, text: &str) {
		self.master.write_all(text.as_bytes()).unwrap();
		self.master.flush().unwrap();
	}
	fn pump(&mut self) {
		let mut buf = [0u8; 4096];
		loop {
			match self.master.read(&mut buf) {
				Ok(0) => break,
				Ok(n) => self.seen.extend_from_slice(&buf[..n]),
				Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
				Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
				Err(e) => panic!("read: {e}"),
			}
		}
	}
	/// Wait until `fragment` appears after everything already matched.
	fn expect(&mut self, fragment: &str) {
		let deadline = Instant::now() + Duration::from_secs(10);
		loop {
			self.pump();
			let text = clean(&self.seen[self.cursor..]);
			if let Some(at) = text.find(fragment) {
				let matched = text[..at + fragment.len()].chars().count();
				self.cursor += raw_offset(&self.seen[self.cursor..], matched);
				return;
			}
			assert!(
				Instant::now() < deadline,
				"timed out waiting for {fragment:?}; terminal so far:\n{}",
				clean(&self.seen)
			);
			std::thread::sleep(Duration::from_millis(10));
		}
	}
	/// Everything received since the last match, cleaned.
	fn pending(&mut self) -> String {
		self.pump();
		clean(&self.seen[self.cursor..])
	}
	/// Wait for an idle prompt: the editor redraws the prompt with the buffer
	/// on every keystroke, so a prompt counts only when nothing follows it.
	fn prompt(&mut self) {
		let deadline = Instant::now() + Duration::from_secs(10);
		loop {
			let text = self.pending();
			if text.ends_with("rnx> ") {
				self.cursor = self.seen.len();
				return;
			}
			assert!(
				Instant::now() < deadline,
				"no idle prompt; terminal so far:\n{}",
				clean(&self.seen)
			);
			std::thread::sleep(Duration::from_millis(10));
		}
	}
	fn wait_exit(mut self) {
		let deadline = Instant::now() + Duration::from_secs(10);
		while self.child.try_wait().unwrap().is_none() {
			assert!(Instant::now() < deadline, "rnx did not exit");
			std::thread::sleep(Duration::from_millis(10));
		}
	}
}
impl Drop for Terminal {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}
/// Strip CSI and OSC control sequences and carriage returns.
fn clean(raw: &[u8]) -> String {
	let text = String::from_utf8_lossy(raw);
	let mut out = String::new();
	let mut chars = text.chars().peekable();
	while let Some(c) = chars.next() {
		match c {
			'\x1b' => match chars.next() {
				Some('[') => {
					for c in chars.by_ref() {
						if ('\x40'..='\x7e').contains(&c) {
							break;
						}
					}
				}
				Some(']') => {
					for c in chars.by_ref() {
						if c == '\x07' {
							break;
						}
					}
				}
				_ => {}
			},
			'\r' => {}
			_ => out.push(c),
		}
	}
	out
}
/// How many raw bytes produce the first `clean_len` cleaned characters.
fn raw_offset(raw: &[u8], clean_len: usize) -> usize {
	let mut end = 0;
	while end < raw.len() && clean(&raw[..end]).chars().count() < clean_len {
		end += 1;
	}
	end
}
fn history_file(tag: &str) -> std::path::PathBuf {
	let dir = std::env::temp_dir().join(format!("rnx-repl-{}-{}", std::process::id(), tag));
	let _ = std::fs::remove_dir_all(&dir);
	let _ = std::fs::create_dir_all(&dir);
	dir.join("history")
}

#[test]
fn gate_1_the_ordinary_session() {
	let history = history_file("ordinary");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send("let v = [1, 2, 3]; struct P { x, y } let p = P { x: 1, y: v };\r");
	t.prompt();
	// A three-line function through the continuation prompt: Enter on an
	// incomplete input inserts a newline and keeps waiting.
	t.send("fn twice(n) {\r");
	t.send("  n * 2\r");
	t.send("}\r");
	t.prompt();
	t.send("twice(4)\r");
	t.expect("8\n");
	t.prompt();
	// Recall the function (two entries back), change its body, re-enter it.
	t.send("\x1b[A\x1b[A");
	t.expect("n * 2");
	t.send("\x7f\x7f\x7f\x7f\x7f\x7f");
	t.send(" * 3\r}\r");
	t.prompt();
	t.send("(twice(4), p)\r");
	t.expect("(12, P {x: 1, y: [1, 2, 3]})\n");
	t.prompt();
	// Each incomplete form named in the record shows the continuation prompt;
	// each complete-but-invalid control prints its diagnostic at once.
	for (open, close) in [
		("let xs = [\r", "1];\r"),
		("let x =\r", "1;\r"),
		("let s = \"unterminated\r", "rest\";\r"),
		("let c = 1; /* open\r", "closed */\r"),
		("let t = (1,\r", "2);\r"),
	] {
		t.send(open);
		std::thread::sleep(Duration::from_millis(200));
		let text = t.pending();
		assert!(
			!text.ends_with("rnx> ") && !text.contains("error"),
			"{open:?} was not continued: {text:?}"
		);
		t.send(close);
		t.prompt();
	}
	t.send("let q = ;\r");
	t.expect("error at input");
	t.expect("line 1, column 9");
	t.expect("        ^");
	t.prompt();
	t.send("twice(2))\r");
	t.expect("error at input");
	t.prompt();
	// An infinite loop, stopped with Ctrl-C, then the same session continues.
	t.send("while true {}\r");
	std::thread::sleep(Duration::from_millis(200));
	t.send("\x03");
	t.expect("interrupted");
	t.prompt();
	t.send("twice(5)\r");
	t.expect("15\n");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
	let saved = std::fs::read_to_string(&history).unwrap();
	assert!(saved.contains("fn twice(n) {"), "{saved}");
}

#[test]
fn gate_2_history_survives_and_state_does_not() {
	let history = history_file("history");
	let effect = history.parent().unwrap().join("effect");
	let quoted = serde_json::to_string(&effect.to_string_lossy()).unwrap();
	{
		let mut t = Terminal::spawn(&history);
		t.prompt();
		t.send("fn twice(n) { n * 2 }\r");
		t.prompt();
		t.send(&format!("host::write_new({quoted}, \"ran\")\r"));
		t.prompt();
		t.send(":quit\r");
		t.wait_exit();
	}
	assert_eq!(std::fs::read_to_string(&effect).unwrap(), "ran");
	std::fs::remove_file(&effect).unwrap();
	let mut t = Terminal::spawn(&history);
	t.prompt();
	// Nothing ran on launch: the side-effecting entry did not recreate the file.
	assert!(!effect.exists(), "history restore executed an input");
	// The function is not defined; the input is recallable.
	t.send("twice(1)\r");
	t.expect("error at input");
	t.prompt();
	t.send("\x1b[A\x1b[A\x1b[A\x1b[A");
	t.expect("fn twice(n) { n * 2 }");
	t.send("\r");
	t.prompt();
	t.send("twice(1)\r");
	t.expect("2\n");
	assert!(!effect.exists());
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_5_a_waited_child_is_cancelled_and_the_next_input_runs() {
	let history = history_file("child");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send("host::process(\"/bin/sh\", [\"-c\", \"sleep 30\"], 60000)\r");
	std::thread::sleep(Duration::from_millis(300));
	let start = Instant::now();
	t.send("\x03");
	t.expect("interrupted");
	assert!(start.elapsed() < Duration::from_secs(2));
	t.prompt();
	t.send("1 + 1\r");
	t.expect("2\n");
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_6_ctrl_c_clears_an_open_input() {
	let history = history_file("clear");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send("fn open() {\r");
	t.send("  1\r");
	std::thread::sleep(Duration::from_millis(150));
	t.send("\x03");
	t.prompt();
	std::thread::sleep(Duration::from_millis(100));
	t.send("1 + 1\r");
	t.expect("2\n");
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_5_interrupt_semantics_in_a_process_of_their_own() {
	// The interrupt flag is process-global, so its semantics are proved here,
	// where each session is its own process, not in the unit tests.
	let history = history_file("interrupt");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send("let shared = [1];\r");
	t.prompt();
	// A Ctrl-C at the prompt does not cancel the next input.
	t.send("\x03");
	t.prompt();
	t.send("41 + 1\r");
	t.expect("42\n");
	t.prompt();
	// During evaluation: the shared push before the loop survives, the `let`
	// in the same input is not published, and the prompt returns.
	t.send("shared.push(2); let unpublished = 5; while true {}\r");
	std::thread::sleep(Duration::from_millis(300));
	let start = Instant::now();
	t.send("\x03");
	t.expect("interrupted");
	assert!(start.elapsed() < Duration::from_millis(500));
	t.prompt();
	t.send("unpublished\r");
	t.expect("error at input");
	t.prompt();
	t.send("shared\r");
	t.expect("[1, 2]\n");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
}
