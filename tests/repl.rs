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
		Self::spawn_with(history, &[])
	}
	/// Spawn with extra environment, for the accounting gates, which need a
	/// ceiling of their own and a process of their own.
	fn spawn_with(history: &std::path::Path, environment: &[(&str, &str)]) -> Self {
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
		for (name, value) in environment {
			command.env(name, value);
		}
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

// Completion (record 0003). List mode: the first Tab inserts the common
// prefix of the candidates, a second Tab lists them.
#[test]
fn gate_0003_completion_in_the_terminal() {
	let history = history_file("completion");
	let effect = history.parent().unwrap().join("effect");
	let quoted = serde_json::to_string(&effect.to_string_lossy()).unwrap();
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send(&format!(
		"let alpha = 1; let alphabet = 2; let boom = || host::write_new({quoted}, \"ran\");\r"
	));
	t.prompt();
	// Ambiguous prefix: common prefix, then the list; then a unique one.
	t.send("alp\t");
	std::thread::sleep(Duration::from_millis(200));
	t.send("\t");
	t.expect("alphabet");
	t.send("b\t");
	std::thread::sleep(Duration::from_millis(200));
	t.send("\r");
	t.expect("2\n");
	t.prompt();
	// A declaration, a qualified host path, the host module listing, a command.
	t.send("fn total(xs) { xs.len() }\r");
	t.prompt();
	t.send("to\t([1, 2])\r");
	t.expect("2\n");
	t.prompt();
	t.send("host::wr\t");
	std::thread::sleep(Duration::from_millis(200));
	assert!(t.pending().contains("host::write_new"), "{}", t.pending());
	t.send("\x03");
	t.prompt();
	t.send("host::\t\t");
	t.expect("host::json_parse");
	t.expect("host::process");
	t.send("\x03");
	t.prompt();
	t.send(":me\t\r");
	t.expect("source and map storage");
	t.prompt();
	// The standard library is not completed; the buffer is unchanged.
	t.send("std::\t\t");
	std::thread::sleep(Duration::from_millis(200));
	assert!(!t.pending().contains("String"), "{}", t.pending());
	t.send("\x03");
	t.prompt();
	// Mid-token: the whole token is replaced, the rest of the line kept.
	t.send("alpx + 40");
	t.send("\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D");
	t.send("\t\r");
	t.expect("41\n");
	t.prompt();
	// No execution: completing the side-effecting closure ran nothing.
	t.send("bo\t");
	std::thread::sleep(Duration::from_millis(200));
	assert!(t.pending().contains("boom"), "{}", t.pending());
	t.send("\x03");
	t.prompt();
	assert!(!effect.exists(), "completion executed a closure");
	// After a failed input the name is absent; after reset only host and
	// commands remain.
	t.send("let gone = 1; panic!(\"x\");\r");
	t.expect("runtime error");
	t.prompt();
	t.send("go\t\t");
	std::thread::sleep(Duration::from_millis(200));
	assert!(!t.pending().contains("gone"), "{}", t.pending());
	t.send("\x03");
	t.prompt();
	t.send(":reset\r");
	t.expect("session reset");
	t.prompt();
	t.send("alp\t\t");
	std::thread::sleep(Duration::from_millis(200));
	assert!(!t.pending().contains("alphabet"), "{}", t.pending());
	t.send("\x03");
	t.prompt();
	t.send("hos\t");
	std::thread::sleep(Duration::from_millis(200));
	assert!(t.pending().contains("host::"), "{}", t.pending());
	t.send("\x03");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
}

// :vars and :help (record 0004) at the terminal.
#[test]
fn gate_0004_vars_and_help_in_the_terminal() {
	let history = history_file("inspect");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send("let count = 7; let label = \"tag\"; struct P { x } let point = P { x: 1 };\r");
	t.prompt();
	t.send("fn twice(n) {\r");
	t.send("  n * 2\r");
	t.send("}\r");
	t.prompt();
	// :vars lists bindings in name order with type and value.
	t.send(":vars\r");
	t.expect("count: i64 = 7");
	t.expect("label: String = \"tag\"");
	t.expect("point: P = P {x: 1}");
	t.prompt();
	// :help on each kind.
	t.send(":help count\r");
	t.expect("count: binding");
	t.expect("type: i64");
	t.prompt();
	t.send(":help twice\r");
	t.expect("twice: function");
	t.expect("n * 2");
	t.prompt();
	t.send(":help host::write_new\r");
	t.expect("host function");
	t.expect("refusing to overwrite");
	t.prompt();
	t.send(":help :vars\r");
	t.expect("session command");
	t.prompt();
	t.send(":help nosuchname\r");
	t.expect("no binding, declaration, host function, or command");
	t.prompt();
	t.send(":help\r");
	t.expect("session commands:");
	t.expect(":vars");
	t.prompt();
	// The command names complete, and a name after :help completes too.
	t.send(":va\t\r");
	t.expect("count: i64 = 7");
	t.prompt();
	t.send(":help cou\t\r");
	t.expect("count: binding");
	t.prompt();
	// After a failed input the surviving mutation shows and the new name does not.
	t.send("let shared = [1];\r");
	t.prompt();
	t.send("shared.push(2); let fresh = 9; panic!(\"stop\");\r");
	t.expect("runtime error");
	t.prompt();
	t.send(":vars\r");
	t.expect("shared: Vec = [1, 2]");
	t.prompt();
	t.send(":help fresh\r");
	t.expect("no binding, declaration");
	t.prompt();
	// After :reset there are no bindings, but commands and host help remain.
	t.send(":reset\r");
	t.expect("session reset");
	t.prompt();
	t.send(":vars\r");
	t.expect("no bindings");
	t.prompt();
	t.send(":help host::read\r");
	t.expect("host function");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
}

// The allocation ceiling (record 0005). Each of these runs in its own
// process, because the counter is process-global.

/// Run `:memory` and read the figure it reports.
fn live_bytes(t: &mut Terminal) -> usize {
	t.send(":memory\r");
	t.expect("tracked live allocation request bytes: ");
	let deadline = Instant::now() + Duration::from_secs(10);
	loop {
		let text = t.pending();
		let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
		if !digits.is_empty() && text.len() > digits.len() {
			t.prompt();
			return digits.parse().expect("a figure");
		}
		assert!(Instant::now() < deadline, "no figure in {text:?}");
		std::thread::sleep(Duration::from_millis(10));
	}
}
/// Read the source and map component of the same report.
fn source_bytes(t: &mut Terminal) -> usize {
	t.send(":memory\r");
	t.expect("source and map storage: ");
	let deadline = Instant::now() + Duration::from_secs(10);
	loop {
		let text = t.pending();
		let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
		if !digits.is_empty() && text.len() > digits.len() {
			t.prompt();
			return digits.parse().expect("a figure");
		}
		assert!(Instant::now() < deadline, "no figure in {text:?}");
		std::thread::sleep(Duration::from_millis(10));
	}
}

#[test]
fn gate_0005_the_ceiling_latches_and_a_reset_that_cannot_recover_says_so() {
	// A ceiling of one byte: every sample is at or above it, so the behaviour
	// under the ceiling is deterministic and owes nothing to what the process
	// happens to have allocated.
	let history = history_file("ceiling");
	let mut t = Terminal::spawn_with(&history, &[("RNX_MEMORY_CEILING", "1")]);
	t.prompt();
	// The startup sample was taken before the first evaluation was admitted.
	t.send("1 + 1\r");
	t.expect("at or above the ceiling");
	t.expect(":reset to continue");
	t.prompt();
	// Inspection still answers, which is the point of a host-side command.
	t.send(":vars\r");
	t.expect("no bindings");
	t.prompt();
	t.send(":help :memory\r");
	t.expect("session command");
	t.prompt();
	t.send(":memory\r");
	t.expect("evaluation is refused until :reset");
	t.prompt();
	// A reset cannot bring a one-byte ceiling back under, and says so.
	t.send(":reset\r");
	t.expect("session reset");
	t.expect("restarting rnx may be necessary");
	t.prompt();
	// The refusal is latched again by the new session's own sample.
	t.send("1 + 1\r");
	t.expect("at or above the ceiling");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_0005_memory_names_what_the_figure_is_and_is_not() {
	let history = history_file("report");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	t.send(":memory\r");
	t.expect("tracked live allocation request bytes:");
	t.expect("startup reference point:");
	t.expect("net change since:");
	t.expect("source and map storage:");
	t.expect("Rust's global allocator only");
	t.expect("not resident memory");
	t.expect("not the session's share");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_0005_the_figure_moves_with_the_payload_against_a_control() {
	// Two arms in one session: the same input shape without the payload, then
	// with it. An arbitrary before and after would prove nothing, since
	// unrelated allocations can be freed in between.
	let history = history_file("payload");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	let start = live_bytes(&mut t);
	let source_start = source_bytes(&mut t);
	// Control arm: the same shape, no payload.
	t.send("let control = String::new(); for i in 0..1 { control.push_str(\"x\") }\r");
	t.prompt();
	let after_control = live_bytes(&mut t);
	// Payload arm: eight megabytes of string.
	t.send(
		"let payload = String::new(); for i in 0..524288 { payload.push_str(\"0123456789abcdef\") }\r",
	);
	t.prompt();
	let after_payload = live_bytes(&mut t);
	let source_end = source_bytes(&mut t);
	let payload = 8 * 1024 * 1024;
	let control_delta = after_control.saturating_sub(start);
	let payload_delta = after_payload.saturating_sub(after_control);
	assert!(
		payload_delta >= payload,
		"payload arm moved {payload_delta}, control arm {control_delta}"
	);
	assert!(
		control_delta < payload / 8,
		"control arm moved {control_delta}, which is not small against {payload}"
	);
	// The inputs and their source maps are retained too, so they grow; the
	// claim is that their growth is small relative to the payload, not zero.
	let source_growth = source_end.saturating_sub(source_start);
	assert!(source_growth > 0, "the retained source did not grow at all");
	assert!(
		source_growth < payload / 100,
		"the source component grew {source_growth} against a payload of {payload}"
	);
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_0005_an_input_past_the_ceiling_is_not_interrupted() {
	// Not a hard limit: the input that crosses the ceiling completes, and the
	// refusal arrives at the next one.
	let history = history_file("nothard");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	let start = live_bytes(&mut t);
	// A ceiling just above the current figure, reached by the next input.
	let mut t2 = Terminal::spawn_with(
		&history_file("nothard2"),
		&[("RNX_MEMORY_CEILING", &(start + 4 * 1024 * 1024).to_string())],
	);
	t.send(":quit\r");
	t.wait_exit();
	t2.prompt();
	// This input allocates far past the ceiling and still returns its value.
	t2.send(
		"let big = String::new(); for i in 0..1048576 { big.push_str(\"0123456789abcdef\") } big.len()\r",
	);
	t2.expect("16777216");
	t2.prompt();
	// The refusal arrives at the next input, not during that one.
	t2.send("1 + 1\r");
	t2.expect("at or above the ceiling");
	t2.prompt();
	// Inspection still answers over the ceiling.
	t2.send(":vars\r");
	t2.expect("big: String");
	t2.prompt();
	t2.send(":quit\r");
	t2.wait_exit();
}

#[test]
fn gate_0005_a_self_referential_value_is_not_reclaimed_by_a_reset() {
	// Rune's values are reference counted with no cycle collector, so a value
	// that holds itself keeps its payload alive past `:reset`. The gate
	// measures that rather than assuming it either way.
	let history = history_file("cycle");
	let mut t = Terminal::spawn(&history);
	t.prompt();
	let start = live_bytes(&mut t);
	t.send(
		"let payload = String::new(); for i in 0..524288 { payload.push_str(\"0123456789abcdef\") }\r",
	);
	t.prompt();
	let with_payload = live_bytes(&mut t);
	let payload = 8 * 1024 * 1024;
	assert!(with_payload - start >= payload, "the payload did not land");
	t.send("let cycle = [payload]; cycle.push(cycle);\r");
	t.prompt();
	t.send(":reset\r");
	t.expect("session reset");
	t.prompt();
	let after_reset = live_bytes(&mut t);
	// The payload survives the reset, because the cycle still holds it.
	assert!(
		after_reset - start >= payload,
		"the cycle was reclaimed after all: {after_reset} against {start}"
	);
	t.send(":quit\r");
	t.wait_exit();

	// The control: the same payload without the cycle is reclaimed.
	let mut t = Terminal::spawn(&history_file("cycle_control"));
	t.prompt();
	let start = live_bytes(&mut t);
	t.send(
		"let payload = String::new(); for i in 0..524288 { payload.push_str(\"0123456789abcdef\") }\r",
	);
	t.prompt();
	t.send(":reset\r");
	t.expect("session reset");
	t.prompt();
	let after_reset = live_bytes(&mut t);
	assert!(
		after_reset - start < payload / 8,
		"the payload was not reclaimed without a cycle: {after_reset} against {start}"
	);
	t.send(":quit\r");
	t.wait_exit();
}

#[test]
fn gate_0005_a_failed_input_that_grew_shared_state_still_charges() {
	// The first release record's rule, on the measured figure: an input that
	// grows an already retained value and then fails has still grown it, and
	// the sample after the failure charges it.
	let probe = Terminal::spawn(&history_file("charge_probe"));
	let mut probe = probe;
	probe.prompt();
	let start = live_bytes(&mut probe);
	probe.send(":quit\r");
	probe.wait_exit();

	let mut t = Terminal::spawn_with(
		&history_file("charge"),
		&[("RNX_MEMORY_CEILING", &(start + 4 * 1024 * 1024).to_string())],
	);
	t.prompt();
	t.send("let shared = [];\r");
	t.prompt();
	// Grows the retained vector far past the ceiling, then fails.
	t.send("for i in 0..1048576 { shared.push(i) } panic!(\"stop\")\r");
	t.expect("runtime error");
	t.prompt();
	// The growth stands, so the sample after the failed input charges it and
	// the next evaluation is refused.
	t.send("1 + 1\r");
	t.expect("at or above the ceiling");
	t.prompt();
	// The mutation the failed input made is still visible to inspection.
	t.send(":vars\r");
	t.expect("shared: Vec");
	t.prompt();
	t.send(":quit\r");
	t.wait_exit();
}
