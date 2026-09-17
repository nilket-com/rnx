//! Inspect bounded slices only. Never format a whole frame or arbitrary AnyValue.
use crate::p;
const ROWS: usize = 10;
const COLUMNS: usize = 8;
const SCALARS: usize = 80;
const BYTES: usize = 8192;
const OMITTED: &str = "\n[preview byte limit; remainder omitted]\n";

struct Output {
	text: String,
}
impl Output {
	fn new() -> Self {
		Self {
			text: String::new(),
		}
	}
	// Whole bounded tokens only; reserve the final marker before every append.
	fn push(&mut self, token: &str) -> bool {
		if token.len() > BYTES - OMITTED.len() - self.text.len() {
			return false;
		}
		self.text.push_str(token);
		true
	}
	fn truncated(mut self) -> String {
		self.text.push_str(OMITTED);
		self.text
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
pub(crate) fn render(frame: &p::DataFrame) -> Result<String, String> {
	let mut out = Output::new();
	macro_rules! append {
		($token:expr) => {
			if !out.push($token) {
				return Ok(out.truncated());
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
	Ok(out.text)
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
		let mut out = Output::new();
		assert!(out.push(&"x".repeat(BYTES - OMITTED.len())));
		assert!(!out.push("x"));
		assert_eq!(out.truncated().len(), BYTES);
		let text = render(&strings(11, 9, "🦀", &"\u{1b}".repeat(100))).unwrap();
		assert!(text.len() <= BYTES && text.ends_with(OMITTED));
		assert!(!text.contains('\u{1b}'));
		// Each entire escaped cell is admitted or rejected, never sliced.
		assert_eq!(text.matches("\\u{1b}").count() % 80, 0);
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
