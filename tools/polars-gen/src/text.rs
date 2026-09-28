/// `A + B` split at top level.
pub(crate) fn split_top_plus(s: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut depth = 0i32;
	let mut cur = String::new();
	for ch in s.chars() {
		match ch {
			'<' | '(' | '[' => {
				depth += 1;
				cur.push(ch);
			}
			'>' | ')' | ']' => {
				depth -= 1;
				cur.push(ch);
			}
			'+' if depth == 0 => {
				out.push(cur.trim().to_string());
				cur.clear();
			}
			_ => cur.push(ch),
		}
	}
	if !cur.trim().is_empty() {
		out.push(cur.trim().to_string());
	}
	out
}

pub(crate) fn split_top_on(s: &str, sep: char) -> Vec<String> {
	let mut out = Vec::new();
	let mut depth = 0i32;
	let mut cur = String::new();
	for ch in s.chars() {
		match ch {
			'<' | '(' | '[' => depth += 1,
			'>' | ')' | ']' => depth -= 1,
			_ => {}
		}
		if ch == sep && depth == 0 {
			out.push(cur.trim().to_string());
			cur.clear();
		} else {
			cur.push(ch);
		}
	}
	if !cur.trim().is_empty() {
		out.push(cur.trim().to_string());
	}
	out
}

/// Whether `ident` occurs in `text` as a whole identifier.
pub(crate) fn mentions(text: &str, ident: &str) -> bool {
	let mut from = 0;
	while let Some(i) = text[from..].find(ident) {
		let start = from + i;
		let end = start + ident.len();
		let before_ok = start == 0
			|| !text.as_bytes()[start - 1].is_ascii_alphanumeric()
				&& text.as_bytes()[start - 1] != b'_';
		let after_ok = end == text.len()
			|| !text.as_bytes()[end].is_ascii_alphanumeric() && text.as_bytes()[end] != b'_';
		if before_ok && after_ok {
			return true;
		}
		from = end;
	}
	false
}

pub(crate) fn sanitize(s: &str) -> String {
	s.chars()
		.map(|c| if c.is_alphanumeric() { c } else { '_' })
		.collect()
}

/// Split a generic argument list at its top-level commas.
pub(crate) fn split_top(s: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut depth = 0i32;
	let mut cur = String::new();
	for ch in s.chars() {
		match ch {
			'<' | '(' | '[' => {
				depth += 1;
				cur.push(ch);
			}
			'>' | ')' | ']' => {
				depth -= 1;
				cur.push(ch);
			}
			',' if depth == 0 => {
				out.push(cur.trim().to_string());
				cur.clear();
			}
			_ => cur.push(ch),
		}
	}
	if !cur.trim().is_empty() {
		out.push(cur.trim().to_string());
	}
	out
}

/// Record 0090: replace `from` in `ty` only where it stands as a whole token:
/// not preceded by an identifier character or `:`, not followed by one.
pub(crate) fn replace_token(ty: &str, from: &str, to: &str) -> String {
	let ident = |c: char| c.is_alphanumeric() || c == '_';
	let mut out = String::new();
	let mut i = 0;
	while let Some(k) = ty[i..].find(from) {
		let at = i + k;
		let end = at + from.len();
		let before_ok = ty[..at]
			.chars()
			.next_back()
			.is_none_or(|c| !ident(c) && c != ':');
		let after_ok = ty[end..].chars().next().is_none_or(|c| !ident(c));
		out.push_str(&ty[i..at]);
		out.push_str(if before_ok && after_ok { to } else { from });
		i = end;
	}
	out.push_str(&ty[i..]);
	out
}
