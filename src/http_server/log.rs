//! Bounded producers; only this thread can block on its duplicated stderr handle.
use std::{
	io::{self, Write},
	sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
		mpsc::{self, SyncSender},
	},
	thread::{self, JoinHandle},
	time::{Duration, Instant},
};
const RECORD: usize = 4096;
const QUEUE: usize = 256;
#[derive(Default)]
struct Counts {
	enqueued: AtomicU64,
	written: AtomicU64,
	refused: AtomicU64,
	write_failed: AtomicU64,
}
#[derive(Clone)]
pub(super) struct Log {
	tx: SyncSender<Vec<u8>>,
	counts: Arc<Counts>,
	requests: bool,
}
pub(super) struct Logger {
	pub log: Log,
	thread: JoinHandle<()>,
}
#[derive(Debug)]
pub(super) struct Report {
	pub enqueued: u64,
	pub written: u64,
	pub dropped: u64,
	pub outstanding: u64,
	pub detached: bool,
}
/// Bound input before escaping, preserving whole characters and escape tokens.
fn text(s: &str, chars: usize) -> String {
	crate::format::terminal_safe(&s.chars().take(chars).collect::<String>())
}
impl Log {
	pub fn event(&self, event: &str, detail: &str) {
		self.send(serde_json::json!({"event":event,"detail":text(detail,256)}));
	}
	pub fn request(&self, method: &str, path: &str, status: Option<u16>, elapsed: Duration) {
		if self.requests {
			self.send(serde_json::json!({"event":"request","method":text(method,16),"path":text(path,128),"status":status,"outcome":if status.is_some() { "completed" } else { "cancelled" },"duration_us":elapsed.as_micros().min(u64::MAX as u128) as u64}));
		}
	}
	fn send(&self, event: serde_json::Value) {
		let mut bytes = event.to_string().into_bytes();
		bytes.push(b'\n');
		if bytes.len() > RECORD {
			self.counts.refused.fetch_add(1, Ordering::Relaxed);
			return;
		}
		match self.tx.try_send(bytes) {
			Ok(()) => {
				self.counts.enqueued.fetch_add(1, Ordering::Relaxed);
			}
			Err(_) => {
				self.counts.refused.fetch_add(1, Ordering::Relaxed);
			}
		}
	}
}
impl Logger {
	pub fn new(writer: impl Write + Send + 'static, requests: bool) -> io::Result<Self> {
		let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(QUEUE);
		let counts = Arc::new(Counts::default());
		let thread_counts = counts.clone();
		let thread = thread::Builder::new()
			.name("rnx-http-log".into())
			.spawn(move || {
				let mut writer = writer;
				while let Ok(record) = rx.recv() {
					if writer.write_all(&record).is_ok() {
						thread_counts.written.fetch_add(1, Ordering::Relaxed);
					} else {
						thread_counts.write_failed.fetch_add(1, Ordering::Relaxed);
					}
				}
				let dropped = thread_counts.refused.load(Ordering::Relaxed)
					+ thread_counts.write_failed.load(Ordering::Relaxed);
				let _ = writeln!(
					writer,
					"{{\"event\":\"log_summary\",\"dropped\":{dropped}}}"
				);
			})?;
		Ok(Self {
			log: Log {
				tx,
				counts,
				requests,
			},
			thread,
		})
	}
	pub fn shutdown(self) -> Report {
		let Self { log, thread } = self;
		let counts = log.counts.clone();
		drop(log); // Host drops all other producers before shutdown.
		let deadline = Instant::now() + Duration::from_millis(100);
		while !thread.is_finished() && Instant::now() < deadline {
			thread::sleep(Duration::from_millis(1));
		}
		let detached = !thread.is_finished();
		if !detached {
			let _ = thread.join();
		} // Otherwise dropping the handle detaches it.
		let enqueued = counts.enqueued.load(Ordering::Relaxed);
		let written = counts.written.load(Ordering::Relaxed);
		let failed = counts.write_failed.load(Ordering::Relaxed);
		Report {
			enqueued,
			written,
			dropped: counts.refused.load(Ordering::Relaxed) + failed,
			outstanding: enqueued.saturating_sub(written + failed),
			detached,
		}
	}
}
#[cfg(unix)]
pub(super) fn stderr() -> io::Result<std::fs::File> {
	use std::os::fd::{FromRawFd, OwnedFd};
	// SAFETY: duplicating a valid standard descriptor creates a new owned handle;
	// no open-file-description flags are changed.
	let fd = unsafe { libc::fcntl(2, libc::F_DUPFD_CLOEXEC, 3) };
	if fd < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}
#[cfg(windows)]
pub(super) fn stderr() -> io::Result<std::fs::File> {
	use std::os::windows::io::{FromRawHandle, OwnedHandle};
	use windows_sys::Win32::{
		Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle},
		System::{
			Console::{GetStdHandle, STD_ERROR_HANDLE},
			Threading::GetCurrentProcess,
		},
	};
	let mut duplicate = std::ptr::null_mut();
	// SAFETY: DuplicateHandle creates the owned handle without changing the source.
	let ok = unsafe {
		let process = GetCurrentProcess();
		DuplicateHandle(
			process,
			GetStdHandle(STD_ERROR_HANDLE),
			process,
			&mut duplicate,
			0,
			0,
			DUPLICATE_SAME_ACCESS,
		)
	};
	if ok == 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(std::fs::File::from(unsafe {
		OwnedHandle::from_raw_handle(duplicate)
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	struct Blocked(std::sync::mpsc::Receiver<()>);
	impl Write for Blocked {
		fn write(&mut self, b: &[u8]) -> io::Result<usize> {
			let _ = self.0.recv();
			Ok(b.len())
		}
		fn flush(&mut self) -> io::Result<()> {
			Ok(())
		}
	}
	#[test]
	fn full_logger_has_bounded_records_and_detaches_without_losing_accounting() {
		let (release, rx) = mpsc::channel();
		let logger = Logger::new(Blocked(rx), true).unwrap();
		let start = Instant::now();
		for _ in 0..2000 {
			logger.log.event("test", "\u{1b}\u{202e}");
		}
		assert!(start.elapsed() < Duration::from_secs(1));
		let report = logger.shutdown();
		assert!(report.detached);
		assert!(report.enqueued <= 257);
		assert_eq!(report.written, 0);
		assert_eq!(report.enqueued + report.dropped, 2000);
		assert_eq!(report.outstanding, report.enqueued);
		assert!(start.elapsed() < Duration::from_secs(2));
		drop(release);
	}
	#[test]
	fn off_suppresses_requests_but_keeps_safe_failure_records() {
		let logger = Logger::new(io::sink(), false).unwrap();
		logger.log.request("GET", "/", Some(200), Duration::ZERO);
		logger.log.event("failure", "\u{1b}\u{202e}");
		let report = logger.shutdown();
		assert_eq!(report.enqueued, 1);
		assert_eq!(report.written, 1);
		assert!(!report.detached);
		assert!(!text("\u{1b}\u{202e}", 256).contains('\u{1b}'));
	}
	#[cfg(unix)]
	#[test]
	fn duplicating_stderr_does_not_change_shared_description_flags() {
		// SAFETY: query flags only; no process descriptor is replaced by this parallel test.
		let before = unsafe { libc::fcntl(2, libc::F_GETFL) };
		assert!(before >= 0);
		let duplicate = stderr().unwrap();
		use std::os::fd::AsRawFd;
		let dup = unsafe { libc::fcntl(duplicate.as_raw_fd(), libc::F_GETFL) };
		assert_eq!(before, dup);
		drop(duplicate);
		assert_eq!(before, unsafe { libc::fcntl(2, libc::F_GETFL) });
	}
}
