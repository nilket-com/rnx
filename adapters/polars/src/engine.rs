//! Joined plain engine threads; observation counters exist only in test-support builds.
use std::cell::Cell;

/// A callback failure can cross infallible Polars APIs as a typed panic
/// payload. Only the outer engine boundary converts that payload to an error.
#[derive(Debug, Clone)]
pub(crate) struct CallbackFailure {
	pub op: String,
	pub cause: String,
}
impl CallbackFailure {
	pub fn text(&self) -> String { format!("callback {}: {}", self.op, self.cause) }
}

#[derive(Debug)]
pub(crate) enum EngineFailure {
	NoThread(String),
	Callback(String),
	Reentry(String),
}
impl std::fmt::Display for EngineFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self { Self::NoThread(s) | Self::Callback(s) | Self::Reentry(s) => f.write_str(s) }
	}
}

thread_local! { static IN_CALLBACK: Cell<bool> = const { Cell::new(false) }; }
pub(crate) fn in_callback() -> bool { IN_CALLBACK.with(Cell::get) }
pub(crate) struct CallbackGuard;
impl CallbackGuard {
	pub(crate) fn enter() -> Self { IN_CALLBACK.with(|c| c.set(true)); Self }
}
impl Drop for CallbackGuard {
	fn drop(&mut self) { IN_CALLBACK.with(|c| c.set(false)); }
}

/// Preserve the original no-thread panic for an infallible binding, and
/// propagate a re-entry refusal through the callback's typed unwind.
pub(crate) fn infallible<T>(result: Result<T, EngineFailure>, binding: &str) -> T {
	match result {
		Ok(value) => value,
		Err(EngineFailure::Reentry(_)) => std::panic::resume_unwind(Box::new(CallbackFailure {
			op: binding.into(), cause: "routed binding called from a callback".into(),
		})),
		Err(e) => panic!("polars engine thread: {e}"),
	}
}
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
pub(crate) fn run<T: Send>(binding: &str, call: impl FnOnce() -> T + Send) -> Result<T, EngineFailure> {
	if in_callback() {
		return Err(EngineFailure::Reentry(format!("callback {binding}: routed binding called from a callback")));
	}
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
			.map_err(|e| EngineFailure::NoThread(format!("cannot start Polars engine thread: {e}")))?;
		let result = worker.join();
		#[cfg(feature = "test-support")]
		JOINED.fetch_add(1, Ordering::SeqCst);
		match result {
			Ok(value) => Ok(value),
			Err(panic) => match panic.downcast::<CallbackFailure>() {
				Ok(failure) => Err(EngineFailure::Callback(failure.text())),
				Err(other) => std::panic::resume_unwind(other),
			},
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
		let before = counts();
		let panic = std::panic::catch_unwind(|| run("test", || panic!("engine-panic-marker")));
		assert!(panic.is_err());
		let after = counts();
		assert!(after.0 > before.0);
		assert!(after.1 > before.1);
		assert!(after.2 > before.2);
		assert!(after.5 > before.5);
	}
}
