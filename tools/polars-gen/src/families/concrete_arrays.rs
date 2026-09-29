//! Record 0120: the concrete Arrow arrays a script can reach. A closed,
//! cited allowlist names exact owners and instantiations (the 0120
//! reachability probe's identities, per pin); each becomes a wrapper under
//! `polars::arrow`, reached from `polars::arrow::ArrayRef` by a checked
//! typed downcast. Every other type under a deferred prefix keeps its 0119
//! treatment.
use crate::text::sanitize;
use crate::world::{World, Wrapper};

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConcreteArray {
	/// The owner's canonical path.
	pub(crate) owner: String,
	/// The native bound to the owner's one type parameter (`i64`, or the
	/// offset type); absent for a non-generic owner (`StructArray`).
	#[serde(default)]
	pub(crate) native: Option<String>,
	/// The Rune name, under `polars::arrow`.
	pub(crate) name: String,
	/// The ArrayRef downcast method's Rune name (`as_int64_array`).
	pub(crate) downcast: String,
	/// The 0120 probe route that proves a script obtains this identity.
	pub(crate) route: String,
	pub(crate) cite: String,
}

/// The owners and natives the record may list: the probe's proven
/// identities, and nothing else (a new row needs a new probe and review).
pub(crate) const PROVEN: &[(&str, Option<&str>)] = &[
	("polars_arrow::array::primitive::PrimitiveArray", Some("i8")),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("i16"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("i32"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("i64"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("i128"),
	),
	("polars_arrow::array::primitive::PrimitiveArray", Some("u8")),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("u16"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("u32"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("u64"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("u128"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("polars_utils::float16::pf16"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("f32"),
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		Some("f64"),
	),
	("polars_arrow::array::utf8::Utf8Array", Some("i64")),
	("polars_arrow::array::binary::BinaryArray", Some("i64")),
	("polars_arrow::array::list::ListArray", Some("i64")),
	("polars_arrow::array::struct_::StructArray", None),
];

/// The identity a row names: `Owner<native>` or the owner itself.
pub(crate) fn identity(row: &ConcreteArray) -> String {
	match &row.native {
		Some(n) => format!("{}<{n}>", row.owner),
		None => row.owner.clone(),
	}
}

/// Whether a type path is an allowlisted owner or one of its listed
/// identities (its callables are then decided per instantiation, and a
/// listed identity may be a trait receiver, instead of deferred). An
/// unlisted instantiation (`Utf8Array<i32>`) stays deferred.
pub(crate) fn listed_owner(rows: &[ConcreteArray], path: &str) -> bool {
	rows.iter().any(|r| r.owner == path || identity(r) == path)
}

/// The rows against the proven set and the inventory, fail closed: each
/// cited, routed, unique, proven, under a deferred prefix, its owner in the
/// inventory with the right genericity, and its names unique.
pub(crate) fn validate(world: &World, rows: &[ConcreteArray]) -> Result<(), String> {
	let mut seen = std::collections::BTreeSet::new();
	let mut names = std::collections::BTreeSet::new();
	for r in rows {
		let id = identity(r);
		if r.cite.trim().is_empty() || r.route.trim().is_empty() {
			return Err(format!("{id}: no citation or probe route"));
		}
		if !seen.insert(id.clone()) {
			return Err(format!("{id}: listed twice"));
		}
		if !names.insert(r.name.clone()) || !names.insert(format!("fn {}", r.downcast)) {
			return Err(format!(
				"{id}: name `{}` or downcast `{}` reused",
				r.name, r.downcast
			));
		}
		if !PROVEN
			.iter()
			.any(|(o, n)| *o == r.owner && *n == r.native.as_deref())
		{
			return Err(format!("{id}: not an identity the 0120 probe proved"));
		}
		if !world
			.release
			.families
			.deferred_type_prefixes
			.iter()
			.any(|d| r.owner.starts_with(d.prefix.as_str()))
		{
			return Err(format!("{id}: not under a deferred prefix"));
		}
		let Some(o) = world.types.get(&r.owner) else {
			return Err(format!("{id}: owner not in the inventory"));
		};
		if o.generic != r.native.is_some() {
			return Err(format!(
				"{id}: the owner is {}generic",
				if o.generic { "" } else { "not " }
			));
		}
	}
	Ok(())
}

/// Record 0120: traits the inventory records under a private module, with
/// their public re-export (polars-arrow array/mod.rs, `pub use
/// static_array::{ParameterFreeDtypeStaticArray, StaticArray}`: line 734 at
/// da47b74, 732 at 0.55.2).
const TRAIT_SPELLINGS: &[(&str, &str)] = &[
	(
		"polars_arrow::array::static_array::ParameterFreeDtypeStaticArray",
		"polars_arrow::array::ParameterFreeDtypeStaticArray",
	),
	(
		"polars_arrow::array::static_array::StaticArray",
		"polars_arrow::array::StaticArray",
	),
];

/// The public spelling of a trait the listed owners reach, when the
/// inventory's spelling goes through a private module.
pub(crate) fn trait_spelling(spelled: &str) -> Option<&'static str> {
	TRAIT_SPELLINGS
		.iter()
		.find(|(private, _)| *private == spelled)
		.map(|(_, public)| *public)
}

/// The spelling of a native in generated Rust.
fn spell_native(n: &str) -> String {
	n.to_string()
}

/// One wrapper per listed identity, under `polars::arrow`.
pub(crate) fn wrappers(world: &World, rows: &[ConcreteArray]) -> Vec<(String, Wrapper)> {
	rows.iter()
		.map(|r| {
			let id = identity(r);
			let owner_spell = format!("polars_arrow::array::{}", crate::ty::last(&r.owner));
			let spell = match &r.native {
				Some(n) => format!("{owner_spell}<{}>", spell_native(n)),
				None => owner_spell,
			};
			let _ = world;
			(
				id.clone(),
				Wrapper {
					rust: format!("W_{}", sanitize(&id)),
					spell,
					rune_item: "::polars::arrow".into(),
					rune_name: r.name.clone(),
					hand: false,
					identity: id.clone(),
					rule: "alias",
					aliases: vec![id],
					base: r.native.as_ref().map(|_| r.owner.clone()),
				},
			)
		})
		.collect()
}

/// The owner's own type-parameter bound, which every listed instantiation
/// satisfies by construction: the struct cannot be named otherwise, and the
/// probe's Rust side names each one. These are the only trait facts the
/// record adds, and only for the listed natives.
const OWNER_BOUNDS: &[(&str, &str)] = &[
	(
		"polars_arrow::array::primitive::PrimitiveArray",
		"polars_arrow::types::native::NativeType",
	),
	(
		"polars_arrow::array::utf8::Utf8Array",
		"polars_arrow::types::offset::Offset",
	),
	(
		"polars_arrow::array::binary::BinaryArray",
		"polars_arrow::types::offset::Offset",
	),
	(
		"polars_arrow::array::list::ListArray",
		"polars_arrow::types::offset::Offset",
	),
];

/// The (native, bound) facts the listed rows establish.
pub(crate) fn facts(rows: &[ConcreteArray]) -> Vec<(String, String)> {
	rows.iter()
		.filter_map(|r| {
			let n = r.native.as_ref()?;
			let (_, b) = OWNER_BOUNDS.iter().find(|(o, _)| *o == r.owner)?;
			Some((n.clone(), b.to_string()))
		})
		.collect()
}

/// One compile-time assertion per fact, in generated Rust: the listed
/// native satisfies the owner's bound at this pin.
pub(crate) fn assertions(facts: &std::collections::BTreeSet<(String, String)>) -> String {
	let mut s = String::from(
		"\n// record 0120: every listed concrete array's parameter bound, asserted at compile time\n",
	);
	for (t, tr) in facts {
		let tr_spell = match crate::ty::last(tr) {
			"NativeType" => "polars_arrow::types::NativeType",
			"Offset" => "polars_arrow::types::Offset",
			_ => unreachable!("an unlisted owner bound"),
		};
		s.push_str(&format!(
			"const _: () = {{ fn holds<T: {tr_spell}>() {{}} let _ = holds::<{t}>; }};\n"
		));
	}
	s
}

/// Record 0120: the rules every method on a listed concrete array meets,
/// by rule and not by row: no mutable receiver, no unsafe or unchecked
/// method, and every `usize` argument of a method with a receiver named by
/// a required receiver guard (an index the call may panic on).
pub(crate) fn admit(world: &World, c: &crate::model::Callable) -> Result<(), String> {
	if c.receiver.contains("&mut") {
		return Err("a mutable receiver: read-side only".into());
	}
	if c.is_unsafe || c.name.ends_with("_unchecked") {
		return Err("an unsafe or unchecked method".into());
	}
	// the legacy array module (its builders and collectors) is deferred to
	// record 0121 with the mutable side, whatever receiver it reaches
	if c.canonical_path.starts_with("polars_arrow::legacy::") {
		return Err("the legacy array module is deferred to record 0121".into());
	}
	// review of 0120: a dtype argument drives construction or validation
	// (`new_null`, `new_empty`, `to`, `get_child_type` panic on a dtype of
	// another physical type), and a static method's length allocates before
	// any bound: both are the write side, deferred to record 0121
	if let Some(q) = c.params.iter().find(|q| {
		q.ty_canonical
			.contains("polars_arrow::datatypes::ArrowDataType")
	}) {
		return Err(format!(
			"a dtype argument `{}`: construction and dtype validation are deferred to record 0121",
			q.name
		));
	}
	if c.receiver == "none" {
		if let Some(q) = c.params.iter().find(|q| q.ty_canonical == "usize") {
			return Err(format!(
				"a static length `{}` allocates before any bound: deferred to record 0121",
				q.name
			));
		}
		return Ok(());
	}
	let guard = world
		.release
		.families
		.receiver_guards
		.iter()
		.find(|g| g.path == c.canonical_path);
	for q in c.params.iter().filter(|q| q.ty_canonical == "usize") {
		let named = guard
			.is_some_and(|g| g.param == q.name || g.param2.as_deref() == Some(q.name.as_str()));
		if !named {
			return Err(format!(
				"index argument `{}` without a required receiver guard",
				q.name
			));
		}
	}
	Ok(())
}

/// Record 0120: the way in. One checked typed downcast per listed identity,
/// an instance function of `polars::arrow::ArrayRef`: the borrowed array is
/// downcast to exactly that type (`as_any`), cloned into its wrapper, and a
/// mismatch is a `ConversionError` naming the actual Arrow dtype.
pub(crate) fn emit_downcasts(world: &World, out: &mut crate::emit::Emitted) {
	use std::fmt::Write as _;
	for r in &world.release.families.concrete_arrays {
		let id = identity(r);
		let Some(w) = world.wrappers.get(&id) else {
			continue;
		};
		let ident = format!("ca_{}", r.downcast);
		let rune = format!("polars::arrow::ArrayRef::{}", r.downcast);
		let summary = format!("{}() -> {} (fallible)", r.downcast, r.name);
		writeln!(
			out.functions,
			"/// Record 0120: a checked downcast to `{}` (probe route: {}).\n#[rune::function(instance, path = {})]\nfn {ident}(this: &ArrayRef) -> Result<{}, Error> {{ this.0.as_any().downcast_ref::<{}>().map(|a| {}(a.clone())).ok_or_else(|| Error::conversion(&format!(\"{}: the array is {{:?}}, not {}\", this.0.dtype()))) }}",
			w.spell, r.route, r.downcast, w.rust, w.spell, w.rust, r.downcast, r.name
		)
		.unwrap();
		out.registrations
			.push(format!("m.function_meta({ident})?;"));
		out.catalogue.push((
			rune,
			format!(
				"{summary}: the array as a {}, or a ConversionError naming its Arrow dtype",
				r.name
			),
		));
	}
}

/// The surface's reachability rows: each listed identity, its wrapper, its
/// downcast and the probe route that proves a script obtains it.
pub(crate) fn surface_rows(world: &World) -> serde_json::Value {
	serde_json::Value::Array(
		world
			.release
			.families
			.concrete_arrays
			.iter()
			.map(|r| {
				serde_json::json!({
					"identity": identity(r),
					"rune": format!("polars::arrow::{}", r.name),
					"downcast": format!("polars::arrow::ArrayRef::{}", r.downcast),
					"route": r.route,
				})
			})
			.collect(),
	)
}

/// Record 0120: the oracle fixture of each listed identity: (fixture name,
/// Rust value expression, Rust type). Each is a Series chunk taken through
/// the bridge and downcast, as a script obtains it (the large-offset string
/// and binary arrays at `CompatLevel::oldest`, as the probe proved).
pub(crate) fn fixtures(world: &World) -> Vec<(String, String, String, String)> {
	let i64s = "p::Series::new(\"x\".into(), [Some(1i64), None, Some(3)])";
	let strs = "p::Series::new(\"x\".into(), [Some(\"a\"), None, Some(\"ccc\")])";
	world
		.release
		.families
		.concrete_arrays
		.iter()
		.filter_map(|r| {
			let id = identity(r);
			let w = world.wrappers.get(&id)?;
			let name = r.downcast.trim_start_matches("as_").to_string();
			let (series, level) = match (crate::ty::last(&r.owner), r.native.as_deref()) {
				("PrimitiveArray", Some(n)) => {
					let dt = match n {
						"i8" => "Int8",
						"i16" => "Int16",
						"i32" => "Int32",
						"i64" => "Int64",
						"i128" => "Int128",
						"u8" => "UInt8",
						"u16" => "UInt16",
						"u32" => "UInt32",
						"u64" => "UInt64",
						"u128" => "UInt128",
						"f32" => "Float32",
						"f64" => "Float64",
						_ => "Float16",
					};
					(format!("{i64s}.cast(&p::DataType::{dt}).unwrap()"), "newest")
				}
				("Utf8Array", _) => (strs.to_string(), "oldest"),
				("BinaryArray", _) => (
					format!("{strs}.cast(&p::DataType::Binary).unwrap()"),
					"oldest",
				),
				("ListArray", _) => (
					format!("p::IntoSeries::into_series({i64s}.implode().unwrap())"),
					"newest",
				),
				_ => (
					format!("p::IntoSeries::into_series(p::StructChunked::from_columns(\"s\".into(), 3, &[p::Column::from({i64s}), p::Column::from({strs}.with_name(\"y\".into()))]).unwrap())"),
					"newest",
				),
			};
			let expr = format!(
				"{series}.to_arrow(0, p::CompatLevel::{level}()).as_any().downcast_ref::<{}>().expect(\"oracle: the probe's concrete array\").clone()",
				w.spell
			);
			Some((id, name, expr, w.spell.clone()))
		})
		.collect()
}

/// How the oracle shows a concrete array: as the Series it converts back to.
pub(crate) const SHOW: &str = "crate_oracle::series_repr(&p::Series::from_arrow(\"\".into(), v.clone().boxed()).expect(\"oracle: a concrete array converts back\"))";

/// Record 0120: the allowlist refuses what it must, the admission rules
/// refuse by rule, and the listed identities become `polars::arrow`
/// wrappers with their owner bound asserted.
pub(crate) fn concrete_arrays_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{
		DeferredPrefix, FamilyTables, InstantiationScope, Release, ReleaseProvenance,
	};
	let sup = |path: &str, generic: bool| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".to_string(),
		canonical_path: path.to_string(),
		found_paths: vec![path.to_string()],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic,
		lifetime: false,
		hidden: false,
		derived: vec![],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let prim = "polars_arrow::array::primitive::PrimitiveArray";
	let utf8 = "polars_arrow::array::utf8::Utf8Array";
	let st = "polars_arrow::array::struct_::StructArray";
	let inv = Inventory {
		callables: vec![],
		provenance: None,
		supporting: vec![sup(prim, true), sup(utf8, true), sup(st, false)],
	};
	let row = |owner: &str, native: Option<&str>, name: &str| ConcreteArray {
		owner: owner.into(),
		native: native.map(String::from),
		name: name.into(),
		downcast: format!("as_{}", name.to_lowercase()),
		route: "r".into(),
		cite: "c".into(),
	};
	let release = |rows: Vec<ConcreteArray>| {
		let mut families = FamilyTables::default();
		families.namespaced_crates = vec!["polars_arrow".into()];
		families.deferred_type_prefixes = vec![DeferredPrefix {
			prefix: "polars_arrow::array::".into(),
			cite: "c".into(),
		}];
		families.concrete_arrays = rows;
		Release {
			name: "t".into(),
			source: "t".into(),
			provenance: ReleaseProvenance::default(),
			instantiation: InstantiationScope::default(),
			api_crates: vec!["polars_arrow".into()],
			unordered: vec![],
			excluded_oracle: vec![],
			refused: vec![],
			families,
		}
	};
	let good = vec![
		row(prim, Some("i64"), "Int64Array"),
		row(utf8, Some("i64"), "LargeStringArray"),
		row(st, None, "StructArray"),
	];
	let w = World::new(&inv, &release(good.clone()), &["mechanical"]);
	assert!(validate(&w, &good).is_ok());
	// each listed identity is a wrapper under polars::arrow, its owner bound a fact
	let id = format!("{prim}<i64>");
	assert_eq!(w.wrappers[&id].rune_item, "::polars::arrow");
	assert_eq!(w.wrappers[&id].rune_name, "Int64Array");
	assert!(w.array_facts.contains(&(
		"i64".to_string(),
		"polars_arrow::types::native::NativeType".to_string()
	)));
	assert!(w.array_facts.contains(&(
		"i64".to_string(),
		"polars_arrow::types::offset::Offset".to_string()
	)));
	assert!(
		!w.release.deferred_type(prim)
			&& w.release
				.deferred_type("polars_arrow::array::binary::BinaryArray")
	);
	// a listed identity may be a trait receiver; an unlisted instantiation stays deferred
	assert!(!w.release.deferred_type(&id) && w.release.deferred_type(&format!("{utf8}<i32>")));
	let asserted = assertions(&w.array_facts);
	assert!(
		asserted.contains("holds::<i64>") && asserted.contains("polars_arrow::types::NativeType"),
		"{asserted}"
	);
	// the refusals, each by name
	let refused = |rows: Vec<ConcreteArray>, want: &str| {
		let e = validate(&w, &rows).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	refused(
		vec![ConcreteArray {
			cite: " ".into(),
			..row(prim, Some("i64"), "A")
		}],
		"no citation",
	);
	refused(
		vec![ConcreteArray {
			route: "".into(),
			..row(prim, Some("i64"), "A")
		}],
		"probe route",
	);
	refused(
		vec![row(prim, Some("i64"), "A"), row(prim, Some("i64"), "B")],
		"listed twice",
	);
	refused(
		vec![row(prim, Some("i64"), "A"), row(prim, Some("i32"), "A")],
		"reused",
	);
	refused(
		vec![row(utf8, Some("i32"), "A")],
		"not an identity the 0120 probe proved",
	);
	refused(
		vec![row(prim, Some("bool"), "A")],
		"not an identity the 0120 probe proved",
	);
	refused(
		vec![row(st, Some("i64"), "A")],
		"not an identity the 0120 probe proved",
	);
	refused(
		vec![row(
			"polars_arrow::array::list::ListArray",
			Some("i64"),
			"A",
		)],
		"owner not in the inventory",
	);
	let mut undeferred = release(vec![]);
	undeferred.families.deferred_type_prefixes.clear();
	let wu = World::new(&inv, &undeferred, &["mechanical"]);
	let e = validate(&wu, &[row(prim, Some("i64"), "A")]).unwrap_err();
	assert!(e.contains("not under a deferred prefix"), "{e}");
	// the admission rules on a listed owner's methods
	let callable = |name: &str,
	                receiver: &str,
	                params: &[(&str, &str)]|
	 -> crate::model::Callable {
		serde_json::from_value(serde_json::json!({
			"key": "k", "kind": "inherent", "krate": "polars_arrow", "owner": prim, "name": name,
			"canonical_path": format!("{prim}::{name}"), "found_paths": [], "crate_paths": [], "receiver": receiver,
			"params": params.iter().map(|(n, t)| serde_json::json!({"name": n, "ty": t, "ty_canonical": t})).collect::<Vec<_>>(),
			"ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
			"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
			"owner_generic": true, "is_unsafe": false, "is_async": false, "deprecated": false,
			"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
			"bucket": "mechanical", "rules": []
		}))
		.unwrap()
	};
	assert!(
		admit(&w, &callable("slice", "&mut self", &[]))
			.unwrap_err()
			.contains("mutable receiver")
	);
	assert!(
		admit(&w, &callable("value_unchecked", "&self", &[]))
			.unwrap_err()
			.contains("unchecked")
	);
	assert!(
		admit(&w, &callable("value", "&self", &[("i", "usize")]))
			.unwrap_err()
			.contains("without a required receiver guard")
	);
	assert!(
		admit(&w, &callable("new_null", "none", &[("length", "usize")]))
			.unwrap_err()
			.contains("static length")
	);
	assert!(
		admit(
			&w,
			&callable(
				"new_empty",
				"none",
				&[("dtype", "polars_arrow::datatypes::ArrowDataType")]
			)
		)
		.unwrap_err()
		.contains("dtype argument")
	);
	assert!(
		admit(
			&w,
			&callable(
				"to",
				"self",
				&[("dtype", "polars_arrow::datatypes::ArrowDataType")]
			)
		)
		.unwrap_err()
		.contains("dtype argument")
	);
	assert!(
		admit(
			&w,
			&callable("from_vec", "none", &[("values", "alloc::vec::Vec<T>")])
		)
		.is_ok()
	);
	assert!(admit(&w, &callable("len", "&self", &[])).is_ok());
	let mut legacy = callable("from_values_iter", "none", &[]);
	legacy.canonical_path =
		"polars_arrow::legacy::array::utf8::Utf8FromIter::from_values_iter".into();
	assert!(admit(&w, &legacy).unwrap_err().contains("record 0121"));
	let mut guarded = release(good.clone());
	guarded.families.receiver_guards = vec![crate::families::receiver_guards::ReceiverGuard {
		path: format!("{prim}::value"),
		check: "below_len".into(),
		param: "i".into(),
		param2: None,
		cite: "c".into(),
	}];
	let inv_g = Inventory {
		callables: vec![callable("value", "&self", &[("i", "usize")])],
		provenance: None,
		supporting: inv.supporting.clone(),
	};
	let wg = World::new(&inv_g, &guarded, &["mechanical"]);
	assert!(admit(&wg, &callable("value", "&self", &[("i", "usize")])).is_ok());
	// a guard naming another parameter does not cover this one
	assert!(admit(&wg, &callable("value", "&self", &[("j", "usize")])).is_err());
	// the scalar rules: binary16 is an exact float on a listed identity only,
	// and refused as an argument there
	let f16 = crate::ty::Ty::Path {
		path: "polars_utils::float16::pf16".into(),
		args: vec![],
	};
	let r = w.ret(&f16, Some(&id), 0).ok().unwrap();
	assert_eq!(
		(r.rust_ty.as_str(), r.conv.as_str()),
		("f64", "f64::from(__r)")
	);
	assert!(
		w.ret(&f16, Some("polars_core::series::Series"), 0)
			.map_or(true, |r| r.rust_ty != "f64")
	);
	let none = std::collections::BTreeMap::new();
	assert!(w.arg(&f16, "v", &none, Some(&id), 0).is_err());
	println!("concrete arrays self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn concrete_arrays() {
		super::concrete_arrays_self_test();
	}
}
