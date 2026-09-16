//! Record 0046: control never shares a stream with script output.
use crate::session::{Failure, Session};
use serde_json::{Value, json};
use std::{
	fs::File,
	io::{self, BufRead, BufReader, Write},
};

pub const FRAME_CAP: usize = 256 * 1024;
const MAX_ID: u64 = (1_u64 << 53) - 1;
pub struct Transport {
	read: BufReader<File>,
	write: File,
}
impl Transport {
	pub fn from_args(args: &[String]) -> Result<Self, (i32, String)> {
		if args.len() != 5 || args[1] != "--control-read" || args[3] != "--control-write" {
			return Err((2, "worker needs --control-read N --control-write N".into()));
		}
		let number = |s: &str| {
			s.parse::<usize>().map_err(|_| {
				(
					2,
					"worker control endpoints must be unsigned integers".into(),
				)
			})
		};
		let (read, write) = crate::worker_transport::open(number(&args[2])?, number(&args[4])?)
			.map_err(|e| (1, format!("cannot open worker control: {e}")))?;
		Ok(Self {
			read: BufReader::new(read),
			write,
		})
	}
	fn receive(&mut self) -> io::Result<Value> {
		let mut line = Vec::new();
		loop {
			let chunk = self.read.fill_buf()?;
			if chunk.is_empty() {
				return Err(io::Error::new(
					io::ErrorKind::UnexpectedEof,
					"worker control closed before shutdown",
				));
			}
			let end = chunk.iter().position(|b| *b == b'\n');
			let n = end.unwrap_or(chunk.len());
			if line.len() + n > FRAME_CAP {
				return Err(io::Error::other(
					"worker control frame exceeds 262144 bytes",
				));
			}
			line.extend_from_slice(&chunk[..n]);
			self.read.consume(n + usize::from(end.is_some()));
			if end.is_some() {
				return serde_json::from_slice(&line).map_err(io::Error::other);
			}
		}
	}
	fn send(&mut self, value: Value) -> io::Result<()> {
		// Every variable field is bounded before this encoding. Never truncate JSON.
		let bytes = serde_json::to_vec(&value)?;
		if bytes.len() > FRAME_CAP {
			return Err(io::Error::other("worker reply exceeds control bound"));
		}
		self.write.write_all(&bytes)?;
		self.write.write_all(b"\n")?;
		self.write.flush()
	}
}
fn object<'a>(v: &'a Value, keys: &[&str]) -> io::Result<&'a serde_json::Map<String, Value>> {
	let o = v
		.as_object()
		.ok_or_else(|| io::Error::other("worker request must be an object"))?;
	if o.len() != keys.len() || o.keys().any(|k| !keys.contains(&k.as_str())) {
		return Err(io::Error::other(
			"worker request has missing or unknown fields",
		));
	}
	Ok(o)
}
fn marker(id: u64, stream: &str, nonce: &str, mut out: impl Write) -> io::Result<()> {
	out.flush()?;
	write!(out, "\x1eRNX-WORKER-1:{id}:{stream}:{nonce}\x1f")?;
	out.flush()
}
/// A formatting sink stops before allocating a large diagnostic transcript.
struct Bounded {
	text: String,
	truncated: bool,
	cap: usize,
}
impl std::fmt::Write for Bounded {
	fn write_str(&mut self, s: &str) -> std::fmt::Result {
		let mut n = s.len().min(self.cap - self.text.len());
		while !s.is_char_boundary(n) {
			n -= 1;
		}
		self.text.push_str(&s[..n]);
		if n < s.len() {
			self.truncated = true;
			Err(std::fmt::Error)
		} else {
			Ok(())
		}
	}
}
fn bounded(value: impl std::fmt::Display, cap: usize) -> (String, bool) {
	let mut sink = Bounded {
		text: String::new(),
		truncated: false,
		cap,
	};
	let _ = std::fmt::write(&mut sink, format_args!("{value}"));
	(sink.text, sink.truncated)
}
fn failure(f: &Failure, epoch: u64) -> Value {
	// Cap a script-derived message before Display escapes it. The source line
	// is already bounded by INPUT_CAP; a panic message need not be.
	let (prepared, message_cut) = match f {
		Failure::Runtime { message, origin } | Failure::Compile { message, origin } => {
			let (message, cut) = bounded(message, 4096);
			let replacement = if matches!(f, Failure::Runtime { .. }) {
				Failure::Runtime {
					message,
					origin: origin.clone(),
				}
			} else {
				Failure::Compile {
					message,
					origin: origin.clone(),
				}
			};
			(Some(replacement), cut)
		}
		_ => (None, false),
	};
	let (diagnostic, mut truncated) = bounded(prepared.as_ref().unwrap_or(f), 16 * 1024);
	truncated |= message_cut;
	let (category, origin) = match f {
		Failure::Refused(_) => ("refused", None),
		Failure::Compile { origin, .. } => ("compile", origin.as_ref()),
		Failure::Runtime { origin, .. } => ("runtime", origin.as_ref()),
		Failure::Interrupted => ("interrupted", None),
		Failure::Budget(_) => ("budget", None),
		Failure::OverCeiling { .. } => ("over_ceiling", None),
	};
	let mut answer = json!({"category":category,"diagnostic":diagnostic,"diagnostic_truncated":truncated,"origin":null});
	if let Some(o) = origin {
		let (excerpt, cut) = bounded(crate::format::terminal_safe(&o.text), 4096);
		answer["origin"] = json!({"epoch":epoch,"input":o.input,"line":o.line,"column":o.column,"excerpt":excerpt,"excerpt_truncated":cut});
	}
	match f {
		Failure::Budget(n) => answer["budget"] = json!(n),
		Failure::OverCeiling { live, ceiling } => {
			answer["live"] = json!(live);
			answer["ceiling"] = json!(ceiling);
		}
		_ => {}
	}
	answer
}
pub fn run(
	mut transport: Transport,
	context: rune::Context,
	http: crate::http::State,
	lifecycle: crate::lifecycle::Lifecycle,
) -> io::Result<()> {
	let mut session = Session::with_ceiling(context, crate::repl::ceiling())
		.map_err(|e| io::Error::other(e.to_string()))?
		.with_http(http)
		.with_lifecycle(lifecycle);
	crate::memory::record_baseline();
	session.sample();
	transport.send(json!({"type":"ready","protocol":1,"rnx":crate::VERSION,"rune":crate::RUNE_VERSION,
        "bounds":{"control_bytes":FRAME_CAP,"source_bytes":crate::session::INPUT_CAP,"render_bytes":16384,"stream_bytes":2097152,"pending_stream_bytes":2097152,"unassociated_stream_bytes":2097152,"parent_wait_ms":5000,"budget":crate::session::BUDGET}}))?;
	let mut epoch = 1_u64;
	let mut last = 0;
	loop {
		let request = transport.receive()?;
		let op = request["op"]
			.as_str()
			.ok_or_else(|| io::Error::other("worker operation must be a string"))?;
		match op {
			"execute" => {
				object(&request, &["id", "op", "source", "nonce"])?;
			}
			"reset" | "shutdown" => {
				object(&request, &["id", "op", "nonce"])?;
			}
			_ => return Err(io::Error::other("unknown worker operation")),
		}
		let id = request["id"]
			.as_u64()
			.filter(|n| *n > last && *n <= MAX_ID)
			.ok_or_else(|| {
				io::Error::other("worker id must increase within 1..=9007199254740991")
			})?;
		let nonce = request["nonce"]
			.as_str()
			.filter(|n| {
				n.len() == 64
					&& n.bytes()
						.all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
			})
			.ok_or_else(|| {
				io::Error::other("worker nonce needs 64 lowercase hexadecimal digits")
			})?;
		last = id;
		let mut reply = json!({"type":"settled","id":id,"epoch":epoch,"input":null,"text_plain":null,"failure":null,"state_lost":false});
		let mut retire = op == "shutdown";
		let mut cleanup_failed = false;
		if op == "execute" {
			let source = request["source"]
				.as_str()
				.ok_or_else(|| io::Error::other("worker source must be a string"))?;
			let result = session.eval_with_armed(source, |input| {
				reply["input"] = json!(input);
				transport.send(json!({"type":"armed","id":id,"epoch":epoch,"input":input}))
			})?;
			match result {
				Ok(value) if !crate::runner::is_unit(&value) => {
					reply["text_plain"] = json!(crate::format::render(
						&value,
						Some(&session.fields()),
						&crate::format::Limits::default()
					));
					reply["render_bounded"] = json!(true);
				}
				Ok(_) => {}
				Err(f) => reply["failure"] = failure(&f, epoch),
			}
			if session.lifecycle_failed() {
				reply["state_lost"] = json!(true);
				cleanup_failed = true;
				retire = true;
			}
		} else {
			if let Err(message) = session.reset_fallible() {
				reply["failure"] = failure(
					&Failure::Runtime {
						message,
						origin: None,
					},
					epoch,
				);
				reply["state_lost"] = json!(true);
				cleanup_failed = true;
				retire = true;
			} else if op == "reset" {
				epoch += 1;
				reply["epoch"] = json!(epoch);
			}
		}
		test_pause("stdout");
		marker(id, "stdout", nonce, std::io::stdout().lock())?;
		test_pause("stderr");
		marker(id, "stderr", nonce, std::io::stderr().lock())?;
		test_pause("settled");
		transport.send(reply)?;
		let ack = transport.receive()?;
		object(&ack, &["op", "id"])?;
		if ack["op"] != "ack" || ack["id"].as_u64() != Some(id) {
			return Err(io::Error::other(
				"worker requires matching acknowledgement before another operation",
			));
		}
		if retire {
			return if !cleanup_failed {
				Ok(())
			} else {
				Err(io::Error::other(
					"worker retired after reset cleanup failure",
				))
			};
		}
		// As in the REPL, sample after disposable per-input data is dropped,
		// before reading/admitting the next input. Reset samples the cleared state.
		drop(request);
		drop(ack);
		session.sample();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn diagnostics_are_capped_on_character_boundaries() {
		let (s, cut) = bounded("é".repeat(20), 9);
		assert_eq!(s, "éééé");
		assert!(cut);
	}
	#[test]
	fn a_flush_failure_cannot_produce_a_barrier() {
		struct Broken;
		impl Write for Broken {
			fn write(&mut self, _: &[u8]) -> io::Result<usize> {
				panic!("must flush before marker")
			}
			fn flush(&mut self) -> io::Result<()> {
				Err(io::Error::other("injected flush failure"))
			}
		}
		assert!(marker(1, "stdout", &"a".repeat(64), Broken).is_err());
	}
	#[test]
	fn unknown_keys_are_not_ignored() {
		assert!(object(&json!({"op":"ack","id":1,"source":"42"}), &["op", "id"]).is_err());
	}
}

// Test-only rendezvous: the parent kills the worker at each partial boundary.
#[cfg(feature = "test-support")]
fn test_pause(phase: &str) {
	if std::env::var("RNX_TEST_WORKER_PAUSE").ok().as_deref() == Some(phase) {
		if let Some(path) = std::env::var_os("RNX_TEST_WORKER_PHASE_FILE") {
			std::fs::write(path, phase).expect("worker phase fixture");
			loop {
				std::thread::sleep(std::time::Duration::from_millis(10));
			}
		}
	}
}
#[cfg(not(feature = "test-support"))]
fn test_pause(_: &str) {}
