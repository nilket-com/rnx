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
fn run_child(program: &str, arguments: Value, timeout_ms: u64) -> Result<Ran, String> {
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
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0)
		.spawn()
		.map_err(|e| about("run", program, e))?;
	let pid = child.id() as i32;
	let stdout = child.stdout.take().unwrap();
	let stderr = child.stderr.take().unwrap();
	let out = std::thread::spawn(move || capture(stdout));
	let err = std::thread::spawn(move || capture(stderr));
	let deadline = Instant::now() + Duration::from_millis(timeout_ms);
	let mut timed_out = false;
	let mut cancelled = false;
	let status = loop {
		if let Some(status) = child.try_wait().map_err(|e| about("run", program, e))? {
			break status;
		}
		cancelled = INTERRUPTED.load(Ordering::Relaxed);
		timed_out = Instant::now() >= deadline;
		if timed_out || cancelled {
			unsafe {
				libc::kill(-pid, libc::SIGKILL);
			}
			break child.wait().map_err(|e| about("run", program, e))?;
		}
		std::thread::sleep(Duration::from_millis(5));
	};
	// Children remaining in this process group may otherwise keep pipes open.
	unsafe {
		libc::kill(-pid, libc::SIGKILL);
	}
	let (out, out_truncated) = out.join().map_err(|_| "stdout reader panicked")?;
	let (err, err_truncated) = err.join().map_err(|_| "stderr reader panicked")?;
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
	let ran = run_child(program, arguments, timeout_ms)?;
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
	let ran = run_child(program, arguments, timeout_ms)?;
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
