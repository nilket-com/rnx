//! Bounded diagnostic copies of a failure; never acceptance evidence.
//!
//! A failure keeps its cause through release: the failing receipt's
//! reviewed fields and an excerpt of its logs, sanitized of environment
//! values, invocation dumps and credential options, within fixed size
//! bounds.
use crate::pyjson;
use crate::receipt::{atomic_json, digest, sync_directory};
use anyhow::{Context as _, Result, bail, ensure};
use regex::Regex;
use regex::bytes::Regex as BytesRegex;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const FILE_CAP: usize = 64 * 1024;
pub const ERROR_CAP: usize = 16 * 1024;
pub const BUNDLE_CAP: usize = 256 * 1024;
pub const FINAL_CAP: usize = 1024 * 1024;
pub const INDEX_RESERVE: usize = 32 * 1024;

pub type Environment = BTreeMap<String, String>;

const NON_SECRET_ENV: [&str; 23] = [
	"HOME",
	"PWD",
	"OLDPWD",
	"USER",
	"LOGNAME",
	"PATH",
	"SHELL",
	"LANG",
	"LANGUAGE",
	"TERM",
	"COLORTERM",
	"CARGO_HOME",
	"RUSTUP_HOME",
	"SHLVL",
	"TMPDIR",
	"TMP",
	"TEMP",
	"TZ",
	"XDG_RUNTIME_DIR",
	"XDG_CACHE_HOME",
	"XDG_CONFIG_HOME",
	"XDG_DATA_HOME",
	"XDG_STATE_HOME",
];

static INVOCATION_DUMP: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"(?i)^\s*(?:[\w.]+:\s*)?(?:(?:argv|command|execve|environment|environ)\s*[:=(\[]|command\s+['"]?\[)|^\s*\[['"]"#).unwrap()
});
static CREDENTIAL_FLAG: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"(?i)(?P<flag>--(?:(?:\w+[-_])*(?:token|key|secret|password|passwd|auth|authorization|credentials?)(?:[-_]\w+)*|api[-_]?key|access[-_]?token|client[-_]?secret))(?P<separator>=|[\t\x20]+)(?P<value>"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s,;]+)"#).unwrap()
});
static TEST_SECTION: LazyLock<BytesRegex> =
	LazyLock::new(|| BytesRegex::new(r"(?-u)^---- .+ (?:stdout|stderr) ----\s*$").unwrap());
static CAUSE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"^(?:cross verification:|error(?:\[[^\]\n]+\])?:|thread .+ panicked|(?:AssertionError|ValueError|RuntimeError|OSError|TypeError):)").unwrap()
});

/// Python's `str.splitlines(keepends=True)`.
pub fn split_lines(text: &str) -> Vec<&str> {
	let mut lines = Vec::new();
	let mut start = 0;
	let mut chars = text.char_indices().peekable();
	while let Some((i, c)) = chars.next() {
		let end = match c {
			'\r' => match chars.peek() {
				Some((_, '\n')) => {
					chars.next();
					i + 2
				}
				_ => i + 1,
			},
			'\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
			| '\u{2029}' => i + c.len_utf8(),
			_ => continue,
		};
		lines.push(&text[start..end]);
		start = end;
	}
	if start < text.len() {
		lines.push(&text[start..]);
	}
	lines
}

/// Keep diagnostic numbers, locations and option prose. Recognizable
/// invocation dumps go; a credential option loses only its value; every
/// environment value that could be a secret is replaced.
pub fn sanitize(text: &str, environment: &Environment) -> String {
	let mut out = String::with_capacity(text.len());
	for line in split_lines(text) {
		if INVOCATION_DUMP.is_match(line) {
			out.push_str("<redacted invocation detail>\n");
		} else {
			out.push_str(&CREDENTIAL_FLAG.replace_all(line, "${flag}${separator}<redacted>"));
		}
	}
	let paths: Vec<&str> = environment
		.get("PATH")
		.map_or(vec![""], |p| p.split(':').collect());
	let mut secrets: Vec<&String> = environment
		.iter()
		.filter(|(key, value)| {
			!value.is_empty()
				&& !NON_SECRET_ENV.contains(&key.as_str())
				&& !key.starts_with("LC_")
				&& value.chars().count() >= 8
				&& !value.chars().all(char::is_numeric)
				&& !paths.contains(&value.as_str())
		})
		.map(|(_, value)| value)
		.collect();
	secrets.sort();
	secrets.dedup();
	// Longest first, so a secret containing another goes whole.
	secrets.sort_by_key(|secret| std::cmp::Reverse(secret.chars().count()));
	for secret in secrets {
		out = out.replace(secret.as_str(), "<redacted>");
	}
	out
}

/// At most `cap` bytes, cut at a character boundary; and whether it was cut.
pub fn bounded(text: &str, cap: usize) -> (String, bool) {
	if text.len() <= cap {
		return (text.to_owned(), false);
	}
	let mut end = cap;
	while !text.is_char_boundary(end) {
		end -= 1;
	}
	(text[..end].to_owned(), true)
}

/// Bytes truncated to `cap` and decoded, dropping a split final character
/// (Python's `data[:cap].decode('utf-8', 'ignore')` on encoded text).
fn bounded_bytes(data: &[u8], cap: usize) -> String {
	let data = &data[..data.len().min(cap)];
	match std::str::from_utf8(data) {
		Ok(text) => text.to_owned(),
		Err(error) => String::from_utf8_lossy(&data[..error.valid_up_to()]).into_owned(),
	}
}

fn absolute(path: &Path) -> PathBuf {
	if path.is_absolute() {
		path.to_owned()
	} else {
		std::env::current_dir().unwrap_or_default().join(path)
	}
}

/// A regular file inside one of `roots`, reached without a link.
pub fn checked(path: &Path, roots: &[PathBuf]) -> Result<PathBuf> {
	let path = absolute(path);
	let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
	if resolved != path || !roots.iter().any(|root| path.starts_with(absolute(root))) {
		bail!("failure artifact is linked or outside invocation");
	}
	if !path.is_file() {
		bail!(
			"failure artifact missing: {}",
			path.file_name()
				.map(|n| n.to_string_lossy())
				.unwrap_or_default()
		);
	}
	Ok(path)
}

pub fn value(path: &Path, roots: &[PathBuf]) -> Result<Map<String, Value>> {
	let path = checked(path, roots)?;
	ensure!(
		std::fs::metadata(&path)?.len() <= 8 * 1024 * 1024,
		"failure receipt exceeds read bound"
	);
	match serde_json::from_slice(&std::fs::read(&path)?)? {
		Value::Object(row) => Ok(row),
		_ => bail!("failure receipt is not an object"),
	}
}

/// A large log's head and tail, without the partial lines at either cut, so
/// a secret split there leaves no fragment that whole-value redaction misses.
pub fn log_excerpt(path: &Path, cap: usize) -> Result<(Vec<u8>, bool)> {
	let size = std::fs::metadata(path)?.len() as usize;
	let mut stream = File::open(path)?;
	if size <= cap {
		let mut data = Vec::new();
		stream.read_to_end(&mut data)?;
		return Ok((data, false));
	}
	let marker = b"\n<truncated>\n";
	let take = (cap - marker.len()) / 2;
	let mut head = vec![0; take];
	stream.read_exact(&mut head)?;
	stream.seek(SeekFrom::Start((size - take) as u64))?;
	let mut tail = Vec::new();
	stream.read_to_end(&mut tail)?;
	let head = match head.iter().rposition(|b| *b == b'\n') {
		Some(i) => head[..i].to_vec(),
		None => Vec::new(),
	};
	let tail = match tail.iter().position(|b| *b == b'\n') {
		Some(i) => tail[i + 1..].to_vec(),
		None => Vec::new(),
	};
	Ok(([head, marker.to_vec(), tail].concat(), true))
}

pub fn cause_line(text: &str) -> Option<String> {
	split_lines(text)
		.into_iter()
		.map(python_strip)
		.find(|line| CAUSE.is_match(line))
		.map(str::to_owned)
}

/// Python's `str.strip()`: Unicode whitespace and the ASCII separators.
fn python_strip(text: &str) -> &str {
	text.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

/// `readline(limit)`: through a newline, or `limit` bytes, or to the end.
fn read_line(stream: &mut BufReader<File>, limit: usize) -> Result<Vec<u8>> {
	let mut line = Vec::new();
	while line.len() < limit {
		let buffer = stream.fill_buf()?;
		if buffer.is_empty() {
			break;
		}
		let want = limit - line.len();
		let (take, done) = match buffer.iter().take(want).position(|b| *b == b'\n') {
			Some(i) => (i + 1, true),
			None => (buffer.len().min(want), false),
		};
		line.extend_from_slice(&buffer[..take]);
		stream.consume(take);
		if done {
			break;
		}
	}
	Ok(line)
}

/// Named libtest failure sections ahead of the bounded ordinary excerpt.
pub fn cause_excerpt(path: &Path, cap: usize) -> Result<(Vec<u8>, bool)> {
	let (mut selected, mut active, mut cut, limit) = (Vec::new(), false, false, cap / 2);
	let mut stream = BufReader::new(File::open(path)?);
	loop {
		let mut line = read_line(&mut stream, FILE_CAP + 1)?;
		if line.is_empty() {
			break;
		}
		if line.len() > FILE_CAP && !line.ends_with(b"\n") {
			while !line.is_empty() && !line.ends_with(b"\n") {
				line = read_line(&mut stream, FILE_CAP + 1)?;
			}
			if active {
				cut = true;
			}
			// Never keep a fragment of an oversized, possibly secret line.
			continue;
		}
		if TEST_SECTION.is_match(&line) {
			active = true;
		} else if line.starts_with(b"     Running ")
			|| line.starts_with(b"   Doc-tests ")
			|| line.starts_with(b"error: test failed")
		{
			// A failing test can print a nested libtest report; its failures
			// and result lines belong to the outer test, not an end.
			active = false;
		}
		if active {
			if selected.len() + line.len() <= limit {
				selected.extend_from_slice(&line);
			} else {
				cut = true;
			}
		}
	}
	if selected.is_empty() {
		return log_excerpt(path, cap);
	}
	let marker = b"\n<log excerpt after failing test sections>\n";
	if cut {
		selected.extend_from_slice(b"\n<failure sections truncated>\n");
	}
	let (raw, excerpt_cut) = log_excerpt(
		path,
		256.max(cap.saturating_sub(selected.len() + marker.len())),
	)?;
	Ok((
		[selected, marker.to_vec(), raw].concat(),
		cut || excerpt_cut,
	))
}

/// Python's `str()` of a JSON value, for messages built from receipt fields.
pub fn python_str(value: &Value) -> String {
	match value {
		Value::String(text) => text.clone(),
		other => python_repr(other),
	}
}

pub fn python_repr(value: &Value) -> String {
	match value {
		Value::Null => "None".into(),
		Value::Bool(true) => "True".into(),
		Value::Bool(false) => "False".into(),
		Value::Number(number) => {
			if number.is_f64() {
				let mut out = String::new();
				pyjson::compact(&Value::Number(number.clone()))
					.map(|text| out.push_str(&text))
					.ok();
				out
			} else {
				number.to_string()
			}
		}
		Value::String(text) => {
			let quote = if text.contains('\'') && !text.contains('"') {
				'"'
			} else {
				'\''
			};
			let mut out = String::from(quote);
			for c in text.chars() {
				match c {
					'\\' => out.push_str("\\\\"),
					'\n' => out.push_str("\\n"),
					'\r' => out.push_str("\\r"),
					'\t' => out.push_str("\\t"),
					c if c == quote => {
						out.push('\\');
						out.push(c);
					}
					c if (c as u32) < 0x20 || c as u32 == 0x7f => {
						out.push_str(&format!("\\x{:02x}", c as u32))
					}
					c => out.push(c),
				}
			}
			out.push(quote);
			out
		}
		Value::Array(items) => format!(
			"[{}]",
			items.iter().map(python_repr).collect::<Vec<_>>().join(", ")
		),
		Value::Object(map) => format!(
			"{{{}}}",
			map.iter()
				.map(|(k, v)| format!(
					"{}: {}",
					python_repr(&Value::String(k.clone())),
					python_repr(v)
				))
				.collect::<Vec<_>>()
				.join(", ")
		),
	}
}

/// A str or int (bool included), as Python's isinstance(x, (str, int)) sees it.
fn str_or_int(value: Option<&Value>) -> Option<String> {
	match value? {
		Value::String(_) | Value::Bool(_) => Some(python_str(value?)),
		Value::Number(n) if !n.is_f64() => Some(n.to_string()),
		_ => None,
	}
}

/// Only reviewed diagnostic fields: no measurement vectors or invocations.
pub fn diagnostic_lines(row: &Map<String, Value>) -> Vec<String> {
	let mut lines = Vec::new();
	for key in ["diagnostics", "failed_cases", "failures", "causes"] {
		let items = match row.get(key) {
			None => continue,
			Some(item @ (Value::String(_) | Value::Object(_))) => vec![item.clone()],
			Some(Value::Array(items)) => items.clone(),
			Some(_) => continue,
		};
		for item in items.iter().take(16) {
			match item {
				Value::String(text) => lines.push(text.clone()),
				Value::Object(map) => {
					let identity = ["id", "case", "name", "stage"]
						.iter()
						.find_map(|k| str_or_int(map.get(*k)))
						.unwrap_or_default();
					let message = ["error", "message", "diagnostic", "reason"]
						.iter()
						.find_map(|k| map.get(*k).and_then(Value::as_str))
						.unwrap_or_default();
					if !identity.is_empty() || !message.is_empty() {
						lines.push(
							format!("{identity}: {message}")
								.trim_matches(|c| c == ':' || c == ' ')
								.to_owned(),
						);
					}
				}
				_ => {}
			}
		}
	}
	lines.truncate(16);
	lines
}

pub fn generic(message: &str) -> bool {
	[
		"stage failed",
		"child failed",
		"producer returned FAIL without an error message",
	]
	.contains(&message)
		|| [
			"child exit is nonzero",
			"Traceback (most recent call last)",
			"CalledProcessError(",
		]
		.iter()
		.any(|part| message.contains(part))
}

pub fn reason(row: &Map<String, Value>) -> String {
	let text = row.get("error").map(python_str).unwrap_or_default();
	if !text.is_empty() && !generic(&text) {
		return cause_line(&text).unwrap_or(text);
	}
	let lines = diagnostic_lines(row);
	if !lines.is_empty() {
		return lines.join("\n");
	}
	if !text.is_empty() {
		return cause_line(&text).unwrap_or(text);
	}
	// Failed rule identities and booleans, never the measurement vectors.
	let mut failed = Vec::new();
	for section in ["per_record", "cumulative"] {
		for check in row
			.get(section)
			.and_then(Value::as_array)
			.into_iter()
			.flatten()
		{
			if check.get("pass_all") == Some(&Value::Bool(false)) {
				let fields: Vec<String> = ["kind", "mode", "metric", "level_pass", "growth_pass"]
					.iter()
					.filter_map(|key| check.get(*key).map(|v| format!("{key}={}", python_str(v))))
					.collect();
				failed.push(format!("{section}: {}", fields.join(", ")));
			}
		}
	}
	failed.truncate(16);
	if failed.is_empty() {
		"producer returned FAIL without an error message".into()
	} else {
		failed.join("\n")
	}
}

pub fn projection(row: &Map<String, Value>, environment: &Environment) -> Map<String, Value> {
	let mut result = Map::new();
	for key in ["status", "kind", "policy", "n", "role", "error_type"] {
		if let Some(value) = row.get(key)
			&& (matches!(value, Value::String(_) | Value::Bool(_) | Value::Null)
				|| value.as_number().is_some_and(|n| !n.is_f64()))
		{
			result.insert(key.into(), value.clone());
		}
	}
	if row.get("status") == Some(&json!("FAIL")) || row.contains_key("error") {
		let (error, truncated) = bounded(&sanitize(&reason(row), environment), ERROR_CAP);
		result.insert("error".into(), json!(error));
		result.insert("error_truncated".into(), json!(truncated));
	}
	let lines = diagnostic_lines(row);
	if !lines.is_empty() {
		result.insert(
			"causes".into(),
			json!(
				lines
					.iter()
					.map(|line| bounded(&sanitize(line, environment), 1024).0)
					.collect::<Vec<_>>()
			),
		);
	}
	// Only the numeric admission fields and the binding label are diagnostic.
	if let Some(Value::Object(admission)) = row.get("admission") {
		let keys = [
			"host_available_bytes",
			"cgroup_headroom_bytes",
			"binding_cgroup",
			"binding_limit",
			"reserve_bytes",
			"per_worker_bytes",
			"workers",
		];
		let kept: Map<String, Value> = admission
			.iter()
			.filter(|(k, v)| {
				keys.contains(&k.as_str())
					&& (v.is_string()
						|| v.is_boolean() || v.as_number().is_some_and(|n| !n.is_f64()))
			})
			.map(|(k, v)| (k.clone(), v.clone()))
			.collect();
		result.insert("admission".into(), Value::Object(kept));
	}
	for item in result.values_mut() {
		match item {
			Value::String(text) => *text = sanitize(text, environment),
			Value::Object(map) => {
				for value in map.values_mut() {
					if let Value::String(text) = value {
						*text = sanitize(text, environment);
					}
				}
			}
			_ => {}
		}
	}
	result
}

/// Bounded diagnostic sources, never arbitrary siblings or a tree walk.
const DIAGNOSTIC_SIBLINGS: [&str; 4] = [
	"controller.stderr",
	"host-check.stderr",
	"child.log",
	"direct.stderr",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
	Receipt,
	Log,
}

impl Kind {
	fn name(self) -> &'static str {
		match self {
			Kind::Receipt => "receipt",
			Kind::Log => "log",
		}
	}
}

pub struct Bundle {
	out: PathBuf,
	directory: PathBuf,
	environment: Environment,
	pub row: Map<String, Value>,
	remaining: usize,
}

impl Bundle {
	pub fn new(out: &Path, stage: &str, environment: &Environment, error: &str) -> Result<Self> {
		let directory = out.join("failure");
		std::fs::create_dir(&directory)
			.with_context(|| format!("create {}", directory.display()))?;
		let (message, cut) = bounded(&sanitize(error, environment), ERROR_CAP);
		let row = json!({
			"schema": 1, "diagnostic_only": true, "stage": bounded(&sanitize(stage, environment), 256).0,
			"files": [], "omitted": [], "error": message, "error_truncated": cut,
		});
		Ok(Self {
			out: out.to_owned(),
			directory,
			environment: environment.clone(),
			row: row.as_object().cloned().expect("an object"),
			remaining: BUNDLE_CAP - INDEX_RESERVE,
		})
	}

	fn files(&self) -> &Vec<Value> {
		self.row["files"].as_array().expect("files is a list")
	}

	pub fn omit(&mut self, reason: &str) {
		self.row["omitted"]
			.as_array_mut()
			.expect("omitted is a list")
			.push(json!(reason));
	}

	pub fn error(&self) -> &str {
		self.row["error"].as_str().unwrap_or_default()
	}

	pub fn add(&mut self, path: &Path, kind: Kind, roots: &[PathBuf]) -> Result<Option<String>> {
		if self.files().len() >= 8 || self.remaining < 1024 {
			self.omit("budget exhausted");
			return Ok(None);
		}
		let path = checked(path, roots)?;
		let (mut data, mut cut) = match kind {
			Kind::Receipt => (
				pyjson::pretty(&Value::Object(projection(
					&value(&path, roots)?,
					&self.environment,
				)))?
				.into_bytes(),
				false,
			),
			Kind::Log => {
				let (raw, cut) = cause_excerpt(&path, FILE_CAP)?;
				(
					sanitize(&String::from_utf8_lossy(&raw), &self.environment).into_bytes(),
					cut,
				)
			}
		};
		let cap = FILE_CAP.min(self.remaining);
		if kind == Kind::Receipt && data.len() > cap {
			data = b"{\"status\": \"FAIL\", \"error\": \"receipt projection omitted: diagnostic size bound\", \"truncated\": true}\n".to_vec();
			cut = true;
		} else {
			cut |= data.len() > cap;
			data = bounded_bytes(&data, cap).into_bytes();
		}
		let name = format!(
			"{:02}-{}",
			self.files().len(),
			if kind == Kind::Receipt {
				"receipt.json"
			} else {
				"child.log"
			}
		);
		let destination = self.directory.join(&name);
		let mut stream = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(&destination)?;
		stream.write_all(&data)?;
		stream.sync_all()?;
		self.remaining -= data.len();
		let source_name = bounded(
			&sanitize(
				&path
					.file_name()
					.map(|n| n.to_string_lossy().into_owned())
					.unwrap_or_default(),
				&self.environment,
			),
			256,
		)
		.0;
		let row = json!({
			"path": format!("failure/{name}"), "kind": kind.name(), "bytes": data.len(), "sha256": digest(&destination)?,
			"source_name": source_name, "source_bytes": std::fs::metadata(&path)?.len(), "source_sha256": digest(&path)?,
			"truncated": cut, "projected": kind == Kind::Receipt,
		});
		self.row["files"]
			.as_array_mut()
			.expect("files is a list")
			.push(row);
		Ok(Some(
			String::from_utf8(data).expect("bounded data is UTF-8"),
		))
	}

	/// Prefer a concrete cause found in a log over a generic error.
	fn adopt_cause(&mut self, text: Option<&str>) -> Option<String> {
		let cause = cause_line(text.unwrap_or_default());
		if let Some(cause) = &cause
			&& generic(self.error())
		{
			self.row.insert("error".into(), json!(cause));
			self.row.insert("error_truncated".into(), json!(false));
		}
		cause
	}

	pub fn finish(mut self) -> Result<Value> {
		let omitted = self.row["omitted"]
			.as_array_mut()
			.expect("omitted is a list");
		omitted.truncate(16);
		let encoded = pyjson::pretty(&Value::Object(self.row.clone()))?;
		ensure!(
			encoded.len() <= INDEX_RESERVE,
			"failure index exceeds bound"
		);
		sync_directory(&self.directory)?;
		let index = self.out.join("FAILURE.json");
		atomic_json(&index, &Value::Object(self.row.clone()))?;
		let files: usize = self
			.files()
			.iter()
			.map(|f| f["bytes"].as_u64().unwrap_or(0) as usize)
			.sum();
		ensure!(
			encoded.len() + files <= BUNDLE_CAP,
			"failure bundle exceeds bound"
		);
		Ok(
			json!({"path": index.to_string_lossy(), "sha256": digest(&index)?, "stage": self.row["stage"], "error": self.row["error"]}),
		)
	}
}

/// Retain a failure's cause: the failing leaf receipt, its declared and
/// conventional diagnostic files, and the stage's logs.
pub fn retain(
	out: &Path,
	stage: &str,
	receipt: Option<&Path>,
	logs: &[PathBuf],
	roots: &[PathBuf],
	environment: &Environment,
	error: &str,
) -> Result<Value> {
	let mut bundle = Bundle::new(out, stage, environment, error)?;
	if let Some(receipt) = receipt
		&& let Err(problem) = retain_receipt(&mut bundle, receipt, roots, environment)
	{
		bundle.omit(&bounded(&sanitize(&problem.to_string(), environment), 1024).0);
	}
	for log in logs {
		let result = (|| -> Result<()> {
			if !log.is_file() {
				return Ok(());
			}
			let text = bundle.add(log, Kind::Log, roots)?;
			let cause = bundle.adopt_cause(text.as_deref());
			let receipt_missing = receipt.is_none_or(|r| !r.is_file());
			if cause.is_none()
				&& (receipt_missing || bundle.error().contains("child exit is nonzero"))
			{
				let (raw, _) = log_excerpt(&checked(log, roots)?, FILE_CAP)?;
				let text = sanitize(&String::from_utf8_lossy(&raw), environment);
				let tail = &text[text.char_indices().rev().nth(8191).map_or(0, |(i, _)| i)..];
				let (error, cut) = bounded(
					&format!("{}\nchild log:\n{tail}", bundle.error()),
					ERROR_CAP,
				);
				let truncated = bundle.row["error_truncated"].as_bool().unwrap_or(false) || cut;
				bundle.row.insert("error".into(), json!(error));
				bundle
					.row
					.insert("error_truncated".into(), json!(truncated));
			}
			Ok(())
		})();
		if let Err(problem) = result {
			bundle.omit(&bounded(&sanitize(&problem.to_string(), environment), 1024).0);
		}
	}
	bundle.finish()
}

fn retain_receipt(
	bundle: &mut Bundle,
	receipt: &Path,
	roots: &[PathBuf],
	environment: &Environment,
) -> Result<()> {
	let parent = value(receipt, roots)?;
	let mut leaf = receipt.to_owned();
	// Only the capture's explicit case receipt list is followed, never a tree.
	for name in parent
		.get("records")
		.and_then(Value::as_array)
		.into_iter()
		.flatten()
		.take(512)
	{
		let Some(name) = name.as_str() else { continue };
		if value(Path::new(name), roots)?.get("status") == Some(&json!("FAIL")) {
			leaf = PathBuf::from(name);
			break;
		}
	}
	let row = value(&leaf, roots)?;
	if row.get("status") == Some(&json!("FAIL")) || row.get("error").is_some_and(truthy) {
		let (error, cut) = bounded(&sanitize(&reason(&row), environment), ERROR_CAP);
		bundle.row.insert("error".into(), json!(error));
		bundle.row.insert("error_truncated".into(), json!(cut));
	}
	bundle.add(&leaf, Kind::Receipt, roots)?;
	let directory = leaf.parent().unwrap_or(Path::new("")).to_owned();
	let mut references: Vec<PathBuf> = Vec::new();
	if let Some(Value::Array(declared)) = row.get("diagnostic_files") {
		references.extend(
			declared
				.iter()
				.take(16)
				.filter_map(Value::as_str)
				.map(|name| directory.join(name)),
		);
	}
	references.extend(DIAGNOSTIC_SIBLINGS.iter().map(|name| directory.join(name)));
	let mut seen = std::collections::BTreeSet::new();
	for path in references {
		if !seen.insert(path.clone()) || !path.is_file() {
			continue;
		}
		let text = bundle.add(&path, Kind::Log, roots)?;
		bundle.adopt_cause(text.as_deref());
	}
	if leaf != receipt {
		bundle.add(receipt, Kind::Receipt, roots)?;
	}
	Ok(())
}

/// Python truthiness of a JSON value.
pub fn truthy(value: &Value) -> bool {
	match value {
		Value::Null => false,
		Value::Bool(b) => *b,
		Value::Number(n) => n.as_f64() != Some(0.0),
		Value::String(s) => !s.is_empty(),
		Value::Array(a) => !a.is_empty(),
		Value::Object(m) => !m.is_empty(),
	}
}

/// Copy only verified diagnostic files, bounded again at each owner.
pub fn copy_bundle(
	out: &Path,
	reference: &Value,
	roots: &[PathBuf],
	environment: &Environment,
) -> Result<Value> {
	let path = checked(
		Path::new(
			reference["path"]
				.as_str()
				.context("failure reference has no path")?,
		),
		roots,
	)?;
	ensure!(
		Some(digest(&path)?.as_str()) == reference["sha256"].as_str(),
		"failure index changed"
	);
	let row = value(&path, roots)?;
	let files = row
		.get("files")
		.and_then(Value::as_array)
		.cloned()
		.unwrap_or_default();
	if row.get("schema") != Some(&json!(1))
		|| row.get("diagnostic_only") != Some(&json!(true))
		|| files.len() > 8
		|| std::fs::metadata(&path)?.len() as usize > INDEX_RESERVE
	{
		bail!("not a bounded diagnostic bundle");
	}
	let stage = row.get("stage").and_then(Value::as_str).unwrap_or_default();
	let error = row.get("error").and_then(Value::as_str).unwrap_or_default();
	let mut bundle = Bundle::new(out, stage, environment, error)?;
	let directory = path.parent().unwrap_or(Path::new("")).to_owned();
	for item in &files {
		let p = checked(
			&directory.join(item["path"].as_str().unwrap_or_default()),
			std::slice::from_ref(&directory),
		)?;
		let bytes = item["bytes"].as_u64().unwrap_or(u64::MAX);
		if std::fs::metadata(&p)?.len() != bytes
			|| bytes as usize > FILE_CAP
			|| Some(digest(&p)?.as_str()) != item["sha256"].as_str()
		{
			bail!("failure artifact changed or exceeds bound");
		}
		let count = bundle.files().len();
		let kind = if item["kind"] == "receipt" {
			Kind::Receipt
		} else {
			Kind::Log
		};
		bundle.add(&p, kind, std::slice::from_ref(&directory))?;
		if bundle.files().len() == count {
			break;
		}
		let last = bundle.row["files"]
			.as_array_mut()
			.and_then(|f| f.last_mut())
			.and_then(Value::as_object_mut)
			.expect("a file row was added");
		let truncated = item["truncated"].as_bool().unwrap_or(false)
			|| last["truncated"].as_bool().unwrap_or(false);
		for key in ["source_name", "source_bytes", "source_sha256", "projected"] {
			last.insert(key.into(), item[key].clone());
		}
		last.insert("truncated".into(), json!(truncated));
	}
	let omitted = row
		.get("omitted")
		.and_then(Value::as_array)
		.cloned()
		.unwrap_or_default();
	bundle.row["omitted"]
		.as_array_mut()
		.expect("omitted is a list")
		.extend(omitted);
	let truncated = bundle.row["error_truncated"].as_bool().unwrap_or(false)
		|| row
			.get("error_truncated")
			.and_then(Value::as_bool)
			.unwrap_or(false);
	bundle
		.row
		.insert("error_truncated".into(), json!(truncated));
	bundle.finish()
}
