//! Record 0125: a listed callable's by-value `PlRefPath` parameter accepts a
//! script string. Polars takes a file path as `polars_utils::pl_path::PlRefPath`;
//! the type is wrapped, but a script has no way to build one. A closed, cited
//! release table names each (callable, parameter). The one Rune argument is
//! dispatched before any Polars call: a string is borrowed, never taken
//! from the script's binding (review of 0125), and converted with Polars' own
//! `<PlRefPath as From<&str>>::from`, a wrapped `PlRefPath` is cloned, and
//! either must be a local path (`support::local_path`: no URI scheme in the
//! string, and none that Polars itself sees in the converted path). Anything
//! else is refused with a VM error, so the binding's fallibility and return
//! type are unchanged.
use crate::families::{ArgSite, Family, Listed, State};
use crate::model::Callable;
use crate::ty::{Ty, last};
use crate::world::World;
use crate::world::mapping::{Arg, Unsupported};

pub(crate) const PATH: &str = "polars_utils::pl_path::PlRefPath";

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathArgument {
	/// The callable's canonical path.
	pub(crate) path: String,
	/// The parameter's name.
	pub(crate) param: String,
	pub(crate) cite: String,
}

/// The table against the inventory and the wrappers, fail closed: each row
/// cited and unique; its callable in the inventory with the named parameter
/// taking `PlRefPath` by value; `PlRefPath` wrapped; and `From<&str>` for it
/// recorded.
pub(crate) fn validate(
	world: &World,
	rows: &[PathArgument],
	callables: &[Callable],
) -> Result<(), String> {
	if rows.is_empty() {
		return Ok(());
	}
	if !world.wrappers.contains_key(PATH) {
		return Err(format!("{PATH} is not wrapped"));
	}
	let from = format!("{PATH} as core::convert::From");
	if !callables.iter().any(|c| {
		c.canonical_path == from && c.params.len() == 1 && c.params[0].ty_canonical == "&str"
	}) {
		return Err(format!("no recorded From<&str> for {PATH}"));
	}
	let mut seen = std::collections::BTreeSet::new();
	for r in rows {
		if r.cite.trim().is_empty() {
			return Err(format!("{}: no citation", r.path));
		}
		if !seen.insert((r.path.clone(), r.param.clone())) {
			return Err(format!("{} `{}`: listed twice", r.path, r.param));
		}
		let found: Vec<&Callable> = callables
			.iter()
			.filter(|c| c.canonical_path == r.path)
			.collect();
		if found.len() != 1 {
			return Err(format!(
				"{}: {} callables with this path, want exactly one",
				r.path,
				found.len()
			));
		}
		match found[0].params.iter().find(|p| p.name == r.param) {
			Some(p) if p.ty_canonical == PATH => {}
			Some(p) => {
				return Err(format!(
					"{} `{}`: takes {}, not {PATH} by value",
					r.path, r.param, p.ty_canonical
				));
			}
			None => return Err(format!("{}: no parameter `{}`", r.path, r.param)),
		}
	}
	Ok(())
}

/// Every listed row was generated: an unused row is refused, never ignored.
pub(crate) fn check_used(rows: &[PathArgument], generated: &[&str]) -> Result<(), String> {
	for r in rows {
		if !generated.contains(&r.path.as_str()) {
			return Err(format!(
				"{} `{}`: listed, but the callable was not generated",
				r.path, r.param
			));
		}
	}
	Ok(())
}

/// The argument: one Rune value, a string or a wrapped `PlRefPath`, checked
/// local before the call; a refusal is a VM error naming the operation, the
/// argument and why.
pub(crate) fn arg(world: &World, op: &str, name: &str) -> Option<Arg> {
	let wt = world.wrappers.get(PATH)?;
	let local = format!("__path_{name}");
	let accepted = format!("String or {}", wt.rune_name);
	let check = format!(
		"let {local} = match {name}.borrow_ref::<{rust}>() {{ \
		   Ok(w) => support::local_path_checked(w.0.clone()), \
		   Err(_) => match {name}.borrow_string_ref() {{ \
		     Ok(s) => support::local_path(&s), \
		     Err(_) => Err(format!(\"must be {accepted}, found {{}}\", {name}.type_info())) }} }}; \
		 let {local} = match {local} {{ Ok(p) => p, \
		   Err(why) => return rune::runtime::VmResult::err(rune::runtime::VmError::panic(format!(\"{op}: `{name}` {{why}}\"))) }}; ",
		rust = wt.rust
	);
	Some(Arg {
		rust_ty: "rune::Value".into(),
		conv: local,
		fallible: false,
		doc: format!("{accepted} (a local path)"),
		pre: vec![],
		borrow: 0,
		owned: None,
		shape: format!("W:{PATH}"),
		vm_check: Some(check),
	})
}

/// Record 0125: a listed callable's path parameter, from a script string.
pub(crate) struct PathArgumentFamily;
pub(crate) static PATH_ARGUMENT: PathArgumentFamily = PathArgumentFamily;

impl Family for PathArgumentFamily {
	fn name(&self) -> &'static str {
		"path_argument"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		match world
			.release
			.families
			.path_arguments
			.iter()
			.find(|r| r.path == c.canonical_path)
		{
			Some(r) => Listed::Active(State::Two(
				format!("{}::{}", last(&c.owner), c.name),
				r.param.clone(),
			)),
			None => Listed::Unlisted,
		}
	}
	fn arg(
		&self,
		world: &World,
		state: &State,
		site: ArgSite,
		t: &Ty,
		name: &str,
		_owner: Option<&str>,
	) -> Result<Option<Arg>, Unsupported> {
		if site != ArgSite::Top {
			return Ok(None);
		}
		let Some((op, param)) = state.two() else {
			return Ok(None);
		};
		match t {
			Ty::Path { path, args } if path == PATH && args.is_empty() && name == param => {
				arg(world, &op, name)
					.map(Some)
					.ok_or(Unsupported("path argument", t.render()))
			}
			_ => Ok(None),
		}
	}
}

/// Record 0125: the table refuses what it must, and a listed parameter's
/// argument takes a string or a `PlRefPath`, checked local before the call.
pub(crate) fn path_arguments_self_test() {
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
	let callable = |key: &str, path: &str, name: &str, params: serde_json::Value| -> Callable {
		serde_json::from_value(serde_json::json!({
			"key": key, "kind": "free_fn", "krate": "polars_lazy", "owner": "polars_lazy::frame::LazyFrame", "name": name,
			"canonical_path": path, "found_paths": [], "crate_paths": [], "receiver": "none",
			"params": params,
			"ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
			"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
			"owner_generic": false, "is_unsafe": false, "is_async": false, "deprecated": false,
			"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
			"bucket": "mechanical", "rules": []
		}))
		.unwrap()
	};
	let from = callable(
		"f",
		&format!("{PATH} as core::convert::From"),
		"from",
		serde_json::json!([{"name": "value", "ty": "&str", "ty_canonical": "&str"}]),
	);
	let scan = callable(
		"s",
		"polars_lazy::frame::LazyFrame::scan_parquet",
		"scan_parquet",
		serde_json::json!([{"name": "path", "ty": PATH, "ty_canonical": PATH}]),
	);
	let borrowed = callable(
		"b",
		"polars_lazy::frame::LazyFrame::scan_borrowed",
		"scan_borrowed",
		serde_json::json!([{"name": "path", "ty": format!("&{PATH}"), "ty_canonical": format!("&{PATH}")}]),
	);
	let inv = Inventory {
		callables: vec![from.clone(), scan.clone(), borrowed.clone()],
		provenance: None,
		supporting: vec![sup(PATH), sup("polars_lazy::frame::LazyFrame")],
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_lazy".into(), "polars_utils".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let w = World::new(&inv, &release, &["mechanical"]);
	let row = |path: &str, param: &str, cite: &str| PathArgument {
		path: path.into(),
		param: param.into(),
		cite: cite.into(),
	};
	let calls = vec![from, scan, borrowed];
	let ok = row("polars_lazy::frame::LazyFrame::scan_parquet", "path", "c");
	assert!(validate(&w, &[ok.clone()], &calls).is_ok());
	let refused = |rows: Vec<PathArgument>, calls: &[Callable], want: &str| {
		let e = validate(&w, &rows, calls).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	// the controls: no recorded From<&str>, a borrowed parameter, a missing
	// parameter or callable, an uncited row, a duplicate
	refused(vec![ok.clone()], &calls[1..], "no recorded From<&str>");
	refused(
		vec![row(
			"polars_lazy::frame::LazyFrame::scan_borrowed",
			"path",
			"c",
		)],
		&calls,
		"not polars_utils::pl_path::PlRefPath by value",
	);
	refused(
		vec![row(
			"polars_lazy::frame::LazyFrame::scan_parquet",
			"source",
			"c",
		)],
		&calls,
		"no parameter `source`",
	);
	refused(
		vec![row("polars_lazy::frame::LazyFrame::nowhere", "path", "c")],
		&calls,
		"0 callables",
	);
	refused(
		vec![row(
			"polars_lazy::frame::LazyFrame::scan_parquet",
			"path",
			" ",
		)],
		&calls,
		"no citation",
	);
	refused(vec![ok.clone(), ok.clone()], &calls, "listed twice");
	// an unused row is refused
	let e = check_used(&[ok.clone()], &[]).unwrap_err();
	assert!(e.contains("was not generated"), "{e}");
	assert!(check_used(&[ok], &["polars_lazy::frame::LazyFrame::scan_parquet"]).is_ok());
	// the argument: one Rune value, a string or a PlRefPath, checked local,
	// a VM error naming the qualified operation otherwise
	let a = arg(&w, "LazyFrame::scan_parquet", "path").unwrap();
	assert_eq!((a.rust_ty.as_str(), a.fallible), ("rune::Value", false));
	let check = a.vm_check.unwrap();
	assert!(
		check.contains("support::local_path(&s)")
			&& check.contains("support::local_path_checked(w.0.clone())"),
		"{check}"
	);
	// review of 0125: the string is borrowed, so the script's binding survives
	assert!(
		check.contains(".borrow_string_ref()") && !check.contains("from_value::<String>"),
		"{check}"
	);
	assert!(
		check.contains("VmError::panic")
			&& check.contains("LazyFrame::scan_parquet: `path`")
			&& check.contains("must be String or"),
		"{check}"
	);
	println!("path arguments self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn path_arguments() {
		super::path_arguments_self_test();
	}
}
