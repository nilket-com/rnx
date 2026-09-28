//! Record 0114: the ordered dispatch chain, moved verbatim from `main`.
use crate::census::{PairRecord, census_summary, instantiation_census};
use crate::emit::callable::emit_callable;
use crate::emit::instantiations::emit_instantiations;
use crate::emit::types::emit_struct_extras;
use crate::emit::{Emitted, Entry, OracleInfo, binding_id, rune_path, signature_of};
use crate::families::conversions::{plan_from_names, resolve_duplicates};
use crate::families::generic_impls::{
	OpArm, emit_from_iter, emit_index_groups, emit_op_groups, from_iter_row, generic_impl_refusal,
	generic_op, index_arm, index_row, op_arms, op_entry, unary_generic_shape,
};
use crate::families::protocols::emit_foreign;
use crate::families::serde::serde_trait;
use crate::model::{Callable, Inventory};
use crate::release::Release;
use crate::ty;
use crate::world::World;
use crate::world::mapping::iterator_return;
use crate::world::proof::Applicability;
use std::collections::{BTreeMap, BTreeSet};

/// Every callable, in the fixed order, through the family rules; then the
/// deferred operator and `INDEX_GET` groups and the census dispositions.
pub(crate) fn generate(
	world: &World,
	inv: &Inventory,
	release: &Release,
	buckets: &Vec<&str>,
) -> (Emitted, Vec<PairRecord>) {
	let mut out = Emitted {
		from_names: plan_from_names(&inv),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
	};
	let mut callables: Vec<&Callable> = inv.callables.iter().collect();
	callables.sort_by(|a, b| {
		a.canonical_path
			.cmp(&b.canonical_path)
			.then(a.key.cmp(&b.key))
	});
	// inherent methods take names before trait methods do
	callables.sort_by_key(|c| match c.kind.as_str() {
		"inherent" => 0,
		"free_fn" => 1,
		"foreign_trait_impl" => 2,
		_ => 3,
	});
	// record 0076 gate 3: the instantiation census, before any binding is emitted
	let census = instantiation_census(&world, &inv);
	{
		let summary = census_summary(&census);
		println!(
			"instantiation census: {}",
			serde_json::to_string(&summary["by_family"]).unwrap()
		);
	}
	let mut census_by_method: BTreeMap<String, Vec<&PairRecord>> = BTreeMap::new();
	for p in &census {
		census_by_method.entry(p.key.clone()).or_default().push(p);
	}
	let census_keys: BTreeSet<String> = census.iter().map(|p| p.key.clone()).collect();
	let mut op_rows: Vec<&Callable> = Vec::new();
	let mut index_rows: Vec<&Callable> = Vec::new();
	let mut index_pending: Vec<(String, String, &'static str, String)> = Vec::new();
	let mut op_pending: Vec<OpArm> = Vec::new();
	for c in callables {
		let api = release.is_api(&c.krate);
		if c.bucket == "unsupported" || c.bucket == "unknown" {
			continue; // not eligible in 0072's terms
		}
		if api && c.kind == "inherent" && c.bucket == "generic" {
			if let Some(pairs) = census_by_method.get(&c.key) {
				emit_instantiations(&world, &mut out, c, pairs);
				continue;
			}
		}
		// record 0077: a callable in the generic bucket only because its
		// return is an iterator (rule T3) is handled by the mapping rules,
		// which materialize it or refuse it with the item named
		if api
			&& c.bucket == "generic"
			&& !c.owner_generic
			&& c.generics_canonical.is_empty()
			&& c.ret_canonical
				.as_deref()
				.is_some_and(|r| iterator_return(&ty::parse(r)).is_some())
		{
			let mut with_generic: Vec<&str> = buckets.clone();
			with_generic.push("generic");
			emit_callable(&world, &mut out, c, &with_generic);
			continue;
		}
		if !api {
			out.entries.push(Entry {
				key: c.key.clone(),
				canonical_path: c.canonical_path.clone(),
				kind: c.kind.clone(),
				bucket: c.bucket.clone(),
				status: "out_of_scope",
				fallible: None,
				signature: signature_of(c),
				execution: None,
				oracle: None,
				reason: Some("internal crate reachable through the prelude".into()),
				rune: None,
				note: None,
				bindings: vec![],
				exceptions: vec![],
				counterpart: None,
			});
			continue;
		}
		// record 0113: Index, grouped into one INDEX_GET per owner
		if index_row(c) {
			match index_arm(&world, c) {
				Ok((o, k, x)) => {
					index_rows.push(c);
					index_pending.push((c.key.clone(), o, k, x));
				}
				Err(why) => out.unsupported(c, "index", &why),
			}
			continue;
		}
		// record 0113: FromIterator constructors, by the admitted item grammar
		if from_iter_row(c) {
			emit_from_iter(&world, &mut out, c);
			continue;
		}
		// record 0113: a unary `Neg`/`Not` in the generic bucket with the
		// exact concrete shape goes to the existing unary protocol route
		if unary_generic_shape(c) {
			emit_foreign(&world, &mut out, c);
			continue;
		}
		// record 0113: generic-bucket operators are decided as a family
		if generic_op(c).is_some() {
			match op_arms(&world, c) {
				Ok(arms) => {
					op_rows.push(c);
					op_pending.extend(arms);
				}
				Err(why) => out.unsupported(c, "operator", &why),
			}
			continue;
		}
		// record 0113: every other generic-bucket trait impl gets its family's
		// named contract (serde keeps its own lane inside emit_callable)
		if serde_trait(c).is_none() {
			if let Some(why) = generic_impl_refusal(c) {
				out.unsupported(c, "0113 family", &why);
				continue;
			}
		}
		emit_callable(&world, &mut out, c, &buckets);
	}
	let op_used = emit_op_groups(&world, &mut out, op_pending);
	let index_used = emit_index_groups(&world, &mut out, &index_pending);
	for c in index_rows {
		let (_, owner, key, output) = index_pending.iter().find(|a| a.0 == c.key).unwrap().clone();
		match index_used.get(&c.key) {
			Some(Ok(())) => {
				let w = &world.wrappers[&owner];
				let info = OracleInfo {
					rune_owner: Some(rune_path(w)),
					rune_name: "[]".into(),
					receiver: "protocol".into(),
					owner: Some((owner.clone(), w.rust.clone())),
					callee: "IndexGen".into(),
					params: vec![(key.to_string(), String::new())],
					param_names: vec![],
					ret_canonical: Some(output.clone()),
					ret_rust: world.wrappers[&output].rust.clone(),
					fallible: true,
					generics: BTreeMap::new(),
					implementors: vec![],
					deref: false,
				};
				out.generated_with(
					c,
					&format!("{}[{key}]", rune_path(w)),
					Some("record 0113: INDEX_GET, the Column cloned out".into()),
					info,
				);
				let b = &mut out.entries.last_mut().unwrap().bindings[0];
				b.id = binding_id(
					&c.canonical_path,
					Some(&format!("{owner}|{}", c.key)),
					false,
				);
			}
			Some(Err(why)) => out.unsupported(c, "index", why),
			None => out.unsupported(c, "index", "no arm"),
		}
	}
	for c in op_rows {
		match op_used.get(&c.key) {
			Some(Ok(arms)) if !arms.is_empty() => op_entry(&world, &mut out, c, arms),
			Some(Err(why)) => out.unsupported(c, "operator", why),
			_ => out.unsupported(c, "operator", "no arm chosen"),
		}
	}
	emit_struct_extras(&world, &mut out, &buckets);
	resolve_duplicates(&mut out.entries);
	// every pair of the census gets exactly one disposition, emitted or not
	let mut census = census;
	{
		let mut per_key: BTreeMap<
			&str,
			(&str, Option<&str>, Vec<(&str, &str)>, Vec<(&str, &str)>),
		> = BTreeMap::new();
		for e in &out.entries {
			if !census_keys.contains(&e.key) {
				continue;
			}
			let bs: Vec<(&str, &str)> = e
				.bindings
				.iter()
				.filter(|b| b.route == "instantiation")
				.filter_map(|b| b.receiver.as_deref().map(|r| (r, b.id.as_str())))
				.collect();
			let xs: Vec<(&str, &str)> = e
				.exceptions
				.iter()
				.filter(|x| x.route == "instantiation")
				.map(|x| (x.receiver.as_str(), x.reason.as_str()))
				.collect();
			per_key.insert(e.key.as_str(), (e.status, e.reason.as_deref(), bs, xs));
		}
		let by_key: BTreeMap<&str, &Callable> =
			inv.callables.iter().map(|c| (c.key.as_str(), c)).collect();
		for p in census.iter_mut() {
			p.disposition = Some(match &p.result {
				Applicability::Rejected(r) => format!("rejected: {r}"),
				Applicability::Unresolved(r) => format!("unresolved: {r}"),
				Applicability::Proven => {
					if !p.eligible {
						let c = by_key[p.key.as_str()];
						format!(
							"not eligible: the callable is in 0072's `{}` bucket ({})",
							c.bucket,
							c.rules.join(" ")
						)
					} else if let Some((status, reason, bs, xs)) = per_key.get(p.key.as_str()) {
						if bs.iter().any(|(r, _)| *r == p.alias) {
							"emitted".to_string()
						} else if let Some((_, why)) = xs.iter().find(|(r, _)| *r == p.alias) {
							if why.starts_with("excluded by the release file") {
								format!("excluded: {why}")
							} else if why.starts_with("not shipped") {
								why.to_string()
							} else {
								format!("refused: {why}")
							}
						} else if *status == "unsupported" {
							format!("refused: {}", reason.unwrap_or("entry unsupported"))
						} else {
							format!("no disposition: entry {status} ({})", reason.unwrap_or(""))
						}
					} else {
						"no disposition: no entry for the callable".to_string()
					}
				}
			});
		}
	}
	(out, census)
}
