use crate::census::PairRecord;
use crate::emit::callable::emit_method;
use crate::emit::{Emitted, OracleInfo, RouteException, binding_id};
use crate::families::bounds::{null_aware_return_matches, sized_self_entry};
use crate::families::callbacks::{binding_route_reason, callback_gate};
use crate::families::snapshots::{
	array_snapshot_entry, chunk_snapshot_entry, indexed_chunk_entry, iter_snapshot_entry,
	layout_entry, owned_iter_entry, view_snapshot_entry,
};
use crate::model::Callable;
use crate::text::mentions;
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
		// record 0092: a listed scalar function generic becomes this pair's native
		if let Some(m) = world
			.release
			.method_scalar_generics
			.iter()
			.find(|m| m.key == c.key && m.path == c.canonical_path)
		{
			if let Err(why) = m.check(c) {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: method scalar generic: {why}"),
				});
				continue;
			}
			let Some(native) = m.native_for(&p.identity) else {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!(
						"refused: method scalar generic: `{}` is not a listed type",
						p.identity
					),
				});
				continue;
			};
			// the family substitution has already spelled `N` as its bound, so the
			// parameters that are exactly `N` in the original signature are
			// bound by position (`check` guarantees `N` appears nowhere else)
			for (q, orig) in syn.params.iter_mut().zip(&c.params) {
				if orig.ty_canonical.trim() == m.generic {
					q.ty_canonical = native.to_string();
				}
			}
			if syn.params.iter().any(|q| {
				mentions(&q.ty_canonical, &m.generic) || q.ty_canonical.contains("NumCast")
			}) {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: method scalar generic: `{}` remains", m.generic),
				});
				continue;
			}
		}
		// record 0096: a listed null-aware return, for this pair's native only
		let null_aware = match world
			.release
			.null_aware_returns
			.iter()
			.find(|n| n.key == c.key && n.path == c.canonical_path)
		{
			None => None,
			Some(n) => {
				if let Err(why) = n.check() {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!("refused: null-aware return: {why}"),
					});
					continue;
				}
				match n.native_for(&p.identity) {
					Some(native) if !null_aware_return_matches(ret.as_deref(), native) => {
						exceptions.push(RouteException {
							route: "instantiation",
							receiver: p.alias.clone(),
							reason: format!(
								"refused: null-aware return: `{}` is not Either<Vec<{native}>, Vec<Option<{native}>>>",
								ret.as_deref().unwrap_or("()")
							),
						});
						continue;
					}
					Some(native) => Some((c.name.clone(), native.to_string())),
					None => {
						exceptions.push(RouteException {
							route: "instantiation",
							receiver: p.alias.clone(),
							reason: format!(
								"refused: null-aware return: `{}` is not a listed type",
								p.identity
							),
						});
						continue;
					}
				}
			}
		};
		syn.ret_canonical = ret;
		syn.impl_head = None;
		syn.impl_bounds.clear();
		syn.impl_where.clear();
		syn.generics_canonical.clear(); // closure bounds are now in the substituted parameter types
		// record 0099: a listed chunk snapshot, for this pair's native only
		let chunk_snapshot = match chunk_snapshot_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: chunk snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Some((c.name.clone(), n.to_string())),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: chunk snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0101: a listed indexed chunk snapshot, for this pair's kind only
		let indexed_chunk = match indexed_chunk_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: indexed chunk snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Some((c.name.clone(), n.to_string())),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: indexed chunk snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0106: a listed layout snapshot, for this pair only
		let layout = match layout_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: layout snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.pair_for(&p.identity) {
				Some((t, n)) => Some((c.name.clone(), n, t)),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: layout snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0105: a listed owned iterator snapshot, for this pair's kind only
		let owned_iter = match owned_iter_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: owned iterator snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Some((c.name.clone(), n.to_string())),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: owned iterator snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0104: a listed view snapshot, for this pair's kind only
		let view_snapshot = match view_snapshot_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: view snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Some((c.name.clone(), n.to_string())),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: view snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0103: a listed iterator snapshot, for this pair's kind only
		let iter_snapshot = match iter_snapshot_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: iterator snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Some((c.name.clone(), n.to_string())),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: iterator snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0102: a listed array snapshot, for this pair's kind only
		let array_snapshot = match array_snapshot_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: array snapshot: {why}"),
				});
				continue;
			}
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Some((c.name.clone(), n.to_string())),
				None => {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: format!(
							"refused: array snapshot: `{}` is not a listed pair",
							p.identity
						),
					});
					continue;
				}
			},
		};
		// record 0097: a listed `Self: Sized` method (re-checked on the original signature)
		let sized_self = match sized_self_entry(&world.release, c) {
			None => None,
			Some(Err(why)) => {
				exceptions.push(RouteException {
					route: "instantiation",
					receiver: p.alias.clone(),
					reason: format!("refused: sized-self method: {why}"),
				});
				continue;
			}
			Some(Ok(_)) => {
				// `Self` is the method's generic (bound `Sized`), and the family
				// substitution spelled it as that bound: on a concrete pair it is
				// the receiver itself, and nothing else may mention it
				if syn.params.iter().any(|q| q.ty_canonical.contains("Sized")) {
					exceptions.push(RouteException {
						route: "instantiation",
						receiver: p.alias.clone(),
						reason: "refused: sized-self method: `Sized` remains in a parameter".into(),
					});
					continue;
				}
				syn.ret_canonical = Some(p.alias.clone());
				Some(c.name.clone())
			}
		};
		let before = out.entries.len();
		*world.null_aware.borrow_mut() = null_aware;
		*world.sized_self.borrow_mut() = sized_self;
		*world.chunk_snapshot.borrow_mut() = chunk_snapshot;
		*world.indexed_chunk.borrow_mut() = indexed_chunk;
		*world.array_snapshot.borrow_mut() = array_snapshot;
		*world.iter_snapshot.borrow_mut() = iter_snapshot;
		*world.view_snapshot.borrow_mut() = view_snapshot;
		*world.owned_iter.borrow_mut() = owned_iter;
		*world.layout.borrow_mut() = layout;
		emit_method(world, out, &syn, &p.alias, None, false);
		*world.layout.borrow_mut() = None;
		*world.owned_iter.borrow_mut() = None;
		*world.view_snapshot.borrow_mut() = None;
		*world.iter_snapshot.borrow_mut() = None;
		*world.indexed_chunk.borrow_mut() = None;
		*world.array_snapshot.borrow_mut() = None;
		*world.null_aware.borrow_mut() = None;
		*world.sized_self.borrow_mut() = None;
		*world.chunk_snapshot.borrow_mut() = None;
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
