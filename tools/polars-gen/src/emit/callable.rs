use crate::emit::free::emit_free;
use crate::emit::{
	Emitted, OracleInfo, RouteException, doc_line, generics_map, rune_path, rust_ident,
	unused_generic,
};
use crate::families::callbacks::{
	binding_route_reason, callback_arg, callback_gate, closure_signature, routed_binding,
};
use crate::families::free_instantiations::emit_free_instantiations;
use crate::families::protocols::emit_foreign;
use crate::families::receivers::proven_trait_receivers;
use crate::families::run_checks;
use crate::families::serde::{emit_serde, serde_trait};
use crate::families::target;
use crate::families::{CALLABLE, collect};
use crate::model::Callable;
use crate::text::sanitize;
use crate::ty;
use crate::ty::{Ty, last};
use crate::world::mapping::{IterLen, IterWrap, Materialize, Ret, Unsupported};
use crate::world::{HAND_METHODS, World, rune_name, spell};
use std::fmt::Write as _;

/// The call expression of a binding whose return is materialized: the
/// iterator is created, driven under the bound and its elements
/// converted to owned values inside one block, which is the routed
/// closure when the binding is routed; only the owned vector leaves it.
pub(crate) fn materialized_call(
	m: &Materialize,
	callee_call: &str,
	name: &str,
	route: bool,
) -> String {
	let helper = match m.known {
		IterLen::Exact => "support::materialize_exact",
		IterLen::Trusted => "support::materialize_trusted",
		IterLen::Unknown => "support::materialize_unknown",
	};
	let conv = format!(
		"|__r| Ok::<_, Error>({})",
		m.elem_conv.replace("__OP__", name)
	);
	let inner = match m.wrap {
		IterWrap::Plain => {
			format!("{{ let __it = {callee_call}; {helper}(__it, \"{name}\", {conv}) }}")
		}
		IterWrap::Result => format!(
			"{{ let __it = {callee_call}.map_err(Error::from)?; {helper}(__it, \"{name}\", {conv}) }}"
		),
		IterWrap::Option => format!(
			"(|| Ok::<_, Error>(match {callee_call} {{ Some(__it) => Some({helper}(__it, \"{name}\", {conv})?), None => None }}))()"
		),
	};
	// record 0082: slice items count against one cumulative bound for the whole iterator
	let inner = if m.elem_conv.contains("support::copy_slice(")
		|| m.elem_conv.contains("support::copy_bits(")
	{
		format!("{{ let __slices = support::SliceBudget::enter(); {inner} }}")
	} else {
		inner
	};
	if route {
		format!("crate::engine::run(\"{name}\", move || {inner}).map_err(Error::engine)??")
	} else {
		format!("({inner})?")
	}
}

/// Record 0084: the `generic_fn` bucket token admits a `generic`-bucket
/// callable only when it carries function-level generics, so the chained
/// and iterator rules reach their targets without enabling the whole
/// bucket (whose other members are generic for unrelated reasons).
pub(crate) fn bucket_admitted(buckets: &[&str], c: &Callable) -> bool {
	buckets.contains(&c.bucket.as_str())
		|| (c.bucket == "generic"
			&& !c.generics_canonical.is_empty()
			&& buckets.contains(&"generic_fn"))
}

/// Generate one method/function binding. `owner` is the canonical owner
/// type (for methods) and `trait_spell` the trait for UFCS calls.
pub(crate) fn emit_callable(world: &World, out: &mut Emitted, c: &Callable, buckets: &[&str]) {
	// record 0115: the callable scope (`CALLABLE`): every listed family's
	// state, in order, until a refusal; cleared when the scope drops
	let states = match collect(CALLABLE, "listed", |f| f.listed(world, c)) {
		Ok(s) => s,
		Err((label, reason)) => {
			out.unsupported(c, label, &reason);
			return;
		}
	};
	let _scope = world.active.enter(CALLABLE, states);
	if let Err(reason) = callback_gate(world, c) {
		out.unsupported(c, "callback audit", &reason);
		return;
	}
	// record 0112: serde impls are generic over the (de)serializer and bind
	// as one pinned serde_json instantiation, whatever their bucket
	if c.kind == "foreign_trait_impl" && serde_trait(c).is_some() {
		emit_serde(world, out, c);
		return;
	}
	if !bucket_admitted(buckets, c) {
		out.unsupported(c, "bucket", c.bucket.clone().as_str());
		return;
	}
	match c.kind.as_str() {
		"inherent" => emit_method(world, out, c, &c.owner, None, false),
		"trait_method" => {
			let trait_ = &c.owner;
			let tspell = match world.types.get(trait_).and_then(|s| {
				spell(
					&s.found_paths,
					&s.crate_paths,
					last(trait_),
					&world.ambiguous_prelude,
				)
			}) {
				Some(s) => s,
				None => {
					out.unsupported(c, "trait has no public path", trait_);
					return;
				}
			};
			// record 0110: the receivers are exactly those proven from the trait's
			// impl records (exact alias identity, or a generic head with every
			// bound discharged); an implementor label alone never admits one
			// (review of 0110: the label list bypassed the proof)
			// record 0117 (stage B): a generic trait's receivers and their
			// resolved signatures come from its impls' trait arguments
			let generic_trait = world.types.get(trait_).is_some_and(|s| s.generic);
			let (proven, refusals, arms) = if generic_trait {
				let (arms, why) = crate::families::generic_traits::generic_trait_arms(world, c);
				(arms.keys().cloned().collect::<Vec<_>>(), why, arms)
			} else {
				let (p, w) = proven_trait_receivers(world, c);
				(p, w, std::collections::BTreeMap::new())
			};
			// record 0120: a trait recorded under a private module, spelled by its re-export
			let tspell = crate::families::concrete_arrays::trait_spelling(&tspell)
				.map(String::from)
				.unwrap_or(tspell);
			let tspell = if generic_trait {
				let n = world.types.get(trait_).map_or(0, |s| s.trait_params.len());
				format!("{tspell}<{}>", vec!["_"; n].join(", "))
			} else {
				tspell
			};
			// record 0119: a deferred type is never a receiver in this record
			let impls: Vec<&String> = proven
				.iter()
				.filter(|o| !world.release.deferred_type(o))
				.collect();
			// record 0119: a trait reached only through a deref route (the
			// `Array` trait on `polars::arrow::ArrayRef`) is not refused here
			let deref_only = !generic_trait
				&& impls.is_empty()
				&& world
					.deref_targets
					.get(trait_)
					.is_some_and(|v| !v.is_empty());
			if impls.is_empty() && !deref_only {
				let why = if refusals.is_empty() {
					c.implementors.join(", ")
				} else {
					format!("{} [{}]", c.implementors.join(", "), refusals.join("; "))
				};
				out.unsupported(c, "no wrapped implementor", &why);
				return;
			}
			let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
			let mut first_err: Option<(String, String)> = None;
			let mut first_info: Option<OracleInfo> = None;
			let mut infos: Vec<OracleInfo> = Vec::new();
			// record 0117: each binding's own call information, for a generic
			// trait whose arms differ per receiver
			let mut done_infos: Vec<Option<OracleInfo>> = Vec::new();
			let mut exceptions: Vec<RouteException> = Vec::new();
			let derefs: Vec<&String> = if generic_trait {
				vec![] // a generic trait's arms are per impl, never through Deref
			} else {
				world
					.deref_targets
					.get(trait_)
					.map(|v| v.iter().collect())
					.unwrap_or_default()
			};
			let candidates: Vec<(&String, bool)> = impls
				.iter()
				.map(|o| (*o, false))
				.chain(derefs.iter().map(|o| (*o, true)))
				.collect();
			for (owner, deref) in candidates {
				let route: &'static str = if deref { "deref" } else { "implementor" };
				let before = out.entries.len();
				match arms.get(owner).map(Vec::as_slice) {
					// record 0117 (stage B): several proven impls on one receiver
					// are one Rune function when their kinds are disjoint
					Some(many @ [_, _, ..]) => {
						crate::families::generic_traits::emit_trait_dispatch(
							world, out, c, many, owner, &tspell,
						)
					}
					Some([one]) => {
						emit_method(world, out, &one.callable, owner, Some(&tspell), deref)
					}
					_ => emit_method(world, out, c, owner, Some(&tspell), deref),
				}
				let e = out.entries.pop().unwrap();
				debug_assert_eq!(before, out.entries.len());
				if e.status == "generated" {
					let callee = if deref {
						e.oracle.as_ref().map(|i| i.callee.clone())
					} else {
						None
					};
					done.push((e.rune.clone().unwrap(), owner.clone(), route, callee));
					done_infos.push(e.oracle.clone());
					if let Some(i) = e.oracle.clone() {
						infos.push(i);
					}
					if first_info.is_none() {
						first_info = e.oracle.clone();
					}
				} else {
					let why = e.reason.clone().unwrap_or_default();
					let why = if deref && why.starts_with("name taken on this type by") {
						format!("not separately exposed, inherent binding retained ({why})")
					} else {
						why
					};
					exceptions.push(RouteException {
						route,
						receiver: owner.clone(),
						reason: why.clone(),
					});
					if first_err.is_none() {
						first_err = Some((e.status.to_string(), why));
					}
				}
			}
			// record 0117 (stage B): a generic trait's refused impls and receivers
			// stay named on the entry, beside the receivers that were proven
			if generic_trait {
				for r in &refusals {
					let (receiver, reason) = match r.split_once(" on ") {
						Some((_, rest)) if rest.contains(": ") => {
							let (id, why) = rest.split_once(": ").unwrap();
							(id.to_string(), why.to_string())
						}
						_ => match r.split_once(": ") {
							Some((head, why)) => (head.to_string(), why.to_string()),
							None => (String::new(), r.clone()),
						},
					};
					exceptions.push(RouteException {
						route: "implementor",
						receiver,
						reason,
					});
				}
			}
			if done.is_empty() {
				let (st, why) = first_err.unwrap();
				if st == "adapted" {
					out.adapted(c, &why, "")
				} else {
					out.unsupported(c, "on every implementor", &why)
				}
				out.entries.last_mut().unwrap().exceptions = exceptions;
			} else if let Some(mut info) = first_info {
				// every generated receiver is a binding with its own case
				info.implementors = infos.iter().filter_map(|i| i.owner.clone()).collect();
				out.generated_on(c, &done, info);
				let entry = out.entries.last_mut().unwrap();
				if generic_trait {
					// a generic trait's arms have per-receiver signatures and
					// fallibility: each binding keeps its own
					entry.fallible = Some(done_infos.iter().flatten().any(|i| i.fallible));
					for (binding, own) in entry.bindings.iter_mut().zip(&done_infos) {
						binding.info = own.clone();
					}
				}
				for binding in &mut entry.bindings {
					let route = routed_binding(world, c, binding.receiver.as_deref());
					binding.route_reason = binding_route_reason(world, c, route);
					if route {
						binding.reentry = Some(
							if entry.fallible == Some(true) {
								"error"
							} else {
								"unwind"
							}
							.into(),
						);
					}
				}
				entry.exceptions = exceptions;
			} else {
				out.generated(
					c,
					&done
						.iter()
						.map(|(r, _, _, _)| r.as_str())
						.collect::<Vec<_>>()
						.join(" "),
					None,
				);
			}
		}
		"free_fn"
			if world
				.release
				.families
				.free_instantiations
				.iter()
				.any(|f| f.key == c.key && f.path == c.canonical_path) =>
		{
			emit_free_instantiations(world, out, c)
		}
		"free_fn" => emit_free(world, out, c),
		"foreign_trait_impl" => emit_foreign(world, out, c),
		_ => out.unsupported(c, "kind", c.kind.clone().as_str()),
	}
}

/// Record 0110: types the generator converts to Rune scalars (strings), never wrapped.
pub(crate) const SCALAR_MAPPED: &[&str] =
	&["alloc::string::String", "polars_utils::pl_str::PlSmallStr"];

/// Record 0108: arguments a Rune method binding can take, receiver included.
pub(crate) const METHOD_ARITY: usize = 5;

pub(crate) fn emit_method(
	world: &World,
	out: &mut Emitted,
	c: &Callable,
	owner: &str,
	trait_spell: Option<&str>,
	deref: bool,
) {
	emit_method_with(world, out, c, owner, trait_spell, deref, None)
}

/// `emit_method` with an explicit Rust callee (record 0078: a `From`
/// constructor calls `<T as From<X>>::from`, never a method named after
/// the binding).
pub(crate) fn emit_method_with(
	world: &World,
	out: &mut Emitted,
	c: &Callable,
	owner: &str,
	trait_spell: Option<&str>,
	deref: bool,
	callee_override: Option<&str>,
) {
	world.tmp.set(0);
	// record 0115: the trace target for this binding
	let _target = target(format!("{}|{owner}", c.key));
	// record 0085: instantiated pairs reach here without `emit_callable`,
	// so the callable scope is entered here too
	// record 0115: the callable scope (`CALLABLE`): every listed family's
	// state, in order, until a refusal; cleared when the scope drops
	let states = match collect(CALLABLE, "listed", |f| f.listed(world, c)) {
		Ok(s) => s,
		Err((label, reason)) => {
			out.unsupported(c, label, &reason);
			return;
		}
	};
	let _scope = world.active.enter(CALLABLE, states);
	if c.is_async {
		out.unsupported(c, "async", &c.name);
		return;
	}
	// record 0120: the concrete arrays are read-side only, fail closed
	if world.concrete_owner(Some(owner)) {
		if let Err(why) = crate::families::concrete_arrays::admit(world, c) {
			out.unsupported(c, "concrete array rule", &why);
			return;
		}
	}
	let Some(w) = world.wrapper_for(owner) else {
		let s = world.types.get(owner);
		let why = match s {
			Some(s) if s.generic => "generic owner",
			Some(s) if s.lifetime => "lifetime owner",
			Some(s) if s.hidden => "hidden owner",
			Some(_) => "owner has no public path",
			None => "unknown owner",
		};
		out.unsupported(c, why, owner.to_string().as_str());
		return;
	};
	let rust_name = c.name.clone();
	let name = rune_name(&rust_name);
	if let Some(g) = unused_generic(c) {
		out.unsupported(c, "generic parameter not inferable from arguments", &g);
		return;
	}
	if let Some(r) = world
		.release
		.refused
		.iter()
		.find(|r| r.path == c.canonical_path)
	{
		out.unsupported(c, "release policy", &format!("{} ({})", r.reason, r.cite));
		return;
	}
	if HAND_METHODS.iter().any(|(o, n)| *o == owner && *n == name) {
		out.adapted(
			c,
			"hand-written binding of the same name",
			&format!("{}::{name}", rune_path(w)),
		);
		return;
	}
	let key = (w.rust.clone(), name.clone());
	if let Some(prev) = out.taken.get(&key) {
		out.unsupported(c, "name taken on this type by", prev.clone().as_str());
		return;
	}
	let generics = generics_map(c);
	let mut params = Vec::new();
	for p in &c.params {
		let mapped = match closure_signature(c, p) {
			Some(sig) => callback_arg(world, c, &sig, &sanitize(&p.name), Some(owner)),
			None => world.arg(
				&ty::parse(&p.ty_canonical),
				&sanitize(&p.name),
				&generics,
				Some(owner),
				0,
			),
		};
		match mapped {
			Ok(a) => params.push((sanitize(&p.name), a)),
			Err(Unsupported(why, what)) => {
				out.unsupported(c, why, &format!("{} ({what})", p.name));
				return;
			}
		}
	}
	let ret = match c.ret_canonical.as_deref() {
		None => Ret {
			materialize: None,
			rust_ty: "()".into(),
			fallible: false,
			conv: "__r".into(),
			doc: "unit".into(),
		},
		Some(rc) => {
			let t = ty::parse(rc);
			// record 0116 (Codex, stage 1): a `&mut` return is a chain (the
			// receiver mutated in place, returned as unit) only for a `&mut self`
			// method whose return, direct or the first value of a Result, is
			// `&mut Self` or `&mut` the owner itself; every other mutable borrow
			// (an inner value, a trait object) has no script contract
			let self_chain = |t: &Ty| match t {
				Ty::Ref {
					mutable: true,
					inner,
				} => {
					c.receiver == "&mut self"
						&& match &**inner {
							Ty::Generic(g) => g == "Self",
							// record 0116 (review): the receiver itself, structurally
							// (`Self`, or the instantiated owner with its arguments)
							Ty::Path { path, .. } => {
								path == "Self" || inner.render() == ty::parse(owner).render()
							}
							_ => false,
						}
				}
				_ => false,
			};
			let inner_borrow =
				|t: &Ty| matches!(t, Ty::Ref { mutable: true, .. }) && !self_chain(t);
			if inner_borrow(&t)
				|| matches!(&t, Ty::Path { path, args } if (path == "polars_error::PolarsResult" || path == "core::result::Result") && args.first().is_some_and(inner_borrow))
			{
				out.unsupported(
					c,
					"inner mutable borrow",
					&format!(
						"return ({rc}): no script contract for a mutable borrow that is not the receiver"
					),
				);
				return;
			}
			// `&mut Self` chains return unit: the receiver was mutated in place
			if matches!(&t, Ty::Ref { mutable: true, .. }) {
				Ret {
					materialize: None,
					rust_ty: "()".into(),
					fallible: false,
					conv: "{ let _ = __r; }".into(),
					doc: "unit (receiver mutated in place)".into(),
				}
			} else if let Ty::Path { path, args } = &t {
				if (path == "polars_error::PolarsResult" || path == "core::result::Result")
					&& matches!(args.first(), Some(Ty::Ref { mutable: true, .. }))
				{
					Ret {
						materialize: None,
						rust_ty: "()".into(),
						fallible: true,
						conv: "{ let _ = __r.map_err(Error::from)?; }".into(),
						doc: "result of unit (receiver mutated in place)".into(),
					}
				} else {
					match world.ret(&t, Some(owner), 0) {
						Ok(r) => r,
						Err(Unsupported(why, what)) => {
							out.unsupported(c, why, &format!("return ({what})"));
							return;
						}
					}
				}
			} else {
				match world.ret(&t, Some(owner), 0) {
					Ok(r) => r,
					Err(Unsupported(why, what)) => {
						out.unsupported(c, why, &format!("return ({what})"));
						return;
					}
				}
			}
		}
	};
	let fallible = ret.fallible || params.iter().any(|(_, a)| a.fallible);
	if ret.materialize.is_some() && c.receiver == "&mut self" {
		out.unsupported(
			c,
			"mutable iterator receiver",
			"the receiver's state after a partial materialization cannot be expressed",
		);
		return;
	}
	// the deref route: the receiver is `&*this.0`, a `&dyn Trait`; it needs
	// a reference receiver, `DerefMut` for `&mut self`, and cannot move out
	if deref {
		match c.receiver.as_str() {
			"&self" => {}
			"&mut self" if world.deref_mut.contains(owner) => {}
			"&mut self" => {
				out.unsupported(c, "deref route needs DerefMut", owner);
				return;
			}
			"self" => {
				out.unsupported(c, "deref route cannot move out of a dyn target", owner);
				return;
			}
			other => {
				out.unsupported(c, "deref route needs a receiver", other);
				return;
			}
		}
	}
	let (recv_sig, mut recv_expr, recv_note) = match c.receiver.as_str() {
		"none" => ("".to_string(), None, None),
		// Record 0109: a non-Clone owner is moved out of the Rune value, as
		// Rust moves it; the slot is taken, a later use is an access error.
		"self" if !world.clonable.contains(owner) => (
			format!("this: {}", w.rust),
			Some("this.0".to_string()),
			Some("consumes the Rune value, as in Rust; a later use of it is an access error"),
		),
		"self" => (
			format!("this: &{}", w.rust),
			Some("this.0.clone()".to_string()),
			Some("consumes in Rust; the Rune value is cloned and stays usable"),
		),
		"&self" if deref => (
			format!("this: &{}", w.rust),
			Some("&*this.0".to_string()),
			Some("through Deref, as Rust's autoderef would"),
		),
		"&mut self" if deref => (
			format!("this: &mut {}", w.rust),
			Some("&mut *this.0".to_string()),
			Some("through DerefMut; mutates the Rune value in place"),
		),
		"&self" => (
			format!("this: &{}", w.rust),
			Some("&this.0".to_string()),
			None,
		),
		"&mut self" => (
			format!("this: &mut {}", w.rust),
			Some("&mut this.0".to_string()),
			Some("mutates the Rune value in place"),
		),
		other => {
			out.unsupported(c, "receiver", other);
			return;
		}
	};
	let commit_receiver = c.receiver == "&mut self"
		&& matches!(c.name.as_str(), "apply_mut" | "apply_in_place")
		&& c.params.iter().any(|p| closure_signature(c, p).is_some());
	if commit_receiver {
		recv_expr = Some("&mut __work".into());
	}
	// Record 0108: `InstanceFunction` is implemented through `Function` over
	// (receiver, args...) (rune 0.14.2 function/mod.rs:122-140), so a method
	// shares the free-function limit, receiver included. First reached at v2
	// (`Expr::qcut`, receiver + 5).
	let arity = usize::from(!recv_sig.is_empty()) + params.len();
	if arity > METHOD_ARITY {
		out.unsupported(
			c,
			"arity",
			&format!(
				"method with {arity} arguments including the receiver; Rune binds at most {METHOD_ARITY}"
			),
		);
		return;
	}
	let idx = out.fn_index;
	out.fn_index += 1;
	// trait methods are emitted once per implementor: the owner is part of the identity
	let ident = rust_ident(
		"f",
		&format!(
			"{}#{owner}{}",
			c.canonical_path,
			if deref { "#deref" } else { "" }
		),
		idx,
	);
	let callee = match (callee_override, trait_spell, deref) {
		(Some(o), _, _) => o.to_string(),
		(None, Some(t), true) => format!("<dyn {t}>::{rust_name}"),
		(None, Some(t), false) => format!("<{} as {t}>::{rust_name}", w.spell),
		(None, None, _) => format!("<{}>::{rust_name}", w.spell),
	};
	// record 0126: a rebuilt borrow's receiver (the view, rebuilt for this
	// call) or recipe (the snapshot's function of the same contract)
	let (recv_expr, callee) = match world
		.active
		.get("rebuilt_borrow")
		.and_then(|st| crate::families::rebuilt_borrows::route(&st, &rust_name))
	{
		Some((recv, callee)) => (recv.or(recv_expr), callee),
		None => (recv_expr, callee),
	};
	let mut args: Vec<String> = Vec::new();
	if let Some(r) = &recv_expr {
		args.push(r.clone());
	}
	args.extend(params.iter().map(|(_, a)| a.conv.clone()));
	let sig: Vec<String> = std::iter::once(recv_sig)
		.filter(|s| !s.is_empty())
		.chain(params.iter().map(|(n, a)| format!("{n}: {}", a.rust_ty)))
		.collect();
	let route = routed_binding(world, c, Some(owner));
	let fallible = fallible
		|| params
			.iter()
			.any(|(_, a)| a.pre.iter().any(|p| p.contains('?')));
	let mut pre: String = params
		.iter()
		.filter(|(_, a)| a.shape.starts_with("callback:"))
		.chain(
			params
				.iter()
				.filter(|(_, a)| !a.shape.starts_with("callback:")),
		)
		.flat_map(|(_, a)| a.pre.iter())
		.map(|p| format!("{p} "))
		.collect();
	if commit_receiver {
		pre.push_str("let mut __work = this.0.clone(); ");
	}
	// record 0115: the listed families' checks (`CHECK`), all run in order;
	// the first refusal ends emission, and each preflight goes first
	let listed_fallible = match run_checks(world, c, owner, &ret) {
		Ok((preflight, fallible)) => {
			pre.insert_str(0, &preflight);
			fallible
		}
		Err((category, why)) => {
			out.unsupported(c, category, &why);
			return;
		}
	};
	// record 0119: listed receiver guards; the guarded arguments are
	// converted first, checked against the receiver, then passed on
	let guards: Vec<&crate::families::receiver_guards::ReceiverGuard> = world
		.release
		.families
		.receiver_guards
		.iter()
		.filter(|g| g.path == c.canonical_path)
		.collect();
	let recv_offset = usize::from(recv_expr.is_some());
	let mut guarded = false;
	// the oracle calls a guarded binding with in-range arguments (index 0,
	// length 1); the out-of-range refusals are tested directly
	let mut guarded_shapes: std::collections::BTreeMap<String, &'static str> = Default::default();
	for g in &guards {
		guarded_shapes.insert(sanitize(&g.param), "int0");
		if let Some(p2) = &g.param2 {
			guarded_shapes.insert(sanitize(p2), "int1");
		}
	}
	for g in &guards {
		let mut hoisted: Vec<(String, String)> = Vec::new();
		for p in std::iter::once(&g.param).chain(g.param2.iter()) {
			let Some(k) = params.iter().position(|(n, _)| n == &sanitize(p)) else {
				continue;
			};
			let local = format!("__guard_{}", sanitize(p));
			if !args[recv_offset + k].starts_with("__guard_") {
				pre.push_str(&format!("let {local} = {}; ", args[recv_offset + k]));
				args[recv_offset + k] = local.clone();
			}
			hoisted.push((p.clone(), local));
		}
		let local = |p: &str| hoisted.iter().find(|(n, _)| n == p).map(|(_, l)| l.clone());
		match crate::families::receiver_guards::guard_code(
			g,
			c,
			&format!("{}::{name}", rune_path(w)),
			local,
		) {
			Ok(code) => {
				pre.push_str(&code);
				guarded = true;
			}
			Err(why) => {
				out.unsupported(c, "receiver guard", &why);
				return;
			}
		}
	}
	let fallible = fallible || listed_fallible || guarded;
	let ret_ty = if fallible {
		format!("Result<{}, Error>", ret.rust_ty)
	} else {
		ret.rust_ty.clone()
	};
	// record 0088: a routed `Cow` result is made owned inside the engine closure,
	// so no borrow of the receiver leaves it
	const OWN: &str = "let __r = __r.into_owned(); ";
	let own_in_closure = route
		&& ret.materialize.is_none()
		&& ret.conv.starts_with("{ let __r = __r.into_owned(); ");
	// `{ let __r = __r.into_owned(); X }` becomes `X`: no leftover braces
	let ret_conv = if own_in_closure {
		ret.conv
			.replacen(OWN, "", 1)
			.trim_start_matches("{ ")
			.trim_end_matches(" }")
			.to_string()
	} else {
		ret.conv.clone()
	};
	let body_conv = if fallible {
		format!("Ok({})", ret_conv)
	} else {
		ret_conv
	}
	.replace("__OP__", &name);
	let attr = if c.receiver == "none" {
		format!("#[rune::function(free, path = {}::{name})]", w.rust)
	} else {
		format!("#[rune::function(instance, path = {name})]")
	};
	let doc = doc_line(c);
	let rune = format!("{}::{name}", rune_path(w));
	let arg_docs: Vec<String> = params
		.iter()
		.map(|(n, a)| format!("{n}: {}", a.doc))
		.collect();
	let summary = format!(
		"{name}({}) -> {}{}",
		arg_docs.join(", "),
		ret.doc,
		if fallible { " (fallible)" } else { "" }
	);
	// record 0118: a call that can write to a `Sink` (the receiver is
	// instantiated at it, or an argument is one) runs as one sink operation:
	// committed on `Ok`, discarded on `Err`
	let uses_sink = w.identity.contains(crate::families::std_facts::SINK)
		|| params.iter().any(|(_, a)| a.shape == "sink");
	if uses_sink && ret.materialize.is_some() {
		out.unsupported(
			c,
			"sink operation",
			"an iterator return from a call that writes to a Sink has no operation boundary",
		);
		return;
	}
	let rust_result = c.ret_canonical.as_deref().is_some_and(|r| {
		r.starts_with("polars_error::PolarsResult<") || r.starts_with("core::result::Result<")
	});
	let sink_wrap = |expr: String| -> String {
		if !uses_sink {
			expr
		} else if rust_result {
			format!("support::sink_op(\"{rune}\", || {expr})")
		} else {
			format!("support::sink_op_infallible(\"{rune}\", || {expr})")
		}
	};
	let (pre, call) = if let Some(m) = &ret.materialize {
		// an iterator return: created, driven and converted inside the
		// (routed) block; the receiver of a `&self` iterator stays usable
		let mut pre = pre.clone();
		let mut hoisted = Vec::new();
		for (i, a) in args.iter().enumerate() {
			pre.push_str(&format!("let __arg{i} = {a}; "));
			hoisted.push(format!("__arg{i}"));
		}
		(
			pre,
			materialized_call(
				m,
				&format!("{callee}({})", hoisted.join(", ")),
				&name,
				route,
			),
		)
	} else if route {
		// conversions may fail with `?`, so they run before the closure
		let mut pre = pre.clone();
		let mut hoisted = Vec::new();
		for (i, a) in args.iter().enumerate() {
			pre.push_str(&format!("let __arg{i} = {a}; "));
			hoisted.push(format!("__arg{i}"));
		}
		// record 0082: a callback that copies slices does so on every
		// invocation; one guard around the whole engine-thread call makes
		// those copies share a single cumulative bound
		let copies = args
			.iter()
			.chain(std::iter::once(&pre))
			.any(|a| a.contains("support::copy_slice("));
		let body = if copies {
			format!(
				"{{ let __slices = support::SliceBudget::enter(); {callee}({}) }}",
				hoisted.join(", ")
			)
		} else {
			format!("{callee}({})", hoisted.join(", "))
		};
		let body = if own_in_closure {
			format!("{body}.into_owned()")
		} else {
			body
		};
		let body = sink_wrap(body);
		let call = format!("crate::engine::run(\"{rune}\", move || {body})");
		(
			pre,
			if fallible {
				format!("{call}.map_err(Error::engine)?")
			} else {
				format!("crate::engine::infallible({call}, \"{rune}\")")
			},
		)
	} else if uses_sink {
		// conversions may fail with `?`, so they run before the sink operation
		let mut pre = pre.clone();
		let mut hoisted = Vec::new();
		for (i, a) in args.iter().enumerate() {
			pre.push_str(&format!("let __arg{i} = {a}; "));
			hoisted.push(format!("__arg{i}"));
		}
		(pre, sink_wrap(format!("{callee}({})", hoisted.join(", "))))
	} else {
		(pre, format!("{callee}({})", args.join(", ")))
	};
	let commit = if commit_receiver {
		"this.0 = __work; "
	} else {
		""
	};
	let docline = if doc.is_empty() {
		String::new()
	} else {
		format!("/// {doc}\n")
	};
	// record 0123: arguments converted or refused before the body (a listed
	// `Into` target's sources); a refusal is a VM error, so the binding returns
	// `VmResult` around its unchanged body and script-visible return type
	let vm_checks: String = params
		.iter()
		.filter_map(|(_, a)| a.vm_check.as_deref())
		.map(|c| c.replace("__OP__", &name))
		.collect();
	if vm_checks.is_empty() {
		writeln!(out.functions, "{docline}/// Polars: `{}`. {}\n{attr}\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {commit}{body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
	} else {
		writeln!(out.functions, "{docline}/// Polars: `{}`. {}\n{attr}\nfn {ident}({}) -> rune::runtime::VmResult<{ret_ty}> {{ {vm_checks}rune::runtime::VmResult::Ok((|| -> {ret_ty} {{ {pre}let __r = {call}; {commit}{body_conv} }})()) }}", c.canonical_path, summary, sig.join(", ")).unwrap();
	}
	out.registrations
		.push(format!("m.function_meta({ident})?;"));
	out.catalogue.push((
		rune.clone(),
		if doc.is_empty() {
			summary.clone()
		} else {
			format!("{summary}: {doc}")
		},
	));
	out.taken.insert(key, c.canonical_path.clone());
	let mut note = recv_note.map(|s| s.to_string());
	if route {
		note = Some(format!(
			"{}{}",
			note.map(|n| n + "; ").unwrap_or_default(),
			"routed through the engine thread"
		));
	}
	if name != rust_name {
		note = Some(format!(
			"{}renamed: `{rust_name}` is a Rune keyword",
			note.map(|n| n + "; ").unwrap_or_default()
		));
	}
	if fallible && !ret.fallible {
		note = Some(format!(
			"{}{}",
			note.map(|n| n + "; ").unwrap_or_default(),
			"fallible in Rune because an argument conversion can fail"
		));
	}
	let info = OracleInfo {
		rune_owner: Some(rune_path(w)),
		rune_name: name.clone(),
		receiver: c.receiver.clone(),
		owner: Some((owner.to_string(), w.rust.clone())),
		callee,
		params: c
			.params
			.iter()
			.zip(params.iter())
			.map(|(p, (n, a))| {
				let shape = match guarded_shapes.get(n) {
					Some(s) if a.shape == "int" => s.to_string(),
					_ => a.shape.clone(),
				};
				(shape, p.ty_canonical.clone())
			})
			.collect(),
		param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
		ret_canonical: c.ret_canonical.clone(),
		ret_rust: ret.rust_ty.clone(),
		fallible,
		generics: generics.clone(),
		implementors: vec![],
		deref: false,
	};
	out.generated_with(c, &rune, note, info);
	if let Some(binding) = out.entries.last_mut().unwrap().bindings.first_mut() {
		binding.route_reason = binding_route_reason(world, c, route);
		if route {
			binding.reentry = Some(if fallible { "error" } else { "unwind" }.into());
		}
	}
}
