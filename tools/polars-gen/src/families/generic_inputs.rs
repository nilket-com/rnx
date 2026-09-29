use crate::emit::Emitted;
use crate::emit::callable::emit_callable;
use crate::families::callbacks::CallbackInvocation;
use crate::families::{ArgSite, Family, Listed, RetSite, State};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{
	FamilyTables, InstantiationScope, RefusedOperation, Release, ReleaseProvenance,
};
use crate::ty::Ty;
use crate::ty::last;
use crate::world::World;
use crate::world::mapping::ok_arg;
use crate::world::mapping::{Arg, Ret, Unsupported};
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Clone)]
pub(crate) struct BitmapInput {
	pub(crate) path: String,
	/// `receiver` (the receiver's total length), `values` (the length of
	/// the parameter named `values`) or `none`.
	pub(crate) length: String,
	pub(crate) cite: String,
}

// ---------------------------------------------------------------- record 0115: this module's families

/// Record 0085: a listed validity bitmap return, copied into bools.
pub(crate) struct BitmapReturnFamily;
pub(crate) static BITMAP_RETURN: BitmapReturnFamily = BitmapReturnFamily;

impl Family for BitmapReturnFamily {
	fn name(&self) -> &'static str {
		"bitmap_return"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		// record 0085: a listed path's validity bitmap return
		if world
			.release
			.families
			.bitmap_returns
			.iter()
			.any(|p| *p == c.canonical_path)
		{
			Listed::Active(State::On)
		} else {
			Listed::Unlisted
		}
	}
	fn ret(
		&self,
		_world: &World,
		_state: &State,
		site: RetSite,
		t: &Ty,
		_owner: Option<&str>,
		_depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Unwrapped {
			return Ok(None);
		}
		// record 0085: a validity bitmap, only for the release's listed paths
		let Ty::Path { path, args } = t else {
			return Ok(None);
		};
		if path == "polars_arrow::bitmap::immutable::Bitmap" && args.is_empty() {
			let limit = 1usize << 20;
			return Ok(Some(Ret {
				materialize: None,
				rust_ty: "Vec<bool>".into(),
				fallible: true,
				conv: "support::copy_bits(&__r, \"__OP__\")?".into(),
				doc: format!(
					"vector of bool (validity bits copied from a bitmap, at most {limit} bits per call)"
				),
			}));
		}
		Ok(None)
	}
}

/// Record 0086: a listed bitmap parameter, built from script bools.
pub(crate) struct BitmapInputFamily;
pub(crate) static BITMAP_INPUT: BitmapInputFamily = BitmapInputFamily;

impl Family for BitmapInputFamily {
	fn name(&self) -> &'static str {
		"bitmap_input"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		// record 0086: a listed path's bitmap parameter and its length
		match world
			.release
			.families
			.bitmap_inputs
			.iter()
			.find(|b| b.path == c.canonical_path)
		{
			Some(b) => Listed::Active(State::Two(
				format!("{}::{}", last(&c.owner), c.name),
				b.length.clone(),
			)),
			None => Listed::Unlisted,
		}
	}
	fn arg(
		&self,
		_world: &World,
		state: &State,
		site: ArgSite,
		t: &Ty,
		name: &str,
		owner: Option<&str>,
	) -> Result<Option<Arg>, Unsupported> {
		if site != ArgSite::Top {
			return Ok(None);
		}
		// record 0086: a listed callable's validity bitmap from a script Vec<bool>
		if let Ty::Path { path, args } = t {
			if path == "polars_arrow::bitmap::immutable::Bitmap" && args.is_empty() {
				if let Some((op, length)) = state.two() {
					let (pre, expect, shape) = match length.as_str() {
						// the oracle's receiver fixtures have 3 rows, the List one 2
						"receiver" => (
							Some("let __mask_len = this.0.len();".to_string()),
							"Some(__mask_len)",
							if owner.is_some_and(|o| o.ends_with("::ListChunked")) {
								"mask2"
							} else {
								"mask3"
							},
						),
						"values" => (
							Some(
								"let __mask_len = support::vec_len(&values, \"values\")?;"
									.to_string(),
							),
							"Some(__mask_len)",
							"mask1",
						),
						_ => (None, "None", "mask3"),
					};
					let mut a = ok_arg(
						"rune::Value",
						format!("support::bitmap_from_bools(&{name}, \"{op}\", {expect})?"),
						"vector of bool (a validity mask, copied)",
					)?;
					a.pre.extend(pre);
					a.shape = shape.into();
					return Ok(Some(a));
				}
			}
		}
		Ok(None)
	}
}

/// Record 0086 gate 2 controls, from a synthetic inventory: a listed
/// callable's `Bitmap` parameter is built from a script `Vec<bool>` with
/// its length rule (receiver, values, none), directly or optionally; an
/// unlisted bitmap input, an unlisted bitmap output and an Arrow array stay
/// refused, and the scope is clear after emission.
pub(crate) fn bitmap_input_self_test() {
	let series = "polars_core::series::Series";
	let bitmap = "polars_arrow::bitmap::immutable::Bitmap";
	let sup = |path: &str| Supporting {
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
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let mk =
		|key: &str, name: &str, receiver: &str, params: Vec<(&str, &str)>, ret: Option<&str>| {
			Callable {
				key: key.into(),
				kind: "inherent".into(),
				krate: "polars_core".into(),
				owner: series.into(),
				name: name.into(),
				canonical_path: format!("{series}::{name}"),
				found_paths: vec![],
				crate_paths: vec![],
				receiver: receiver.into(),
				params: params
					.iter()
					.map(|(n, t)| Param {
						name: n.to_string(),
						ty: t.to_string(),
						ty_canonical: t.to_string(),
					})
					.collect(),
				ret: None,
				ret_canonical: ret.map(String::from),
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
			}
		};
	let opt = format!("core::option::Option<{bitmap}>");
	let inv = Inventory {
		callables: vec![
			mk(
				"set",
				"set_mask",
				"&mut self",
				vec![("validity", &opt)],
				None,
			),
			mk(
				"values",
				"from_values_mask",
				"none",
				vec![
					("name", "polars_utils::pl_str::PlSmallStr"),
					("values", "alloc::vec::Vec<i64>"),
					("buffer", &opt),
				],
				Some(series),
			),
			mk(
				"bits",
				"from_bits",
				"none",
				vec![
					("name", "polars_utils::pl_str::PlSmallStr"),
					("bitmap", bitmap),
				],
				Some(series),
			),
			mk(
				"unlisted_in",
				"other_mask",
				"&mut self",
				vec![("validity", &opt)],
				None,
			),
			mk("unlisted_out", "mask_out", "&self", vec![], Some(&opt)),
			mk(
				"array",
				"with_chunk",
				"&self",
				vec![("arr", "polars_arrow::array::PrimitiveArray<i64>")],
				Some(series),
			),
		],
		supporting: vec![sup(series)],
		provenance: None,
	};
	let mut release = Release {
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
	for (n, l) in [
		("set_mask", "receiver"),
		("from_values_mask", "values"),
		("from_bits", "none"),
	] {
		release.families.bitmap_inputs.push(BitmapInput {
			path: format!("{series}::{n}"),
			length: l.into(),
			cite: "t".into(),
		});
	}
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let emit = |key: &str| {
		let mut e = empty();
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
	let (status, reason, f) = emit("set");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("let __mask_len = this.0.len();")
			&& f.contains("None => None")
			&& f.contains(
				"support::bitmap_from_bools(&v, \"Series::set_mask\", Some(__mask_len))?"
			),
		"an optional receiver-length mask; None stays None: {f}"
	);
	let (status, reason, f) = emit("values");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("let __mask_len = support::vec_len(&values, \"values\")?;"),
		"a values-length mask: {f}"
	);
	let (status, reason, f) = emit("bits");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("support::bitmap_from_bools(&bitmap, \"Series::from_bits\", None)?"),
		"no length rule: {f}"
	);
	for key in ["unlisted_in", "unlisted_out", "array"] {
		let (status, reason, _) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must stay refused, got {reason}"
		);
	}
	assert!(
		!world.active.is_set("bitmap_input") && !world.active.is_set("bitmap_return"),
		"the bitmap scope never outlives its callable"
	);
	println!("bitmap-input self-test: ok");
}

/// Record 0085 gate 2 controls, from a synthetic inventory: a validity
/// `Bitmap` return maps to an owned `Vec<bool>` only for the paths the
/// release lists, directly, optionally and as iterator items (one bound
/// for the whole call); an unlisted path, a bitmap input and an Arrow
/// array stay refused.
pub(crate) fn bitmap_self_test() {
	let series = "polars_core::series::Series";
	let bitmap = "polars_arrow::bitmap::immutable::Bitmap";
	let sup = |path: &str| Supporting {
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
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let mk = |key: &str, name: &str, params: Vec<(&str, &str)>, ret: Option<&str>| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: series.into(),
		name: name.into(),
		canonical_path: format!("{series}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: ret.map(String::from),
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
	let opt = format!("core::option::Option<{bitmap}>");
	let items = format!(
		"impl core::iter::traits::exact_size::ExactSizeIterator<Item = core::option::Option<&{bitmap}>>"
	);
	let inv = Inventory {
		callables: vec![
			mk("direct", "bits", vec![], Some(bitmap)),
			mk("optional", "maybe_bits", vec![], Some(&opt)),
			mk("items", "bits_per_chunk", vec![], Some(&items)),
			mk("unlisted", "other_bits", vec![], Some(&opt)),
			mk(
				"input",
				"with_bits",
				vec![("validity", bitmap)],
				Some(series),
			),
			mk(
				"array",
				"chunk",
				vec![],
				Some("polars_arrow::array::PrimitiveArray<i64>"),
			),
		],
		supporting: vec![sup(series)],
		provenance: None,
	};
	let mut release = Release {
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
	for n in ["bits", "maybe_bits", "bits_per_chunk", "with_bits"] {
		release
			.families
			.bitmap_returns
			.push(format!("{series}::{n}"));
	}
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let emit = |key: &str| {
		let mut e = empty();
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
	let (status, reason, f) = emit("direct");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("-> Result<Vec<bool>, Error>")
			&& f.contains("support::copy_bits(&__r, \"bits\")?"),
		"{f}"
	);
	let (status, reason, f) = emit("optional");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("Result<Option<Vec<bool>>, Error>") && f.contains("None => None"),
		"None stays None, never an empty vector: {f}"
	);
	let (status, reason, f) = emit("items");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("support::SliceBudget::enter()")
			&& f.contains("support::materialize_exact")
			&& f.contains("support::copy_bits("),
		"one cumulative bound over every chunk's bitmap: {f}"
	);
	for (key, why) in [
		("unlisted", "unreachable polars type"),
		("input", "unreachable polars type"),
		("array", ""),
	] {
		let (status, reason, _) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must stay refused, got {reason}"
		);
		assert!(
			reason.contains(why),
			"{key}: `{why}` expected, got {reason}"
		);
	}
	assert!(
		!world.active.is_set("bitmap_return"),
		"the listed-path flag never outlives its callable"
	);
	println!("bitmap self-test: ok");
}

/// Record 0084 gate 2 controls, from a synthetic inventory: chained generic
/// inference (`I: IntoIterator<Item = S>, S: AsRef<str>`; `E: AsRef<[IE]>,
/// IE: Into<Expr>`), iterator inputs lowered from a script vector (owned
/// items through `into_iter`, borrowed `&str`/`&[u8]`/`Option` items through
/// a held temporary), and the refusals that stay: a tuple-with-`Field`
/// item, a return-only generic, a closure bound, and a release-refused path.
pub(crate) fn generic_input_self_test() {
	fn sup(path: &str) -> Supporting {
		Supporting {
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
			derived: vec!["Clone".into(), "Debug".into()],
			alias_target: None,
			implementors: vec![],
			impls: vec![],
		}
	}
	let frame = "polars_core::frame::dataframe::DataFrame";
	let expr = "polars_plan::dsl::expr::Expr";
	let field = "polars_core::datatypes::field::Field";
	let mk = |key: &str,
	          name: &str,
	          params: Vec<(&str, &str)>,
	          generics: Vec<(&str, &str)>,
	          ret: Option<&str>| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: frame.into(),
		name: name.into(),
		canonical_path: format!("{frame}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: ret.map(String::from),
		generics_canonical: generics
			.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
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
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let into_iter =
		|item: &str| format!("core::iter::traits::collect::IntoIterator<Item = {item}>");
	let iter = |item: &str| format!("core::iter::traits::iterator::Iterator<Item = {item}>");
	let inv = Inventory {
		callables: vec![
			mk(
				"chain",
				"pick",
				vec![("names", "I")],
				vec![("I", &into_iter("S")), ("S", "core::convert::AsRef<str>")],
				Some(frame),
			),
			mk(
				"chain_small",
				"drop_some",
				vec![("names", "I")],
				vec![
					("I", &into_iter("S")),
					("S", "core::convert::Into<polars_utils::pl_str::PlSmallStr>"),
				],
				Some(frame),
			),
			mk(
				"two_chains",
				"rename_some",
				vec![("existing", "I"), ("new", "J")],
				vec![
					("I", &into_iter("T")),
					("J", &into_iter("S")),
					("T", "core::convert::AsRef<str>"),
					("S", "core::convert::AsRef<str>"),
				],
				Some(frame),
			),
			mk(
				"exprs",
				"over_some",
				vec![("partition_by", "E")],
				vec![
					("E", "core::convert::AsRef<[IE]>"),
					(
						"IE",
						&format!("core::convert::Into<{expr}> + core::clone::Clone"),
					),
				],
				Some(frame),
			),
			mk(
				"opt_exprs",
				"over_opt",
				vec![("partition_by", "core::option::Option<E>")],
				vec![
					("E", "core::convert::AsRef<[IE]>"),
					(
						"IE",
						&format!("core::convert::Into<{expr}> + core::clone::Clone"),
					),
				],
				Some(frame),
			),
			mk(
				"owned_iter",
				"take_ids",
				vec![("ids", "I")],
				vec![("I", &iter("usize"))],
				Some(frame),
			),
			mk(
				"trusted",
				"take_flags",
				vec![("flags", "I")],
				vec![(
					"I",
					&format!(
						"{} + polars_arrow::trusted_len::TrustedLen",
						iter("core::option::Option<bool>")
					),
				)],
				Some(frame),
			),
			mk(
				"strs",
				"take_strs",
				vec![("iter", "I")],
				vec![("I", &iter("&str"))],
				Some(frame),
			),
			mk(
				"bytes",
				"take_bytes",
				vec![("iter", "I")],
				vec![("I", &iter("&[u8]"))],
				Some(frame),
			),
			mk(
				"opt_strs",
				"take_opt_strs",
				vec![("iter", "I")],
				vec![(
					"I",
					&format!(
						"{} + polars_arrow::trusted_len::TrustedLen",
						iter("core::option::Option<&str>")
					),
				)],
				Some(frame),
			),
			mk(
				"tuple_field",
				"with_fields",
				vec![("iter", "I")],
				vec![
					("I", &into_iter("F")),
					(
						"F",
						&format!(
							"core::convert::Into<(polars_utils::pl_str::PlSmallStr, {field})>"
						),
					),
				],
				Some(frame),
			),
			mk(
				"ret_only",
				"total",
				vec![],
				vec![("T", "num_traits::cast::NumCast")],
				Some("core::option::Option<T>"),
			),
			mk(
				"closure",
				"each",
				vec![("f", "F")],
				vec![("F", "core::ops::function::FnMut(i64) -> i64")],
				None,
			),
			mk(
				"policy",
				"refused_by_release",
				vec![("ids", "I")],
				vec![("I", &iter("usize"))],
				Some(frame),
			),
		],
		supporting: vec![sup(frame), sup(expr), sup(field)],
		provenance: None,
	};
	let mut release = Release {
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
	release.refused.push(RefusedOperation {
		path: format!("{frame}::refused_by_release"),
		reason: "validated first".into(),
		cite: "t".into(),
	});
	let world = World::new(&inv, &release, &["mechanical", "generic_fn"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let emit = |key: &str| {
		let mut e = empty();
		emit_callable(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical", "generic_fn"],
		);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, must) in [
		(
			"chain",
			vec![
				"let v: String = support::borrow_element",
				"collect::<Result<Vec<_>, Error>>()?",
			],
		),
		(
			"chain_small",
			vec!["let v: String = support::borrow_element"],
		),
		(
			"two_chains",
			vec![
				"borrow_vec(&existing, \"existing\")",
				"borrow_vec(&new, \"new\")",
			],
		),
		("exprs", vec!["support::take::<Expr>(&v, \"v\")?.0"]),
		(
			"opt_exprs",
			vec!["Some(v) => Some(", "support::take::<Expr>(&v, \"v\")?.0"],
		),
		(
			"owned_iter",
			vec![").into_iter()", "let v: i64 = support::borrow_element"],
		),
		(
			"trusted",
			vec![
				").into_iter()",
				"let v: Option<bool> = support::borrow_element",
			],
		),
		(
			"strs",
			vec![
				"let __hold_iter = ",
				"__hold_iter.iter().map(String::as_str)",
			],
		),
		(
			"bytes",
			vec![
				"let __hold_iter = ",
				"__hold_iter.iter().map(Vec::as_slice)",
				"support::narrow::<u8>",
			],
		),
		(
			"opt_strs",
			vec![
				"let __hold_iter = ",
				"__hold_iter.iter().map(Option::as_deref)",
			],
		),
	] {
		let (status, reason, functions) = emit(key);
		assert_eq!(status, "generated", "{key}: {reason}");
		for m in must {
			assert!(
				functions.contains(m),
				"{key}: expected `{m}` in:\n{functions}"
			);
		}
		assert!(
			!functions.contains("as_str()?"),
			"{key}: no vector of borrowed strings"
		);
	}
	let (_, _, functions) = emit("chain");
	assert!(
		!functions.contains("Vec<&str>") && !functions.contains("v.as_str()"),
		"the chained item is an owned String, never a borrowed vector element:\n{functions}"
	);
	for (key, why) in [
		("tuple_field", "tuple conversion"),
		("ret_only", "generic parameter not inferable"),
		("closure", "callback"),
		("policy", "release policy"),
	] {
		let (status, reason, _) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must stay refused, got {reason}"
		);
		assert!(
			reason.contains(why),
			"{key}: refusal must say `{why}`, got {reason}"
		);
	}
	let plain = World::new(&inv, &release, &["mechanical"]);
	let mut e = empty();
	emit_callable(
		&plain,
		&mut e,
		inv.callables.iter().find(|c| c.key == "chain").unwrap(),
		&["mechanical"],
	);
	assert_eq!(
		e.entries[0].status, "unsupported",
		"without the generic_fn token the generic bucket stays closed"
	);
	println!("generic-input self-test: ok");
}

/// Record 0082 gate 2 controls, from a synthetic inventory: immutable
/// borrowed slices are copied into bounded owned vectors as returns,
/// as iterator items and as callback inputs; a mutable slice, an Arrow
/// element and a slice of iterators stay refused.
pub(crate) fn slice_self_test() {
	fn sup(path: &str) -> Supporting {
		Supporting {
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
			derived: vec!["Clone".into(), "Debug".into()],
			alias_target: None,
			implementors: vec![],
			impls: vec![],
		}
	}
	let series = "polars_core::series::Series";
	let field = "polars_core::datatypes::field::Field";
	let mk = |key: &str,
	          name: &str,
	          params: Vec<(&str, &str)>,
	          generics: Vec<(&str, &str)>,
	          ret: Option<&str>,
	          bucket: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: series.into(),
		name: name.into(),
		canonical_path: format!("{series}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: ret.map(String::from),
		generics_canonical: generics
			.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
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
		bucket: bucket.into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let inv = Inventory {
		callables: vec![
			mk("ints", "ints", vec![], vec![], Some("&[i64]"), "mechanical"),
			mk(
				"bytes",
				"bytes",
				vec![],
				vec![],
				Some("&[u8]"),
				"mechanical",
			),
			mk(
				"opt",
				"opt",
				vec![],
				vec![],
				Some("core::option::Option<&[u8]>"),
				"mechanical",
			),
			mk(
				"fields",
				"fields",
				vec![],
				vec![],
				Some(&format!("&[{field}]")),
				"mechanical",
			),
			mk(
				"nested",
				"nested",
				vec![],
				vec![],
				Some("&[&[u8]]"),
				"mechanical",
			),
			mk(
				"items",
				"items",
				vec![],
				vec![],
				Some("impl core::iter::traits::iterator::Iterator<Item = &[i64]>"),
				"mechanical",
			),
			mk(
				"cb",
				"each_bytes",
				vec![("f", "F")],
				vec![(
					"F",
					"core::ops::function::FnMut(core::option::Option<&[u8]>)",
				)],
				None,
				"callback",
			),
			mk(
				"mutable",
				"mutable",
				vec![],
				vec![],
				Some("&mut [u8]"),
				"mechanical",
			),
			mk(
				"arrow",
				"arrow",
				vec![],
				vec![],
				Some("&[alloc::boxed::Box<dyn polars_arrow::array::Array>]"),
				"mechanical",
			),
			mk(
				"iters",
				"iters",
				vec![],
				vec![],
				Some("&[impl core::iter::traits::iterator::Iterator<Item = i64>]"),
				"mechanical",
			),
		],
		supporting: vec![sup(series), sup(field)],
		provenance: None,
	};
	let mut release = Release {
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
	release
		.families
		.callback_invocation
		.push(CallbackInvocation {
			path: format!("{series}::each_bytes"),
			param: "f".into(),
			invocation: "immediate".into(),
			sinks: vec![],
			cite: "t".into(),
		});
	let world = World::new(&inv, &release, &["mechanical", "callback"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let emit = |key: &str| {
		let mut e = empty();
		emit_callable(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical", "callback"],
		);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for key in ["ints", "bytes", "opt", "fields", "nested", "items"] {
		let (status, reason, functions) = emit(key);
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			functions.contains("support::copy_slice(__r, \"")
				&& functions.contains(&format!("\"{}\"", key.replace("ints", "ints"))),
			"{key}: the slice is copied under the bound, naming the operation:\n{functions}"
		);
		assert!(
			functions.contains("-> Result<"),
			"{key}: a bounded copy is fallible"
		);
	}
	let (_, _, functions) = emit("nested");
	assert_eq!(
		functions.matches("support::copy_slice(").count(),
		2,
		"a slice of slices copies at both levels"
	);
	let (_, _, functions) = emit("items");
	assert!(
		functions.contains("support::SliceBudget::enter()")
			&& functions.contains("support::materialize_"),
		"iterator items share one cumulative bound over the materialized run"
	);
	let (status, reason, functions) = emit("cb");
	assert_eq!(status, "generated", "cb: {reason}");
	assert!(
		functions.contains("support::copy_slice(")
			&& functions.contains("CallbackFailure { op: \"Series::each_bytes\""),
		"a callback's slice is a bounded snapshot whose refusal is the typed failure:\n{functions}"
	);
	// a mutable slice return is a mutable borrow of an inner value (record
	// 0116, stage 1: only `&mut Self`/`&mut` owner returns are chains), so it
	// is refused by name: no slice is exposed and nothing is copied
	let (status, reason, functions) = emit("mutable");
	assert_eq!(status, "unsupported");
	assert!(reason.starts_with("inner mutable borrow"), "{reason}");
	assert!(
		!functions.contains("support::copy_slice("),
		"a mutable slice is never copied or exposed:\n{functions}"
	);
	for (key, why) in [("arrow", "foreign type"), ("iters", "iterator")] {
		let (status, reason, _) = emit(key);
		assert_eq!(status, "unsupported", "{key} must stay refused");
		assert!(
			reason.contains(why),
			"{key}: refusal must say `{why}`, got {reason}"
		);
	}
	println!("slice self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn slice() {
		super::slice_self_test();
	}
	#[test]
	fn generic_input() {
		super::generic_input_self_test();
	}
	#[test]
	fn bitmap() {
		super::bitmap_self_test();
	}
	#[test]
	fn bitmap_input() {
		super::bitmap_input_self_test();
	}
}
