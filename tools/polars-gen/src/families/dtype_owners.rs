//! Record 0116 (rule 3): generic owners with no alias, instantiated per
//! dtype from a release listing. Each listed (owner, type) becomes one
//! synthetic wrapper that the instantiation census treats like an alias
//! wrapper: its methods are decided by the same applicability proof, pair
//! by pair, and bound on that wrapper.
use crate::text::sanitize;
use crate::ty::last;
use crate::world::proof::{is_param, split_head};
use crate::world::{World, Wrapper, spell};

/// One listed owner and its dtype table.
#[derive(serde::Deserialize, Clone)]
pub(crate) struct DtypeInstantiation {
	/// The generic owner's canonical path.
	pub(crate) owner: String,
	/// Its one type parameter, as the impl heads name it.
	pub(crate) generic: String,
	/// The canonical dtypes, one wrapper each.
	pub(crate) types: Vec<String>,
	/// The Rune name of each instantiation, in `types`' order.
	pub(crate) names: Vec<String>,
	pub(crate) cite: String,
}

/// The synthetic wrappers of every listing, or the first malformed entry's
/// reason: an owner that is not a generic struct in the inventory, a
/// generic that is not the owner's one parameter, an unknown type, a name
/// count that is not the type count, or no citation.
pub(crate) fn synthetic_wrappers(world: &World) -> Result<Vec<(String, Wrapper)>, String> {
	let mut out = Vec::new();
	for d in &world.release.families.dtype_instantiations {
		if d.cite.trim().is_empty() {
			return Err(format!("{}: no citation", d.owner));
		}
		let Some(o) = world.types.get(&d.owner) else {
			return Err(format!("{}: not in the inventory", d.owner));
		};
		if !o.generic || !matches!(o.kind.as_str(), "struct" | "enum") {
			return Err(format!("{}: not a generic struct or enum", d.owner));
		}
		// record 0118: a lifetime-bearing owner (`JsonReader<'a, R>`) cannot be
		// a wrapper's owned value
		if o.lifetime {
			return Err(format!(
				"{}: carries a lifetime parameter, which a wrapper cannot own",
				d.owner
			));
		}
		// the listed generic is the owner's one type parameter, as its
		// recorded generic impl heads name it (a specialized head, all of
		// whose arguments are concrete, instantiates nothing and is skipped)
		let heads: Vec<(String, Vec<String>)> = world
			.owner_heads
			.get(&d.owner)
			.into_iter()
			.flatten()
			.map(|h| split_head(h))
			.filter(|(_, args)| args.iter().any(|a| is_param(a)))
			.collect();
		if heads.is_empty() {
			return Err(format!("{}: no recorded generic impl head", d.owner));
		}
		if let Some((_, args)) = heads
			.iter()
			.find(|(_, args)| args.len() != 1 || args[0] != d.generic)
		{
			return Err(format!(
				"{}: generic `{}` is not the owner's one parameter (a recorded head has <{}>)",
				d.owner,
				d.generic,
				args.join(", ")
			));
		}
		if d.names.len() != d.types.len() || d.types.is_empty() {
			return Err(format!(
				"{}: {} names for {} types",
				d.owner,
				d.names.len(),
				d.types.len()
			));
		}
		let owner_spell =
			spell_type(world, &d.owner).ok_or_else(|| format!("{}: no public path", d.owner))?;
		for (t, name) in d.types.iter().zip(&d.names) {
			// record 0118: the in-memory source and sink, spelled by the adapter
			let io_spell = crate::families::std_facts::io_type_spelling(t);
			if io_spell.is_none() && !world.types.contains_key(t) {
				return Err(format!("{}: type {t} not in the inventory", d.owner));
			}
			let t_spell = match io_spell {
				Some(s) => s.to_string(),
				None => spell_type(world, t)
					.ok_or_else(|| format!("{}: type {t} has no public path", d.owner))?,
			};
			let identity = format!("{}<{t}>", d.owner);
			out.push((
				identity.clone(),
				Wrapper {
					rust: format!("W_{}_{}", sanitize(&d.owner), sanitize(t)),
					spell: format!("{owner_spell}<{t_spell}>"),
					rune_item: "::polars".into(),
					rune_name: name.clone(),
					hand: false,
					identity: identity.clone(),
					rule: "alias",
					aliases: vec![identity],
					base: Some(d.owner.clone()),
				},
			));
		}
	}
	Ok(out)
}

/// The public spelling of a type, as the wrappers spell theirs.
fn spell_type(world: &World, path: &str) -> Option<String> {
	let s = world.types.get(path)?;
	spell(
		&s.found_paths,
		&s.crate_paths,
		last(path),
		&world.ambiguous_prelude,
	)
}

/// Record 0116 (rule 3, review): a listed instantiation is useful only if a
/// script can reach a result from it. After emission, every listed wrapper
/// must have at least one generated constructor (no receiver, returning the
/// wrapper) and one generated method returning something other than the
/// wrapper or unit; otherwise the listing is refused by name.
pub(crate) fn check_reachable(world: &World, entries: &[crate::emit::Entry]) -> Result<(), String> {
	// every listed identity, with the type its listing binds the generic to
	let listed: Vec<(String, String, String)> = world
		.release
		.families
		.dtype_instantiations
		.iter()
		.flat_map(|d| {
			d.types
				.iter()
				.map(|t| (format!("{}<{t}>", d.owner), d.generic.clone(), t.clone()))
				.collect::<Vec<_>>()
		})
		.collect();
	let mut calls: std::collections::BTreeMap<&str, Vec<(String, String)>> = Default::default();
	for (identity, generic, t) in &listed {
		let subst: std::collections::BTreeMap<String, String> =
			[(generic.clone(), t.clone())].into();
		for e in entries.iter().filter(|e| e.status == "generated") {
			for b in &e.bindings {
				if b.receiver.as_deref() != Some(identity.as_str()) {
					continue;
				}
				if let Some(info) = b.info.as_ref().or(e.oracle.as_ref()) {
					let ret = info.ret_canonical.clone().unwrap_or_else(|| "()".into());
					calls.entry(identity).or_default().push((
						info.receiver.clone(),
						crate::world::proof::subst_params(&ret, &subst),
					));
				}
			}
		}
	}
	let ids: Vec<&str> = listed.iter().map(|(id, _, _)| id.as_str()).collect();
	let constructed = constructed(&ids, &calls);
	let mut refused = Vec::new();
	for (identity, _, _) in &listed {
		let (_, result) = reachable(
			identity,
			calls.get(identity.as_str()).map_or(&[][..], |v| v),
		);
		let construct = constructed.contains(identity.as_str());
		if !(construct && result) {
			refused.push(format!(
				"{identity}: no script path from construction to a result (constructor {construct}, result {result})"
			));
		}
	}
	if refused.is_empty() {
		Ok(())
	} else {
		Err(refused.join("; "))
	}
}

/// Record 0118: the listed wrappers a script can construct: by a
/// constructor of their own, or by a method of a constructed listed wrapper
/// that returns them (`CsvWriter<Sink>::batched` gives `BatchedWriter<Sink>`),
/// to a fixpoint. `calls` are each wrapper's (receiver, substituted return).
pub(crate) fn constructed<'a>(
	ids: &[&'a str],
	calls: &std::collections::BTreeMap<&str, Vec<(String, String)>>,
) -> std::collections::BTreeSet<&'a str> {
	let own = |id: &str| calls.get(id).map_or(&[][..], |v| v);
	let mut done: std::collections::BTreeSet<&str> = ids
		.iter()
		.copied()
		.filter(|id| reachable(id, own(id)).0)
		.collect();
	loop {
		let more: Vec<&str> = ids
			.iter()
			.copied()
			.filter(|id| !done.contains(id))
			.filter(|id| {
				done.iter().any(|from| {
					own(from).iter().any(|(recv, ret)| {
						recv != "none"
							&& (ret == id
								|| ret
									.strip_prefix("polars_error::PolarsResult<")
									.and_then(|r| r.strip_suffix('>'))
									== Some(*id))
					})
				})
			})
			.collect();
		if more.is_empty() {
			return done;
		}
		done.extend(more);
	}
}

/// Whether the generated calls on `identity` (receiver, return) include a
/// constructor (no receiver, returning the wrapper) and a result (a method
/// returning something other than the wrapper or unit).
pub(crate) fn reachable(identity: &str, calls: &[(String, String)]) -> (bool, bool) {
	let own = |r: &str| r == "Self" || r == identity;
	let construct = calls.iter().any(|(recv, ret)| recv == "none" && own(ret));
	let result = calls
		.iter()
		.any(|(recv, ret)| recv != "none" && ret != "()" && !own(ret));
	(construct, result)
}

/// Record 0116 (rule 3) controls: a listing is validated before any wrapper
/// exists, and a listed wrapper must be reachable from construction to a result.
pub(crate) fn dtype_owners_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	let sup = |path: &str, generic: bool| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::prelude::{}", last(path))],
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
	let owner = "polars_core::b::Builder";
	let (a, b) = (
		"polars_core::datatypes::AType",
		"polars_core::datatypes::BType",
	);
	// one inherent method whose impl head names the owner's parameter `T`,
	// and one on a specialized (concrete) head, which instantiates nothing
	let method = |key: &str, head: &str| -> crate::model::Callable {
		serde_json::from_value(serde_json::json!({
			"key": key, "kind": "inherent", "krate": "polars_core", "owner": owner, "name": key,
			"canonical_path": format!("{owner}::{key}"), "found_paths": [], "crate_paths": [],
			"receiver": "&self", "params": [], "ret": null, "ret_canonical": "bool",
			"generics_canonical": [], "impl_for": null, "impl_bounds": [], "impl_head": head,
			"impl_where": [], "impl_assoc": [], "docs_first": null, "owner_generic": true,
			"is_unsafe": false, "is_async": false, "deprecated": false, "hidden": false,
			"implementors": [], "trait_reachable": false, "derived": false, "bucket": "generic", "rules": []
		}))
		.unwrap()
	};
	let inv = Inventory {
		callables: vec![
			method("m", &format!("{owner}<T>")),
			method("n", &format!("{owner}<{a}>")),
		],
		supporting: vec![
			sup(owner, true),
			sup(a, false),
			sup(b, false),
			sup("polars_core::b::Plain", false),
		],
		provenance: None,
	};
	let listing =
		|owner: &str, types: Vec<&str>, names: Vec<&str>, cite: &str| DtypeInstantiation {
			owner: owner.into(),
			generic: "T".into(),
			types: types.into_iter().map(String::from).collect(),
			names: names.into_iter().map(String::from).collect(),
			cite: cite.into(),
		};
	let world_with = |d: DtypeInstantiation| {
		let mut families = FamilyTables::default();
		families.dtype_instantiations.push(d);
		let release = Release {
			name: "t".into(),
			source: "t".into(),
			provenance: ReleaseProvenance::default(),
			instantiation: InstantiationScope::default(),
			api_crates: vec!["polars_core".into()],
			unordered: vec![],
			excluded_oracle: vec![],
			refused: vec![],
			families,
		};
		release
	};
	// well formed: one wrapper per type, named as listed, spelled with its type
	let r = world_with(listing(
		owner,
		vec![a, b],
		vec!["ABuilder", "BBuilder"],
		"c",
	));
	let world = World::new(&inv, &r, &["mechanical"]);
	let ws = synthetic_wrappers(&world).unwrap();
	assert_eq!(ws.len(), 2);
	assert_eq!(ws[0].0, format!("{owner}<{a}>"));
	assert_eq!(
		(ws[0].1.rune_name.as_str(), ws[0].1.rule),
		("ABuilder", "alias")
	);
	assert!(
		ws[0]
			.1
			.spell
			.ends_with("Builder<polars_core::datatypes::AType>"),
		"the owner's spelling applied to the type's: {}",
		ws[0].1.spell
	);
	assert!(
		world.wrappers.contains_key(&format!("{owner}<{b}>"))
			&& world.types.contains_key(&format!("{owner}<{b}>"))
	);
	// malformed listings are refused before any wrapper, each by name
	for (d, want) in [
		(
			listing(owner, vec![a, b], vec!["ABuilder"], "c"),
			"1 names for 2 types",
		),
		(
			listing(owner, vec!["polars_core::datatypes::Nope"], vec!["N"], "c"),
			"not in the inventory",
		),
		(
			listing(owner, vec![a], vec!["ABuilder"], " "),
			"no citation",
		),
		(
			listing("polars_core::b::Plain", vec![a], vec!["P"], "c"),
			"not a generic struct",
		),
		// review of 0116: the listed generic must be the owner's recorded parameter
		(
			DtypeInstantiation {
				generic: "Bogus".into(),
				..listing(owner, vec![a], vec!["ABuilder"], "c")
			},
			"generic `Bogus` is not the owner's one parameter",
		),
	] {
		let mut families = FamilyTables::default();
		families.dtype_instantiations.push(d);
		let mut world = World::new(
			&inv,
			&Release {
				families: FamilyTables::default(),
				..r.clone()
			},
			&["mechanical"],
		);
		world.release.families = families;
		let err = synthetic_wrappers(&world)
			.err()
			.expect("a malformed listing");
		assert!(err.contains(want), "{err}");
	}
	// a two-parameter generic head is refused whatever the listing says
	let mut w2 = World::new(
		&inv,
		&Release {
			families: FamilyTables::default(),
			..r.clone()
		},
		&["mechanical"],
	);
	w2.owner_heads
		.get_mut(owner)
		.unwrap()
		.insert(format!("{owner}<T, U>"));
	w2.release
		.families
		.dtype_instantiations
		.push(listing(owner, vec![a], vec!["ABuilder"], "c"));
	assert!(
		synthetic_wrappers(&w2)
			.err()
			.unwrap()
			.contains("a recorded head has <T, U>")
	);
	// reachability: a constructor alone, or a result alone, is not a path
	let id = format!("{owner}<{a}>");
	let c = |r: &str, t: &str| (r.to_string(), t.to_string());
	assert_eq!(reachable(&id, &[c("none", "Self")]), (true, false));
	assert_eq!(
		reachable(&id, &[c("&mut self", "()"), c("&self", &id)]),
		(false, false)
	);
	assert_eq!(
		reachable(
			&id,
			&[
				c("none", &id),
				c("&mut self", "polars_core::datatypes::ListChunked")
			]
		),
		(true, true)
	);
	// record 0118: construction through a constructed wrapper's method, to a
	// fixpoint; never through an unconstructed one
	let (w, bw, orphan) = ("W<Sink>", "BW<Sink>", "O<Sink>");
	let calls: std::collections::BTreeMap<&str, Vec<(String, String)>> = [
		(
			w,
			vec![
				c("none", "Self"),
				c("self", "polars_error::PolarsResult<BW<Sink>>"),
			],
		),
		(bw, vec![c("&mut self", "polars_error::PolarsResult<()>")]),
		(orphan, vec![c("&self", "bool")]),
	]
	.into();
	let got = constructed(&[w, bw, orphan], &calls);
	assert!(got.contains(w) && got.contains(bw), "{got:?}");
	assert!(
		!got.contains(orphan),
		"no constructor and no constructed parent"
	);
	let calls2: std::collections::BTreeMap<&str, Vec<(String, String)>> = [
		(w, vec![c("self", "polars_error::PolarsResult<BW<Sink>>")]),
		(bw, vec![c("&mut self", "polars_error::PolarsResult<()>")]),
	]
	.into();
	assert!(
		constructed(&[w, bw], &calls2).is_empty(),
		"a parent that is not constructed constructs nothing"
	);
	// record 0118: an in-memory I/O type instantiates without an inventory
	// record, spelled by the adapter; a lifetime-bearing owner is refused
	let cursor = crate::families::std_facts::CURSOR;
	let mut w3 = World::new(
		&inv,
		&Release {
			families: FamilyTables::default(),
			..r.clone()
		},
		&["mechanical"],
	);
	w3.release.families.dtype_instantiations.push(listing(
		owner,
		vec![cursor],
		vec!["CBuilder"],
		"c",
	));
	let ws = synthetic_wrappers(&w3).unwrap();
	assert!(
		ws[0].1.spell.ends_with("Builder<std::io::Cursor<Vec<u8>>>"),
		"{}",
		ws[0].1.spell
	);
	w3.types.get_mut(owner).unwrap().lifetime = true;
	assert!(
		synthetic_wrappers(&w3)
			.err()
			.unwrap()
			.contains("carries a lifetime parameter")
	);
	// record 0118: a head parameter re-stated in a method where-clause is the
	// head's, moved to the proof; a true function generic is untouched
	let mut m = method("m", &format!("{owner}<T>"));
	m.generics_canonical = vec![
		("T".into(), "core::io::write::Write".into()),
		("F".into(), "core::marker::Send".into()),
	];
	let moved = crate::census::head_bound_generics(&m);
	assert_eq!(
		moved.impl_where,
		vec!["T: core::io::write::Write".to_string()]
	);
	assert_eq!(
		moved.generics_canonical,
		vec![("F".to_string(), "core::marker::Send".to_string())]
	);
	// record 0118: a call on a `Sink` instantiation is one sink operation, its
	// fallible conversions hoisted before it; any other call is not wrapped
	let sink = crate::families::std_facts::SINK;
	let r4 = world_with(listing(
		owner,
		vec![sink, a],
		vec!["SBuilder", "ABuilder"],
		"c",
	));
	let w4 = World::new(&inv, &r4, &["mechanical"]);
	let mut m = method("m", &format!("{owner}<T>"));
	m.params = vec![crate::model::Param {
		name: "sep".into(),
		ty: "u8".into(),
		ty_canonical: "u8".into(),
	}];
	let emit = |identity: &str| {
		let mut out = crate::emit::Emitted {
			from_names: Default::default(),
			functions: String::new(),
			registrations: vec![],
			catalogue: vec![],
			entries: vec![],
			taken: Default::default(),
			fn_index: 0,
			frozen_ids: Default::default(),
		};
		crate::emit::callable::emit_method(&w4, &mut out, &m, identity, None, false);
		assert_eq!(
			out.entries[0].status, "generated",
			"{:?}",
			out.entries[0].reason
		);
		out.functions
	};
	let f = emit(&format!("{owner}<{sink}>"));
	let (hoist, op) = (
		f.find("let __arg1 = support::narrow::<u8>(sep").expect(&f),
		f.find("support::sink_op_infallible(").expect(&f),
	);
	assert!(hoist < op, "the conversion runs before the operation: {f}");
	let g = emit(&format!("{owner}<{a}>"));
	assert!(!g.contains("sink_op"), "not a sink call: {g}");
	println!("dtype-owners self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn dtype_owners() {
		super::dtype_owners_self_test();
	}
}
