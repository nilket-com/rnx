//! A controlled pipe: the one question an end-to-end cancellation cannot ask.
//!
//! Record 0025's amendment to gate 5. In `run_child`'s cleanup the job ends
//! before the writer is stopped, and on Windows the closing of the last
//! reader when a job dies can end a blocked write by itself. So a call that
//! reports `cancelled` does not establish that `CancelIoEx` reached the
//! operation — it establishes the result, and the mechanism is a separate
//! question.
//!
//! This asks it. The fixture owns **both** ends of a pipe. Nothing closes the
//! read end and nothing drains it, so the only thing that can end the write
//! is `Pipe::stop`, and the only proof that it did is the write's own
//! `ERROR_OPERATION_ABORTED`: an announced intention to write is not proof of
//! a pending operation.
//!
//! It lives in the binary because `Pipe` does, and it runs as its own process
//! because a cancellation that does not reach the operation leaves a write
//! blocked in the kernel. A parent can bound that and kill it; an in-process
//! gate would stop the suite it belongs to. Built only under `test-support`,
//! and only on Windows, where the mechanism exists at all: an ordinary build
//! has no such command.
use crate::platform::{Pipe, Stopped};
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The command, kept here so the gate and the dispatch cannot disagree.
pub const COMMAND: &str = "test-pipe-control";

/// More than any pipe's buffer holds, so one submitted write fills it and
/// blocks in the kernel with the rest still to go. Measured on this machine:
/// a `std` pipe takes far less than this before a write stops returning.
const MORE_THAN_A_PIPE_HOLDS: usize = 4 * 1024 * 1024;

/// How long the negative control lets the write sit before it says the write
/// is unfinished. Long enough that a write which was going to return on its
/// own has had every chance to.
const LONG_ENOUGH_TO_BE_STUCK: Duration = Duration::from_millis(500);

/// How long either half waits for the worker to declare its write. A bound,
/// because a declaration that never comes is a failure and not a reason to
/// wait for ever.
const TO_DECLARE: Duration = Duration::from_secs(5);

pub fn run(mode: Option<&str>) -> crate::Result<()> {
	match mode {
		Some("cancel") => cancelled(),
		Some("held") => held(),
		Some(other) => Err(format!("{COMMAND} takes `cancel` or `held`, not `{other}`").into()),
		None => Err(format!("{COMMAND} takes `cancel` or `held`").into()),
	}
}

/// The gate: a blocked write, ended by a stop, with the read end still open.
fn cancelled() -> crate::Result<()> {
	let (reader, writer) = std::io::pipe()?;
	let pipe = Pipe::new();
	let declared = Arc::new(AtomicBool::new(false));
	let worker = {
		let pipe = pipe.clone();
		let declared = Arc::clone(&declared);
		std::thread::spawn(move || write_once(writer, pipe, declared))
	};
	if !within(TO_DECLARE, || declared.load(Ordering::Relaxed)) {
		return Err("the worker never declared its write".into());
	}

	// That the write is blocked is established here rather than assumed. A
	// write which was going to return on its own has had every chance to by
	// now, so a worker still unfinished is one inside the kernel — and the
	// declaration alone would not have said that: an announced intention to
	// write is not a pending operation.
	std::thread::sleep(LONG_ENOUGH_TO_BE_STUCK);
	let blocked_before_stop = !worker.is_finished();

	// Nothing else is touched between here and the join. The read end is a
	// local this function holds and never reads: draining it, or closing it,
	// would each have been another thing able to end the write, which is the
	// confusion this control exists to remove. It goes after the print.
	let began = Instant::now();
	let stopped = pipe.stop();
	let outcome = worker.join().map_err(|_| "the writer panicked")?;
	let took = began.elapsed();

	println!(
		"mode=cancel blocked_before_stop={blocked_before_stop} stopped={stopped:?} {outcome} took_ms={}",
		took.as_millis()
	);
	drop(reader);
	if stopped != Stopped::Reached {
		return Err(format!("the stop found nothing to reach: {stopped:?}").into());
	}
	Ok(())
}

/// The negative control: the same write, with nothing cancelling it. It must
/// stay unfinished until the fixture releases the pipe, which is what makes
/// the gate above about the cancellation rather than about the write.
fn held() -> crate::Result<()> {
	let (mut reader, writer) = std::io::pipe()?;
	let pipe = Pipe::new();
	let declared = Arc::new(AtomicBool::new(false));
	let worker = {
		let pipe = pipe.clone();
		let declared = Arc::clone(&declared);
		std::thread::spawn(move || write_once(writer, pipe, declared))
	};
	if !within(TO_DECLARE, || declared.load(Ordering::Relaxed)) {
		return Err("the worker never declared its write".into());
	}
	std::thread::sleep(LONG_ENOUGH_TO_BE_STUCK);
	let finished_before_release = worker.is_finished();

	// The release is a drain, not a close: an end that came from closing the
	// read end would be the very thing decision 5's amendment says cannot be
	// told apart from a cancellation.
	let mut drained = 0;
	let mut sink = vec![0u8; 64 * 1024];
	while drained < MORE_THAN_A_PIPE_HOLDS {
		match reader.read(&mut sink) {
			Ok(0) => break,
			Ok(n) => drained += n,
			Err(e) => return Err(format!("the read end could not be drained: {e}").into()),
		}
	}
	let outcome = worker.join().map_err(|_| "the writer panicked")?;
	println!(
		"mode=held finished_before_release={finished_before_release} {outcome} drained={drained}"
	);
	Ok(())
}

/// The shape `deliver` uses: declare, submit one write, withdraw. Nothing
/// retries, because what is gated here is one submitted operation.
fn write_once(writer: std::io::PipeWriter, pipe: Pipe, declared: Arc<AtomicBool>) -> String {
	let mut writer = writer;
	let block = vec![0u8; MORE_THAN_A_PIPE_HOLDS];
	let Some(writing) = pipe.begin(&writer) else {
		return "outcome=never-declared error=none wrote=0".to_owned();
	};
	declared.store(true, Ordering::Relaxed);
	let wrote = writer.write(&block);
	drop(writing);
	match wrote {
		Ok(n) => format!("outcome=returned error=none wrote={n}"),
		Err(e) => format!(
			"outcome=failed error={} wrote=0",
			e.raw_os_error().unwrap_or(-1)
		),
	}
}

/// Wait for something to become true, bounded, and say whether it did.
fn within(how_long: Duration, mut yet: impl FnMut() -> bool) -> bool {
	let ends = Instant::now() + how_long;
	while Instant::now() < ends {
		if yet() {
			return true;
		}
		std::thread::sleep(Duration::from_millis(1));
	}
	yet()
}
