//! Minimal trusted-local host for the spike, not a proposed standard library.
use rune::{Context, Module, runtime::Value};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Whether Ctrl-C arrived since the flag was last cleared. The flag and the
/// handler that sets it belong to the platform, because a signal and a
/// console control handler are not the same mechanism; what they mean is.
pub fn interrupted() -> bool {
	crate::platform::interrupted()
}
/// Cleared by the session before each evaluation, so an interrupt that
/// arrived at the prompt never cancels the next input.
pub fn clear_interrupt() {
	crate::platform::clear_interrupt();
}

fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}
/// An error that names what it was working on. The first port found that a
/// script reading several files could not say which one was missing without
/// wrapping every call, so every host function that takes a path or a
/// program names it here instead.
fn about(what: &str, subject: &str, e: impl std::fmt::Display) -> String {
	format!("cannot {what} {subject}: {e}")
}
fn json_parse(text: &str) -> Result<Value, String> {
	serde_json::from_str(text).map_err(error)
}
/// The one JSON serializer a script can reach. The bound and the refusal
/// vocabulary live in `json`, so nothing else in the crate decides what JSON
/// can represent.
fn json_stringify(value: Value) -> Result<String, String> {
	super::json::stringify(&value)
}
fn file_read(path: &str) -> Result<String, String> {
	let mut bytes = Vec::new();
	std::fs::File::open(path)
		.map_err(|e| about("read", path, e))?
		.take(8 * 1024 * 1024 + 1)
		.read_to_end(&mut bytes)
		.map_err(|e| about("read", path, e))?;
	if bytes.len() > 8 * 1024 * 1024 {
		return Err(format!("cannot read {path}: it exceeds the 8 MiB limit"));
	}
	String::from_utf8(bytes).map_err(|e| about("read", path, e))
}
/// Whether standard input has already been consumed. The stream can only be
/// read to its end once, and a second attempt is a mistake worth naming
/// rather than an empty string indistinguishable from an empty stream.
static STDIN_READ: AtomicBool = AtomicBool::new(false);

/// The whole of standard input as UTF-8, under the same limit as a file.
///
/// A terminal is refused instead of read. In the session the line editor owns
/// the terminal, and under `run` with no redirection reading one blocks until
/// somebody types an end-of-file, which is indistinguishable from a hung
/// script. A pipe, a redirected file, and a closed stream are all read.
fn stdin_read() -> Result<String, String> {
	// SAFETY: isatty only inspects the descriptor.
	if crate::platform::stdin_is_a_terminal() {
		return Err(
			"cannot read standard input: it is a terminal; redirect a file or pipe into it"
				.to_owned(),
		);
	}
	if STDIN_READ.swap(true, Ordering::Relaxed) {
		return Err("cannot read standard input: it has already been read".to_owned());
	}
	let mut bytes = Vec::new();
	std::io::stdin()
		.lock()
		.take(8 * 1024 * 1024 + 1)
		.read_to_end(&mut bytes)
		.map_err(|e| format!("cannot read standard input: {e}"))?;
	if bytes.len() > 8 * 1024 * 1024 {
		return Err("cannot read standard input: it exceeds the 8 MiB limit".to_owned());
	}
	String::from_utf8(bytes).map_err(|e| format!("cannot read standard input: {e}"))
}

/// Whether rnx is running one thing and then exiting, rather than holding a
/// prompt. Set by the two entry points that have a status to give.
static RUNNING_A_SCRIPT: AtomicBool = AtomicBool::new(false);

/// Called by `run` and by `eval`, the two entry points whose whole job is to
/// run one thing and exit. The session never calls it, so `host::exit` is
/// refused there rather than ending a person's session.
pub fn running_a_script() {
	RUNNING_A_SCRIPT.store(true, Ordering::Relaxed);
}

/// End the process with `code`, having first made sure the script's output
/// actually reached somewhere. Never returns.
///
/// Standard output is flushed here because output with no trailing newline is
/// still buffered and nothing unwinds from this point. A flush that fails
/// means the output was lost, and losing a script's report must never be
/// reported as success: the failure is named on standard error, and a status
/// of 0 becomes 1. A script that had already decided it was failing keeps the
/// status it chose, because that status is still true.
fn leave(code: i32) -> ! {
	let code = match std::io::stdout().flush() {
		Ok(()) => code,
		Err(e) => {
			eprintln!("error: cannot write standard output: {e}");
			if code == 0 { 1 } else { code }
		}
	};
	std::process::exit(code);
}

/// End the script with `code`.
///
/// A status is one byte by the time a shell reads it, so 256 would arrive as
/// 0 and turn a failure into a success. Anything outside 0 to 255 ends the
/// script with 1 instead, which is the one moment it can be refused rather
/// than truncated.
///
/// In a script this does not return, including when the status is refused. An
/// ordinary error would be a value the script could discard, and discarding
/// it would let a script that asked for an impossible status carry on and
/// exit 0, which is exactly the outcome the refusal exists to prevent. At a
/// session prompt there is nothing to end, so the refusal there is an
/// ordinary error the session reports and recovers from.
fn exit(code: i64) -> Result<(), String> {
	if !RUNNING_A_SCRIPT.load(Ordering::Relaxed) {
		return Err("cannot exit: this is a session, not a script; use :quit".to_owned());
	}
	if !(0..=255).contains(&code) {
		eprintln!(
			"error: cannot exit with {code}: a status is 0 to 255, and {code} would reach the shell as {}",
			(code as u8) as i64
		);
		leave(1);
	}
	leave(code as i32);
}

/// Write to standard error, adding nothing. A function that appended a
/// newline could not be asked not to; this one can be asked to.
fn eprint(text: &str) -> Result<(), String> {
	let mut stderr = std::io::stderr().lock();
	stderr
		.write_all(text.as_bytes())
		.and_then(|()| stderr.flush())
		.map_err(|e| format!("cannot write to standard error: {e}"))
}

fn file_write(path: &str, text: &str) -> Result<(), String> {
	std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)
		.map_err(|e| about("write", path, e))?
		.write_all(text.as_bytes())
		.map_err(|e| about("write", path, e))
}
fn mkdir(path: &str) -> Result<(), String> {
	std::fs::create_dir(path).map_err(|e| about("create directory", path, e))
}
fn absolute(path: &str) -> Result<String, String> {
	std::fs::canonicalize(path)
		.map_err(|e| about("resolve", path, e))?
		.into_os_string()
		.into_string()
		.map_err(|_| format!("cannot resolve {path}: it is not valid UTF-8"))
}
/// How long the readers may go on reading once the wait is over.
///
/// A measured starting choice rather than a derived one: draining the whole
/// two mebibyte cap takes a few milliseconds, so this is comfortably more
/// than an ordinary cleanup needs. It cannot be justified by how much is
/// left in the pipes, because a descendant outside the process group can go
/// on writing for as long as it lives; what bounds that is reading the clock
/// before every attempt.
///
/// It bounds how long rnx keeps **asking**. The pause between attempts is a
/// sleep, which a scheduler may overrun, so it is not a wall-clock promise.
const CLEANUP_ALLOWANCE: Duration = Duration::from_millis(100);

/// Whether a reader should panic, and which one. A panicking thread cannot be
/// arranged from a script either, and the contract being gated is that the
/// **other** reader is still collected when one of them dies.
#[cfg(feature = "test-support")]
fn injected_reader_panic(stream: &str) -> bool {
	std::env::var("RNX_TEST_READER_PANICS").is_ok_and(|which| which == stream)
}

#[cfg(not(feature = "test-support"))]
fn injected_reader_panic(_stream: &str) -> bool {
	false
}

/// How long to hold a reader back before its first attempt. Nothing a script
/// can do delays a reader by a known amount, and without one the gate for
/// "the readers are drained before any of them is reached" is a race: an
/// ordinary child's output is usually read before the call even ends. Held
/// back by a known amount, with an allowance longer than it, the two
/// orderings give different answers — everything, or nothing.
#[cfg(feature = "test-support")]
fn injected_reader_delay() -> Duration {
	std::env::var("RNX_TEST_READER_STARTS_LATE_MS")
		.ok()
		.and_then(|ms| ms.parse().ok())
		.map(Duration::from_millis)
		.unwrap_or_default()
}

#[cfg(not(feature = "test-support"))]
fn injected_reader_delay() -> Duration {
	Duration::ZERO
}

/// Whether to fail a read deliberately. A read error on a pipe cannot be
/// provoked from a script — a non-blocking pipe offers a full buffer, an
/// interruption, or the end of the stream — so the gate that proves an
/// unreadable stream is reported, and refuses the exemptions a truncated one
/// gets, injects it. Off unless `test-support` is built.
#[cfg(feature = "test-support")]
fn injected_read_failure() -> bool {
	std::env::var("RNX_TEST_CAPTURE_FAILS").is_ok_and(|v| !v.is_empty())
}

#[cfg(not(feature = "test-support"))]
fn injected_read_failure() -> bool {
	false
}

/// The allowance in force. Ordinarily the constant; under `test-support` a
/// gate may set it, which is how "an ordinary exit spends none of it" can be
/// measured with a clock rather than guessed from a stopwatch: make the
/// allowance seconds long and an unnecessary wait becomes impossible to miss.
#[cfg(feature = "test-support")]
fn cleanup_allowance() -> Duration {
	std::env::var("RNX_TEST_CLEANUP_ALLOWANCE_MS")
		.ok()
		.and_then(|ms| ms.parse().ok())
		.map(Duration::from_millis)
		.unwrap_or(CLEANUP_ALLOWANCE)
}

#[cfg(not(feature = "test-support"))]
fn cleanup_allowance() -> Duration {
	CLEANUP_ALLOWANCE
}

/// What a stream was, and what went wrong with reading it. The three
/// shortfalls are independent: a stream can pass the cap, then be cut short
/// when the cleanup instant arrives, and a read of it can fail.
struct Captured {
	bytes: Vec<u8>,
	/// The size cap was reached and bytes past it were discarded.
	truncated: bool,
	/// Reading stopped before the stream ended, because the call had.
	cut_short: bool,
	/// A read failed. Before record 0023 this ended the capture exactly as
	/// the end of the stream did, so a broken stream and a finished one were
	/// the same thing to a caller.
	unreadable: bool,
}

/// The largest a single stream's capture may grow. Unchanged by record 0023.
const CAPTURE_CAP: usize = 2 * 1024 * 1024;

/// Read a stream until it ends, the cap is reached, or the call is done with
/// it — whichever comes first.
///
/// The descriptor is made non-blocking and `until` is read **before every
/// attempt**, which is what bounds a descendant that left the process group
/// and goes on writing: the group kill cannot stop it, and no reasoning about
/// how much is left in the pipe covers a writer that keeps going.
///
/// `until` is a set-once cell holding the instant the call published when its
/// wait ended. While it is empty the stream is drained as before.
fn capture(
	input: impl Read + crate::platform::Stream,
	pipe: crate::platform::Pipe,
	until: std::sync::Arc<std::sync::OnceLock<Instant>>,
	stream: &'static str,
) -> Captured {
	assert!(
		!injected_reader_panic(stream),
		"the {stream} reader was told to panic"
	);
	let mut got = Captured {
		bytes: Vec::new(),
		truncated: false,
		cut_short: false,
		unreadable: false,
	};
	if crate::platform::prepare_stream(&input).is_err() {
		got.unreadable = true;
		return got;
	}
	let mut input = input;
	let mut chunk = [0; 8192];
	std::thread::sleep(injected_reader_delay());
	let started = Instant::now();
	loop {
		// Before every attempt, so a stream that always has more to give
		// cannot outlast the call that wanted it.
		// An interrupt stops a reader wherever it is, including in the middle
		// of the cleanup allowance: by then the wait loop has stopped watching
		// for one, so this is the only thing looking. So does the call itself,
		// through the pipe, when the allowance has run out.
		if crate::platform::interrupted() || pipe.stopped() {
			got.cut_short = true;
			return got;
		}
		if let Some(deadline) = until.get()
			&& Instant::now() >= *deadline
		{
			got.cut_short = true;
			return got;
		}
		if injected_read_failure() {
			got.unreadable = true;
			return got;
		}
		// Whether a read may be attempted at all. A non-blocking descriptor
		// answers by returning `WouldBlock`; a Windows pipe has to be asked
		// first, because a read of it would block where no flag reaches.
		if !crate::platform::readable(&input) {
			std::thread::sleep(next_wait(
				started.elapsed(),
				until
					.get()
					.map(|d| d.saturating_duration_since(Instant::now()))
					.unwrap_or(LONGEST_WAIT),
			));
			continue;
		}
		// Declaring the read and checking whether the call still wants it are
		// one step, so an operation cannot be declared behind a stop. The
		// declaration lasts as long as the guard, and a panic inside the read
		// withdraws it on the way out.
		let Some(reading) = pipe.begin(&input) else {
			got.cut_short = true;
			return got;
		};
		let read = input.read(&mut chunk);
		drop(reading);
		match read {
			Ok(0) => return got,
			Ok(n) => {
				let keep = n.min(CAPTURE_CAP.saturating_sub(got.bytes.len()));
				got.bytes.extend_from_slice(&chunk[..keep]);
				got.truncated |= keep < n;
			}
			Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
				std::thread::sleep(next_wait(
					started.elapsed(),
					until
						.get()
						.map(|d| d.saturating_duration_since(Instant::now()))
						.unwrap_or(LONGEST_WAIT),
				));
			}
			Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
			// A read cancelled from outside — which is how Windows reaches one
			// blocked in the kernel — is this call's own doing, so the stream
			// was cut short rather than unreadable.
			Err(e) if aborted(&e) => {
				got.cut_short = true;
				return got;
			}
			Err(_) => {
				got.unreadable = true;
				return got;
			}
		}
	}
}

/// Run a child, optionally writing `input` to its standard input and closing
/// it afterwards.
///
/// The input is copied into the writer thread rather than borrowed: record
/// 0022 hands the bytes to a thread, which outlives the `Value` they came
/// from even though it no longer outlives the call.
fn run_child(
	program: &str,
	arguments: Value,
	timeout_ms: u64,
	input: Option<Vec<u8>>,
) -> Result<Ran, String> {
	let values = arguments
		.borrow_ref::<rune::runtime::Vec>()
		.map_err(|e| about("run", program, e))?;
	let args: Vec<String> = values
		.iter()
		.map(|v| {
			v.borrow_string_ref()
				.map(|s| s.to_string())
				.map_err(|e| about("run", program, e))
		})
		.collect::<Result<_, _>>()?;
	if timeout_ms == 0 || timeout_ms > 90_000 {
		return Err(about(
			"run",
			program,
			"the deadline must be between 1 and 90000 ms",
		));
	}
	let mut command = Command::new(program);
	command
		.args(args)
		.stdin(if input.is_some() {
			Stdio::piped()
		} else {
			Stdio::null()
		})
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	// The child goes into a group of its own — a process group, or a job
	// object on Windows — so its descendants can be ended with it. On Windows
	// the assignment happens while the child is still suspended, because
	// assigning a running child races whatever it spawns first.
	let (mut child, group) =
		crate::platform::spawn_in_group(&mut command).map_err(|e| about("run", program, e))?;
	let deadline = Instant::now() + Duration::from_millis(timeout_ms);
	let stdout = child.stdout.take().unwrap();
	let stderr = child.stderr.take().unwrap();
	// One instant, set once when the wait ends, read by both readers: two of
	// them cannot spend the allowance twice.
	let until = std::sync::Arc::new(std::sync::OnceLock::<Instant>::new());
	// Each pipe is owned by an object both the reader and this call hold, so
	// neither can close it under the other, and a reader inside a read can be
	// reached rather than only one between two.
	// Each reader keeps its own stream and publishes it to the protocol only
	// while it is inside a read, so this call can reach an operation without
	// ever holding a handle its owner may have let go of.
	let reading = [crate::platform::Pipe::new(), crate::platform::Pipe::new()];
	let out = {
		let (until, pipe) = (until.clone(), reading[0].clone());
		std::thread::spawn(move || capture(stdout, pipe, until, "stdout"))
	};
	let err = {
		let (until, pipe) = (until.clone(), reading[1].clone());
		std::thread::spawn(move || capture(stderr, pipe, until, "stderr"))
	};
	// The input moves on a third thread, so requests are delivered while
	// replies and complaints are drained: what that removes is mutual pipe
	// backpressure, not every way a child can fail to finish.
	//
	// The delivery gets a stop flag of its own, per call. `INTERRUPTED` is the
	// whole process's and the deadline is a time; neither says "this call is
	// finished with you", which is what has to be said when the child exits
	// with input still undelivered.
	let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
	let writing = crate::platform::Pipe::new();
	let delivery = input.map(|bytes| {
		let stream = child.stdin.take().expect("stdin was piped for an input");
		let (pipe, stop) = (writing.clone(), stop.clone());
		// The writer owns the stream, so it closes when the writer is done —
		// which is the end of input the child is waiting for. A call that held
		// the write end open would leave `cat` waiting for ever.
		std::thread::spawn(move || deliver(stream, pipe, bytes, deadline, stop))
	});
	let mut timed_out = false;
	let mut cancelled = false;
	// Each pause asked for is a fraction of how long the child has already
	// run, so the pauses are short while the child is young and grow to what
	// this loop used for everything before. Doubling from a short start was
	// tried first and measured worse — it reaches a millisecond just as a
	// `git cat-file` finishes, so the last sleep overshoots by most of one.
	let started = Instant::now();
	// The wait's own failures are held rather than returned: there is a
	// delivery to stop and collect, and a `?` here would leave by the front
	// door with a thread still writing. Every way out of this function now
	// goes through the cleanup below.
	let waited: Result<std::process::ExitStatus, String> = loop {
		match child.try_wait() {
			Ok(Some(status)) => break Ok(status),
			Ok(None) => {}
			Err(e) => break Err(about("run", program, e)),
		}
		cancelled = crate::platform::interrupted();
		timed_out = Instant::now() >= deadline;
		if timed_out || cancelled {
			group.end();
			break child.wait().map_err(|e| about("run", program, e));
		}
		// No sleep outlives the deadline: overshooting it by a whole interval
		// would report a timeout later than the caller asked for.
		let left = deadline.saturating_duration_since(Instant::now());
		std::thread::sleep(next_wait(started.elapsed(), left));
	};
	// First, before anything else is stopped or collected: the readers should
	// already be winding down while the rest of the cleanup happens.
	//
	// A cancellation gets no allowance at all. Ctrl-C means stop, not tidy up,
	// and record 0020 measured interruption latency in single milliseconds;
	// spending a hundred of them collecting output nobody asked for would
	// undo that. Anything unread is reported as `cut_short`.
	let allowance = if cancelled {
		Duration::ZERO
	} else {
		cleanup_allowance()
	};
	let cleanup_ends = Instant::now() + allowance;
	let _ = until.set(cleanup_ends);
	// The readers are **not** told to stop yet. An ordinary exit leaves bytes
	// in the pipes and record 0023 promised to drain them; cancelling a read
	// now would discard them and call the capture cut short, which is the
	// promise and not the mechanism. So they are given until the instant, and
	// only what is still reading when it passes is reached — below, after the
	// rest of the cleanup, so the waiting costs nothing that was not going to
	// be waited for anyway.
	//
	// A cancellation is the exception, and it is already expressed: its
	// allowance is zero, so the instant has passed and the loop below reaches
	// both readers immediately.
	// Children remaining in this process group may otherwise keep pipes open.
	// A descendant that left the group — `setsid` — is not killed here and
	// cannot be, but it can no longer extend this call: it holds a pipe rnx
	// has stopped reading. Record 0020 named that limitation, record 0022
	// closed it for the input, and record 0023 closes it for the replies.
	group.end();
	// The wait is over, however it ended — including badly — so the delivery is
	// told to stop and is then **always** collected. It cannot outlast this
	// call: the pipe is non-blocking and the flag is read before every
	// attempt, so the thread returns within one of its own waits. Detaching it
	// instead — which an earlier draft did — would discard whatever it had to
	// say, including a failure this promises to report.
	stop.store(true, Ordering::Relaxed);
	// The writer has nothing left to deliver that anyone wants, so it is told
	// to stop now rather than at the instant. `Pipe::stop` decides whether a
	// flag or a cancellation reaches it, under the lock that keeps the worker
	// from submitting an operation in between.
	writing.stop();
	let mut delivery_failed = None;
	if let Some(delivery) = delivery {
		match delivery.join() {
			// The child stopping is its prerogative, and being told to stop is
			// this function's own doing; neither is a failure to report.
			Ok(DeliveryOutcome::Failed(e)) => delivery_failed = Some(about("run", program, e)),
			Err(_) => delivery_failed = Some(about("run", program, "the writer panicked")),
			_ => {}
		}
	}
	// Now the readers: each is given until the instant to reach the end of its
	// stream on its own, and is reached only if it is still going when the
	// instant has passed. Each is decided separately — one reader still
	// draining is no reason to cut the other short.
	while Instant::now() < cleanup_ends && !(out.is_finished() && err.is_finished()) {
		std::thread::sleep(SHORTEST_WAIT);
	}
	for (worker, pipe) in [(&out, &reading[0]), (&err, &reading[1])] {
		if !worker.is_finished() {
			pipe.stop();
		}
	}
	// Both, before either failure is propagated: a `?` on the first join would
	// leave the second reader unjoined, which is the same defect record 0022
	// fixed for the writer and the same promise — always collected — made here.
	let out = out.join();
	let err = err.join();
	let out = out.map_err(|_| "stdout reader panicked")?;
	let err = err.map_err(|_| "stderr reader panicked")?;
	// Now that everything is stopped and collected, a failure can be reported.
	let status = waited?;
	if let Some(failure) = delivery_failed {
		return Err(failure);
	}
	// An interrupt that arrived while this was cleaning up is still an
	// interrupt. The wait loop stopped watching for one when it ended, so it
	// is read again here: the readers stop on it themselves, and without this
	// the call would report a capture cut short for no reason it could name.
	let cancelled = cancelled || crate::platform::interrupted();
	Ok(Ran {
		code: status.code(),
		timed_out,
		cancelled,
		out,
		err,
	})
}

/// The floor on a pause while waiting for a child, so a child that exits at
/// once is noticed at once rather than after a fixed sleep: record 0020
/// measured 1,346 spawns at 7.17 s with a flat 5 ms wait.
const SHORTEST_WAIT: Duration = Duration::from_micros(100);

/// The ceiling on a pause. It is deliberately the interval this loop used for
/// everything before record 0020, so a long-lived child is polled no harder
/// than it ever was; making a short wait cheaper must not make a long one
/// more expensive. A child reaches this after running for 40 ms.
const LONGEST_WAIT: Duration = Duration::from_millis(5);

/// How long to ask to sleep before looking at a child again.
///
/// This is the duration **requested**: an eighth of how long the child has
/// already run, floored at `SHORTEST_WAIT` so a child that exits at once is
/// looked at again promptly, capped at `LONGEST_WAIT` so a long-lived one is
/// polled no harder than it was before record 0020, and never longer than the
/// time left before the deadline.
///
/// It is not a bound on how quickly a child is noticed. The floor makes the
/// pause longer than an eighth for any child younger than 800 µs, and a sleep
/// is a request the scheduler may overrun. What the shape buys is measured in
/// record 0020 rather than promised here.
fn next_wait(elapsed: Duration, left: Duration) -> Duration {
	(elapsed / 8).clamp(SHORTEST_WAIT, LONGEST_WAIT).min(left)
}

/// Write a child's whole input, or give up trying.
///
/// The pipe is made non-blocking, so a child that stops reading — or a
/// descendant that holds the read end open without reading it — cannot pin
/// this thread. Before every attempt it reads three things: the stop flag its
/// call gives it, the deadline, and the process's interrupt flag. So it stops
/// when its call is finished with it, when the time is up, or when the person
/// running it says so, and the call can therefore always collect what it has
/// to say rather than detaching it and hoping.
///
/// Between attempts the wait grows the way record 0020's does.
///
/// The input is owned here rather than borrowed. The thread no longer
/// outlives its call — every exit path stops and collects it — but it does
/// outlive the `Value` the bytes came from, which the virtual machine may
/// collect while this is still writing.
fn deliver(
	stream: std::process::ChildStdin,
	pipe: crate::platform::Pipe,
	bytes: Vec<u8>,
	deadline: Instant,
	stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> DeliveryOutcome {
	if let Err(e) = crate::platform::prepare_stream(&stream) {
		return DeliveryOutcome::Failed(e.to_string());
	}
	let mut stream = stream;
	let mut written = 0;
	let started = Instant::now();
	while written < bytes.len() {
		// Before every attempt, not only after one that would block: a write
		// that can proceed must not be started once the call has finished with
		// this delivery, or the flag would only be honoured while blocked.
		if stop.load(Ordering::Relaxed)
			|| Instant::now() >= deadline
			|| crate::platform::interrupted()
		{
			// The one place a failure can be injected, and it is here because
			// here is the moment that matters: the call has begun its cleanup
			// and a failure arriving now must still be collected.
			if let Some(reason) = injected_delivery_failure() {
				return DeliveryOutcome::Failed(reason);
			}
			return DeliveryOutcome::Abandoned;
		}
		// One step again: the call cannot decide to stop between this check
		// and the write it declares. A cancellation that lands before the
		// write is submitted cancels nothing, which is why the call keeps
		// asking while this declaration stands.
		let Some(writing) = pipe.begin(&stream) else {
			return DeliveryOutcome::Abandoned;
		};
		let wrote = stream.write(&bytes[written..]);
		drop(writing);
		match wrote {
			Ok(0) => break,
			Ok(n) => written += n,
			Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
				// Nothing can be written yet. The checks above decide whether to
				// keep waiting; this only decides how long.
				std::thread::sleep(next_wait(
					started.elapsed(),
					deadline.saturating_duration_since(Instant::now()),
				));
			}
			// The child stopped reading, which is its prerogative.
			Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
				return DeliveryOutcome::ChildStoppedReading;
			}
			Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
			// A write cancelled from outside is this call's own doing too:
			// abandoned, not failed.
			Err(e) if aborted(&e) => return DeliveryOutcome::Abandoned,
			Err(e) => return DeliveryOutcome::Failed(e.to_string()),
		}
	}
	// The pipe drops here, which is the close a child needs to see the end of
	// its input. `bytes` drops with it, releasing the copy.
	DeliveryOutcome::Delivered
}

/// A deliberate delivery failure, for the gate that proves a failure arriving
/// after the call has begun cleaning up is still reported.
///
/// It exists because that failure cannot be provoked: a non-blocking pipe
/// offers a full buffer, an interruption, or a closed reader, and a closed
/// reader is deliberately not a failure. `cfg(test)` would not reach an
/// integration test, so this is a feature that is off by default — in an
/// ordinary build the function below is the one that compiles, and nothing
/// reads the environment.
#[cfg(feature = "test-support")]
fn injected_delivery_failure() -> Option<String> {
	std::env::var("RNX_TEST_DELIVERY_FAILS")
		.ok()
		.filter(|value| !value.is_empty())
		.map(|reason| format!("the input could not be delivered: {reason}"))
}

#[cfg(not(feature = "test-support"))]
fn injected_delivery_failure() -> Option<String> {
	None
}

/// Whether a failed write was cancelled rather than broken. Windows reaches a
/// write blocked in the kernel by cancelling it from outside, and reports
/// `ERROR_OPERATION_ABORTED`; on Unix nothing cancels a write, so nothing is
/// aborted.
fn aborted(e: &std::io::Error) -> bool {
	#[cfg(windows)]
	{
		// 995, ERROR_OPERATION_ABORTED.
		return e.raw_os_error() == Some(995);
	}
	#[cfg(not(windows))]
	{
		let _ = e;
		false
	}
}

/// What became of a child's input. A success reports that every byte was
/// written, and nothing here reports that the child read them.
enum DeliveryOutcome {
	Delivered,
	ChildStoppedReading,
	Abandoned,
	Failed(String),
}

/// What a child left behind, before anything decides whether it is text.
///
/// The two streams are captured separately and fall short separately, so each
/// carries its own flag. Only a stream's own flag can excuse that stream's
/// last character, and combining them before decoding would let a truncated
/// standard output excuse a standard error the child ended mid-character.
struct Ran {
	code: Option<i32>,
	timed_out: bool,
	cancelled: bool,
	out: Captured,
	err: Captured,
}

impl Ran {
	/// What a script is told, in each case: either stream falling short means
	/// the capture as a whole is not everything the child produced.
	///
	/// The three are independent — a stream can pass the cap and then be cut
	/// short and then fail to read — so they are three answers rather than
	/// one of four endings.
	fn truncated(&self) -> bool {
		self.out.truncated || self.err.truncated
	}
	fn cut_short(&self) -> bool {
		self.out.cut_short || self.err.cut_short
	}
	fn unreadable(&self) -> bool {
		self.out.unreadable || self.err.unreadable
	}
}

/// A captured stream as text, or the reason it is not.
///
/// A capture stops at a fixed size, which can fall inside a character, so an
/// incomplete sequence at the very end of a truncated capture is the
/// capture's doing rather than the child's: the partial character is dropped
/// and the shortfall is reported, which the caller already has to check. A
/// capture cut short by the cleanup instant earns the same exemption for the
/// same reason — the bytes stopped because rnx stopped reading.
///
/// **A stream whose read failed earns neither.** It is not known to have
/// stopped at a boundary or anywhere else, so being unreadable overrides both
/// exemptions rather than joining them. Anything else is the child's,
/// including an incomplete sequence at the end of a capture that finished.
///
/// Each stream is judged on **its own** flags: a truncated standard output has
/// never excused a standard error the child ended mid-character, and an
/// unreadable standard error must not refuse a standard output read whole.
fn decode(program: &str, stream: &str, got: &Captured) -> Result<String, String> {
	let bytes = &got.bytes[..];
	let stopped_by_rnx = (got.truncated || got.cut_short) && !got.unreadable;
	match std::str::from_utf8(bytes) {
		Ok(text) => Ok(text.to_owned()),
		Err(error) if stopped_by_rnx && error.error_len().is_none() => {
			let whole = &bytes[..error.valid_up_to()];
			Ok(std::str::from_utf8(whole)
				.expect("valid_up_to marks a valid prefix")
				.to_owned())
		}
		Err(error) => Err(format!(
			"cannot run {program}: its {stream} is not UTF-8 at byte {}; use host::process_bytes to read it",
			error.valid_up_to()
		)),
	}
}

fn process(program: &str, arguments: Value, timeout_ms: u64) -> Result<Value, String> {
	let ran = run_child(program, arguments, timeout_ms, None)?;
	// Each stream is decoded against its own flag. The record reports both
	// together, because a caller checking `truncated` wants to know that
	// something fell short, not which half did.
	let stdout = decode(program, "standard output", &ran.out)?;
	let stderr = decode(program, "standard error", &ran.err)?;
	json_parse(
		&serde_json::json!({
			"code": ran.code, "timed_out": ran.timed_out, "cancelled": ran.cancelled,
			"stdout": stdout, "stderr": stderr, "truncated": ran.truncated(),
			"cut_short": ran.cut_short(), "unreadable": ran.unreadable(),
		})
		.to_string(),
	)
}

/// The same child, with its streams exactly as they came.
fn process_bytes(program: &str, arguments: Value, timeout_ms: u64) -> Result<Value, String> {
	bytes_reply(run_child(program, arguments, timeout_ms, None)?)
}

/// As `process_bytes`, and the child is told what to do: `input` is written to
/// its standard input, which is then closed, because a child like
/// `git cat-file --batch` needs the end of its input to finish.
///
/// A success does not certify that every byte was consumed — a child may stop
/// reading, and that is its prerogative. What comes back is what the child
/// said and the status it exited with.
fn process_bytes_input(
	program: &str,
	arguments: Value,
	input: Value,
	timeout_ms: u64,
) -> Result<Value, String> {
	let bytes = input
		.borrow_ref::<rune::runtime::Bytes>()
		.map_err(|e| about("run", program, e))?
		.as_slice()
		.to_vec();
	bytes_reply(run_child(program, arguments, timeout_ms, Some(bytes))?)
}

/// What both byte-returning forms report, so the two cannot describe the same
/// child differently.
fn bytes_reply(ran: Ran) -> Result<Value, String> {
	let mut object = rune::runtime::Object::new();
	let mut put = |name: &str, value: Value| -> Result<(), String> {
		let key = rune::alloc::String::try_from(name).map_err(error)?;
		object.insert(key, value).map_err(error)?;
		Ok(())
	};
	// `process` reports the code through JSON, where a child that was killed
	// has none and arrives as unit. The two must agree field for field, so
	// this spells the same thing rather than an option.
	let code = match ran.code {
		Some(code) => rune::to_value(i64::from(code)).map_err(error)?,
		None => rune::to_value(()).map_err(error)?,
	};
	put("code", code)?;
	put("timed_out", rune::to_value(ran.timed_out).map_err(error)?)?;
	put("cancelled", rune::to_value(ran.cancelled).map_err(error)?)?;
	put("truncated", rune::to_value(ran.truncated()).map_err(error)?)?;
	put("cut_short", rune::to_value(ran.cut_short()).map_err(error)?)?;
	put(
		"unreadable",
		rune::to_value(ran.unreadable()).map_err(error)?,
	)?;
	let bytes = |raw: Vec<u8>| -> Result<Value, String> {
		let held = rune::alloc::Vec::try_from(raw).map_err(error)?;
		rune::to_value(rune::runtime::Bytes::from_vec(held)).map_err(error)
	};
	put("stdout", bytes(ran.out.bytes)?)?;
	put("stderr", bytes(ran.err.bytes)?)?;
	drop(put);
	rune::to_value(object).map_err(error)
}
/// A host function as registered: its path and the one-line description
/// carried beside it, so a function cannot exist without its help.
pub struct HostFunction {
	pub path: String,
	pub doc: &'static str,
}

/// Install the host module and return every function it registered, path and
/// description recorded at the registration itself so completion and `:help`
/// have no second list to keep in step.
pub fn install(context: &mut Context) -> super::Result<Vec<HostFunction>> {
	crate::platform::watch_for_interrupt();
	let mut module = Module::with_crate("host")?;
	let mut registered = Vec::new();
	macro_rules! register {
		($name:literal, $function:expr, $doc:literal) => {
			module.function($name, $function).build()?;
			registered.push(HostFunction {
				path: format!("host::{}", $name),
				doc: $doc,
			});
		};
	}
	register!(
		"json_parse",
		json_parse,
		"json_parse(text) -> Result<value>: parse JSON text into a Rune value"
	);
	register!(
		"json_stringify",
		json_stringify,
		"json_stringify(value) -> Result<String>: render a value as JSON text"
	);
	register!(
		"read",
		file_read,
		"read(path) -> Result<String>: the whole file as UTF-8, up to 8 MiB"
	);
	register!(
		"stdin",
		stdin_read,
		"stdin() -> Result<String>: the whole of standard input as UTF-8, up to 8 MiB; Err on a terminal or a second read"
	);
	register!(
		"exit",
		exit,
		"exit(code) -> Result<()>: end the script with this status, which must be 0 to 255; a status outside that ends the script with 1 rather than being truncated; in a script it never returns, and at a session prompt it is refused with Err"
	);
	register!(
		"eprint",
		eprint,
		"eprint(text) -> Result<()>: write text to standard error, adding nothing"
	);
	register!(
		"process_bytes",
		process_bytes,
		"process_bytes(program, args, timeout_ms) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated, cut_short, unreadable}>: as process, with the streams as byte strings and no decoding; for everything the child produced, check timed_out and cancelled first and then truncated, cut_short and unreadable"
	);
	register!(
		"process_bytes_input",
		process_bytes_input,
		"process_bytes_input(program, args, input, timeout_ms) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated, cut_short, unreadable}>: as process_bytes, writing the byte string `input` to the child's standard input and closing it; a success does not mean every byte was read; for everything the child produced, check timed_out and cancelled first and then truncated, cut_short and unreadable"
	);
	register!(
		"write_new",
		file_write,
		"write_new(path, text) -> Result<()>: write a new file, refusing to overwrite an existing one"
	);
	register!(
		"mkdir",
		mkdir,
		"mkdir(path) -> Result<()>: create one directory"
	);
	register!(
		"absolute",
		absolute,
		"absolute(path) -> Result<String>: canonicalize an existing path"
	);
	register!(
		"process",
		process,
		"process(program, args, timeout_ms) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated, cut_short, unreadable}>: run a child with a deadline, bounded capture, and cancellation on Ctrl-C; for everything the child produced, check timed_out and cancelled first and then truncated, cut_short and unreadable"
	);
	context.install(module)?;
	Ok(registered)
}

pub fn process_checks(context: &Context) -> super::Result<()> {
	for (label, source) in [
		(
			"exit status",
			r#"pub fn main(_) { host::process("/bin/sh", ["-c", "exit 7"], 1000)? }"#,
		),
		(
			"deadline",
			r#"pub fn main(_) { host::process("/bin/sh", ["-c", "sleep 10 & wait"], 40)? }"#,
		),
		(
			"capture cap",
			r#"pub fn main(_) { let r = host::process("/usr/bin/head", ["-c", "3000000", "/dev/zero"], 1000)?; (r.truncated, r.stdout.len()) }"#,
		),
		(
			"interruption",
			r#"pub fn main(_) { host::process("/bin/sh", ["-c", "kill -INT $PPID; sleep 10"], 1000)? }"#,
		),
	] {
		let result = super::call(context, source, Value::empty())?;
		let value = serde_json::to_value(&result)?;
		match label {
			"exit status" => assert_eq!(value["code"], 7),
			"deadline" => {
				assert_eq!(value["timed_out"], true);
				assert!(value["code"].is_null());
			}
			"capture cap" => assert_eq!(value, serde_json::json!([true, 2097152])),
			"interruption" => {
				assert_eq!(value["cancelled"], true);
				assert!(value["code"].is_null());
			}
			_ => unreachable!(),
		}
		println!("{label}: {}", super::json::stringify(&result)?);
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_wait_grows_with_the_child_and_never_outlives_the_deadline() {
		let far = Duration::from_secs(60);
		// A child that has just started is noticed at once.
		assert_eq!(next_wait(Duration::ZERO, far), SHORTEST_WAIT);
		// Then the pause asked for is an eighth of the child's life so far, so
		// after a `git cat-file` has run 1.4 ms the next request is 175 us.
		assert_eq!(
			next_wait(Duration::from_millis(8), far),
			Duration::from_millis(1)
		);
		assert_eq!(
			next_wait(Duration::from_micros(1_400), far),
			Duration::from_micros(175)
		);
		// And it stops growing at the interval this loop used for everything
		// before record 0020, reached after 40 ms.
		assert_eq!(next_wait(Duration::from_millis(40), far), LONGEST_WAIT);
		assert_eq!(next_wait(Duration::from_secs(60), far), LONGEST_WAIT);
		// The deadline wins whenever it is nearer than the wait would be.
		assert_eq!(
			next_wait(Duration::from_secs(60), Duration::from_micros(200)),
			Duration::from_micros(200)
		);
		assert_eq!(
			next_wait(Duration::ZERO, Duration::from_micros(10)),
			Duration::from_micros(10)
		);
		assert_eq!(
			next_wait(Duration::from_secs(60), Duration::ZERO),
			Duration::ZERO
		);
	}

	/// A capture with the shortfalls a test wants and nothing else.
	fn fell_short(bytes: &[u8], truncated: bool, cut_short: bool, unreadable: bool) -> Captured {
		Captured {
			bytes: bytes.to_vec(),
			truncated,
			cut_short,
			unreadable,
		}
	}

	#[test]
	fn a_shortfall_rnx_caused_excuses_a_partial_last_character() {
		// Two bytes of a three-byte character. Where rnx knows it stopped the
		// bytes — the size cap, or the cleanup instant — the partial
		// character is rnx's doing and is dropped; the caller is told about
		// the shortfall through its own flag.
		let cut = &[b'a', 0xe2, 0x82][..];
		assert_eq!(
			decode("p", "standard output", &fell_short(cut, true, false, false)).unwrap(),
			"a"
		);
		assert_eq!(
			decode("p", "standard output", &fell_short(cut, false, true, false)).unwrap(),
			"a"
		);
		// And a capture that simply finished mid-character is the child's
		// doing, which is refused as it always was.
		assert!(
			decode(
				"p",
				"standard output",
				&fell_short(cut, false, false, false)
			)
			.is_err()
		);
	}

	#[test]
	fn an_unreadable_stream_overrides_both_exemptions() {
		// A stream whose read failed is not known to have stopped at a
		// boundary or anywhere else, so it earns neither exemption even when
		// it also passed the cap or was cut short.
		let cut = &[b'a', 0xe2, 0x82][..];
		for (truncated, cut_short) in [(true, false), (false, true), (true, true)] {
			let got = fell_short(cut, truncated, cut_short, true);
			assert!(
				decode("p", "standard output", &got).is_err(),
				"unreadable was excused by truncated={truncated} cut_short={cut_short}"
			);
		}
		// Being unreadable does not refuse text that decoded cleanly: the
		// flag says the capture is short, not that what was read is wrong.
		let whole = &b"fine"[..];
		assert_eq!(
			decode(
				"p",
				"standard output",
				&fell_short(whole, false, false, true)
			)
			.unwrap(),
			"fine"
		);
	}

	#[test]
	fn each_stream_is_judged_on_its_own_flags() {
		// Record 0016's rule, carried through record 0023's two new flags: a
		// shortfall on one stream has never excused the other, and an
		// unreadable standard error must not refuse a standard output that
		// was read whole.
		let cut = &[b'a', 0xe2, 0x82][..];
		let out = fell_short(cut, true, false, false);
		let err = fell_short(cut, false, false, false);
		assert!(decode("p", "standard output", &out).is_ok());
		assert!(
			decode("p", "standard error", &err).is_err(),
			"a truncated standard output excused a standard error"
		);

		let whole_out = fell_short(&b"fine"[..], false, false, false);
		let broken_err = fell_short(cut, true, false, true);
		assert!(
			decode("p", "standard output", &whole_out).is_ok(),
			"an unreadable standard error refused a standard output read whole"
		);
		assert!(decode("p", "standard error", &broken_err).is_err());
	}

	fn absent() -> String {
		std::env::temp_dir()
			.join("rnx-host-no-such-path-24680")
			.to_string_lossy()
			.into_owned()
	}

	/// The first port could not tell which of several files was missing,
	/// because the error named none of them.
	#[test]
	fn every_error_that_touches_a_path_names_it() {
		let path = absent();
		let read = file_read(&path).unwrap_err();
		assert!(read.contains(&path), "{read}");
		assert!(read.starts_with("cannot read "), "{read}");

		let nested = format!("{path}/inner/deeper");
		let made = mkdir(&nested).unwrap_err();
		assert!(made.contains(&nested), "{made}");
		assert!(made.starts_with("cannot create directory "), "{made}");

		let resolved = absolute(&path).unwrap_err();
		assert!(resolved.contains(&path), "{resolved}");
		assert!(resolved.starts_with("cannot resolve "), "{resolved}");

		// Writing refuses to overwrite, so an existing file is the failure.
		let existing = std::env::temp_dir().join(format!("rnx-host-exists-{}", std::process::id()));
		std::fs::write(&existing, "there").unwrap();
		let existing = existing.to_string_lossy().into_owned();
		let written = file_write(&existing, "again").unwrap_err();
		assert!(written.contains(&existing), "{written}");
		assert!(written.starts_with("cannot write "), "{written}");
		let _ = std::fs::remove_file(&existing);
	}

	/// Four of the six ways `process` can fail. The two that remain, a failure
	/// of `try_wait` or of `wait` on a child already running, are named in the
	/// code but are not reached here: they need the operating system to fail a
	/// wait on a live child, which no fixture can arrange reliably.
	#[test]
	fn the_reachable_ways_running_a_program_can_fail_name_the_program() {
		let named = |failure: String| {
			assert!(failure.starts_with("cannot run "), "{failure}");
			assert!(failure.contains("rnx-no-such-program-13579"), "{failure}");
		};
		let arguments = rune::to_value(rune::runtime::Vec::new()).unwrap();
		named(process("rnx-no-such-program-13579", arguments, 1000).unwrap_err());
		// A deadline outside the accepted range.
		let arguments = rune::to_value(rune::runtime::Vec::new()).unwrap();
		named(process("rnx-no-such-program-13579", arguments, 0).unwrap_err());
		// Arguments that are not a vector of strings.
		named(
			process(
				"rnx-no-such-program-13579",
				rune::to_value(7i64).unwrap(),
				1000,
			)
			.unwrap_err(),
		);
		let mixed = rune::to_value(vec![rune::to_value(7i64).unwrap()]).unwrap();
		named(process("rnx-no-such-program-13579", mixed, 1000).unwrap_err());
	}

	#[test]
	fn a_file_past_the_limit_names_itself_too() {
		let path = std::env::temp_dir().join(format!("rnx-host-large-{}", std::process::id()));
		std::fs::write(&path, vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
		let path = path.to_string_lossy().into_owned();
		let failure = file_read(&path).unwrap_err();
		assert!(failure.contains(&path), "{failure}");
		assert!(failure.contains("8 MiB limit"), "{failure}");
		let _ = std::fs::remove_file(&path);
	}
}
