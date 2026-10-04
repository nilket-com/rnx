//! Pure source transformation. The CLI runs this only in a bounded worker.
use rune::ast::{self, Spanned};
use rune::parse::Parser;
use rune::{Source, SourceId, Sources};
use std::ops::Range;

type Error = Box<dyn std::error::Error>;

#[derive(Debug)]
struct Token {
	kind: ast::Kind,
	span: Range<usize>,
	text: String,
}

fn tokens(source: &str) -> Result<Vec<Token>, Error> {
	let mut parser = Parser::new(source, SourceId::empty(), true);
	let mut out = Vec::new();
	let mut template = None;
	let mut depth = 0usize;
	while !parser.is_eof()? {
		let token = parser.parse::<ast::Token>()?;
		let mut span = token.span.range();
		let mut kind = token.kind;
		if matches!(token.kind, ast::Kind::Open(ast::Delimiter::Empty))
			&& &source[span.clone()] == "`"
		{
			if depth == 0 {
				template = Some(span.start);
			}
			depth += 1;
			continue;
		}
		if depth > 0 {
			if matches!(token.kind, ast::Kind::Close(ast::Delimiter::Empty))
				&& &source[span.clone()] == "`"
			{
				depth -= 1;
				if depth == 0 {
					span.start = template.take().ok_or("missing template start")?;
					kind = ast::Kind::TemplateString;
				} else {
					continue;
				}
			} else {
				continue;
			}
		}
		if let Some(previous) = out
			.last_mut()
			.filter(|p: &&mut Token| span.start < p.span.end)
		{
			previous.span.end = previous.span.end.max(span.end);
			previous.text = source[previous.span.clone()].to_owned();
		} else if !span.is_empty() {
			out.push(Token {
				kind,
				text: source[span.clone()].to_owned(),
				span,
			});
		}
	}
	Ok(out)
}

fn comments(source: &str, tokens: &[Token]) -> Result<Vec<Range<usize>>, Error> {
	let mut ranges = Vec::new();
	let mut end = 0;
	for (next, token_end) in tokens
		.iter()
		.map(|t| (t.span.start, t.span.end))
		.chain(std::iter::once((source.len(), source.len())))
	{
		if next < end {
			return Err("overlapping source token spans".into());
		}
		let mut at = end;
		while at < next {
			let rest = &source[at..next];
			if rest.starts_with("//") || rest.starts_with("#!") {
				let n = rest.find('\n').unwrap_or(rest.len());
				let content = n - usize::from(rest[..n].ends_with('\r'));
				ranges.push(at..at + content);
				at += n;
			} else if rest.starts_with("/*") {
				let start = at;
				at += 2;
				let mut depth = 1;
				while at < next && depth != 0 {
					if source[at..next].starts_with("/*") {
						depth += 1;
						at += 2;
					} else if source[at..next].starts_with("*/") {
						depth -= 1;
						at += 2;
					} else {
						at += source[at..]
							.chars()
							.next()
							.ok_or("comment ends unexpectedly")?
							.len_utf8();
					}
				}
				if depth != 0 {
					return Err("unterminated comment".into());
				}
				ranges.push(start..at);
			} else {
				let c = rest.chars().next().ok_or("missing trivia")?;
				if !c.is_whitespace() {
					return Err(format!(
						"unrecognized source trivia at {at}: {:?}",
						rest.chars().take(40).collect::<String>()
					)
					.into());
				}
				at += c.len_utf8();
			}
		}
		end = token_end;
	}
	Ok(ranges)
}

fn file(source: &str) -> Result<ast::File, Error> {
	Ok(Parser::new(source, SourceId::empty(), true).parse_all::<ast::File>()?)
}

fn function_gaps(file: &ast::File, out: &mut Vec<(usize, usize)>) {
	let mut previous = None;
	for (item, _) in &file.items {
		if let ast::Item::Fn(f) = item {
			if let Some(end) = previous {
				out.push((end, f.span().range().start));
			}
			previous = Some(f.span().range().end);
		} else {
			previous = None;
		}
		match item {
			ast::Item::Mod(m) => {
				if let ast::ItemModBody::InlineBody(b) = &m.body {
					function_gaps(&b.file, out);
				}
			}
			ast::Item::Impl(i) => {
				for pair in i.functions.windows(2) {
					out.push((pair[0].span().range().end, pair[1].span().range().start));
				}
			}
			_ => (),
		}
	}
}

fn inserted_commas(original: &[Token], rendered: &[Token]) -> Result<Vec<Range<usize>>, Error> {
	let mut index = 0;
	let mut extra_commas = Vec::new();
	for token in rendered {
		if original.get(index).is_some_and(|t| {
			std::mem::discriminant(&t.kind) == std::mem::discriminant(&token.kind)
				&& t.text == token.text
		}) {
			index += 1;
		} else if token.kind == ast::Kind::Comma && token.text == "," {
			extra_commas.push(token.span.clone());
		} else {
			return Err("formatter changed a non-whitespace token".into());
		}
	}
	if index != original.len() {
		return Err("formatter removed source tokens".into());
	}
	Ok(extra_commas)
}

fn once(source: &str) -> Result<String, Error> {
	file(source)?;
	let original = tokens(source)?;
	let original_comments = comments(source, &original)?;
	let mut sources = Sources::new();
	sources.insert(Source::new("formatter", source)?)?;
	let options = rune::Options::default();
	let formatted = rune::fmt::prepare(&sources)
		.with_options(&options)
		.format()?;
	let mut output = formatted
		.into_iter()
		.next()
		.ok_or("formatter produced no source")?
		.1
		.to_string();
	let rendered = tokens(&output)?;
	let extra_commas = inserted_commas(&original, &rendered)?;
	for span in extra_commas.into_iter().rev() {
		output.replace_range(span, "");
	}
	let rendered = tokens(&output)?;
	let rendered_comments = comments(&output, &rendered)?;
	if original_comments
		.iter()
		.map(|r| &source[r.clone()])
		.ne(rendered_comments.iter().map(|r| &output[r.clone()]))
	{
		return Err("formatter changed comment content or order".into());
	}
	let mut gaps = Vec::new();
	function_gaps(&file(&output)?, &mut gaps);
	let mut edits = Vec::new();
	for (end, start) in gaps {
		let first_comment = rendered_comments
			.iter()
			.find(|c| c.start >= end && c.end <= start && output[end..c.start].contains('\n'))
			.map(|c| c.start);
		let item = first_comment.unwrap_or(start);
		let line = output[..item].rfind('\n').map_or(0, |n| n + 1);
		if line < end {
			if output[end..start].chars().all(char::is_whitespace) {
				edits.push((end..start, "\n\n".to_owned()));
			}
			continue;
		}
		let prefix = &output[end..line];
		if prefix.chars().all(char::is_whitespace) {
			edits.push((end..line, "\n\n".to_owned()));
		}
	}
	// A prefix crossing a token or comment belongs to its contents, not layout.
	let mut protected: Vec<_> = rendered
		.iter()
		.map(|t| t.span.clone())
		.chain(rendered_comments)
		.collect();
	protected.sort_by_key(|r| r.start);
	let mut offset = 0;
	for line in output.split_inclusive('\n') {
		let spaces = line.bytes().take_while(|b| *b == b' ').count();
		if spaces >= 4
			&& !protected
				.get(
					protected
						.partition_point(|r| r.start < offset)
						.wrapping_sub(1),
				)
				.is_some_and(|r| offset < r.end)
		{
			edits.push((
				offset..offset + spaces,
				format!("{}{}", "\t".repeat(spaces / 4), " ".repeat(spaces % 4)),
			));
		}
		offset += line.len();
	}
	edits.sort_by_key(|(r, _)| r.start);
	let mut laid_out = String::with_capacity(output.len());
	let mut cursor = 0;
	for (span, replacement) in edits {
		if span.start < cursor {
			return Err("overlapping layout edits".into());
		}
		laid_out.push_str(&output[cursor..span.start]);
		laid_out.push_str(&replacement);
		cursor = span.end;
	}
	laid_out.push_str(&output[cursor..]);
	output = laid_out;
	file(&output)?;
	if tokens(&output)?
		.iter()
		.map(|t| (std::mem::discriminant(&t.kind), &t.text))
		.ne(original
			.iter()
			.map(|t| (std::mem::discriminant(&t.kind), &t.text)))
	{
		return Err("layout changed tokens".into());
	}
	Ok(output)
}

fn format(source: &str) -> Result<String, Error> {
	let out = once(source)?;
	if once(&out)? != out {
		return Err("formatting is not idempotent".into());
	}
	Ok(out)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn fidelity_refuses_token_and_literal_edits_and_keeps_original_commas() {
		let original = tokens("fn a(){let x=[1,2];let s=\"hello\";}").unwrap();
		for changed in [
			"fn a(){let x=[1,3];let s=\"hello\";}",
			"fn a(){let x=[1,2];let s=\"bye\";}",
			"fn a(){let x=[1,2];let s=\"hello\"}",
		] {
			assert!(inserted_commas(&original, &tokens(changed).unwrap()).is_err());
		}
		let extra = tokens("fn a(){let x=[1,2,];let s=\"hello\";}").unwrap();
		assert_eq!(inserted_commas(&original, &extra).unwrap().len(), 1);
		assert!(inserted_commas(&extra, &original).is_err());
	}
	#[test]
	fn protocol_binds_length_and_request() {
		let bytes = packet(42, b"source");
		assert_eq!(unpack(&bytes, Some(42)).unwrap().1, b"source");
		assert!(unpack(&bytes[..23], Some(42)).is_err());
		assert!(unpack(&bytes, Some(43)).is_err());
		let mut truncated = bytes.clone();
		truncated.pop();
		assert!(unpack(&truncated, Some(42)).is_err());
		let mut extra = bytes.clone();
		extra.push(0);
		assert!(unpack(&extra, Some(42)).is_err());
		let mut bad = bytes.clone();
		bad[0] = 0;
		assert!(unpack(&bad, Some(42)).is_err());
	}
	#[test]
	fn dense_functions_use_tabs_and_blank_lines() {
		let out = format("fn a(){let x=1;println!(\"{}\",x);} fn b(){a();}").unwrap();
		assert!(out.contains("\n\tlet x = 1;"), "{out}");
		assert!(out.contains("}\n\nfn b"), "{out}");
		assert_eq!(format(&out).unwrap(), out);
	}
	#[test]
	fn templates_comments_and_nested_functions_are_preserved() {
		for source in [
			"// top\nfn a(){let t=`hello ${1} world`;} // tail\n/// docs\nfn b(){}",
			"mod m{fn a(){}fn b(){vec![1,2,3];}} impl T{fn a(self){}fn b(self){}}",
			"fn a(){println!(\"hi\");} fn b(){let s=b\"x\"; let c='x';}",
		] {
			let out = format(source).unwrap();
			assert_eq!(format(&out).unwrap(), out);
		}
	}
	#[test]
	fn literal_whitespace_is_preserved() {
		let out = format("fn a(){let s=\"first\n    second\";} fn b(){}").unwrap();
		assert!(out.contains("\"first\n    second\""));
	}
}

const INPUT: usize = 1024 * 1024;
const OUTPUT: usize = 4 * 1024 * 1024;

/// Dispatch before any runtime or Context exists, including assembled hosts.
pub(crate) fn dispatch() -> Option<i32> {
	let args: Vec<_> = std::env::args_os().skip(1).collect();
	if args.first().is_some_and(|s| s == "--rnx-fmt-worker") {
		return Some(match worker(args.get(1).and_then(|s| s.to_str())) {
			Ok(()) => 0,
			Err(e) => {
				eprintln!(
					"{}",
					crate::format::terminal_safe(
						&e.to_string().chars().take(1000).collect::<String>()
					)
				);
				2
			}
		});
	}
	#[cfg(all(feature = "test-support", target_os = "linux"))]
	if args.first().is_some_and(|s| s == "--rnx-fmt-test") {
		if args
			.get(1)
			.is_some_and(|s| s == "fail-publish" || s == "edit-target")
		{
			FAIL_PUBLISH.store(
				args[1] == "fail-publish",
				std::sync::atomic::Ordering::Relaxed,
			);
			EDIT_TARGET.store(
				args[1] == "edit-target",
				std::sync::atomic::Ordering::Relaxed,
			);
			return Some(match cli(&args[2..]) {
				Ok(status) => status,
				Err(e) => {
					eprintln!("{e}");
					2
				}
			});
		}
		let result = (|| -> Result<String, Error> {
			let control = args
				.get(1)
				.and_then(|s| s.to_str())
				.ok_or("missing formatter control")?;
			if args.len() != 2 {
				return Err("test control takes one mode".into());
			}
			isolated_with(
				bounded_read(std::io::stdin().lock(), INPUT)?,
				Some(control),
				OUTPUT,
			)
		})();
		return Some(match result {
			Ok(s) => {
				print!("{s}");
				0
			}
			Err(e) => {
				eprintln!("{e}");
				2
			}
		});
	}
	if !args.first().is_some_and(|s| s == "fmt") {
		return None;
	}
	#[cfg(target_os = "linux")]
	return Some(match cli(&args[1..]) {
		Ok(status) => status,
		Err(e) => {
			eprintln!(
				"rnx fmt: {}",
				crate::format::terminal_safe(&e.to_string().chars().take(1000).collect::<String>())
			);
			2
		}
	});
	#[cfg(not(target_os = "linux"))]
	Some({
		eprintln!("rnx fmt: bounded formatter is currently available on Linux only");
		2
	})
}

fn bounded_read(reader: impl std::io::Read, cap: usize) -> Result<Vec<u8>, Error> {
	use std::io::Read;
	let mut data = Vec::new();
	reader.take((cap + 1) as u64).read_to_end(&mut data)?;
	if data.len() > cap {
		return Err("byte limit exceeded".into());
	}
	Ok(data)
}

#[cfg(feature = "test-support")]
static FAIL_PUBLISH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "test-support")]
static EDIT_TARGET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "test-support")]
pub(crate) static CONTEXTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn fingerprint(bytes: &[u8]) -> u64 {
	use std::hash::{Hash, Hasher};
	let mut hash = std::collections::hash_map::DefaultHasher::new();
	bytes.hash(&mut hash);
	hash.finish()
}
fn packet(hash: u64, payload: &[u8]) -> Vec<u8> {
	let mut out = Vec::with_capacity(24 + payload.len());
	out.extend_from_slice(b"RNXFMT1\n");
	out.extend_from_slice(&hash.to_le_bytes());
	out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
	out.extend_from_slice(payload);
	out
}
fn unpack(bytes: &[u8], expected: Option<u64>) -> Result<(u64, &[u8]), Error> {
	if bytes.len() < 24 || &bytes[..8] != b"RNXFMT1\n" {
		return Err("malformed formatter protocol header".into());
	}
	let hash = u64::from_le_bytes(bytes[8..16].try_into()?);
	let len = u64::from_le_bytes(bytes[16..24].try_into()?);
	if len != (bytes.len() - 24) as u64 || expected.is_some_and(|h| h != hash) {
		return Err("formatter protocol length or request binding mismatch".into());
	}
	Ok((hash, &bytes[24..]))
}
fn reply(hash: u64, bytes: &[u8]) -> Result<(), Error> {
	use std::io::Write;
	if bytes.len() > OUTPUT {
		return Err("formatted output exceeds 4 MiB".into());
	}
	std::io::stdout().lock().write_all(&packet(hash, bytes))?;
	Ok(())
}

fn worker(control: Option<&str>) -> Result<(), Error> {
	#[cfg(feature = "test-support")]
	use std::io::Write;
	let bytes = bounded_read(std::io::stdin().lock(), INPUT + 24)?;
	let (hash, payload) = unpack(&bytes, None)?;
	if fingerprint(payload) != hash {
		return Err("formatter request fingerprint mismatch".into());
	}
	let source = std::str::from_utf8(payload)?;
	#[cfg(feature = "test-support")]
	if let Some(control) = control {
		match control {
			"timeout" => std::thread::sleep(std::time::Duration::from_secs(10)),
			"panic" => panic!("formatter worker test panic"),
			"malformed" => {
				std::io::stdout().lock().write_all(b"garbage")?;
				return Ok(());
			}
			"overflow" => {
				std::io::stdout()
					.lock()
					.write_all(&vec![b'x'; OUTPUT + 25])?;
				return Ok(());
			}
			"allocation" | "completion" => {
				let mut sources = Sources::new();
				sources.insert(Source::new("upstream-reproducer", source)?)?;
				let output = rune::fmt::prepare(&sources)
					.with_options(&rune::Options::default())
					.format()?;
				return reply(hash, output[0].1.as_bytes());
			}
			"lean" => {
				let status = std::fs::read_to_string("/proc/self/status")?;
				let threads = status
					.lines()
					.find(|l| l.starts_with("Threads:"))
					.ok_or("no thread count")?
					.split_whitespace()
					.nth(1)
					.ok_or("no thread value")?;
				let report = format!(
					"{{\"threads\":{threads},\"contexts\":{}}}\n",
					CONTEXTS.load(std::sync::atomic::Ordering::Relaxed)
				);
				return reply(hash, report.as_bytes());
			}
			_ => return Err("unknown formatter test control".into()),
		}
	}
	#[cfg(not(feature = "test-support"))]
	if control.is_some() {
		return Err("formatter worker takes no arguments".into());
	}
	let output = format(source)?;
	if output.len() > OUTPUT {
		return Err("formatted output exceeds 4 MiB".into());
	}
	reply(hash, output.as_bytes())
}

#[cfg(target_os = "linux")]
fn isolated(input: Vec<u8>) -> Result<String, Error> {
	isolated_with(input, None, OUTPUT)
}

#[cfg(target_os = "linux")]
fn isolated_with(
	input: Vec<u8>,
	control: Option<&str>,
	output_cap: usize,
) -> Result<String, Error> {
	use std::{
		io::Write,
		os::unix::process::CommandExt,
		process::{Command, Stdio},
		sync::mpsc,
		time::{Duration, Instant},
	};
	let mut cmd = Command::new(std::env::current_exe()?);
	cmd.arg("--rnx-fmt-worker")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	if let Some(control) = control {
		cmd.arg(control);
	}
	unsafe {
		cmd.pre_exec(|| {
			for (resource, value) in [
				(libc::RLIMIT_AS, 512 * 1024 * 1024),
				(libc::RLIMIT_STACK, 8 * 1024 * 1024),
				(libc::RLIMIT_CPU, 2),
			] {
				let limit = libc::rlimit {
					rlim_cur: value,
					rlim_max: value,
				};
				if libc::setrlimit(resource, &limit) != 0 {
					return Err(std::io::Error::last_os_error());
				}
			}
			Ok(())
		});
	}
	let hash = fingerprint(&input);
	let input = packet(hash, &input);
	let began = Instant::now();
	struct Reaped(std::process::Child);
	impl std::ops::Deref for Reaped {
		type Target = std::process::Child;
		fn deref(&self) -> &Self::Target {
			&self.0
		}
	}
	impl std::ops::DerefMut for Reaped {
		fn deref_mut(&mut self) -> &mut Self::Target {
			&mut self.0
		}
	}
	impl Drop for Reaped {
		fn drop(&mut self) {
			let _ = self.0.kill();
			let _ = self.0.wait();
		}
	}
	let mut child = Reaped(cmd.spawn()?);
	let mut stdin = child.stdin.take().ok_or("worker stdin missing")?;
	let stdout = child.stdout.take().ok_or("worker stdout missing")?;
	let stderr = child.stderr.take().ok_or("worker stderr missing")?;
	let writer = std::thread::spawn(move || stdin.write_all(&input));
	let (tx, rx) = mpsc::channel();
	let reader = std::thread::spawn(move || {
		let result = bounded_read(stdout, output_cap + 24).map_err(|e| e.to_string());
		let _ = tx.send(result);
	});
	let errors = std::thread::spawn(move || bounded_read(stderr, 4096).map_err(|e| e.to_string()));
	let status = loop {
		if let Some(status) = child.try_wait()? {
			break status;
		}
		if began.elapsed() >= Duration::from_secs(5) {
			let _ = child.kill();
			let _ = child.wait();
			let _ = writer.join();
			let _ = reader.join();
			let _ = errors.join();
			return Err("formatter worker exceeded 5-second deadline (killed and reaped)".into());
		}
		std::thread::sleep(Duration::from_millis(5));
	};
	let write = writer.join().map_err(|_| "worker input thread panicked")?;
	reader.join().map_err(|_| "worker output thread panicked")?;
	let errors = errors
		.join()
		.map_err(|_| "worker error thread panicked")??;
	if !status.success() {
		return Err(format!(
			"formatter worker {status}: {}",
			String::from_utf8_lossy(&errors)
		)
		.into());
	}
	write?;
	let bytes = rx.recv()??;
	let (_, payload) = unpack(&bytes, Some(hash))?;
	Ok(std::str::from_utf8(payload)?.to_owned())
}

#[cfg(target_os = "linux")]
fn open(path: &std::path::Path) -> Result<std::fs::File, Error> {
	use std::os::unix::fs::OpenOptionsExt;
	let file = std::fs::OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
		.open(path)?;
	if !file.metadata()?.is_file() {
		return Err("input must be a regular file".into());
	}
	Ok(file)
}

#[cfg(target_os = "linux")]
fn cli(args: &[std::ffi::OsString]) -> Result<i32, Error> {
	use std::{
		io::Write,
		os::unix::fs::{MetadataExt, PermissionsExt},
		path::PathBuf,
	};
	let mut check = false;
	let mut stdin = false;
	let mut literal = false;
	let mut paths = Vec::new();
	for arg in args {
		if !literal && arg == "--" {
			literal = true;
		} else if !literal && arg == "--check" {
			if check {
				return Err("duplicate --check".into());
			}
			check = true;
		} else if !literal && arg == "--stdin" {
			if stdin {
				return Err("duplicate --stdin".into());
			}
			stdin = true;
		} else if !literal && arg == "--help" {
			if args.len() != 1 {
				return Err("--help takes no other arguments".into());
			}
			println!(
				"rnx fmt FILE... | --check FILE... | --stdin\nTabs for indentation; blank lines between functions. No source execution.\nExplicit regular UTF-8 files only; -- ends options. Linux only.\nLimits: 128 files, 1 MiB input/file, 16 MiB total; 4 MiB output/file, 32 MiB total.\nExit: 0 success, 1 check found differences, 2 error."
			);
			return Ok(0);
		} else if !literal && arg.as_encoded_bytes().starts_with(b"-") {
			return Err("unknown formatter option (use -- before dash-prefixed files)".into());
		} else {
			paths.push(PathBuf::from(arg));
		}
	}
	if stdin {
		if check || !paths.is_empty() {
			return Err("--stdin cannot be combined with --check or files".into());
		}
		let bytes = bounded_read(std::io::stdin().lock(), INPUT)?;
		std::str::from_utf8(&bytes)?;
		std::io::stdout()
			.lock()
			.write_all(isolated(bytes)?.as_bytes())?;
		return Ok(0);
	}
	if paths.is_empty() || paths.len() > 128 {
		return Err("want 1 to 128 explicit files".into());
	}
	struct Input {
		path: PathBuf,
		bytes: Vec<u8>,
		meta: std::fs::Metadata,
		output: String,
	}
	let mut files = Vec::new();
	let mut identities = std::collections::HashSet::new();
	let mut total = 0usize;
	for path in paths {
		let file = open(&path).map_err(|e| format!("{}: {e}", path_label(&path)))?;
		let meta = file.metadata()?;
		if !identities.insert((meta.dev(), meta.ino())) {
			return Err("duplicate file identity".into());
		}
		if !check && meta.nlink() != 1 {
			return Err("in-place formatting refuses hard-linked files".into());
		}
		if meta.len() > INPUT as u64 {
			return Err("input exceeds 1 MiB".into());
		}
		let remaining = 16 * 1024 * 1024 - total;
		if meta.len() > remaining as u64 {
			return Err("total input exceeds 16 MiB".into());
		}
		let bytes = bounded_read(file, INPUT.min(remaining))?;
		std::str::from_utf8(&bytes)?;
		total = total
			.checked_add(bytes.len())
			.ok_or("input byte overflow")?;
		if total > 16 * 1024 * 1024 {
			return Err("total input exceeds 16 MiB".into());
		}
		files.push(Input {
			path,
			bytes,
			meta,
			output: String::new(),
		});
	}
	let mut total = 0usize;
	for file in &mut files {
		file.output = isolated_with(
			file.bytes.clone(),
			None,
			OUTPUT.min(32 * 1024 * 1024 - total),
		)
		.map_err(|e| format!("{}: {e}", path_label(&file.path)))?;
		total = total
			.checked_add(file.output.len())
			.ok_or("output byte overflow")?;
		if total > 32 * 1024 * 1024 {
			return Err("total output exceeds 32 MiB".into());
		}
	}
	if check {
		let mut changed = false;
		for file in &files {
			if file.bytes != file.output.as_bytes() {
				changed = true;
				println!(
					"needs formatting: {}",
					crate::format::terminal_safe(
						&file
							.path
							.to_string_lossy()
							.chars()
							.take(200)
							.collect::<String>()
					)
				);
			}
		}
		return Ok(i32::from(changed));
	}
	struct Stages(Vec<PathBuf>);
	impl Drop for Stages {
		fn drop(&mut self) {
			for path in &self.0 {
				let _ = std::fs::remove_file(path);
			}
		}
	}
	let mut stages = Stages(Vec::new());
	let mut targets = Vec::new();
	for (i, file) in files.iter().enumerate() {
		if file.bytes == file.output.as_bytes() {
			continue;
		}
		let path = file
			.path
			.with_file_name(format!(".rnx-fmt-{}-{i}", std::process::id()));
		let mut stage = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(&path)?;
		stages.0.push(path);
		stage.write_all(file.output.as_bytes())?;
		stage.set_permissions(std::fs::Permissions::from_mode(file.meta.mode()))?;
		stage.sync_all()?;
		targets.push(file);
	}
	for (index, (stage, file)) in stages.0.iter().zip(targets).enumerate() {
		#[cfg(feature = "test-support")]
		if index == 1 && FAIL_PUBLISH.load(std::sync::atomic::Ordering::Relaxed) {
			return Err(format!(
				"injected publication failure after {index} replacements (listed on stdout)"
			)
			.into());
		}
		#[cfg(feature = "test-support")]
		if index == 0 && EDIT_TARGET.load(std::sync::atomic::Ordering::Relaxed) {
			let mut changed = file.bytes.clone();
			changed.extend_from_slice(b"\n// concurrent edit\n");
			std::fs::write(&file.path, changed)?;
		}

		let current = open(&file.path)?;
		let meta = current.metadata()?;
		if meta.dev() != file.meta.dev()
			|| meta.ino() != file.meta.ino()
			|| meta.mode() != file.meta.mode()
			|| meta.nlink() != 1
			|| bounded_read(current, INPUT)? != file.bytes
		{
			return Err(format!(
				"{}: target changed since reading; {index} files already replaced (listed on stdout)",
				path_label(&file.path)
			)
			.into());
		}
		std::fs::rename(stage, &file.path).map_err(|e| {
			format!(
				"{}: publication failed after {index} replacements (listed on stdout): {e}",
				path_label(&file.path)
			)
		})?;
		println!("formatted: {}", path_label(&file.path));
	}
	Ok(0)
}

#[cfg(target_os = "linux")]
fn path_label(path: &std::path::Path) -> String {
	crate::format::terminal_safe(&path.to_string_lossy().chars().take(200).collect::<String>())
}
