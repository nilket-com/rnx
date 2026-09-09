//! Minimal trusted-local host for the spike, not a proposed standard library.
use rune::{Context, Module, runtime::Value};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
extern "C" fn interrupt(_: libc::c_int) {
	INTERRUPTED.store(true, Ordering::Relaxed);
}
/// Whether Ctrl-C arrived since the flag was last cleared.
pub fn interrupted() -> bool {
	INTERRUPTED.load(Ordering::Relaxed)
}
/// Cleared by the session before each evaluation, so an interrupt that
/// arrived at the prompt never cancels the next input.
pub fn clear_interrupt() {
	INTERRUPTED.store(false, Ordering::Relaxed);
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
	if unsafe { libc::isatty(libc::STDIN_FILENO) } == 1 {
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
fn capture(mut input: impl Read) -> (Vec<u8>, bool) {
	let mut captured = Vec::new();
	let mut chunk = [0; 8192];
	let mut truncated = false;
	while let Ok(n) = input.read(&mut chunk) {
		if n == 0 {
			break;
		}
		let keep = n.min((2 * 1024 * 1024usize).saturating_sub(captured.len()));
		captured.extend_from_slice(&chunk[..keep]);
		truncated |= keep < n;
	}
	(captured, truncated)
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
	use std::os::unix::process::CommandExt;
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
	let mut child = Command::new(program)
		.args(args)
		.stdin(if input.is_some() {
			Stdio::piped()
		} else {
			Stdio::null()
		})
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0)
		.spawn()
		.map_err(|e| about("run", program, e))?;
	let pid = child.id() as i32;
	let deadline = Instant::now() + Duration::from_millis(timeout_ms);
	let stdout = child.stdout.take().unwrap();
	let stderr = child.stderr.take().unwrap();
	let out = std::thread::spawn(move || capture(stdout));
	let err = std::thread::spawn(move || capture(stderr));
	// The input moves on a third thread, so requests are delivered while
	// replies and complaints are drained: what that removes is mutual pipe
	// backpressure, not every way a child can fail to finish.
	//
	// The delivery gets a stop flag of its own, per call. `INTERRUPTED` is the
	// whole process's and the deadline is a time; neither says "this call is
	// finished with you", which is what has to be said when the child exits
	// with input still undelivered.
	let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
	let delivery = input.map(|bytes| {
		let pipe = child.stdin.take().expect("stdin was piped for an input");
		let stop = stop.clone();
		std::thread::spawn(move || deliver(pipe, bytes, deadline, stop))
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
		cancelled = INTERRUPTED.load(Ordering::Relaxed);
		timed_out = Instant::now() >= deadline;
		if timed_out || cancelled {
			unsafe {
				libc::kill(-pid, libc::SIGKILL);
			}
			break child.wait().map_err(|e| about("run", program, e));
		}
		// No sleep outlives the deadline: overshooting it by a whole interval
		// would report a timeout later than the caller asked for.
		let left = deadline.saturating_duration_since(Instant::now());
		std::thread::sleep(next_wait(started.elapsed(), left));
	};
	// Children remaining in this process group may otherwise keep pipes open.
	// A descendant that left the group — `setsid` — is not killed here, and if
	// it holds a **captured** pipe the capture joins below wait for it however
	// long it lives: record 0020's limitation, carried with a reproducer
	// rather than a test. Holding the read end of standard input no longer
	// does that, which is what record 0022 added.
	unsafe {
		libc::kill(-pid, libc::SIGKILL);
	}
	// The wait is over, however it ended — including badly — so the delivery is
	// told to stop and is then **always** collected. It cannot outlast this
	// call: the pipe is non-blocking and the flag is read before every
	// attempt, so the thread returns within one of its own waits. Detaching it
	// instead — which an earlier draft did — would discard whatever it had to
	// say, including a failure this promises to report.
	stop.store(true, Ordering::Relaxed);
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
	let (out, out_truncated) = out.join().map_err(|_| "stdout reader panicked")?;
	let (err, err_truncated) = err.join().map_err(|_| "stderr reader panicked")?;
	// Now that everything is stopped and collected, a failure can be reported.
	let status = waited?;
	if let Some(failure) = delivery_failed {
		return Err(failure);
	}
	Ok(Ran {
		code: status.code(),
		timed_out,
		cancelled,
		out,
		out_truncated,
		err,
		err_truncated,
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
	pipe: std::process::ChildStdin,
	bytes: Vec<u8>,
	deadline: Instant,
	stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> DeliveryOutcome {
	use std::os::fd::AsRawFd;
	let fd = pipe.as_raw_fd();
	unsafe {
		let flags = libc::fcntl(fd, libc::F_GETFL);
		if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
			return DeliveryOutcome::Failed(std::io::Error::last_os_error().to_string());
		}
	}
	let mut pipe = pipe;
	let mut written = 0;
	let started = Instant::now();
	while written < bytes.len() {
		// Before every attempt, not only after one that would block: a write
		// that can proceed must not be started once the call has finished with
		// this delivery, or the flag would only be honoured while blocked.
		if stop.load(Ordering::Relaxed)
			|| Instant::now() >= deadline
			|| INTERRUPTED.load(Ordering::Relaxed)
		{
			// The one place a failure can be injected, and it is here because
			// here is the moment that matters: the call has begun its cleanup
			// and a failure arriving now must still be collected.
			if let Some(reason) = injected_delivery_failure() {
				return DeliveryOutcome::Failed(reason);
			}
			return DeliveryOutcome::Abandoned;
		}
		match pipe.write(&bytes[written..]) {
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
	out: Vec<u8>,
	out_truncated: bool,
	err: Vec<u8>,
	err_truncated: bool,
}

impl Ran {
	/// What the record reports: either stream falling short means the capture
	/// as a whole is not everything the child produced.
	fn truncated(&self) -> bool {
		self.out_truncated || self.err_truncated
	}
}

/// A captured stream as text, or the reason it is not.
///
/// A capture stops at a fixed size, which can fall inside a character, so an
/// incomplete sequence at the very end of a truncated capture is the
/// capture's doing rather than the child's: the partial character is dropped
/// and the truncation is reported, which the caller already has to check.
/// Anything else is the child's, including an incomplete sequence at the end
/// of a capture that was not truncated.
fn decode(program: &str, stream: &str, bytes: &[u8], truncated: bool) -> Result<String, String> {
	match std::str::from_utf8(bytes) {
		Ok(text) => Ok(text.to_owned()),
		Err(error) if truncated && error.error_len().is_none() => {
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
	let stdout = decode(program, "standard output", &ran.out, ran.out_truncated)?;
	let stderr = decode(program, "standard error", &ran.err, ran.err_truncated)?;
	json_parse(
		&serde_json::json!({
			"code": ran.code, "timed_out": ran.timed_out, "cancelled": ran.cancelled,
			"stdout": stdout, "stderr": stderr, "truncated": ran.truncated(),
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
	let bytes = |raw: Vec<u8>| -> Result<Value, String> {
		let held = rune::alloc::Vec::try_from(raw).map_err(error)?;
		rune::to_value(rune::runtime::Bytes::from_vec(held)).map_err(error)
	};
	put("stdout", bytes(ran.out)?)?;
	put("stderr", bytes(ran.err)?)?;
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
	unsafe {
		libc::signal(libc::SIGINT, interrupt as *const () as libc::sighandler_t);
	}
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
		"process_bytes(program, args, timeout_ms) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated}>: as process, with the streams as byte strings and no decoding"
	);
	register!(
		"process_bytes_input",
		process_bytes_input,
		"process_bytes_input(program, args, input, timeout_ms) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated}>: as process_bytes, writing the byte string `input` to the child's standard input and closing it; a success does not mean every byte was read"
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
		"process(program, args, timeout_ms) -> Result<#{code, stdout, stderr, timed_out, cancelled, truncated}>: run a child with a deadline, bounded capture, and cancellation on Ctrl-C"
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
