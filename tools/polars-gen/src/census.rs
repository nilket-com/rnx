use crate::emit::{Entry, NOT_ROUTED_TYPES, generics_map, routed};
use crate::families::bounds::sized_self_entry;
use crate::families::callbacks::{ClosureSig, closure_signature, plan_holder_non_plan_method};
use crate::families::protocols::ASSIGN_OPS;
use crate::model::Callable;
use crate::model::Inventory;
use crate::text::mentions;
use crate::ty;
use crate::ty::Ty;
use crate::world::World;
use crate::world::mapping::{Unsupported, iterator_return};
use crate::world::proof::{Applicability, family};
use std::collections::{BTreeMap, BTreeSet};

/// The instantiation census: every (method, alias identity) candidate
/// on generic owners that have alias wrappers, with its applicability.
#[derive(serde::Serialize, Clone)]
pub(crate) struct PairRecord {
	/// The callable's inventory key: one canonical path can carry several
	/// impl blocks (specialized heads), each its own callable.
	pub(crate) key: String,
	pub(crate) method: String,
	pub(crate) identity: String,
	pub(crate) alias: String,
	pub(crate) family: &'static str,
	#[serde(flatten)]
	pub(crate) result: Applicability,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) signature: Option<String>,
	/// Whether the callable was eligible for emission at all (0072's
	/// terms: not in the `unsupported`/`unknown` bucket); an ineligible
	/// callable's pairs are gross applicability, never emitted.
	pub(crate) eligible: bool,
	/// Filled after emission: `emitted`, `refused: …`, `excluded: …`,
	/// `not eligible: …`, or for an unproven pair its result.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) disposition: Option<String>,
}

pub(crate) fn instantiation_census(world: &World, inv: &Inventory) -> Vec<PairRecord> {
	// alias identities per generic base, representative alias first
	let mut by_base: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
	for (path, w) in &world.wrappers {
		if w.rule != "alias" {
			continue;
		}
		let Some(base) = &w.base else { continue };
		if !w.identity.starts_with(&format!("{base}<")) {
			continue;
		} // an Arc<…> target is not an instantiation of the base
		by_base
			.entry(base.clone())
			.or_default()
			.entry(w.identity.clone())
			.or_insert_with(|| w.aliases.first().cloned().unwrap_or_else(|| path.clone()));
	}
	let mut out = Vec::new();
	// record 0116 (rule 2): every generic owner with concrete alias wrappers,
	// not only `ChunkedArray` and `Logical` (0076's scope); each alias is a
	// pair decided by the same applicability proof
	for c0 in &inv.callables {
		if c0.kind != "inherent" || c0.impl_head.is_none() {
			continue;
		}
		let Some(ids) = by_base.get(&c0.owner) else {
			continue;
		};
		let c_owned = head_bound_generics(c0);
		let c = c_owned.as_ref();
		for (identity, alias) in ids {
			let closure_generics: BTreeSet<&str> = c
				.params
				.iter()
				.filter(|p| closure_signature(c, p).is_some())
				.map(|p| p.ty_canonical.as_str())
				.collect();
			let scalar = world
				.release
				.families
				.method_scalar_generics
				.iter()
				.find(|m| m.key == c.key && m.path == c.canonical_path && m.check(c).is_ok());
			// record 0097: a listed, well-formed `Self: Sized` method is decided by applicability
			let sized = sized_self_entry(&world.release, c);
			if let Some(Err(why)) = &sized {
				out.push(PairRecord {
					key: c.key.clone(),
					method: c.canonical_path.clone(),
					identity: identity.clone(),
					alias: alias.clone(),
					family: family(world, identity),
					result: Applicability::Unresolved(format!("sized-self method: {why}")),
					signature: None,
					eligible: !matches!(c.bucket.as_str(), "unsupported" | "unknown"),
					disposition: None,
				});
				continue;
			}
			let sized_ok = matches!(sized, Some(Ok(_)));
			if c.generics_canonical.iter().any(|(name, _)| {
				!closure_generics.contains(name.as_str())
					&& scalar.is_none_or(|m| m.generic != *name)
					&& !(sized_ok && name == "Self")
			}) {
				out.push(PairRecord {
					key: c.key.clone(),
					method: c.canonical_path.clone(),
					identity: identity.clone(),
					alias: alias.clone(),
					family: family(world, identity),
					result: Applicability::Unresolved(
						"function-level generics are out of this record's scope".into(),
					),
					signature: None,
					eligible: !matches!(c.bucket.as_str(), "unsupported" | "unknown"),
					disposition: None,
				});
				continue;
			}
			let (r, subst) = world.applicability(c, identity);
			let (r, sig) = match r {
				Applicability::Proven => match world.substitute_signature(c, identity, &subst) {
					Ok((ps, ret)) => (
						Applicability::Proven,
						Some(format!(
							"({}) -> {}",
							ps.join(", "),
							ret.as_deref().unwrap_or("()")
						)),
					),
					Err(e) => (Applicability::Unresolved(format!("signature: {e}")), None),
				},
				other => (other, None),
			};
			out.push(PairRecord {
				key: c.key.clone(),
				method: c.canonical_path.clone(),
				identity: identity.clone(),
				alias: alias.clone(),
				family: family(world, identity),
				result: r,
				signature: sig,
				eligible: !matches!(c.bucket.as_str(), "unsupported" | "unknown"),
				disposition: None,
			});
		}
	}
	out
}

/// Record 0077 gate 1: every eligible callable and every instantiation
/// pair whose return is an iterator, with the item and its disposition.
pub(crate) fn iterator_census(
	entries: &[Entry],
	inv: &Inventory,
	pairs: &[PairRecord],
) -> serde_json::Value {
	let mut callables = Vec::new();
	let by_key: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
	for c in &inv.callables {
		let Some(r) = &c.ret_canonical else { continue };
		let Some((item, known, wrap)) = iterator_return(&ty::parse(r)) else {
			continue;
		};
		let disposition = match by_key.get(c.key.as_str()) {
			Some(e) if e.status == "generated" => {
				if e.bindings.iter().any(|b| b.disposition.is_some()) {
					"materialized".to_string()
				} else {
					"generated".to_string()
				}
			}
			Some(e) => format!("{}: {}", e.status, e.reason.as_deref().unwrap_or("")),
			None => match c.bucket.as_str() {
				"unsupported" | "unknown" => "not eligible".to_string(),
				_ => "out of scope".to_string(),
			},
		};
		callables.push(serde_json::json!({"key": c.key, "method": c.canonical_path, "owner_generic": c.owner_generic, "receiver": c.receiver, "item": item.render(), "known_length": format!("{known:?}"), "wrap": format!("{wrap:?}"), "disposition": disposition}));
	}
	let pair_rows: Vec<serde_json::Value> = pairs.iter().filter(|p| p.signature.as_deref().is_some_and(|s| s.contains("impl ")) || p.disposition.as_deref().is_some_and(|d| d.contains("iterator") || d.contains("impl return"))).map(|p| serde_json::json!({"key": p.key, "method": p.method, "alias": p.alias, "disposition": p.disposition})).collect();
	let mut by_disp: BTreeMap<String, usize> = BTreeMap::new();
	for c in &callables {
		*by_disp
			.entry(
				c["disposition"]
					.as_str()
					.unwrap()
					.split(':')
					.next()
					.unwrap()
					.to_string(),
			)
			.or_insert(0) += 1;
	}
	let mut pair_disp: BTreeMap<String, usize> = BTreeMap::new();
	for p in &pair_rows {
		*pair_disp
			.entry(
				p["disposition"]
					.as_str()
					.unwrap_or("none")
					.split(':')
					.take(2)
					.collect::<Vec<_>>()
					.join(":"),
			)
			.or_insert(0) += 1;
	}
	serde_json::json!({"callables": callables.len(), "callables_by_disposition": by_disp, "pairs": pair_rows.len(), "pairs_by_disposition": pair_disp, "callable_rows": callables, "pair_rows": pair_rows})
}

pub(crate) fn callback_census(
	world: &World,
	inv: &Inventory,
	entries: &[Entry],
) -> serde_json::Value {
	let release = &world.release;
	let by_key: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
	// an empty family list in the release file means every family (as the
	// instantiation census reads it); the pair-level applicability is 0080's
	let families = true;
	let generic_family_owner = |o: &str| {
		o == "polars_core::chunked_array::ChunkedArray"
			|| o == "polars_core::chunked_array::logical::Logical"
	};
	// every non-plan-returning method of a plan-holding type is classified by the audit, or listed as unclassified
	let mut sinks = Vec::new();
	let mut unclassified_sinks: Vec<String> = Vec::new();
	let mut sink_groups: BTreeSet<&str> = BTreeSet::new();
	for c in &inv.callables {
		if !release.is_api(&c.krate) || !plan_holder_non_plan_method(c) {
			continue;
		}
		let e = by_key.get(c.key.as_str());
		let routed_today = routed(
			&c.name,
			Some(&c.owner),
			&c.params,
			c.ret_canonical.as_deref(),
		);
		match release
			.families
			.callback_sink
			.iter()
			.find(|k| k.path == c.canonical_path)
		{
			Some(k) => {
				if k.sink != "none" {
					sink_groups.insert(k.sink.as_str());
				}
				sinks.push(serde_json::json!({"key": c.key, "path": c.canonical_path, "sink": k.sink, "cite": k.cite, "status": e.map(|e| e.status).unwrap_or("not emitted"), "rune": e.and_then(|e| e.rune.clone()), "routed_today": routed_today}));
			}
			None => unclassified_sinks.push(c.canonical_path.clone()),
		}
	}
	let mut rows = Vec::new();
	let mut disp: BTreeMap<String, usize> = BTreeMap::new();
	for c in &inv.callables {
		let closures: Vec<ClosureSig> = c
			.params
			.iter()
			.filter_map(|p| closure_signature(c, p))
			.collect();
		if closures.is_empty() {
			continue;
		}
		let api = release.is_api(&c.krate);
		let mut refusals: Vec<String> = Vec::new();
		let mut contracts: Vec<String> = Vec::new();
		let mut per_family = false;
		let generics = generics_map(c);
		let closure_names: BTreeSet<&str> = c
			.params
			.iter()
			.filter(|p| closure_signature(c, p).is_some())
			.map(|p| {
				p.ty_canonical
					.trim()
					.trim_start_matches("&mut ")
					.trim_start_matches('&')
					.trim()
			})
			.collect();
		let free_generics: Vec<&str> = generics
			.keys()
			.map(|s| s.as_str())
			.filter(|g| !closure_names.contains(g))
			.collect();
		let is_projection = |t: &str| t.contains("::Native") || t.contains("::Physical");
		let free_in = |t: &str| {
			free_generics
				.iter()
				.find(|g| mentions(t, g))
				.map(|g| g.to_string())
		};
		if !api {
			let d = "out of scope: internal crate".to_string();
			*disp.entry(d.clone()).or_insert(0) += 1;
			rows.push(serde_json::json!({"key": c.key, "owner": c.owner, "name": c.name, "closures": closures.iter().map(|s| format!("{}: {}({}) -> {}", s.param, s.kind, s.args.join(", "), s.ret)).collect::<Vec<_>>(), "disposition": d}));
			continue;
		}
		if c.bucket == "unsupported" || c.bucket == "unknown" {
			let d = format!("not eligible: bucket {}", c.bucket);
			*disp.entry("not eligible".into()).or_insert(0) += 1;
			rows.push(
				serde_json::json!({"key": c.key, "owner": c.owner, "name": c.name, "disposition": d}),
			);
			continue;
		}
		for s in &closures {
			if s.udf {
				refusals.push(format!("{}: Udf trait object", s.param));
				continue;
			}
			if c.is_async || mentions(&s.ret, "Fut") {
				refusals.push(format!("{}: async closure", s.param));
				continue;
			}
			for a in &s.args {
				let ty = ty::parse(a);
				if let Ty::Ref {
					mutable: true,
					inner,
				} = &ty
				{
					match release
						.families
						.callback_mutable
						.iter()
						.find(|m| m.path == c.canonical_path && m.param == s.param)
					{
						Some(m) => contracts
							.push(format!("{} `{}`: {} ({})", s.param, a, m.contract, m.cite)),
						None => refusals.push(format!(
							"{}: mutable argument `{}` the callback cannot write back (no audited contract)",
							s.param,
							inner.render()
						)),
					}
					continue;
				}
				if is_projection(a) {
					if generic_family_owner(&c.owner) && families {
						per_family = true;
					} else {
						refusals.push(format!(
							"{}: projection `{a}` on an owner without instantiation families",
							s.param
						));
					}
					continue;
				}
				if let Some(g) = free_in(a) {
					refusals.push(format!(
						"{}: argument `{a}` is the free generic `{g}`",
						s.param
					));
					continue;
				}
				// an amortized series is a borrow Polars reuses between calls; it is kept out of callbacks (the plan's census class), a later record may relax it
				if NOT_ROUTED_TYPES.iter().any(|d| mentions(a, d)) {
					refusals.push(format!(
						"{}: argument `{a}` is an amortized borrow Polars reuses between calls (AmortSeries)",
						s.param
					));
					continue;
				}
				// a shared slice or vector of a wrapped type arrives as a Rune vector of clones
				let elem = match &ty {
					Ty::Ref {
						mutable: false,
						inner,
					} => match &**inner {
						Ty::Slice(e) => Some((**e).clone()),
						_ => None,
					},
					Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 => {
						Some(args[0].clone())
					}
					_ => None,
				};
				if let Some(e) = elem {
					match world.ret(&e, Some(&c.owner), 0) {
						Ok(_) => continue,
						Err(Unsupported(why, what)) => {
							refusals.push(format!("{}: argument `{a}`: {why} ({what})", s.param));
							continue;
						}
					}
				}
				if let Err(Unsupported(why, what)) = world.ret(&ty, Some(&c.owner), 0) {
					refusals.push(format!("{}: argument `{a}`: {why} ({what})", s.param));
				}
			}
			let ret = s.ret.trim();
			let ret = ret
				.strip_prefix("polars_error::PolarsResult<")
				.and_then(|r| r.strip_suffix('>'))
				.unwrap_or(ret);
			let rty = ty::parse(ret);
			if ret == "()" {
			} else if matches!(rty, Ty::Ref { .. }) || ret.starts_with("&'") {
				refusals.push(format!(
					"{}: return `{ret}` borrowed from the argument",
					s.param
				));
			} else if is_projection(ret) {
				if generic_family_owner(&c.owner) && families {
					per_family = true;
				} else {
					refusals.push(format!(
						"{}: projection `{ret}` on an owner without instantiation families",
						s.param
					));
				}
			} else if let Some(g) = free_in(ret) {
				refusals.push(format!("{}: return `{ret}` is the free generic `{g}` (a Rune function cannot choose a Rust type parameter)", s.param));
			} else if let Err(Unsupported(why, what)) =
				world.arg(&rty, "r", &generics, Some(&c.owner), 0)
			{
				refusals.push(format!("{}: return `{ret}`: {why} ({what})", s.param));
			}
		}
		// the rest of the callable: owner, receiver, other parameters, other generics
		if world.wrapper_for(&c.owner).is_none() && !c.owner.is_empty() {
			if generic_family_owner(&c.owner) && families {
				per_family = true;
			} else {
				refusals.push(format!("owner not wrapped: {}", c.owner));
			}
		}
		if !matches!(c.receiver.as_str(), "none" | "self" | "&self" | "&mut self") {
			refusals.push(format!("receiver form `{}`", c.receiver));
		}
		for p in &c.params {
			if closure_signature(c, p).is_some() {
				continue;
			}
			let t = ty::parse(&p.ty_canonical);
			if is_projection(&p.ty_canonical) {
				if !(generic_family_owner(&c.owner) && families) {
					refusals.push(format!(
						"parameter `{}`: projection without instantiation families",
						p.name
					));
				}
				continue;
			}
			if let Err(Unsupported(why, what)) =
				world.arg(&t, &p.name, &generics, Some(&c.owner), 0)
			{
				refusals.push(format!("parameter `{}`: {why} ({what})", p.name));
			}
		}
		if let Some(r) = c.ret_canonical.as_deref() {
			let r = r
				.strip_prefix("polars_error::PolarsResult<")
				.and_then(|x| x.strip_suffix('>'))
				.unwrap_or(r);
			if r != "Self" && !is_projection(r) {
				if let Err(Unsupported(why, what)) = world.ret(&ty::parse(r), Some(&c.owner), 0) {
					refusals.push(format!("return: {why} ({what})"));
				}
			}
		}
		// invocation per closure from the release file's source audit; a
		// closure without an entry leaves the operation unresolved
		let mut unresolved: Vec<String> = Vec::new();
		let mut invocation: Vec<serde_json::Value> = Vec::new();
		for s in &closures {
			match release
				.families
				.callback_invocation
				.iter()
				.find(|a| a.path == c.canonical_path && a.param == s.param)
			{
				Some(a) => {
					if a.invocation == "stored" {
						for g in &a.sinks {
							if !sink_groups.contains(g.as_str()) {
								unresolved.push(format!(
									"{}: sink group `{g}` has no classified member",
									s.param
								));
							}
						}
						if !unclassified_sinks.is_empty() {
							unresolved.push(format!(
								"{}: unclassified execution path(s): {}",
								s.param,
								unclassified_sinks.join(", ")
							));
						}
					}
					invocation.push(serde_json::json!({"param": s.param, "invocation": a.invocation, "sinks": a.sinks, "cite": a.cite, "static_bound": s.static_bound}));
				}
				None => {
					unresolved.push(format!("{}: invocation not audited", s.param));
					invocation.push(serde_json::json!({"param": s.param, "invocation": "unresolved", "static_bound": s.static_bound}));
				}
			}
		}
		let disposition = if !refusals.is_empty() {
			"refused".to_string()
		} else if !unresolved.is_empty() {
			"unresolved".to_string()
		} else {
			let mut q: Vec<&str> = Vec::new();
			if per_family {
				q.push("per family");
			}
			if contracts.iter().any(|x| x.contains("result buffer")) {
				q.push("result buffer");
			}
			if contracts.iter().any(|x| x.contains("vector")) {
				q.push("vector argument");
			}
			if q.is_empty() {
				"feasible".to_string()
			} else {
				format!("feasible ({})", q.join(", "))
			}
		};
		*disp.entry(disposition.clone()).or_insert(0) += 1;
		let today = by_key
			.get(c.key.as_str())
			.map(|e| e.status.to_string())
			.unwrap_or_else(|| "not emitted".into());
		let after_emission = match by_key.get(c.key.as_str()) {
			Some(e) if e.status == "generated" => "generated".to_string(),
			Some(e) if per_family && !e.exceptions.is_empty() => format!(
				"pair refused: {}",
				e.reason.as_deref().unwrap_or("see exceptions")
			),
			Some(e) => format!("refused: {}", e.reason.as_deref().unwrap_or(e.status)),
			None => "refused: not eligible for emission".into(),
		};
		assert!(
			disposition.starts_with("feasible") || after_emission != "generated",
			"callback audit refused {} but it acquired a binding",
			c.canonical_path
		);
		rows.push(serde_json::json!({
            "key": c.key, "owner": c.owner, "name": c.name, "canonical_path": c.canonical_path, "bucket": c.bucket,
            "closures": closures.iter().map(|s| format!("{}: {}({}) -> {}", s.param, s.kind, s.args.join(", "), s.ret)).collect::<Vec<_>>(),
            "invocation": invocation, "mutable_contracts": contracts, "disposition": disposition, "refusals": refusals, "unresolved": unresolved, "today": today, "disposition after emission": after_emission,
        }));
	}
	let unrouted = sinks
		.iter()
		.filter(|s| s["status"] == "generated" && s["routed_today"] == false && s["sink"] != "none")
		.count();
	serde_json::json!({
		"count": rows.len(),
		"by_disposition": disp,
		"rows": rows,
		"rule": "every binding that accepts a closure is routed through engine::run (route reason `callback`); a stored callback is invoked only from a classified sink, and every sink binding is routed (route reason `executes callbacks`)",
		"sinks": {
			"audit": ["polars-expr-0.55.2/src/expressions/apply.rs:114,133,235,295,325,376,395 (ColumnsUdf::call_udf during plan execution)", "polars-plan-0.55.2/src/plans/functions/mod.rs:202 (DataFrame UDF of LazyFrame::map)", "polars-stream-0.55.2/src/nodes/map.rs:50, columnar_function.rs:94, in_memory_map.rs:51 (the streaming engine)", "polars-plan-0.55.2/src/plans/aexpr/schema.rs:287,299,391, plans/ir/unoptimized.rs:48,65 and plans/schema.rs:21 (FunctionOutputField::get_field during schema resolution)"],
			"classified": "every method of LazyFrame, DslPlan, DslBuilder, JoinBuilder, LazyGroupBy and Expr whose result is not a plan type, by the release file's [[callback_sink]] entries",
			"bindings": sinks,
			"unclassified": unclassified_sinks,
			"generated_unrouted_today": unrouted,
		},
	})
}

pub(crate) fn conversion_census(
	entries: &[Entry],
	inv: &Inventory,
	from_names: &BTreeMap<String, String>,
) -> serde_json::Value {
	let by_key: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
	let mut rows = Vec::new();
	let mut disp: BTreeMap<String, usize> = BTreeMap::new();
	for c in &inv.callables {
		if c.kind != "foreign_trait_impl" {
			continue;
		}
		let short = c.name.split('<').next().unwrap_or("");
		let class = if short == "From" {
			"from"
		} else if ASSIGN_OPS.iter().any(|(n, _, _, _)| *n == short) {
			"assign"
		} else if short == "Not" {
			"not"
		} else {
			continue;
		};
		let name = match class {
			"from" => from_names.get(&c.key).cloned().unwrap_or_default(),
			"assign" => ASSIGN_OPS
				.iter()
				.find(|(n, _, _, _)| *n == short)
				.map(|x| x.2.to_string())
				.unwrap_or_default(),
			_ => "not_".into(),
		};
		let source = c
			.params
			.first()
			.map(|p| p.ty_canonical.clone())
			.unwrap_or_default();
		let disposition = match by_key.get(c.key.as_str()) {
			Some(e) if e.status == "generated" => "generated".to_string(),
			Some(e) => format!("{}: {}", e.status, e.reason.as_deref().unwrap_or("")),
			None => "not eligible".to_string(),
		};
		*disp
			.entry(format!(
				"{class}: {}",
				disposition.split(':').next().unwrap_or("")
			))
			.or_insert(0) += 1;
		rows.push(serde_json::json!({"key": c.key, "class": class, "owner": c.owner, "source": source, "name": name, "disposition": disposition}));
	}
	serde_json::json!({"count": rows.len(), "by_disposition": disp, "rows": rows})
}

pub(crate) fn census_summary(pairs: &[PairRecord]) -> serde_json::Value {
	let mut per: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
	for p in pairs {
		let k = match &p.result {
			Applicability::Proven => "proven",
			Applicability::Rejected(_) => "rejected",
			Applicability::Unresolved(_) => "unresolved",
		};
		*per.entry(p.family).or_default().entry(k).or_insert(0) += 1;
		*per.entry("all").or_default().entry(k).or_insert(0) += 1;
	}
	let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
	for p in pairs {
		if let Applicability::Rejected(r) | Applicability::Unresolved(r) = &p.result {
			let key = r.split('`').next().unwrap_or(r).trim().to_string();
			*reasons.entry(key).or_insert(0) += 1;
		}
	}
	let mut eligible: BTreeMap<&str, usize> = BTreeMap::new();
	for p in pairs.iter().filter(|p| p.eligible) {
		let k = match &p.result {
			Applicability::Proven => "proven",
			Applicability::Rejected(_) => "rejected",
			Applicability::Unresolved(_) => "unresolved",
		};
		*eligible.entry(k).or_insert(0) += 1;
	}
	let mut disp: BTreeMap<String, usize> = BTreeMap::new();
	for p in pairs
		.iter()
		.filter(|p| matches!(p.result, Applicability::Proven))
	{
		let d = p
			.disposition
			.as_deref()
			.unwrap_or("none")
			.split(':')
			.next()
			.unwrap_or("none")
			.to_string();
		*disp.entry(d).or_insert(0) += 1;
	}
	serde_json::json!({"pairs": pairs.len(), "by_family": per, "eligible": eligible, "proven_dispositions": disp, "exception_reasons": reasons})
}

/// Record 0115 (moved verbatim from `pipeline::generate`): every pair of
/// the census gets exactly one disposition, emitted or not.
pub(crate) fn census_dispositions(
	census: Vec<PairRecord>,
	entries: &[Entry],
	census_keys: &BTreeSet<String>,
	inv: &Inventory,
) -> Vec<PairRecord> {
	let mut census = census;
	{
		let mut per_key: BTreeMap<
			&str,
			(&str, Option<&str>, Vec<(&str, &str)>, Vec<(&str, &str)>),
		> = BTreeMap::new();
		for e in entries {
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
	census
}

/// Record 0118: a method where-clause on the impl head's own parameter
/// (`impl<W> ParquetWriter<W> { fn new(w: W) -> Self where W: Write }`) is
/// listed by rustdoc as a method generic. It is the head's, bound by the
/// receiver, so it moves to the impl predicates the proof discharges and
/// stops being a generic to infer or to map to its bound.
pub(crate) fn head_bound_generics(c: &Callable) -> std::borrow::Cow<'_, Callable> {
	let Some(head) = c.impl_head.as_deref() else {
		return std::borrow::Cow::Borrowed(c);
	};
	let head_params: BTreeSet<String> = crate::world::proof::split_head(head)
		.1
		.into_iter()
		.filter(|a| crate::world::proof::is_param(a))
		.collect();
	if !c
		.generics_canonical
		.iter()
		.any(|(n, _)| head_params.contains(n))
	{
		return std::borrow::Cow::Borrowed(c);
	}
	let mut x = c.clone();
	for (n, b) in &c.generics_canonical {
		if head_params.contains(n) && !b.is_empty() {
			x.impl_where.push(format!("{n}: {b}"));
		}
	}
	x.generics_canonical
		.retain(|(n, _)| !head_params.contains(n));
	std::borrow::Cow::Owned(x)
}
