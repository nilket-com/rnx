use crate::emit::Emitted;
use crate::emit::callable::emit_callable;
use crate::families::callbacks::callback_input;
use crate::model::{Callable, Inventory, Supporting};
use crate::release::{InstantiationScope, Release, ReleaseProvenance};
use crate::ty;
use crate::world::World;
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Clone)]
pub(crate) struct BoundedReadback {
	pub(crate) key: String,
	pub(crate) path: String,
	pub(crate) cite: String,
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct CowReturn {
	pub(crate) key: String,
	pub(crate) path: String,
	pub(crate) cite: String,
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct IteratorReturn {
	pub(crate) path: String,
	pub(crate) item: String,
	pub(crate) cite: String,
}

/// Record 0093 controls, from a synthetic inventory: a u64/usize read-back is
/// range-checked (`support::widen`, fallible, named by its operation) on every
/// return route; only a listed key+path pair keeps the plain bounded `usize`;
/// an unlisted same-named callable, the same path under another key and an
/// uncited `height` stay checked; narrow integers keep the lossless cast; a
/// callback's `u64` input fails the callback instead of wrapping.
pub(crate) fn checked_readback_self_test() {
	let frame = "polars_core::frame::dataframe::DataFrame";
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
	let mk = |key: &str, owner: &str, name: &str, ret: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: name.into(),
		canonical_path: format!("{owner}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
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
	let other = "polars_core::series::Series";
	let inv = Inventory {
		callables: vec![
			mk("width", frame, "width", "usize"),
			mk("width_other_key", frame, "width", "usize"),
			mk("width_other_owner", other, "width", "usize"),
			mk("len", frame, "height", "usize"),
			mk("hash", frame, "hash_rows", "u64"),
			mk("opt", frame, "maybe_count", "core::option::Option<u64>"),
			mk("tuple", frame, "shape", "(usize, usize)"),
			mk("signed", frame, "offset", "isize"),
			mk("narrow", frame, "small", "u32"),
		],
		supporting: vec![sup(frame), sup(other)],
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
		bitmap_returns: vec![],
		bitmap_inputs: vec![],
		iterator_returns: vec![],
		cow_returns: vec![],
		free_instantiations: vec![],
		method_scalar_generics: vec![],
		bounded_readbacks: vec![],
		hash_tokens: vec![],
		null_aware_returns: vec![],
		sized_self_methods: vec![],
		external_bounds: vec![],
		chunk_snapshots: vec![],
		indexed_chunk_snapshots: vec![],
		array_snapshots: vec![],
		iter_snapshots: vec![],
		view_snapshots: vec![],
		owned_iter_snapshots: vec![],
		layout_snapshots: vec![],
		callback_mutable: vec![],
		callback_invocation: vec![],
		callback_sink: vec![],
		callback_safe: vec![],
		callback_recipe: vec![],
	};
	release.bounded_readbacks.push(BoundedReadback {
		key: "width".into(),
		path: format!("{frame}::width"),
		cite: "t".into(),
	});
	// an entry without a citation proves nothing: `height` stays checked below
	release.bounded_readbacks.push(BoundedReadback {
		key: "len".into(),
		path: format!("{frame}::height"),
		cite: " ".into(),
	});
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
	let (status, reason, f) = emit("width");
	assert_eq!(status, "generated", "width: {reason}");
	assert!(
		f.contains("support::bounded_usize(") && !f.contains("Result<") && !f.contains("widen"),
		"a listed bounded read-back keeps the plain API: {f}"
	);
	for (key, source, op) in [
		("width_other_key", "usize", "width"),
		("width_other_owner", "usize", "width"),
		("len", "usize", "height"),
		("hash", "u64", "hash_rows"),
		("opt", "u64", "maybe_count"),
		("tuple", "usize", "shape"),
		("signed", "isize", "offset"),
	] {
		let (status, reason, f) = emit(key);
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains("-> Result<"),
			"{key}: a checked read-back is fallible: {f}"
		);
		assert!(
			f.contains(&format!("support::widen::<{source}>(")),
			"{key}: widened with a range check: {f}"
		);
		assert!(
			f.contains(&format!("\"{op}\"")),
			"{key}: the error names the method {op}, as every return conversion has since record 0087: {f}"
		);
		assert!(
			!f.contains("bounded_usize") && !f.contains(" as i64"),
			"{key}: no residual unchecked cast: {f}"
		);
		assert!(
			!f.contains("__OP__"),
			"{key}: operation placeholder substituted: {f}"
		);
	}
	let (_, _, f) = emit("tuple");
	assert_eq!(
		f.matches("support::widen::<usize>(").count(),
		2,
		"both tuple fields are checked: {f}"
	);
	let (status, reason, f) = emit("narrow");
	assert_eq!(status, "generated", "narrow: {reason}");
	assert!(
		f.contains(" as i64") && !f.contains("widen") && !f.contains("Result<"),
		"a u32 read-back stays a lossless plain cast: {f}"
	);
	for t in ["u64", "usize", "&[u64]"] {
		let conv = callback_input(&world, &ty::parse(t), "__x", None)
			.unwrap_or_else(|e| panic!("{t}: {e:?}"));
		assert!(
			conv.contains(&format!(
				"support::widen::<{}>(",
				t.trim_start_matches("&[").trim_end_matches(']')
			)) || conv.contains("support::copy_slice("),
			"{t}: a callback input is checked: {conv}"
		);
		assert!(
			conv.contains("support::callback::unwind(") && conv.contains("CallbackFailure"),
			"{t}: a failed conversion fails the callback, typed: {conv}"
		);
		assert!(
			!conv.contains(" as i64"),
			"{t}: no residual unchecked cast: {conv}"
		);
	}
	assert!(
		!world.bounded_ok.get(),
		"the scope never outlives its callable"
	);
	println!("checked-readback self-test: ok");
}

/// Record 0088 gate 2 controls, from a synthetic inventory: a listed
/// callable's `Cow<Self>` or `Cow<Wrapped>` return is made owned inside the
/// call (inside the engine closure when routed); an unlisted `Cow<Wrapped>`,
/// a listed path under another key, `Cow<Field>`-style unwrapped inners and
/// a `Cow` of an Arrow array stay refused.
pub(crate) fn cow_return_self_test() {
	let series = "polars_core::series::Series";
	let frame = "polars_core::frame::dataframe::DataFrame";
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
	let mk = |key: &str, name: &str, ret: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: series.into(),
		name: name.into(),
		canonical_path: format!("{series}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
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
			mk("self", "rechunk_cow", "alloc::borrow::Cow<Self>"),
			mk(
				"other",
				"to_frame_cow",
				&format!("alloc::borrow::Cow<{frame}>"),
			),
			mk(
				"same_path_other_key",
				"rechunk_cow",
				"alloc::borrow::Cow<Self>",
			),
			mk("unlisted", "maybe_owned", "alloc::borrow::Cow<Self>"),
			mk("unwrapped", "name_cow", "alloc::borrow::Cow<str>"),
			mk(
				"arrow",
				"chunk_cow",
				"alloc::borrow::Cow<polars_arrow::array::PrimitiveArray<i64>>",
			),
		],
		supporting: vec![sup(series), sup(frame)],
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
		bitmap_returns: vec![],
		bitmap_inputs: vec![],
		iterator_returns: vec![],
		cow_returns: vec![],
		free_instantiations: vec![],
		method_scalar_generics: vec![],
		bounded_readbacks: vec![],
		hash_tokens: vec![],
		null_aware_returns: vec![],
		sized_self_methods: vec![],
		external_bounds: vec![],
		chunk_snapshots: vec![],
		indexed_chunk_snapshots: vec![],
		array_snapshots: vec![],
		iter_snapshots: vec![],
		view_snapshots: vec![],
		owned_iter_snapshots: vec![],
		layout_snapshots: vec![],
		callback_mutable: vec![],
		callback_invocation: vec![],
		callback_sink: vec![],
		callback_safe: vec![],
		callback_recipe: vec![],
	};
	for (k, n) in [
		("self", "rechunk_cow"),
		("other", "to_frame_cow"),
		("unwrapped", "name_cow"),
		("arrow", "chunk_cow"),
	] {
		release.cow_returns.push(CowReturn {
			key: k.into(),
			path: format!("{series}::{n}"),
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
	for key in ["self", "other"] {
		let (status, reason, f) = emit(key);
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains(".into_owned()"),
			"{key}: the Cow is made owned inside the call: {f}"
		);
		assert!(
			!f.contains("Cow<"),
			"{key}: no Cow in the binding's signature: {f}"
		);
	}
	for key in ["same_path_other_key", "unlisted", "unwrapped", "arrow"] {
		let (status, reason, _) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must stay refused, got {reason}"
		);
	}
	assert!(!world.cow_ok.get(), "the scope never outlives its callable");
	println!("cow-return self-test: ok");
}

/// Record 0087 gate 2 controls, from a synthetic inventory: a listed
/// callable's concrete `Map` iterator (through its alias) materializes as
/// exact-size, range-checked integers inside the call; an unlisted method
/// returning the same alias, a different `Map` and an Arrow array return
/// stay refused, and the scope is clear after emission.
pub(crate) fn iterator_return_self_test() {
	let series = "polars_core::series::Series";
	let alias = "polars_core::chunked_array::ChunkLenIter";
	let map = "core::iter::adapters::map::Map<core::slice::iter::Iter<'a, alloc::boxed::Box<dyn polars_arrow::array::Array>>, fn(&alloc::boxed::Box<dyn polars_arrow::array::Array>) -> usize>";
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
	let mut alias_sup = sup(alias);
	alias_sup.kind = "type_alias".into();
	alias_sup.alias_target = Some(map.into());
	let mk = |key: &str, name: &str, ret: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: series.into(),
		name: name.into(),
		canonical_path: format!("{series}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
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
			mk("listed", "chunk_lengths", alias),
			mk("unlisted", "other_lengths", alias),
			mk(
				"other_map",
				"names",
				"core::iter::adapters::map::Map<core::slice::iter::Iter<'a, alloc::string::String>, fn(&alloc::string::String) -> usize>",
			),
			mk(
				"arrays",
				"chunks",
				"&alloc::vec::Vec<alloc::boxed::Box<dyn polars_arrow::array::Array>>",
			),
		],
		supporting: vec![sup(series), alias_sup],
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
		bitmap_returns: vec![],
		bitmap_inputs: vec![],
		iterator_returns: vec![],
		cow_returns: vec![],
		free_instantiations: vec![],
		method_scalar_generics: vec![],
		bounded_readbacks: vec![],
		hash_tokens: vec![],
		null_aware_returns: vec![],
		sized_self_methods: vec![],
		external_bounds: vec![],
		chunk_snapshots: vec![],
		indexed_chunk_snapshots: vec![],
		array_snapshots: vec![],
		iter_snapshots: vec![],
		view_snapshots: vec![],
		owned_iter_snapshots: vec![],
		layout_snapshots: vec![],
		callback_mutable: vec![],
		callback_invocation: vec![],
		callback_sink: vec![],
		callback_safe: vec![],
		callback_recipe: vec![],
	};
	release.iterator_returns.push(IteratorReturn {
		path: format!("{series}::chunk_lengths"),
		item: "usize".into(),
		cite: "t".into(),
	});
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
	let (status, reason, f) = emit("listed");
	assert_eq!(status, "generated", "{reason}");
	assert!(
		f.contains("-> Result<Vec<i64>, Error>")
			&& f.contains("support::materialize_exact(__it, \"chunk_lengths\"")
			&& f.contains("support::widen::<usize>(__r, \"chunk_lengths\")?"),
		"exact-size, range-checked, materialized inside the call: {f}"
	);
	assert!(
		f.find("chunk_lengths(__arg0)").unwrap() < f.find("materialize_exact").unwrap()
			&& !f.contains("__it }"),
		"the iterator is consumed before the receiver borrow ends: {f}"
	);
	for key in ["unlisted", "other_map", "arrays"] {
		let (status, reason, _) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must stay refused, got {reason}"
		);
	}
	assert!(
		world.iter_return.borrow().is_none(),
		"the scope never outlives its callable"
	);
	println!("iterator-return self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn iterator_return() {
		super::iterator_return_self_test();
	}
	#[test]
	fn cow_return() {
		super::cow_return_self_test();
	}
	#[test]
	fn checked_readback() {
		super::checked_readback_self_test();
	}
}
