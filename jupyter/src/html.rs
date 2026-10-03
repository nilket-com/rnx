//! Record 0159: a top-level result's optional HTML form. The worker checks
//! it before sending; the kernel checks it again here, with a copy of the
//! worker's allowlist run against the same corpus
//! (`tests/fixtures/html_allowlist.json` at the repository root). A refused
//! form is dropped and the result is published as text alone: never a
//! protocol failure.
use serde_json::{Value, json};

/// The bound of either form, the worker's `render_bytes`.
pub const HTML_BYTES: usize = 16_384;

/// Tags from a fixed list with no attributes, balanced and at most `DEPTH`
/// deep; text with no raw `<` or `>`, `&` only in five forms, and no control
/// or bidi control except a newline. One iterative pass.
pub fn allowed(html: &str) -> bool {
	const TAGS: [&str; 8] = ["div", "small", "table", "thead", "tbody", "tr", "th", "td"];
	const ENTITIES: [&str; 5] = ["&amp;", "&lt;", "&gt;", "&quot;", "&#39;"];
	const DEPTH: usize = 8;
	if html.len() > HTML_BYTES {
		return false;
	}
	let mut open: Vec<&str> = Vec::new();
	let mut rest = html;
	while let Some(ch) = rest.chars().next() {
		match ch {
			'<' => {
				let Some(end) = rest.find('>') else {
					return false;
				};
				let inner = &rest[1..end];
				let (closing, name) = match inner.strip_prefix('/') {
					Some(name) => (true, name),
					None => (false, inner),
				};
				if !TAGS.contains(&name) {
					return false;
				}
				if closing {
					if open.pop() != Some(name) {
						return false;
					}
				} else {
					if open.len() == DEPTH {
						return false;
					}
					open.push(name);
				}
				rest = &rest[end + 1..];
			}
			'>' => return false,
			'&' => {
				let Some(entity) = ENTITIES.iter().find(|e| rest.starts_with(**e)) else {
					return false;
				};
				rest = &rest[entity.len()..];
			}
			ch if (ch.is_control() && ch != '\n')
				|| matches!(ch, '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
			{
				return false;
			}
			ch => rest = &rest[ch.len_utf8()..],
		}
	}
	open.is_empty()
}

/// The `execute_result` data for a settled reply: `text/plain`, plus
/// `text/html` when the worker sent an allowed one. `None` without a text
/// result: HTML never stands alone.
pub fn result_data(reply: &Value) -> Option<Value> {
	let text = reply["text_plain"].as_str()?;
	let mut data = json!({"text/plain": text});
	if let Some(html) = reply
		.get("text_html")
		.and_then(Value::as_str)
		.filter(|h| allowed(h))
	{
		data["text/html"] = json!(html);
	}
	Some(data)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_html_allowlist_corpus() {
		let corpus: Value =
			serde_json::from_str(include_str!("../../tests/fixtures/html_allowlist.json")).unwrap();
		for case in corpus["accept"].as_array().unwrap() {
			assert!(allowed(case.as_str().unwrap()), "refused {case}");
		}
		for case in corpus["reject"].as_array().unwrap() {
			assert!(!allowed(case.as_str().unwrap()), "accepted {case}");
		}
		assert!(!allowed(&"<div>".repeat(100_000)));
		assert!(!allowed(&format!("<div>{}</div>", "x".repeat(HTML_BYTES))));
	}

	#[test]
	fn a_refused_or_absent_html_form_leaves_text_alone() {
		let good = "<div>ok</div>";
		let data = result_data(&json!({"text_plain": "t", "text_html": good})).unwrap();
		assert_eq!(data, json!({"text/plain": "t", "text/html": good}));
		let oversized = format!("<div>{}</div>", "x".repeat(HTML_BYTES));
		for html in [
			json!(null),
			json!("<td onclick=x>1</td>"),
			json!(oversized),
			json!(42),
			json!({"html": good}),
		] {
			let data = result_data(&json!({"text_plain": "t", "text_html": html})).unwrap();
			assert_eq!(data, json!({"text/plain": "t"}), "{html}");
		}
		// an older worker's reply, with no field at all
		assert_eq!(
			result_data(&json!({"text_plain": "t"})).unwrap(),
			json!({"text/plain": "t"})
		);
		// HTML never stands alone
		assert_eq!(
			result_data(&json!({"text_plain": null, "text_html": good})),
			None
		);
	}
}
