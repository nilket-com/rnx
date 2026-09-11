//! What `host::stdin` reads, and what it refuses.
//!
//! Unix uses a pseudo-terminal; Windows uses a headless ConPTY console.
//! Both must refuse input with the same message. Pipe and file cases run on
//! both platforms, with a separate Windows NUL case for record 0025 gate 16.
//!
//! Record 0012 decides that the stream is read once, that a terminal is
//! refused rather than read, and that the limit and the refusals are the ones
//! `host::read` already has. Each of those is a behaviour, so each has a test
//! that fails if it stops holding.

#[cfg(windows)]
#[path = "harness/console.rs"]
mod console;

use std::io::Write;
#[cfg(unix)]
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Ran {
	stdout: String,
	stderr: String,
	code: i32,
}

/// A directory of this test's own, so parallel tests never share a name.
fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-stdin-{}-{}",
		std::process::id(),
		NEXT.fetch_add(1, Ordering::Relaxed)
	));
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

fn write_script(dir: &Path, source: &str) -> PathBuf {
	let path = dir.join("script.rn");
	std::fs::write(&path, source).unwrap();
	path
}

fn finish(output: std::process::Output) -> Ran {
	Ran {
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		code: output.status.code().unwrap_or(-1),
	}
}

/// Run a script with `input` piped into it.
fn piped(source: &str, input: &[u8]) -> Ran {
	let dir = scratch();
	let path = write_script(&dir, source);
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.take().unwrap().write_all(input).unwrap();
	let ran = finish(child.wait_with_output().unwrap());
	let _ = std::fs::remove_dir_all(&dir);
	ran
}

/// Run a script with `file` redirected into it. Used where the input is large
/// enough that filling a pipe from this thread could deadlock.
fn redirected(source: &str, file: &Path) -> Ran {
	let dir = scratch();
	let path = write_script(&dir, source);
	let output = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.stdin(Stdio::from(std::fs::File::open(file).unwrap()))
		.output()
		.unwrap();
	let ran = finish(output);
	let _ = std::fs::remove_dir_all(&dir);
	ran
}

/// Run a script whose standard input is a terminal.
#[cfg(unix)]
fn on_a_terminal(source: &str) -> Ran {
	let dir = scratch();
	let path = write_script(&dir, source);
	let mut leader = 0;
	let mut follower = 0;
	// SAFETY: openpty fills the two descriptors and is given no other buffers.
	let opened = unsafe {
		libc::openpty(
			&mut leader,
			&mut follower,
			std::ptr::null_mut(),
			std::ptr::null_mut(),
			std::ptr::null_mut(),
		)
	};
	assert_eq!(opened, 0, "openpty failed");
	// SAFETY: both descriptors are freshly opened and owned from here.
	let leader = unsafe { OwnedFd::from_raw_fd(leader) };
	let follower = unsafe { OwnedFd::from_raw_fd(follower) };
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.stdin(Stdio::from(follower))
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	// Bounded, because the failure this test guards against is a script that
	// waits for an end-of-file the terminal will never send. An unbounded
	// wait would hang the suite instead of failing this test.
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
	let outlived = loop {
		match child.try_wait().unwrap() {
			Some(_) => break false,
			None if std::time::Instant::now() >= deadline => {
				let _ = child.kill();
				break true;
			}
			None => std::thread::sleep(std::time::Duration::from_millis(20)),
		}
	};
	let output = child.wait_with_output().unwrap();
	drop(leader);
	let ran = finish(output);
	let _ = std::fs::remove_dir_all(&dir);
	assert!(
		!outlived,
		"the script was still reading the terminal after 10 seconds"
	);
	ran
}

/// Prints standard input exactly, escaped, so a trailing newline is visible.
const ECHO: &str = "pub fn main(args) {\n\tprintln!(\"{:?}\", host::stdin()?);\n\tOk(())\n}\n";

#[test]
fn a_pipe_is_read_exactly() {
	let ran = piped(ECHO, b"a\nb\n");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "\"a\\nb\\n\"\n");
	// A stream with no final newline keeps not having one.
	let ran = piped(ECHO, b"x");
	assert_eq!(ran.stdout, "\"x\"\n");
	// Bytes beyond ASCII survive as characters.
	let ran = piped(ECHO, "café\n".as_bytes());
	assert_eq!(ran.stdout, "\"café\\n\"\n");
}

#[test]
fn a_redirected_file_reads_as_the_same_text_host_read_gives() {
	let dir = scratch();
	let file = dir.join("input.txt");
	std::fs::write(&file, "one\ntwo\n").unwrap();
	let from_stream = redirected(ECHO, &file);
	assert_eq!(from_stream.code, 0, "{}", from_stream.stderr);
	let source = format!(
		"pub fn main(args) {{\n\tprintln!(\"{{:?}}\", host::read({:?})?);\n\tOk(())\n}}\n",
		file.to_string_lossy()
	);
	let from_path = piped(&source, b"");
	assert_eq!(from_stream.stdout, from_path.stdout);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_stream_is_not_an_error() {
	let ran = piped(ECHO, b"");
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "\"\"\n");
	assert_eq!(ran.stderr, "");
}

#[test]
#[cfg(unix)]
fn a_terminal_is_refused_and_does_not_block() {
	let start = std::time::Instant::now();
	let ran = on_a_terminal(ECHO);
	let elapsed = start.elapsed();
	assert_eq!(ran.code, 1, "{}", ran.stderr);
	assert!(ran.stderr.contains("it is a terminal"), "{}", ran.stderr);
	assert!(
		ran.stderr.contains("redirect a file or pipe into it"),
		"{}",
		ran.stderr
	);
	// Reading a terminal would wait for an end-of-file that never comes.
	assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");
	// A refusal is not a consumed stream, so it does not report one.
	assert!(!ran.stderr.contains("already been read"), "{}", ran.stderr);
}

#[test]
fn a_second_read_is_refused_and_is_not_the_empty_stream() {
	let source = "pub fn main(args) {\n\tlet first = host::stdin()?;\n\tprintln!(\"first {:?}\", first);\n\tlet second = host::stdin()?;\n\tprintln!(\"second {:?}\", second);\n\tOk(())\n}\n";
	let ran = piped(source, b"a\n");
	assert_eq!(ran.code, 1);
	// The first read succeeded and printed before the second was refused.
	assert_eq!(ran.stdout, "first \"a\\n\"\n");
	assert!(
		ran.stderr.contains("it has already been read"),
		"{}",
		ran.stderr
	);
	// An empty stream is a different thing and says so differently.
	let empty = piped(ECHO, b"");
	assert_eq!(empty.code, 0);
	assert!(
		!empty.stderr.contains("already been read"),
		"{}",
		empty.stderr
	);
}

#[test]
fn the_limit_holds_at_its_boundary() {
	let dir = scratch();
	let limit = 8 * 1024 * 1024;
	let at = dir.join("at.txt");
	std::fs::write(&at, vec![b'a'; limit]).unwrap();
	let source = "pub fn main(args) {\n\tprintln!(\"{}\", host::stdin()?.len());\n\tOk(())\n}\n";
	let ran = redirected(source, &at);
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, format!("{limit}\n"));

	let over = dir.join("over.txt");
	std::fs::write(&over, vec![b'a'; limit + 1]).unwrap();
	let ran = redirected(source, &over);
	assert_eq!(ran.code, 1);
	assert!(
		ran.stderr.contains("it exceeds the 8 MiB limit"),
		"{}",
		ran.stderr
	);
	let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn input_that_is_not_utf8_is_refused_rather_than_replaced() {
	let ran = piped(ECHO, b"ok \xff\xfe bad\n");
	assert_eq!(ran.code, 1);
	assert!(ran.stderr.contains("invalid utf-8"), "{}", ran.stderr);
	assert!(
		ran.stderr.starts_with("error: cannot read standard input:"),
		"{}",
		ran.stderr
	);
	// No replacement character reached the script.
	assert_eq!(ran.stdout, "");
	assert!(!ran.stderr.contains('\u{fffd}'), "{}", ran.stderr);
}

#[test]
fn the_read_is_bounded_and_does_not_wait_past_the_limit() {
	// A file cannot show this: it ends, so a read that waits for the end and
	// a read that stops at the limit finish alike. A pipe whose write end
	// stays open separates them. With the bound the read returns as soon as
	// the limit is passed; without it `read_to_end` waits for an end-of-file
	// that nothing is going to send, and this test fails instead of passing
	// by accident.
	let dir = scratch();
	let path = write_script(&dir, ECHO);
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.arg("run")
		.arg(&path)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let mut writer = child.stdin.take().unwrap();
	// Exactly one byte past the limit, so the bound is reached and no more.
	let payload = vec![b'a'; 8 * 1024 * 1024 + 1];
	// A broken pipe here would only mean the child stopped reading even
	// earlier, which is the same conclusion, so it is not a failure.
	let wrote = writer.write_all(&payload).and_then(|()| writer.flush());
	if let Err(e) = &wrote {
		assert_eq!(
			e.kind(),
			std::io::ErrorKind::BrokenPipe,
			"writing the payload failed for another reason: {e}"
		);
	}

	// The write end is deliberately still open. Nothing sends an end-of-file.
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
	let outlived = loop {
		match child.try_wait().unwrap() {
			Some(_) => break false,
			None if std::time::Instant::now() >= deadline => {
				let _ = child.kill();
				break true;
			}
			None => std::thread::sleep(std::time::Duration::from_millis(20)),
		}
	};
	let output = child.wait_with_output().unwrap();
	drop(writer);
	let ran = finish(output);
	let _ = std::fs::remove_dir_all(&dir);

	assert!(
		!outlived,
		"the script was still reading a stream that had passed the limit \
		 after 10 seconds, so the read is not bounded"
	);
	assert_eq!(ran.code, 1, "{}", ran.stderr);
	assert!(
		ran.stderr.contains("it exceeds the 8 MiB limit"),
		"{}",
		ran.stderr
	);
	assert_eq!(ran.stdout, "");
}

#[cfg(windows)]
#[test]
fn a_console_is_refused_and_does_not_block() {
	let dir = scratch();
	let path = write_script(&dir, ECHO);
	let mut terminal = console::Console::spawn(&["run", path.to_str().unwrap()], &[]);
	terminal.expect("it is a terminal");
	terminal.expect("redirect a file or pipe into it");
	let code = terminal.finish();
	drop(terminal);
	std::fs::remove_dir_all(&dir).unwrap();
	assert_eq!(code, 1);
}

#[cfg(windows)]
#[test]
fn nul_is_an_empty_stream_not_a_console() {
	let ran = redirected(ECHO, Path::new("NUL"));
	assert_eq!(ran.code, 0, "{}", ran.stderr);
	assert_eq!(ran.stdout, "\"\"\n");
	assert_eq!(ran.stderr, "");
}
