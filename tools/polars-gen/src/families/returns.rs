use crate::emit::Emitted;
use crate::emit::callable::emit_callable;
use crate::families::callbacks::callback_input;
use crate::families::{Family, Listed, RetSite, State};
use crate::model::{Callable, Inventory, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::ty;
use crate::ty::Ty;
use crate::world::World;
use crate::world::mapping::IterLen;
use crate::world::mapping::IterWrap;
use crate::world::mapping::Materialize;
use crate::world::mapping::{Ret, Unsupported};
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

// ---------------------------------------------------------------- record 0115: this module's families

/// Record 0088: a listed `Cow<Wrapped>` return, made owned inside the call.
pub(crate) struct CowReturnFamily;
pub(crate) static COW_RETURN: CowReturnFamily = CowReturnFamily;

impl Family for CowReturnFamily {
	fn name(&self) -> &'static str {
		"cow_return"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		// record 0088: callables whose `Cow<Wrapped>` return is made owned
		if world
			.release
			.families
			.cow_returns
			.iter()
			.any(|r| r.key == c.key && r.path == c.canonical_path)
		{
			Listed::Active(State::On)
		} else {
			Listed::Unlisted
		}
	}
	fn ret(
		&self,
		world: &World,
		_state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Cow {
			return Ok(None);
		}
		// record 0088: a listed callable's `Cow<Wrapped>` becomes owned inside the call
		let Ty::Path { path, args } = t else {
			return Ok(None);
		};
		if !(path == "alloc::borrow::Cow" && args.len() == 1) {
			return Ok(None);
		}
		(|| -> Result<Ret, Unsupported> {
			let wrapped = match &args[0] {
				Ty::Path { path: p, args: a } if a.is_empty() => {
					let p = if p == "Self" {
						owner.unwrap_or("")
					} else {
						p.as_str()
					};
					world.wrapper_for(p).is_some() && world.clonable.contains(p)
				}
				Ty::Generic(g) if g == "Self" => owner
					.is_some_and(|o| world.wrapper_for(o).is_some() && world.clonable.contains(o)),
				_ => false,
			};
			if !wrapped {
				return Err(Unsupported("cow of an unwrapped type", t.render()));
			}
			let x = world.ret(&args[0], owner, depth + 1)?;
			Ok(Ret {
				materialize: None,
				rust_ty: x.rust_ty,
				fallible: x.fallible,
				conv: format!("{{ let __r = __r.into_owned(); {} }}", x.conv),
				doc: format!("{} (owned)", x.doc),
			})
		})()
		.map(Some)
	}
}

/// Record 0093: a listed `usize` proven to be a length or index.
pub(crate) struct BoundedReadbackFamily;
pub(crate) static BOUNDED_READBACK: BoundedReadbackFamily = BoundedReadbackFamily;

impl Family for BoundedReadbackFamily {
	fn name(&self) -> &'static str {
		"bounded_readback"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		// record 0093: a proven length or index, with its citation
		if world
			.release
			.families
			.bounded_readbacks
			.iter()
			.any(|r| r.key == c.key && r.path == c.canonical_path && !r.cite.trim().is_empty())
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
		if site != RetSite::Scalar {
			return Ok(None);
		}
		// record 0093: a proven length or index keeps a plain integer
		let Ty::Path { path, .. } = t else {
			return Ok(None);
		};
		if path == "usize" {
			return Ok(Some(Ret {
				materialize: None,
				rust_ty: "i64".to_string(),
				fallible: false,
				conv: "support::bounded_usize(__r)".into(),
				doc: "int (a proven length or index)".to_string(),
			}));
		}
		Ok(None)
	}
}

/// Record 0087: a listed concrete iterator of integers.
pub(crate) struct IteratorReturnFamily;
pub(crate) static ITERATOR_RETURN: IteratorReturnFamily = IteratorReturnFamily;

impl Family for IteratorReturnFamily {
	fn name(&self) -> &'static str {
		"iterator_return"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		// record 0087: a listed path's concrete iterator item
		match world
			.release
			.families
			.iterator_returns
			.iter()
			.find(|r| r.path == c.canonical_path)
		{
			Some(r) => Listed::Active(State::One(r.item.clone())),
			None => Listed::Unlisted,
		}
	}
	fn ret(
		&self,
		_world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		_owner: Option<&str>,
		_depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0087: a listed callable's concrete `Map` iterator of integers
		// (reached through the `ChunkLenIter` alias, one level down)
		{
			if let (Some(item), Ty::Path { path, .. }) = (state.one(), t) {
				if path == "core::iter::adapters::map::Map" {
					let limit = 1usize << 20;
					return Ok(Some(Ret {
						materialize: Some(Materialize {
							elem_conv: format!("support::widen::<{item}>(__r, \"__OP__\")?"),
							known: IterLen::Exact,
							wrap: IterWrap::Plain,
						}),
						rust_ty: "Vec<i64>".into(),
						conv: "__r".into(),
						fallible: true,
						doc: format!(
							"vector of int (materialized, at most {limit} items, each checked into range)"
						),
					}));
				}
			}
		}
		Ok(None)
	}
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
		families: FamilyTables::default(),
	};
	release.families.bounded_readbacks.push(BoundedReadback {
		key: "width".into(),
		path: format!("{frame}::width"),
		cite: "t".into(),
	});
	// an entry without a citation proves nothing: `height` stays checked below
	release.families.bounded_readbacks.push(BoundedReadback {
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
		frozen_ids: Default::default(),
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
		!world.active.is_set("bounded_readback"),
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
		families: FamilyTables::default(),
	};
	for (k, n) in [
		("self", "rechunk_cow"),
		("other", "to_frame_cow"),
		("unwrapped", "name_cow"),
		("arrow", "chunk_cow"),
	] {
		release.families.cow_returns.push(CowReturn {
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
		frozen_ids: Default::default(),
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
	// record 0115: a listed callable's state does not reach the next one:
	// "unlisted" is emitted after the listed "self" and "other" on the same
	// owner, and nothing is active between them
	assert!(!world.active.is_set("cow_return"));
	for key in ["same_path_other_key", "unlisted", "unwrapped", "arrow"] {
		let (status, reason, _) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must stay refused, got {reason}"
		);
	}
	assert!(
		!world.active.is_set("cow_return"),
		"the scope never outlives its callable"
	);
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
		families: FamilyTables::default(),
	};
	release.families.iterator_returns.push(IteratorReturn {
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
		frozen_ids: Default::default(),
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
		!world.active.is_set("iterator_return"),
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
