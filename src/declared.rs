//! Field-name candidates, read from declarations rnx itself compiled.
//!
//! Rune 0.14.1 will not tell a value its own field names — `Rtti` carries the
//! map, but it is `pub(crate)` with no accessor — and it offers no identity
//! that separates two compiled units, because a type's hash is the hash of
//! its item path. So a name is never assumed from a lookup: candidates are
//! collected here and each is **verified against the value** before it is
//! used, which is record 0014's rule that a match proves a name and never a
//! fault.
//!
//! Candidates are keyed by the short type name, which is only a key. The full
//! item path would be no better: the same path can come from two units, and a
//! struct declared inside a function body is `main::$0::P` to Rune, which no
//! reading of the source can predict. Verification is what decides.
use rune::SourceId;
use rune::ast;
use rune::ast::Spanned;
use rune::parse::parse_all;
use std::collections::HashMap;

/// Field lists rnx knows about, by short type name. More than one list may
/// share a name: two scopes of one file can each declare `P`.
#[derive(Debug, Default)]
pub struct Fields {
	by_name: HashMap<String, Vec<Vec<String>>>,
}

impl Fields {
	pub fn add(&mut self, name: &str, fields: Vec<String>) {
		let candidates = self.by_name.entry(name.to_owned()).or_default();
		if !candidates.contains(&fields) {
			candidates.push(fields);
		}
	}

	/// The candidate that fits this value, or none. A candidate fits when the
	/// value holds exactly as many fields and answers to every name in it, so
	/// the names describe this shape whichever unit built it.
	pub fn fitting<'a>(
		&'a self,
		name: &str,
		arity: usize,
		holds: impl Fn(&str) -> bool,
	) -> Option<&'a [String]> {
		self.by_name
			.get(name)?
			.iter()
			.find(|candidate| candidate.len() == arity && candidate.iter().all(|f| holds(f)))
			.map(|candidate| candidate.as_slice())
	}
}

/// Every struct and every named enum variant declared anywhere in a file: at
/// the top level, inside a module, or inside a function's body. A file that
/// does not parse yields nothing rather than failing; the compiler is what
/// reports that, and it reports it better.
pub fn in_file(text: &str) -> Fields {
	let mut fields = Fields::default();
	into_fields(text, &mut fields);
	fields
}

/// The same, collecting into a table that already holds candidates. This is
/// the one place a declaration becomes a candidate, so a session and a file
/// cannot come to different conclusions about the same text.
pub fn into_fields(text: &str, fields: &mut Fields) {
	let Ok(file) = parse_all::<ast::File>(text, SourceId::empty(), false) else {
		return;
	};
	for (item, _) in &file.items {
		item_into(item, text, fields);
	}
}

fn item_into(item: &ast::Item, text: &str, fields: &mut Fields) {
	match item {
		ast::Item::Struct(declaration) => {
			let name = &text[declaration.ident.span().range()];
			fields.add(name, named_fields(declaration, text));
		}
		ast::Item::Mod(module) => {
			if let ast::ItemModBody::InlineBody(body) = &module.body {
				for (item, _) in &body.file.items {
					item_into(item, text, fields);
				}
			}
		}
		// A struct variant carries field names exactly as a struct does, and
		// a value of one is a `Struct` to the renderer. Rune's runtime type
		// information names it `E::C`, whose short name is `C`.
		ast::Item::Enum(declaration) => {
			for (variant, _) in declaration.variants.iter() {
				let name = &text[variant.name.span().range()];
				if let ast::Fields::Named(braced) = &variant.body {
					let named = braced
						.iter()
						.map(|(field, _)| text[field.name.span().range()].to_owned())
						.collect();
					fields.add(name, named);
				}
			}
		}
		ast::Item::Fn(function) => block_into(&function.body, text, fields),
		_ => {}
	}
}

fn block_into(block: &ast::Block, text: &str, fields: &mut Fields) {
	for statement in &block.statements {
		if let ast::Stmt::Item(item, _) = statement {
			item_into(item, text, fields);
		}
	}
}

/// The names of a struct's fields, in declaration order. A tuple struct and a
/// unit struct have none to give.
fn named_fields(declaration: &ast::ItemStruct, text: &str) -> Vec<String> {
	match &declaration.body {
		ast::Fields::Named(braced) => braced
			.iter()
			.map(|(field, _)| text[field.name.span().range()].to_owned())
			.collect(),
		_ => Vec::new(),
	}
}
