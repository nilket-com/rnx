//! Inspect bounded slices only. Never format a whole frame or arbitrary AnyValue.
//!
//! Record 0158: the bounded cells are laid out as a Polars-style table (the
//! look of Polars' default `UTF8_FULL_CONDENSED` display) by a writer of our
//! own. Polars' `Display` is never called: it reads every column and formats
//! values without these bounds or this escaping (review of 0158).
use crate::p;
use unicode_width::UnicodeWidthStr;
const ROWS: usize = 10;
const COLUMNS: usize = 8;
const SCALARS: usize = 80;
const BYTES: usize = 8192;
const OMITTED: &str = "\n[preview byte limit; remainder omitted]\n";
/// The marker after a scalar cut at `SCALARS`, and the text of an elided row
/// or column: Polars' ellipsis. It is a display convention, not a proof: a
/// value may itself be `…` (review of 0158).
const CUT: &str = "…";

/// The adapter's own byte accounting over any sink: at most `BYTES` bytes
/// of whole tokens, then the omission marker, then nothing. Both the explicit
/// `preview()` string and the session presenter go through this, so their
/// text is identical when the outer budget is sufficient.
struct Capped<'a> {
	written: usize,
	stopped: bool,
	sink: &'a mut dyn FnMut(&str) -> bool,
}
impl<'a> Capped<'a> {
	fn new(sink: &'a mut dyn FnMut(&str) -> bool) -> Self {
		Self {
			written: 0,
			stopped: false,
			sink,
		}
	}
	// Whole bounded tokens only; reserve the final marker before every append.
	fn push(&mut self, token: &str) -> bool {
		if self.stopped {
			return false;
		}
		if token.len() > BYTES - OMITTED.len() - self.written {
			self.stopped = true;
			(self.sink)(OMITTED);
			return false;
		}
		self.written += token.len();
		if (self.sink)(token) {
			true
		} else {
			// The outer sink is full; it has appended its own marker.
			self.stopped = true;
			false
		}
	}
}

/// One scalar, written whole: backslashes and the common controls as their
/// short escapes; every other control and every line, paragraph, embedding,
/// override or isolate control as `\u{…}`. Names, strings and a dtype's zone
/// share this policy (review of 0124: a zone is arbitrary text). Inside
/// quotes (a list's strings, a zone) a quote is escaped too; unquoted text
/// (record 0158: a top-level string or name, as Polars shows them) keeps it,
/// and stays unambiguous because a backslash is always escaped.
fn escaped_into(ch: char, text: &mut String, quoted: bool) {
	match ch {
		'"' if quoted => text.push_str("\\\""),
		'\\' => text.push_str("\\\\"),
		'\n' => text.push_str("\\n"),
		'\r' => text.push_str("\\r"),
		'\t' => text.push_str("\\t"),
		ch if ch.is_control()
			|| matches!(ch, '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
		{
			text.extend(ch.escape_unicode());
		}
		ch => text.push(ch),
	}
}
/// A top-level string, category or name: unquoted and escaped a scalar at a
/// time, at most `SCALARS` of them, then the marker.
fn plain(value: &str) -> String {
	let mut text = String::new();
	let mut end = 0;
	for (offset, ch) in value.char_indices().take(SCALARS) {
		end = offset + ch.len_utf8();
		escaped_into(ch, &mut text, false);
	}
	// Byte length reveals remaining input without inspecting an 81st scalar.
	if end < value.len() {
		text.push_str(CUT);
	}
	text
}
/// A column's dtype. The four names record 0058 shipped keep their spelling.
/// Every other dtype is named without ever formatting it whole (review of
/// 0124: a nested schema can be arbitrarily large): a list is named
/// recursively and stops at `SCALARS` characters; a struct, an array, an
/// object, a categorical, an enum or an extension has a fixed name; the
/// remaining dtypes are flat (integers, floats, decimal, temporal, binary,
/// null) and their Polars text is of fixed size.
fn dtype(kind: &p::DataType) -> String {
	let mut name = Bounded::new();
	dtype_into(kind, &mut name);
	name.text
}
/// Text bounded in input scalars, as a quoted string is: at most `SCALARS`
/// are written, each whole, then the truncation marker once, and nothing
/// after it (review of 0124). There is no output-side cut, so an escape is
/// never split. Dtype names and list cells are built with it.
struct Bounded {
	text: String,
	budget: usize,
	cut: bool,
}
impl Bounded {
	fn new() -> Self {
		Bounded {
			text: String::new(),
			budget: SCALARS,
			cut: false,
		}
	}
	/// Plain text, a scalar at a time (a dtype name's fixed pieces).
	fn push_str(&mut self, part: &str) {
		for ch in part.chars() {
			if !self.take("") {
				return;
			}
			self.text.push(ch);
		}
	}
	/// A token shown whole or not at all (a number, a date, a placeholder):
	/// never cut inside, so `12345` cannot show as `123`.
	fn push_token(&mut self, token: &str) {
		let n = token.chars().count();
		if self.cut || n > self.budget {
			self.budget = 0;
			self.take("");
			return;
		}
		self.budget -= n;
		self.text.push_str(token);
	}
	/// Arbitrary text (a zone): every scalar escaped as names and strings are.
	fn push_escaped(&mut self, part: &str) {
		for ch in part.chars() {
			if !self.take("") {
				return;
			}
			escaped_into(ch, &mut self.text, true);
		}
	}
	/// A string, quoted and escaped a scalar at a time; the closing quote is
	/// always written, before the marker if the budget ends inside.
	fn push_quoted(&mut self, value: &str) {
		if !self.take("") {
			return;
		}
		self.text.push('"');
		for ch in value.chars() {
			if !self.take("\"") {
				return;
			}
			escaped_into(ch, &mut self.text, true);
		}
		self.text.push('"');
	}
	/// Bytes as `b"…"`, a byte at a time, as `blob` escapes them.
	fn push_bytes(&mut self, bytes: &[u8]) {
		if !self.take("") {
			return;
		}
		self.text.push_str("b\"");
		for b in bytes {
			if !self.take("\"") {
				return;
			}
			self.text
				.extend(std::ascii::escape_default(*b).map(char::from));
		}
		self.text.push('"');
	}
	/// One scalar of budget; when it is spent, `close` (an open quote's
	/// closing) and the marker are written once, and every later push is
	/// dropped.
	fn take(&mut self, close: &str) -> bool {
		if self.cut {
			return false;
		}
		if self.budget == 0 {
			self.text.push_str(close);
			self.text.push_str(CUT);
			self.cut = true;
			return false;
		}
		self.budget -= 1;
		true
	}
}
fn dtype_into(kind: &p::DataType, text: &mut Bounded) {
	if text.cut {
		// the budget is spent: stop descending
		return;
	}
	match kind {
		p::DataType::String => text.push_str("string"),
		p::DataType::Int64 => text.push_str("i64"),
		p::DataType::Float64 => text.push_str("f64"),
		p::DataType::Boolean => text.push_str("bool"),
		p::DataType::List(inner) => {
			text.push_str("list[");
			dtype_into(inner, text);
			text.push_str("]");
		}
		// review of 0124: a zone is arbitrary text (Polars' own Display writes
		// it whole), so it is escaped, and counted against the budget
		p::DataType::Datetime(unit, Some(zone)) => {
			text.push_str("datetime[");
			text.push_str(&unit.to_string());
			text.push_str(", ");
			text.push_escaped(zone);
			text.push_str("]");
		}
		k if fixed_width(k) => text.push_str(&k.to_string()),
		k if k.is_struct() => text.push_str("struct"),
		k if k.is_array() => text.push_str("array"),
		k if k.is_object() => text.push_str("object"),
		k if k.is_categorical() => text.push_str("cat"),
		k if k.is_enum() => text.push_str("enum"),
		k if k.is_extension() => text.push_str("ext"),
		k if k.is_nested() => text.push_str("nested"),
		// a dtype this list does not name is never formatted
		_ => text.push_str("opaque"),
	}
}
/// The dtypes whose Polars text, and whose values' Polars text, is of fixed
/// size and never panics: a closed list (review of 0124). A zoned datetime
/// is not in it (its zone is arbitrary), nor anything nested or extension.
/// Date, datetime and time values are in it only through [`temporal`], which
/// avoids Polars' panicking conversions.
fn fixed_width(kind: &p::DataType) -> bool {
	use p::DataType as D;
	kind.is_primitive_numeric()
		|| kind.is_decimal()
		|| matches!(
			kind,
			D::Null
				| D::Boolean | D::String
				| D::Binary | D::BinaryOffset
				| D::Date | D::Time
				| D::Duration(_)
				| D::Datetime(_, None)
				| D::Unknown(_)
		)
}
/// A date, datetime or time cell, read as its physical integer and converted
/// with Polars' checked conversions: Polars' own `Display` for these values
/// `expect`s the value in range and panics otherwise (review of 0124). An
/// out-of-range value shows a placeholder. A zoned datetime shows its UTC
/// instant, marked `UTC`: the 0.55.2 build has no time zone database, and
/// Polars' zoned `Display` panics there; the header names the zone.
fn temporal(value: &p::AnyValue<'_>, kind: &p::DataType) -> Option<String> {
	use polars_arrow::temporal_conversions as tc;
	const NANOSECONDS_PER_DAY: i64 = 86_400 * 1_000_000_000;
	let shown = |text: Option<String>| text.unwrap_or_else(|| "<out of range>".into());
	match kind {
		p::DataType::Date => Some(shown(
			value
				.extract::<i32>()
				// `date32_to_date_opt` adds the epoch's day number unchecked
				// (an overflow panic in debug): keep within i32 first
				.filter(|&days| days <= i32::MAX - tc::EPOCH_DAYS_FROM_CE)
				.and_then(tc::date32_to_date_opt)
				.map(|d| d.to_string()),
		)),
		p::DataType::Datetime(unit, zone) => {
			let instant = value.extract::<i64>().and_then(|v| match unit {
				p::TimeUnit::Nanoseconds => tc::timestamp_ns_to_datetime_opt(v),
				p::TimeUnit::Microseconds => tc::timestamp_us_to_datetime_opt(v),
				p::TimeUnit::Milliseconds => tc::timestamp_ms_to_datetime_opt(v),
			});
			Some(match (instant, zone) {
				(Some(t), Some(_)) => format!("{t} UTC"),
				(t, _) => shown(t.map(|t| t.to_string())),
			})
		}
		p::DataType::Time => Some(shown(
			value
				.extract::<i64>()
				// `time64ns_to_time_opt` casts the seconds to u32 unchecked,
				// which would wrap a huge value into a wrong time: a day only
				.filter(|v| (0..NANOSECONDS_PER_DAY).contains(v))
				.and_then(tc::time64ns_to_time_opt)
				.map(|t| t.to_string()),
		)),
		_ => None,
	}
}
/// Text cut at `SCALARS` characters, with the truncation marker.
fn bounded(text: String) -> String {
	match text.char_indices().nth(SCALARS) {
		Some((end, _)) => format!("{}{CUT}", &text[..end]),
		None => text,
	}
}
/// Bytes as `b"…"`, each non-printable byte escaped, cut at `SCALARS` bytes.
fn blob(bytes: &[u8]) -> String {
	let mut text = String::from("b\"");
	for b in bytes.iter().take(SCALARS) {
		text.extend(std::ascii::escape_default(*b).map(char::from));
	}
	text.push('"');
	if bytes.len() > SCALARS {
		text.push_str(CUT);
	}
	text
}
/// A list cell, bounded as a string is, in input scalars across every
/// nesting level: an opening bracket, a separator's scalars, a string's or a
/// byte string's scalars each count, and any other element is a token shown
/// whole or not at all (review of 0124: an output-side cut split escapes).
/// So at most `SCALARS` elements and `SCALARS` levels are visited, whatever
/// the list's length or depth, and nothing is cut inside an escape.
fn list(values: &p::Series) -> String {
	let mut text = Bounded::new();
	list_into(values, &mut text);
	text.text
}
fn list_into(values: &p::Series, text: &mut Bounded) {
	if !text.take("") {
		return;
	}
	text.text.push('[');
	for i in 0..values.len() {
		if i != 0 {
			text.push_str(", ");
		}
		if text.cut {
			return;
		}
		match values.get(i) {
			Ok(p::AnyValue::List(inner)) => list_into(&inner, text),
			Ok(p::AnyValue::String(s)) => text.push_quoted(s),
			Ok(p::AnyValue::StringOwned(s)) => text.push_quoted(s.as_str()),
			Ok(p::AnyValue::Binary(b)) => text.push_bytes(b),
			Ok(p::AnyValue::BinaryOwned(b)) => text.push_bytes(&b),
			Ok(v) => match v.get_str() {
				// a categorical or enum element: its category's string
				Some(s) => text.push_quoted(s),
				_ => text.push_token(&cell(v, values.dtype())),
			},
			Err(_) => text.push_token("<unreadable>"),
		}
		if text.cut {
			return;
		}
	}
	text.text.push(']');
}
/// One cell, always rendered and always bounded (record 0124): no dtype
/// refuses the preview. Scalars show their exact text (floats as their
/// shortest round-trip form, temporal and decimal values as Polars writes
/// them); strings and categories are escaped, unquoted (record 0158); bytes are escaped;
/// a list is visited under the scalar bound. A struct, an array, an object
/// or any value this cannot bound shows its dtype in angle brackets.
fn cell(value: p::AnyValue<'_>, kind: &p::DataType) -> String {
	match value {
		p::AnyValue::Null => "null".into(),
		p::AnyValue::String(s) => plain(s),
		p::AnyValue::StringOwned(s) => plain(s.as_str()),
		p::AnyValue::Int64(v) => v.to_string(),
		p::AnyValue::Float64(v) => format!("{v:?}"),
		p::AnyValue::Float32(v) => format!("{v:?}"),
		p::AnyValue::Boolean(v) => v.to_string(),
		p::AnyValue::Binary(b) => blob(b),
		p::AnyValue::BinaryOwned(b) => blob(&b),
		p::AnyValue::List(s) => list(&s),
		other => {
			// the column's dtype, by reference: a value's own `dtype()` would
			// rebuild a struct's field list for every cell
			if kind.is_nested() || kind.is_object() {
				format!("<{}>", dtype(kind))
			} else if let Some(text) = temporal(&other, kind) {
				text
			} else if let Some(s) = other.get_str() {
				// a categorical or enum value: its category's string
				plain(s)
			} else if fixed_width(kind) {
				// integers of every width, f16, decimal and duration: Polars'
				// own fixed-width text, bounded
				bounded(other.to_string())
			} else {
				// anything else is never formatted (review of 0124)
				format!("<{}>", dtype(kind))
			}
		}
	}
}
/// The columns a preview shows: all of them up to `COLUMNS`; beyond that the
/// first and the last `COLUMNS / 2` (record 0124: a workflow's derived
/// columns are appended last, and Polars' own display keeps both ends).
/// `None` marks where the omitted columns are.
fn shown_columns(width: usize) -> Vec<Option<usize>> {
	if width <= COLUMNS {
		return (0..width).map(Some).collect();
	}
	let half = COLUMNS / 2;
	(0..half)
		.map(Some)
		.chain(std::iter::once(None))
		.chain((width - half..width).map(Some))
		.collect()
}
/// Render into a fresh string: the explicit `preview()`.
pub(crate) fn render(frame: &p::DataFrame) -> Result<String, String> {
	let mut text = String::new();
	render_into(frame, &mut |token| {
		text.push_str(token);
		true
	})?;
	Ok(text)
}
/// The rows a preview shows: all of them up to `ROWS`; beyond that the first
/// and the last `ROWS / 2`, as Polars shows them (record 0158; 0124 showed
/// the first `ROWS`). `None` marks where the omitted rows are.
fn shown_rows(height: usize) -> Vec<Option<usize>> {
	if height <= ROWS {
		return (0..height).map(Some).collect();
	}
	let half = ROWS / 2;
	(0..half)
		.map(Some)
		.chain(std::iter::once(None))
		.chain((height - half..height).map(Some))
		.collect()
}
/// A dimension with Polars' `_` thousands grouping: `1_000_000`.
fn grouped(n: usize) -> String {
	let digits = n.to_string();
	let mut text = String::new();
	for (i, ch) in digits.chars().enumerate() {
		if i != 0 && (digits.len() - i) % 3 == 0 {
			text.push('_');
		}
		text.push(ch);
	}
	text
}
#[cfg(test)]
thread_local! {
	/// Cells read from the frame by this thread's renders (a test hook).
	static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// Render into any sink under the adapter's `BYTES` cap and omission marker.
/// The sink returns `false` when it is full (the session presenter's outer
/// budget); rendering then stops and `Ok(false)` says the output is partial.
/// Only structurally bounded slices are visited: at most `ROWS` rows and
/// `COLUMNS` columns, each scalar cut at `SCALARS`.
///
/// Record 0158: the cells are computed first (at most `ROWS` + 4 rows of
/// `COLUMNS` + 1 bounded texts), then laid out one line at a time, each line
/// one whole token: a line that does not fit ends the output with the
/// marker, and a cut table never has its bottom border.
pub(crate) fn render_into(
	frame: &p::DataFrame,
	sink: &mut dyn FnMut(&str) -> bool,
) -> Result<bool, String> {
	let mut out = Capped::new(sink);
	macro_rules! append {
		($token:expr) => {
			if !out.push($token) {
				return Ok(false);
			}
		};
	}
	append!(&format!(
		"shape: ({}, {})\n",
		grouped(frame.height()),
		grouped(frame.width())
	));
	let shown = shown_columns(frame.width());
	let columns = frame.columns();
	// the name, `---` and dtype rows, then the data rows; an elided column
	// is `…` with blanks under it, an elided row `…` in every column
	let mut names = Vec::with_capacity(shown.len());
	let mut dashes = Vec::with_capacity(shown.len());
	let mut dtypes = Vec::with_capacity(shown.len());
	for c in &shown {
		match c {
			Some(c) => {
				names.push(plain(columns[*c].name().as_str()));
				dashes.push("---".to_owned());
				dtypes.push(dtype(columns[*c].dtype()));
			}
			None => {
				names.push(CUT.to_owned());
				dashes.push(String::new());
				dtypes.push(String::new());
			}
		}
	}
	let mut rows = vec![names, dashes, dtypes];
	for row in shown_rows(frame.height()) {
		let mut cells = Vec::with_capacity(shown.len());
		for c in &shown {
			cells.push(match (row, c) {
				(Some(row), Some(c)) => {
					#[cfg(test)]
					READS.with(|n| n.set(n.get() + 1));
					let value = columns[*c]
						.get(row)
						.map_err(|e| format!("polars preview: {e}"))?;
					cell(value, columns[*c].dtype())
				}
				_ => CUT.to_owned(),
			});
		}
		rows.push(cells);
	}
	// each column's content width: its widest text, at least `---` for a
	// real column; display width in unicode-width's model
	let widths: Vec<usize> = (0..shown.len())
		.map(|c| rows.iter().map(|r| r[c].width()).max().unwrap_or(0))
		.collect();
	let rule = |left: &str, line: &str, join: &str, right: &str| {
		let mut text = String::from(left);
		for (i, w) in widths.iter().enumerate() {
			if i != 0 {
				text.push_str(join);
			}
			for _ in 0..w + 2 {
				text.push_str(line);
			}
		}
		text.push_str(right);
		text.push('\n');
		text
	};
	let line = |cells: &[String]| {
		let mut text = String::from("│");
		for (i, (cell, w)) in cells.iter().zip(&widths).enumerate() {
			text.push_str(if i == 0 { " " } else { " ┆ " });
			text.push_str(cell);
			for _ in cell.width()..*w {
				text.push(' ');
			}
		}
		text.push_str(if cells.is_empty() { "│\n" } else { " │\n" });
		text
	};
	append!(&rule("┌", "─", "┬", "┐"));
	for (i, cells) in rows.iter().enumerate() {
		// a frame with no columns has no header rows: `┌┐ ╞╡ └┘`, as Polars
		if i == 3 {
			append!(&rule("╞", "═", "╪", "╡"));
		}
		if !shown.is_empty() {
			append!(&line(cells));
		}
	}
	if rows.len() == 3 {
		append!(&rule("╞", "═", "╪", "╡"));
	}
	append!(&rule("└", "─", "┴", "┘"));
	Ok(true)
}

#[cfg(test)]
mod tests {
	use super::*;
	use p::NamedFrom;
	fn strings(rows: usize, columns: usize, name: &str, value: &str) -> p::DataFrame {
		p::DataFrame::new(
			rows,
			(0..columns)
				.map(|i| p::Series::new(format!("{name}{i}").into(), vec![value; rows]).into())
				.collect(),
		)
		.unwrap()
	}
	/// The table's cells, row by row: the name, `---` and dtype rows, then
	/// the data rows; borders, the shape line and padding removed. Only for
	/// the simple cells these tests build (no `│` or `┆` inside a cell).
	fn grid(text: &str) -> Vec<Vec<String>> {
		text.lines()
			.filter(|l| l.starts_with('│'))
			.map(|l| {
				l.trim_start_matches('│')
					.trim_end_matches('│')
					.split('┆')
					.map(|c| c.trim().to_owned())
					.collect()
			})
			.collect()
	}
	/// Cells read from the frame by `render`, through the test hook.
	fn reads(frame: &p::DataFrame) -> (String, usize) {
		READS.with(|n| n.set(0));
		let text = render(frame).unwrap();
		(text, READS.with(|n| n.get()))
	}
	/// Every table line has the same display width (unicode-width's model).
	fn aligned(text: &str) {
		let widths: Vec<usize> = text.lines().skip(1).map(UnicodeWidthStr::width).collect();
		assert!(
			widths.windows(2).all(|w| w[0] == w[1]),
			"{widths:?}\n{text}"
		);
	}
	/// Record 0158: the user's two ipython frames (Python Polars' default
	/// display), byte for byte, with the preview's final newline.
	#[test]
	fn the_users_reference_frames() {
		let one =
			p::DataFrame::new(3, vec![p::Series::new("k1".into(), [1i64, 2, 3]).into()]).unwrap();
		assert_eq!(
			render(&one).unwrap(),
			"shape: (3, 1)\n┌─────┐\n│ k1  │\n│ --- │\n│ i64 │\n╞═════╡\n│ 1   │\n│ 2   │\n│ 3   │\n└─────┘\n"
		);
		let two = p::DataFrame::new(
			3,
			vec![
				p::Series::new("k1".into(), [1i64, 2, 3]).into(),
				p::Series::new("k2".into(), [4i64, 5, 6]).into(),
			],
		)
		.unwrap();
		assert_eq!(
			render(&two).unwrap(),
			"shape: (3, 2)\n┌─────┬─────┐\n│ k1  ┆ k2  │\n│ --- ┆ --- │\n│ i64 ┆ i64 │\n╞═════╪═════╡\n│ 1   ┆ 4   │\n│ 2   ┆ 5   │\n│ 3   ┆ 6   │\n└─────┴─────┘\n"
		);
	}
	/// Review of 0158: empty frames are laid out as Polars lays them out, and
	/// nothing is read.
	#[test]
	fn empty_frames() {
		let (text, n) = reads(&p::DataFrame::empty());
		assert_eq!((text.as_str(), n), ("shape: (0, 0)\n┌┐\n╞╡\n└┘\n", 0));
		let (text, n) = reads(&p::DataFrame::new(3, vec![]).unwrap());
		assert_eq!((text.as_str(), n), ("shape: (3, 0)\n┌┐\n╞╡\n└┘\n", 0));
		let no_rows = p::DataFrame::new(
			0,
			vec![
				p::Series::new("a".into(), Vec::<i64>::new()).into(),
				p::Series::new("bb".into(), Vec::<String>::new()).into(),
			],
		)
		.unwrap();
		let (text, n) = reads(&no_rows);
		assert_eq!(n, 0);
		assert_eq!(
			text,
			"shape: (0, 2)\n┌─────┬────────┐\n│ a   ┆ bb     │\n│ --- ┆ ---    │\n│ i64 ┆ string │\n╞═════╪════════╡\n└─────┴────────┘\n"
		);
	}
	/// Rows past `ROWS`: the first and last `ROWS / 2` with an elided row;
	/// columns past `COLUMNS`: the first and last `COLUMNS / 2` with an
	/// elided column (record 0124). Only shown cells are read.
	#[test]
	fn dimensions_and_row_column_boundaries() {
		for (rows, columns) in [(0, 1), (1, 1), (9, 7), (10, 8), (11, 9), (10, 9), (11, 8)] {
			let frame = p::DataFrame::new(
				rows,
				(0..columns)
					.map(|c| {
						let values: Vec<String> = (0..rows).map(|r| format!("r{r}c{c}")).collect();
						p::Series::new(format!("column{c}").into(), values).into()
					})
					.collect(),
			)
			.unwrap();
			let (text, n) = reads(&frame);
			assert!(text.starts_with(&format!("shape: ({rows}, {columns})\n")));
			assert_eq!(n, rows.min(ROWS) * columns.min(COLUMNS), "{text}");
			let g = grid(&text);
			let shown_rows: Vec<Option<usize>> = shown_rows(rows);
			let shown: Vec<Option<usize>> = shown_columns(columns);
			assert_eq!(g.len(), 3 + shown_rows.len(), "{text}");
			for (i, c) in shown.iter().enumerate() {
				match c {
					Some(c) => assert_eq!(
						(g[0][i].as_str(), g[1][i].as_str(), g[2][i].as_str()),
						(format!("column{c}").as_str(), "---", "string")
					),
					None => assert_eq!(
						(g[0][i].as_str(), g[1][i].as_str(), g[2][i].as_str()),
						("…", "", "")
					),
				}
				for (j, r) in shown_rows.iter().enumerate() {
					let want = match (r, c) {
						(Some(r), Some(c)) => format!("r{r}c{c}"),
						_ => "…".to_owned(),
					};
					assert_eq!(g[3 + j][i], want, "{text}");
				}
			}
			assert_eq!(text.contains('…'), rows > ROWS || columns > COLUMNS);
			if rows > ROWS {
				assert!(text.contains("r0c0") && text.contains(&format!("r{}c0", rows - 1)));
				assert!(!text.contains(&format!("r{}c0", ROWS / 2)));
			}
			assert!(text.ends_with("┘\n"));
			aligned(&text);
		}
	}
	/// Review of 0158: hazardous values in omitted columns are never read:
	/// an out-of-range datetime, a deep list, a huge string and a hostile
	/// zone, in the four middle columns of twelve.
	#[test]
	fn omitted_hazardous_columns_are_never_read() {
		use p::{IntoSeries, NamedFrom};
		let mut deep = p::Series::new("".into(), [1i64]);
		for _ in 0..200 {
			deep = p::Series::new("".into(), [deep]);
		}
		let zone = unsafe { p::TimeZone::new_unchecked("\u{1b}[2J".repeat(1000)) };
		let hazards: Vec<p::Column> = vec![
			p::Int64Chunked::new("h0".into(), &[i64::MAX])
				.into_datetime(p::TimeUnit::Milliseconds, None)
				.into_series()
				.into(),
			deep.with_name("h1".into()).into(),
			p::Series::new("h2".into(), ["\u{202e}".repeat(1_000_000)]).into(),
			p::Int64Chunked::new("h3".into(), &[0i64])
				.into_datetime(p::TimeUnit::Milliseconds, Some(zone))
				.into_series()
				.into(),
		];
		let mut columns: Vec<p::Column> = (0..4)
			.map(|i| p::Series::new(format!("a{i}").into(), [i as i64]).into())
			.collect();
		columns.extend(hazards);
		columns.extend((0..4).map(|i| p::Series::new(format!("z{i}").into(), [i as i64]).into()));
		let (text, n) = reads(&p::DataFrame::new(1, columns).unwrap());
		assert_eq!(n, COLUMNS, "{text}");
		for unseen in [
			"h0",
			"h1",
			"h2",
			"h3",
			"out of range",
			"\\u{202e}",
			"\\u{1b}",
		] {
			assert!(!text.contains(unseen), "{unseen} in {text}");
		}
		aligned(&text);
	}
	/// Display width: wide and combining characters and an emoji ZWJ
	/// sequence align by unicode-width's model (not every terminal's).
	#[test]
	fn cells_align_by_display_width() {
		let frame = p::DataFrame::new(
			4,
			vec![
				p::Series::new(
					"漢字".into(),
					["漢", "e\u{301}", "👩\u{200d}👩\u{200d}👧", "x"],
				)
				.into(),
				p::Series::new("n".into(), [1i64, 22, 333, 4444]).into(),
			],
		)
		.unwrap();
		let text = render(&frame).unwrap();
		aligned(&text);
		assert_eq!(grid(&text)[5][0], "👩\u{200d}👩\u{200d}👧");
	}
	/// Review of 0158: `…` is a display convention. A literal `…` name or cell
	/// renders as itself; elision is known from the shape, not the text.
	#[test]
	fn a_literal_ellipsis_is_just_text() {
		let frame = p::DataFrame::new(1, vec![p::Series::new("…".into(), ["…"]).into()]).unwrap();
		let text = render(&frame).unwrap();
		assert!(text.starts_with("shape: (1, 1)\n"));
		assert_eq!(grid(&text), [["…"], ["---"], ["string"], ["…"]]);
	}
	#[test]
	fn scalar_boundaries_and_escaping() {
		for n in [79, 80, 81, 1_000_000] {
			let s = "🦀".repeat(n);
			let text = plain(&s);
			assert_eq!(text.matches('🦀').count(), n.min(SCALARS));
			assert_eq!(text.ends_with(CUT), n > SCALARS);
		}
		assert_eq!(plain("null"), "null");
		assert_eq!(plain("\u{1b}\t\n\r\0\"\\"), "\\u{1b}\\t\\n\\r\\u{0}\"\\\\");
		assert_eq!(plain("e\u{301}🦀"), "e\u{301}🦀");
		// review of 0158: unquoted text stays unambiguous, written a scalar at
		// a time: a backslash and quote, a quote, a backslash and n, a newline
		let shown: Vec<String> = ["\\\"", "\"", "\\n", "\n"]
			.iter()
			.map(|s| plain(s))
			.collect();
		assert_eq!(shown, ["\\\\\"", "\"", "\\\\n", "\\n"]);
		// the cut is whole: 80 escapes, then the marker
		let cut = plain(&"\u{202e}".repeat(81));
		assert_eq!(cut, format!("{}{CUT}", "\\u{202e}".repeat(SCALARS)));
		let text = render(&strings(
			1,
			1,
			&"\u{1b}".repeat(1000),
			&"\u{1b}".repeat(1000),
		))
		.unwrap();
		assert!(!text.contains('\u{1b}'));
		assert_eq!(text.matches("\\u{1b}").count(), 160);
	}
	#[test]
	fn total_byte_boundary_and_complete_tokens() {
		let mut text = String::new();
		let mut sink = |t: &str| {
			text.push_str(t);
			true
		};
		let mut out = Capped::new(&mut sink);
		assert!(out.push(&"x".repeat(BYTES - OMITTED.len())));
		assert!(!out.push("x"));
		assert!(!out.push("y"));
		assert_eq!(text.len(), BYTES);
		assert!(text.ends_with(OMITTED));
		let big = strings(10, 8, "column", &"\u{1b}".repeat(80));
		let text = render(&big).unwrap();
		assert!(text.len() <= BYTES && text.ends_with(OMITTED));
		assert!(!text.contains('\u{1b}'));
		// review of 0158: whole lines only, and no bottom border on a cut table
		let body = text.strip_suffix(OMITTED).unwrap();
		assert!(
			body.lines()
				.all(|l| l.starts_with("shape: ") || l.ends_with(['│', '┐', '╡'])),
			"{body}"
		);
		assert!(!text.contains('└'));
		// a first border that cannot fit: the shape line, then the marker
		let wide = strings(1, 8, &"\u{1b}".repeat(80), "x");
		let text = render(&wide).unwrap();
		assert_eq!(text, format!("shape: (1, 8)\n{OMITTED}"));
	}
	#[test]
	fn a_presenter_sink_gets_the_same_bounded_text_as_the_explicit_preview() {
		// A frame that exhausts the adapter cap: ten rows, eight columns, eighty
		// crab scalars per cell (the reviewer's counterexample).
		let big = strings(10, 8, "column", &"🦀".repeat(80));
		let explicit = render(&big).unwrap();
		assert!(explicit.len() < BYTES && explicit.ends_with(OMITTED));
		let mut automatic = String::new();
		let complete = render_into(&big, &mut |t| {
			automatic.push_str(t);
			true
		})
		.unwrap();
		assert!(!complete);
		assert_eq!(automatic, explicit);
		// A smaller outer sink truncates earlier and keeps its own marker.
		let mut small = String::new();
		let complete = render_into(&big, &mut |t| {
			if small.len() + t.len() > 500 {
				small.push_str("<outer>");
				false
			} else {
				small.push_str(t);
				true
			}
		})
		.unwrap();
		assert!(!complete && small.ends_with("<outer>") && small.len() <= 500 + "<outer>".len());
		assert!(!small.contains(OMITTED));
	}
	/// The work behind a presentation is structural, not proportional to
	/// the frame: the sink sees one more line (the elided row) for 200,000
	/// rows by 40 columns than for 10 by 8, exactly ROWS × COLUMNS cells are
	/// read, and each string scalar is cut at SCALARS characters before it is
	/// laid out. Output bytes are bounded separately by the cap.
	#[test]
	fn inspection_is_bounded_by_structure_not_frame_size() {
		fn tokens(frame: &p::DataFrame) -> (usize, usize, usize) {
			let (mut count, mut cells, mut bytes) = (0, 0, 0);
			READS.with(|n| n.set(0));
			render_into(frame, &mut |t| {
				count += 1;
				bytes += t.len();
				cells += t.matches("cell").count();
				true
			})
			.unwrap();
			assert_eq!(READS.with(|n| n.get()), cells);
			(count, cells, bytes)
		}
		let small = tokens(&strings(10, 8, "column", "cell"));
		let huge = tokens(&strings(200_000, 40, "column", "cell"));
		// one line more (the elided row), each line one elided column wider
		assert_eq!(huge.1, ROWS * COLUMNS);
		assert_eq!(
			(huge.0, huge.1),
			(small.0 + 1, small.1),
			"{small:?} {huge:?}"
		);
		assert!(huge.2 - small.2 < 2_000, "{small:?} {huge:?}");
		// Long scalars: at most SCALARS characters of each of the ROWS ×
		// COLUMNS visited cells reach the sink, whatever their length.
		let long = strings(12, 9, "column", &"🦀".repeat(5_000));
		let mut scalars = 0;
		let mut pushed = 0;
		let mut bytes = 0;
		let complete = render_into(&long, &mut |t| {
			pushed += 1;
			bytes += t.len();
			let count = t.matches('🦀').count();
			// a line holds at most COLUMNS cells of SCALARS each
			assert!(count <= COLUMNS * SCALARS, "{count} scalars in one token");
			scalars += count;
			true
		})
		.unwrap();
		// The byte cap stops this frame long before its 80 cells; the count
		// of scalars that reached the sink is bounded by both limits.
		assert!(!complete && bytes <= BYTES);
		assert!(
			scalars <= ROWS * COLUMNS * SCALARS && pushed <= small.0 + 1,
			"{scalars} {pushed}"
		);
	}
	/// Rendering reads the frame and changes nothing: the same text on
	/// every call, and the frame equal to its clone afterwards.
	#[test]
	fn rendering_is_deterministic_and_leaves_the_frame_unchanged() {
		let frame = strings(11, 9, "column", "cell");
		let copy = frame.clone();
		let first = render(&frame).unwrap();
		for _ in 0..10 {
			assert_eq!(render(&frame).unwrap(), first);
		}
		assert!(frame.equals_missing(&copy));
		assert_eq!(frame.shape(), (11, 9));
	}
	#[test]
	fn null_strings_and_numeric_spelling() {
		let f = p::DataType::Float64;
		assert_eq!(cell(p::AnyValue::Null, &p::DataType::Null), "null");
		assert_eq!(
			cell(p::AnyValue::String("null"), &p::DataType::String),
			"null"
		);
		for v in [
			-0.0,
			0.0,
			f64::MIN_POSITIVE,
			f64::MAX,
			f64::from_bits(1),
			1.2345678901234567,
		] {
			let text = cell(p::AnyValue::Float64(v), &f);
			assert!(text.len() <= SCALARS);
			assert_eq!(text.parse::<f64>().unwrap().to_bits(), v.to_bits());
		}
		assert_eq!(
			cell(p::AnyValue::Int64(i64::MIN), &p::DataType::Int64),
			i64::MIN.to_string()
		);
		assert_eq!(
			cell(p::AnyValue::Boolean(false), &p::DataType::Boolean),
			"false"
		);
	}

	/// Record 0124: no dtype refuses the preview. Every dtype this build has
	/// renders (the four legacy names unchanged; the rest in Polars' own text),
	/// and each cell stays within the scalar bound.
	#[test]
	fn every_dtype_renders_within_the_bound() {
		use p::NamedFrom;
		let base = p::Series::new("x".into(), [Some(1i64), None, Some(300)]);
		let mut columns: Vec<p::Column> = vec![
			base.clone().into(),
			p::Series::new("s".into(), [Some("a"), None, Some("ccc")]).into(),
			p::Series::new("f".into(), [Some(1.5f64), None, Some(-0.0)]).into(),
			p::Series::new("b".into(), [Some(true), None, Some(false)]).into(),
			p::Series::new("f32".into(), [Some(1.5f32), None, Some(2.25)]).into(),
			p::Series::new("bin".into(), [Some(&b"\x00ab"[..]), None, Some(&b""[..])]).into(),
			p::Series::new_null("n".into(), 3).into(),
		];
		// every integer width and float the build supports, from the i64 base
		for dt in [
			p::DataType::Int8,
			p::DataType::Int16,
			p::DataType::Int32,
			p::DataType::UInt8,
			p::DataType::UInt16,
			p::DataType::UInt32,
			p::DataType::UInt64,
			p::DataType::Float32,
			p::DataType::Date,
			p::DataType::Datetime(p::TimeUnit::Milliseconds, None),
			p::DataType::Duration(p::TimeUnit::Milliseconds),
			p::DataType::Time,
		] {
			// a dtype the build does not enable is skipped, not an error
			if let Ok(s) = base.cast(&dt) {
				columns.push(s.with_name(format!("c_{dt}").as_str().into()).into());
			}
		}
		let lists = p::Series::new(
			"l".into(),
			[
				p::Series::new("".into(), [1i64, 2]),
				p::Series::new("".into(), [3i64]),
			],
		);
		// in groups the preview shows whole, so every dtype reaches the render
		let text: String = columns
			.chunks(COLUMNS)
			.map(|group| render(&p::DataFrame::new(3, group.to_vec()).unwrap()).unwrap())
			.collect();
		assert!(
			!text.contains("unsupported") && !text.contains('…'),
			"{text}"
		);
		let headers: Vec<(String, String)> = text
			.split("shape: ")
			.skip(1)
			.flat_map(|t| {
				let g = grid(t);
				g[0].clone().into_iter().zip(g[2].clone())
			})
			.collect();
		for want in [
			("x", "i64"),
			("s", "string"),
			("b", "bool"),
			("c_date", "date"),
		] {
			assert!(
				headers.contains(&(want.0.to_owned(), want.1.to_owned())),
				"{want:?} {headers:?}"
			);
		}
		assert!(text.contains("b\"\\x00ab\""), "{text}");
		let frame = p::DataFrame::new(3, columns).unwrap();
		assert_eq!(cell(lists.get(0).unwrap(), lists.dtype()), "[1, 2]");
		assert_eq!(dtype(lists.dtype()), "list[i64]");
		for c in frame.columns() {
			for row in 0..3 {
				let t = cell(c.get(row).unwrap(), c.dtype());
				assert!(t.chars().count() <= SCALARS + CUT.chars().count(), "{t}");
			}
		}
	}

	/// A huge list cell costs what a small one does: at most SCALARS
	/// characters are built, whatever the list's length.
	#[test]
	fn a_huge_list_cell_is_bounded() {
		use p::NamedFrom;
		let big = p::Series::new("".into(), (0..1_000_000i64).collect::<Vec<_>>());
		let cell_text = list(&big);
		assert!(cell_text.starts_with("[0, 1, 2"), "{cell_text}");
		assert!(cell_text.ends_with(CUT), "{cell_text}");
		assert!(cell_text.chars().count() <= SCALARS + CUT.chars().count());
		// a list of lists nested deeper than the bound (SCALARS levels): the
		// same cost
		let mut deep = p::Series::new("".into(), [1i64]);
		for _ in 0..(2 * SCALARS + 40) {
			deep = p::Series::new("".into(), [deep]);
		}
		let deep_text = list(&deep);
		assert!(
			deep_text.starts_with("[[[[") && deep_text.ends_with(CUT),
			"{deep_text}"
		);
		assert!(deep_text.chars().count() <= SCALARS + CUT.chars().count());
		// a short list that fits is shown whole, with no marker
		let small = p::Series::new("".into(), [1i64, 2, 3]);
		assert_eq!(list(&small), "[1, 2, 3]");
	}

	/// Review of 0124: a zoned datetime. The zone is an arbitrary string, so
	/// its name is copied only up to the bound; a zoned cell shows its UTC
	/// instant, and never reaches Polars' zoned `Display`, which panics in the
	/// 0.55.2 build (no `timezones` feature).
	#[test]
	fn a_zoned_datetime_is_bounded_and_never_panics() {
		use p::{IntoSeries, NamedFrom};
		// any zone Polars may carry: validated or not, the name is bounded
		let long = unsafe { p::TimeZone::new_unchecked("Z".repeat(1_000_000)) };
		let name = dtype(&p::DataType::Datetime(
			p::TimeUnit::Milliseconds,
			Some(long),
		));
		assert!(name.starts_with("datetime[ms, ZZZ"), "{name}");
		assert!(name.chars().count() <= SCALARS + CUT.chars().count());
		let zone = p::TimeZone::opt_try_new(Some("Europe/Paris")).unwrap();
		assert!(zone.is_some());
		// the logical column built directly: a cast from integers drops the zone
		let zoned = p::Int64Chunked::new("t".into(), &[Some(0i64), None, Some(1_700_000_000_000)])
			.into_datetime(p::TimeUnit::Milliseconds, zone)
			.into_series();
		let frame = p::DataFrame::new(3, vec![zoned.into()]).unwrap();
		let text = render(&frame).unwrap();
		let g = grid(&text);
		assert_eq!(
			(g[0][0].as_str(), g[2][0].as_str()),
			("t", "datetime[ms, Europe/Paris]"),
			"{text}"
		);
		let cells: Vec<&str> = g[3..].iter().map(|r| r[0].as_str()).collect();
		assert_eq!(
			cells,
			["1970-01-01 00:00:00 UTC", "null", "2023-11-14 22:13:20 UTC"]
		);
	}

	/// Review of 0124, round 2: a hostile zone. Its scalars are escaped like a
	/// name's, counted against the name's budget, and never split; the
	/// header and the frame's whole text carry no raw control.
	#[test]
	fn a_hostile_zone_is_escaped_whole_within_the_bound() {
		use p::{IntoSeries, NamedFrom};
		let hostile = "\u{1b}[2J\nevil\t\u{202e}rev\u{2066}\r\"\\";
		let zone = unsafe { p::TimeZone::new_unchecked(hostile) };
		let kind = p::DataType::Datetime(p::TimeUnit::Milliseconds, Some(zone.clone()));
		assert_eq!(
			dtype(&kind),
			"datetime[ms, \\u{1b}[2J\\nevil\\t\\u{202e}rev\\u{2066}\\r\\\"\\\\]"
		);
		let column = p::Int64Chunked::new("t".into(), &[0i64])
			.into_datetime(p::TimeUnit::Milliseconds, Some(zone))
			.into_series();
		let text = render(&p::DataFrame::new(1, vec![column.into()]).unwrap()).unwrap();
		for raw in ['\u{1b}', '\t', '\r', '\u{202e}', '\u{2066}'] {
			assert!(!text.contains(raw), "{raw:?} in {text}");
		}
		// shape, three borders, three header rows, one row: the zone's newline
		// did not add a line
		assert_eq!(text.lines().count(), 8, "{text}");
		assert_eq!(grid(&text)[3][0], "1970-01-01 00:00:00 UTC", "{text}");
		// a zone of nothing but controls, past the bound: every escape whole,
		// as many as the budget has scalars left, then the marker once
		let controls = unsafe { p::TimeZone::new_unchecked("\u{1b}".repeat(10_000)) };
		let name = dtype(&p::DataType::Datetime(
			p::TimeUnit::Milliseconds,
			Some(controls),
		));
		let prefix = "datetime[ms, ";
		let escapes = name
			.strip_prefix(prefix)
			.and_then(|rest| rest.strip_suffix(CUT))
			.unwrap_or_else(|| panic!("{name}"));
		assert_eq!(escapes, "\\u{1b}".repeat(SCALARS - prefix.chars().count()));
	}

	/// Review of 0124: Polars' own `Display` for date, datetime and time
	/// values `expect`s the value in range. An out-of-range value shows a
	/// placeholder instead of panicking.
	#[test]
	fn out_of_range_temporal_values_never_panic() {
		use p::{IntoSeries, NamedFrom};
		// the logical columns built directly from their physical integers:
		// a cast would validate them (an out-of-range time casts to null)
		let columns: Vec<p::Column> = vec![
			p::Int32Chunked::new("d".into(), &[i32::MAX])
				.into_date()
				.into_series()
				.into(),
			p::Int64Chunked::new("dt".into(), &[i64::MAX])
				.into_datetime(p::TimeUnit::Milliseconds, None)
				.into_series()
				.into(),
			p::Int64Chunked::new("t".into(), &[i64::MAX])
				.into_time()
				.into_series()
				.into(),
		];
		// the conversions directly, past the guards Polars' own `_opt`
		// conversions lack: a day number that overflows the epoch addition,
		// and a time whose seconds would wrap in a u32 to 00:16:40
		let last = i32::MAX - polars_arrow::temporal_conversions::EPOCH_DAYS_FROM_CE;
		for days in [last + 1, i32::MAX] {
			let text = temporal(&p::AnyValue::Int32(days), &p::DataType::Date);
			assert_eq!(text.as_deref(), Some("<out of range>"));
		}
		let wraps = (1i64 << 32) * 1_000_000_000 + 1_000 * 1_000_000_000;
		for v in [-1, 86_400 * 1_000_000_000, wraps, i64::MAX] {
			let text = temporal(&p::AnyValue::Int64(v), &p::DataType::Time);
			assert_eq!(text.as_deref(), Some("<out of range>"), "{v}");
		}
		let noon = temporal(
			&p::AnyValue::Int64(43_200 * 1_000_000_000),
			&p::DataType::Time,
		);
		assert_eq!(noon.as_deref(), Some("12:00:00"));
		let frame = p::DataFrame::new(1, columns).unwrap();
		let text = render(&frame).unwrap();
		// Polars itself nulls an out-of-range time when the column is built;
		// a date and a datetime keep theirs and reach the preview
		assert_eq!(
			grid(&text)[3],
			["<out of range>", "<out of range>", "null"],
			"{text}"
		);
	}

	/// Review of 0124, round 3: a list cell is bounded in input scalars and
	/// never cut inside an escape or a token, at any depth (an output-side
	/// cut split `\u{202e}` and `\x00`).
	#[test]
	fn a_list_cell_keeps_every_escape_and_token_whole() {
		use p::NamedFrom;
		let marker = CUT;
		// a string element of bidi overrides: `[` and `"` take two scalars
		let bidi = "\u{202e}".repeat(200);
		let strings = p::Series::new("".into(), [bidi.as_str()]);
		assert_eq!(
			list(&strings),
			format!("[\"{}\"{marker}", "\\u{202e}".repeat(SCALARS - 2))
		);
		// the same one level deeper: one more `[`
		let nested = p::Series::new("".into(), [strings.clone()]);
		assert_eq!(
			list(&nested),
			format!("[[\"{}\"{marker}", "\\u{202e}".repeat(SCALARS - 3))
		);
		// a binary element of zero bytes: `[` and `b"` take two scalars
		let zeros = vec![0u8; 200];
		let bytes = p::Series::new("".into(), [zeros.as_slice()]);
		assert_eq!(
			list(&bytes),
			format!("[b\"{}\"{marker}", "\\x00".repeat(SCALARS - 2))
		);
		let nested = p::Series::new("".into(), [bytes]);
		assert_eq!(
			list(&nested),
			format!("[[b\"{}\"{marker}", "\\x00".repeat(SCALARS - 3))
		);
		// tokens are whole: `[` + 13 × `1234` + 13 separators is 79 scalars;
		// the 14th `1234` does not fit and is not shown as `1`
		let numbers = p::Series::new("".into(), vec![1234i64; 100]);
		assert_eq!(
			list(&numbers),
			format!("[{}, {marker}", vec!["1234"; 13].join(", "))
		);
		// a short list is shown whole, escapes and all, with no marker
		let short = p::Series::new("".into(), ["a\u{1b}b", "\""]);
		assert_eq!(list(&short), "[\"a\\u{1b}b\", \"\\\"\"]");
	}

	/// Review of 0124: a very large nested schema is named without formatting
	/// it whole; the name stops at the bound however deep the nesting.
	#[test]
	fn a_deep_nested_dtype_is_named_within_the_bound() {
		let mut kind = p::DataType::Int64;
		for _ in 0..2_000 {
			kind = p::DataType::List(Box::new(kind));
		}
		let name = dtype(&kind);
		assert!(
			name.starts_with("list[list[") && name.chars().count() <= SCALARS + CUT.chars().count(),
			"{name}"
		);
		// the flat dtypes keep Polars' text, the legacy four their spelling
		assert_eq!(dtype(&p::DataType::UInt32), "u32");
		assert_eq!(dtype(&p::DataType::String), "string");
	}
}
