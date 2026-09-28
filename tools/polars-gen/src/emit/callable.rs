use crate::emit::free::emit_free;
use crate::emit::{
	Emitted, OracleInfo, RouteException, doc_line, generics_map, rune_path, rust_ident,
	unused_generic,
};
use crate::families::bounds::{hash_token_scope, null_aware_return_matches};
use crate::families::callbacks::{
	binding_route_reason, callback_arg, callback_gate, closure_signature, routed_binding,
};
use crate::families::free_instantiations::emit_free_instantiations;
use crate::families::protocols::emit_foreign;
use crate::families::receivers::proven_trait_receivers;
use crate::families::serde::{emit_serde, serde_trait};
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
	#[allow(clippy::type_complexity)]
	struct BitmapScope<'a>(
		&'a std::cell::Cell<bool>,
		&'a std::cell::RefCell<Option<(String, String)>>,
		&'a std::cell::RefCell<Option<String>>,
		&'a std::cell::Cell<bool>,
		&'a std::cell::Cell<bool>,
		&'a std::cell::RefCell<Option<(String, bool, Option<String>)>>,
	);
	impl Drop for BitmapScope<'_> {
		fn drop(&mut self) {
			self.0.set(false);
			*self.1.borrow_mut() = None;
			*self.2.borrow_mut() = None;
			self.3.set(false);
			self.4.set(false);
			*self.5.borrow_mut() = None;
		}
	}
	world.cow_ok.set(
		world
			.release
			.cow_returns
			.iter()
			.any(|r| r.key == c.key && r.path == c.canonical_path),
	);
	world.bounded_ok.set(
		world
			.release
			.bounded_readbacks
			.iter()
			.any(|r| r.key == c.key && r.path == c.canonical_path && !r.cite.trim().is_empty()),
	);
	world.bitmap_ok.set(
		world
			.release
			.bitmap_returns
			.iter()
			.any(|p| *p == c.canonical_path),
	);
	*world.bitmap_input.borrow_mut() = world
		.release
		.bitmap_inputs
		.iter()
		.find(|b| b.path == c.canonical_path)
		.map(|b| (format!("{}::{}", last(&c.owner), c.name), b.length.clone()));
	*world.iter_return.borrow_mut() = world
		.release
		.iterator_returns
		.iter()
		.find(|r| r.path == c.canonical_path)
		.map(|r| r.item.clone());
	let _bitmap_scope = BitmapScope(
		&world.bitmap_ok,
		&world.bitmap_input,
		&world.iter_return,
		&world.cow_ok,
		&world.bounded_ok,
		&world.hash_token,
	);
	match hash_token_scope(&world.release, c) {
		Ok(scope) => {
			*world.hash_token.borrow_mut() = scope.map(|(ret, param)| (c.name.clone(), ret, param))
		}
		Err(reason) => {
			out.unsupported(c, "release policy", &reason);
			return;
		}
	}
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
			let (proven, refusals) = proven_trait_receivers(world, c);
			let impls: Vec<&String> = proven.iter().collect();
			if impls.is_empty() {
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
			let mut exceptions: Vec<RouteException> = Vec::new();
			let derefs: Vec<&String> = world
				.deref_targets
				.get(trait_)
				.map(|v| v.iter().collect())
				.unwrap_or_default();
			let candidates: Vec<(&String, bool)> = impls
				.iter()
				.map(|o| (*o, false))
				.chain(derefs.iter().map(|o| (*o, true)))
				.collect();
			for (owner, deref) in candidates {
				let route: &'static str = if deref { "deref" } else { "implementor" };
				let before = out.entries.len();
				emit_method(world, out, c, owner, Some(&tspell), deref);
				let e = out.entries.pop().unwrap();
				debug_assert_eq!(before, out.entries.len());
				if e.status == "generated" {
					let callee = if deref {
						e.oracle.as_ref().map(|i| i.callee.clone())
					} else {
						None
					};
					done.push((e.rune.clone().unwrap(), owner.clone(), route, callee));
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
	// record 0085: instantiated pairs reach here without `emit_callable`
	#[allow(clippy::type_complexity)]
	struct BitmapScope<'a>(
		&'a std::cell::Cell<bool>,
		&'a std::cell::RefCell<Option<(String, String)>>,
		&'a std::cell::RefCell<Option<String>>,
		&'a std::cell::Cell<bool>,
		&'a std::cell::Cell<bool>,
		&'a std::cell::RefCell<Option<(String, bool, Option<String>)>>,
	);
	impl Drop for BitmapScope<'_> {
		fn drop(&mut self) {
			self.0.set(false);
			*self.1.borrow_mut() = None;
			*self.2.borrow_mut() = None;
			self.3.set(false);
			self.4.set(false);
			*self.5.borrow_mut() = None;
		}
	}
	world.cow_ok.set(
		world
			.release
			.cow_returns
			.iter()
			.any(|r| r.key == c.key && r.path == c.canonical_path),
	);
	world.bounded_ok.set(
		world
			.release
			.bounded_readbacks
			.iter()
			.any(|r| r.key == c.key && r.path == c.canonical_path && !r.cite.trim().is_empty()),
	);
	world.bitmap_ok.set(
		world
			.release
			.bitmap_returns
			.iter()
			.any(|p| *p == c.canonical_path),
	);
	*world.bitmap_input.borrow_mut() = world
		.release
		.bitmap_inputs
		.iter()
		.find(|b| b.path == c.canonical_path)
		.map(|b| (format!("{}::{}", last(&c.owner), c.name), b.length.clone()));
	*world.iter_return.borrow_mut() = world
		.release
		.iterator_returns
		.iter()
		.find(|r| r.path == c.canonical_path)
		.map(|r| r.item.clone());
	let _bitmap_scope = BitmapScope(
		&world.bitmap_ok,
		&world.bitmap_input,
		&world.iter_return,
		&world.cow_ok,
		&world.bounded_ok,
		&world.hash_token,
	);
	match hash_token_scope(&world.release, c) {
		Ok(scope) => {
			*world.hash_token.borrow_mut() = scope.map(|(ret, param)| (c.name.clone(), ret, param))
		}
		Err(reason) => {
			out.unsupported(c, "release policy", &reason);
			return;
		}
	}
	if c.is_async {
		out.unsupported(c, "async", &c.name);
		return;
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
	// record 0106: the layout's preflight goes before the Polars call too
	if let Some((op, kind, _)) = world.layout.borrow().clone() {
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::layout_snapshot")
			|| c.receiver != "&self"
		{
			out.unsupported(
				c,
				"layout snapshot",
				"needs an unrouted `&self` call and the layout-snapshot conversion",
			);
			return;
		}
		let preflight = match kind.as_str() {
			"bool" => "support::preflight_bool".to_string(),
			"str" => "support::preflight_str".into(),
			"binary" => "support::preflight_binview".into(),
			"binary_offset" => "support::preflight_binary_offset".into(),
			n => format!("support::preflight_numeric::<{n}>"),
		};
		pre.insert_str(
			0,
			&format!("let __total = {preflight}(this.0.chunks(), \"{op}\")?; "),
		);
	}
	// record 0105: the consuming iterator runs on a clone; its preflight goes
	// first, before the clone in the call and before Polars
	if let Some((op, kind)) = world.owned_iter.borrow().clone() {
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::owned_snapshot")
			|| c.receiver != "self"
		{
			out.unsupported(
				c,
				"owned iterator snapshot",
				"needs an unrouted consuming call and the owned-snapshot conversion",
			);
			return;
		}
		let preflight = match kind.as_str() {
			"bool" => "support::preflight_bool".to_string(),
			"str" => "support::preflight_str".into(),
			"binary" => "support::preflight_binview".into(),
			"binary_offset" => "support::preflight_binary_offset".into(),
			n => format!("support::preflight_numeric::<{n}>"),
		};
		pre.insert_str(
			0,
			&format!("let __total = {preflight}(this.0.chunks(), \"{op}\")?; "),
		);
	}
	// record 0104: likewise for the borrowed indexed chunk view
	if world.view_snapshot.borrow().is_some()
		&& (routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::view_snapshot")
			|| c.receiver != "&self")
	{
		out.unsupported(
			c,
			"view snapshot",
			"needs an unrouted `&self` call and the view-snapshot conversion",
		);
		return;
	}
	// record 0103: likewise for the borrowed typed-chunk iterator
	if world.iter_snapshot.borrow().is_some()
		&& (routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::iter_snapshot")
			|| c.receiver != "&self")
	{
		out.unsupported(
			c,
			"iterator snapshot",
			"needs an unrouted `&self` call and the iterator-snapshot conversion",
		);
		return;
	}
	// record 0102: likewise for the one borrowed array of `downcast_as_array`
	if world.array_snapshot.borrow().is_some()
		&& (routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::array_snapshot")
			|| c.receiver != "&self")
	{
		out.unsupported(
			c,
			"array snapshot",
			"needs an unrouted `&self` call and the array-snapshot conversion",
		);
		return;
	}
	// record 0101: likewise for the one borrowed chunk of `downcast_get`
	if world.indexed_chunk.borrow().is_some()
		&& (routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::indexed_snapshot")
			|| c.receiver != "&self")
	{
		out.unsupported(
			c,
			"indexed chunk snapshot",
			"needs an unrouted `&self` call and the indexed-snapshot conversion",
		);
		return;
	}
	// record 0099: the chunk borrow ends with the call, so the copy must happen in it
	if world.chunk_snapshot.borrow().is_some()
		&& (routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::chunk_snapshot")
			|| c.receiver != "&self")
	{
		out.unsupported(
			c,
			"chunk snapshot",
			"needs an unrouted `&self` call and the chunk-snapshot conversion",
		);
		return;
	}
	// record 0097: Polars's signed slice offsets need the receiver length within i64
	let sized_self_op = world.sized_self.borrow().clone();
	if let Some(op) = &sized_self_op {
		if c.receiver != "&self"
			|| c.ret_canonical.as_deref().map(|r| ty::parse(r).render())
				!= Some(ty::parse(owner).render())
		{
			out.unsupported(
				c,
				"sized-self method",
				&format!(
					"needs a `&self` receiver returning the owner, got {}",
					c.ret_canonical.as_deref().unwrap_or("()")
				),
			);
			return;
		}
		pre.insert_str(
			0,
			&format!("support::signed_len(this.0.len(), \"{op}\")?; "),
		);
	}
	// record 0096: the whole null-aware result is bounded before Polars allocates it
	let null_aware_op = world.null_aware.borrow().as_ref().map(|(op, _)| op.clone());
	if let Some(op) = &null_aware_op {
		if c.receiver != "&self"
			|| !ret.fallible
			|| !ret.conv.starts_with("__r.either(")
			|| !null_aware_return_matches(
				c.ret_canonical.as_deref(),
				&world.null_aware.borrow().as_ref().unwrap().1,
			) {
			out.unsupported(
				c,
				"null-aware return",
				"needs a `&self` receiver and the null-aware conversion",
			);
			return;
		}
		pre.insert_str(
			0,
			&format!("support::null_aware_bound(this.0.len(), \"{op}\")?; "),
		);
	}
	let fallible = fallible || sized_self_op.is_some();
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
		let call = format!("crate::engine::run(\"{rune}\", move || {body})");
		(
			pre,
			if fallible {
				format!("{call}.map_err(Error::engine)?")
			} else {
				format!("crate::engine::infallible({call}, \"{rune}\")")
			},
		)
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
	writeln!(out.functions, "{docline}/// Polars: `{}`. {}\n{attr}\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {commit}{body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
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
			.map(|(p, (_, a))| (a.shape.clone(), p.ty_canonical.clone()))
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
