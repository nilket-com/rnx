use crate::emit::callable::{emit_callable, emit_method_with};
use crate::emit::{Emitted, Entry, signature_of};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::oracle::emit::emit_oracle;
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::world::World;
use std::collections::{BTreeMap, BTreeSet};

/// Record 0078 gate 1 controls: a by-reference twin gets its own `_ref`
/// name; two distinct sources with one last segment get crate-qualified
/// names; a twin pair is never merged, whatever the impls return; the
/// segment rule spells `Vec` and `Option` sources.
pub(crate) fn from_naming_self_test() {
	fn from(key: &str, owner: &str, src: &str) -> Callable {
		Callable {
			key: key.into(),
			kind: "foreign_trait_impl".into(),
			krate: "polars_core".into(),
			owner: owner.into(),
			name: format!("From<{}>", src.rsplit("::").next().unwrap()),
			canonical_path: format!("{owner} as core::convert::From"),
			found_paths: vec![],
			crate_paths: vec![],
			receiver: "none".into(),
			params: vec![Param {
				name: "value".into(),
				ty: src.into(),
				ty_canonical: src.into(),
			}],
			ret: None,
			ret_canonical: Some(owner.into()),
			generics_canonical: vec![],
			impl_for: Some(owner.into()),
			impl_bounds: vec![],
			impl_head: None,
			impl_where: vec![],
			impl_assoc: vec![],
			docs_first: None,
			owner_generic: false,
			is_unsafe: false,
			is_async: false,
			deprecated: false,
			hidden: false,
			implementors: vec![],
			trait_reachable: false,
			derived: false,
			bucket: "conversion".into(),
			rules: vec![],
			trait_lifetimes: vec![],
		}
	}
	let o = "polars_core::datatypes::field::Field";
	let inv = Inventory {
		callables: vec![
			from("a", o, "polars_core::datatypes::dtype::DataType"),
			from("b", o, "&polars_core::datatypes::dtype::DataType"),
			from("c", o, "polars_core::schema::Field"),
			from("d", o, "polars_arrow::datatypes::field::Field"),
			from("e", o, "alloc::vec::Vec<polars_core::series::Series>"),
			from("f", o, "core::option::Option<i64>"),
			from("g", o, "i64"),
		],
		supporting: vec![],
		provenance: None,
	};
	let names = plan_from_names(&inv);
	assert_eq!(names["a"], "from_data_type");
	assert_eq!(
		names["b"], "from_data_type_ref",
		"a by-reference twin is its own binding"
	);
	assert_ne!(names["a"], names["b"], "twins are never merged");
	assert_eq!(
		names["c"], "from_core_field",
		"distinct sources with one last segment are crate-qualified"
	);
	assert_eq!(names["d"], "from_arrow_field");
	assert_eq!(names["e"], "from_vec_series");
	assert_eq!(names["f"], "from_option_i64");
	assert_eq!(names["g"], "from_i64");
	let distinct: BTreeSet<&String> = names.values().collect();
	assert_eq!(distinct.len(), names.len(), "every impl has its own name");
	println!("from-naming self-test: ok");
}

/// Record 0078 gate 1 controls, from a synthetic inventory through the
/// production emission: an inherent `from_x` makes the impl unsupported
/// with the collision named; an integer source is fallible and an `f32`
/// source is not; an unmappable source is refused with the type named; a
/// by-value/by-reference twin pair yields two bindings and two oracle
/// cases, each calling its own impl.
pub(crate) fn from_emission_self_test() {
	fn sup(path: &str, derived: &[&str]) -> Supporting {
		Supporting {
			trait_params: vec![],
			key: path.to_string(),
			kind: "struct".into(),
			canonical_path: path.to_string(),
			found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
			crate_paths: vec![path.to_string()],
			public_fields: 0,
			fields_canonical: vec![],
			variant_shapes: vec![],
			variant_payloads: vec![],
			generic: false,
			lifetime: false,
			hidden: false,
			derived: derived.iter().map(|d| d.to_string()).collect(),
			alias_target: None,
			implementors: vec![],
			impls: vec![],
		}
	}
	let owner = "polars_core::scalar::Scalar";
	let dtype = "polars_core::datatypes::dtype::DataType";
	let field = "polars_core::datatypes::field::Field";
	let from = |key: &str, src: &str| Callable {
		key: key.into(),
		kind: "foreign_trait_impl".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: format!("From<{}>", src.rsplit("::").next().unwrap()),
		canonical_path: format!("{owner} as core::convert::From"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "none".into(),
		params: vec![Param {
			name: "value".into(),
			ty: src.into(),
			ty_canonical: src.into(),
		}],
		ret: None,
		ret_canonical: Some(owner.into()),
		generics_canonical: vec![],
		impl_for: Some(owner.into()),
		impl_bounds: vec![],
		impl_head: None,
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: false,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "conversion".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let inherent = Callable {
		key: "inh".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: "from_data_type".into(),
		canonical_path: format!("{owner}::from_data_type"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "none".into(),
		params: vec![Param {
			name: "d".into(),
			ty: dtype.into(),
			ty_canonical: dtype.into(),
		}],
		ret: None,
		ret_canonical: Some(owner.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![],
		impl_head: None,
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: false,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "mechanical".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let inv = Inventory {
		callables: vec![
			inherent,
			from("clash", dtype),
			from("int", "i8"),
			from("float", "f32"),
			from("arrow", "polars_arrow::datatypes::field::Field"),
			from("twin_v", field),
			from("twin_r", &format!("&{field}")),
		],
		supporting: vec![
			sup(owner, &["Clone", "Debug", "PartialEq"]),
			sup(dtype, &["Clone", "Debug", "PartialEq", "Default"]),
			sup(field, &["Clone", "Debug", "PartialEq", "Default"]),
		],
		provenance: None,
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let world = World::new(&inv, &release, &["mechanical", "conversion"]);
	let mut out = Emitted {
		from_names: plan_from_names(&inv),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let buckets = ["mechanical", "conversion"];
	for c in &inv.callables {
		emit_callable(&world, &mut out, c, &buckets);
	}
	fn find<'a>(entries: &'a [Entry], key: &str) -> &'a Entry {
		entries
			.iter()
			.find(|e| e.key == key)
			.unwrap_or_else(|| panic!("no entry for {key}"))
	}
	let entry = |key: &str| find(&out.entries, key);
	let clash = entry("clash");
	assert_eq!(
		clash.status, "unsupported",
		"an inherent from_x makes the impl unsupported"
	);
	assert!(
		clash
			.reason
			.as_deref()
			.unwrap_or("")
			.contains("name taken by inherent from_data_type"),
		"the collision is named: {:?}",
		clash.reason
	);
	assert_eq!(
		(entry("int").status, entry("int").fallible),
		("generated", Some(true)),
		"an integer source narrows fallibly"
	);
	assert_eq!(
		(entry("float").status, entry("float").fallible),
		("generated", Some(false)),
		"an f32 source is an infallible cast"
	);
	assert!(
		out.functions.contains("(value as f32)"),
		"the f32 cast is emitted as written"
	);
	let arrow = entry("arrow");
	assert_eq!(arrow.status, "unsupported");
	assert!(
		arrow
			.reason
			.as_deref()
			.unwrap_or("")
			.starts_with("conversion source"),
		"{:?}",
		arrow.reason
	);
	assert!(
		arrow
			.reason
			.as_deref()
			.unwrap_or("")
			.contains("polars_arrow::datatypes::field::Field"),
		"the unmappable source is named: {:?}",
		arrow.reason
	);
	let (tv, tr) = (entry("twin_v"), entry("twin_r"));
	assert_eq!((tv.status, tr.status), ("generated", "generated"));
	assert_eq!(
		tv.bindings.len() + tr.bindings.len(),
		2,
		"a twin pair is two bindings"
	);
	assert_ne!(
		tv.bindings[0].rune, tr.bindings[0].rune,
		"with distinct names"
	);
	// the arrow Field source shares the last segment, so both twins are crate-qualified
	assert_eq!(
		(tv.bindings[0].rune.as_str(), tr.bindings[0].rune.as_str()),
		(
			"polars::Scalar::from_core_field",
			"polars::Scalar::from_core_field_ref"
		)
	);
	assert!(
		out.functions
			.contains("<polars::prelude::Scalar as From<polars::prelude::Field>>::from")
			|| out.functions.contains(&format!(
				"<{} as From<{}>>::from",
				world.wrappers[owner].spell, world.wrappers[field].spell
			)),
		"the by-value twin calls its own impl"
	);
	assert!(
		out.functions.contains(&format!(
			"<{} as From<&{}>>::from",
			world.wrappers[owner].spell, world.wrappers[field].spell
		)),
		"the by-reference twin calls its own impl"
	);
	assert!(
		!out.entries.iter().any(|e| e.status == "adapted"),
		"no impl is adapted as provided by another"
	);
	let (_, harness, _, _) = emit_oracle(&world, &mut out.entries, &inv);
	let entry = |key: &str| find(&out.entries, key);
	let ids: Vec<String> = ["twin_v", "twin_r"]
		.iter()
		.map(|k| {
			entry(k).bindings[0]
				.case_id
				.clone()
				.unwrap_or_else(|| panic!("{k}: no case ({:?})", entry(k).bindings[0].disposition))
		})
		.collect();
	assert_ne!(ids[0], ids[1], "a twin pair is two oracle cases");
	assert!(
		ids.iter()
			.all(|i| harness.contains(&format!("Case {{ id: \"{i}\""))),
		"both cases are emitted: {ids:?}"
	);
	// the by-value case hands the prepared source to the call and returns it for comparison; the by-reference case calls with the prepared value
	assert!(
		harness
			.contains("let s = __fx[0]; let r = polars::Scalar::from_core_field(s); ((r, s), ())"),
		"the by-value case preserves its source"
	);
	assert!(
		harness.contains("polars::Scalar::from_core_field_ref(__fx[0])"),
		"the by-reference case calls its own binding"
	);
	assert!(
		harness.contains("as From<polars::prelude::Field>>::from(__a0.clone())")
			|| harness.contains(&format!(
				"<{} as From<{}>>::from(__a0.clone())",
				world.wrappers[owner].spell, world.wrappers[field].spell
			)),
		"the by-value oracle clones the source for its own impl"
	);
	// duplicate listings: a repeated impl (one impl id on two pages) is
	// adapted with its retained counterpart named; an outward impl on the
	// source page alone is unsupported; a repeated impl whose retained
	// listing is unsupported is unsupported too
	let listing = |key: &str, page: &str, for_ty: &str, src: &str| {
		let mut c = from(key, src);
		c.owner = page.into();
		c.canonical_path = format!("{page} as core::convert::From");
		c.impl_for = Some(for_ty.into());
		c
	};
	let arrow = "polars_arrow::datatypes::field::Field";
	let inv = Inventory {
		callables: vec![
			listing("polars_core:s:100", owner, owner, dtype),
			listing("polars_core:d:100", dtype, owner, dtype),
			listing("polars_core:s:200", owner, "&'static str", owner),
			listing(
				"polars_core:s:201",
				owner,
				"(polars_utils::pl_str::PlSmallStr, polars_core::datatypes::dtype::DataType)",
				owner,
			),
			listing("polars_core:s:300", owner, owner, arrow),
			listing("polars_core:f:300", arrow, owner, arrow),
		],
		supporting: vec![
			sup(owner, &["Clone", "Debug", "PartialEq"]),
			sup(dtype, &["Clone", "Debug", "PartialEq", "Default"]),
		],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical", "conversion"]);
	let mut out = Emitted {
		from_names: plan_from_names(&inv),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	for c in &inv.callables {
		emit_callable(&world, &mut out, c, &buckets);
	}
	resolve_duplicates(&mut out.entries);
	let entry = |key: &str| find(&out.entries, key);
	assert_eq!(entry("polars_core:s:100").status, "generated");
	let dup = entry("polars_core:d:100");
	assert_eq!(
		(dup.status, dup.counterpart.as_deref()),
		("adapted", Some("polars_core:s:100")),
		"a repeated impl names its retained listing"
	);
	assert_eq!(
		dup.rune,
		entry("polars_core:s:100").rune,
		"and carries that listing's binding"
	);
	for k in ["polars_core:s:200", "polars_core:s:201"] {
		let e = entry(k);
		assert_eq!(
			(e.status, e.counterpart.as_deref()),
			("unsupported", None),
			"{k}: an outward impl is unsupported"
		);
		assert!(
			e.reason
				.as_deref()
				.unwrap_or("")
				.starts_with("outward conversion"),
			"{k}: {:?}",
			e.reason
		);
	}
	assert_eq!(entry("polars_core:s:300").status, "unsupported");
	let e = entry("polars_core:f:300");
	assert_eq!(
		(e.status, e.rune.as_deref()),
		("unsupported", None),
		"a repeated impl whose retained listing is unsupported is unsupported"
	);
	assert!(
		e.reason
			.as_deref()
			.unwrap_or("")
			.contains("retained listing is unsupported"),
		"{:?}",
		e.reason
	);
	assert!(
		!out.entries
			.iter()
			.any(|e| e.status == "adapted" && e.counterpart.is_none()),
		"no listing is adapted without an identified counterpart"
	);
	println!("from-emission self-test: ok");
}

/// The snake-case name of a conversion source's last segment, with `Vec`
/// and `Option` spelled as prefixes.
pub(crate) fn source_segment(src: &str) -> String {
	let t = src.trim().trim_start_matches('&').trim();
	if t.starts_with('(') {
		return "tuple".into();
	}
	if let Some(inner) = t
		.strip_prefix("alloc::vec::Vec<")
		.and_then(|r| r.strip_suffix('>'))
	{
		return format!("vec_{}", source_segment(inner));
	}
	if let Some(inner) = t
		.strip_prefix("core::option::Option<")
		.and_then(|r| r.strip_suffix('>'))
	{
		return format!("option_{}", source_segment(inner));
	}
	let seg = t
		.split('<')
		.next()
		.unwrap_or(t)
		.rsplit("::")
		.next()
		.unwrap_or(t);
	let mut out = String::new();
	for (i, ch) in seg.chars().enumerate() {
		if ch.is_ascii_uppercase() && i > 0 {
			out.push('_');
		}
		out.push(ch.to_ascii_lowercase());
	}
	out
}

/// The crate short name of a source type (`polars_core::…` gives `core`),
/// or the scalar's own name.
pub(crate) fn source_crate(src: &str) -> String {
	let t = src.trim().trim_start_matches('&').trim();
	let first = t.split("::").next().unwrap_or(t);
	if first.contains('<') || !t.contains("::") {
		return "scalar".into();
	}
	first.trim_start_matches("polars_").to_string()
}

/// The `from_<source>` names of every `From` impl, per callable key
/// (record 0078). A by-reference impl beside its by-value twin gets
/// `_ref`; two distinct sources whose last segment coincides on one owner
/// get names qualified by the source's crate; nothing is merged.
pub(crate) fn plan_from_names(inv: &Inventory) -> BTreeMap<String, String> {
	let mut by_owner: BTreeMap<&str, Vec<(&Callable, String, bool)>> = BTreeMap::new();
	for c in &inv.callables {
		if c.kind != "foreign_trait_impl" || !c.name.starts_with("From<") {
			continue;
		}
		// rustdoc lists `impl From<&X> for Y` on X's page as well; the impl
		// belongs to its `for` type, and the listing on X is a duplicate
		if !from_impl_is_on_its_owner(c) {
			continue;
		}
		let Some(src) = c.params.first().map(|p| p.ty_canonical.as_str()) else {
			continue;
		};
		let by_ref = src.trim().starts_with('&');
		by_owner
			.entry(c.owner.as_str())
			.or_default()
			.push((c, src.to_string(), by_ref));
	}
	let mut names = BTreeMap::new();
	for (_, impls) in by_owner {
		// distinct (by-value) sources per last segment
		let mut per_seg: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
		for (_, src, _) in &impls {
			per_seg
				.entry(source_segment(src))
				.or_default()
				.insert(src.trim().trim_start_matches('&').trim().to_string());
		}
		for (c, src, by_ref) in &impls {
			let seg = source_segment(src);
			let base = if per_seg[&seg].len() > 1 {
				format!("from_{}_{seg}", source_crate(src))
			} else {
				format!("from_{seg}")
			};
			names.insert(
				c.key.clone(),
				if *by_ref { format!("{base}_ref") } else { base },
			);
		}
	}
	names
}

/// Whether a `From` impl's recorded `for` type is the type it is listed
/// under (rustdoc lists an impl on the pages of both types it mentions).
pub(crate) fn from_impl_is_on_its_owner(c: &Callable) -> bool {
	match &c.impl_for {
		Some(f) => f.split('<').next().unwrap_or(f) == c.owner,
		None => true,
	}
}

/// Record 0078: a duplicate listing stays adapted only when its
/// counterpart is generated; a counterpart refused for any reason makes
/// the listing unsupported with that reason, so nothing is called bound
/// that is not.
pub(crate) fn resolve_duplicates(entries: &mut [Entry]) {
	let by_key: BTreeMap<String, (&'static str, Option<String>, Option<String>)> = entries
		.iter()
		.map(|e| (e.key.clone(), (e.status, e.rune.clone(), e.reason.clone())))
		.collect();
	for e in entries.iter_mut() {
		let Some(k) = e.counterpart.clone() else {
			continue;
		};
		match by_key.get(&k) {
			Some(("generated", rune, _)) => {
				e.rune = rune.clone();
				e.reason = Some(format!(
					"{}; bound there as `{}`",
					e.reason.take().unwrap_or_default(),
					rune.as_deref()
						.unwrap_or("")
						.rsplit("::")
						.next()
						.unwrap_or("")
				));
			}
			Some((status, _, reason)) => {
				e.status = "unsupported";
				e.rune = None;
				e.reason = Some(format!(
					"duplicate listing whose retained listing is {status}: {}",
					reason.as_deref().unwrap_or("(no reason)")
				));
			}
			None => {
				e.status = "unsupported";
				e.rune = None;
				e.reason = Some("duplicate listing whose retained listing has no entry".into());
			}
		}
	}
}

/// The impl identity a rustdoc impl listing carries: crate and impl id
/// (`krate:owner:impl` keys share the impl id across the pages it is
/// listed on).
pub(crate) fn impl_identity(key: &str) -> String {
	let mut it = key.split(':');
	let krate = it.next().unwrap_or("");
	let last = key.rsplit(':').next().unwrap_or("");
	format!("{krate}:{last}")
}

/// A `From<X>` impl as a constructor binding `T::from_<source>(x)` calling
/// `<T as From<X>>::from` by UFCS; an unmappable source is refused with
/// the reason `conversion source`.
pub(crate) fn emit_from(world: &World, out: &mut Emitted, c: &Callable, name: &str) {
	let owner = &c.owner;
	let Some(w) = world.wrapper_for(owner) else {
		out.unsupported(c, "owner not wrapped", owner);
		return;
	};
	let Some(src) = c.params.first() else {
		out.unsupported(c, "conversion source", "none");
		return;
	};
	if out.taken.contains_key(&(w.rust.clone(), name.to_string())) {
		out.unsupported(
			c,
			&format!("name taken by inherent {name}"),
			&out.taken[&(w.rust.clone(), name.to_string())].clone(),
		);
		return;
	}
	let mut syn = c.clone();
	syn.kind = "inherent".into();
	syn.receiver = "none".into();
	syn.name = name.to_string();
	syn.params = vec![Param {
		name: "value".into(),
		ty: src.ty.clone(),
		ty_canonical: src.ty_canonical.clone(),
	}];
	syn.ret = Some(w.spell.clone());
	syn.ret_canonical = Some(owner.clone());
	syn.generics_canonical.clear();
	// the Rust identifier is hashed from the callable's path: make it name the source too
	syn.canonical_path = format!("{} as core::convert::From<{}>", owner, src.ty_canonical);
	let src_spell = &c.name[5..c.name.len() - 1]; // the `X` of `From<X>` as rustdoc rendered it
	let callee = format!(
		"<{} as From<{}>>::from",
		w.spell,
		spell_source(world, src_spell, &src.ty_canonical)
	);
	let before = out.entries.len();
	emit_method_with(world, out, &syn, owner, None, false, Some(&callee));
	debug_assert_eq!(before + 1, out.entries.len());
	let e = out.entries.last_mut().unwrap();
	e.key = c.key.clone();
	e.canonical_path = c.canonical_path.clone();
	e.kind = c.kind.clone();
	e.signature = signature_of(c);
	if e.status == "unsupported" {
		if let Some(r) = &e.reason {
			if !r.starts_with("name taken") {
				e.reason = Some(format!("conversion source: {r}"));
			}
		}
	} else if e.status == "generated" {
		e.note = Some(format!(
			"From<{}> as `{name}`{}",
			src_spell,
			if src.ty_canonical.trim().starts_with('&') {
				" (by reference; its by-value twin, if any, is a separate binding)"
			} else {
				""
			}
		));
	}
}

/// The Rust spelling of a conversion source for the UFCS path: a wrapped
/// type by its wrapper's spelling, a scalar as is, a reference kept.
pub(crate) fn spell_source(world: &World, rendered: &str, canonical: &str) -> String {
	let by_ref = canonical.trim().starts_with('&');
	let bare = canonical.trim().trim_start_matches('&').trim();
	let inner = match world.wrappers.get(bare) {
		Some(w) => w.spell.clone(),
		None => match bare.split('<').next().unwrap_or(bare) {
			"alloc::string::String" => "String".into(),
			"polars_utils::pl_str::PlSmallStr" => "polars::prelude::PlSmallStr".into(),
			"alloc::vec::Vec" | "core::option::Option" => {
				rendered.trim_start_matches('&').to_string()
			}
			_ => bare.to_string(),
		},
	};
	if by_ref { format!("&{inner}") } else { inner }
}

#[cfg(test)]
mod tests {
	#[test]
	fn from_naming() {
		super::from_naming_self_test();
	}
	#[test]
	fn from_emission() {
		super::from_emission_self_test();
	}
}
