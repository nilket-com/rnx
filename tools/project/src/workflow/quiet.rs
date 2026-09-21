//! Record 0070: quiet preparation. While a preparation runs, the tool's
//! stderr — its own notices and everything Cargo writes — goes through a
//! pipe that a thread drains completely; a bounded prefix reaches
//! `.rnx/dep.log` and a rolling tail stays in memory for the failure report.
//! On success the terminal gets nothing; on failure it gets the error, the
//! tail and the log's path. Nothing here is an input to any identity.
use std::{
	fs::File,
	io::{Read, Write},
	os::fd::{AsRawFd, FromRawFd, OwnedFd},
	path::{Path, PathBuf},
	sync::{Arc, Mutex},
	time::Instant,
};

/// Bytes of Cargo output the log keeps before the marker.
const LOG_LIMIT: usize = crate::input::DOCUMENT_LIMIT;
/// The failure tail: at most this many lines and at most this many bytes.
const TAIL_LINES: usize = 30;
const TAIL_BYTES: usize = 16 * 1024;

fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
}

/// The rolling tail, the log and its fate, shared with the drain thread.
/// The log handle is the one `open_log` checked; nothing reopens the path.
#[derive(Default)]
struct Sinks {
	tail: std::collections::VecDeque<u8>,
	newlines: usize,
	/// Bytes seen before the log was attached, kept up to the log limit so
	/// the log starts with them: the scratch-directory repair notice prints
	/// before a project exists.
	pending: Vec<u8>,
	log: Option<File>,
	log_written: usize,
	unwritten: usize,
	log_error: Option<String>,
	#[cfg(test)]
	fail_after: Option<usize>,
}
impl Sinks {
	fn write_log(&mut self, bytes: &[u8]) {
		let Some(f) = self.log.as_mut() else {
			// No log yet: keep the prefix for it.
			let room = LOG_LIMIT.saturating_sub(self.pending.len());
			let take = bytes.len().min(room);
			self.pending.extend_from_slice(&bytes[..take]);
			self.unwritten += bytes.len() - take;
			return;
		};
		if self.log_error.is_some() {
			self.unwritten += bytes.len();
			return;
		}
		let room = LOG_LIMIT.saturating_sub(self.log_written);
		let take = bytes.len().min(room);
		#[cfg(test)]
		let take = match self.fail_after {
			Some(limit) if self.log_written + take > limit => {
				self.log_error = Some("injected write failure".into());
				self.unwritten += bytes.len();
				return;
			}
			_ => take,
		};
		if take > 0 {
			if let Err(e) = f.write_all(&bytes[..take]) {
				self.log_error = Some(e.to_string());
				self.unwritten += bytes.len();
				return;
			}
			self.log_written += take;
		}
		self.unwritten += bytes.len() - take;
	}
	fn push(&mut self, bytes: &[u8]) {
		self.write_log(bytes);
		// The tail: the last bytes, cut to the byte bound and then to the
		// line bound, where a trailing partial line is a line. The newline
		// count is kept, not recounted, so a flood costs its own length.
		if bytes.len() >= TAIL_BYTES {
			self.tail.clear();
			self.newlines = 0;
		}
		let kept = &bytes[bytes.len().saturating_sub(TAIL_BYTES)..];
		self.tail.extend(kept);
		self.newlines += kept.iter().filter(|b| **b == b'\n').count();
		while self.tail.len() > TAIL_BYTES {
			if self.tail.pop_front() == Some(b'\n') {
				self.newlines -= 1;
			}
		}
		while self.tail_lines() > TAIL_LINES {
			while let Some(b) = self.tail.pop_front() {
				if b == b'\n' {
					self.newlines -= 1;
					break;
				}
			}
		}
	}
	fn tail_lines(&self) -> usize {
		self.newlines + usize::from(self.tail.back().is_some_and(|b| *b != b'\n'))
	}
	/// Attach the checked log: the header, then everything seen so far.
	fn attach(&mut self, mut f: File, header: &str) {
		if let Err(e) = writeln!(f, "{header}") {
			self.log_error = Some(e.to_string());
			return;
		}
		self.log = Some(f);
		let pending = std::mem::take(&mut self.pending);
		self.write_log(&pending);
	}
	/// Through the checked handle only: the marker for what was not written,
	/// then the failure and its recovery commands.
	fn finalize(&mut self, failure: Option<&str>) {
		if self.log_error.is_some() {
			return;
		}
		let Some(f) = self.log.as_mut() else { return };
		let mut trailer = String::new();
		if self.unwritten > 0 {
			trailer.push_str(&format!(
				"\n[dep.log limit reached; {} more bytes were not written]\n",
				self.unwritten
			));
		}
		if let Some(text) = failure {
			trailer.push('\n');
			trailer.push_str(text);
			trailer.push('\n');
		}
		if let Err(e) = f.write_all(trailer.as_bytes()).and_then(|()| f.sync_all()) {
			self.log_error = Some(e.to_string());
		}
	}
	/// Whether the path still leads to the file this handle wrote.
	fn path_is_ours(&self, path: &Path) -> bool {
		use std::os::unix::fs::MetadataExt;
		let Some(f) = &self.log else { return false };
		match (f.metadata(), std::fs::symlink_metadata(path)) {
			(Ok(ours), Ok(there)) => {
				there.is_file() && ours.dev() == there.dev() && ours.ino() == there.ino()
			}
			_ => false,
		}
	}
	fn tail_text(&self) -> String {
		String::from_utf8_lossy(&self.tail.iter().copied().collect::<Vec<_>>())
			.trim_end()
			.to_owned()
	}
}

pub(super) struct Capture {
	saved_stderr: OwnedFd,
	saved_stdout: OwnedFd,
	done: std::sync::mpsc::Receiver<()>,
	sinks: Arc<Mutex<Sinks>>,
	log_path: Option<PathBuf>,
	log_open_error: Option<String>,
	started: Instant,
}

/// The log file, created like every managed `.rnx` file: private, no-follow,
/// and refused before truncation when the path is not a plain owned file
/// with one link. The caller holds the project's command lock.
fn open_log(path: &Path) -> Result<File, String> {
	use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
	let f = std::fs::OpenOptions::new()
		.write(true)
		.create(true)
		.truncate(false)
		.mode(0o600)
		// Non-blocking: a planted FIFO must not hold the tool waiting for a reader.
		.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
		.open(path)
		.map_err(|e| format!("{}: {e}", path.display()))?;
	let m = f.metadata().map_err(err)?;
	if !m.is_file()
		|| m.nlink() != 1
		|| m.uid() != unsafe { libc::geteuid() }
		|| m.mode() & 0o022 != 0
	{
		return Err(format!(
			"{}: not a private owned regular file with one link; remove it",
			path.display()
		));
	}
	f.set_len(0).map_err(err)?;
	// Blocking writes again for the drain thread.
	let flags = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_GETFL) };
	if flags < 0
		|| unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK) } < 0
	{
		return Err(err(std::io::Error::last_os_error()));
	}
	Ok(f)
}

fn dup_saved(fd: i32) -> Result<OwnedFd, String> {
	let saved = unsafe { libc::dup(fd) };
	if saved < 0 {
		return Err(err(std::io::Error::last_os_error()));
	}
	let saved = unsafe { OwnedFd::from_raw_fd(saved) };
	if unsafe { libc::fcntl(saved.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
		return Err(err(std::io::Error::last_os_error()));
	}
	Ok(saved)
}

impl Capture {
	/// Redirect this process's standard streams into the drain until
	/// `finish`. The drain thread exists before any descriptor moves, and
	/// a failure while moving them puts them back.
	pub(super) fn begin() -> Result<Self, String> {
		let saved_stderr = dup_saved(2)?;
		let saved_stdout = dup_saved(1)?;
		let mut fds = [0; 2];
		if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
			return Err(err(std::io::Error::last_os_error()));
		}
		let (read_end, write_end) =
			unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
		let sinks = Arc::new(Mutex::new(Sinks::default()));
		let shared = sinks.clone();
		let (sender, done) = std::sync::mpsc::channel();
		std::thread::Builder::new()
			.name("rnx-dep-drain".into())
			.spawn(move || {
				let mut reader = File::from(read_end);
				let mut buffer = [0u8; 8192];
				loop {
					match reader.read(&mut buffer) {
						Ok(0) => break,
						Ok(n) => shared.lock().unwrap().push(&buffer[..n]),
						Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
						Err(_) => break,
					}
				}
				let _ = sender.send(());
			})
			.map_err(err)?;
		// Both standard streams: the tool's own notices on either, and
		// everything Cargo writes. Children inherit fds 1 and 2; the pipe's
		// own ends stay close-on-exec. The protocol socket is neither.
		for (fd, saved) in [(1, &saved_stdout), (2, &saved_stderr)] {
			if unsafe { libc::dup2(write_end.as_raw_fd(), fd) } < 0 {
				let e = std::io::Error::last_os_error();
				// Roll back: whatever moved goes back before returning.
				unsafe {
					libc::dup2(saved_stdout.as_raw_fd(), 1);
					libc::dup2(saved_stderr.as_raw_fd(), 2);
				}
				let _ = saved;
				return Err(err(e));
			}
		}
		drop(write_end);
		Ok(Self {
			saved_stderr,
			saved_stdout,
			done,
			sinks,
			log_path: None,
			log_open_error: None,
			started: Instant::now(),
		})
	}
	/// Open the log under the project's command lock and hand the checked
	/// handle to the drain; the header is its first line and everything
	/// captured so far follows it.
	pub(super) fn attach_log(&mut self, dot: &Path, header: &str) {
		let path = dot.join("dep.log");
		match open_log(&path) {
			Ok(f) => {
				self.sinks.lock().unwrap().attach(f, header);
				self.log_path = Some(path);
			}
			Err(e) => self.log_open_error = Some(e),
		}
	}
	/// Restore the streams, wait for the drain within a bound (a writer we
	/// do not own may keep the pipe open; the tail then is what arrived),
	/// finalize the log through its checked handle, and report. On success,
	/// nothing: quiet prints no line that would read the same every time.
	/// On failure the error is the underlying error, the actual tail of the
	/// output and the log's path or its fate; the recovery commands, being
	/// standard text, go to the log.
	pub(super) fn finish(
		self,
		outcome: Result<(), String>,
		phase: &str,
		recovery: &str,
	) -> Result<(), String> {
		let restored = unsafe { libc::dup2(self.saved_stderr.as_raw_fd(), 2) } >= 0
			&& unsafe { libc::dup2(self.saved_stdout.as_raw_fd(), 1) } >= 0;
		let drained = self
			.done
			.recv_timeout(std::time::Duration::from_secs(5))
			.is_ok();
		let mut sinks = self.sinks.lock().unwrap();
		let _ = self.started.elapsed();
		let failure = outcome
			.as_ref()
			.err()
			.map(|e| format!("{phase}: {e}\n{recovery}"));
		sinks.finalize(failure.as_deref());
		match outcome {
			Ok(()) => {
				if !restored {
					return Err(
						"could not restore the standard streams after quiet preparation".into(),
					);
				}
				Ok(())
			}
			Err(e) => {
				let mut message = format!("{phase}: {e}");
				let tail = sinks.tail_text();
				if !tail.is_empty() {
					message.push('\n');
					message.push_str(&tail);
				}
				if !drained {
					message.push_str("\n[output still arriving when the report was made]");
				}
				match (&self.log_path, &sinks.log_error, &self.log_open_error) {
					// Only a path that still names the file the tool wrote is
					// worth printing; a replaced path names something else.
					(Some(p), None, _) if sinks.path_is_ours(p) => {
						message.push_str(&format!("\n{}", p.display()))
					}
					(Some(p), None, _) => message.push_str(&format!(
						"\n{}: replaced after it was written; not reported as the log",
						p.display()
					)),
					(Some(p), Some(e), _) => {
						message.push_str(&format!("\n{}: could not be written: {e}", p.display()))
					}
					(None, _, Some(e)) => message.push_str(&format!("\nno log: {e}")),
					(None, _, None) => {}
				}
				Err(message)
			}
		}
	}
	#[cfg(test)]
	fn inject_write_failure(&self, after: usize) {
		self.sinks.lock().unwrap().fail_after = Some(after);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::process::{Command, Stdio};
	fn temp(tag: &str) -> PathBuf {
		let d = std::env::temp_dir().join(format!("rnx-quiet-{}-{tag}", std::process::id()));
		let _ = std::fs::remove_dir_all(&d);
		std::fs::create_dir_all(&d).unwrap();
		d
	}
	#[test]
	fn the_tail_keeps_the_last_lines_within_both_bounds() {
		let mut s = Sinks::default();
		for i in 0..100 {
			s.push(format!("line {i}\n").as_bytes());
		}
		let text = s.tail_text();
		assert_eq!(text.lines().count(), TAIL_LINES);
		assert!(text.starts_with("line 70\n") && text.ends_with("line 99"));
		// Thirty terminated lines plus a partial one are thirty-one lines: one goes.
		s.push(b"partial");
		assert_eq!(s.tail_lines(), TAIL_LINES);
		assert!(s.tail_text().starts_with("line 71\n") && s.tail_text().ends_with("partial"));
		// One enormous line: cut to the byte bound, the end preserved.
		let mut s = Sinks::default();
		s.push(&[b'x'; 100_000]);
		s.push(b" the end");
		let text = s.tail_text();
		assert!(text.len() <= TAIL_BYTES && text.ends_with(" the end"));
	}
	#[test]
	fn the_log_is_opened_privately_and_planted_paths_refuse() {
		use std::os::unix::fs::{MetadataExt, PermissionsExt};
		let dir = temp("log");
		let path = dir.join("dep.log");
		let f = open_log(&path).unwrap();
		assert_eq!(f.metadata().unwrap().mode() & 0o777, 0o600);
		drop(f);
		std::fs::write(&path, b"previous attempt").unwrap();
		let f = open_log(&path).unwrap();
		assert_eq!(f.metadata().unwrap().len(), 0, "replaced, not appended");
		drop(f);
		let target = dir.join("elsewhere");
		std::fs::write(&target, b"do not truncate").unwrap();
		std::fs::remove_file(&path).unwrap();
		std::os::unix::fs::symlink(&target, &path).unwrap();
		assert!(open_log(&path).is_err());
		assert_eq!(std::fs::read(&target).unwrap(), b"do not truncate");
		std::fs::remove_file(&path).unwrap();
		std::fs::write(&path, b"linked").unwrap();
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
		std::fs::hard_link(&path, dir.join("twin")).unwrap();
		assert!(open_log(&path).is_err());
		assert_eq!(std::fs::read(dir.join("twin")).unwrap(), b"linked");
		std::fs::remove_file(dir.join("twin")).unwrap();
		std::fs::remove_file(&path).unwrap();
		let fifo = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
		assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
		assert!(open_log(&path).is_err());
		let _ = std::fs::remove_dir_all(&dir);
	}
	/// A real child on both streams, past pipe capacity and past the log
	/// limit, then a distinct final error: the tail ends with the error, the
	/// log holds the header, the prefix and the byte-counted marker, and
	/// nothing reaches the terminal. Serialized: the capture moves this
	/// process's descriptors.
	#[test]
	fn a_flooding_child_is_drained_and_reported() {
		let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let dir = temp("flood");
		let mut c = Capture::begin().unwrap();
		c.attach_log(&dir, "reopen with: fixture");
		// The harness captures `eprintln!`; the tool's notices reach fd 2.
		fd2("a tool notice before the child\n");
		let status = Command::new("sh")
			.arg("-c")
			.arg(format!(
				"yes 'out line' | head -c {n}; yes 'err line' | head -c {n} >&2; yes 'out line' | head -c {n}; echo 'error: the distinct final error' >&2",
				n = LOG_LIMIT / 2 + 1_000_000
			))
			.stdin(Stdio::null())
			.status()
			.unwrap();
		assert!(status.success());
		let result = c.finish(
			Err("child failed".into()),
			"build/attach",
			"retry with: fixture",
		);
		let message = result.unwrap_err();
		assert!(
			message.starts_with("build/attach: child failed\n"),
			"{message}"
		);
		assert!(
			message.contains("error: the distinct final error\n"),
			"{}",
			&message[message.len().saturating_sub(400)..]
		);
		assert!(message.trim_end().ends_with("dep.log"), "{message}");
		assert!(!message.contains("[output still arriving"));
		let log = std::fs::read(dir.join("dep.log")).unwrap();
		let text = String::from_utf8_lossy(&log);
		assert!(
			text.starts_with("reopen with: fixture\na tool notice before the child\n"),
			"{}",
			&text[..80]
		);
		assert!(text.contains("out line\n") && text.contains("err line\n"));
		assert!(log.len() < LOG_LIMIT + 4096, "{}", log.len());
		let marker = text.rfind("[dep.log limit reached; ").expect("marker");
		let count: usize = text[marker + "[dep.log limit reached; ".len()..]
			.split(' ')
			.next()
			.unwrap()
			.parse()
			.unwrap();
		assert!(count > 100_000, "{count}");
		assert!(
			text.ends_with("build/attach: child failed\nretry with: fixture\n"),
			"{}",
			&text[text.len() - 120..]
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
	/// Parsed output keeps its own pipe under capture: a child's stdout
	/// captured by the caller arrives intact while its stderr goes to the log.
	#[test]
	fn parsed_stdout_is_untouched_while_stderr_is_captured() {
		let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let dir = temp("metadata");
		let mut c = Capture::begin().unwrap();
		c.attach_log(&dir, "header");
		let out = Command::new("sh")
			.arg("-c")
			.arg("echo '{\"packages\":[]}' ; echo 'diagnostic' >&2")
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit()) // fd 2: the capture
			.output()
			.unwrap();
		assert_eq!(out.stdout, b"{\"packages\":[]}\n");
		c.finish(Ok(()), "resolve", "").unwrap();
		let text = std::fs::read_to_string(dir.join("dep.log")).unwrap();
		assert!(
			text.contains("diagnostic\n") && !text.contains("packages"),
			"{text}"
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
	/// A write failure in the middle of the stream: the log stops there, the
	/// tail continues, and the report names the failure instead of a path.
	#[test]
	fn a_mid_stream_write_failure_keeps_the_tail_and_reports_no_usable_log() {
		let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let dir = temp("midwrite");
		let mut c = Capture::begin().unwrap();
		c.attach_log(&dir, "header");
		c.inject_write_failure(2000);
		let status = Command::new("sh")
			.arg("-c")
			.arg("i=0; while [ $i -lt 2000 ]; do echo 'a line of output'; i=$((i+1)); done; echo 'the final error' >&2")
			.stdin(Stdio::null())
			.status()
			.unwrap();
		assert!(status.success());
		let message = c
			.finish(Err("failed".into()), "build/attach", "retry")
			.unwrap_err();
		assert!(message.contains("the final error"), "{message}");
		assert!(
			message.contains("dep.log: could not be written: injected write failure"),
			"{message}"
		);
		let log = std::fs::read_to_string(dir.join("dep.log")).unwrap();
		assert!(
			log.len() < 4000 && !log.contains("the final error") && !log.contains("retry"),
			"{}",
			log.len()
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
	/// Finalization writes through the checked handle: replacing the path by
	/// a symlink after the log was opened reaches nothing outside.
	#[test]
	fn finalization_never_follows_a_replaced_path() {
		let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let dir = temp("replace");
		let mut c = Capture::begin().unwrap();
		c.attach_log(&dir, "header");
		let sentinel = dir.join("sentinel");
		std::fs::write(&sentinel, b"untouched").unwrap();
		std::fs::remove_file(dir.join("dep.log")).unwrap();
		std::os::unix::fs::symlink(&sentinel, dir.join("dep.log")).unwrap();
		let message = c
			.finish(Err("failed".into()), "build/attach", "recovery text")
			.unwrap_err();
		assert!(message.contains("failed"));
		assert_eq!(std::fs::read(&sentinel).unwrap(), b"untouched");
		assert!(
			message.contains("replaced after it was written")
				&& !message.trim_end().ends_with("dep.log"),
			"{message}"
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
	/// Cancellation while the pipes are full: the child's group is killed as
	/// 0063's handling does, it is reaped, the report is prompt and holds the
	/// tail, and this process's streams are restored.
	#[test]
	fn cancellation_with_full_pipes_is_prompt_and_reaped() {
		use std::os::unix::process::CommandExt;
		let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let dir = temp("cancel");
		let mut c = Capture::begin().unwrap();
		c.attach_log(&dir, "header");
		let mut child = Command::new("sh")
			.arg("-c")
			.arg("while :; do echo 'flooding stdout'; echo 'flooding stderr' >&2; done")
			.stdin(Stdio::null())
			.process_group(0)
			.spawn()
			.unwrap();
		std::thread::sleep(std::time::Duration::from_millis(300));
		unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
		let status = child.wait().unwrap();
		assert!(!status.success());
		let start = Instant::now();
		let message = c
			.finish(Err("interrupted".into()), "build/attach", "")
			.unwrap_err();
		assert!(start.elapsed() < std::time::Duration::from_secs(5));
		assert!(
			message.contains("flooding") && !message.contains("[output still arriving"),
			"{message}"
		);
		// The group is gone: signalling it finds no process.
		assert_eq!(unsafe { libc::kill(-(child.id() as i32), 0) }, -1);
		assert_eq!(
			std::io::Error::last_os_error().raw_os_error(),
			Some(libc::ESRCH)
		);
		// The streams are ours again.
		fd2("restored\n");
		let _ = std::fs::remove_dir_all(&dir);
	}
	/// A writer we do not own keeps the pipe open: finalization is bounded
	/// and says so.
	#[test]
	fn a_lingering_writer_cannot_hang_finalization() {
		use std::os::unix::process::CommandExt;
		let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let dir = temp("linger");
		let mut c = Capture::begin().unwrap();
		c.attach_log(&dir, "header");
		let mut lingering = Command::new("sh")
			.arg("-c")
			.arg("echo early; sleep 30")
			.stdin(Stdio::null())
			.process_group(0)
			.spawn()
			.unwrap();
		std::thread::sleep(std::time::Duration::from_millis(200));
		let start = Instant::now();
		let message = c
			.finish(Err("failed".into()), "build/attach", "")
			.unwrap_err();
		assert!(
			start.elapsed() < std::time::Duration::from_secs(8),
			"{:?}",
			start.elapsed()
		);
		assert!(
			message.contains("early") && message.contains("[output still arriving"),
			"{message}"
		);
		// Clean up the whole group, the `sleep` descendant included, and prove it gone.
		unsafe { libc::kill(-(lingering.id() as i32), libc::SIGKILL) };
		lingering.wait().unwrap();
		let deadline = Instant::now() + std::time::Duration::from_secs(5);
		while unsafe { libc::kill(-(lingering.id() as i32), 0) } == 0 && Instant::now() < deadline {
			std::thread::sleep(std::time::Duration::from_millis(20));
		}
		assert_eq!(unsafe { libc::kill(-(lingering.id() as i32), 0) }, -1);
		let _ = std::fs::remove_dir_all(&dir);
	}
	static SERIAL: Mutex<()> = Mutex::new(());
	fn fd2(text: &str) {
		unsafe { libc::write(2, text.as_ptr().cast(), text.len()) };
	}
}
