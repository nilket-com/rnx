//! Inspect bounded slices only. Never format a whole frame or arbitrary AnyValue.
use crate::p;
const ROWS: usize = 10;
const COLUMNS: usize = 8;
const SCALARS: usize = 80;
const BYTES: usize = 8192;
const OMITTED: &str = "\n[preview byte limit; remainder omitted]\n";

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

fn quoted(value: &str) -> String {
	let mut text = String::from("\"");
	let mut end = 0;
	for (offset, ch) in value.char_indices().take(SCALARS) {
		end = offset + ch.len_utf8();
		match ch {
			'"' => text.push_str("\\\""),
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
	text.push('"');
	// Byte length reveals remaining input without inspecting an 81st scalar.
	if end < value.len() {
		text.push_str("…[truncated]");
	}
	text
}
fn dtype(kind: &p::DataType) -> Result<&'static str, String> {
	match kind {
		p::DataType::String => Ok("string"),
		p::DataType::Int64 => Ok("i64"),
		p::DataType::Float64 => Ok("f64"),
		p::DataType::Boolean => Ok("bool"),
		_ => Err("polars preview: unsupported column dtype".into()),
	}
}
fn cell(value: p::AnyValue<'_>) -> Result<String, String> {
	Ok(match value {
		p::AnyValue::Null => "null".into(),
		p::AnyValue::String(s) => quoted(s),
		p::AnyValue::StringOwned(s) => quoted(s.as_str()),
		p::AnyValue::Int64(v) => v.to_string(),
		p::AnyValue::Float64(v) => format!("{v:?}"),
		p::AnyValue::Boolean(v) => v.to_string(),
		_ => return Err("polars preview: unsupported cell dtype".into()),
	})
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
/// Render into any sink under the adapter's `BYTES` cap and omission marker.
/// The sink returns `false` when it is full (the session presenter's outer
/// budget); rendering then stops and `Ok(false)` says the output is partial.
/// Only structurally bounded slices are visited: at most `ROWS` rows and
/// `COLUMNS` columns, each scalar cut at `SCALARS`.
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
		"DataFrame: {} rows × {} columns\n",
		frame.height(),
		frame.width()
	));
	if frame.height() > ROWS || frame.width() > COLUMNS {
		append!(&format!(
			"[{} rows and {} columns omitted by display limits]\n",
			frame.height().saturating_sub(ROWS),
			frame.width().saturating_sub(COLUMNS)
		));
	}
	for (i, column) in frame.columns().iter().take(COLUMNS).enumerate() {
		if i != 0 {
			append!(" | ");
		}
		append!(&quoted(column.name().as_str()));
		append!(": ");
		append!(dtype(column.dtype())?);
	}
	append!("\n");
	for row in 0..frame.height().min(ROWS) {
		for (i, column) in frame.columns().iter().take(COLUMNS).enumerate() {
			if i != 0 {
				append!(" | ");
			}
			let value = column
				.get(row)
				.map_err(|e| format!("polars preview: {e}"))?;
			append!(&cell(value)?);
		}
		append!("\n");
	}
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
	#[test]
	fn dimensions_and_row_column_boundaries() {
		for (rows, columns) in [(0, 1), (9, 7), (10, 8), (11, 9)] {
			let text = render(&strings(rows, columns, "column", "cell")).unwrap();
			assert!(text.starts_with(&format!("DataFrame: {rows} rows × {columns} columns\n")));
			assert_eq!(
				text.matches("\"cell\"").count(),
				rows.min(ROWS) * columns.min(COLUMNS)
			);
			assert_eq!(
				text.contains("omitted by display limits"),
				rows > ROWS || columns > COLUMNS
			);
			assert!(!text.contains("column8"));
		}
	}
	#[test]
	fn scalar_boundaries_and_escaping() {
		for n in [79, 80, 81, 1_000_000] {
			let s = "🦀".repeat(n);
			let text = quoted(&s);
			assert_eq!(text.matches('🦀').count(), n.min(SCALARS));
			assert_eq!(text.ends_with("…[truncated]"), n > SCALARS);
		}
		assert_eq!(quoted("null"), "\"null\"");
		assert_eq!(
			quoted("\u{1b}\t\n\r\0\"\\"),
			"\"\\u{1b}\\t\\n\\r\\u{0}\\\"\\\\\""
		);
		assert_eq!(quoted("e\u{301}🦀"), "\"e\u{301}🦀\"");
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
	/// the frame: the sink sees the same token count for 200,000 rows by 40
	/// columns as for 10 by 8, exactly ROWS × COLUMNS cells are inspected,
	/// and each string scalar is cut at SCALARS characters before it is
	/// pushed. Output bytes are bounded separately by the cap.
	#[test]
	fn inspection_is_bounded_by_structure_not_frame_size() {
		fn tokens(frame: &p::DataFrame) -> (usize, usize, usize) {
			let (mut count, mut cells, mut bytes) = (0, 0, 0);
			render_into(frame, &mut |t| {
				count += 1;
				bytes += t.len();
				if t.starts_with("\"cell") {
					cells += 1;
				}
				true
			})
			.unwrap();
			(count, cells, bytes)
		}
		let small = tokens(&strings(10, 8, "column", "cell"));
		let huge = tokens(&strings(200_000, 40, "column", "cell"));
		// One more token for the omission line and a longer title: nothing
		// else grows with the frame.
		assert_eq!(huge.1, ROWS * COLUMNS);
		assert_eq!(
			(huge.0, huge.1),
			(small.0 + 1, small.1),
			"{small:?} {huge:?}"
		);
		assert!(huge.2 - small.2 < 100, "{small:?} {huge:?}");
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
			assert!(count <= SCALARS, "{count} scalars in one token");
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
		assert_eq!(cell(p::AnyValue::Null).unwrap(), "null");
		assert_eq!(cell(p::AnyValue::String("null")).unwrap(), "\"null\"");
		for v in [
			-0.0,
			0.0,
			f64::MIN_POSITIVE,
			f64::MAX,
			f64::from_bits(1),
			1.2345678901234567,
		] {
			let text = cell(p::AnyValue::Float64(v)).unwrap();
			assert!(text.len() <= SCALARS);
			assert_eq!(text.parse::<f64>().unwrap().to_bits(), v.to_bits());
		}
		assert_eq!(
			cell(p::AnyValue::Int64(i64::MIN)).unwrap(),
			i64::MIN.to_string()
		);
		assert_eq!(cell(p::AnyValue::Boolean(false)).unwrap(), "false");
	}
}
