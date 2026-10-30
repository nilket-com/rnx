//! Prove a missing method's name from structured Rune error identity.
//!
//! Fault spans identify the receiver, so a chain cannot be named by taking its
//! first method. Hash the source's method candidates against the receiver's
//! type hash; use the linked runtime's receiver display name only after proof.
use rune::parse::Parser;
use rune::runtime::{TypeHash, VmError};
use rune::{Hash, SourceId, ast};

/// Distinct method-position identifiers read with Rune's own lexer.
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

/// Name a root missing-instance-function error, or preserve Rune's diagnostic.
/// Panics quoting a diagnostic and absent source candidates cannot acquire a name.
pub fn named(error: &VmError, source: &str) -> Option<String> {
	let missing = error.missing_instance_function()?;
	named_identity(
		missing.instance,
		missing.hash,
		&missing.instance_name().to_string(),
		source,
	)
}

pub(crate) fn named_execution(
	error: &rune::experimental_core::execute::Failure,
	program: &rune::experimental_core::frontend::Program,
) -> Option<String> {
	let missing = error.missing_instance_function()?;
	// Source identity comes from the typed fault, not a candidate entry file.
	let location = error.error.location?;
	let source = program
		.sources()
		.iter()
		.find(|source| source.id == location.source)?;
	named_identity(
		missing.instance,
		missing.hash,
		&missing.instance_name(program).to_string(),
		&source.text,
	)
}

fn named_identity(instance_hash: Hash, hash: Hash, instance: &str, source: &str) -> Option<String> {
	let name = method_candidates(source)
		.into_iter()
		.find(|candidate| Hash::associated_function(instance_hash, candidate.as_str()) == hash)?;
	if instance_hash == <std::result::Result<rune::Value, rune::Value> as TypeHash>::HASH {
		return Some(format!(
			"no method `{name}` on `{instance}` (this value is a `Result`; did you mean to unwrap it with `?` first?)"
		));
	}
	Some(format!("no method `{name}` on `{instance}`"))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn failure(expression: &str) -> VmError {
		let context = rune::Context::with_default_modules().unwrap();
		let mut sources = rune::Sources::new();
		sources
			.insert(rune::Source::memory(format!("pub fn main() {{ {expression} }}")).unwrap())
			.unwrap();
		let unit = rune::prepare(&mut sources)
			.with_context(&context)
			.build()
			.unwrap();
		let mut vm = rune::Vm::new(
			rune::alloc::sync::Arc::try_new(context.runtime().unwrap()).unwrap(),
			rune::alloc::sync::Arc::try_new(unit).unwrap(),
		);
		vm.call(["main"], ()).unwrap_err()
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
	fn receiver_and_method_identity_come_from_the_linked_error() {
		let error = failure("[1].missing()");
		let missing = error.missing_instance_function().unwrap();
		assert_eq!(missing.instance, <rune::runtime::Vec as TypeHash>::HASH);
		assert_eq!(
			missing.hash,
			Hash::associated_function(missing.instance, "missing")
		);
		assert_eq!(
			named(&error, "parts.iter().missing()"),
			Some("no method `missing` on `std::vec::Vec`".into())
		);
		assert_eq!(named(&error, "parts.copy()"), None);
	}

	#[test]
	fn chains_name_the_missing_call_on_an_iterator_type() {
		let source = "#{a: 1}.values().sum()";
		let error = failure(source);
		assert_eq!(
			named(&error, source),
			Some("no method `sum` on `std::object::Values`".into())
		);
	}

	#[test]
	fn result_hint_uses_receiver_identity() {
		let source = "Ok(1).missing()";
		let error = failure(source);
		assert_eq!(named(&error, source), Some("no method `missing` on `std::result::Result` (this value is a `Result`; did you mean to unwrap it with `?` first?)".into()));
		assert_eq!(named(&error, "x.other()"), None);
	}

	#[test]
	fn quoted_and_unrelated_errors_are_never_rewritten() {
		let error =
			VmError::panic("Missing instance function `0xf77d93259f11131a` for `::std::vec::Vec`");
		assert!(error.missing_instance_function().is_none());
		assert_eq!(named(&error, "parts.join(())"), None);
		assert_eq!(named(&VmError::panic("unrelated"), "x.missing()"), None);
	}
}
