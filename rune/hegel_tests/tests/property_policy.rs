//! Fail closed when a property bypasses the shared deterministic runner.
use std::collections::BTreeMap;
use std::path::Path;

use syn::visit::{self, Visit};

const PROPERTIES: &[(&str, u64)] = &[
	("arithmetic_matches_checked_i64_reference", 100),
	("shift_ops_match_checked_reference", 100),
	("compile_never_panics_on_arbitrary_text", 100),
	("compile_never_panics_on_token_soup", 100),
	("nested_expressions_compile_without_crashing", 100),
	("const_eval_matches_runtime_eval", 100),
	("formatting_never_panics_on_arbitrary_text", 2000),
	("formatting_is_idempotent", 5000),
	("template_int_formatting_matches_rust", 100),
	("budget_halts_infinite_loops", 100),
	("integer_literal_roundtrip_all_bases", 100),
	("float_literal_roundtrip", 100),
	("string_literal_roundtrip", 100),
	("char_literal_roundtrip", 100),
	("programs_evaluate_as_the_reference_does", 1000),
	("value_roundtrip_preserves_primitives", 100),
	("value_roundtrip_preserves_compound_values", 100),
];

#[derive(Default)]
struct Policy {
	function: String,
	properties: Vec<(String, u64)>,
	error: Option<String>,
}

impl<'ast> Visit<'ast> for Policy {
	fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
		let old = std::mem::replace(&mut self.function, function.sig.ident.to_string());
		visit::visit_item_fn(self, function);
		self.function = old;
	}

	fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
		if attr.path().segments.iter().any(|s| s.ident == "hegel") {
			self.error = Some("attribute properties must use the shared runner".into());
		}
		visit::visit_attribute(self, attr);
	}

	fn visit_path(&mut self, path: &'ast syn::Path) {
		if path
			.segments
			.iter()
			.any(|s| s.ident == "Hegel" || s.ident == "Settings")
		{
			self.error =
				Some("raw property/settings construction bypasses the shared runner".into());
		}
		let names: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
		if names.first().is_some_and(|n| n == "hegel")
			&& names
				.get(1)
				.is_none_or(|n| !matches!(n.as_str(), "generators" | "one_of" | "tuples"))
		{
			self.error =
				Some("only Hegel's generator API is allowed outside the shared runner".into());
		}
		visit::visit_path(self, path);
	}

	fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
		// UseTree identifiers are not Paths: a renamed import otherwise
		// hides its constructor from visit_path.
		fn generator_import(tree: &syn::UseTree) -> bool {
			match tree {
				syn::UseTree::Path(p) => p.ident == "generators",
				syn::UseTree::Name(p) => p.ident == "generators",
				syn::UseTree::Rename(p) => p.ident == "generators",
				syn::UseTree::Group(g) => g.items.iter().all(generator_import),
				_ => false,
			}
		}
		let allowed = match &item.tree {
			syn::UseTree::Path(p) if p.ident == "hegel" => generator_import(&p.tree),
			syn::UseTree::Rename(p) if p.ident == "hegel" => false,
			_ => true,
		};
		if !allowed {
			self.error = Some("Hegel imports must name only the generator API".into());
		}
		visit::visit_item_use(self, item);
	}

	fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
		if item.ident == "hegel" {
			self.error = Some("extern-crate aliases cannot bypass the shared runner".into());
		}
		visit::visit_item_extern_crate(self, item);
	}

	fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
		if matches!(
			call.method.to_string().as_str(),
			"settings" | "seed" | "database"
		) {
			self.error = Some("property settings may only be set by the shared runner".into());
		}
		visit::visit_expr_method_call(self, call);
	}

	fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
		if let syn::Expr::Path(path) = &*call.func {
			let names: Vec<_> = path
				.path
				.segments
				.iter()
				.map(|s| s.ident.to_string())
				.collect();
			if names.last().is_some_and(|s| s == "property") {
				let values: Vec<_> = call.args.iter().collect();
				let parsed = match values.as_slice() {
					[syn::Expr::Lit(name), syn::Expr::Lit(count), syn::Expr::Closure(_)] => {
						match (&name.lit, &count.lit) {
							(syn::Lit::Str(name), syn::Lit::Int(count)) => {
								count.base10_parse().ok().map(|count| (name.value(), count))
							}
							_ => None,
						}
					}
					_ => None,
				};
				if names != ["rune_hegel_tests", "property"] {
					self.error = Some("use the qualified shared property runner".into());
				} else if let Some((name, count)) = parsed {
					if name != self.function {
						self.error = Some("property label must match its test function".into());
					}
					self.properties.push((name, count));
				} else {
					self.error = Some("property name/count/body must be explicit".into());
				}
			}
		}
		visit::visit_expr_call(self, call);
	}
}

fn inspect(source: &str) -> Result<Vec<(String, u64)>, String> {
	let file = syn::parse_file(source).map_err(|e| e.to_string())?;
	let mut policy = Policy::default();
	policy.visit_file(&file);
	if let Some(error) = policy.error {
		return Err(error);
	}
	for (name, count) in &policy.properties {
		let minimum = PROPERTIES
			.iter()
			.find(|(n, _)| n == name)
			.ok_or_else(|| format!("undeclared property: {name}"))?
			.1;
		if *count < minimum {
			return Err(format!("reduced case count: {name}"));
		}
	}
	Ok(policy.properties)
}

#[test]
fn every_property_uses_the_shared_configuration() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let mut actual = BTreeMap::new();
	let mut pending = vec![root.join("tests"), root.join("src")];
	while let Some(path) = pending.pop() {
		if path.is_dir() {
			pending.extend(std::fs::read_dir(path).unwrap().map(|e| e.unwrap().path()));
			continue;
		}
		if path == root.join("src/property.rs") {
			continue; // The one reviewed settings/runner implementation.
		}
		if path.extension().is_none_or(|e| e != "rs") {
			continue;
		}
		for (name, count) in inspect(&std::fs::read_to_string(&path).unwrap())
			.unwrap_or_else(|e| panic!("{}: {e}", path.display()))
		{
			assert!(
				actual.insert(name.clone(), count).is_none(),
				"duplicate property: {name}"
			);
		}
	}
	assert_eq!(actual.len(), PROPERTIES.len());
	for &(name, minimum) in PROPERTIES {
		assert!(
			actual.get(name).is_some_and(|n| *n >= minimum),
			"missing/reduced property: {name}"
		);
	}
}

#[test]
fn raw_settings_attributes_and_reduced_counts_are_rejected() {
	for source in [
		"#[hegel::test] fn surprise(tc: hegel::TestCase) {}",
		"fn surprise() { hegel::Hegel::new(|tc| {}).run(); }",
		"fn surprise() { let s = hegel::Settings::default(); }",
		"use hegel::Settings as S; fn surprise() { let s = S::new(); }",
		"use hegel::{Hegel as Driver}; fn surprise() { Driver::new(|tc| {}).run(); }",
		"use hegel::*; fn surprise() { Hegel::new(|tc| {}).run(); }",
		"use hegel as h; fn surprise() { h::hegel(|tc| {}); }",
		"extern crate hegel as h; fn surprise() { h::hegel(|tc| {}); }",
		"fn surprise() { hegel::hegel(|tc| {}); }",
		"fn arithmetic_matches_checked_i64_reference() { rune_hegel_tests::property(\"arithmetic_matches_checked_i64_reference\", 99, |tc| {}); }",
	] { assert!(inspect(source).is_err(), "bypass accepted: {source}"); }
	let good = "fn arithmetic_matches_checked_i64_reference() { rune_hegel_tests::property(\"arithmetic_matches_checked_i64_reference\", 100, |tc| {}); }";
	assert_eq!(inspect(good).unwrap().len(), 1);
}
