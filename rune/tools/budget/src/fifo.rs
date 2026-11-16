//! A final's jobserver: a named fifo of tokens that the controller, Cargo,
//! rustc and every stage's own clients share.
//!
//! The fifo is the GNU make protocol (`-jN --jobserver-auth=fifo:PATH`), so a
//! client that never heard of this crate takes part: Cargo, rustc, and the
//! stage programs that read `MAKEFLAGS` themselves. This module is transport
//! only; which stage may take tokens, and when, is the controller's policy.
use anyhow::{bail, ensure, Context as _, Result};
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// How long a token return may wait on a full fifo before the tokens are
/// reported lost; a return never blocks cleanup indefinitely.
pub const RETURN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

struct State {
	fifo: File,
	outstanding: Mutex<usize>,
	/// Returns that failed: tokens lost from the pool, reported at close.
	lost: Mutex<Vec<String>>,
	return_deadline: std::time::Duration,
}

impl State {
	/// Every byte goes back, across partial writes, EINTR and a full fifo,
	/// within one deadline over all retries.
	fn write(&self, mut bytes: &[u8]) -> std::io::Result<()> {
		let deadline = std::time::Instant::now() + self.return_deadline;
		while !bytes.is_empty() {
			match (&self.fifo).write(bytes) {
				Ok(0) => {
					return Err(std::io::Error::new(
						ErrorKind::WriteZero,
						"the jobserver accepted no bytes",
					))
				}
				Ok(n) => bytes = &bytes[n..],
				Err(error) if error.kind() == ErrorKind::Interrupted => {}
				Err(error) if error.kind() == ErrorKind::WouldBlock => {
					let left = deadline.saturating_duration_since(std::time::Instant::now());
					if left.is_zero() {
						return Err(std::io::Error::new(
							ErrorKind::TimedOut,
							format!(
								"the jobserver stayed full for {} s",
								self.return_deadline.as_secs_f64()
							),
						));
					}
					let mut ready = libc::pollfd {
						fd: self.fifo.as_raw_fd(),
						events: libc::POLLOUT,
						revents: 0,
					};
					// SAFETY: one valid pollfd for the duration of the call.
					if unsafe {
						libc::poll(
							&mut ready,
							1,
							left.as_millis().clamp(1, i32::MAX as u128) as i32,
						)
					} < 0
					{
						let error = std::io::Error::last_os_error();
						if error.kind() != ErrorKind::Interrupted {
							return Err(error);
						}
					}
				}
				Err(error) => return Err(error),
			}
		}
		Ok(())
	}

	fn give(&self, bytes: &[u8]) -> std::io::Result<()> {
		let result = self.write(bytes);
		*self.outstanding.lock().unwrap() -= bytes.len();
		if let Err(error) = &result {
			self.lost
				.lock()
				.unwrap()
				.push(format!("{} tokens not returned: {error}", bytes.len()));
		}
		result
	}
}

/// Tokens taken from one [`Pool`]. Not `Clone`, and not constructible
/// outside this module: the only tokens a pool receives are its own, given
/// back. Dropping a lease returns it, so cancellation loses nothing.
#[must_use = "a lease returns its tokens when dropped"]
pub struct Tokens {
	state: Arc<State>,
	bytes: Vec<u8>,
}

impl Tokens {
	pub fn count(&self) -> usize {
		self.bytes.len()
	}
}

impl Drop for Tokens {
	fn drop(&mut self) {
		if !self.bytes.is_empty() {
			let _ = self.state.give(&std::mem::take(&mut self.bytes));
		}
	}
}

pub struct Pool {
	path: PathBuf,
	capacity: usize,
	state: Arc<State>,
}

impl Pool {
	/// A new fifo at `path` holding `capacity` tokens.
	pub fn create(path: &Path, capacity: usize) -> Result<Self> {
		ensure!(capacity >= 1, "a jobserver needs one token");
		let name = CString::new(path.as_os_str().as_bytes()).context("fifo path has a NUL byte")?;
		// SAFETY: name is a valid NUL-terminated path.
		if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } != 0 {
			return Err(std::io::Error::last_os_error())
				.with_context(|| format!("mkfifo {}", path.display()));
		}
		let fifo = OpenOptions::new()
			.read(true)
			.write(true)
			.custom_flags(libc::O_NONBLOCK)
			.open(path)
			.with_context(|| format!("open {}", path.display()))?;
		let state = Arc::new(State {
			fifo,
			outstanding: Mutex::new(0),
			lost: Mutex::new(Vec::new()),
			return_deadline: RETURN_DEADLINE,
		});
		state.write(&vec![b'+'; capacity])?;
		Ok(Self {
			path: path.to_owned(),
			capacity,
			state,
		})
	}

	pub fn capacity(&self) -> usize {
		self.capacity
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	/// `count` tokens at once, or none: the controller never waits holding a
	/// part, and a partial read goes back before this returns.
	pub fn take(&self, count: usize) -> Result<Option<Tokens>> {
		ensure!(
			count >= 1 && count <= self.capacity,
			"invalid token request: {count} of {}",
			self.capacity
		);
		let mut bytes = Vec::with_capacity(count);
		let mut failure = None;
		while bytes.len() < count {
			let mut buffer = vec![0; count - bytes.len()];
			match (&self.state.fifo).read(&mut buffer) {
				Ok(0) => break,
				Ok(n) => bytes.extend_from_slice(&buffer[..n]),
				Err(error) if error.kind() == ErrorKind::Interrupted => {}
				Err(error) if error.kind() == ErrorKind::WouldBlock => break,
				Err(error) => {
					failure = Some(error);
					break;
				}
			}
		}
		*self.state.outstanding.lock().unwrap() += bytes.len();
		let lease = Tokens {
			state: self.state.clone(),
			bytes,
		};
		if let Some(error) = failure {
			drop(lease);
			return Err(error).context("take jobserver tokens");
		}
		if lease.count() < count {
			drop(lease);
			return Ok(None);
		}
		Ok(Some(lease))
	}

	/// Return a lease now, reporting a failed return.
	pub fn give(&self, mut tokens: Tokens) -> Result<()> {
		ensure!(
			Arc::ptr_eq(&tokens.state, &self.state),
			"tokens belong to another jobserver"
		);
		let bytes = std::mem::take(&mut tokens.bytes);
		self.state.give(&bytes).context("return jobserver tokens")
	}

	/// Tokens taken and not yet returned.
	pub fn outstanding(&self) -> usize {
		*self.state.outstanding.lock().unwrap()
	}

	/// The jobserver flags a stage that shares this pool receives.
	pub fn flags(&self) -> String {
		format!(
			"-j{} --jobserver-auth=fifo:{}",
			self.capacity,
			self.path.display()
		)
	}

	/// Remove the fifo's name once every stage has ended, so no client
	/// still needs to open it. A lease still out, or a return that failed,
	/// is reported; the name is removed either way.
	pub fn close(self) -> Result<()> {
		let outstanding = self.outstanding();
		std::fs::remove_file(&self.path)
			.with_context(|| format!("remove {}", self.path.display()))?;
		let lost = self.state.lost.lock().unwrap();
		if !lost.is_empty() {
			bail!("jobserver lost tokens: {}", lost.join("; "));
		}
		ensure!(
			outstanding == 0,
			"{outstanding} jobserver tokens were still leased when the pool closed"
		);
		Ok(())
	}
}

/// The width a stage inherited from its final's jobserver, if it runs in one.
pub fn inherited_width(makeflags: &str) -> Option<usize> {
	if !makeflags.contains("--jobserver-auth=fifo:") {
		return None;
	}
	makeflags
		.split_whitespace()
		.find_map(|flag| flag.strip_prefix("-j"))
		.and_then(|width| width.parse().ok())
}

#[cfg(test)]
mod tests {
	use super::*;

	struct Dir(PathBuf);

	impl Dir {
		fn new() -> Self {
			let nanos = std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos();
			let base =
				std::env::temp_dir().join(format!("job-budget-{}-{nanos}", std::process::id()));
			std::fs::create_dir(&base).unwrap();
			Self(base)
		}
	}

	impl Drop for Dir {
		fn drop(&mut self) {
			let _ = std::fs::remove_dir_all(&self.0);
		}
	}

	fn pool(capacity: usize) -> (Dir, Pool) {
		let dir = Dir::new();
		let pool = Pool::create(&dir.0.join("jobserver.fifo"), capacity).unwrap();
		(dir, pool)
	}

	/// The tokens in the fifo, read without the pool's bookkeeping.
	fn available(pool: &Pool) -> usize {
		let mut count = 0;
		while let Some(lease) = pool.take(1).unwrap() {
			std::mem::forget(lease);
			count += 1;
		}
		pool.state.write(&vec![b'+'; count]).unwrap();
		*pool.state.outstanding.lock().unwrap() -= count;
		count
	}

	#[test]
	fn tokens_are_taken_whole_or_not_at_all() {
		let (_dir, pool) = pool(3);
		let two = pool.take(2).unwrap().unwrap();
		assert!(
			pool.take(2).unwrap().is_none(),
			"one token left; two refused"
		);
		assert_eq!(available(&pool), 1, "the refused partial read went back");
		pool.give(two).unwrap();
		let all = pool.take(3).unwrap().unwrap();
		assert_eq!(all.count(), 3);
		pool.give(all).unwrap();
		assert_eq!(pool.outstanding(), 0);
		pool.close().unwrap();
	}

	#[test]
	fn a_request_outside_capacity_is_refused() {
		let (_dir, pool) = pool(2);
		assert!(pool.take(3).is_err());
		assert!(pool.take(0).is_err());
		pool.close().unwrap();
	}

	#[test]
	fn a_pool_needs_one_token() {
		let dir = Dir::new();
		assert!(Pool::create(&dir.0.join("fifo"), 0).is_err());
	}

	#[test]
	fn a_dropped_lease_returns_its_tokens() {
		let (_dir, pool) = pool(2);
		{
			let _lease = pool.take(2).unwrap().unwrap();
			assert_eq!(available(&pool), 0);
		}
		assert_eq!((available(&pool), pool.outstanding()), (2, 0));
		pool.close().unwrap();
	}

	#[test]
	fn a_lease_from_another_pool_is_refused() {
		let (_a, first) = pool(1);
		let (_b, second) = pool(1);
		let lease = first.take(1).unwrap().unwrap();
		assert!(second
			.give(lease)
			.unwrap_err()
			.to_string()
			.contains("another jobserver"));
		// The refused lease was dropped, so it went back to its own pool.
		assert_eq!((available(&first), available(&second)), (1, 1));
		first.close().unwrap();
		second.close().unwrap();
	}

	#[test]
	fn a_lease_outliving_close_still_returns_and_is_reported() {
		let (dir, pool) = pool(2);
		let lease = pool.take(1).unwrap().unwrap();
		let state = pool.state.clone();
		assert!(pool
			.close()
			.unwrap_err()
			.to_string()
			.contains("1 jobserver tokens were still leased"));
		assert!(!dir.0.join("jobserver.fifo").exists());
		drop(lease);
		assert_eq!(
			(
				*state.outstanding.lock().unwrap(),
				state.lost.lock().unwrap().len()
			),
			(0, 0)
		);
	}

	#[test]
	fn a_return_to_a_full_fifo_is_lost_after_its_deadline_and_reported() {
		let (_dir, mut pool) = pool(1);
		Arc::get_mut(&mut pool.state).unwrap().return_deadline =
			std::time::Duration::from_millis(100);
		let lease = pool.take(1).unwrap().unwrap();
		let chunk = vec![b'+'; 4096];
		while (&pool.state.fifo).write(&chunk).is_ok() {}
		while (&pool.state.fifo).write(b"+").is_ok() {}
		let started = std::time::Instant::now();
		let error = pool.give(lease).unwrap_err();
		assert!(
			started.elapsed() < std::time::Duration::from_secs(5),
			"the return blocked past its deadline"
		);
		assert!(format!("{error:#}").contains("stayed full"), "{error:#}");
		assert!(pool
			.close()
			.unwrap_err()
			.to_string()
			.contains("lost tokens"));
	}

	#[test]
	fn a_full_fifo_waits_rather_than_dropping_bytes() {
		let (_dir, pool) = pool(1);
		// Fill the fifo to its limit, then free room from another thread.
		let mut filled = 0;
		let chunk = vec![b'+'; 4096];
		loop {
			match (&pool.state.fifo).write(&chunk) {
				Ok(n) => filled += n,
				Err(error) if error.kind() == ErrorKind::WouldBlock => break,
				Err(error) => panic!("{error}"),
			}
		}
		let state = pool.state.clone();
		let drain = std::thread::spawn(move || {
			std::thread::sleep(std::time::Duration::from_millis(50));
			let mut buffer = vec![0; 8192];
			(&state.fifo).read(&mut buffer).unwrap()
		});
		pool.state.write(b"++").unwrap();
		let drained = drain.join().unwrap();
		let mut rest = 0;
		let mut buffer = vec![0; 65536];
		while let Ok(n) = (&pool.state.fifo).read(&mut buffer) {
			if n == 0 {
				break;
			}
			rest += n;
		}
		assert_eq!(
			drained + rest,
			1 + filled + 2,
			"every byte written arrived once"
		);
		pool.close().unwrap();
	}

	#[test]
	fn the_flags_name_the_fifo_and_its_width() {
		let (_dir, pool) = pool(5);
		let flags = pool.flags();
		assert_eq!(
			flags,
			format!("-j5 --jobserver-auth=fifo:{}", pool.path().display())
		);
		assert_eq!(inherited_width(&flags), Some(5));
		assert_eq!(
			inherited_width("-j5"),
			None,
			"a width without the final's fifo is not inherited"
		);
		pool.close().unwrap();
	}

	#[test]
	fn a_jobserver_client_reads_the_same_tokens() {
		let (_dir, pool) = pool(2);
		let held = pool.take(1).unwrap().unwrap();
		// No other test reads CARGO_MAKEFLAGS.
		unsafe { std::env::set_var("CARGO_MAKEFLAGS", pool.flags()) };
		// SAFETY: the flags name only this test's fifo.
		let client = unsafe { jobserver::Client::from_env() }.expect("fifo client");
		let token = client.acquire().unwrap();
		assert!(
			pool.take(1).unwrap().is_none(),
			"the client holds the last token"
		);
		drop(token);
		pool.give(pool.take(1).unwrap().expect("the client's token came back"))
			.unwrap();
		pool.give(held).unwrap();
		pool.close().unwrap();
	}
}
