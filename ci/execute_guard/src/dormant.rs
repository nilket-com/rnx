//! Structural AST checker for representation-only dormant feature diffs (ADR 0355).
//!
//! Proves that a production Rust delta between a measured baseline and candidate:
//! - modifies only `.rs` production files (no Cargo.toml, build scripts, or non-`.rs` files),
//! - only skips files outside `src/` (integration tests/benches) or files under `src/`
//!   whose exact parent-module-to-file edge is AST-proven to carry `#[cfg(test)]` (no
//!   nested inline module confusion, no `#[path]` attributes),
//! - adds only new enum variants on enums whose attributes are strictly inert/safe
//!   (no constructing derives, no expanding `cfg_attr` other than inert `allow(...)`),
//! - binds every enum by its canonical `(crate_root, module_path, enum_name)` and resolves
//!   pattern qualifiers and `impl` targets through exact `use` / re-export scope (failing
//!   closed on aliases, unknown globs, or ambiguity),
//! - admits only new-variant-only match arms or non-binding `|` alternatives on existing
//!   arms, leaving all other tokens identical,
//! - audits added arms against `unsafe`, `transmute`, and macro variant expansion.
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syn::visit::{self, Visit};
use syn::visit_mut::{self, VisitMut};

/// Safe enum derives that never construct new enum values.
const SAFE_DERIVES: &[&str] = &[
	"Clone",
	"Copy",
	"Debug",
	"PartialEq",
	"Eq",
	"PartialOrd",
	"Ord",
	"Hash",
];

#[derive(Debug, Clone)]
pub struct FilePair {
	pub rel_path: String,
	pub base_src: String,
	pub cand_src: String,
}

#[derive(Debug, Clone)]
pub struct StructuralProof {
	pub added_variants: Vec<String>,
	pub added_arms: Vec<(String, String)>,
}

/// Exact canonical identity of an enum declaration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalType {
	crate_root: String,
	module: Vec<String>,
	name: String,
}

impl CanonicalType {
	fn display(&self) -> String {
		if self.module.is_empty() {
			self.name.clone()
		} else {
			format!("{}::{}", self.module.join("::"), self.name)
		}
	}
}

#[derive(Debug, Clone)]
enum Binding {
	Exact(CanonicalType),
	Ambiguous,
}

fn canonical(stream: proc_macro2::TokenStream) -> String {
	stream
		.into_iter()
		.map(|t| match t {
			proc_macro2::TokenTree::Group(g) => {
				let (a, b) = match g.delimiter() {
					proc_macro2::Delimiter::Brace => ("{", "}"),
					proc_macro2::Delimiter::Bracket => ("[", "]"),
					proc_macro2::Delimiter::Parenthesis => ("(", ")"),
					proc_macro2::Delimiter::None => ("<none>", "</none>"),
				};
				format!("{a} {} {b}", canonical(g.stream()))
			}
			t => t.to_string(),
		})
		.collect::<Vec<_>>()
		.join(" ")
}

fn tokens(t: &impl ToTokens) -> String {
	canonical(t.to_token_stream())
}

fn is_cfg_test_attr(attr: &syn::Attribute) -> bool {
	attr.path().is_ident("cfg") && tokens(&attr.meta) == "cfg ( test )"
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
	attrs.iter().any(is_cfg_test_attr)
}

fn has_path_attr(attrs: &[syn::Attribute]) -> bool {
	attrs.iter().any(|a| a.path().is_ident("path"))
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
	attrs.iter().any(|a| {
		let p = tokens(a.path());
		p == "test" || p.ends_with(":: test")
	})
}

fn strip_cfg_test_items(items: &mut Vec<syn::Item>) {
	items.retain(|item| !item_is_test_only(item));
	for item in items.iter_mut() {
		if let syn::Item::Mod(m) = item {
			if let Some((_, inner)) = &mut m.content {
				strip_cfg_test_items(inner);
			}
		}
	}
}

fn item_is_test_only(item: &syn::Item) -> bool {
	let attrs = match item {
		syn::Item::Const(x) => &x.attrs,
		syn::Item::Enum(x) => &x.attrs,
		syn::Item::ExternCrate(x) => &x.attrs,
		syn::Item::Fn(x) => {
			return has_cfg_test(&x.attrs) || has_test_attr(&x.attrs);
		}
		syn::Item::ForeignMod(x) => &x.attrs,
		syn::Item::Impl(x) => &x.attrs,
		syn::Item::Macro(x) => &x.attrs,
		syn::Item::Mod(x) => &x.attrs,
		syn::Item::Static(x) => &x.attrs,
		syn::Item::Struct(x) => &x.attrs,
		syn::Item::Trait(x) => &x.attrs,
		syn::Item::TraitAlias(x) => &x.attrs,
		syn::Item::Type(x) => &x.attrs,
		syn::Item::Union(x) => &x.attrs,
		syn::Item::Use(x) => &x.attrs,
		_ => return false,
	};
	has_cfg_test(attrs)
}

/// Split a `.rs` path into `(crate_root, module_segments)`.
fn file_module_path(rel_path: &str) -> Option<(String, Vec<String>)> {
	let src_idx = if rel_path.starts_with("src/") {
		Some(0)
	} else {
		rel_path.find("/src/").map(|i| i + 1)
	}?;
	let crate_root = rel_path[..src_idx + 4].to_owned();
	let under_src = &rel_path[src_idx + 4..];
	let no_ext = under_src.strip_suffix(".rs")?;
	let mut segs: Vec<String> = no_ext
		.split('/')
		.filter(|s| !s.is_empty())
		.map(String::from)
		.collect();
	if matches!(
		segs.last().map(String::as_str),
		Some("mod" | "lib" | "main")
	) {
		segs.pop();
	}
	Some((crate_root, segs))
}

/// True iff `rel_path` is outside any `src/` tree and lies in an integration
/// `tests/` or `benches/` directory.
fn is_outside_src_test_or_bench(rel_path: &str) -> bool {
	if rel_path.starts_with("src/") || rel_path.contains("/src/") {
		return false;
	}
	rel_path.starts_with("tests/")
		|| rel_path.starts_with("benches/")
		|| rel_path.contains("/tests/")
		|| rel_path.contains("/benches/")
}

/// Check ONLY the exact parent-module-to-file edge:
/// - Returns `Some(true)` if `file_src` has a top-level `#[cfg(test)] mod <mod_name>;`
///   without `#[path]`.
/// - Returns `Some(false)` if `file_src` has a top-level non-`#[cfg(test)]` `mod <mod_name>;`
///   (or a `#[path]` `mod <mod_name>;`).
/// - Returns `None` if `file_src` does not declare `mod <mod_name>;` at top level.
fn top_level_mod_decl_is_cfg_test(file_src: &str, mod_name: &str) -> Option<bool> {
	let parsed = syn::parse_file(file_src).ok()?;
	let mut saw_cfg_test = false;
	for item in &parsed.items {
		if let syn::Item::Mod(m) = item {
			if m.ident == mod_name && m.content.is_none() {
				if has_path_attr(&m.attrs) || !has_cfg_test(&m.attrs) {
					return Some(false);
				}
				saw_cfg_test = true;
			}
		}
	}
	saw_cfg_test.then_some(true)
}

/// Prove whether `rel_path` under `src/` is gated by a `#[cfg(test)]` ancestor
/// `mod <name>;` declaration along EVERY crate root / parent module that reaches it
/// (Fix 9: if `lib.rs` has `#[cfg(test)] mod worker;` while `main.rs` or any `src/bin/*`
/// has production `mod worker;`, returns `false`).
fn is_proven_cfg_test_under_src(rel_path: &str, parent_sources: &BTreeMap<String, String>) -> bool {
	let src_idx = if rel_path.starts_with("src/") {
		Some(0)
	} else {
		rel_path.find("/src/").map(|i| i + 1)
	};
	let Some(src_start) = src_idx else {
		return false;
	};
	let src_prefix = &rel_path[..src_start + 4];
	let under_src = &rel_path[src_start + 4..];
	let path = Path::new(under_src);
	let stem = match path.file_stem().and_then(|s| s.to_str()) {
		Some(s) => s,
		None => return false,
	};
	let mut cur_dir = path.parent().unwrap_or(Path::new(""));
	let mut cur_mod = if stem == "mod" {
		match cur_dir.file_name().and_then(|s| s.to_str()) {
			Some(name) => {
				cur_dir = cur_dir.parent().unwrap_or(Path::new(""));
				name.to_owned()
			}
			None => return false,
		}
	} else {
		stem.to_owned()
	};

	loop {
		let mut candidate_parents = Vec::new();
		if cur_dir.as_os_str().is_empty() {
			// Check ALL crate roots under `src_prefix`: lib.rs, main.rs, src/bin/*.rs, src/bin/*/main.rs.
			candidate_parents.push(format!("{src_prefix}lib.rs"));
			candidate_parents.push(format!("{src_prefix}main.rs"));
			let bin_prefix = format!("{src_prefix}bin/");
			for key in parent_sources.keys() {
				if key.starts_with(&bin_prefix) && key.ends_with(".rs") {
					candidate_parents.push(key.clone());
				}
			}
		} else {
			let dir_str = cur_dir.to_string_lossy();
			candidate_parents.push(format!("{src_prefix}{dir_str}.rs"));
			candidate_parents.push(format!("{src_prefix}{dir_str}/mod.rs"));
		}
		let mut any_cfg_test = false;
		let mut any_prod = false;
		for parent_path in &candidate_parents {
			if let Some(src) = parent_sources.get(parent_path) {
				match top_level_mod_decl_is_cfg_test(src, &cur_mod) {
					Some(true) => any_cfg_test = true,
					Some(false) => any_prod = true,
					None => {}
				}
			}
		}
		if any_cfg_test && !any_prod {
			return true;
		}
		if cur_dir.as_os_str().is_empty() {
			break;
		}
		match cur_dir.file_name().and_then(|s| s.to_str()) {
			Some(name) => {
				cur_mod = name.to_owned();
				cur_dir = cur_dir.parent().unwrap_or(Path::new(""));
			}
			None => break,
		}
	}
	false
}

/// Audit ALL attributes on a changed enum and its variants (Fix 7):
/// allow only inert `doc`, `allow`, `repr`, allowlisted `derive(...)`, and
/// inert `cfg_attr(..., allow(...))`. Refuse any other `cfg_attr` or expanding
/// attribute fail-closed.
fn check_enum_attrs(item: &syn::ItemEnum) -> Result<(), String> {
	let check_one = |attr: &syn::Attribute, where_str: &str| -> Result<(), String> {
		let path = attr.path();
		if path.is_ident("doc") || path.is_ident("allow") || path.is_ident("repr") {
			return Ok(());
		}
		if path.is_ident("derive") {
			let list = attr
				.parse_args_with(
					syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
				)
				.map_err(|e| format!("cannot parse derive on {where_str}: {e}"))?;
			for p in list {
				let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
				let allowed = match segs.as_slice() {
					[name] => SAFE_DERIVES.contains(&name.as_str()),
					[root, .., name] if root == "std" || root == "core" => {
						p.leading_colon.is_none() && SAFE_DERIVES.contains(&name.as_str())
					}
					_ => false,
				};
				if !allowed {
					return Err(format!(
						"{where_str} has non-allowlisted derive `{}` (possible value ingress)",
						tokens(&p)
					));
				}
			}
			return Ok(());
		}
		if path.is_ident("cfg_attr") {
			let args = attr
				.parse_args_with(
					syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
				)
				.map_err(|e| format!("cannot parse cfg_attr on {where_str}: {e}"))?;
			if args.len() >= 2 && args.iter().skip(1).all(|m| m.path().is_ident("allow")) {
				return Ok(());
			}
			return Err(format!(
				"{where_str} has non-inert cfg_attr `{}` (possible expanding derive/attribute)",
				tokens(&attr.meta)
			));
		}
		Err(format!(
			"{where_str} has unreviewed/expanding attribute `{}`",
			tokens(&attr.meta)
		))
	};

	let label = format!("enum {}", item.ident);
	for attr in &item.attrs {
		check_one(attr, &label)?;
	}
	for v in &item.variants {
		let vlabel = format!("enum {}::{}", item.ident, v.ident);
		for attr in &v.attrs {
			check_one(attr, &vlabel)?;
		}
	}
	Ok(())
}

fn collect_enums(
	crate_root: &str,
	module: &[String],
	items: &[syn::Item],
	out: &mut BTreeMap<CanonicalType, syn::ItemEnum>,
) {
	for item in items {
		match item {
			syn::Item::Enum(e) => {
				out.insert(
					CanonicalType {
						crate_root: crate_root.to_owned(),
						module: module.to_vec(),
						name: e.ident.to_string(),
					},
					e.clone(),
				);
			}
			syn::Item::Mod(m) => {
				if let Some((_, inner)) = &m.content {
					let mut sub = module.to_vec();
					sub.push(m.ident.to_string());
					collect_enums(crate_root, &sub, inner, out);
				}
			}
			_ => {}
		}
	}
}

fn normalize_enum_trailing_commas(items: &mut [syn::Item]) {
	for item in items.iter_mut() {
		match item {
			syn::Item::Enum(e) => {
				if e.variants.trailing_punct() {
					let _ = e.variants.pop_punct();
				}
			}
			syn::Item::Mod(m) => {
				if let Some((_, inner)) = &mut m.content {
					normalize_enum_trailing_commas(inner);
				}
			}
			_ => {}
		}
	}
}

fn strip_added_enum_variants(
	crate_root: &str,
	module: &[String],
	items: &mut [syn::Item],
	added_by_enum: &BTreeMap<CanonicalType, BTreeSet<String>>,
) {
	for item in items.iter_mut() {
		match item {
			syn::Item::Enum(e) => {
				let key = CanonicalType {
					crate_root: crate_root.to_owned(),
					module: module.to_vec(),
					name: e.ident.to_string(),
				};
				if let Some(added) = added_by_enum.get(&key) {
					let kept: syn::punctuated::Punctuated<syn::Variant, syn::Token![,]> = e
						.variants
						.iter()
						.filter(|v| !added.contains(&v.ident.to_string()))
						.cloned()
						.collect();
					e.variants = kept;
				}
			}
			syn::Item::Mod(m) => {
				if let Some((_, inner)) = &mut m.content {
					let mut sub = module.to_vec();
					sub.push(m.ident.to_string());
					strip_added_enum_variants(crate_root, &sub, inner, added_by_enum);
				}
			}
			_ => {}
		}
	}
}

/// Resolve a relative module path (`crate::...`, `super::...`, `self::...`)
/// into canonical module segments from `crate_root`.
fn resolve_mod_segments(cur_mod: &[String], segs: &[String]) -> Option<Vec<String>> {
	if segs.is_empty() {
		return None;
	}
	match segs[0].as_str() {
		"crate" => Some(segs[1..].to_vec()),
		"self" => {
			let mut out = cur_mod.to_vec();
			out.extend(segs[1..].iter().cloned());
			Some(out)
		}
		"super" => {
			let mut out = cur_mod.to_vec();
			let mut i = 0;
			while i < segs.len() && segs[i] == "super" {
				out.pop()?;
				i += 1;
			}
			out.extend(segs[i..].iter().cloned());
			Some(out)
		}
		_ => None,
	}
}

fn insert_binding(scope: &mut BTreeMap<String, Binding>, name: String, ty: CanonicalType) {
	match scope.get(&name) {
		Some(Binding::Exact(existing)) if existing == &ty => {}
		Some(_) => {
			scope.insert(name, Binding::Ambiguous);
		}
		None => {
			scope.insert(name, Binding::Exact(ty));
		}
	}
}

/// Collect `pub use` re-exports of types from known module sources (e.g.
/// `experimental_core/error.rs` re-exporting `super::detail::TypeName`).
fn collect_reexports(
	crate_root: &str,
	module: &[String],
	items: &[syn::Item],
	reexports: &mut BTreeMap<CanonicalType, CanonicalType>,
) {
	fn walk_tree(
		crate_root: &str,
		cur_mod: &[String],
		prefix: &mut Vec<String>,
		tree: &syn::UseTree,
		reexports: &mut BTreeMap<CanonicalType, CanonicalType>,
	) {
		match tree {
			syn::UseTree::Path(p) => {
				prefix.push(p.ident.to_string());
				walk_tree(crate_root, cur_mod, prefix, &p.tree, reexports);
				prefix.pop();
			}
			syn::UseTree::Group(g) => {
				for t in &g.items {
					walk_tree(crate_root, cur_mod, prefix, t, reexports);
				}
			}
			syn::UseTree::Name(n) => {
				let name = n.ident.to_string();
				let mut full = prefix.clone();
				full.push(name.clone());
				if let Some(mut resolved) = resolve_mod_segments(cur_mod, &full) {
					if let Some(target_name) = resolved.pop() {
						let alias = CanonicalType {
							crate_root: crate_root.to_owned(),
							module: cur_mod.to_vec(),
							name,
						};
						let target = CanonicalType {
							crate_root: crate_root.to_owned(),
							module: resolved,
							name: target_name,
						};
						reexports.insert(alias, target);
					}
				}
			}
			syn::UseTree::Rename(_) | syn::UseTree::Glob(_) => {}
		}
	}

	for item in items {
		if let syn::Item::Use(u) = item {
			if !matches!(u.vis, syn::Visibility::Inherited) {
				let mut prefix = Vec::new();
				walk_tree(crate_root, module, &mut prefix, &u.tree, reexports);
			}
		}
	}
}

fn canonicalize_reexport(
	mut ty: CanonicalType,
	reexports: &BTreeMap<CanonicalType, CanonicalType>,
) -> CanonicalType {
	for _ in 0..8 {
		if let Some(next) = reexports.get(&ty) {
			ty = next.clone();
		} else {
			break;
		}
	}
	ty
}

/// Populate local type bindings from `items` (`enum` declarations and `use` items).
/// Refuses `use ... as Alias` (marks alias ambiguous) and unknown globs.
fn populate_scope_from_items(
	crate_root: &str,
	cur_mod: &[String],
	items: &[syn::Item],
	known_module_enums: &BTreeMap<(String, Vec<String>), Vec<String>>,
	reexports: &BTreeMap<CanonicalType, CanonicalType>,
	scope: &mut BTreeMap<String, Binding>,
) {
	for item in items {
		if let syn::Item::Enum(e) = item {
			let name = e.ident.to_string();
			insert_binding(
				scope,
				name.clone(),
				CanonicalType {
					crate_root: crate_root.to_owned(),
					module: cur_mod.to_vec(),
					name,
				},
			);
		}
	}
	for item in items {
		if let syn::Item::Use(u) = item {
			let mut prefix = Vec::new();
			collect_use_bindings(
				crate_root,
				cur_mod,
				&mut prefix,
				&u.tree,
				known_module_enums,
				reexports,
				scope,
			);
		}
	}
}

fn collect_use_bindings(
	crate_root: &str,
	cur_mod: &[String],
	prefix: &mut Vec<String>,
	tree: &syn::UseTree,
	known_module_enums: &BTreeMap<(String, Vec<String>), Vec<String>>,
	reexports: &BTreeMap<CanonicalType, CanonicalType>,
	scope: &mut BTreeMap<String, Binding>,
) {
	match tree {
		syn::UseTree::Path(p) => {
			prefix.push(p.ident.to_string());
			collect_use_bindings(
				crate_root,
				cur_mod,
				prefix,
				&p.tree,
				known_module_enums,
				reexports,
				scope,
			);
			prefix.pop();
		}
		syn::UseTree::Group(g) => {
			for t in &g.items {
				collect_use_bindings(
					crate_root,
					cur_mod,
					prefix,
					t,
					known_module_enums,
					reexports,
					scope,
				);
			}
		}
		syn::UseTree::Name(n) => {
			let name = n.ident.to_string();
			let mut full = prefix.clone();
			full.push(name.clone());
			if let Some(mut resolved) = resolve_mod_segments(cur_mod, &full) {
				if let Some(ty_name) = resolved.pop() {
					let ty = canonicalize_reexport(
						CanonicalType {
							crate_root: crate_root.to_owned(),
							module: resolved,
							name: ty_name,
						},
						reexports,
					);
					insert_binding(scope, name, ty);
				}
			}
		}
		syn::UseTree::Rename(r) => {
			// Aliases (`use ... as Alias`) fail closed.
			scope.insert(r.rename.to_string(), Binding::Ambiguous);
		}
		syn::UseTree::Glob(_) => {
			// Only resolve globs of a known parsed module (e.g. `use super::*;`
			// inside a submodule of a parsed parent module). Unknown globs bind nothing.
			if let Some(target_mod) = resolve_mod_segments(cur_mod, prefix) {
				if let Some(enums) =
					known_module_enums.get(&(crate_root.to_owned(), target_mod.clone()))
				{
					for name in enums {
						let ty = canonicalize_reexport(
							CanonicalType {
								crate_root: crate_root.to_owned(),
								module: target_mod.clone(),
								name: name.clone(),
							},
							reexports,
						);
						insert_binding(scope, name.clone(), ty);
					}
				}
			}
		}
	}
}

/// Resolve a type qualifier path (without the variant segment) to its exact
/// `CanonicalType`. Fails closed (`None`) if ambiguous or unresolved.
fn resolve_type_path(
	segs: &[String],
	crate_root: &str,
	cur_mod: &[String],
	scope: &BTreeMap<String, Binding>,
	reexports: &BTreeMap<CanonicalType, CanonicalType>,
) -> Option<CanonicalType> {
	if segs.is_empty() {
		return None;
	}
	if segs.len() == 1 {
		return match scope.get(&segs[0]) {
			Some(Binding::Exact(ty)) => Some(ty.clone()),
			_ => None,
		};
	}
	if let Some(mut resolved) = resolve_mod_segments(cur_mod, segs) {
		let name = resolved.pop()?;
		return Some(canonicalize_reexport(
			CanonicalType {
				crate_root: crate_root.to_owned(),
				module: resolved,
				name,
			},
			reexports,
		));
	}
	// Handle `<local_mod>::<Type>` when `<local_mod>` is a child module or imported module.
	// Fail closed unless exact `crate::`/`super::`/`self::` or single-segment imported type.
	None
}

fn path_is_new_variant(
	path: &syn::Path,
	crate_root: &str,
	cur_mod: &[String],
	scope: &BTreeMap<String, Binding>,
	reexports: &BTreeMap<CanonicalType, CanonicalType>,
	added_by_enum: &BTreeMap<CanonicalType, BTreeSet<String>>,
	current_impl_enum: Option<&CanonicalType>,
) -> bool {
	let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
	if segs.len() < 2 {
		return false;
	}
	let var = segs.last().unwrap();
	let qual = &segs[..segs.len() - 1];
	let target_enum = if qual.len() == 1 && qual[0] == "Self" {
		match current_impl_enum {
			Some(ty) => ty.clone(),
			None => return false,
		}
	} else {
		match resolve_type_path(qual, crate_root, cur_mod, scope, reexports) {
			Some(ty) => ty,
			None => return false,
		}
	};
	added_by_enum
		.get(&target_enum)
		.is_some_and(|vs| vs.contains(var))
}

fn is_new_variant_pat(pat: &syn::Pat, ctx: &MatchNormalizer<'_>, forbid_bindings: bool) -> bool {
	match pat {
		syn::Pat::Path(p) => path_is_new_variant(
			&p.path,
			&ctx.crate_root,
			&ctx.cur_mod,
			&ctx.scope,
			ctx.reexports,
			ctx.added_by_enum,
			ctx.current_impl_enum.as_ref(),
		),
		syn::Pat::Ident(_) => false,
		syn::Pat::TupleStruct(ts) => {
			if !path_is_new_variant(
				&ts.path,
				&ctx.crate_root,
				&ctx.cur_mod,
				&ctx.scope,
				ctx.reexports,
				ctx.added_by_enum,
				ctx.current_impl_enum.as_ref(),
			) {
				return false;
			}
			if forbid_bindings {
				ts.elems
					.iter()
					.all(|e| matches!(e, syn::Pat::Wild(_) | syn::Pat::Rest(_)))
			} else {
				true
			}
		}
		syn::Pat::Struct(ps) => {
			if !path_is_new_variant(
				&ps.path,
				&ctx.crate_root,
				&ctx.cur_mod,
				&ctx.scope,
				ctx.reexports,
				ctx.added_by_enum,
				ctx.current_impl_enum.as_ref(),
			) {
				return false;
			}
			if forbid_bindings {
				ps.fields
					.iter()
					.all(|f| matches!(f.pat.as_ref(), syn::Pat::Wild(_)))
			} else {
				true
			}
		}
		syn::Pat::Or(or_pat) => {
			!or_pat.cases.is_empty()
				&& or_pat
					.cases
					.iter()
					.all(|c| is_new_variant_pat(c, ctx, forbid_bindings))
		}
		_ => false,
	}
}

/// Inspect an added arm's body for forbidden ingress constructs (`unsafe`, `transmute`,
/// macros naming a new variant).
struct AddedArmAudit<'a> {
	new_variant_names: &'a BTreeSet<String>,
	error: Option<String>,
}

impl<'a, 'ast> Visit<'ast> for AddedArmAudit<'a> {
	fn visit_expr_unsafe(&mut self, _: &'ast syn::ExprUnsafe) {
		if self.error.is_none() {
			self.error = Some("unsafe block inside added arm".into());
		}
	}
	fn visit_path(&mut self, p: &'ast syn::Path) {
		if let Some(last) = p.segments.last() {
			if last.ident == "transmute" && self.error.is_none() {
				self.error = Some("transmute inside added arm".into());
			}
		}
		visit::visit_path(self, p);
	}
	fn visit_macro(&mut self, m: &'ast syn::Macro) {
		let s = tokens(&m.tokens);
		for v in self.new_variant_names {
			if s.split_whitespace().any(|tok| tok == v) && self.error.is_none() {
				self.error = Some(format!(
					"macro `{}` references new variant `{v}` inside added arm",
					tokens(&m.path)
				));
			}
		}
		visit::visit_macro(self, m);
	}
}

struct MatchNormalizer<'a> {
	rel_path: &'a str,
	crate_root: String,
	cur_mod: Vec<String>,
	scope: BTreeMap<String, Binding>,
	known_module_enums: &'a BTreeMap<(String, Vec<String>), Vec<String>>,
	reexports: &'a BTreeMap<CanonicalType, CanonicalType>,
	added_by_enum: &'a BTreeMap<CanonicalType, BTreeSet<String>>,
	changed_enum_names: &'a BTreeSet<String>,
	new_variant_names: &'a BTreeSet<String>,
	current_impl_enum: Option<CanonicalType>,
	added_arms: Vec<(String, String)>,
	error: Option<String>,
}

impl<'a> VisitMut for MatchNormalizer<'a> {
	fn visit_item_mod_mut(&mut self, item: &mut syn::ItemMod) {
		if let Some((_, inner)) = &mut item.content {
			let prev_mod = self.cur_mod.clone();
			let prev_scope = self.scope.clone();
			self.cur_mod.push(item.ident.to_string());
			let mut next_scope = BTreeMap::new();
			populate_scope_from_items(
				&self.crate_root,
				&self.cur_mod,
				inner,
				self.known_module_enums,
				self.reexports,
				&mut next_scope,
			);
			self.scope = next_scope;
			for sub in inner.iter_mut() {
				self.visit_item_mut(sub);
			}
			self.cur_mod = prev_mod;
			self.scope = prev_scope;
		}
	}

	fn visit_item_impl_mut(&mut self, item: &mut syn::ItemImpl) {
		let prev = self.current_impl_enum.clone();
		self.current_impl_enum = if let syn::Type::Path(tp) = item.self_ty.as_ref() {
			let segs: Vec<String> = tp
				.path
				.segments
				.iter()
				.map(|s| s.ident.to_string())
				.collect();
			resolve_type_path(
				&segs,
				&self.crate_root,
				&self.cur_mod,
				&self.scope,
				self.reexports,
			)
		} else {
			None
		};
		visit_mut::visit_item_impl_mut(self, item);
		self.current_impl_enum = prev;
	}

	fn visit_block_mut(&mut self, block: &mut syn::Block) {
		let prev_scope = self.scope.clone();
		let mut block_items = Vec::new();
		for stmt in &block.stmts {
			if let syn::Stmt::Item(it) = stmt {
				// Fix 8: any block-local type with the same name as a changed enum
				// fails closed (prevents shadowing a module enum).
				let shadowed = match it {
					syn::Item::Enum(x) => Some(x.ident.to_string()),
					syn::Item::Struct(x) => Some(x.ident.to_string()),
					syn::Item::Type(x) => Some(x.ident.to_string()),
					syn::Item::Union(x) => Some(x.ident.to_string()),
					syn::Item::Trait(x) => Some(x.ident.to_string()),
					_ => None,
				};
				if let Some(name) = shadowed {
					if self.changed_enum_names.contains(&name) && self.error.is_none() {
						self.error = Some(format!(
							"{}: block-local type `{name}` shadows changed enum",
							self.rel_path
						));
					}
				}
				block_items.push(it.clone());
			}
		}
		if !block_items.is_empty() {
			populate_scope_from_items(
				&self.crate_root,
				&self.cur_mod,
				&block_items,
				self.known_module_enums,
				self.reexports,
				&mut self.scope,
			);
		}
		visit_mut::visit_block_mut(self, block);
		self.scope = prev_scope;
	}

	fn visit_expr_match_mut(&mut self, expr: &mut syn::ExprMatch) {
		let mut kept = Vec::with_capacity(expr.arms.len());
		for mut arm in expr.arms.drain(..) {
			if arm.guard.is_none() && is_new_variant_pat(&arm.pat, self, false) {
				let mut audit = AddedArmAudit {
					new_variant_names: self.new_variant_names,
					error: None,
				};
				audit.visit_expr(&arm.body);
				if let Some(err) = audit.error {
					if self.error.is_none() {
						self.error = Some(format!("{}: {err}", self.rel_path));
					}
				}
				self.added_arms
					.push((self.rel_path.to_owned(), tokens(&arm.pat)));
				continue;
			}
			if let syn::Pat::Or(ref mut or_pat) = arm.pat {
				let mut remaining = syn::punctuated::Punctuated::new();
				let mut stripped_any = false;
				for case in or_pat.cases.iter() {
					if is_new_variant_pat(case, self, true) {
						stripped_any = true;
						self.added_arms
							.push((self.rel_path.to_owned(), format!("| {}", tokens(case))));
					} else {
						remaining.push(case.clone());
					}
				}
				if stripped_any && !remaining.is_empty() {
					if remaining.len() == 1 {
						arm.pat = remaining.into_iter().next().unwrap();
					} else {
						or_pat.cases = remaining;
					}
				}
			}
			self.visit_arm_mut(&mut arm);
			kept.push(arm);
		}
		expr.arms = kept;
		self.visit_expr_mut(&mut expr.expr);
	}
}

/// Verify that a set of changed files (from `git diff --name-only baseline candidate`)
/// constitutes a representation-only dormant feature delta.
pub fn verify_structural_delta(
	all_changed_paths: &[String],
	pairs: &[FilePair],
	parent_sources: &BTreeMap<String, String>,
) -> Result<StructuralProof, String> {
	// 1. Refuse any non-`.rs` file outside `plans/`, and any `build.rs`.
	for path in all_changed_paths {
		if path.starts_with("plans/") {
			continue;
		}
		if !path.ends_with(".rs") || path == "build.rs" || path.ends_with("/build.rs") {
			return Err(format!(
				"non-structural production file in delta: {path} (only .rs production files allowed)"
			));
		}
	}

	let mut known_module_enums: BTreeMap<(String, Vec<String>), Vec<String>> = BTreeMap::new();
	let mut reexports: BTreeMap<CanonicalType, CanonicalType> = BTreeMap::new();

	// Index enums and `pub use` re-exports from all available parent/sibling module sources.
	for (path, src) in parent_sources {
		if let Some((crate_root, module)) = file_module_path(path) {
			if let Ok(mut ast) = syn::parse_file(src) {
				strip_cfg_test_items(&mut ast.items);
				let mut enums = BTreeMap::new();
				collect_enums(&crate_root, &module, &ast.items, &mut enums);
				for key in enums.keys() {
					known_module_enums
						.entry((key.crate_root.clone(), key.module.clone()))
						.or_default()
						.push(key.name.clone());
				}
				collect_reexports(&crate_root, &module, &ast.items, &mut reexports);
			}
		}
	}

	let mut parsed_pairs = Vec::new();
	let mut added_by_enum: BTreeMap<CanonicalType, BTreeSet<String>> = BTreeMap::new();
	let mut new_variant_names: BTreeSet<String> = BTreeSet::new();
	let mut added_variants: Vec<String> = Vec::new();

	// 2. Parse every non-test production `.rs` file pair, strip `#[cfg(test)]`, and
	//    collect newly added enum variants with their full canonical module identity.
	for pair in pairs {
		if pair.rel_path.starts_with("plans/")
			|| is_outside_src_test_or_bench(&pair.rel_path)
			|| is_proven_cfg_test_under_src(&pair.rel_path, parent_sources)
		{
			continue;
		}
		let (crate_root, module) = file_module_path(&pair.rel_path)
			.ok_or_else(|| format!("cannot determine module path for {}", pair.rel_path))?;
		let mut base_ast = syn::parse_file(&pair.base_src)
			.map_err(|e| format!("parse baseline {}: {e}", pair.rel_path))?;
		let mut cand_ast = syn::parse_file(&pair.cand_src)
			.map_err(|e| format!("parse candidate {}: {e}", pair.rel_path))?;
		strip_cfg_test_items(&mut base_ast.items);
		strip_cfg_test_items(&mut cand_ast.items);

		collect_reexports(&crate_root, &module, &cand_ast.items, &mut reexports);

		let mut base_enums = BTreeMap::new();
		let mut cand_enums = BTreeMap::new();
		collect_enums(&crate_root, &module, &base_ast.items, &mut base_enums);
		collect_enums(&crate_root, &module, &cand_ast.items, &mut cand_enums);

		for key in cand_enums.keys() {
			let entry = known_module_enums
				.entry((key.crate_root.clone(), key.module.clone()))
				.or_default();
			if !entry.contains(&key.name) {
				entry.push(key.name.clone());
			}
		}

		for (key, cand_enum) in &cand_enums {
			let Some(base_enum) = base_enums.get(key) else {
				return Err(format!(
					"{}: new enum `{}` added (only new variants on existing enums allowed)",
					pair.rel_path,
					key.display()
				));
			};
			if tokens(base_enum) == tokens(cand_enum) {
				continue;
			}
			check_enum_attrs(cand_enum)?;
			let base_names: BTreeSet<String> = base_enum
				.variants
				.iter()
				.map(|v| v.ident.to_string())
				.collect();
			for v in &cand_enum.variants {
				let vname = v.ident.to_string();
				if !base_names.contains(&vname) {
					added_by_enum
						.entry(key.clone())
						.or_default()
						.insert(vname.clone());
					new_variant_names.insert(vname.clone());
					added_variants.push(format!("{}::{vname}", key.display()));
				}
			}
		}

		parsed_pairs.push((
			pair.rel_path.clone(),
			crate_root,
			module,
			base_ast,
			cand_ast,
		));
	}

	if added_variants.is_empty() {
		return Err("no newly added enum variants found in production delta".into());
	}
	added_variants.sort();
	added_variants.dedup();

	// 3. For every production file, strip the newly added enum variants and legal
	//    new-variant-only match arms / `|` alternatives (resolved to exact CanonicalType),
	//    then require exact token equality with baseline.
	let changed_enum_names: BTreeSet<String> =
		added_by_enum.keys().map(|k| k.name.clone()).collect();
	let mut added_arms = Vec::new();
	for (rel_path, crate_root, module, mut base_ast, mut cand_ast) in parsed_pairs {
		strip_added_enum_variants(&crate_root, &module, &mut cand_ast.items, &added_by_enum);
		normalize_enum_trailing_commas(&mut base_ast.items);
		normalize_enum_trailing_commas(&mut cand_ast.items);
		let mut scope = BTreeMap::new();
		populate_scope_from_items(
			&crate_root,
			&module,
			&cand_ast.items,
			&known_module_enums,
			&reexports,
			&mut scope,
		);
		let mut norm = MatchNormalizer {
			rel_path: &rel_path,
			crate_root,
			cur_mod: module,
			scope,
			known_module_enums: &known_module_enums,
			reexports: &reexports,
			added_by_enum: &added_by_enum,
			changed_enum_names: &changed_enum_names,
			new_variant_names: &new_variant_names,
			current_impl_enum: None,
			added_arms: Vec::new(),
			error: None,
		};
		norm.visit_file_mut(&mut cand_ast);
		if let Some(err) = norm.error {
			return Err(err);
		}
		let base_toks = tokens(&base_ast);
		let cand_toks = tokens(&cand_ast);
		if base_toks != cand_toks {
			return Err(format!(
				"{rel_path}: production AST differs outside new enum variants and new-variant-only match arms"
			));
		}
		added_arms.extend(norm.added_arms);
	}

	added_arms.sort();
	Ok(StructuralProof {
		added_variants,
		added_arms,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn accepts_representation_only_variant_and_arms_and_or_alternative() {
		let base = r#"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type { String, Vec }
enum Payload { String(String), Vec(Vec<u8>) }
impl Payload {
    fn tag(&self) -> Type {
        match self {
            Self::String(_) => Type::String,
            Self::Vec(_) => Type::Vec,
        }
    }
}
fn children(p: &Payload) -> usize {
    match p {
        Payload::String(_) => 0,
        Payload::Vec(v) => v.len(),
    }
}
"#;
		let cand = r#"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type { String, Vec, Bytes }
enum Payload {
    String(String),
    Vec(Vec<u8>),
    #[cfg_attr(not(test), allow(dead_code))]
    Bytes(Vec<u8>),
}
impl Payload {
    fn tag(&self) -> Type {
        match self {
            Self::String(_) => Type::String,
            Self::Bytes(_) => Type::Bytes,
            Self::Vec(_) => Type::Vec,
        }
    }
}
fn children(p: &Payload) -> usize {
    match p {
        Payload::String(_) | Payload::Bytes(_) => 0,
        Payload::Vec(v) => v.len(),
    }
}
"#;
		let proof = verify_structural_delta(
			&["src/heap.rs".into()],
			&[FilePair {
				rel_path: "src/heap.rs".into(),
				base_src: base.into(),
				cand_src: cand.into(),
			}],
			&BTreeMap::new(),
		)
		.unwrap();
		assert_eq!(
			proof.added_variants,
			vec!["heap::Payload::Bytes", "heap::Type::Bytes"]
		);
		assert_eq!(proof.added_arms.len(), 2);
	}

	#[test]
	fn refuses_same_named_enum_in_different_module() {
		// Fix 2 control: `src/a.rs` adds `a::Payload::Bytes`; `src/b.rs` already has
		// `b::Payload::Bytes`; an arm added on `b::Payload::Bytes` MUST be refused!
		let a_base = "pub enum Payload { String(String) }";
		let a_cand = "pub enum Payload { String(String), Bytes(Vec<u8>) }";
		let b_base = r#"
pub enum Payload { Int, Bytes(u8) }
pub fn handle(p: Payload) -> u8 {
    match p {
        Payload::Int => 0,
        _ => 1,
    }
}
"#;
		let b_cand = r#"
pub enum Payload { Int, Bytes(u8) }
pub fn handle(p: Payload) -> u8 {
    match p {
        Payload::Int => 0,
        Payload::Bytes(x) => x + 1,
        _ => 1,
    }
}
"#;
		let err = verify_structural_delta(
			&["src/a.rs".into(), "src/b.rs".into()],
			&[
				FilePair {
					rel_path: "src/a.rs".into(),
					base_src: a_base.into(),
					cand_src: a_cand.into(),
				},
				FilePair {
					rel_path: "src/b.rs".into(),
					base_src: b_base.into(),
					cand_src: b_cand.into(),
				},
			],
			&BTreeMap::new(),
		)
		.unwrap_err();
		assert!(err.contains("src/b.rs"), "{err}");
	}

	#[test]
	fn refuses_nested_inline_cfg_test_exempting_root_file() {
		// Fix 3 control: `src/lib.rs` has `mod worker; mod controls { #[cfg(test)] mod worker; }`.
		// The nested `#[cfg(test)] mod worker;` points to `src/controls/worker.rs`, NOT `src/worker.rs`!
		let mut parents = BTreeMap::new();
		parents.insert(
			"src/lib.rs".into(),
			"mod heap;\nmod worker;\nmod controls {\n\t#[cfg(test)]\n\tmod worker;\n}\n".into(),
		);
		let err = verify_structural_delta(
			&["src/heap.rs".into(), "src/worker.rs".into()],
			&[
				FilePair {
					rel_path: "src/heap.rs".into(),
					base_src: "pub enum Payload { String(String) }".into(),
					cand_src: "pub enum Payload { String(String), Bytes(Vec<u8>) }".into(),
				},
				FilePair {
					rel_path: "src/worker.rs".into(),
					base_src: "pub fn run() -> usize { 1 }".into(),
					cand_src: "pub fn run() -> usize { 2 }".into(),
				},
			],
			&parents,
		)
		.unwrap_err();
		assert!(err.contains("src/worker.rs"), "{err}");
	}

	#[test]
	fn refuses_expanding_cfg_attr_derive_on_changed_enum() {
		// Fix 7 control: unchanged `#[cfg_attr(feature = "serde", derive(Deserialize))]`
		// on a changed enum MUST be refused!
		let base = r#"
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub enum Payload { String(String) }
"#;
		let cand = r#"
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub enum Payload { String(String), Bytes(Vec<u8>) }
"#;
		let err = verify_structural_delta(
			&["src/heap.rs".into()],
			&[FilePair {
				rel_path: "src/heap.rs".into(),
				base_src: base.into(),
				cand_src: cand.into(),
			}],
			&BTreeMap::new(),
		)
		.unwrap_err();
		assert!(err.contains("non-inert cfg_attr"), "{err}");
	}

	#[test]
	fn refuses_block_local_enum_shadowing_changed_enum() {
		// Fix 8 control (Codex's exact `shadow` fixture): a block-local `enum Payload`
		// shadows the module-level changed `enum Payload` so an added arm on the local
		// enum MUST fail closed.
		let base = r#"
enum Payload { A }
pub fn f() -> u8 {
    enum Payload { A, Bytes }
    match Payload::Bytes { _ => 0 }
}
"#;
		let cand = r#"
enum Payload { A, Bytes }
pub fn f() -> u8 {
    enum Payload { A, Bytes }
    match Payload::Bytes { Payload::Bytes => 1, _ => 0 }
}
"#;
		let err = verify_structural_delta(
			&["src/lib.rs".into()],
			&[FilePair {
				rel_path: "src/lib.rs".into(),
				base_src: base.into(),
				cand_src: cand.into(),
			}],
			&BTreeMap::new(),
		)
		.unwrap_err();
		assert!(err.contains("shadows changed enum"), "{err}");
	}

	#[test]
	fn refuses_worker_shared_by_two_crate_roots() {
		// Fix 9 control (Codex's exact `two-roots` fixture): `src/lib.rs` declares
		// `#[cfg(test)] mod worker;`, while `src/main.rs` declares non-test `mod worker;`.
		let mut parents = BTreeMap::new();
		parents.insert(
			"src/lib.rs".into(),
			"enum P { A, Bytes }\n#[cfg(test)] mod worker;\n".into(),
		);
		parents.insert(
			"src/main.rs".into(),
			"mod worker;\nfn main() { println!(\"{}\", worker::f()); }\n".into(),
		);
		let err = verify_structural_delta(
			&["src/lib.rs".into(), "src/worker.rs".into()],
			&[
				FilePair {
					rel_path: "src/lib.rs".into(),
					base_src: "enum P { A }\n#[cfg(test)] mod worker;\n".into(),
					cand_src: "enum P { A, Bytes }\n#[cfg(test)] mod worker;\n".into(),
				},
				FilePair {
					rel_path: "src/worker.rs".into(),
					base_src: "pub fn f() -> u8 { 0 }\n".into(),
					cand_src: "pub fn f() -> u8 { 1 }\n".into(),
				},
			],
			&parents,
		)
		.unwrap_err();
		assert!(err.contains("src/worker.rs"), "{err}");
	}

	#[test]
	fn refuses_qualified_custom_derive_on_changed_enum() {
		// Fix 14 control: `#[derive(my_crate::Clone)]` is NOT standard `Clone` and MUST be
		// refused, whereas `#[derive(Clone, core::fmt::Debug)]` is accepted.
		let bad_base = "#[derive(my_crate::Clone)]\npub enum Payload { String(String) }\n";
		let bad_cand =
			"#[derive(my_crate::Clone)]\npub enum Payload { String(String), Bytes(Vec<u8>) }\n";
		let err = verify_structural_delta(
			&["src/heap.rs".into()],
			&[FilePair {
				rel_path: "src/heap.rs".into(),
				base_src: bad_base.into(),
				cand_src: bad_cand.into(),
			}],
			&BTreeMap::new(),
		)
		.unwrap_err();
		assert!(err.contains("non-allowlisted derive"), "{err}");

		let ok_base = "#[derive(Clone, core::fmt::Debug)]\npub enum Payload { String(String) }\n";
		let ok_cand = "#[derive(Clone, core::fmt::Debug)]\npub enum Payload { String(String), Bytes(Vec<u8>) }\n";
		assert!(verify_structural_delta(
			&["src/heap.rs".into()],
			&[FilePair {
				rel_path: "src/heap.rs".into(),
				base_src: ok_base.into(),
				cand_src: ok_cand.into(),
			}],
			&BTreeMap::new(),
		)
		.is_ok());
	}
}
