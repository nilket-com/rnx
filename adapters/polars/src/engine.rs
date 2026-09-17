//! Joined plain engine threads; observation counters exist only in test-support builds.
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "test-support")]
static STARTED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-support")]
static FINISHED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-support")]
static JOINED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-support")]
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-support")]
static MAX_ACTIVE: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-support")]
static NO_CONTEXT: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-support")]
struct Finished;
#[cfg(feature = "test-support")]
impl Drop for Finished {
	fn drop(&mut self) {
		ACTIVE.fetch_sub(1, Ordering::SeqCst);
		FINISHED.fetch_add(1, Ordering::SeqCst);
	}
}
pub(crate) fn run<T: Send>(call: impl FnOnce() -> T + Send) -> Result<T, String> {
	std::thread::scope(|scope| {
		let worker = std::thread::Builder::new()
			.name("rnx-polars-engine".into())
			.spawn_scoped(scope, move || {
				#[cfg(feature = "test-support")]
				let _finished = {
					STARTED.fetch_add(1, Ordering::SeqCst);
					let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
					MAX_ACTIVE.fetch_max(active, Ordering::SeqCst);
					let guard = Finished;
					assert!(tokio::runtime::Handle::try_current().is_err());
					NO_CONTEXT.fetch_add(1, Ordering::SeqCst);
					guard
				};
				call()
			})
			.map_err(|e| format!("cannot start Polars engine thread: {e}"))?;
		let result = worker.join();
		#[cfg(feature = "test-support")]
		JOINED.fetch_add(1, Ordering::SeqCst);
		match result {
			Ok(value) => Ok(value),
			Err(panic) => std::panic::resume_unwind(panic),
		}
	})
}
#[cfg(feature = "test-support")]
pub(crate) fn counts() -> (usize, usize, usize, usize, usize, usize) {
	(
		STARTED.load(Ordering::SeqCst),
		FINISHED.load(Ordering::SeqCst),
		JOINED.load(Ordering::SeqCst),
		ACTIVE.load(Ordering::SeqCst),
		MAX_ACTIVE.load(Ordering::SeqCst),
		NO_CONTEXT.load(Ordering::SeqCst),
	)
}
#[cfg(all(test, feature = "test-support"))]
mod tests {
	use super::*;
	#[test]
	fn panic_is_joined_before_resuming_on_the_caller() {
		let panic = std::panic::catch_unwind(|| run(|| panic!("engine-panic-marker")));
		assert!(panic.is_err());
		let (started, finished, joined, active, _, no_context) = counts();
		assert!(started >= 1);
		assert_eq!(
			(started, finished, joined, active, no_context),
			(started, started, started, 0, started)
		);
	}
}
