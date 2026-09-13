//! How a script runs: record 0032.
//!
//! Two paths:
//!
//! - [`slice_sync`]: resumed in slices of [`SLICE`] instructions with the
//!   interrupt flag read between them. What the session has always done for
//!   an input (record 0002), unchanged, and what an input that cannot await
//!   still gets, so Ctrl-C still ends a loop at a prompt.
//! - [`drive_async`]: the whole execution under its whole budget on a
//!   current-thread Tokio runtime, raced against a future that reads the
//!   interrupt flag on a cadence while the execution is pending on a host
//!   future. Every file `run` executes, and every session input that awaits.
//!
//! A file is not classified at all. Whether its `main` can await is not a
//! question this can answer from the source: `pub use inner::work as main`
//! makes an async function the entry point without an `async` token in sight,
//! and the compiled unit keeps its calling convention to itself. So every
//! file takes the driver that copes with either, and what a file gives up by
//! not being sliced it never had — record 0021's file run was never
//! interruptible mid-loop.
//!
//! Why the async path is not sliced, and why the two synchronous paths are
//! kept exactly: an `async fn` awaited from Rune code runs as a nested
//! execution wrapped in a future value, and a budget halt inside it resolves
//! that future to an error, after which it is gone — Rune drops a settled
//! future. Resuming the outer execution then completes with a unit value,
//! silently, in place of the nested result. So the budget cannot be used to
//! slice an execution that may nest, which is any execution that can await.
//! The record's evidence has the probe; the number it produced was `()` for
//! a loop that sums to 1,999,000. A synchronous execution nests nothing —
//! a synchronous call is a frame in the same execution — so its slices are
//! resumable, and Rune 0.14.2 offers no other per-instruction hook. Hence
//! the property, and the three paths.
use rune::Vm;
use rune::runtime::{GeneratorState, Value, VmError, budget};
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

/// Instructions per slice between interrupt checks, on the synchronous
/// sliced path. Record 0002 chose it.
pub const SLICE: usize = 10_000;

/// How often the interrupt flag is read while an async execution is pending
/// on a host future. Nothing wakes the executor when Ctrl-C sets the flag —
/// a store to an atomic wakes no task, measured — so the driver looks on a
/// timer, which is how record 0020's waits already behave. Only a pending
/// execution pays it.
const CADENCE: Duration = Duration::from_millis(5);

/// What a completed execution means when Ctrl-C arrived while it ran.
///
/// The session abandons the input: at a prompt Ctrl-C means stop this one,
/// and record 0002 has always observed, at the boundary, an interrupt that
/// arrived while an input was inside a host call. A file run lets the
/// script's own ending stand, which is what it has always done and what
/// records 0022 and 0023 ask of it — a cancelled child is reported to the
/// script, and what the script does about it is the script's to decide.
#[derive(Clone, Copy)]
pub enum WhenInterrupted {
	Abandon,
	Finish,
}

/// How an execution ended.
pub enum Outcome {
	/// The entry point returned this.
	Complete(Value),
	/// The budget ran out. Nothing is resumable.
	Budget,
	/// Ctrl-C was observed. The execution and any future it was waiting on
	/// are already dropped.
	Interrupted,
	/// The entry point yielded, which no entry point of rnx's may do.
	Yielded,
	/// The VM raised this, with whatever location it carries. `exhausted`
	/// says the budget was spent to the last instruction at the moment it
	/// settled, which is worth reporting beside the error and is never
	/// reported instead of it: a failure on the last permitted instruction
	/// leaves the budget at zero exactly as a halt does, and the error is
	/// the thing the reader needs.
	Failed { error: VmError, exhausted: bool },
}

/// An input that never awaits: resumed in slices with the flag read between
/// them, as the session has always done. A budget halt in a synchronous
/// execution leaves it resumable: a synchronous call is a frame in the same
/// execution, not a nested one.
pub fn slice_sync(vm: &mut Vm, entry: [&str; 1], args: impl rune::runtime::Args, budget_n: usize) -> Outcome {
	let mut execution = match vm.execute(entry, args) {
		Ok(execution) => execution,
		Err(error) => return Outcome::Failed { error, exhausted: false },
	};
	let mut spent = 0usize;
	loop {
		let (outcome, exhausted) = budget::with(SLICE, || {
			let outcome = execution.resume().into_result();
			let exhausted = !budget::acquire().take();
			(outcome, exhausted)
		})
		.call();
		match outcome {
			// Completion is a slice boundary too: an interrupt that arrived
			// during a blocking host call is observed here.
			Ok(GeneratorState::Complete(_)) if crate::host::interrupted() => return Outcome::Interrupted,
			Ok(GeneratorState::Complete(value)) => return Outcome::Complete(value),
			Ok(GeneratorState::Yielded(_)) => return Outcome::Yielded,
			Err(error) if error.first_location().is_none() && exhausted => {
				spent += SLICE;
				if crate::host::interrupted() {
					return Outcome::Interrupted;
				}
				if spent >= budget_n {
					return Outcome::Budget;
				}
			}
			Err(error) => {
				// The guard says this slice was spent, not that the input's
				// budget was. Only the whole budget is worth naming beside an
				// error, and only the last slice can have spent it.
				let exhausted = exhausted && spent + SLICE >= budget_n;
				return Outcome::Failed { error, exhausted };
			}
		}
	}
}

/// The runtime an async execution runs on. Built only when a script can
/// await, so a synchronous script never pays for it and records 0030's
/// `version` and `help` never see it. A session keeps one across inputs and
/// across `:reset`. Current-thread, timer enabled, nothing else.
pub struct Runtime(tokio::runtime::Runtime);

impl Runtime {
	pub fn new() -> std::io::Result<Self> {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.map(Self)
	}
}

/// A script that can await: the whole execution under its whole budget,
/// raced against the interrupt flag. Pending on a host future, it is ended
/// within one cadence of Ctrl-C. Running Rune code, it is bounded by its
/// budget and by nothing else: the budget cannot slice it, for the reason at
/// the top of this file, and Rune offers no other hook.
pub fn drive_async(
	runtime: &Runtime,
	vm: &mut Vm,
	entry: [&str; 1],
	args: impl rune::runtime::Args,
	budget_n: usize,
	when_interrupted: WhenInterrupted,
) -> Outcome {
	let execution = match vm.execute(entry, args) {
		Ok(execution) => execution,
		Err(error) => return Outcome::Failed { error, exhausted: false },
	};
	let exhausted = Rc::new(Cell::new(false));
	let settled = Settled {
		inner: Box::pin(async move {
			let mut execution = execution;
			execution.async_resume().await
		}),
		exhausted: exhausted.clone(),
	};
	let work = budget::with(budget_n, settled);
	// The interval is a runtime resource, so it is made inside the runtime.
	let outcome = runtime.0.block_on(async move {
		Watched {
			work: Box::pin(work),
			interrupt: Interrupt::new(),
		}
		.await
	});
	let Some(outcome) = outcome else {
		// `Watched` is dropped with the execution and the host future it
		// was pending on. That is the whole of cancellation: the driver
		// spawned nothing, so nothing else is running.
		return Outcome::Interrupted;
	};
	match outcome.into_result() {
		Ok(GeneratorState::Complete(_))
			if matches!(when_interrupted, WhenInterrupted::Abandon)
				&& crate::host::interrupted() =>
		{
			Outcome::Interrupted
		}
		Ok(GeneratorState::Complete(value)) => Outcome::Complete(value),
		Ok(GeneratorState::Yielded(_)) => Outcome::Yielded,
		// A halt for want of budget, recognised as it is on both synchronous
		// paths: no location, and the guard exhausted when the execution
		// settled. The guard alone would not do, because a failure on the
		// last permitted instruction leaves it exhausted too and its
		// diagnostic is the thing worth keeping. The cost of the two
		// together is that a budget spent inside a nested async function
		// carries that function's location and so reports as the halt it is,
		// with the budget named beside it, rather than as the tidier line
		// the head gets; reading the halt apart from the error would need
		// Rune's error kind, which it keeps to itself.
		Err(error) if error.first_location().is_none() && exhausted.get() => Outcome::Budget,
		Err(error) => Outcome::Failed { error, exhausted: exhausted.get() },
	}
}

/// Polls the execution and, at the moment it settles, records whether the
/// budget installed around this poll was exhausted. That reading must be
/// taken inside the budget wrapper's poll, while the budget is installed.
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

/// Resolves when the interrupt flag is seen, looking on the cadence. The
/// interval is what wakes the task; the flag alone would not.
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

/// The execution's outcome, or `None` if the interrupt was seen first. The
/// execution is polled first, so one that completes in a single poll never
/// consults the flag here; the completion check does that.
struct Watched<F> {
	work: Pin<Box<F>>,
	interrupt: Interrupt,
}

impl<F: Future> Future for Watched<F> {
	type Output = Option<F::Output>;
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		if let Poll::Ready(out) = self.work.as_mut().poll(cx) {
			return Poll::Ready(Some(out));
		}
		match Pin::new(&mut self.interrupt).poll(cx) {
			Poll::Ready(()) => Poll::Ready(None),
			Poll::Pending => Poll::Pending,
		}
	}
}
