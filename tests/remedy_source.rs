//! Production recovery strings have a single owner; test goldens are excluded.
use std::{
	collections::BTreeSet,
	path::{Path, PathBuf},
};
use syn::{
	Attribute, Item, Lit, Meta,
	visit::{self, Visit},
};

fn test_only(attributes: &[Attribute]) -> bool {
	fn requires_test(meta: &Meta) -> bool {
		match meta {
			Meta::Path(path) => path.is_ident("test"),
			Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
				let parts = list
					.parse_args_with(
						syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
					)
					.unwrap();
				if list.path.is_ident("all") {
					parts.iter().any(requires_test)
				} else {
					!parts.is_empty() && parts.iter().all(requires_test)
				}
			}
			_ => false,
		}
	}
	attributes.iter().any(|attribute| {
		attribute.path().is_ident("test")
			|| attribute.path().is_ident("cfg")
				&& attribute
					.parse_args::<Meta>()
					.is_ok_and(|meta| requires_test(&meta))
	})
}
fn files(path: &Path, result: &mut Vec<PathBuf>) {
	for entry in std::fs::read_dir(path).unwrap() {
		let p = entry.unwrap().path();
		if p.is_dir() {
			files(&p, result);
		} else if p.extension().is_some_and(|x| x == "rs") {
			result.push(p);
		}
	}
}
#[derive(Default)]
struct Strings(Vec<String>);
impl<'ast> Visit<'ast> for Strings {
	fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
		if !test_only(&item.attrs) {
			visit::visit_item_mod(self, item);
		}
	}
	fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
		if !test_only(&item.attrs) {
			visit::visit_item_fn(self, item);
		}
	}
	fn visit_lit_str(&mut self, lit: &'ast syn::LitStr) {
		self.0.push(lit.value());
	}
	fn visit_macro(&mut self, mac: &'ast syn::Macro) {
		fn tokens(stream: proc_macro2::TokenStream, result: &mut Vec<String>) {
			for token in stream {
				match token {
					proc_macro2::TokenTree::Group(group) => tokens(group.stream(), result),
					proc_macro2::TokenTree::Literal(lit) => {
						if let Ok(s) = syn::parse_str::<syn::LitStr>(&lit.to_string()) {
							result.push(s.value());
						}
					}
					_ => {}
				}
			}
		}
		tokens(mac.tokens.clone(), &mut self.0);
	}
}
#[test]
fn recovery_strings_have_one_production_owner() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let mut paths = Vec::new();
	files(&root.join("src"), &mut paths);
	files(&root.join("jupyter/src"), &mut paths);
	let parsed: Vec<_> = paths
		.iter()
		.map(|p| {
			(
				p,
				syn::parse_file(&std::fs::read_to_string(p).unwrap()).unwrap(),
			)
		})
		.collect();
	let mut excluded = BTreeSet::new();
	for (path, file) in &parsed {
		let parent = path.parent().unwrap();
		let stem = path.file_stem().unwrap();
		let lookup = if ["lib", "main", "mod"].iter().any(|s| stem == *s) {
			parent.to_owned()
		} else {
			parent.join(stem)
		};
		for item in &file.items {
			if let Item::Mod(module) = item
				&& module.content.is_none()
				&& test_only(&module.attrs)
			{
				let named = module
					.attrs
					.iter()
					.find_map(|a| {
						if a.path().is_ident("path")
							&& let Meta::NameValue(value) = &a.meta
							&& let syn::Expr::Lit(lit) = &value.value
							&& let Lit::Str(text) = &lit.lit
						{
							Some(parent.join(text.value()))
						} else {
							None
						}
					})
					.unwrap_or_else(|| lookup.join(module.ident.to_string()).with_extension("rs"));
				excluded.insert(named.with_extension(""));
				excluded.insert(named);
			}
		}
	}
	let mut observed = Vec::new();
	for (path, file) in parsed {
		if excluded.iter().any(|prefix| path.starts_with(prefix)) {
			continue;
		}
		let mut strings = Strings::default();
		strings.visit_file(&file);
		observed.extend(strings.0.into_iter().map(|text| (path, text)));
	}
	for (needle, owner) in [
		(
			"start `rnx repl` without --engine=execution",
			"src/session/remedy.rs",
		),
		("select the `{}` kernel", "src/session/remedy.rs"),
		("use the legacy session engine", "src/session/remedy.rs"),
		("Rune (rnx)", "src/kernel_name.rs"),
	] {
		let found: Vec<_> = observed
			.iter()
			.filter(|(_, text)| text.contains(needle))
			.map(|(p, _)| p.strip_prefix(root).unwrap())
			.collect();
		assert_eq!(
			found,
			vec![Path::new(owner)],
			"{needle} duplicated or moved"
		);
	}
}
