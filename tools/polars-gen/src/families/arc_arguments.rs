//! Record 0127: a listed callable's by-value `Arc<T>` parameter, where `T`
//! is a wrapped, `Clone` type, accepts the script's `T`. The binding passes
//! `Arc::new(value.0.clone())`: a shallow clone (a `DataFrame` clone shares
//! its column buffers) that the `Arc` owns, so Polars holds its own shared
//! snapshot and the script keeps its value, borrowed, never taken. The
//! table is closed and cited (`LazyFrame::pivot`'s `on_columns`, which
//! Polars only reads and validates as errors).
use crate::families::{ArgSite, Family, Listed, State};
use crate::model::Callable;
use crate::ty::Ty;
use crate::world::World;
use crate::world::mapping::{Arg, Unsupported};

const ARC: &str = "alloc::sync::Arc";

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArcArgument {
	/// The callable's canonical path.
	pub(crate) path: String,
	/// The parameter's name.
	pub(crate) param: String,
	/// The `Arc`'s wrapped, `Clone` target.
	pub(crate) target: String,
	pub(crate) cite: String,
}

/// The table, fail closed: each row cited and unique; its callable present
/// exactly once with the named parameter taking `Arc<target>` by value; the
/// target wrapped and `Clone`.
pub(crate) fn validate(
	world: &World,
	rows: &[ArcArgument],
	callables: &[Callable],
) -> Result<(), String> {
	let mut seen = std::collections::BTreeSet::new();
	for r in rows {
		if r.cite.trim().is_empty() {
			return Err(format!("{}: no citation", r.path));
		}
		if !seen.insert((r.path.clone(), r.param.clone())) {
			return Err(format!("{} `{}`: listed twice", r.path, r.param));
		}
		if !world.wrappers.contains_key(&r.target) {
			return Err(format!("{}: target {} is not wrapped", r.path, r.target));
		}
		if !world.clonable.contains(&r.target) {
			return Err(format!("{}: target {} is not Clone", r.path, r.target));
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
		let want = format!("{ARC}<{}>", r.target);
		match found[0].params.iter().find(|p| p.name == r.param) {
			Some(p) if p.ty_canonical == want => {}
			Some(p) => {
				return Err(format!(
					"{} `{}`: takes {}, not {want} by value",
					r.path, r.param, p.ty_canonical
				));
			}
			None => return Err(format!("{}: no parameter `{}`", r.path, r.param)),
		}
	}
	Ok(())
}

/// Every listed row was generated: an unused row is refused.
pub(crate) fn check_used(rows: &[ArcArgument], generated: &[&str]) -> Result<(), String> {
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

/// The argument: the script's wrapped target, borrowed; Polars gets a shared
/// snapshot of it.
pub(crate) fn arg(world: &World, target: &str) -> Option<Arg> {
	let w = world.wrappers.get(target)?;
	Some(Arg {
		rust_ty: format!("&{}", w.rust),
		conv: "std::sync::Arc::new(__ARG__.0.clone())".into(),
		fallible: false,
		doc: format!("{} (shared with Polars as a snapshot)", w.rune_name),
		pre: vec![],
		borrow: 0,
		owned: None,
		shape: format!("W:{target}"),
		vm_check: None,
	})
}

pub(crate) struct ArcArgumentFamily;
pub(crate) static ARC_ARGUMENT: ArcArgumentFamily = ArcArgumentFamily;

impl Family for ArcArgumentFamily {
	fn name(&self) -> &'static str {
		"arc_argument"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		match world
			.release
			.families
			.arc_arguments
			.iter()
			.find(|r| r.path == c.canonical_path)
		{
			Some(r) => Listed::Active(State::Two(r.param.clone(), r.target.clone())),
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
		let Some((param, target)) = state.two() else {
			return Ok(None);
		};
		match t {
			Ty::Path { path, args }
				if path == ARC
					&& name == param
					&& args.len() == 1
					&& args[0].render() == target =>
			{
				let mut a = arg(world, &target).ok_or(Unsupported("arc argument", t.render()))?;
				a.conv = a.conv.replace("__ARG__", name);
				Ok(Some(a))
			}
			_ => Ok(None),
		}
	}
}

/// Record 0127: the table refuses what it must, and the argument is the
/// script's wrapped value, borrowed, passed as an `Arc` of its clone.
pub(crate) fn arc_arguments_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	const FRAME: &str = "polars_core::frame::dataframe::DataFrame";
	let sup = |path: &str, derived: Vec<String>| Supporting {
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
		derived,
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let callable = |path: &str, ty: &str| -> Callable {
		serde_json::from_value(serde_json::json!({
			"key": path, "kind": "inherent", "krate": "polars_lazy", "owner": "polars_lazy::frame::LazyFrame", "name": "pivot",
			"canonical_path": path, "found_paths": [], "crate_paths": [], "receiver": "self",
			"params": [{"name": "on_columns", "ty": ty, "ty_canonical": ty}],
			"ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
			"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
			"owner_generic": false, "is_unsafe": false, "is_async": false, "deprecated": false,
			"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
			"bucket": "mechanical", "rules": []
		}))
		.unwrap()
	};
	let arc = format!("{ARC}<{FRAME}>");
	let calls = vec![
		callable("x::pivot", &arc),
		callable("x::borrowed", &format!("&{arc}")),
	];
	let inv = Inventory {
		callables: calls.clone(),
		provenance: None,
		supporting: vec![
			sup(FRAME, vec!["Clone".into()]),
			sup("polars_core::x::NoClone", vec![]),
		],
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into(), "polars_lazy".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let w = World::new(&inv, &release, &["mechanical"]);
	let row = |path: &str, param: &str, target: &str, cite: &str| ArcArgument {
		path: path.into(),
		param: param.into(),
		target: target.into(),
		cite: cite.into(),
	};
	assert!(validate(&w, &[row("x::pivot", "on_columns", FRAME, "c")], &calls).is_ok());
	let refused = |r: ArcArgument, want: &str| {
		let e = validate(&w, &[r], &calls).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	refused(row("x::pivot", "on_columns", FRAME, " "), "no citation");
	refused(
		row("x::pivot", "on_columns", "polars_core::x::Unwrapped", "c"),
		"is not wrapped",
	);
	refused(
		row("x::pivot", "on_columns", "polars_core::x::NoClone", "c"),
		"is not Clone",
	);
	refused(
		row("x::borrowed", "on_columns", FRAME, "c"),
		"not alloc::sync::Arc",
	);
	refused(row("x::pivot", "other", FRAME, "c"), "no parameter `other`");
	refused(row("x::nowhere", "on_columns", FRAME, "c"), "0 callables");
	assert!(
		check_used(&[row("x::pivot", "on_columns", FRAME, "c")], &[])
			.unwrap_err()
			.contains("not generated")
	);
	let a = arg(&w, FRAME).unwrap();
	assert!(
		a.rust_ty.starts_with('&') && a.conv == "std::sync::Arc::new(__ARG__.0.clone())",
		"{} {}",
		a.rust_ty,
		a.conv
	);
	println!("arc arguments self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn arc_arguments() {
		super::arc_arguments_self_test();
	}
}
