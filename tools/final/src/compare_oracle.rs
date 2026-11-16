//! The Polars oracle report's unordered rows. Where a case's policy leaves
//! row order unspecified, its rendered output is compared as a multiset of
//! complete rows: the verb, shape, columns and types, and every row with
//! its multiplicity stay compared. Only the two rendered forms below are
//! read; anything else stays raw, so a reordering there still differs.
use serde_json::{Value, json};

/// The canonical form of one unordered detail, or `None` when the detail
/// is not exactly one of the supported forms.
pub fn canonical(detail: &str) -> Option<Value> {
	frame(detail).or_else(|| series(detail))
}

/// `collected shape=(R, C); columns=[...]; rows=[(..) | (..)]`, and the
/// same with `selected`. The row count must equal R and every row must
/// hold C values, so a split inside a value cannot pass unnoticed.
fn frame(detail: &str) -> Option<Value> {
	let (verb, rest) = ["collected", "selected"].iter().find_map(|v| {
		detail
			.strip_prefix(*v)
			.and_then(|r| r.strip_prefix(" shape=("))
			.map(|r| (*v, r))
	})?;
	let (shape, rest) = rest.split_once("); columns=[")?;
	let (rows, columns) = shape.split_once(", ")?;
	let (rows, columns): (usize, usize) = (number(rows)?, number(columns)?);
	if rest.matches("]; rows=[").count() != 1 {
		return None;
	}
	let (names, values) = rest.split_once("]; rows=[")?;
	let values = values.strip_suffix(']')?;
	let mut cells = if values.is_empty() {
		Vec::new()
	} else {
		split(values, " | ")?
	};
	if cells.len() != rows {
		return None;
	}
	for cell in &cells {
		let inner = cell.strip_prefix('(')?.strip_suffix(')')?;
		if split(inner, ", ")?.len() != columns {
			return None;
		}
	}
	cells.sort();
	Some(
		json!({"form": "frame", "verb": verb, "shape": [rows, columns], "columns": names, "rows": cells}),
	)
}

/// `name:Type:[Type(v), ...]`, with a plain name and type and every value
/// written as a constructor.
fn series(detail: &str) -> Option<Value> {
	let (name, rest) = detail.split_once(':')?;
	let (kind, rest) = rest.split_once(':')?;
	let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
	if !plain(name) || !plain(kind) {
		return None;
	}
	let values = rest.strip_prefix('[')?.strip_suffix(']')?;
	let mut cells = if values.is_empty() {
		Vec::new()
	} else {
		split(values, ", ")?
	};
	for cell in &cells {
		let (constructor, tail) = cell.split_once('(')?;
		if !plain(constructor) || !tail.ends_with(')') {
			return None;
		}
	}
	cells.sort();
	Some(json!({"form": "series", "name": name, "type": kind, "values": cells}))
}

fn number(text: &str) -> Option<usize> {
	(!text.is_empty() && text.chars().all(|c| c.is_ascii_digit()))
		.then(|| text.parse().ok())
		.flatten()
}

/// Split at every top-level separator: outside quoted strings (with their
/// backslash escapes) and outside brackets. Unbalanced text is refused.
fn split(text: &str, separator: &str) -> Option<Vec<String>> {
	let (bytes, sep) = (text.as_bytes(), separator.as_bytes());
	let mut parts = Vec::new();
	let mut open: Vec<u8> = Vec::new();
	let (mut quoted, mut escaped, mut start, mut i) = (false, false, 0usize, 0usize);
	// Every byte that steers the scan is ASCII, and a UTF-8 continuation or
	// lead byte is never ASCII, so byte positions where they match are
	// character boundaries.
	while i < bytes.len() {
		let c = bytes[i];
		if quoted {
			match (escaped, c) {
				(true, _) => escaped = false,
				(false, b'\\') => escaped = true,
				(false, b'"') => quoted = false,
				_ => {}
			}
		} else {
			match c {
				b'"' => quoted = true,
				b'(' | b'[' | b'{' => open.push(c),
				b')' | b']' | b'}' => {
					let expected = match c {
						b')' => b'(',
						b']' => b'[',
						_ => b'{',
					};
					if open.pop() != Some(expected) {
						return None;
					}
				}
				_ if open.is_empty() && bytes[i..].starts_with(sep) => {
					parts.push(text.get(start..i)?.to_owned());
					i += sep.len();
					start = i;
					continue;
				}
				_ => {}
			}
		}
		i += 1;
	}
	if quoted || !open.is_empty() {
		return None;
	}
	parts.push(text.get(start..)?.to_owned());
	Some(parts)
}

/// Replace each unordered row's detail by its canonical form where it has
/// one. Ordered rows and every other field are left as they are.
pub fn canonicalize(report: &mut Value) {
	for result in report["results"].as_array_mut().into_iter().flatten() {
		if result["unordered"] == true
			&& let Some(form) = result["detail"].as_str().and_then(canonical)
		{
			result["detail"] = form;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const A: &str = r#"collected shape=(3, 3); columns=[x:Int64,y:String,z:Float64,]; rows=[(Int64(2), String("b"), Float64(2.5)) | (Int64(1), String("a"), Float64(1.5)) | (Int64(3), String("c"), Float64(3.5))]"#;

	fn with(detail: &str) -> Option<Value> {
		canonical(detail)
	}

	#[test]
	fn order_alone_is_not_a_difference() {
		let reordered = r#"collected shape=(3, 3); columns=[x:Int64,y:String,z:Float64,]; rows=[(Int64(3), String("c"), Float64(3.5)) | (Int64(1), String("a"), Float64(1.5)) | (Int64(2), String("b"), Float64(2.5))]"#;
		assert_eq!(with(A).unwrap(), with(reordered).unwrap());
		assert_eq!(
			with("x:Int64:[Int64(1), Int64(2)]"),
			with("x:Int64:[Int64(2), Int64(1)]")
		);
	}

	#[test]
	fn values_multiplicity_shape_and_columns_still_differ() {
		let value = A.replace("Float64(2.5)", "Float64(2.6)");
		let twice = r#"collected shape=(3, 3); columns=[x:Int64,y:String,z:Float64,]; rows=[(Int64(1), String("a"), Float64(1.5)) | (Int64(1), String("a"), Float64(1.5)) | (Int64(3), String("c"), Float64(3.5))]"#;
		let column = A.replace("z:Float64", "z:Float32");
		let verb = A.replace("collected", "selected");
		for other in [value.as_str(), twice, column.as_str(), verb.as_str()] {
			assert_ne!(with(A).unwrap(), with(other).unwrap(), "{other}");
		}
		assert_ne!(
			with("x:Int64:[Int64(1), Int64(1)]"),
			with("x:Int64:[Int64(1), Int64(2)]")
		);
		assert_ne!(with("x:Int64:[Int64(1)]"), with("x:Int32:[Int64(1)]"));
	}

	#[test]
	fn unicode_and_mismatched_brackets_never_panic_or_pass() {
		// Unquoted multibyte text at the top level of a value list or row
		// list reaches the separator scan: stays raw, no panic.
		assert_eq!(with("café é | x"), None);
		assert_eq!(with("x:Int64:[é, Int64(1)]"), None);
		assert_eq!(with("x:Int64:[Int64(1), é]"), None);
		assert_eq!(
			with("collected shape=(2, 1); columns=[y:String,]; rows=[(Int64(1)) | é]"),
			None
		);
		assert_eq!(
			split("é, a", ", "),
			Some(vec!["é".to_owned(), "a".to_owned()])
		);
		assert_eq!(
			with("x:Int64:[Int64(é), Int64(1)]"),
			with("x:Int64:[Int64(1), Int64(é)]")
		);
		let frame =
			|rows: &str| format!("collected shape=(2, 1); columns=[y:String,]; rows=[{rows}]");
		// Quoted Unicode, separators included, is one value; order alone passes.
		assert_eq!(
			with(&frame(r#"(String("é | ü, ß")) | (String("a"))"#)),
			with(&frame(r#"(String("a")) | (String("é | ü, ß"))"#))
		);
		assert!(with(&frame(r#"(String("é | ü, ß")) | (String("a"))"#)).is_some());
		// Unquoted Unicode in a value keeps the value distinct.
		assert_ne!(
			with(&frame("(Utf8(é)) | (Utf8(a))")),
			with(&frame("(Utf8(e)) | (Utf8(a))"))
		);
		// Mismatched or unbalanced brackets are not a supported form.
		for raw in [
			frame("(String(\"a\"]) | (String(\"b\"))"),
			frame("(Int64(1}) | (Int64(2))"),
			"x:Int64:[Int64(1], Int64(2)]".to_owned(),
			frame("(Int64(1))) | ((Int64(2)"),
		] {
			assert_eq!(with(&raw), None, "{raw}");
		}
	}

	#[test]
	fn an_inconsistent_or_unknown_form_stays_raw() {
		// Shape says four rows; three are written.
		assert_eq!(with(&A.replace("shape=(3, 3)", "shape=(4, 3)")), None);
		// A row with a value too few.
		assert_eq!(with(&A.replace(", Float64(2.5)", "")), None);
		// A separator inside a quoted value does not split it.
		let quoted = r#"collected shape=(1, 1); columns=[y:String,]; rows=[(String("a | b, c"))]"#;
		assert!(with(quoted).is_some());
		// Unbalanced, unknown verb, or a free-form error message.
		for raw in [
			"collected shape=(1, 1); columns=[y:String,]; rows=[(String(\"a)]",
			"joined shape=(1, 1); columns=[x:Int64,]; rows=[(Int64(1))]",
			"error: column not found",
		] {
			assert_eq!(with(raw), None, "{raw}");
		}
		let mut report = json!({"results": [{"unordered": true, "detail": "error: x"}, {"unordered": false, "detail": A}]});
		canonicalize(&mut report);
		assert_eq!(report["results"][0]["detail"], "error: x");
		assert_eq!(
			report["results"][1]["detail"], A,
			"ordered rows are never rewritten"
		);
	}
}
