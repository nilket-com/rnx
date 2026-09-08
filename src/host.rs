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
fn json_stringify(value: Value) -> Result<String, String> {
	serde_json::to_string(&value).map_err(error)
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
fn process(program: &str, arguments: Value, timeout_ms: u64) -> Result<Value, String> {
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
	json_parse(
		&serde_json::json!({
			"code": status.code(), "timed_out": timed_out, "cancelled": cancelled,
			"stdout": String::from_utf8_lossy(&out), "stderr": String::from_utf8_lossy(&err),
			"truncated": out_truncated || err_truncated,
		})
		.to_string(),
	)
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
		"json_parse(text) -> value: parse JSON text into a Rune value, or Err"
	);
	register!(
		"json_stringify",
		json_stringify,
		"json_stringify(value) -> String: render a value as JSON text, or Err"
	);
	register!(
		"read",
		file_read,
		"read(path) -> String: the whole file as UTF-8, up to 8 MiB, or Err"
	);
	register!(
		"write_new",
		file_write,
		"write_new(path, text): write a new file, refusing to overwrite, or Err"
	);
	register!("mkdir", mkdir, "mkdir(path): create one directory, or Err");
	register!(
		"absolute",
		absolute,
		"absolute(path) -> String: canonicalize an existing path, or Err"
	);
	register!(
		"process",
		process,
		"process(program, args, timeout_ms) -> #{code, stdout, stderr, timed_out, cancelled, truncated}: run a child with a deadline, bounded capture, and cancellation on Ctrl-C"
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
		println!("{label}: {}", super::display(&result));
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
