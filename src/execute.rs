//! One way to run a script: record 0032.
//!
//! `run`, `eval` and the session each hand a VM and an entry point to
//! [`drive`], which resumes the execution slice by slice on a current-thread
//! Tokio runtime and says how it ended. The three callers interpret the
//! outcome in their own words; none of them resumes a VM itself any more.
//!
//! A slice is `min(SLICE, remaining)` instructions under Rune's budget,
//! selected against a future that notices the interrupt flag on a cadence
//! while the slice is pending. `budget::Budget<F>` restores the remaining
//! budget on each poll and saves it after, so a slice that goes pending on a
//! host future resumes with what it had left; nothing gets a fresh budget on
//! a wakeup. Record 0032's evidence measured the slice count of a script with
//! and without an await in the middle of it and found them equal.
use rune::runtime::{GeneratorState, Value, VmError, budget};
use rune::Vm;
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

/// Instructions per slice between interrupt checks. Record 0002 chose the
/// number for the session; a file run now uses the same one, because the
/// promise is the same: Ctrl-C is observed within one slice.
pub const SLICE: usize = 10_000;

/// How often the interrupt flag is read while a slice is pending on a host
/// future. Nothing wakes the executor when Ctrl-C sets the flag, so the
/// driver looks on a cadence, which is how record 0020's waits already
/// behave. Only a pending slice pays it; a running one never sleeps.
const CADENCE: Duration = Duration::from_millis(5);

/// How an execution ended.
pub enum Outcome {
	/// The entry point returned this.
	Complete(Value),
	/// The budget ran out. The execution is dropped; nothing is resumable.
	Budget,
	/// Ctrl-C was observed, at a slice boundary or while a slice was pending.
	/// The execution and any future it was waiting on are already dropped.
	Interrupted,
	/// The entry point yielded, which no entry point of rnx's may do.
	Yielded,
	/// The VM raised this, with whatever location it carries.
	Failed(VmError),
}

/// The runtime a command runs on. Built after the context, so `version` and
/// `help` (record 0030) never pay for it; kept by a session across inputs
/// and across `:reset`, and dropped with the command otherwise.
///
/// Current-thread, timer enabled, nothing else: no worker threads, no I/O
/// driver, no blocking pool. A battery that needs more measures it in its
/// own record.
pub struct Runtime(tokio::runtime::Runtime);

impl Runtime {
	pub fn new() -> std::io::Result<Self> {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.map(Self)
	}
}

/// Run `entry` on `vm` with `args`, spending at most `budget` instructions,
/// and say how it ended.
///
/// The VM is borrowed for the call and released whatever the outcome, so the
/// caller may inspect it or drop it; the execution never outlives this call.
pub fn drive(
	runtime: &Runtime,
	vm: &mut Vm,
	entry: [&str; 1],
	args: impl rune::runtime::Args,
	budget: usize,
) -> Outcome {
	runtime.0.block_on(drive_inner(vm, entry, args, budget))
}

async fn drive_inner(
	vm: &mut Vm,
	entry: [&str; 1],
	args: impl rune::runtime::Args,
	budget: usize,
) -> Outcome {
	let mut execution = match vm.execute(entry, args) {
		Ok(execution) => execution,
		Err(error) => return Outcome::Failed(error),
	};
	let mut spent = 0usize;
	loop {
		let slice = SLICE.min(budget - spent);
		let exhausted = Rc::new(Cell::new(false));
		let sliced = budget::with(
			slice,
			Settled {
				inner: Box::pin(execution.async_resume()),
				exhausted: exhausted.clone(),
			},
		);
		let outcome = Watched {
			slice: Box::pin(sliced),
			interrupt: Interrupt::new(),
		}
		.await;
		let Some(outcome) = outcome else {
			// The slice future, and the host future it was pending on, are
			// dropped with `Watched`. That is the whole of cancellation here:
			// the driver spawned nothing, so nothing else is running.
			return Outcome::Interrupted;
		};
		match outcome.into_result() {
			// Completion is a slice boundary too: an interrupt that arrived
			// during a blocking host call is observed here, as the session
			// always has.
			Ok(GeneratorState::Complete(_)) if crate::host::interrupted() => {
				return Outcome::Interrupted;
			}
			Ok(GeneratorState::Complete(value)) => return Outcome::Complete(value),
			Ok(GeneratorState::Yielded(_)) => return Outcome::Yielded,
			// A budget halt: no location on the error, and the guard was
			// exhausted when the poll settled. Never the error's text.
			Err(error) if error.first_location().is_none() && exhausted.get() => {
				spent += slice;
				if crate::host::interrupted() {
					return Outcome::Interrupted;
				}
				if spent >= budget {
					return Outcome::Budget;
				}
			}
			Err(error) => return Outcome::Failed(error),
		}
	}
}

/// Polls the slice and, at the moment it settles, records whether the
/// budget installed around this poll was exhausted. That reading has to be
/// taken inside the budget wrapper's poll, while the budget is installed;
/// after `Budget<F>` returns, the thread's budget is whatever it was before.
struct Settled<F> {
	inner: Pin<Box<F>>,
	exhausted: Rc<Cell<bool>>,
}

impl<F: Future> Future for Settled<F> {
	type Output = F::Output;
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
		let poll = self.inner.as_mut().poll(cx);
		if poll.is_ready() {
			self.exhausted.set(!budget::acquire().take());
		}
		poll
	}
}

/// Resolves when the interrupt flag is seen, looking on the cadence.
struct Interrupt {
	interval: tokio::time::Interval,
}

impl Interrupt {
	fn new() -> Self {
		let mut interval = tokio::time::interval(CADENCE);
		interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		Self { interval }
	}
}

impl Future for Interrupt {
	type Output = ();
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
		loop {
			match self.interval.poll_tick(cx) {
				Poll::Ready(_) => {
					if crate::host::interrupted() {
						return Poll::Ready(());
					}
				}
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

/// The slice, or `None` if the interrupt was seen first. The slice is polled
/// first, so an input that completes in one poll never consults the flag
/// here; the boundary check in the loop does that.
struct Watched<F> {
	slice: Pin<Box<F>>,
	interrupt: Interrupt,
}

impl<F: Future> Future for Watched<F> {
	type Output = Option<F::Output>;
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		if let Poll::Ready(out) = self.slice.as_mut().poll(cx) {
			return Poll::Ready(Some(out));
		}
		match Pin::new(&mut self.interrupt).poll(cx) {
			Poll::Ready(()) => Poll::Ready(None),
			Poll::Pending => Poll::Pending,
		}
	}
}
