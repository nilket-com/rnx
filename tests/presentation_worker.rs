//! Record 0068: the notebook worker presents a top-level native value through
//! the same registry as the session, and falls back with bounded text.
#![cfg(all(target_os = "linux", feature = "test-support"))]
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

fn pipe() -> (OwnedFd, OwnedFd) {
	let mut fds = [0; 2];
	assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
	unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

struct Worker {
	child: Child,
	send: std::fs::File,
	receive: BufReader<std::fs::File>,
	next: u64,
}
impl Worker {
	fn spawn() -> Self {
		let (child_read, parent_write) = pipe();
		let (parent_read, child_write) = pipe();
		let (r, w) = (child_read.as_raw_fd(), child_write.as_raw_fd());
		let mut command = Command::new(env!("CARGO_BIN_EXE_rnx"));
		command
			.args([
				"worker",
				"--control-read",
				&r.to_string(),
				"--control-write",
				&w.to_string(),
			])
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped());
		unsafe {
			command.pre_exec(move || {
				for fd in [r, w] {
					let flags = libc::fcntl(fd, libc::F_GETFD);
					if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
						return Err(std::io::Error::last_os_error());
					}
				}
				Ok(())
			});
		}
		let child = command.spawn().unwrap();
		drop((child_read, child_write));
		let mut worker = Self {
			child,
			send: parent_write.into(),
			receive: BufReader::new(parent_read.into()),
			next: 0,
		};
		assert_eq!(worker.reply()["type"], "ready");
		worker
	}
	fn reply(&mut self) -> Value {
		let mut line = String::new();
		self.receive.read_line(&mut line).unwrap();
		assert!(!line.is_empty(), "worker closed its control pipe");
		serde_json::from_str(&line).unwrap()
	}
	/// One operation through to its settled reply, acknowledged.
	fn operate(&mut self, op: &str, source: Option<&str>) -> Value {
		self.next += 1;
		let mut request = json!({"op": op, "id": self.next, "nonce": "ab".repeat(32)});
		if let Some(source) = source {
			request["source"] = json!(source);
		}
		writeln!(self.send, "{request}").unwrap();
		loop {
			let reply = self.reply();
			if reply["type"] == "settled" {
				writeln!(self.send, "{}", json!({"op": "ack", "id": self.next})).unwrap();
				return reply;
			}
		}
	}
	fn execute(&mut self, source: &str) -> Value {
		self.operate("execute", Some(source))
	}
}
impl Drop for Worker {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

#[test]
fn a_presented_value_reaches_text_plain_bounded_and_escaped() {
	let mut w = Worker::spawn();
	let settled = w.execute("let v = rnx_test::test_presented(3);");
	assert_eq!(settled["text_plain"], Value::Null);
	let settled = w.execute("v");
	assert_eq!(settled["text_plain"], "Presented with 3 rows\n");
	assert_eq!(settled["failure"], Value::Null);
	assert_eq!(w.execute("[v]")["text_plain"], "[<::rnx_test::Presented>]");
	assert_eq!(w.execute("42")["text_plain"], "42");
	// The failure path keeps the evaluation and the bindings; the text is
	// the opaque label plus a bounded, escaped reason.
	let settled = w.execute("rnx_test::test_broken()");
	let text = settled["text_plain"].as_str().unwrap();
	assert!(
		text.starts_with("<::rnx_test::Broken> (preview unavailable: \\u{1b}[31mbroken"),
		"{text}"
	);
	assert!(
		!text.contains('\u{1b}') && text.len() <= 16_384,
		"{}",
		text.len()
	);
	assert_eq!(settled["failure"], Value::Null);
	assert_eq!(settled["render_bounded"], true);
	let settled = w.execute("rnx_test::test_loud()");
	let text = settled["text_plain"].as_str().unwrap();
	assert!(
		text.starts_with("\\u{1b}]0;title\\u{7}")
			&& !text.contains('\u{1b}')
			&& text.len() <= 16_384
	);
	assert_eq!(w.execute("v")["text_plain"], "Presented with 3 rows\n");
	// Reset keeps the registry.
	w.operate("reset", None);
	assert_eq!(
		w.execute("rnx_test::test_presented(1)")["text_plain"],
		"Presented with 1 rows\n"
	);
	w.operate("shutdown", None);
	assert_eq!(w.child.wait().unwrap().code(), Some(0));
}
