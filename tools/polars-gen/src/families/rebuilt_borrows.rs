//! Record 0126: a rebuilt borrow. A listed Polars type that borrows a frame
//! (`GroupBy<'a> { df: &'a DataFrame, .. }`) is wrapped by a hand-written
//! snapshot that owns what the borrow holds (`support::GroupBySnapshot`), and
//! each call rebuilds the borrowed value for that call only
//! (`snapshot.view()`). The release lists, closed and cited, the owner, its
//! one borrowed field, the snapshot type and every way a script creates or
//! consumes one (a recipe in the snapshot type, making Polars' own public
//! calls). Every other `&self` method of the owner is generated with the
//! view as its receiver; any other use of the owner, anywhere, is refused.
use crate::families::{Family, Listed, State};
use crate::model::Callable;
use crate::ty::last;
use crate::world::World;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct RebuiltBorrow {
	/// The borrowing Polars type's canonical path.
	pub(crate) owner: String,
	/// Its one borrowed field (`df`), which must be its only public field.
	pub(crate) field: String,
	/// The hand-written snapshot type, as spelled from the generated functions.
	pub(crate) snapshot: String,
	/// The Polars type as spelled for a call (`p::GroupBy`).
	pub(crate) polars: String,
	/// Each constructor and consuming method, with its snapshot recipe.
	pub(crate) recipes: Vec<Recipe>,
	pub(crate) cite: String,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Recipe {
	/// The callable's canonical path.
	pub(crate) path: String,
	/// The snapshot's function of the same contract.
	pub(crate) recipe: String,
}

/// The table against the inventory, fail closed: each row cited; its owner a
/// struct with a lifetime and no type parameter, whose only public field is
/// the listed borrowed field; each recipe's callable present exactly once and
/// either returning the owner or taking it by value.
pub(crate) fn validate(
	world: &World,
	rows: &[RebuiltBorrow],
	callables: &[Callable],
) -> Result<(), String> {
	for r in rows {
		if r.cite.trim().is_empty() {
			return Err(format!("{}: no citation", r.owner));
		}
		let Some(s) = world.types.get(&r.owner) else {
			return Err(format!("{}: not in the inventory", r.owner));
		};
		if s.kind != "struct" || !s.lifetime || s.generic {
			return Err(format!(
				"{}: not a struct with a lifetime and no type parameter",
				r.owner
			));
		}
		let fields: Vec<&str> = s.fields_canonical.iter().map(|(n, _)| n.as_str()).collect();
		if fields != [r.field.as_str()] {
			return Err(format!(
				"{}: public fields {fields:?}, want only the borrowed `{}`",
				r.owner, r.field
			));
		}
		if !s.fields_canonical[0].1.starts_with('&') {
			return Err(format!("{}: `{}` is not a borrow", r.owner, r.field));
		}
		let mut seen = std::collections::BTreeSet::new();
		for x in &r.recipes {
			if !seen.insert(&x.path) {
				return Err(format!("{}: listed twice", x.path));
			}
			let found: Vec<&Callable> = callables
				.iter()
				.filter(|c| c.canonical_path == x.path)
				.collect();
			if found.len() != 1 {
				return Err(format!(
					"{}: {} callables with this path, want exactly one",
					x.path,
					found.len()
				));
			}
			let c = found[0];
			let returns = c
				.ret_canonical
				.as_deref()
				.is_some_and(|t| mentions(t, &r.owner));
			let consumes = c.owner == r.owner && c.receiver == "self";
			let borrows = c.owner == r.owner && c.receiver == "&self";
			if !(returns || consumes || borrows) {
				return Err(format!(
					"{}: neither creates, consumes nor lends from {}",
					x.path, r.owner
				));
			}
		}
	}
	Ok(())
}

/// Every listed recipe was generated: an unused row is refused.
pub(crate) fn check_used(rows: &[RebuiltBorrow], generated: &[&str]) -> Result<(), String> {
	for r in rows {
		for x in &r.recipes {
			if !generated.contains(&x.path.as_str()) {
				return Err(format!(
					"{}: listed recipe `{}`, but the callable was not generated",
					x.path, x.recipe
				));
			}
		}
	}
	Ok(())
}

fn mentions(ty: &str, owner: &str) -> bool {
	ty.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
		.any(|t| t == owner)
}

/// The listing for one callable:
/// - a recipe: `Two("recipe", snapshot::recipe)`;
/// - a `&self` method of the owner that neither takes nor returns the owner
///   (nor lends from it): `Two("view", polars)`, the receiver rebuilt per call;
/// - any other callable that mentions an owner: refused by name.
pub(crate) fn listing(world: &World, c: &Callable) -> Listed {
	// a release refusal names its own cited reason (`sliced`)
	if world
		.release
		.refused
		.iter()
		.any(|r| r.path == c.canonical_path)
	{
		return Listed::Unlisted;
	}
	for r in &world.release.families.rebuilt_borrows {
		if let Some(x) = r.recipes.iter().find(|x| x.path == c.canonical_path) {
			return Listed::Active(State::Two(
				"recipe".into(),
				format!("{}::{}", r.snapshot, x.recipe),
			));
		}
		let in_params = c.params.iter().any(|p| mentions(&p.ty_canonical, &r.owner));
		let in_ret = c
			.ret_canonical
			.as_deref()
			.is_some_and(|t| mentions(t, &r.owner));
		let lends = c
			.ret_canonical
			.as_deref()
			.is_some_and(|t| t.starts_with('&'));
		if c.owner == r.owner {
			// Polars deprecates the eager aggregations at 0.55.2 ("use
			// polars.lazy aggregations") and removes them by v2: not bound
			if c.deprecated {
				return Listed::Refused(
					"rebuilt borrow",
					format!(
						"{}: deprecated in Polars (use the lazy aggregations); removed by v2",
						c.name
					),
				);
			}
			if c.receiver == "&self" && !in_params && !in_ret && !lends {
				return Listed::Active(State::Two("view".into(), r.polars.clone()));
			}
			return Listed::Refused(
				"rebuilt borrow",
				format!(
					"{}: not a listed use of {} (receiver {}); only the listed recipes create or consume one ({})",
					c.name,
					last(&r.owner),
					c.receiver,
					r.cite
				),
			);
		}
		if in_params || in_ret {
			return Listed::Refused(
				"rebuilt borrow",
				format!(
					"{} takes or returns {}, and is not a listed recipe ({})",
					c.name,
					last(&r.owner),
					r.cite
				),
			);
		}
	}
	Listed::Unlisted
}

/// The receiver and callee for a listed callable, from its state.
pub(crate) fn route(state: &State, rust_name: &str) -> Option<(Option<String>, String)> {
	let (kind, what) = state.two()?;
	match kind.as_str() {
		// the borrowed value, rebuilt for this call from the snapshot
		"view" => Some((
			Some("&this.0.view()".into()),
			format!("<{what}>::{rust_name}"),
		)),
		// the snapshot's recipe, with the receiver as generated
		"recipe" => Some((None, what)),
		_ => None,
	}
}

/// The owner's wrapper: the snapshot type, under the owner's own Rune name.
pub(crate) fn wrappers(world: &World) -> Vec<(String, crate::world::Wrapper)> {
	world
		.release
		.families
		.rebuilt_borrows
		.iter()
		.map(|r| {
			let name = last(&r.owner).to_string();
			(
				r.owner.clone(),
				crate::world::Wrapper {
					rust: format!("W_{}", crate::text::sanitize(&r.owner)),
					spell: format!("super::{}", r.snapshot),
					rune_item: "::polars".into(),
					rune_name: name,
					hand: false,
					identity: r.owner.clone(),
					rule: "rebuilt borrow",
					aliases: vec![r.owner.clone()],
					base: None,
				},
			)
		})
		.collect()
}

pub(crate) struct RebuiltBorrowFamily;
pub(crate) static REBUILT_BORROW: RebuiltBorrowFamily = RebuiltBorrowFamily;

impl Family for RebuiltBorrowFamily {
	fn name(&self) -> &'static str {
		"rebuilt_borrow"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		if world.release.families.rebuilt_borrows.is_empty() {
			return Listed::Unlisted;
		}
		listing(world, c)
	}
}

/// Record 0126: the table refuses what it must; a listed recipe, a `&self`
/// method and every other use of the owner are routed or refused as the
/// plan says.
pub(crate) fn rebuilt_borrows_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	const OWNER: &str = "polars_core::frame::group_by::GroupBy";
	const FRAME: &str = "polars_core::frame::dataframe::DataFrame";
	let sup = |path: &str, lifetime: bool, fields: Vec<(String, String)>| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".to_string(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::prelude::{}", last(path))],
		crate_paths: vec![path.to_string()],
		public_fields: fields.len(),
		fields_canonical: fields,
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime,
		hidden: false,
		derived: vec![],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let callable = |path: &str,
	                owner: &str,
	                receiver: &str,
	                params: serde_json::Value,
	                ret: Option<&str>,
	                deprecated: bool|
	 -> Callable {
		serde_json::from_value(serde_json::json!({
			"key": path, "kind": "inherent", "krate": "polars_core", "owner": owner, "name": last(path),
			"canonical_path": path, "found_paths": [], "crate_paths": [], "receiver": receiver,
			"params": params, "ret": ret, "ret_canonical": ret, "generics_canonical": [], "impl_for": null,
			"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
			"owner_generic": false, "is_unsafe": false, "is_async": false, "deprecated": deprecated,
			"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
			"bucket": "mechanical", "rules": []
		}))
		.unwrap()
	};
	let none = serde_json::json!([]);
	let group_by = callable(
		&format!("{FRAME}::group_by"),
		FRAME,
		"&self",
		none.clone(),
		Some(&format!("polars_error::PolarsResult<{OWNER}>")),
		false,
	);
	let count = callable(
		&format!("{OWNER}::count"),
		OWNER,
		"&self",
		none.clone(),
		Some(&format!("polars_error::PolarsResult<{FRAME}>")),
		false,
	);
	let mean = callable(
		&format!("{OWNER}::mean"),
		OWNER,
		"&self",
		none.clone(),
		Some(&format!("polars_error::PolarsResult<{FRAME}>")),
		true,
	);
	let select = callable(
		&format!("{OWNER}::select"),
		OWNER,
		"self",
		none.clone(),
		Some(OWNER),
		false,
	);
	let sliced = callable(
		&format!("{OWNER}::sliced"),
		OWNER,
		"self",
		none.clone(),
		Some(OWNER),
		false,
	);
	let new = callable(
		&format!("{OWNER}::new"),
		OWNER,
		"none",
		serde_json::json!([{"name": "df", "ty": format!("&{FRAME}"), "ty_canonical": format!("&{FRAME}")}]),
		Some(OWNER),
		false,
	);
	let taker = callable(
		"polars_x::Other::take_gb",
		"polars_x::Other",
		"&self",
		serde_json::json!([{"name": "g", "ty": OWNER, "ty_canonical": OWNER}]),
		None,
		false,
	);
	let unrelated = callable(
		&format!("{FRAME}::height"),
		FRAME,
		"&self",
		none.clone(),
		Some("usize"),
		false,
	);
	let calls = vec![
		group_by.clone(),
		count.clone(),
		mean.clone(),
		select.clone(),
		sliced.clone(),
		new.clone(),
		taker.clone(),
		unrelated.clone(),
	];
	let borrowed = vec![("df".to_string(), format!("&{FRAME}"))];
	let inv = Inventory {
		callables: calls.clone(),
		provenance: None,
		supporting: vec![
			sup(OWNER, true, borrowed.clone()),
			sup(FRAME, false, vec![]),
			sup("polars_x::Plain", false, vec![]),
			sup(
				"polars_x::Two",
				true,
				vec![
					("df".into(), format!("&{FRAME}")),
					("x".into(), "i64".into()),
				],
			),
			sup("polars_x::Owned", true, vec![("df".into(), FRAME.into())]),
		],
	};
	let row = |owner: &str, recipes: &[(&str, &str)], cite: &str| RebuiltBorrow {
		owner: owner.into(),
		field: "df".into(),
		snapshot: "support::GroupBySnapshot".into(),
		polars: "p::GroupBy".into(),
		recipes: recipes
			.iter()
			.map(|(p, r)| Recipe {
				path: p.to_string(),
				recipe: r.to_string(),
			})
			.collect(),
		cite: cite.into(),
	};
	let recipes = [
		(group_by.canonical_path.as_str(), "group_by"),
		(select.canonical_path.as_str(), "select"),
	];
	let good = row(OWNER, &recipes, "c");
	let mut families = FamilyTables::default();
	families.rebuilt_borrows = vec![good.clone()];
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into(), "polars_x".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families,
	};
	let w = World::new(&inv, &release, &["mechanical"]);
	assert!(validate(&w, &[good.clone()], &calls).is_ok());
	let refused = |r: RebuiltBorrow, want: &str| {
		let e = validate(&w, &[r], &calls).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	// the controls: no citation; an owner without a lifetime; a second public
	// field; an owned (not borrowed) field; a missing recipe callable; a
	// recipe that neither creates, consumes nor lends; a duplicate recipe
	refused(row(OWNER, &recipes, " "), "no citation");
	refused(
		row("polars_x::Plain", &recipes, "c"),
		"not a struct with a lifetime",
	);
	refused(
		row("polars_x::Two", &recipes, "c"),
		"want only the borrowed",
	);
	refused(row("polars_x::Owned", &recipes, "c"), "is not a borrow");
	refused(
		row(OWNER, &[("polars_x::nowhere", "x")], "c"),
		"0 callables",
	);
	refused(
		row(OWNER, &[(unrelated.canonical_path.as_str(), "height")], "c"),
		"neither creates, consumes nor lends",
	);
	refused(row(OWNER, &[recipes[0], recipes[0]], "c"), "listed twice");
	// routing: a recipe, a view, and refusals by name
	let state = |c: &Callable| match listing(&w, c) {
		Listed::Active(s) => format!("{s:?}"),
		Listed::Refused(_, why) => format!("refused: {why}"),
		Listed::Unlisted => "unlisted".into(),
	};
	assert!(
		state(&group_by).contains("recipe")
			&& state(&group_by).contains("support::GroupBySnapshot::group_by"),
		"{}",
		state(&group_by)
	);
	assert!(state(&count).contains("view"), "{}", state(&count));
	assert!(
		state(&mean).starts_with("refused: mean: deprecated"),
		"{}",
		state(&mean)
	);
	assert!(
		state(&sliced).starts_with("refused: sliced: not a listed use"),
		"{}",
		state(&sliced)
	);
	assert!(
		state(&new).starts_with("refused: new: not a listed use"),
		"{}",
		state(&new)
	);
	assert!(
		state(&taker).starts_with("refused: take_gb takes or returns GroupBy"),
		"{}",
		state(&taker)
	);
	assert_eq!(state(&unrelated), "unlisted");
	let (recv, callee) = route(&State::Two("view".into(), "p::GroupBy".into()), "count").unwrap();
	assert_eq!(
		(recv.as_deref(), callee.as_str()),
		(Some("&this.0.view()"), "<p::GroupBy>::count")
	);
	let (recv, callee) = route(
		&State::Two("recipe".into(), "support::GroupBySnapshot::select".into()),
		"select",
	)
	.unwrap();
	assert_eq!(
		(recv, callee.as_str()),
		(None, "support::GroupBySnapshot::select")
	);
	// an unused recipe is refused
	let e = check_used(&[good.clone()], &[group_by.canonical_path.as_str()]).unwrap_err();
	assert!(
		e.contains("select") && e.contains("was not generated"),
		"{e}"
	);
	assert!(
		check_used(
			&[good],
			&[
				group_by.canonical_path.as_str(),
				select.canonical_path.as_str()
			]
		)
		.is_ok()
	);
	println!("rebuilt borrows self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn rebuilt_borrows() {
		super::rebuilt_borrows_self_test();
	}
}
