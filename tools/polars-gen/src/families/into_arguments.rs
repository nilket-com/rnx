//! Record 0123: an `impl Into<T>` parameter accepts its recorded `From`
//! sources. A listed target `T` (a closed, cited release table) takes a
//! `T`, or a value of a listed source `S` for which the inventory records
//! `From<S> for T`, converted with Polars' own `<T as From<S>>::from`. The
//! one Rune argument is dispatched on its type before any Polars call;
//! anything else is refused with a VM error naming the accepted types, so
//! the binding's fallibility and return type are unchanged.
use crate::model::Callable;
use crate::world::World;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntoArgument {
	/// The `Into` target's canonical path.
	pub(crate) target: String,
	/// The admitted sources' canonical paths.
	pub(crate) sources: Vec<String>,
	pub(crate) cite: String,
}

/// The table against the inventory and the wrappers, fail closed: each row
/// cited and unique, its target and every source wrapped, and each source's
/// `From<S> for T` recorded in the inventory.
pub(crate) fn validate(
	world: &World,
	rows: &[IntoArgument],
	callables: &[Callable],
) -> Result<(), String> {
	let mut seen = std::collections::BTreeSet::new();
	for r in rows {
		if r.cite.trim().is_empty() {
			return Err(format!("{}: no citation", r.target));
		}
		if !seen.insert(r.target.clone()) {
			return Err(format!("{}: listed twice", r.target));
		}
		if r.sources.is_empty() {
			return Err(format!("{}: no source", r.target));
		}
		if !world.wrappers.contains_key(&r.target) {
			return Err(format!("{}: the target is not wrapped", r.target));
		}
		for s in &r.sources {
			if !world.wrappers.contains_key(s) {
				return Err(format!("{}: source {s} is not wrapped", r.target));
			}
			let from = format!("{} as core::convert::From", r.target);
			let recorded = callables.iter().any(|c| {
				c.canonical_path == from && c.params.len() == 1 && c.params[0].ty_canonical == *s
			});
			if !recorded {
				return Err(format!("{}: no recorded From<{s}> for it", r.target));
			}
		}
	}
	Ok(())
}

/// The listed row for an `Into` target, if any.
pub(crate) fn row<'a>(world: &'a World, target: &str) -> Option<&'a IntoArgument> {
	world
		.release
		.families
		.into_arguments
		.iter()
		.find(|r| r.target == target)
}

/// The argument for a listed target: one Rune value, dispatched on its type
/// before the call (the check runs first, and a refusal is a VM error), each
/// branch borrowing and cloning the wrapped value as a by-value wrapper
/// argument does.
pub(crate) fn arg(
	world: &World,
	r: &IntoArgument,
	name: &str,
) -> Option<crate::world::mapping::Arg> {
	let wt = world.wrappers.get(&r.target)?;
	let local = format!("__into_{name}");
	let mut accepted = vec![wt.rune_name.clone()];
	let mut check = format!(
		"let {local} = match {name}.borrow_ref::<{}>() {{ Ok(w) => w.0.clone(), Err(_) => ",
		wt.rust
	);
	let mut close = String::new();
	for s in &r.sources {
		let ws = world.wrappers.get(s)?;
		accepted.push(ws.rune_name.clone());
		check.push_str(&format!(
			"match {name}.borrow_ref::<{}>() {{ Ok(w) => <{} as From<{}>>::from(w.0.clone()), Err(_) => ",
			ws.rust, wt.spell, ws.spell
		));
		close.push_str(" }");
	}
	let accepted = accepted.join(" or ");
	check.push_str(&format!(
		"return rune::runtime::VmResult::err(rune::runtime::VmError::panic(format!(\"__OP__: `{name}` must be {accepted}, found {{}}\", {name}.type_info())))"
	));
	check.push_str(&close);
	check.push_str(" }; ");
	Some(crate::world::mapping::Arg {
		rust_ty: "rune::Value".into(),
		conv: local,
		fallible: false,
		doc: accepted,
		pre: vec![],
		borrow: 0,
		owned: None,
		shape: format!("W:{}", r.target),
		vm_check: Some(check),
	})
}

/// Record 0123: the table refuses what it must, and a listed source's
/// argument dispatches before the call with a VM error for anything else.
pub(crate) fn into_arguments_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".to_string(),
		canonical_path: path.to_string(),
		found_paths: vec![format!(
			"polars::prelude::{}",
			path.rsplit("::").next().unwrap()
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
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let (t, s, x) = (
		"polars_plan::dsl::datatype_expr::DataTypeExpr",
		"polars_core::datatypes::dtype::DataType",
		"polars_core::frame::column::Column",
	);
	let from: Callable = serde_json::from_value(serde_json::json!({
		"key": "f", "kind": "foreign_trait_impl", "krate": "polars_plan", "owner": t, "name": "From",
		"canonical_path": format!("{t} as core::convert::From"), "found_paths": [], "crate_paths": [], "receiver": "none",
		"params": [{"name": "value", "ty": s, "ty_canonical": s}],
		"ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
		"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
		"owner_generic": false, "is_unsafe": false, "is_async": false, "deprecated": false,
		"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
		"bucket": "mechanical", "rules": []
	}))
	.unwrap();
	let inv = Inventory {
		callables: vec![from.clone()],
		provenance: None,
		supporting: vec![sup(t), sup(s), sup(x)],
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_plan".into(), "polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let w = World::new(&inv, &release, &["mechanical"]);
	let row = |target: &str, sources: &[&str], cite: &str| IntoArgument {
		target: target.into(),
		sources: sources.iter().map(|s| s.to_string()).collect(),
		cite: cite.into(),
	};
	let calls = vec![from];
	assert!(validate(&w, &[row(t, &[s], "c")], &calls).is_ok());
	// the controls: an unlisted source (no recorded From), an unwrapped
	// target, an uncited row, a duplicate, an empty source list
	let refused = |rows: Vec<IntoArgument>, want: &str| {
		let e = validate(&w, &rows, &calls).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	refused(vec![row(t, &[x], "c")], "no recorded From");
	refused(
		vec![row("polars_plan::dsl::expr::Expr", &[s], "c")],
		"not wrapped",
	);
	refused(vec![row(t, &[s], " ")], "no citation");
	refused(vec![row(t, &[s], "c"), row(t, &[s], "c")], "listed twice");
	refused(vec![row(t, &[], "c")], "no source");
	// the argument: one Rune value, T or S borrowed and cloned, a VM error otherwise
	let a = arg(&w, &row(t, &[s], "c"), "dtype").unwrap();
	assert_eq!((a.rust_ty.as_str(), a.fallible), ("rune::Value", false));
	let check = a.vm_check.unwrap();
	assert!(
		check.contains("From<") && check.contains("w.0.clone()"),
		"{check}"
	);
	assert!(
		check.contains("VmError::panic") && check.contains("DataTypeExpr or DataType"),
		"{check}"
	);
	println!("into arguments self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn into_arguments() {
		super::into_arguments_self_test();
	}
}
