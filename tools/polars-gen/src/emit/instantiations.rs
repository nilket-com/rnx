use crate::census::PairRecord;
use crate::emit::callable::emit_method;
use crate::emit::{Emitted, OracleInfo, RouteException, binding_id};
use crate::families::callbacks::{binding_route_reason, callback_gate};
use crate::families::target;
use crate::families::{PAIR, PAIR_POST, PAIR_PRE, collect};
use crate::model::Callable;
use crate::ty::last;
use crate::world::World;
use crate::world::proof::Applicability;

/// Record 0076 gate 4: one binding per proven (method, alias) pair of a
/// generic owner, with the owner's parameters substituted; each binding
/// has its own oracle information. Pairs outside the release's
/// instantiation scope, and proven pairs the mapping rules refuse, are
/// route exceptions with their reason.
pub(crate) fn emit_instantiations(
	world: &World,
	out: &mut Emitted,
	c: &Callable,
	pairs: &[&PairRecord],
) {
	// record 0118: head parameters re-stated in a method where-clause
	let c_owned = crate::census::head_bound_generics(c);
	let c = c_owned.as_ref();
	if let Err(reason) = callback_gate(world, c) {
		out.unsupported(c, "callback audit", &reason);
		return;
	}
	let scope = &world.release.instantiation.families;
	let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
	let mut infos: Vec<OracleInfo> = Vec::new();
	let mut exceptions: Vec<RouteException> = Vec::new();
	let mut first_info: Option<OracleInfo> = None;
	for p in pairs {
		let _target = target(format!("{}|{}", c.key, p.alias));
		match &p.result {
			Applicability::Proven => {}
			Applicability::Rejected(r) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("rejected: {r}"),
				});
				continue;
			}
			Applicability::Unresolved(r) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("unresolved: {r}"),
				});
				continue;
			}
		}
		if !scope.is_empty() && !scope.iter().any(|f| f == p.family) {
			exceptions.push(RouteException {
				route: "instantiation",
				receiver: p.alias.clone(),
				reason: format!(
					"not shipped: family `{}` is outside the release's instantiation scope",
					p.family
				),
			});
			continue;
		}
		if let Some(x) = world
			.release
			.instantiation
			.exclude
			.iter()
			.find(|x| last(&p.alias) == x.alias && x.methods.iter().any(|m| *m == c.name))
		{
			exceptions.push(RouteException {
				route: "instantiation",
				receiver: p.alias.clone(),
				reason: format!("excluded by the release file: {}", x.cite),
			});
			continue;
		}
		let (_, subst) = world.applicability(c, &p.identity);
		let Ok((params, ret)) = world.substitute_signature(c, &p.identity, &subst) else {
			continue;
		};
		// an instantiation an alias wrapper holds exactly is spelled as that
		// alias everywhere downstream (mapping, oracle formatting)
		let norm = |t: String| -> String {
			let mut t = t;
			for (identity, alias) in &world.by_identity {
				if t.contains(identity.as_str()) {
					t = t.replace(identity.as_str(), alias);
				}
			}
			t
		};
		let params: Vec<String> = params.into_iter().map(norm).collect();
		let ret = ret.map(norm);
		let mut syn = c.clone();
		syn.owner = p.alias.clone();
		syn.bucket = "mechanical".into();
		for (q, t) in syn.params.iter_mut().zip(params) {
			q.ty_canonical = t;
		}
		// record 0115: the pair gates before the return is substituted
		// (`PAIR_PRE`): every listed state, in order, until a refusal
		let mut states = match collect(PAIR_PRE, "pair", |f| {
			f.listed_pair_pre(world, c, p, &mut syn, ret.as_deref())
		}) {
			Ok(s) => s,
			Err((label, why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: {label}: {why}"),
				});
				continue;
			}
		};
		syn.ret_canonical = ret;
		syn.impl_head = None;
		syn.impl_bounds.clear();
		syn.impl_where.clear();
		syn.generics_canonical.clear(); // closure bounds are now in the substituted parameter types
		// record 0115: the pair gates after it (`PAIR_POST`)
		match collect(PAIR_POST, "pair", |f| {
			f.listed_pair_post(world, c, p, &mut syn)
		}) {
			Ok(s) => states.extend(s),
			Err((label, why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: {label}: {why}"),
				});
				continue;
			}
		}
		let before = out.entries.len();
		{
			let _scope = world.active.enter(PAIR, states);
			emit_method(world, out, &syn, &p.alias, None, false);
		}
		let e = out.entries.pop().unwrap();
		debug_assert_eq!(before, out.entries.len());
		if e.status == "generated" {
			done.push((
				e.rune.clone().unwrap(),
				p.alias.clone(),
				"instantiation",
				None,
			));
			if let Some(i) = e.oracle.clone() {
				if first_info.is_none() {
					first_info = Some(i.clone());
				}
				infos.push(i);
			}
		} else {
			exceptions.push(RouteException {
				route: "instantiation",
				receiver: p.alias.clone(),
				reason: format!("{}: {}", e.status, e.reason.unwrap_or_default()),
			});
		}
	}
	if done.is_empty() {
		let why = if pairs
			.iter()
			.any(|p| matches!(p.result, Applicability::Proven))
		{
			"generic (proven instantiations were not emitted; see exceptions)"
		} else {
			"generic (no proven instantiation)"
		};
		out.unsupported(c, "bucket", why);
		out.entries.last_mut().unwrap().exceptions = exceptions;
		return;
	}
	let info = first_info.unwrap();
	out.generated_on(c, &done, info);
	let e = out.entries.last_mut().unwrap();
	let reason = binding_route_reason(world, c, true);
	for binding in &mut e.bindings {
		binding.route_reason = reason.clone();
		binding.reentry = Some(
			if e.fallible == Some(true) {
				"error"
			} else {
				"unwind"
			}
			.into(),
		);
	}
	for (b, i) in e.bindings.iter_mut().zip(infos) {
		b.info = Some(i);
		// one canonical path can carry several callables (one per impl
		// head): every instantiation id names its receiver
		b.id = binding_id(&c.canonical_path, b.receiver.as_deref(), false);
	}
	e.note = Some(format!(
		"instantiated on {} of {} alias identities",
		done.len(),
		pairs.len()
	));
	e.exceptions = exceptions;
}
