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

	/// The tokens in the fifo now, counted without reading any (`FIONREAD`):
	/// a count short of a stage's weight refuses it with nothing read and
	/// nothing written back.
	pub fn available(&self) -> Result<usize> {
		let mut count: libc::c_int = 0;
		// SAFETY: FIONREAD writes one int for a valid pipe descriptor.
		if unsafe { libc::ioctl(self.state.fifo.as_raw_fd(), libc::FIONREAD, &mut count) } != 0 {
			return Err(std::io::Error::last_os_error()).context("count jobserver tokens");
		}
		Ok(count.max(0) as usize)
	}

	/// A watch on the fifo for writes by any process; see [`Watch`].
	pub fn watch(&self) -> Result<Watch> {
		Watch::new(&self.path)
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

/// A stage's side of a final's jobserver, joined through the `MAKEFLAGS`
/// the controller set: a permit client, never a second pool. The tokens
/// it reads belong to the controller's [`Pool`]; every permit puts each
/// token back where it came from.
pub struct Client {
	path: PathBuf,
	capacity: usize,
	held: usize,
}

/// The tokens the controller already holds for this stage, idle while none
/// of its own builds run: a nonblocking pipe of one byte per token.
pub struct Slots {
	read: File,
	write: File,
}

impl Slots {
	pub fn new(count: usize) -> Result<Self> {
		let mut fds = [0; 2];
		// SAFETY: two valid out-pointers for pipe2.
		if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
			return Err(std::io::Error::last_os_error()).context("pipe2 for the stage slots");
		}
		use std::os::fd::FromRawFd as _;
		// SAFETY: pipe2 returned two fresh descriptors this value now owns.
		let (read, write) = unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) };
		(&write)
			.write_all(&vec![b'+'; count])
			.context("fill the stage slots")?;
		Ok(Self { read, write })
	}
}

/// Tokens a [`Client`] holds: some from the stage's own slots, the rest
/// from the pool. Dropping a permit returns each to its source, so an
/// error, a panic or a cancelled case loses nothing.
#[must_use = "a permit returns its tokens when dropped"]
pub struct Permit<'a> {
	pool: Option<File>,
	slots: Option<&'a Slots>,
	own: Vec<u8>,
	pooled: Vec<u8>,
}

impl Permit<'_> {
	pub fn count(&self) -> usize {
		self.own.len() + self.pooled.len()
	}

	/// The tokens from the stage's own slots, and from the pool.
	pub fn sources(&self) -> (usize, usize) {
		(self.own.len(), self.pooled.len())
	}

	/// Return the tokens now, reporting a failed return.
	pub fn release(mut self) -> Result<()> {
		self.give()
	}

	fn give(&mut self) -> Result<()> {
		let deadline = std::time::Instant::now() + RETURN_DEADLINE;
		let mut result = Ok(());
		if !self.own.is_empty() {
			let slots = self.slots.expect("own tokens come from slots");
			if let Err(error) = write_within(&slots.write, &std::mem::take(&mut self.own), deadline)
			{
				result = Err(error).context("return tokens to the stage slots");
			}
		}
		if !self.pooled.is_empty() {
			let pool = self
				.pool
				.as_ref()
				.expect("pooled tokens come from the pool");
			if let Err(error) = write_within(pool, &std::mem::take(&mut self.pooled), deadline) {
				result = Err(error).context("return tokens to the jobserver");
			}
		}
		result
	}
}

impl Drop for Permit<'_> {
	fn drop(&mut self) {
		let _ = self.give();
	}
}

impl Client {
	/// The client a stage's environment names, or none outside a final:
	/// `MAKEFLAGS` gives the fifo and its width, `RNX_FINAL_STAGE_TOKENS`
	/// the tokens the controller holds for this stage (one by default).
	pub fn from_environment(
		makeflags: Option<&str>,
		stage_tokens: Option<&str>,
	) -> Result<Option<Self>> {
		let flags = makeflags.unwrap_or_default();
		let path = flags
			.split_whitespace()
			.find_map(|f| f.strip_prefix("--jobserver-auth=fifo:"));
		let width = flags
			.split_whitespace()
			.find_map(|f| f.strip_prefix("-j"))
			.and_then(|w| w.parse::<usize>().ok());
		let (Some(path), Some(capacity)) = (path, width) else {
			return Ok(None);
		};
		let held = match stage_tokens {
			None => 1,
			Some(text) => text
				.parse()
				.with_context(|| format!("RNX_FINAL_STAGE_TOKENS is not a count: {text}"))?,
		};
		ensure!(
			held <= capacity,
			"the stage holds {held} of {capacity} tokens"
		);
		Ok(Some(Self {
			path: PathBuf::from(path),
			capacity,
			held,
		}))
	}

	pub fn capacity(&self) -> usize {
		self.capacity
	}

	pub fn held(&self) -> usize {
		self.held
	}

	fn open(&self) -> Result<File> {
		OpenOptions::new()
			.read(true)
			.write(true)
			.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
			.open(&self.path)
			.with_context(|| format!("open {}", self.path.display()))
	}

	/// `count` tokens from the pool, waiting until all are read; the part
	/// already read goes back if the wait fails.
	pub fn acquire(&self, count: usize) -> Result<Permit<'static>> {
		let mut permit = Permit {
			pool: Some(self.open()?),
			slots: None,
			own: Vec::new(),
			pooled: Vec::with_capacity(count),
		};
		while permit.pooled.len() < count {
			let pool = permit.pool.as_ref().unwrap();
			ready(&[pool])?;
			let mut buffer = vec![0; count - permit.pooled.len()];
			match (&*pool).read(&mut buffer) {
				Ok(0) => bail!("the jobserver closed"),
				Ok(n) => permit.pooled.extend_from_slice(&buffer[..n]),
				Err(error)
					if matches!(error.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) => {}
				Err(error) => return Err(error).context("read jobserver tokens"),
			}
		}
		Ok(permit)
	}

	/// One token from the stage's own slots or from the pool, whichever
	/// frees first, so a one-token pool still runs every case on the slot
	/// the controller holds for the stage.
	pub fn acquire_one<'a>(&self, slots: &'a Slots) -> Result<Permit<'a>> {
		let pool = self.open()?;
		loop {
			let which = ready(&[&slots.read, &pool])?;
			let mut token = [0u8; 1];
			let source = if which == 0 { &slots.read } else { &pool };
			match (&*source).read(&mut token) {
				Ok(1) => {
					let own = which == 0;
					return Ok(Permit {
						pool: (!own).then_some(pool),
						slots: own.then_some(slots),
						own: if own { token.to_vec() } else { Vec::new() },
						pooled: if own { Vec::new() } else { token.to_vec() },
					});
				}
				Ok(_) => bail!("a jobserver source closed"),
				Err(error)
					if matches!(error.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) => {}
				Err(error) => return Err(error).context("read a jobserver token"),
			}
		}
	}
}

/// The index of the first of `files` readable, after blocking until one is.
fn ready(files: &[&File]) -> Result<usize> {
	let mut fds: Vec<libc::pollfd> = files
		.iter()
		.map(|f| libc::pollfd {
			fd: f.as_raw_fd(),
			events: libc::POLLIN,
			revents: 0,
		})
		.collect();
	loop {
		// SAFETY: valid pollfds for the duration of the call; no timeout.
		if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) } < 0 {
			let error = std::io::Error::last_os_error();
			if error.kind() == ErrorKind::Interrupted {
				continue;
			}
			return Err(error).context("poll the jobserver");
		}
		if let Some(i) = fds
			.iter()
			.position(|p| p.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0)
		{
			return Ok(i);
		}
	}
}

/// Every byte written to `file`, across partial writes, EINTR and a full
/// pipe, before `deadline`.
fn write_within(
	file: &File,
	mut bytes: &[u8],
	deadline: std::time::Instant,
) -> std::io::Result<()> {
	while !bytes.is_empty() {
		match (&*file).write(bytes) {
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
						"the jobserver stayed full",
					));
				}
				let mut wait = libc::pollfd {
					fd: file.as_raw_fd(),
					events: libc::POLLOUT,
					revents: 0,
				};
				// SAFETY: one valid pollfd for the duration of the call.
				unsafe {
					libc::poll(
						&mut wait,
						1,
						left.as_millis().clamp(1, i32::MAX as u128) as i32,
					)
				};
			}
			Err(error) => return Err(error),
		}
	}
	Ok(())
}

/// What a [`Watch`] saw.
#[derive(Debug, PartialEq, Eq)]
pub enum Notice {
	/// Something wrote to the fifo, or the queue overflowed: capacity may
	/// have changed. Notices coalesce; one says only "recheck".
	Modified,
	/// The watch was asked to stop.
	Stopped,
}

/// An inotify watch for `IN_MODIFY` on a jobserver fifo, with a stop pipe.
/// Every write to the fifo, by this process, a stage's client, or Cargo's
/// and rustc's raw returns, is notified; reads are not.
pub struct Watch {
	inotify: File,
	stop_read: File,
	stop_write: File,
}

/// A handle that stops a [`Watch`] waiting on another thread.
pub struct StopWatch(File);

impl StopWatch {
	pub fn stop(&self) -> Result<()> {
		(&self.0)
			.write_all(b"!")
			.context("stop the jobserver watch")
	}
}

impl Watch {
	pub fn new(path: &Path) -> Result<Self> {
		use std::os::fd::FromRawFd as _;
		// SAFETY: flags only; a new descriptor or -1.
		let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
		if fd < 0 {
			return Err(std::io::Error::last_os_error()).context("inotify_init1 for the jobserver");
		}
		// SAFETY: a fresh descriptor this value now owns.
		let inotify = unsafe { File::from_raw_fd(fd) };
		let name = CString::new(path.as_os_str().as_bytes()).context("fifo path has a NUL byte")?;
		// SAFETY: a valid inotify descriptor and NUL-terminated path.
		if unsafe { libc::inotify_add_watch(fd, name.as_ptr(), libc::IN_MODIFY) } < 0 {
			return Err(std::io::Error::last_os_error())
				.with_context(|| format!("watch {}", path.display()));
		}
		let mut pipe = [0; 2];
		// SAFETY: two valid out-pointers for pipe2.
		if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
			return Err(std::io::Error::last_os_error()).context("pipe2 for the jobserver watch");
		}
		// SAFETY: pipe2 returned two fresh descriptors this value now owns.
		let (stop_read, stop_write) =
			unsafe { (File::from_raw_fd(pipe[0]), File::from_raw_fd(pipe[1])) };
		Ok(Self {
			inotify,
			stop_read,
			stop_write,
		})
	}

	pub fn stopper(&self) -> Result<StopWatch> {
		Ok(StopWatch(self.stop_write.try_clone()?))
	}

	/// Block until the fifo is written or the watch is stopped. A stop
	/// takes priority whenever it is readable, and each poll is followed by
	/// at most one bounded batch, so a writer that never pauses cannot
	/// postpone a stop. A notice covers every write read so far; a write
	/// still queued raises the next one.
	pub fn wait(&self) -> Result<Notice> {
		let mut buffer = vec![0u8; 64 * 1024];
		loop {
			let mut fds = [
				libc::pollfd {
					fd: self.inotify.as_raw_fd(),
					events: libc::POLLIN,
					revents: 0,
				},
				libc::pollfd {
					fd: self.stop_read.as_raw_fd(),
					events: libc::POLLIN,
					revents: 0,
				},
			];
			// SAFETY: two valid pollfds for the duration of the call; no timeout.
			if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
				let error = std::io::Error::last_os_error();
				if error.kind() == ErrorKind::Interrupted {
					continue;
				}
				return Err(error).context("poll the jobserver watch");
			}
			let readable = libc::POLLIN | libc::POLLHUP | libc::POLLERR;
			if fds[1].revents & readable != 0 {
				return Ok(Notice::Stopped);
			}
			if fds[0].revents & readable == 0 {
				continue;
			}
			match (&self.inotify).read(&mut buffer) {
				Ok(0) => bail!("the jobserver watch closed"),
				// A batch with no write in it says nothing; keep waiting.
				Ok(n) => {
					if classify(&buffer[..n])? {
						return Ok(Notice::Modified);
					}
				}
				Err(error)
					if matches!(error.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) => {}
				Err(error) => return Err(error).context("read the jobserver watch"),
			}
		}
	}
}

/// Whether a batch of inotify records says capacity may have changed: a
/// write, or a full queue. A removed or invalidated watch is an error, so
/// the controller never sleeps on a dead source.
pub fn classify(batch: &[u8]) -> Result<bool> {
	const HEADER: usize = std::mem::size_of::<libc::inotify_event>();
	let mut modified = false;
	let mut at = 0;
	while at < batch.len() {
		ensure!(
			batch.len() - at >= HEADER,
			"a truncated jobserver watch record"
		);
		let mask = u32::from_ne_bytes(batch[at + 4..at + 8].try_into().unwrap());
		let len = u32::from_ne_bytes(batch[at + 12..at + 16].try_into().unwrap()) as usize;
		if mask & (libc::IN_IGNORED | libc::IN_DELETE_SELF | libc::IN_UNMOUNT) != 0 {
			bail!("the jobserver watch was removed (mask {mask:#x})");
		}
		if mask & (libc::IN_MODIFY | libc::IN_Q_OVERFLOW) != 0 {
			modified = true;
		}
		// The declared name must be in the batch too.
		at = at
			.checked_add(HEADER)
			.and_then(|e| e.checked_add(len))
			.filter(|e| *e <= batch.len())
			.context("a truncated jobserver watch record")?;
	}
	Ok(modified)
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

	fn client(pool: &Pool, held: usize) -> Client {
		Client::from_environment(Some(&pool.flags()), Some(&held.to_string()))
			.unwrap()
			.unwrap()
	}

	fn slots_left(slots: &Slots) -> usize {
		let mut n = 0;
		let mut byte = [0u8; 1];
		while let Ok(1) = (&slots.read).read(&mut byte) {
			n += 1;
		}
		(&slots.write).write_all(&vec![b'+'; n]).unwrap();
		n
	}

	#[test]
	fn a_client_is_found_only_inside_a_final() {
		let (_dir, pool) = pool(3);
		let c = client(&pool, 2);
		assert_eq!((c.capacity(), c.held()), (3, 2));
		assert!(Client::from_environment(Some("-j3"), None)
			.unwrap()
			.is_none());
		assert!(Client::from_environment(None, None).unwrap().is_none());
		assert!(Client::from_environment(Some(&pool.flags()), Some("x")).is_err());
		assert!(Client::from_environment(Some(&pool.flags()), Some("4")).is_err());
		assert_eq!(client(&pool, 1).held(), 1);
		pool.close().unwrap();
	}

	/// Own tokens go back to the slots, pool tokens to the fifo; on a
	/// release, a drop and a panic alike.
	#[test]
	fn every_token_returns_where_it_came_from() {
		let (_dir, pool) = pool(2);
		let c = client(&pool, 1);
		let slots = Slots::new(1).unwrap();
		let first = c.acquire_one(&slots).unwrap();
		let second = c.acquire_one(&slots).unwrap();
		let mut sources = [first.sources(), second.sources()];
		sources.sort();
		assert_eq!(sources, [(0, 1), (1, 0)]);
		assert_eq!((available(&pool), slots_left(&slots)), (1, 0));
		first.release().unwrap();
		drop(second);
		assert_eq!((available(&pool), slots_left(&slots)), (2, 1));
		let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let _held = c.acquire(2).unwrap();
			assert_eq!(available(&pool), 0);
			panic!("a failed case");
		}));
		assert!(taken.is_err());
		assert_eq!(available(&pool), 2);
		pool.close().unwrap();
	}

	/// With every pool token held elsewhere, each case still runs on the
	/// stage's own slot.
	#[test]
	fn a_one_token_pool_runs_every_case_on_the_stage_slot() {
		let (_dir, pool) = pool(1);
		let held = pool.take(1).unwrap().unwrap();
		let c = client(&pool, 1);
		let slots = Slots::new(1).unwrap();
		for _ in 0..3 {
			let permit = c.acquire_one(&slots).unwrap();
			assert_eq!(permit.sources(), (1, 0));
		}
		pool.give(held).unwrap();
		pool.close().unwrap();
	}

	/// An exclusive acquisition waits for every token it asks for, and
	/// holds them until it returns them.
	#[test]
	fn an_exclusive_acquisition_waits_for_every_token() {
		let (_dir, pool) = pool(3);
		let c = client(&pool, 1);
		let lease = pool.take(1).unwrap().unwrap();
		let (sender, receiver) = std::sync::mpsc::channel();
		std::thread::scope(|s| {
			s.spawn(|| {
				let permit = c.acquire(3).unwrap();
				sender.send(permit.count()).unwrap();
				drop(permit);
			});
			// Two of three are free: the acquisition cannot complete.
			assert!(receiver
				.recv_timeout(std::time::Duration::from_millis(200))
				.is_err());
			pool.give(lease).unwrap();
			assert_eq!(receiver.recv().unwrap(), 3);
		});
		assert_eq!(available(&pool), 3);
		pool.close().unwrap();
	}

	fn record(mask: u32) -> Vec<u8> {
		let mut r = Vec::new();
		r.extend_from_slice(&1i32.to_ne_bytes());
		r.extend_from_slice(&mask.to_ne_bytes());
		r.extend_from_slice(&0u32.to_ne_bytes());
		r.extend_from_slice(&0u32.to_ne_bytes());
		r
	}

	#[test]
	fn a_batch_rechecks_on_a_write_or_an_overflow_and_fails_on_removal() {
		assert!(classify(&record(libc::IN_MODIFY)).unwrap());
		assert!(classify(&record(libc::IN_Q_OVERFLOW)).unwrap());
		let mut two = record(libc::IN_MODIFY);
		two.extend(record(libc::IN_MODIFY));
		assert!(classify(&two).unwrap());
		assert!(!classify(&record(libc::IN_ACCESS)).unwrap());
		for removed in [libc::IN_IGNORED, libc::IN_DELETE_SELF, libc::IN_UNMOUNT] {
			let mut batch = record(libc::IN_MODIFY);
			batch.extend(record(removed));
			assert!(classify(&batch).is_err(), "{removed:#x}");
		}
		assert!(classify(&record(libc::IN_MODIFY)[..10]).is_err());
		// A header declaring a name the batch does not hold.
		let mut named = record(libc::IN_MODIFY);
		named[12..16].copy_from_slice(&16u32.to_ne_bytes());
		assert!(classify(&named).is_err());
		named.extend([0u8; 16]);
		assert!(classify(&named).unwrap());
		let mut huge = record(libc::IN_MODIFY);
		huge[12..16].copy_from_slice(&u32::MAX.to_ne_bytes());
		assert!(classify(&huge).is_err());
	}

	/// With a write and a stop both pending, the stop wins; and a watch
	/// stopped while a writer keeps writing ends.
	#[test]
	fn a_stop_takes_priority_over_writes() {
		let (_dir, pool) = pool(1);
		let watch = pool.watch().unwrap();
		pool.state.write(b"+").unwrap();
		watch.stopper().unwrap().stop().unwrap();
		assert_eq!(watch.wait().unwrap(), Notice::Stopped);
		let watch = pool.watch().unwrap();
		let stop = watch.stopper().unwrap();
		let writing = std::sync::atomic::AtomicBool::new(true);
		let (first, seen) = std::sync::mpsc::channel();
		let (ended, done) = std::sync::mpsc::channel();
		let path = pool.path().to_owned();
		std::thread::scope(|s| {
			s.spawn(|| {
				let mut fifo = OpenOptions::new().write(true).open(&path).unwrap();
				while writing.load(std::sync::atomic::Ordering::SeqCst) {
					fifo.write_all(b"+").unwrap();
					let mut back = [0u8; 1];
					let _ = (&pool.state.fifo).read(&mut back);
				}
			});
			s.spawn(|| {
				let mut notices = 0;
				while watch.wait().unwrap() == Notice::Modified {
					notices += 1;
					if notices == 1 {
						first.send(()).unwrap();
					}
				}
				ended.send(notices).unwrap();
			});
			// Stop once the writer is seen running, while it still writes.
			let bound = std::time::Duration::from_secs(20);
			let outcome = seen
				.recv_timeout(bound)
				.map_err(|_| "no first notice")
				.and_then(|()| {
					stop.stop().unwrap();
					done.recv_timeout(bound)
						.map_err(|_| "the stop was starved by writes")
				});
			// Every path ends the writer and asks the watcher to stop before
			// the scope joins them; the outcome is checked after.
			writing.store(false, std::sync::atomic::Ordering::SeqCst);
			let _ = stop.stop();
			outcome
		})
		.map(|notices| assert!(notices >= 1))
		.unwrap();
		pool.close().unwrap();
	}

	/// Own and other processes' writes are notified, coalesced into one
	/// notice when they come before the wait; reads are not; the count is
	/// read without consuming; stop ends a wait.
	#[test]
	fn a_watch_sees_writes_by_any_process_and_not_reads() {
		let (_dir, pool) = pool(2);
		let watch = pool.watch().unwrap();
		assert_eq!(pool.available().unwrap(), 2);
		let lease = pool.take(2).unwrap().unwrap();
		assert_eq!(pool.available().unwrap(), 0);
		pool.give(lease).unwrap();
		for _ in 0..3 {
			let status = std::process::Command::new("sh")
				.args(["-c", &format!("printf + > {}", pool.path().display())])
				.status()
				.unwrap();
			assert!(status.success());
		}
		assert_eq!(pool.available().unwrap(), 5);
		// The give and three raw writes: one notice for them all.
		assert_eq!(watch.wait().unwrap(), Notice::Modified);
		// A raw read, as a client takes a token, raises nothing: the stop
		// is the next notice.
		let mut other = OpenOptions::new()
			.read(true)
			.write(true)
			.custom_flags(libc::O_NONBLOCK)
			.open(pool.path())
			.unwrap();
		let mut token = [0u8; 1];
		other.read_exact(&mut token).unwrap();
		assert_eq!(pool.available().unwrap(), 4);
		watch.stopper().unwrap().stop().unwrap();
		assert_eq!(watch.wait().unwrap(), Notice::Stopped);
		pool.close().unwrap();
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
