use crate::emit::Emitted;
use crate::emit::callable::emit_callable;
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::world::proof::{Applicability, is_param, split_head};
use crate::world::{World, Wrapper};
use crate::{model, ty};
use std::collections::{BTreeMap, BTreeSet};

/// Record 0110: trait receivers are proven from impl records. A concrete
/// head resolves to its alias wrapper by exact identity; a generic head is
/// admitted per alias only when its bound holds; a blanket impl and a generic
/// trait give no receiver.
pub(crate) fn trait_receivers_self_test() {
	let sup = |path: &str, kind: &str, target: Option<&str>| Supporting {
		key: path.to_string(),
		kind: kind.to_string(),
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
		derived: vec!["Clone".into()],
		alias_target: target.map(|t| t.to_string()),
		implementors: vec![],
		impls: vec![],
	};
	let ca = "polars_core::chunked_array::ChunkedArray";
	let ti = |for_type: String, bounds: Vec<(&str, &str)>, blanket: bool| model::TraitImpl {
		for_type,
		blanket,
		bounds: bounds
			.into_iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		where_predicates: vec![],
		assoc_types: vec![],
	};
	let mut base = sup(ca, "struct", None);
	base.generic = true;
	let mut numeric = sup("polars_core::datatypes::PolarsNumericType", "trait", None);
	numeric.impls = vec![ti(
		"polars_core::datatypes::Int64Type".into(),
		vec![],
		false,
	)];
	let tr_path = "polars_core::chunked_array::ops::ChunkUnique";
	let mut tr = sup(tr_path, "trait", None);
	tr.impls = vec![
		ti(
			format!("{ca}<polars_core::datatypes::StringType>"),
			vec![],
			false,
		),
		ti(
			format!("{ca}<T>"),
			vec![("T", "polars_core::datatypes::PolarsNumericType")],
			false,
		),
		ti("T".into(), vec![], true),
	];
	let mut gtr = sup(
		"polars_core::chunked_array::ops::ChunkGeneric",
		"trait",
		None,
	);
	gtr.generic = true;
	gtr.impls = vec![ti(
		format!("{ca}<polars_core::datatypes::StringType>"),
		vec![],
		false,
	)];
	let mut btr = sup(
		"polars_core::chunked_array::ops::ChunkBounded",
		"trait",
		None,
	);
	btr.impls = vec![ti(
		format!("{ca}<polars_core::datatypes::StringType>"),
		vec![("Self", "core::marker::Send")],
		false,
	)];
	let inv = Inventory {
		callables: vec![],
		provenance: None,
		supporting: vec![
			base,
			numeric,
			tr,
			gtr,
			btr,
			sup("polars_core::datatypes::Int64Type", "struct", None),
			sup("polars_core::datatypes::BooleanType", "struct", None),
			sup("polars_core::datatypes::StringType", "struct", None),
			sup(
				"polars_core::datatypes::Int64Chunked",
				"type_alias",
				Some(&format!("{ca}<polars_core::datatypes::Int64Type>")),
			),
			sup(
				"polars_core::datatypes::BooleanChunked",
				"type_alias",
				Some(&format!("{ca}<polars_core::datatypes::BooleanType>")),
			),
			sup(
				"polars_core::datatypes::StringChunked",
				"type_alias",
				Some(&format!("{ca}<polars_core::datatypes::StringType>")),
			),
		],
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
	let w = World::new(&inv, &release, &["mechanical"]);
	let method = |owner: &str| Callable {
		key: "k".into(),
		kind: "trait_method".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: "n_unique".into(),
		canonical_path: format!("{owner}::n_unique"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: Some("usize".into()),
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
		trait_reachable: true,
		derived: false,
		bucket: "mechanical".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let wrapper_of = |identity: &str| {
		w.by_identity
			.get(identity)
			.cloned()
			.unwrap_or_else(|| panic!("no wrapper for {identity}: {:?}", w.by_identity))
	};
	let (got, why) = proven_trait_receivers(&w, &method(tr_path));
	let mut got = got;
	got.sort();
	let mut want = vec![
		wrapper_of(&format!("{ca}<polars_core::datatypes::StringType>")),
		wrapper_of(&format!("{ca}<polars_core::datatypes::Int64Type>")),
	];
	want.sort();
	assert_eq!(
		got, want,
		"concrete by identity, generic by proven bound, blanket never ({why:?})"
	);
	assert!(
		!got.contains(&wrapper_of(&format!(
			"{ca}<polars_core::datatypes::BooleanType>"
		))),
		"a failed bound gives no receiver"
	);
	let (got, why) =
		proven_trait_receivers(&w, &method("polars_core::chunked_array::ops::ChunkGeneric"));
	assert!(
		got.is_empty() && why == vec!["generic trait".to_string()],
		"{got:?} {why:?}"
	);
	// review of 0110: the whole emit path refuses a receiver the proof
	// rejects, even when an implementor label names a wrapped type: a
	// generic trait, and a concrete impl whose bounds are not decided
	let str_wrapper = wrapper_of(&format!("{ca}<polars_core::datatypes::StringType>"));
	let emit = |owner: &str| {
		let mut m = method(owner);
		m.implementors = vec![str_wrapper.clone()];
		let mut e = Emitted {
			from_names: BTreeMap::new(),
			functions: String::new(),
			registrations: vec![],
			catalogue: vec![],
			entries: vec![],
			taken: BTreeMap::new(),
			fn_index: 0,
		};
		emit_callable(&w, &mut e, &m, &["mechanical"]);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	let (status, reason, f) = emit("polars_core::chunked_array::ops::ChunkGeneric");
	assert_eq!(
		status, "unsupported",
		"a generic trait with a wrapped label must not emit: {reason}\n{f}"
	);
	assert!(reason.contains("generic trait"), "{reason}");
	let (status, reason, f) = emit("polars_core::chunked_array::ops::ChunkBounded");
	assert_eq!(
		status, "unsupported",
		"a concrete impl with undecided bounds must not emit: {reason}\n{f}"
	);
	assert!(
		reason.contains("a concrete impl with bounds is not decided here"),
		"{reason}"
	);
	println!("trait-receivers self-test: ok");
}

/// Record 0109: a non-Clone owner moves. A `self` receiver takes the
/// wrapper by value and passes `this.0`; a Clone owner still clones; a
/// top-level non-Clone argument moves; one inside a container stays refused.
pub(crate) fn move_semantics_self_test() {
	let series = "polars_core::series::Series";
	let ns = "polars_plan::dsl::string::StringNameSpace";
	let sup = |path: &str, clone: bool| Supporting {
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
		derived: if clone {
			vec!["Clone".into(), "Debug".into()]
		} else {
			vec!["Debug".into()]
		},
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let mk = |key: &str, owner: &str, receiver: &str, params: Vec<(&str, String)>| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: key.into(),
		canonical_path: format!("{owner}::{key}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params: params
			.into_iter()
			.map(|(n, t)| Param {
				name: n.into(),
				ty: t.clone(),
				ty_canonical: t,
			})
			.collect(),
		ret: None,
		ret_canonical: Some("bool".into()),
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
			mk("ns_consume", ns, "self", vec![]),
			mk("series_consume", series, "self", vec![]),
			mk("take_ns", series, "&self", vec![("n", ns.to_string())]),
			mk(
				"take_vec",
				series,
				"&self",
				vec![("v", format!("alloc::vec::Vec<{ns}>"))],
			),
		],
		supporting: vec![sup(series, true), sup(ns, false)],
		provenance: None,
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
	let world = World::new(&inv, &release, &["mechanical"]);
	let emit = |key: &str| {
		let mut e = Emitted {
			from_names: BTreeMap::new(),
			functions: String::new(),
			registrations: vec![],
			catalogue: vec![],
			entries: vec![],
			taken: BTreeMap::new(),
			fn_index: 0,
		};
		emit_callable(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical"],
		);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	let (status, reason, f) = emit("ns_consume");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("(this: W_polars_plan__dsl__string__StringNameSpace)") && f.contains("(this.0)"),
		"moved receiver:\n{f}"
	);
	let (status, reason, f) = emit("series_consume");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("this: &") && f.contains("this.0.clone()"),
		"a Clone receiver still clones:\n{f}"
	);
	let (status, reason, f) = emit("take_ns");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("n: W_polars_plan__dsl__string__StringNameSpace") && f.contains("n.0"),
		"moved argument:\n{f}"
	);
	let (status, reason, _) = emit("take_vec");
	assert_eq!(status, "unsupported");
	assert!(
		reason.contains("by-value argument of a non-Clone type"),
		"{reason}"
	);
	println!("move-semantics self-test: ok");
}

/// Record 0108: a method binds at most METHOD_ARITY arguments, receiver
/// included (Rune's InstanceFunction goes through Function); a longer one is
/// refused with the arity reason instead of emitting code that cannot compile.
pub(crate) fn method_arity_self_test() {
	let series = "polars_core::series::Series";
	let mk = |key: &str, n: usize| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: series.into(),
		name: key.into(),
		canonical_path: format!("{series}::{key}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: (0..n)
			.map(|i| Param {
				name: format!("a{i}"),
				ty: "bool".into(),
				ty_canonical: "bool".into(),
			})
			.collect(),
		ret: None,
		ret_canonical: Some("bool".into()),
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
	let sup = Supporting {
		key: series.to_string(),
		kind: "struct".into(),
		canonical_path: series.to_string(),
		found_paths: vec!["polars::Series".into()],
		crate_paths: vec![series.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let inv = Inventory {
		callables: vec![mk("four", 4), mk("five", 5)],
		supporting: vec![sup],
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
	let world = World::new(&inv, &release, &["mechanical"]);
	let emit = |key: &str| {
		let mut e = Emitted {
			from_names: BTreeMap::new(),
			functions: String::new(),
			registrations: vec![],
			catalogue: vec![],
			entries: vec![],
			taken: BTreeMap::new(),
			fn_index: 0,
		};
		emit_callable(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical"],
		);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.fn_index,
		)
	};
	let (status, reason, _) = emit("four");
	assert_eq!(status, "generated", "receiver + 4 binds: {reason}");
	let (status, reason, idx) = emit("five");
	assert_eq!(status, "unsupported");
	assert!(
		reason.starts_with(
			"arity: method with 6 arguments including the receiver; Rune binds at most 5"
		),
		"{reason}"
	);
	assert_eq!(idx, 0, "a refused method consumes no function index");
	println!("method-arity self-test: ok");
}

/// Record 0110: the wrapped receivers of a trait method proven from the
/// trait's impl records (`Supporting.impls`, recovered by trait identity in
/// the extractor). A concrete head resolves through its exact canonical
/// identity (`ChunkedArray<StringType>` -> the `StringChunked` wrapper); a
/// generic head is instantiated on each alias wrapper of its base and admitted
/// only when `applicability` proves the head and every impl bound and where
/// clause; a bare-parameter head (`impl<T: B> Tr for T`) is tried on every
/// wrapper under the same proof. Blanket impls and generic traits are not
/// receivers. The second vector names each candidate that failed.
pub(crate) fn proven_trait_receivers(world: &World, c: &Callable) -> (Vec<String>, Vec<String>) {
	let mut out: Vec<String> = Vec::new();
	let mut why: Vec<String> = Vec::new();
	let Some(tr) = world.types.get(&c.owner) else {
		return (out, why);
	};
	if tr.generic {
		why.push("generic trait".into());
		return (out, why);
	}
	let push = |w: &str, out: &mut Vec<String>| {
		if !out.iter().any(|x| x == w) {
			out.push(w.to_string())
		}
	};
	for ti in &tr.impls {
		if ti.blanket {
			continue;
		}
		let head = ty::parse(&ti.for_type).render();
		let (base, args) = split_head(&head);
		let generic_head = is_param(&head) || args.iter().any(|a| is_param(a));
		if !generic_head {
			if ti.bounds.iter().any(|(_, b)| !b.is_empty()) || !ti.where_predicates.is_empty() {
				why.push(format!(
					"{head}: a concrete impl with bounds is not decided here"
				));
				continue;
			}
			if let Some(w) = world.by_identity.get(&head) {
				push(w, &mut out);
			} else if world.wrappers.contains_key(&head) {
				push(&head, &mut out);
			} else {
				why.push(format!("{head}: not wrapped"));
			}
			continue;
		}
		let mut probe = c.clone();
		probe.impl_head = Some(head.clone());
		probe.impl_bounds = ti.bounds.clone();
		probe.impl_where = ti.where_predicates.clone();
		let mut cands: Vec<(&String, &Wrapper)> = world
			.wrappers
			.iter()
			.filter(|(_, w)| {
				if is_param(&head) {
					true
				} else {
					w.rule == "alias"
						&& w.base.as_deref() == Some(base.as_str())
						&& w.identity.starts_with(&format!("{base}<"))
				}
			})
			.collect();
		cands.sort_by(|a, b| a.0.cmp(b.0));
		let mut seen_identity: BTreeSet<&str> = BTreeSet::new();
		for (path, w) in cands {
			if !seen_identity.insert(w.identity.as_str()) {
				continue;
			}
			match world.applicability(&probe, &w.identity).0 {
				Applicability::Proven => push(path, &mut out),
				Applicability::Rejected(_) => {}
				Applicability::Unresolved(e) => why.push(format!("{head} on {}: {e}", w.identity)),
			}
		}
	}
	why.sort();
	why.dedup();
	(out, why)
}

#[cfg(test)]
mod tests {
	#[test]
	fn method_arity() {
		super::method_arity_self_test();
	}
	#[test]
	fn move_semantics() {
		super::move_semantics_self_test();
	}
	#[test]
	fn trait_receivers() {
		super::trait_receivers_self_test();
	}
}
