use crate::emit::callable::{emit_callable, emit_method_with};
use crate::emit::{Emitted, OracleInfo, RouteException, binding_id};
use crate::families::bounds::check_natives;
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{InstantiationScope, Release, ReleaseProvenance};
use crate::text::{mentions, replace_token, sanitize};
use crate::world::World;
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Clone)]
pub(crate) struct FreeInstantiation {
	pub(crate) key: String,
	pub(crate) path: String,
	/// The public path the binding calls; Rust infers `T` from the argument.
	pub(crate) callee: String,
	/// The generic parameter and its concrete types, one binding each.
	pub(crate) generic: String,
	pub(crate) types: Vec<String>,
	/// Record 0090: the native scalar of each listed type, in the same
	/// order, for `T::Native` in the parameters (empty when unused).
	#[serde(default)]
	pub(crate) natives: Vec<String>,
	/// Record 0091: a parameter checked before the call, and the check
	/// (`below_idx_max`: the converted `usize` must be `< IdxSize::MAX`).
	#[serde(default)]
	pub(crate) guard_param: Option<String>,
	#[serde(default)]
	pub(crate) guard: Option<String>,
	pub(crate) cite: String,
}

/// Record 0089 gate 2 controls, from a synthetic inventory: a listed generic
/// free function over `&ChunkedArray<T>` becomes a static function on each
/// listed type's wrapper (including a type held by an alias such as
/// `IdxCa`), borrowing its argument and checking a `usize` result into
/// range; an unlisted generic free function, a listed function under an
/// unlisted type, and a return-only generic stay refused.
pub(crate) fn free_instantiation_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let i64c = "polars_core::datatypes::Int64Chunked";
	let idx = "polars_core::datatypes::aliases::IdxCa";
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
	let alias = |path: &str, target: &str| {
		let mut a = sup(path);
		a.kind = "type_alias".into();
		a.alias_target = Some(target.into());
		a
	};
	let mut generic = sup(ca);
	generic.generic = true;
	let mk = |key: &str,
	          name: &str,
	          params: Vec<(&str, &str)>,
	          generics: Vec<(&str, &str)>,
	          ret: &str| Callable {
		key: key.into(),
		kind: "free_fn".into(),
		krate: "polars_core".into(),
		owner: String::new(),
		name: name.into(),
		canonical_path: format!("polars_core::m::{name}"),
		found_paths: vec![format!("polars::m::{name}")],
		crate_paths: vec![],
		receiver: "none".into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: Some(ret.into()),
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
	let arg = format!("&{ca}<T>");
	let inv = Inventory {
		callables: vec![
			mk(
				"listed",
				"arg_lo",
				vec![("ca", &arg)],
				vec![("T", "polars_core::datatypes::PolarsNumericType")],
				"core::option::Option<usize>",
			),
			mk(
				"unlisted",
				"arg_hi",
				vec![("ca", &arg)],
				vec![("T", "polars_core::datatypes::PolarsNumericType")],
				"core::option::Option<usize>",
			),
			mk(
				"ret_only",
				"zero",
				vec![],
				vec![("T", "polars_core::datatypes::PolarsNumericType")],
				"T",
			),
		],
		supporting: vec![
			generic,
			alias(i64c, &format!("{ca}<polars_core::datatypes::Int64Type>")),
			alias(idx, &format!("{ca}<polars_core::datatypes::UInt32Type>")),
		],
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
	release.free_instantiations.push(FreeInstantiation {
		key: "listed".into(),
		path: "polars_core::m::arg_lo".into(),
		callee: "polars::m::arg_lo".into(),
		generic: "T".into(),
		types: vec![
			"polars_core::datatypes::Int64Type".into(),
			"polars_core::datatypes::UInt32Type".into(),
			"polars_core::datatypes::Float32Type".into(),
		],
		natives: vec![],
		guard_param: None,
		guard: None,
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
		(e.entries[0].clone(), e.functions)
	};
	let (e, f) = emit("listed");
	assert_eq!(e.status, "generated", "{:?}", e.reason);
	assert_eq!(
		e.bindings.len(),
		2,
		"one binding per listed type that a wrapper holds: {:?}",
		e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>()
	);
	assert!(
		e.exceptions
			.iter()
			.any(|x| x.reason.contains("Float32Type")),
		"a listed type without a wrapper is a named exception"
	);
	assert!(
		f.contains("::arg_lo)]")
			&& f.contains("polars::m::arg_lo(&ca.0)")
			&& f.contains("support::widen::<usize>(__r, \"arg_lo\")?")
			&& f.contains("None => None"),
		"static on the wrapper, borrowed, range-checked: {f}"
	);
	assert!(
		!f.contains("ca.0.clone()") && !f.contains("as i64"),
		"no clone of the argument, no unchecked cast: {f}"
	);
	for key in ["unlisted", "ret_only"] {
		let (e, _) = emit(key);
		assert_eq!(
			e.status, "unsupported",
			"{key} must stay refused, got {:?}",
			e.reason
		);
	}
	println!("free-instantiation self-test: ok");
}

/// Record 0091: the shape a `[[free_instantiations]]` guard must have: both
/// fields or neither, a known check, and a parameter of the callable whose
/// type is `usize` (the only type the check is defined for).
pub(crate) fn guard_shape(f: &FreeInstantiation, c: &Callable) -> Result<(), String> {
	if !f.natives.is_empty() {
		check_natives(&f.path, &f.types, &f.natives)?;
	}
	match (&f.guard_param, &f.guard) {
		(None, None) => Ok(()),
		(Some(_), None) | (None, Some(_)) => Err(format!(
			"`{}`: guard_param and guard must be given together",
			f.path
		)),
		(Some(param), Some(check)) => {
			if check != "below_idx_max" {
				return Err(format!("`{}`: unknown guard `{check}`", f.path));
			}
			match c.params.iter().find(|p| sanitize(&p.name) == *param) {
				None => Err(format!(
					"`{}`: guard_param `{param}` is not a parameter",
					f.path
				)),
				Some(p) if p.ty_canonical.trim() != "usize" => Err(format!(
					"`{}`: guard_param `{param}` is `{}`, not `usize`",
					f.path, p.ty_canonical
				)),
				Some(_) => Ok(()),
			}
		}
	}
}

/// Record 0089: a generic free function instantiated once per concrete type
/// the release file lists. `ChunkedArray<T>` in its parameters is spelled as
/// that type's wrapper, the binding is a static function on the wrapper
/// (`polars::Int64Chunked::arg_min_numeric(ca)`), and it calls the Polars
/// function by its public path, Rust inferring `T` from the argument. A
/// `usize` result converts with a range check.
pub(crate) fn emit_free_instantiations(world: &World, out: &mut Emitted, c: &Callable) {
	let f = world
		.release
		.free_instantiations
		.iter()
		.find(|f| f.key == c.key && f.path == c.canonical_path)
		.unwrap()
		.clone();
	// record 0091 review: a guard must be complete, known, and name one of the
	// callable's `usize` parameters, or nothing is emitted (fail closed)
	if let Err(why) = guard_shape(&f, c) {
		out.unsupported(c, "free instantiation guard", &why);
		return;
	}
	struct Guard<'a>(&'a std::cell::RefCell<Option<(String, String, String)>>);
	impl Drop for Guard<'_> {
		fn drop(&mut self) {
			*self.0.borrow_mut() = None;
		}
	}
	let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
	let mut infos: Vec<OracleInfo> = Vec::new();
	let mut exceptions: Vec<RouteException> = Vec::new();
	let generic_ca = format!("polars_core::chunked_array::ChunkedArray<{}>", f.generic);
	let native_of = format!("{}::Native", f.generic);
	for (n, t) in f.types.iter().enumerate() {
		let identity = format!("polars_core::chunked_array::ChunkedArray<{t}>");
		let Some(alias) = world.by_identity.get(&identity).cloned() else {
			exceptions.push(RouteException {
				route: "free instantiation",
				receiver: t.clone(),
				reason: format!("no wrapper holds `{identity}`"),
			});
			continue;
		};
		let mut syn = c.clone();
		syn.owner = alias.clone();
		syn.bucket = "mechanical".into();
		syn.kind = "inherent".into();
		syn.receiver = "none".into();
		// record 0090: whole-token substitution only; a path or bound that merely
		// contains the spelling is left alone and then refused as residual
		for q in syn.params.iter_mut() {
			q.ty_canonical = replace_token(&q.ty_canonical, &generic_ca, &alias);
			if let Some(native) = f.natives.get(n) {
				q.ty_canonical = replace_token(&q.ty_canonical, &native_of, native);
			}
		}
		syn.generics_canonical.clear();
		if syn
			.params
			.iter()
			.any(|q| mentions(&q.ty_canonical, &f.generic) || q.ty_canonical.contains("::Native"))
		{
			exceptions.push(RouteException {
				route: "free instantiation",
				receiver: alias.clone(),
				reason: format!("`{}` remains in a parameter after substitution", f.generic),
			});
			continue;
		}
		if let (Some(param), Some(check)) = (&f.guard_param, &f.guard) {
			*world.arg_guard.borrow_mut() = Some((c.name.clone(), param.clone(), check.clone()));
		}
		let _guard = Guard(&world.arg_guard);
		let before = out.entries.len();
		emit_method_with(world, out, &syn, &alias, None, false, Some(&f.callee));
		let e = out.entries.pop().unwrap();
		debug_assert_eq!(before, out.entries.len());
		if e.status == "generated" {
			done.push((
				e.rune.clone().unwrap(),
				alias.clone(),
				"free instantiation",
				None,
			));
			if let Some(i) = e.oracle.clone() {
				infos.push(i);
			}
		} else {
			exceptions.push(RouteException {
				route: "free instantiation",
				receiver: alias.clone(),
				reason: format!("{}: {}", e.status, e.reason.unwrap_or_default()),
			});
		}
	}
	if done.is_empty() || infos.is_empty() {
		out.unsupported(
			c,
			"free instantiation",
			"no listed type emitted; see exceptions",
		);
		out.entries.last_mut().unwrap().exceptions = exceptions;
		return;
	}
	out.generated_on(c, &done, infos[0].clone());
	let e = out.entries.last_mut().unwrap();
	for (b, i) in e.bindings.iter_mut().zip(infos) {
		b.info = Some(i);
		b.id = binding_id(&c.canonical_path, b.receiver.as_deref(), false);
	}
	e.note = Some(format!(
		"instantiated on {} of {} listed types ({})",
		done.len(),
		f.types.len(),
		f.cite
	));
	e.exceptions = exceptions;
}

#[cfg(test)]
mod tests {
	#[test]
	fn free_instantiation() {
		super::free_instantiation_self_test();
	}
}
