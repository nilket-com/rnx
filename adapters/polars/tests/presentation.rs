//! Record 0068, the product matrix: a wrapper-shaped binary driven as the
//! notebook worker. A bare `DataFrame` presents exactly the explicit preview
//! for every oracle shape; lazy values stay opaque and unexecuted; the frame
//! is unchanged by presentation; explicit formatting agrees.
#![cfg(target_os = "linux")]
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

fn pipe() -> (OwnedFd, OwnedFd) {
	let mut fds = [0; 2];
	assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
	unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

/// The smallest JSON reader this test needs: one flat object of strings,
/// numbers, booleans and null per settled reply.
fn field<'a>(reply: &'a str, name: &str) -> Option<&'a str> {
	let key = format!("\"{name}\":");
	let at = reply.find(&key)? + key.len();
	let rest = &reply[at..];
	if let Some(inner) = rest.strip_prefix('"') {
		let mut escaped = false;
		for (i, c) in inner.char_indices() {
			match c {
				'\\' if !escaped => escaped = true,
				'"' if !escaped => return Some(&inner[..i]),
				_ => escaped = false,
			}
		}
		None
	} else {
		let end = rest.find([',', '}']).unwrap_or(rest.len());
		Some(&rest[..end])
	}
}
/// JSON string unescaping for a settled reply's `text_plain`.
fn json_unescape(s: &str) -> String {
	let mut out = String::new();
	let mut chars = s.chars();
	while let Some(c) = chars.next() {
		if c != '\\' {
			out.push(c);
			continue;
		}
		match chars.next().unwrap() {
			'n' => out.push('\n'),
			'r' => out.push('\r'),
			't' => out.push('\t'),
			'"' => out.push('"'),
			'\\' => out.push('\\'),
			'\'' => out.push('\''),
			'/' => out.push('/'),
			'u' => {
				let hex: String = chars.by_ref().take(4).collect();
				let unit = u32::from_str_radix(&hex, 16).unwrap();
				if (0xd800..0xdc00).contains(&unit) {
					assert_eq!((chars.next(), chars.next()), (Some('\\'), Some('u')));
					let low: String = chars.by_ref().take(4).collect();
					let low = u32::from_str_radix(&low, 16).unwrap();
					out.push(
						char::from_u32(0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00)).unwrap(),
					);
				} else {
					out.push(char::from_u32(unit).unwrap());
				}
			}
			other => panic!("unknown escape \\{other}"),
		}
	}
	out
}

struct Worker {
	child: Child,
	send: std::fs::File,
	receive: BufReader<std::fs::File>,
	next: u64,
	dir: PathBuf,
}
impl Worker {
	fn spawn() -> Self {
		static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
		let dir = std::env::temp_dir().join(format!(
			"rnx-polars-presentation-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
		));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		let (child_read, parent_write) = pipe();
		let (parent_read, child_write) = pipe();
		let (r, w) = (child_read.as_raw_fd(), child_write.as_raw_fd());
		let mut command = Command::new(env!("CARGO_BIN_EXE_rnx-polars-fixture"));
		command
			.args([
				"worker",
				"--control-read",
				&r.to_string(),
				"--control-write",
				&w.to_string(),
			])
			.current_dir(&dir)
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped());
		unsafe {
			command.pre_exec(move || {
				for fd in [r, w] {
					let flags = libc::fcntl(fd, libc::F_GETFD);
					if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
						return Err(std::io::Error::last_os_error());
					}
				}
				Ok(())
			});
		}
		let child = command.spawn().unwrap();
		drop((child_read, child_write));
		let mut worker = Self {
			child,
			send: parent_write.into(),
			receive: BufReader::new(parent_read.into()),
			next: 0,
			dir,
		};
		assert_eq!(field(&worker.reply(), "type"), Some("ready"));
		worker
	}
	fn reply(&mut self) -> String {
		let mut line = String::new();
		self.receive.read_line(&mut line).unwrap();
		assert!(!line.is_empty(), "worker closed its control pipe");
		line
	}
	fn operate(&mut self, op: &str, source: Option<&str>) -> String {
		self.next += 1;
		let mut request = format!(
			"{{\"op\":\"{op}\",\"id\":{},\"nonce\":\"{}\"",
			self.next,
			"ab".repeat(32)
		);
		if let Some(source) = source {
			request += &format!(",\"source\":{}", json_quote(source));
		}
		request += "}";
		writeln!(self.send, "{request}").unwrap();
		loop {
			let reply = self.reply();
			if field(&reply, "type") == Some("settled") {
				writeln!(self.send, "{{\"op\":\"ack\",\"id\":{}}}", self.next).unwrap();
				return reply;
			}
		}
	}
	/// A cell's `text_plain`, with any failure reported as a test failure.
	fn text(&mut self, source: &str) -> String {
		let reply = self.operate("execute", Some(source));
		assert_eq!(field(&reply, "failure"), Some("null"), "{source}: {reply}");
		match field(&reply, "text_plain") {
			Some("null") | None => String::new(),
			Some(text) => json_unescape(text),
		}
	}
	fn csv(&self, name: &str, text: &str) {
		std::fs::write(self.dir.join(name), text).unwrap();
	}
}
impl Drop for Worker {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
		let _ = std::fs::remove_dir_all(&self.dir);
	}
}
fn json_quote(s: &str) -> String {
	let mut out = String::from("\"");
	for c in s.chars() {
		match c {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\n' => out.push_str("\\n"),
			c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
			c => out.push(c),
		}
	}
	out + "\""
}

const BYTES: usize = 8192;
const OMITTED: &str = "\n[preview byte limit; remainder omitted]\n";

#[test]
fn every_oracle_shape_presents_exactly_the_explicit_preview() {
	let mut w = Worker::spawn();
	let crabs = "🦀".repeat(81);
	let long_row: String = (0..8)
		.map(|_| "🦀".repeat(80))
		.collect::<Vec<_>>()
		.join(",");
	let cases: Vec<(&str, String, &str)> = vec![
		("empty", "a,b\n".into(), r#"[("a","string"),("b","i64")]"#),
		("narrow", "a\nx\n".into(), r#"[("a","string")]"#),
		(
			"wide",
			format!(
				"{}\n{}\n",
				(0..12)
					.map(|i| format!("c{i}"))
					.collect::<Vec<_>>()
					.join(","),
				(0..12).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
			),
			WIDE_SCHEMA,
		),
		(
			"tall",
			format!(
				"n\n{}",
				(0..15).map(|i| format!("{i}\n")).collect::<String>()
			),
			r#"[("n","i64")]"#,
		),
		(
			"nulls",
			"s,n,f,b\n,,,\nx,1,1.5,true\n".into(),
			r#"[("s","string"),("n","i64"),("f","f64"),("b","bool")]"#,
		),
		("long-utf8", format!("s\n{crabs}\n"), r#"[("s","string")]"#),
		(
			"controls",
			"s\n\"esc\u{1b}[2J tab\t nl\nend quote\"\"\"\n".into(),
			r#"[("s","string")]"#,
		),
		(
			"bidi",
			"s\n\u{202e}reversed\u{2066}isolate\u{200f}\n".into(),
			r#"[("s","string")]"#,
		),
		(
			"floats",
			"f\n1e308\n-0.0\n5e-324\n0.1\n-1.7976931348623157e308\n".into(),
			r#"[("f","f64")]"#,
		),
		(
			"byte-limit",
			format!(
				"{}\n{}",
				(0..8)
					.map(|i| format!("c{i}"))
					.collect::<Vec<_>>()
					.join(","),
				format!("{long_row}\n").repeat(10)
			),
			LIMIT_SCHEMA,
		),
	];
	let mut report = Vec::new();
	for (name, csv, schema) in cases {
		w.csv(&format!("{name}.csv"), &csv);
		assert_eq!(
			w.text(&format!(
				"let f = polars::read_csv(\"{name}.csv\", {schema})?;"
			)),
			""
		);
		let automatic = w.text("f");
		// The explicit preview leaves the worker as a file: rendering a string
		// value cuts it at the worker's string limit, which is below the
		// adapter's byte cap.
		assert_eq!(
			w.text(&format!(
				"fs::write_new(\"{name}.preview\", f.preview()?)?;"
			)),
			""
		);
		let explicit = std::fs::read_to_string(w.dir.join(format!("{name}.preview"))).unwrap();
		assert_eq!(
			automatic, explicit,
			"{name}: automatic differs from preview()"
		);
		assert_eq!(
			w.text("format!(\"{f}\") == f.preview()?"),
			"true",
			"{name}: format! differs from preview()"
		);
		assert!(
			automatic.len() <= BYTES,
			"{name}: {} bytes",
			automatic.len()
		);
		assert!(
			!automatic.chars().any(|c| c.is_control() && c != '\n'),
			"{name}: raw control"
		);
		assert!(automatic.starts_with("DataFrame: "), "{name}: {automatic}");
		report.push((name, automatic.len(), automatic.ends_with(OMITTED)));
		// A second presentation is byte-identical and the frame is unchanged.
		assert_eq!(w.text("f"), automatic, "{name}: not deterministic");
		assert_eq!(
			w.text(&format!("fs::write_new(\"{name}.again\", f.preview()?)?;")),
			""
		);
		assert_eq!(
			std::fs::read_to_string(w.dir.join(format!("{name}.again"))).unwrap(),
			explicit,
			"{name}: frame changed"
		);
	}
	let shape = |name: &str| report.iter().find(|r| r.0 == name).unwrap();
	assert!(shape("byte-limit").2 && shape("byte-limit").1 <= BYTES);
	assert!(!shape("empty").2 && !shape("bidi").2);
	// Specific spellings the oracle fixes.
	w.csv("nulls.csv", "s,n,f,b\n,,,\n");
	w.text(r#"let f = polars::read_csv("nulls.csv", [("s","string"),("n","i64"),("f","f64"),("b","bool")])?;"#);
	let text = w.text("f");
	assert!(text.contains("null | null | null"), "{text}");
	w.text(r#"let f = polars::read_csv("long-utf8.csv", [("s","string")])?;"#);
	let text = w.text("f");
	assert_eq!(text.matches('🦀').count(), 80);
	assert!(text.contains("…[truncated]"), "{text}");
	w.text(r#"let f = polars::read_csv("controls.csv", [("s","string")])?;"#);
	let text = w.text("f");
	assert!(
		text.contains("\\u{1b}[2J tab\\t nl\\nend quote\\\""),
		"{text}"
	);
	w.text(r#"let f = polars::read_csv("bidi.csv", [("s","string")])?;"#);
	let text = w.text("f");
	// Embeddings, overrides and isolates are escaped; a bare directional
	// mark (U+200F) is not an embedding control and passes as the preview's
	// existing policy allows.
	assert!(
		text.contains("\\u{202e}reversed\\u{2066}isolate\u{200f}"),
		"{text}"
	);
	// Controls in a column name are escaped like controls in a cell.
	w.csv("control-names.csv", "\"a\u{1b}[2Jb\tc\"\n1\n");
	w.text(r#"let f = polars::read_csv("control-names.csv", [("a\u{1b}[2Jb\tc","i64")])?;"#);
	let text = w.text("f");
	assert!(text.contains("\"a\\u{1b}[2Jb\\tc\": i64\n1\n"), "{text}");
	assert!(!text.contains('\u{1b}') && !text.contains('\t'));
	w.text(r#"let f = polars::read_csv("floats.csv", [("f","f64")])?;"#);
	let text = w.text("f");
	for spelled in ["1e308", "-0.0", "5e-324", "0.1", "-1.7976931348623157e308"] {
		assert!(
			text.contains(&format!("\n{spelled}\n")),
			"{spelled} in {text}"
		);
	}
	w.text(r#"let f = polars::read_csv("wide.csv", [("c0","i64"),("c1","i64"),("c2","i64"),("c3","i64"),("c4","i64"),("c5","i64"),("c6","i64"),("c7","i64"),("c8","i64"),("c9","i64"),("c10","i64"),("c11","i64")])?;"#);
	let text = w.text("f");
	assert!(
		text.contains("[0 rows and 4 columns omitted by display limits]") && !text.contains("c8"),
		"{text}"
	);
	w.text(r#"let f = polars::read_csv("tall.csv", [("n","i64")])?;"#);
	let text = w.text("f");
	assert!(
		text.contains("[5 rows and 0 columns omitted by display limits]")
			&& text.matches('\n').count() == 13,
		"{text}"
	);
	w.operate("shutdown", None);
	assert_eq!(w.child.wait().unwrap().code(), Some(0));
}

static WIDE_SCHEMA: &str = r#"[("c0","i64"),("c1","i64"),("c2","i64"),("c3","i64"),("c4","i64"),("c5","i64"),("c6","i64"),("c7","i64"),("c8","i64"),("c9","i64"),("c10","i64"),("c11","i64")]"#;
static LIMIT_SCHEMA: &str = r#"[("c0","string"),("c1","string"),("c2","string"),("c3","string"),("c4","string"),("c5","string"),("c6","string"),("c7","string")]"#;

#[test]
fn lazy_values_stay_opaque_and_unexecuted_and_presentation_runs_no_engine() {
	let mut w = Worker::spawn();
	w.csv("sales.csv", "item,qty\napple,3\npear,1\n");
	w.text(r#"let sales = polars::read_csv("sales.csv", [("item","string"),("qty","i64")])?;"#);
	let started = |w: &mut Worker| {
		w.text("polars::engine_counts().0")
			.parse::<usize>()
			.unwrap()
	};
	let before = started(&mut w);
	// Presenting a frame starts no engine thread: presentation is separate from collecting.
	assert!(
		w.text("sales")
			.starts_with("DataFrame: 2 rows × 2 columns\n")
	);
	assert_eq!(started(&mut w), before);
	// A plan with a missing column, displayed bare, is not executed.
	assert_eq!(
		w.text(r#"let plan = sales.lazy().filter(polars::col("missing").gt(polars::lit(1)?));"#),
		""
	);
	assert_eq!(w.text("plan"), "<::polars::LazyFrame>");
	assert_eq!(w.text("sales.lazy()"), "<::polars::LazyFrame>");
	assert_eq!(w.text(r#"polars::col("qty")"#), "<::polars::Expr>");
	// The pinned crate version (adapters/polars/Cargo.toml); a bump must change both.
	assert_eq!(w.text("polars::version()"), "\"0.55.2\"");
	assert_eq!(
		w.text(r#"sales.lazy().group_by([polars::col("item")])?"#),
		"<::polars::LazyGroupBy>"
	);
	assert_eq!(started(&mut w), before);
	// Collecting it is what runs it, and that is where the error surfaces.
	assert_eq!(w.text("plan.collect().is_err()"), "true");
	assert_eq!(started(&mut w), before + 1);
	// Containers and results keep the generic rendering.
	assert_eq!(w.text("[sales]"), "[<::polars::DataFrame>]");
	assert_eq!(
		w.text("sales.lazy().collect()"),
		"Ok(<::polars::DataFrame>)"
	);
	assert_eq!(w.text("(sales, 1)"), "(<::polars::DataFrame>, 1)");
	// Explicit formatting of a lazy value has no display protocol.
	let reply = w.operate("execute", Some("format!(\"{plan}\")"));
	assert_ne!(field(&reply, "failure"), Some("null"), "{reply}");
	assert!(w.text("sales").starts_with("DataFrame: 2 rows"));
	w.operate("shutdown", None);
	assert_eq!(w.child.wait().unwrap().code(), Some(0));
}
