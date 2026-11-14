//! An owned, invocation-scoped loan of the session's existing runtime.
use super::*;
use std::cell::RefCell;

thread_local! {
	static RUNTIME: RefCell<Option<tokio::runtime::Runtime>> = const { RefCell::new(None) };
	static POLLING: Cell<bool> = const { Cell::new(false) };
	static CANCELLED: Cell<bool> = const { Cell::new(false) };
}

struct Loan<'a>(&'a mut Option<tokio::runtime::Runtime>);
impl Drop for Loan<'_> {
	fn drop(&mut self) {
		CANCELLED.with(|v| v.set(false));
		// No reference to the slot escapes with_native_wait. Its Ref and polling
		// guard unwind before this outer loan, including a native panic.
		RUNTIME.with(|slot| {
			if let Ok(mut slot) = slot.try_borrow_mut() {
				*self.0 = slot.take();
			}
		});
	}
}

impl Runtime {
	pub(crate) fn with_host_wait<T>(&mut self, run: impl FnOnce() -> T) -> Result<T, String> {
		if tokio::runtime::Handle::try_current().is_ok() {
			return Err("cannot enter a session host wait from an active runtime context".into());
		}
		let owned = RUNTIME.with(|slot| -> Result<_, String> {
			let mut slot = slot
				.try_borrow_mut()
				.map_err(|_| "session host wait is reentrant")?;
			if slot.is_some() {
				return Err("session host wait is already active".into());
			}
			let runtime = self.0.take().ok_or("session runtime is unavailable")?;
			let handle = runtime.handle().clone();
			*slot = Some(runtime);
			Ok(handle)
		})?;
		CANCELLED.with(|v| v.set(false));
		let _loan = Loan(&mut self.0);
		// enter supplies the SAME context to other synchronous handlers without
		// running the scheduler. block_on below is legal under this enter guard.
		let _entered = owned.enter();
		Ok(run())
	}
}

struct Polling;
impl Drop for Polling {
	fn drop(&mut self) {
		POLLING.with(|p| p.set(false));
	}
}
fn failed(message: &'static str) -> VmError {
	VmError::panic(message)
}

/// The native future owns its complete input and cannot call back into script.
/// VmError is terminal; an interrupted wait is never a catchable script Err.
pub(crate) fn with_native_wait<F: Future>(future: F) -> Result<F::Output, VmError> {
	native_wait(future, crate::host::interrupted)
}

fn native_wait<F: Future>(
	future: F,
	mut interrupted: impl FnMut() -> bool,
) -> Result<F::Output, VmError> {
	if POLLING.with(|p| p.replace(true)) {
		return Err(failed("reentrant native wait"));
	}
	let _polling = Polling;
	RUNTIME.with(|slot| {
		let slot = slot
			.try_borrow()
			.map_err(|_| failed("session host wait is reentrant"))?;
		let runtime = slot
			.as_ref()
			.ok_or_else(|| failed("no session runtime for awaited host call"))?;
		if interrupted() {
			CANCELLED.with(|v| v.set(true));
			return Err(failed("host wait interrupted"));
		}
		let value = runtime.block_on(async {
			Watched {
				work: Box::pin(future),
				interrupt: Interrupt::new(),
			}
			.await
		});
		match value {
			Some(value) if !interrupted() => Ok(value),
			_ => {
				CANCELLED.with(|v| v.set(true));
				Err(failed("host wait interrupted"))
			}
		}
	})
}

// Structured invocation-local cancellation: a concurrent flag must not replace
// an unrelated script error, and no diagnostic prose is interpreted.
pub(crate) fn host_wait_interrupted() -> bool {
	CANCELLED.with(Cell::get)
}

/// Preserve a normal Rust Result as the script value; conversion failure is fatal.
pub(crate) fn result_value<T: rune::ToValue>(value: T) -> Result<Value, VmError> {
	rune::to_value(value).map_err(VmError::from)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	};
	#[test]
	fn entered_context_permits_wait_and_drives_spawned_work() {
		crate::host::clear_interrupt();
		let mut runtime = Runtime::new().unwrap();
		let ran = Arc::new(AtomicUsize::new(0));
		let task = ran.clone();
		runtime
			.with_host_wait(|| {
				assert!(tokio::runtime::Handle::try_current().is_ok());
				tokio::spawn(async move {
					task.store(1, Ordering::SeqCst);
				});
				assert_eq!(
					with_native_wait(async {
						tokio::time::sleep(Duration::from_millis(1)).await;
						7
					})
					.unwrap(),
					7
				);
			})
			.unwrap();
		assert_eq!(ran.load(Ordering::SeqCst), 1);
		assert!(tokio::runtime::Handle::try_current().is_err());
		runtime.run_sync(|| assert!(tokio::runtime::Handle::try_current().is_ok()));
		runtime.drain_shutdown().unwrap();
	}
	#[test]
	fn interrupt_between_completion_and_return_is_terminal() {
		let mut runtime = Runtime::new().unwrap();
		let checked = Cell::new(false);
		let result = runtime
			.with_host_wait(|| native_wait(async { 7 }, || checked.replace(true)))
			.unwrap();
		assert!(result.is_err());
		runtime
			.with_host_wait(|| assert_eq!(with_native_wait(async { 9 }).unwrap(), 9))
			.unwrap();
		runtime.drain_shutdown().unwrap();
	}
	#[test]
	fn two_sessions_do_not_drive_each_others_runtime_tasks() {
		crate::host::clear_interrupt();
		let mut first = Runtime::new().unwrap();
		let mut second = Runtime::new().unwrap();
		let count = Arc::new(AtomicUsize::new(0));
		let copy = count.clone();
		first
			.with_host_wait(|| {
				tokio::spawn(async move {
					copy.store(1, Ordering::SeqCst);
				});
				assert!(second.with_host_wait(|| ()).is_err());
			})
			.unwrap();
		second
			.with_host_wait(|| {
				with_native_wait(async { tokio::time::sleep(Duration::from_millis(1)).await })
			})
			.unwrap()
			.unwrap();
		assert_eq!(count.load(Ordering::SeqCst), 0);
		first
			.with_host_wait(|| {
				with_native_wait(async { tokio::time::sleep(Duration::from_millis(1)).await })
			})
			.unwrap()
			.unwrap();
		assert_eq!(count.load(Ordering::SeqCst), 1);
		first.drain_shutdown().unwrap();
		second.drain_shutdown().unwrap();
	}
	#[test]
	fn loan_restores_after_return_error_and_unwind_and_separates_sessions() {
		let mut first = Runtime::new().unwrap();
		let mut second = Runtime::new().unwrap();
		assert!(with_native_wait(async { 0 }).is_err());
		for result in [Ok(1), Err("script error")] {
			assert_eq!(first.with_host_wait(|| result).unwrap(), result);
			first.run_sync(|| {});
		}
		assert!(
			std::panic::catch_unwind(std::panic::AssertUnwindSafe(
				|| first.with_host_wait(|| panic!("native panic"))
			))
			.is_err()
		);
		first.run_sync(|| {});
		assert!(with_native_wait(async { 0 }).is_err());
		second
			.with_host_wait(|| assert_eq!(with_native_wait(async { 9 }).unwrap(), 9))
			.unwrap();
		first
			.with_host_wait(|| assert_eq!(with_native_wait(async { 8 }).unwrap(), 8))
			.unwrap();
		first.drain_shutdown().unwrap();
		second.drain_shutdown().unwrap();
	}
}
