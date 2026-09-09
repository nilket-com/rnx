//! Naming the method in a missing-instance-function diagnostic.
//!
//! Rune's error carries a hash and the instance type and no name, and
//! `VmErrorKind` is crate-private in 0.14.1, so the two are read out of the
//! rendered message. The name is then recovered by proof rather than by
//! reading: every identifier the script uses in method position is hashed
//! against the instance type, and one is printed only when its hash is the
//! one the error carries.
//!
//! Reading is not enough because the fault position points at the receiver.
//! For `counts.values().sum()` the span starts at `counts`, so taking the
//! first method name in the line would confidently name `values` when the
//! missing one is `sum`.
use rune::Hash;
use rune::SourceId;
use rune::ast;
use rune::parse::Parser;
use rune::runtime::{TypeHash, TypeOf};

const OPENING: &str = "Missing instance function `";
const BETWEEN: &str = "` for `";

/// The displayed name and type hash of each type a script commonly calls a
/// method on, taken from the linked Rune rather than written down, so the two
/// cannot disagree with the version in use.
pub fn known_types() -> Vec<(String, Hash)> {
	macro_rules! entry {
		($t:ty) => {
			(
				format!("{}", <$t as TypeOf>::type_info()),
				<$t as TypeHash>::HASH,
			)
		};
	}
	std::vec![
		entry!(rune::runtime::Vec),
		entry!(rune::runtime::Object),
		entry!(rune::alloc::String),
		entry!(rune::runtime::Tuple),
		entry!(rune::runtime::Bytes),
		entry!(rune::runtime::Range),
		entry!(i64),
		entry!(f64),
		entry!(bool),
		entry!(char),
	]
}

/// The identifiers the source uses in method position, in the order they
/// appear and without repeats.
///
/// This reads Rune's own tokens rather than the text, so whitespace and a
/// comment between the dot and the name are handled the way the compiler
/// handles them, an identifier inside a string literal is not a candidate,
/// and a tuple index or a range is not one either.
pub fn method_candidates(source: &str) -> Vec<String> {
	let mut parser = Parser::new(source, SourceId::EMPTY, false);
	let mut found: Vec<String> = Vec::new();
	let mut after_dot = false;
	// The token stream ends by returning an error, and a source that fails to
	// lex simply yields what was read before it. The bound is insurance
	// against a token that never advances.
	for _ in 0..=source.len() {
		let Ok(token) = parser.parse::<ast::Token>() else {
			break;
		};
		let is_ident = matches!(token.kind, ast::Kind::Ident(..));
		if after_dot && is_ident {
			let range = token.span.range();
			if let Some(text) = source.get(range.start..range.end)
				&& !found.iter().any(|seen| seen == text)
			{
				found.push(text.to_owned());
			}
		}
		after_dot = matches!(token.kind, ast::Kind::Dot);
	}
	found
}

/// A sentence naming the method, or `None` to leave the message alone.
///
/// Every step can fail: a message that no longer parses, a type rnx has no
/// entry for, a name the source never spells. Each falls back to the message
/// Rune produced, which is a poor answer and still a true one.
pub fn named(message: &str, source: &str) -> Option<String> {
	// The whole message must be this diagnostic, beginning and end. Matching
	// the pattern anywhere would rewrite any error that quotes it, and a
	// panic carrying the text of one is still a panic: the hash would match a
	// candidate and name a method that was never called.
	let rest = message.strip_prefix(OPENING)?;
	let end = rest.find(BETWEEN)?;
	let hash = &rest[..end];
	let instance = rest[end + BETWEEN.len()..].strip_suffix('`')?;
	let (_, type_hash) = known_types()
		.into_iter()
		.find(|(name, _)| name == instance)?;
	let name = method_candidates(source).into_iter().find(|candidate| {
		Hash::associated_function(type_hash, candidate.as_str()).to_string() == hash
	})?;
	Some(format!("no method `{name}` on `{instance}`"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_map_comes_from_the_linked_rune_and_reproduces_a_reported_hash() {
		let types = known_types();
		let (_, sequence) = types
			.iter()
			.find(|(name, _)| name == "::std::vec::Vec")
			.expect("the sequence type is in the map");
		// The hash the second port's diagnostic actually reported, and the
		// pair it stands for. A Rune that hashes differently fails here rather
		// than silently naming nothing.
		assert_eq!(
			Hash::associated_function(*sequence, "join").to_string(),
			"0xf77d93259f11131a"
		);
		assert_eq!(
			Hash::associated_function(*sequence, "contains").to_string(),
			"0x7514a9c8fb463f51"
		);
		// The displayed names come from the same types, not from this file.
		for expected in ["::std::object::Object", "::std::string::String"] {
			assert!(
				types.iter().any(|(name, _)| name == expected),
				"{expected} is missing from {types:?}"
			);
		}
	}

	#[test]
	fn candidates_are_the_names_used_in_method_position() {
		assert_eq!(method_candidates("parts.join(UNIT)"), ["join"]);
		assert_eq!(
			method_candidates("counts.values().sum() + 1"),
			["values", "sum"]
		);
		// Whitespace and comments between the dot and the name are what the
		// compiler ignores, so the scan ignores them too.
		assert_eq!(
			method_candidates("parts . /* ordinary comment */ join(\"-\")"),
			["join"]
		);
		assert_eq!(
			method_candidates("parts.\n\t// a line comment\n\tjoin(\"-\")"),
			["join"]
		);
		assert_eq!(method_candidates("parts\n\t.join(\"-\")"), ["join"]);
		// An identifier inside a string literal is not a method call, and
		// reading tokens rather than text is what makes that true.
		assert_eq!(
			method_candidates("let message = \"call a.join(b) somewhere\";"),
			[] as [String; 0]
		);
		// A range, a float, and a tuple index are not method calls.
		assert_eq!(
			method_candidates("for i in 0..10 { let x = 1.5; let y = pair.0; }"),
			[] as [String; 0]
		);
		// A repeat is listed once, in the order it first appears.
		assert_eq!(
			method_candidates("a.one(); b.two(); c.one()"),
			["one", "two"]
		);
		// Identifiers beyond ASCII are identifiers.
		assert_eq!(method_candidates("café.método(\"x\")"), ["método"]);
		assert_eq!(method_candidates("x._private()"), ["_private"]);
	}

	#[test]
	fn a_message_that_does_not_parse_is_left_alone() {
		assert_eq!(named("something else entirely", "a.join(b)"), None);
		// A panic that quotes the diagnostic is still a panic. The candidate's
		// hash would match; the fault is not a missing method, so the whole
		// message has to be this diagnostic and not merely contain it.
		assert_eq!(
			named(
				"Panicked: Missing instance function `0xf77d93259f11131a` for `::std::vec::Vec`",
				"a.join(b)"
			),
			None
		);
		// Trailing text means the message is something else too.
		assert_eq!(
			named(
				"Missing instance function `0xf77d93259f11131a` for `::std::vec::Vec` while resolving",
				"a.join(b)"
			),
			None
		);
		// A type with no entry falls back rather than guessing.
		assert_eq!(
			named(
				"Missing instance function `0x1` for `::std::object::Values`",
				"a.sum()"
			),
			None
		);
		// A name the source never spells falls back too.
		assert_eq!(
			named(
				"Missing instance function `0xf77d93259f11131a` for `::std::vec::Vec`",
				"a.push(1)"
			),
			None
		);
	}

	#[test]
	fn a_name_is_proved_before_it_is_printed() {
		let message = "Missing instance function `0xf77d93259f11131a` for `::std::vec::Vec`";
		assert_eq!(
			named(message, "let joined = parts.join(UNIT);").unwrap(),
			"no method `join` on `::std::vec::Vec`"
		);
		// The proof is what makes a chain right: `join` is not the first name
		// in this source, and it is still the one named.
		assert_eq!(
			named(message, "parts.iter().map(f).join(UNIT)").unwrap(),
			"no method `join` on `::std::vec::Vec`"
		);
	}
}
