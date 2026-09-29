pub(crate) mod mapping;
pub(crate) mod proof;
use crate::census::callback_census;
use crate::emit::callable::{SCALAR_MAPPED, bucket_admitted};
use crate::emit::rune_path;
use crate::families::Active;
use crate::families::callbacks::plan_holder_non_plan_method;
use crate::model::{Callable, Inventory, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::text::{sanitize, split_top};
use crate::ty;
use crate::ty::last;
use crate::world::mapping::SCALARS;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(crate) fn wrapper_self_test() {
	fn sup(path: &str, kind: &str, target: Option<&str>) -> Supporting {
		Supporting {
			trait_params: vec![],
			key: path.to_string(),
			kind: kind.to_string(),
			canonical_path: path.to_string(),
			found_paths: vec![format!(
				"polars::{}",
				path.split("::").skip(1).collect::<Vec<_>>().join("::")
			)],
			crate_paths: vec![path.to_string()],
			public_fields: 0,
			fields_canonical: vec![],
			variant_shapes: vec![],
			variant_payloads: vec![],
			generic: false,
			lifetime: false,
			hidden: false,
			derived: vec![],
			alias_target: target.map(|t| t.to_string()),
			implementors: vec![],
			impls: vec![],
		}
	}
	let mut generic = sup("polars_core::chunked_array::ChunkedArray", "struct", None);
	generic.generic = true;
	let inv = Inventory {
		callables: vec![],
		provenance: None,
		supporting: vec![
			generic,
			sup(
				"polars_core::datatypes::UInt32Chunked",
				"type_alias",
				Some(
					"polars_core::chunked_array::ChunkedArray<polars_core::datatypes::UInt32Type>",
				),
			),
			sup(
				"polars_core::datatypes::aliases::IdxCa",
				"type_alias",
				Some(
					"polars_core::chunked_array::ChunkedArray<polars_core::datatypes::UInt32Type>",
				),
			),
			sup(
				"polars_core::datatypes::BooleanChunked",
				"type_alias",
				Some(
					"polars_core::chunked_array::ChunkedArray<polars_core::datatypes::BooleanType>",
				),
			),
			sup("polars_core::schema::Field", "struct", None),
			sup("polars_plan::dsl::Field", "struct", None),
			sup(
				"polars_dtype::categorical::CatSize",
				"type_alias",
				Some("u32"),
			),
		],
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into(), "polars_plan".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let w = World::new(&inv, &release, &["mechanical"]);
	let idx = &w.wrappers["polars_core::datatypes::aliases::IdxCa"];
	let u32c = &w.wrappers["polars_core::datatypes::UInt32Chunked"];
	assert_eq!(
		idx.rust, u32c.rust,
		"equivalent aliases must share one wrapper"
	);
	assert_eq!(
		rune_path(idx),
		"polars::IdxCa",
		"the representative is the alphabetically first alias name"
	);
	assert_eq!(
		idx.aliases,
		vec![
			"polars_core::datatypes::aliases::IdxCa".to_string(),
			"polars_core::datatypes::UInt32Chunked".to_string()
		]
	);
	let boolean = &w.wrappers["polars_core::datatypes::BooleanChunked"];
	assert_ne!(
		boolean.rust, idx.rust,
		"a different instantiation is a different wrapper"
	);
	let f1 = rune_path(&w.wrappers["polars_core::schema::Field"]);
	let f2 = rune_path(&w.wrappers["polars_plan::dsl::Field"]);
	assert_ne!(
		f1, f2,
		"same-name distinct types must have distinct Rune paths"
	);
	assert_eq!(
		(f1.as_str(), f2.as_str()),
		("polars::core::Field", "polars::plan::Field")
	);
	assert!(
		!w.wrappers
			.contains_key("polars_dtype::categorical::CatSize"),
		"a scalar alias is not wrapped"
	);
	let distinct: BTreeSet<&str> = w.wrappers.values().map(|w| w.rust.as_str()).collect();
	assert_eq!(
		distinct.len(),
		4,
		"four wrapper structs: one shared alias, BooleanChunked, two Fields"
	);
	println!("wrapper self-test: ok");
	// deref targets: only a `dyn Trait` target of a wrapped type names a route
	let mk = |owner: &str, name: &str, assoc: Vec<(String, String)>| Callable {
		key: format!("{owner}#{name}"),
		kind: "foreign_trait_impl".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: name.into(),
		canonical_path: format!("{owner} as {name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: None,
		generics_canonical: vec![],
		impl_for: Some(owner.into()),
		impl_bounds: vec![],
		impl_head: None,
		impl_where: vec![],
		impl_assoc: assoc,
		docs_first: None,
		owner_generic: false,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let mut tr = sup("polars_core::schema::SomeTrait", "trait", None);
	tr.kind = "trait".into();
	let inv = Inventory {
		callables: vec![
			mk(
				"polars_core::schema::Field",
				"Deref",
				vec![("Target".into(), "dyn polars_core::schema::SomeTrait".into())],
			),
			mk(
				"polars_plan::dsl::Field",
				"Deref",
				vec![("Target".into(), "alloc::vec::Vec<u8>".into())],
			),
			mk("polars_plan::dsl::Field", "DerefMut", vec![]),
		],
		provenance: None,
		supporting: vec![
			sup("polars_core::schema::Field", "struct", None),
			sup("polars_plan::dsl::Field", "struct", None),
			tr,
		],
	};
	let w = World::new(&inv, &release, &["mechanical"]);
	assert_eq!(
		w.deref_targets
			.get("polars_core::schema::SomeTrait")
			.map(|v| v.len()),
		Some(1),
		"a dyn-trait target names one route"
	);
	assert!(
		w.deref_targets
			.values()
			.flatten()
			.all(|o| o == "polars_core::schema::Field"),
		"a Deref to a non-trait target binds nothing"
	);
	assert!(
		w.deref_mut.contains("polars_plan::dsl::Field")
			&& !w.deref_mut.contains("polars_core::schema::Field")
	);
	println!("deref self-test: ok");
}

/// Hand-written wrappers in the adapter, by canonical type path, with the
/// Rust path of the wrapper and the Rune method names they already bind.
pub(crate) const HAND_WRAPPERS: &[(&str, &str)] = &[
	(
		"polars_core::frame::dataframe::DataFrame",
		"crate::DataFrame",
	),
	("polars_lazy::frame::LazyFrame", "crate::LazyFrame"),
	("polars_lazy::frame::LazyGroupBy", "crate::LazyGroupBy"),
	("polars_plan::dsl::expr::Expr", "crate::Expr"),
];
pub(crate) const HAND_METHODS: &[(&str, &str)] = &[
	("polars_core::frame::dataframe::DataFrame", "lazy"),
	("polars_core::frame::dataframe::DataFrame", "preview"),
	(
		"polars_core::frame::dataframe::DataFrame",
		"write_parquet_new",
	),
	("polars_lazy::frame::LazyFrame", "filter"),
	("polars_lazy::frame::LazyFrame", "group_by"),
	("polars_lazy::frame::LazyFrame", "sort"),
	("polars_lazy::frame::LazyFrame", "collect"),
	("polars_lazy::frame::LazyGroupBy", "agg"),
	("polars_plan::dsl::expr::Expr", "gt"),
	("polars_plan::dsl::expr::Expr", "add"),
	("polars_plan::dsl::expr::Expr", "sum"),
	("polars_plan::dsl::expr::Expr", "alias"),
];
/// Rune keywords, from rune 0.14.2 `ast::generated`; a binding whose name
/// is one gets a trailing underscore.
pub(crate) const RUNE_KEYWORDS: &[&str] = &[
	"abstract", "alignof", "as", "async", "await", "become", "break", "const", "continue", "crate",
	"default", "do", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in",
	"is", "let", "loop", "macro", "match", "mod", "move", "mut", "not", "offsetof", "override",
	"priv", "proc", "pub", "pure", "ref", "return", "select", "self", "sizeof", "static", "struct",
	"super", "true", "typeof", "unsafe", "use", "virtual", "while", "yield",
];

pub(crate) fn rune_name(name: &str) -> String {
	if RUNE_KEYWORDS.contains(&name) {
		format!("{name}_")
	} else {
		name.to_string()
	}
}

pub(crate) const HAND_FREE: &[&str] = &["col", "lit", "version", "read_csv", "read_parquet"];
pub(crate) const HAND_PROTOCOLS: &[(&str, &str)] = &[
	("polars_core::frame::dataframe::DataFrame", "Display"),
	("polars_plan::dsl::expr::Expr", "Add"),
];

/// One wrapped Polars type.
#[derive(Clone)]
pub(crate) struct Wrapper {
	/// Rust path of the wrapper struct as seen from `crate::generated::functions`.
	pub(crate) rust: String,
	/// Rust spelling of the wrapped Polars type.
	pub(crate) spell: String,
	/// Rune item (module path) and name.
	pub(crate) rune_item: String,
	pub(crate) rune_name: String,
	pub(crate) hand: bool,
	/// Identity: the canonical instantiated type this wrapper holds. For a
	/// struct or enum it is the type's own path; for an alias it is the
	/// expanded alias target, so equivalent aliases share one wrapper.
	pub(crate) identity: String,
	/// The rule that admitted the wrapper: `api type`, `alias`, `internal type`.
	pub(crate) rule: &'static str,
	/// Every canonical path (the type, or each alias) this wrapper serves.
	pub(crate) aliases: Vec<String>,
	/// For an alias: the generic base type of its target (an `Arc<T>`
	/// target reports `T`'s base), whose Clone/Debug/Default impls apply.
	pub(crate) base: Option<String>,
}

pub(crate) struct World {
	pub(crate) release: Release,
	pub(crate) callback_dispositions: BTreeMap<String, String>,
	pub(crate) callback_unclassified_sinks: Vec<String>,
	pub(crate) callback_sink_groups: BTreeSet<String>,
	/// Counter for per-binding temporaries.
	pub(crate) tmp: std::cell::Cell<usize>,
	/// Record 0115: the listed families' states for the callable or pair
	/// being emitted (see `families::CALLABLE`, `PAIR`, `FREE`).
	pub(crate) active: Active,
	/// Record 0087: rendered return type of each listed iterator-return
	/// callable, to its item, so the oracle can frame the Rust result.
	pub(crate) iter_return_items: BTreeMap<String, String>,
	pub(crate) types: BTreeMap<String, Supporting>,
	pub(crate) wrappers: BTreeMap<String, Wrapper>,
	pub(crate) ambiguous_prelude: BTreeSet<String>,
	/// Types with a `Clone` impl, derived or hand-written.
	pub(crate) clonable: BTreeSet<String>,
	/// Foreign trait impls by owner: (trait short name, `for` type, bounds).
	pub(crate) impls: BTreeMap<String, Vec<(String, String, Vec<(String, String)>)>>,
	/// Record 0116 (rule 3, review): each owner's recorded inherent impl
	/// heads (`Owner<T>`), from the inventory, so a dtype listing's generic
	/// is checked against the owner's actual parameter.
	pub(crate) owner_heads: BTreeMap<String, BTreeSet<String>>,
	/// Record 0076: trait canonical path -> wrapped types whose `Deref`
	/// target is `dyn` that trait (`Series` -> `dyn SeriesTrait`).
	pub(crate) deref_targets: BTreeMap<String, Vec<String>>,
	/// Wrapped types with a `DerefMut` impl.
	pub(crate) deref_mut: BTreeSet<String>,
	/// Record 0076: an instantiation rendered canonically (`ChunkedArray<
	/// Int64Type>`) -> the alias wrapper key that holds exactly that type.
	pub(crate) by_identity: BTreeMap<String, String>,
}

/// A spellable Rust path for an inventoried item, in order: a public
/// re-export path from the facade that avoids `prelude`; a public path
/// inside the defining crate (the adapter depends on those crates
/// directly); an unambiguous name directly under the facade prelude.
pub(crate) fn spell(
	found: &[String],
	own: &[String],
	name: &str,
	ambiguous: &BTreeSet<String>,
) -> Option<String> {
	let mut direct: Vec<&String> = found
		.iter()
		.filter(|f| !f.contains(" as ") && !f.contains("::prelude::"))
		.collect();
	direct.sort_by_key(|f| (f.matches("::").count(), f.as_str()));
	if let Some(d) = direct.first() {
		return Some((*d).clone());
	}
	let mut own_direct: Vec<&String> = own
		.iter()
		.filter(|f| !f.contains(" as ") && !f.contains("::prelude::"))
		.collect();
	own_direct.sort_by_key(|f| (f.matches("::").count(), f.as_str()));
	if let Some(d) = own_direct.first() {
		return Some((*d).clone());
	}
	let mut own_prelude: Vec<&String> = own
		.iter()
		.filter(|f| {
			!f.contains(" as ") && f.contains("::prelude::") && f.matches("::").count() == 2
		})
		.collect();
	own_prelude.sort();
	if let Some(o) = own_prelude.first() {
		return Some((*o).clone());
	}
	let mut via: Vec<&String> = found
		.iter()
		.filter(|f| {
			!f.contains(" as ")
				&& f.starts_with("polars::prelude::")
				&& f.matches("::").count() == 2
		})
		.collect();
	via.sort_by_key(|f| (f.matches("::").count(), f.as_str()));
	if let Some(v) = via.first() {
		if !ambiguous.contains(name) {
			return Some((*v).clone());
		}
	}
	None
}

/// A spelling through the `polars` facade only, for a type of a crate the
/// adapter does not depend on directly: the shortest non-prelude re-export,
/// then a nested prelude path, then `polars::prelude::Name` when unambiguous.
pub(crate) fn spell_facade(
	found: &[String],
	name: &str,
	ambiguous: &BTreeSet<String>,
) -> Option<String> {
	let mut direct: Vec<&String> = found
		.iter()
		.filter(|f| !f.contains(" as ") && f.starts_with("polars::") && !f.contains("::prelude::"))
		.collect();
	direct.sort_by_key(|f| (last(f) != name, f.matches("::").count(), f.as_str()));
	if let Some(d) = direct.first() {
		return Some((*d).clone());
	}
	let mut nested: Vec<&String> = found
		.iter()
		.filter(|f| {
			!f.contains(" as ") && f.starts_with("polars::prelude::") && f.matches("::").count() > 2
		})
		.collect();
	nested.sort_by_key(|f| (last(f) != name, f.matches("::").count(), f.as_str()));
	if let Some(n) = nested.first() {
		return Some((*n).clone());
	}
	let short = format!("polars::prelude::{name}");
	if found.iter().any(|f| *f == short) && !ambiguous.contains(name) {
		return Some(short);
	}
	None
}

impl World {
	pub(crate) fn new(inv: &Inventory, release: &Release, buckets: &[&str]) -> World {
		let mut types = BTreeMap::new();
		let mut owners: HashMap<String, BTreeSet<String>> = HashMap::new();
		for s in &inv.supporting {
			types.insert(s.canonical_path.clone(), s.clone());
			for fp in &s.found_paths {
				if fp.starts_with("polars::prelude::") && fp.matches("::").count() == 2 {
					owners
						.entry(last(fp).to_string())
						.or_default()
						.insert(s.key.clone());
				}
			}
		}
		for c in &inv.callables {
			for fp in &c.found_paths {
				if fp.starts_with("polars::prelude::") && fp.matches("::").count() == 2 {
					owners
						.entry(last(fp).to_string())
						.or_default()
						.insert(c.key.clone());
				}
			}
		}
		let ambiguous_prelude: BTreeSet<String> = owners
			.into_iter()
			.filter(|(_, v)| v.len() > 1)
			.map(|(k, _)| k)
			.collect();
		let mut clonable: BTreeSet<String> = types
			.values()
			.filter(|s| s.derived.iter().any(|d| d == "Clone"))
			.map(|s| s.canonical_path.clone())
			.collect();
		for c in &inv.callables {
			if c.kind == "foreign_trait_impl" && c.name.starts_with("Clone") {
				clonable.insert(c.owner.clone());
			}
		}
		// Types an API-crate signature in the accepted buckets mentions, by
		// canonical path: the admission test for internal-crate wrappers.
		let mut mentioned: BTreeSet<String> = BTreeSet::new();
		for c in &inv.callables {
			if !bucket_admitted(buckets, c) || !release.is_api(&c.krate) {
				continue;
			}
			for t in c
				.params
				.iter()
				.map(|p| p.ty_canonical.as_str())
				.chain(c.ret_canonical.as_deref())
			{
				for m in type_paths(t) {
					mentioned.insert(m);
				}
			}
		}
		let mut impls: BTreeMap<String, Vec<(String, String, Vec<(String, String)>)>> =
			BTreeMap::new();
		for c in &inv.callables {
			if c.kind == "foreign_trait_impl" {
				if let Some(f) = &c.impl_for {
					let short = c.name.split('<').next().unwrap_or("").to_string();
					impls.entry(c.owner.clone()).or_default().push((
						short,
						f.clone(),
						c.impl_bounds.clone(),
					));
				}
			}
		}
		let callback_unclassified_sinks = inv
			.callables
			.iter()
			.filter(|c| {
				release.is_api(&c.krate)
					&& plan_holder_non_plan_method(c)
					&& !release
						.families
						.callback_sink
						.iter()
						.any(|s| s.path == c.canonical_path)
			})
			.map(|c| c.canonical_path.clone())
			.collect();
		let callback_sink_groups = release
			.families
			.callback_sink
			.iter()
			.filter(|s| s.sink != "none")
			.map(|s| s.sink.clone())
			.collect();
		let mut w = World {
			release: release.clone(),
			callback_dispositions: BTreeMap::new(),
			callback_unclassified_sinks,
			callback_sink_groups,
			tmp: std::cell::Cell::new(0),
			active: Active::default(),
			iter_return_items: inv
				.callables
				.iter()
				.filter_map(|c| {
					release
						.families
						.iterator_returns
						.iter()
						.find(|r| r.path == c.canonical_path)
						.and_then(|r| {
							c.ret_canonical
								.as_ref()
								.map(|t| (ty::parse(t).render(), r.item.clone()))
						})
				})
				.collect(),
			types,
			wrappers: BTreeMap::new(),
			ambiguous_prelude,
			clonable,
			impls,
			deref_targets: BTreeMap::new(),
			deref_mut: BTreeSet::new(),
			by_identity: BTreeMap::new(),
			owner_heads: BTreeMap::new(),
		};
		for c in &inv.callables {
			if let (true, Some(h)) = (c.kind == "inherent", &c.impl_head) {
				w.owner_heads
					.entry(c.owner.clone())
					.or_default()
					.insert(h.clone());
			}
		}
		w.assign_wrappers(&mentioned);
		for (path, wr) in &w.wrappers {
			if wr.rule == "alias"
				&& wr.identity.contains('<')
				&& wr.aliases.first().is_some_and(|a| a == path)
			{
				w.by_identity.insert(wr.identity.clone(), path.clone());
			}
		}
		for c in &inv.callables {
			if c.kind != "foreign_trait_impl" || !w.wrappers.contains_key(&c.owner) {
				continue;
			}
			let short = c.name.split('<').next().unwrap_or("");
			if short == "DerefMut" {
				w.deref_mut.insert(c.owner.clone());
			}
			if short == "Deref" {
				if let Some((_, target)) = c.impl_assoc.iter().find(|(n, _)| n == "Target") {
					// only a trait-object target names a trait whose methods the type exposes
					if let Some(tr) = target.strip_prefix("dyn ") {
						let tr = tr.split(" + ").next().unwrap_or(tr).trim();
						if w.types.get(tr).is_some_and(|t| t.kind == "trait") {
							w.deref_targets
								.entry(tr.to_string())
								.or_default()
								.push(c.owner.clone());
						}
					}
				}
			}
		}
		for v in w.deref_targets.values_mut() {
			v.sort();
			v.dedup();
		}
		// an alias wrapper clones when an impl covers its instantiation
		let mut more = Vec::new();
		for (path, wr) in &w.wrappers {
			if wr.base.is_some() && w.trait_holds(&wr.identity, "Clone", 0) {
				more.push(path.clone());
			}
		}
		w.clonable.extend(more);
		let callback_rows = callback_census(&w, inv, &[]);
		for row in callback_rows["rows"].as_array().into_iter().flatten() {
			if let (Some(key), Some(disposition)) =
				(row["key"].as_str(), row["disposition"].as_str())
			{
				w.callback_dispositions
					.insert(key.into(), disposition.into());
			}
		}
		w
	}

	/// Whether `trait_short` (Clone, Debug, Default, PartialEq) holds for a
	/// canonical type, from the inventory alone: a derive or impl on a
	/// concrete type; for an instantiation `Base<A, B>`, an impl on `Base`
	/// whose `for` type unifies with it and whose bounds on the unified
	/// parameters hold (a local trait by its recorded implementors, one of
	/// the four by recursion, marker bounds trivially). `Arc<T>` clones
	/// always and otherwise follows `T`; `()` and scalars satisfy all four.
	pub(crate) fn trait_holds(&self, ty: &str, trait_short: &str, depth: usize) -> bool {
		if depth > 8 {
			return false;
		}
		let ty = ty.trim();
		if ty == "()"
			|| SCALARS.contains(&ty)
			|| ty == "alloc::string::String"
			|| ty == "polars_utils::pl_str::PlSmallStr"
		{
			return true;
		}
		if let Some(inner) = ty
			.strip_prefix("alloc::sync::Arc<")
			.and_then(|r| r.strip_suffix('>'))
		{
			return trait_short == "Clone" || self.trait_holds(inner, trait_short, depth + 1);
		}
		let base = ty.split('<').next().unwrap_or(ty);
		if let Some(s) = self.types.get(base) {
			if s.derived.iter().any(|d| d == trait_short) && !ty.contains('<') {
				return true;
			}
			if s.kind == "type_alias" {
				if let Some(t) = &s.alias_target {
					return self.trait_holds(t, trait_short, depth + 1);
				}
			}
		}
		let Some(impls) = self.impls.get(base) else {
			return false;
		};
		let args = split_top(
			ty.strip_prefix(base)
				.and_then(|r| r.strip_prefix('<'))
				.and_then(|r| r.strip_suffix('>'))
				.unwrap_or(""),
		);
		'impls: for (short, for_ty, bounds) in impls {
			if short != trait_short {
				continue;
			}
			if for_ty == ty {
				return true;
			}
			let for_args = split_top(
				for_ty
					.strip_prefix(base)
					.and_then(|r| r.strip_prefix('<'))
					.and_then(|r| r.strip_suffix('>'))
					.unwrap_or(""),
			);
			if for_args.len() != args.len() {
				continue;
			}
			for (fa, a) in for_args.iter().zip(&args) {
				if fa == a {
					continue;
				}
				let Some((_, b)) = bounds.iter().find(|(n, _)| n == fa) else {
					if fa.contains("::") || !fa.chars().all(|c| c.is_alphanumeric() || c == '_') {
						continue 'impls;
					}
					continue; // unbounded parameter
				};
				for bound in b.split(" + ").map(str::trim).filter(|b| !b.is_empty()) {
					let tpath = bound.split('<').next().unwrap();
					let tshort = last(tpath);
					let ok = match tshort {
						"Sized" | "Send" | "Sync" | "Unpin" | "Any" => true,
						"Clone" | "Debug" | "Default" | "PartialEq"
							if tpath.starts_with("core::") =>
						{
							self.trait_holds(a, tshort, depth + 1)
						}
						_ => self
							.types
							.get(tpath)
							.is_some_and(|t| t.implementors.iter().any(|i| i == a)),
					};
					if !ok {
						continue 'impls;
					}
				}
			}
			return true;
		}
		false
	}

	/// Every concrete, reachable, unhidden struct/enum/union in the API
	/// crates gets a wrapper; hand-written wrappers are reused. Record
	/// 0075 adds two rules: a concrete alias in an API crate whose expanded
	/// target is an instantiation of a reachable struct or enum (or an
	/// `Arc` of one) is wrapped under the alias's name, equivalent aliases
	/// sharing one wrapper; and a concrete, lifetime-free struct or enum of
	/// an internal crate that a bound API signature mentions is wrapped
	/// under `polars::<crate short name>::<Name>`.
	pub(crate) fn assign_wrappers(&mut self, mentioned: &BTreeSet<String>) {
		struct Cand {
			paths: Vec<String>,
			identity: String,
			rule: &'static str,
			base: Option<String>,
			internal: bool,
		}
		let mut by_name: BTreeMap<String, Vec<Cand>> = BTreeMap::new();
		let mut alias_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
		for (path, s) in &self.types {
			let api = self
				.release
				.api_crates
				.iter()
				.any(|c| path.starts_with(&format!("{c}::")));
			if s.generic || s.lifetime || s.hidden {
				continue;
			}
			// record 0110: a type converted natively to a Rune scalar is never
			// wrapped; the recovered impl records made `PlSmallStr` admissible,
			// which turned its string returns into an opaque wrapper
			if SCALAR_MAPPED.contains(&path.as_str()) {
				continue;
			}
			if spell(
				&s.found_paths,
				&s.crate_paths,
				last(path),
				&self.ambiguous_prelude,
			)
			.is_none() && spell_facade(&s.found_paths, last(path), &self.ambiguous_prelude)
				.is_none()
			{
				continue;
			}
			match s.kind.as_str() {
				"struct" | "enum" | "union" if api => {
					by_name
						.entry(last(path).to_string())
						.or_default()
						.push(Cand {
							paths: vec![path.clone()],
							identity: path.clone(),
							rule: "api type",
							base: None,
							internal: false,
						});
				}
				"struct" | "enum" | "union" if mentioned.contains(path) => {
					by_name
						.entry(last(path).to_string())
						.or_default()
						.push(Cand {
							paths: vec![path.clone()],
							identity: path.clone(),
							rule: "internal type",
							base: None,
							internal: true,
						});
				}
				"type_alias" if api => {
					let Some(target) = &s.alias_target else {
						continue;
					};
					let Some(base) = alias_base(target) else {
						continue;
					};
					let Some(b) = self.types.get(&base) else {
						continue;
					};
					if !matches!(b.kind.as_str(), "struct" | "enum" | "union")
						|| b.lifetime || b.hidden
					{
						continue;
					}
					if target.contains("&") || target.contains("dyn ") || target.contains("impl ") {
						continue;
					}
					alias_groups
						.entry(target.clone())
						.or_default()
						.push(path.clone());
				}
				_ => {}
			}
		}
		for (identity, mut paths) in alias_groups {
			// deterministic representative: the alphabetically first short
			// name, then the alphabetically first path
			paths.sort_by(|a, b| last(a).cmp(last(b)).then(a.cmp(b)));
			let base = alias_base(&identity);
			by_name
				.entry(last(&paths[0]).to_string())
				.or_default()
				.push(Cand {
					paths,
					identity,
					rule: "alias",
					base,
					internal: false,
				});
		}
		for (name, cands) in by_name {
			let collision = cands.iter().filter(|c| !c.internal).count() > 1;
			for cand in cands {
				let rep = &cand.paths[0];
				let s = &self.types[rep];
				let spelled = if cand.internal {
					spell_facade(&s.found_paths, &name, &self.ambiguous_prelude)
				} else {
					spell(
						&s.found_paths,
						&s.crate_paths,
						&name,
						&self.ambiguous_prelude,
					)
				};
				let Some(spelled) = spelled else { continue };
				let hand = HAND_WRAPPERS.iter().find(|(c, _)| *c == rep.as_str());
				let krate_short = rep
					.split("::")
					.next()
					.unwrap()
					.trim_start_matches("polars_")
					.to_string();
				let rune_item = if (collision || cand.internal) && hand.is_none() {
					format!("::polars::{krate_short}")
				} else {
					"::polars".to_string()
				};
				let w = Wrapper {
					rust: hand
						.map(|(_, r)| r.rsplit("::").next().unwrap().to_string())
						.unwrap_or_else(|| format!("W_{}", sanitize(rep))),
					spell: spelled,
					rune_item,
					rune_name: name.clone(),
					hand: hand.is_some(),
					identity: cand.identity.clone(),
					rule: cand.rule,
					aliases: cand.paths.clone(),
					base: cand.base.clone(),
				};
				for p in &cand.paths {
					self.wrappers.insert(p.clone(), w.clone());
				}
			}
		}
		// record 0116 (rule 3): the listed dtype instantiations of generic
		// owners with no alias, as synthetic alias wrappers
		match crate::families::dtype_owners::synthetic_wrappers(self) {
			Ok(ws) => {
				for (key, w) in ws {
					// the instantiation is a type of its own: the owner's record,
					// no longer generic, under the instantiated identity
					let base = w.base.clone().unwrap();
					let mut s = self.types[&base].clone();
					s.key = key.clone();
					s.canonical_path = key.clone();
					s.generic = false;
					s.alias_target = None;
					self.types.insert(key.clone(), s);
					if self.clonable.contains(&base) {
						self.clonable.insert(key.clone());
					}
					self.wrappers.insert(key, w);
				}
			}
			Err(why) => {
				eprintln!("refusing to generate: dtype instantiations: {why}");
				std::process::exit(2);
			}
		}
		// two wrappers must never share a Rune path
		let mut seen: BTreeMap<String, String> = BTreeMap::new();
		for w in self.wrappers.values() {
			let rp = rune_path(w);
			if let Some(prev) = seen.insert(rp.clone(), w.identity.clone()) {
				if prev != w.identity {
					panic!(
						"two wrappers share the Rune path {rp}: {prev} and {}",
						w.identity
					);
				}
			}
		}
	}
}

/// The generic base of an alias target: `ChunkedArray<BooleanType>` gives
/// `ChunkedArray`; an `Arc<T>` target gives `T`'s base. None for a scalar,
/// tuple, slice or reference target.
pub(crate) fn alias_base(target: &str) -> Option<String> {
	let t = target
		.strip_prefix("alloc::sync::Arc<")
		.and_then(|r| r.strip_suffix('>'))
		.unwrap_or(target);
	let base = t.split('<').next()?.trim();
	if base.is_empty()
		|| !base.contains("::")
		|| base.starts_with('(')
		|| base.starts_with('[')
		|| base.starts_with('&')
	{
		return None;
	}
	Some(base.to_string())
}

/// Every `a::b::C` path that occurs in a canonical type rendering.
pub(crate) fn type_paths(text: &str) -> Vec<String> {
	let mut out = Vec::new();
	let b = text.as_bytes();
	let mut i = 0;
	while i < b.len() {
		if b[i].is_ascii_alphabetic() || b[i] == b'_' {
			let start = i;
			while i < b.len()
				&& (b[i].is_ascii_alphanumeric()
					|| b[i] == b'_' || (b[i] == b':' && i + 1 < b.len() && b[i + 1] == b':')
					|| (b[i] == b':' && i > 0 && b[i - 1] == b':'))
			{
				i += 1;
			}
			let tok = &text[start..i];
			if tok.contains("::") {
				out.push(tok.to_string());
			}
		} else {
			i += 1;
		}
	}
	out
}

#[cfg(test)]
mod tests {
	#[test]
	fn wrapper() {
		super::wrapper_self_test();
	}
}
