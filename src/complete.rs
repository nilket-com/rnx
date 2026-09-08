//! Tab completion over what the session already knows. Candidates come from
//! the session's tables and the host module's registration record; nothing
//! here compiles or runs Rune. Token boundaries come from Rune's own parser.
use rune::SourceId;
use rune::ast::{self, Kind, Spanned};

/// The names completion draws on, snapshotted from the live session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
	pub bindings: Vec<String>,
	pub declarations: Vec<String>,
	/// Full paths, `host::read` and so on, as registered.
	pub host: Vec<String>,
	pub commands: Vec<String>,
}

/// A completion: the byte range of the whole token containing the cursor,
/// and the candidates that replace it, deduplicated in priority order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
	pub start: usize,
	pub end: usize,
	pub candidates: Vec<String>,
}

/// Complete at `cursor` (a byte offset) in `buffer`, or `None` where no
/// name can stand: inside a string, template, or comment, after `.`, on a
/// leading `::`, or where the cursor is not on an identifier or path.
pub fn complete(buffer: &str, cursor: usize, names: &Names) -> Option<Completion> {
	if cursor > buffer.len() || !buffer.is_char_boundary(cursor) {
		return None;
	}
	if let Some(c) = command(buffer, cursor, names) {
		return Some(c);
	}
	let tokens = tokens(buffer, cursor)?;
	if inside_template(&tokens, buffer.len(), cursor) {
		return None;
	}
	let (start, end, leading_colons, previous) = group_at(&tokens, cursor)?;
	if leading_colons {
		return None;
	}
	if let Some(Kind::Dot) = previous {
		return None;
	}
	let prefix = &buffer[start..cursor];
	let mut candidates = Vec::new();
	if prefix.contains("::") {
		for path in &names.host {
			if path.starts_with(prefix) {
				push_unique(&mut candidates, path);
			}
		}
	} else {
		for name in names.bindings.iter().chain(&names.declarations) {
			if name.starts_with(prefix) {
				push_unique(&mut candidates, name);
			}
		}
		if "host::".starts_with(prefix) && !prefix.is_empty() {
			push_unique(&mut candidates, "host::");
		}
	}
	Some(Completion {
		start,
		end,
		candidates,
	})
}

/// A command is a `:` at the start of the input followed by letters; it is
/// completed only while the cursor is within it.
fn command(buffer: &str, cursor: usize, names: &Names) -> Option<Completion> {
	let start = buffer.len() - buffer.trim_start().len();
	if !buffer[start..].starts_with(':') || buffer[start..].starts_with("::") {
		return None;
	}
	let end = start
		+ 1 + buffer[start + 1..]
		.bytes()
		.take_while(|b| b.is_ascii_alphabetic())
		.count();
	if cursor < start || cursor > end {
		return None;
	}
	let prefix = &buffer[start..cursor];
	let candidates = names
		.commands
		.iter()
		.filter(|c| c.starts_with(prefix))
		.cloned()
		.collect();
	Some(Completion {
		start,
		end,
		candidates,
	})
}

/// Tokens of the buffer as Rune lexes it. `None` when the lexer stops at an
/// unterminated string, template, or block comment that contains the
/// cursor; tokens before an error elsewhere are kept.
fn tokens(buffer: &str, cursor: usize) -> Option<Vec<(Kind, std::ops::Range<usize>)>> {
	let mut parser = rune::parse::Parser::new(buffer, SourceId::empty(), false);
	let mut tokens = Vec::new();
	loop {
		match parser.parse::<ast::Token>() {
			Ok(token) if matches!(token.kind, Kind::Eof) => break,
			Ok(token) => tokens.push((token.kind, token.span.range())),
			Err(error) => {
				// End of input is a zero-width error, not a token. An unterminated
				// string, template, or block comment spans from its opening
				// delimiter to the end, so a non-empty span holding the cursor
				// means the cursor is inside one.
				let range = error.span().range();
				if !range.is_empty() && range.start <= cursor && cursor <= range.end {
					return None;
				}
				break;
			}
		}
	}
	Some(tokens)
}

/// The identifier-or-path group containing the cursor: adjacent `Ident` and
/// `::` tokens with no space between them. Returns its byte range, whether
/// it begins with `::`, and the kind of the token before it.
fn group_at(
	tokens: &[(Kind, std::ops::Range<usize>)],
	cursor: usize,
) -> Option<(usize, usize, bool, Option<Kind>)> {
	let is_path = |k: &Kind| matches!(k, Kind::Ident(_) | Kind::ColonColon);
	let mut i = 0;
	while i < tokens.len() {
		if !is_path(&tokens[i].0) {
			i += 1;
			continue;
		}
		let first = i;
		let mut last = i;
		while last + 1 < tokens.len()
			&& is_path(&tokens[last + 1].0)
			&& tokens[last + 1].1.start == tokens[last].1.end
		{
			last += 1;
		}
		let start = tokens[first].1.start;
		let end = tokens[last].1.end;
		if start <= cursor && cursor <= end {
			let leading = matches!(tokens[first].0, Kind::ColonColon);
			let previous = first.checked_sub(1).map(|p| tokens[p].0.clone());
			return Some((start, end, leading, previous));
		}
		i = last + 1;
	}
	None
}

/// Whether the cursor is inside a template string. The parser desugars a
/// template into synthetic tokens bracketed by an empty open delimiter at
/// the opening backtick and a matching empty close at the closing one; an
/// unfinished template has no close and runs to the end of the buffer.
/// Identifiers inside an interpolation are real tokens, which is why the
/// template range is tracked rather than inferred from token kinds.
fn inside_template(tokens: &[(Kind, std::ops::Range<usize>)], len: usize, cursor: usize) -> bool {
	let mut open: Vec<usize> = Vec::new();
	for (kind, range) in tokens {
		match kind {
			Kind::Open(ast::Delimiter::Empty) => open.push(range.start),
			Kind::Close(ast::Delimiter::Empty) => {
				if let Some(start) = open.pop() {
					if start < cursor && cursor < range.end {
						return true;
					}
				}
			}
			_ => {}
		}
	}
	open.into_iter()
		.any(|start| start < cursor && cursor <= len)
}

fn push_unique(candidates: &mut Vec<String>, name: &str) {
	if !candidates.iter().any(|c| c == name) {
		candidates.push(name.to_owned());
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn names() -> Names {
		Names {
			bindings: vec!["alpha".into(), "alphabet".into(), "same".into()],
			declarations: vec!["total".into(), "same".into(), "Point".into()],
			host: vec!["host::read".into(), "host::write_new".into()],
			commands: vec![
				":quit".into(),
				":reset".into(),
				":memory".into(),
				":debug".into(),
			],
		}
	}
	fn at(buffer: &str, cursor: usize) -> Option<Completion> {
		complete(buffer, cursor, &names())
	}
	fn candidates(buffer: &str, cursor: usize) -> Vec<String> {
		at(buffer, cursor).map(|c| c.candidates).unwrap_or_default()
	}

	#[test]
	fn the_whole_token_containing_the_cursor_is_the_range() {
		let c = at("alpha + beta", 3).unwrap();
		assert_eq!((c.start, c.end), (0, 5));
		assert_eq!(c.candidates, vec!["alpha", "alphabet"]);
		let c = at("x = alpx + beta", 7).unwrap();
		assert_eq!((c.start, c.end), (4, 8));
		assert_eq!(c.candidates, vec!["alpha", "alphabet"]);
		assert_eq!(candidates("alphab", 6), vec!["alphabet"]);
	}

	#[test]
	fn qualified_paths_complete_within_the_module() {
		assert_eq!(candidates("host::wr", 8), vec!["host::write_new"]);
		assert_eq!(
			candidates("let r = host::", 14),
			vec!["host::read", "host::write_new"]
		);
		assert_eq!(candidates("hos", 3), vec!["host::"]);
		assert_eq!(candidates("std::", 5), Vec::<String>::new());
		for cursor in 0..=5 {
			assert!(at("::alp", cursor).is_none(), "cursor {cursor}");
		}
		assert!(at("  ::me", 3).is_none());
	}

	#[test]
	fn nothing_is_offered_inside_strings_comments_or_after_a_dot() {
		assert!(at("\"alp", 4).is_none());
		assert!(at("`alp", 4).is_none());
		// Interpolations expose identifiers as tokens; the template context
		// still suppresses them, closed or unfinished, and text after a closed
		// template completes.
		assert!(at("`hello ${alp}` + x", 12).is_none());
		assert!(at("`hello ${alp", 12).is_none());
		assert!(at("`hello ${alp} ${b", 17).is_none());
		assert!(at("`hello ${alp}` + x", 14).is_none());
		assert_eq!(candidates("`a` + alp", 9), vec!["alpha", "alphabet"]);
		assert!(at("`a` + alp", 1).is_none());
		assert!(at("/* alp", 6).is_none());
		assert!(at("x // alp", 8).is_none());
		assert!(at("obj.alp", 7).is_none());
		assert!(at("let s = \"alp\"; x", 11).is_none());
		assert!(at("alp ", 4).is_none());
		assert!(at("", 0).is_none());
		// A string before the cursor is closed: completion proceeds after it.
		let buffer = "\"héllo\" + alp";
		assert_eq!(buffer.len(), 14);
		let c = at(buffer, 14).unwrap();
		assert_eq!((c.start, c.end), (11, 14));
		assert_eq!(c.candidates, vec!["alpha", "alphabet"]);
	}

	#[test]
	fn commands_complete_only_at_the_start_and_names_are_deduplicated() {
		assert_eq!(candidates(":me", 3), vec![":memory"]);
		assert_eq!(candidates(":", 1).len(), 4);
		assert_eq!(candidates("x :me", 5), Vec::<String>::new());
		assert_eq!(candidates("sa", 2), vec!["same"]);
		assert_eq!(candidates("to", 2), vec!["total"]);
	}

	#[test]
	fn a_later_line_of_a_multiline_input_is_found_by_byte_offset() {
		let buffer = "fn f() {\n  alp\n}";
		let c = at(buffer, 14).unwrap();
		assert_eq!(&buffer[c.start..c.end], "alp");
		assert_eq!(c.candidates, vec!["alpha", "alphabet"]);
	}
}

#[cfg(test)]
mod host_source_tests {
	/// Every registered host path is a candidate, and every candidate
	/// resolves in the context it was registered into: the same registration
	/// produces both, so there is no second list to drift.
	#[test]
	fn host_candidates_are_the_registered_functions() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let registered: Vec<String> = crate::host::install(&mut context)
			.unwrap()
			.into_iter()
			.map(|f| f.path)
			.collect();
		assert_eq!(registered.len(), 7);
		let names = super::Names {
			host: registered.clone(),
			..Default::default()
		};
		let listed = super::complete("host::", 6, &names).unwrap().candidates;
		assert_eq!(listed, registered);
		for path in &registered {
			let source = format!("pub fn main() {{ {path} }}");
			assert!(
				crate::compile(&context, &source).is_ok(),
				"{path} does not resolve"
			);
		}
	}
}
