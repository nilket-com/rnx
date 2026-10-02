//! Record 0117 (stage B): methods of generic traits. A generic trait's
//! impls each bind the trait's own parameters (`trait_params`) to their
//! recorded `trait_args`; for every wrapped receiver an impl is proven for
//! (head unification, every bound discharged), the method's signature is
//! resolved for that (receiver, impl) pair: the trait parameters replaced
//! by the impl's arguments, the impl's associated types by its own
//! bindings, the impl's parameters by the proof's substitution. What is
//! left open refuses the arm, except a scalar impl generic whose bound is
//! proven for the script natives, which is instantiated at each of them
//! (0113's rule). Each resulting arm is a concrete callable.
use crate::families::generic_impls::SCALAR_OP_BOUNDS;
use crate::model::Callable;
use crate::ty;
use crate::world::proof::{Applicability, is_param, split_head, subst_params};
use crate::world::{World, Wrapper};
use std::collections::{BTreeMap, BTreeSet};

/// Bounds proven for `i64` and `f64` (num-traits 0.2.19, pinned in both
/// locks): 0113's operator bounds, and `ToPrimitive` alone (`cast.rs`
/// `impl_to_primitive_int!(i64)`, `impl_to_primitive_float!(f64)`).
pub(crate) const SCALAR_NATIVE_BOUNDS: &[&str] = &["num_traits::cast::ToPrimitive"];
pub(crate) const SCALAR_NATIVES: &[&str] = &["i64", "f64"];

/// One way to call a generic trait's method on one receiver.
#[derive(Clone)]
pub(crate) struct TraitArm {
	/// The method with every parameter and the return resolved.
	pub(crate) callable: Callable,
}

fn scalar_bound(b: &str) -> bool {
	SCALAR_OP_BOUNDS.contains(&b) || SCALAR_NATIVE_BOUNDS.contains(&b)
}

/// Every proven arm of a generic trait's method, by receiver (wrapper key),
/// and the named reasons of the impls and receivers that were refused.
pub(crate) fn generic_trait_arms(
	world: &World,
	c: &Callable,
) -> (BTreeMap<String, Vec<TraitArm>>, Vec<String>) {
	let mut out: BTreeMap<String, Vec<TraitArm>> = BTreeMap::new();
	let mut why: Vec<String> = Vec::new();
	let Some(tr) = world.types.get(&c.owner) else {
		return (out, why);
	};
	// a generic trait's parameters come from the inventory (record 0117);
	// without them no trait argument can be bound, so nothing is admitted
	if tr.trait_params.is_empty() {
		why.push(
			"generic trait with no recorded trait parameters (inventory predates record 0117)"
				.into(),
		);
		return (out, why);
	}
	// the inventory can record one impl twice (it did before record 0117);
	// by coherence two records with one identity are one impl
	let mut impl_seen: BTreeSet<String> = BTreeSet::new();
	for ti in &tr.impls {
		if ti.blanket {
			continue; // rustdoc's synthesized per-type copy; the generic item is decided (stage D)
		}
		if !impl_seen.insert(format!(
			"{}|{:?}|{:?}|{:?}",
			ti.for_type, ti.trait_args, ti.bounds, ti.where_predicates
		)) {
			continue;
		}
		let head = ty::parse(&ti.for_type).render();
		if tr.trait_params.len() != ti.trait_args.len() {
			why.push(format!(
				"{head}: {} trait arguments for the trait's {} parameters",
				ti.trait_args.len(),
				tr.trait_params.len()
			));
			continue;
		}
		// the trait's parameters, as this impl binds them
		let tparams: BTreeMap<String, String> = tr
			.trait_params
			.iter()
			.cloned()
			.zip(ti.trait_args.iter().cloned())
			.collect();
		if let Some(bad) = ti
			.trait_args
			.iter()
			.find(|a| a.contains(" as ") && !a.starts_with('<'))
		{
			why.push(format!("{head}: trait argument `{bad}` is not modelled"));
			continue;
		}
		let rewrite = |s: &str| -> String {
			let mut s = subst_params(s, &tparams);
			for (name, val) in &ti.assoc_types {
				s = s.replace(&format!("Self::{name}"), val);
			}
			s
		};
		let mut probe = c.clone();
		probe.impl_head = Some(head.clone());
		probe.impl_bounds = ti.bounds.clone();
		probe.impl_where = ti.where_predicates.clone();
		for p in probe.params.iter_mut() {
			p.ty_canonical = rewrite(&p.ty_canonical);
		}
		probe.ret_canonical = probe.ret_canonical.as_deref().map(rewrite);
		// the impl's scalar generics are decided per native; any other open
		// impl generic that the head does not bind refuses the impl
		let (base, head_args) = split_head(&head);
		let head_params: BTreeSet<String> = if is_param(&head) {
			[head.clone()].into()
		} else {
			head_args.iter().filter(|a| is_param(a)).cloned().collect()
		};
		let scalars: Vec<(String, String)> = ti
			.bounds
			.iter()
			.filter(|(n, _)| !head_params.contains(n))
			.cloned()
			.collect();
		if let Some((n, b)) = scalars.iter().find(|(_, b)| !scalar_bound(b)) {
			why.push(format!(
				"{head}: impl generic `{n}: {b}` is not instantiable"
			));
			continue;
		}
		if scalars.len() > 1 {
			why.push(format!("{head}: {} scalar impl generics", scalars.len()));
			continue;
		}
		let mut cands: Vec<(&String, &Wrapper)> = world
			.wrappers
			.iter()
			.filter(|(_, w)| {
				if is_param(&head) {
					false // a bare-`T` impl is a blanket over a bound (stage D)
				} else if head_params.is_empty() {
					w.identity == head
				} else {
					w.rule == "alias"
						&& w.base.as_deref() == Some(base.as_str())
						&& w.identity.starts_with(&format!("{base}<"))
				}
			})
			.collect();
		cands.sort_by(|a, b| a.0.cmp(b.0));
		let mut seen: BTreeSet<&str> = BTreeSet::new();
		for (path, w) in cands {
			if !seen.insert(w.identity.as_str()) {
				continue;
			}
			// the proof binds the head's parameters; a scalar generic is bound
			// here, per native, before the bounds are discharged
			let natives: Vec<Option<&'static str>> = if scalars.is_empty() {
				vec![None]
			} else {
				SCALAR_NATIVES.iter().map(|n| Some(*n)).collect()
			};
			for native in natives {
				let mut p2 = probe.clone();
				if let (Some(n), Some((g, _))) = (native, scalars.first()) {
					let s: BTreeMap<String, String> = [(g.clone(), n.to_string())].into();
					for p in p2.params.iter_mut() {
						p.ty_canonical = subst_params(&p.ty_canonical, &s);
					}
					p2.ret_canonical = p2.ret_canonical.as_deref().map(|r| subst_params(r, &s));
					p2.impl_bounds.retain(|(bn, _)| bn != g);
					p2.impl_where
						.retain(|wp| !wp.starts_with(&format!("{g}: ")));
				}
				// every arm is proven, concrete heads included: head, bounds and
				// where-predicates, for this exact receiver
				let (a, subst) = world.applicability(&p2, &w.identity);
				match a {
					Applicability::Proven => {}
					Applicability::Rejected(_) => continue,
					Applicability::Unresolved(e) => {
						why.push(format!("{head} on {}: {e}", w.identity));
						continue;
					}
				}
				// record 0118: a method's `Self: B` (rustdoc lists it with the
				// generics) is proven for this receiver, then dropped; it is
				// the receiver, never inferred from an argument
				let mut self_ok = Ok(());
				for (g, b) in p2.generics_canonical.iter().filter(|(g, _)| g == "Self") {
					let _ = g;
					match world.holds_all(&w.identity, b, 0) {
						Applicability::Proven => {}
						Applicability::Rejected(e) | Applicability::Unresolved(e) => {
							self_ok = Err(e);
						}
					}
				}
				if let Err(e) = self_ok {
					why.push(format!("{head} on {}: `Self` bound: {e}", w.identity));
					continue;
				}
				p2.generics_canonical.retain(|(g, _)| g != "Self");
				let (params, ret) = match world.substitute_signature(&p2, &w.identity, &subst) {
					Ok(x) => x,
					Err(e) => {
						why.push(format!("{head} on {}: {e}", w.identity));
						continue;
					}
				};
				let norm = |t: String| -> String {
					let mut t = t;
					for (identity, alias) in &world.by_identity {
						if t.contains(identity.as_str()) {
							t = t.replace(identity.as_str(), alias);
						}
					}
					t
				};
				let mut syn = c.clone();
				syn.owner = c.owner.clone();
				for (q, t) in syn.params.iter_mut().zip(params) {
					q.ty_canonical = norm(t);
				}
				syn.ret_canonical = ret.map(norm);
				syn.impl_head = None;
				syn.impl_bounds.clear();
				syn.impl_where.clear();
				// the receiver's `Self` bounds were proven above
				syn.generics_canonical.retain(|(g, _)| g != "Self");
				out.entry(path.clone())
					.or_default()
					.push(TraitArm { callable: syn });
			}
		}
	}
	why.sort();
	why.dedup();
	(out, why)
}

/// What a script passes for one dispatched parameter, by Rune value kind.
/// Two kinds are disjoint when they differ: a Rune value has exactly one.
#[derive(Clone, Debug, PartialEq)]
enum ArgKind {
	/// A wrapped value, borrowed (the wrapper's Rust type).
	Wrapped(String),
	Str,
	Int,
	Float,
	Bool,
	/// A Rune list, converted by the arm's own helper.
	List,
}

impl ArgKind {
	fn of(rust_ty: &str, shape: &str) -> Option<ArgKind> {
		match rust_ty {
			"&str" | "String" => Some(ArgKind::Str),
			"i64" => Some(ArgKind::Int),
			"f64" => Some(ArgKind::Float),
			"bool" => Some(ArgKind::Bool),
			"rune::Value" if shape.starts_with("vec(") => Some(ArgKind::List),
			t if shape.starts_with("W:") && t.starts_with('&') && !t.starts_with("&mut") => {
				Some(ArgKind::Wrapped(t[1..].trim().to_string()))
			}
			_ => None,
		}
	}
	fn doc(&self) -> String {
		match self {
			ArgKind::Wrapped(w) => w.rsplit("__").next().unwrap_or(w).to_string(),
			ArgKind::Str => "string".into(),
			ArgKind::Int => "int".into(),
			ArgKind::Float => "float".into(),
			ArgKind::Bool => "bool".into(),
			ArgKind::List => "list".into(),
		}
	}
	/// The condition that selects this kind of the `rune::Value` `v` (binding
	/// `__d` where it converts), and the argument the arm's helper receives.
	fn test(&self, v: &str, rust_ty: &str) -> (String, String) {
		let by = |t: &str| format!("let Ok(__d) = rune::from_value::<{t}>({v}.clone())");
		match self {
			ArgKind::Wrapped(w) => (
				format!("let Ok(__d) = {v}.borrow_ref::<{w}>()"),
				"&__d".into(),
			),
			ArgKind::List => (
				format!("{v}.borrow_ref::<rune::runtime::Vec>().is_ok()"),
				format!("{v}.clone()"),
			),
			ArgKind::Str if rust_ty == "&str" => (by("String"), "&__d".into()),
			ArgKind::Str => (by("String"), "__d".into()),
			ArgKind::Int => (by("i64"), "__d".into()),
			ArgKind::Float => (by("f64"), "__d".into()),
			ArgKind::Bool => (by("bool"), "__d".into()),
		}
	}
}

/// Split a generated parameter list at its top-level commas.
fn split_sig(s: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut depth = 0i32;
	let mut cur = String::new();
	for ch in s.chars() {
		match ch {
			'<' | '(' | '[' => depth += 1,
			'>' | ')' | ']' => depth -= 1,
			',' if depth == 0 => {
				out.push(cur.trim().to_string());
				cur.clear();
				continue;
			}
			_ => {}
		}
		cur.push(ch);
	}
	if !cur.trim().is_empty() {
		out.push(cur.trim().to_string());
	}
	out
}

/// One emitted arm: its helper text (unregistered), parameter list and
/// return type, as `emit_method` wrote them.
struct ArmText {
	helper: String,
	text: String,
	sig: Vec<String>,
	ret: String,
	fallible: bool,
	attr: String,
}

fn arm_text(functions: &str, ident: &str, helper: &str) -> Option<ArmText> {
	let head = format!("fn {ident}(");
	let at = functions.find(&head)?;
	let attr_line = functions[..at].trim_end().lines().last()?.to_string();
	if !attr_line.starts_with("#[rune::function(") {
		return None;
	}
	let open = at + head.len();
	let mut depth = 1i32;
	let mut close = None;
	for (k, ch) in functions[open..].char_indices() {
		match ch {
			'(' => depth += 1,
			')' => {
				depth -= 1;
				if depth == 0 {
					close = Some(open + k);
					break;
				}
			}
			_ => {}
		}
	}
	let close = close?;
	let rest = functions[close + 1..].strip_prefix(" -> ")?;
	let ret = rest[..rest.find(" { ")?].to_string();
	let (ret, fallible) = match ret
		.strip_prefix("Result<")
		.and_then(|r| r.strip_suffix(", Error>"))
	{
		Some(inner) => (inner.to_string(), true),
		None => (ret, false),
	};
	let text = functions
		.replacen(&format!("{attr_line}\n"), "", 1)
		.replacen(&head, &format!("fn {helper}("), 1);
	Some(ArmText {
		helper: helper.to_string(),
		text,
		sig: split_sig(&functions[open..close]),
		ret,
		fallible,
		attr: attr_line,
	})
}

/// Record 0117 (stage B): several proven impls of one generic trait on one
/// receiver become one Rune function when their arms differ in exactly one
/// parameter whose script kinds are pairwise disjoint (a wrapped value, a
/// string, an int, a float, a bool) and agree on everything else. The
/// function takes that parameter as a `rune::Value` and calls the arm its
/// kind selects; a value of no listed kind is a `ConversionError`. Anything
/// else is refused by name, never one arm picked. Pushes exactly one entry.
pub(crate) fn emit_trait_dispatch(
	world: &World,
	out: &mut crate::emit::Emitted,
	c: &Callable,
	arms: &[TraitArm],
	owner: &str,
	tspell: &str,
) {
	let refuse = |out: &mut crate::emit::Emitted, why: String| {
		out.unsupported(c, "trait arms", &why);
	};
	let n = arms[0].callable.params.len();
	if arms.iter().any(|a| a.callable.params.len() != n) {
		return refuse(out, format!("{} impls with different arities", arms.len()));
	}
	let differ: Vec<usize> = (0..n)
		.filter(|&i| {
			let t0 = &arms[0].callable.params[i].ty_canonical;
			arms.iter()
				.any(|a| &a.callable.params[i].ty_canonical != t0)
		})
		.collect();
	let [k] = differ.as_slice() else {
		return refuse(
			out,
			format!(
				"{} impls differ in {} parameters; a dispatch decides by exactly one",
				arms.len(),
				differ.len()
			),
		);
	};
	let k = *k;
	let pname = crate::text::sanitize(&arms[0].callable.params[k].name);
	// each arm's script kind for the dispatched parameter
	let mut kinds: Vec<(ArgKind, String)> = Vec::new();
	for a in arms {
		let p = &a.callable.params[k];
		if crate::families::callbacks::closure_signature(&a.callable, p).is_some() {
			return refuse(
				out,
				format!("`{}` is a callback; it is not kind-dispatchable", p.name),
			);
		}
		let arg = match world.arg(
			&crate::ty::parse(&p.ty_canonical),
			&pname,
			&crate::emit::generics_map(&a.callable),
			Some(owner),
			0,
		) {
			Ok(x) => x,
			Err(crate::world::mapping::Unsupported(why, what)) => {
				return refuse(out, format!("arm `{}`: {why} ({what})", p.ty_canonical));
			}
		};
		let Some(kind) = ArgKind::of(&arg.rust_ty, &arg.shape) else {
			return refuse(
				out,
				format!(
					"arm `{}` takes `{}` ({}), which is not kind-dispatchable",
					p.ty_canonical, arg.rust_ty, arg.shape
				),
			);
		};
		if let Some((_, other)) = kinds.iter().find(|(x, _)| *x == kind) {
			return refuse(
				out,
				format!(
					"arms overlap in script: `{other}` and `{}` both take a {}",
					p.ty_canonical,
					kind.doc()
				),
			);
		}
		kinds.push((kind, arg.rust_ty));
	}
	// every arm emitted by the ordinary path, into a scratch output; any
	// refusal refuses the whole receiver with that arm's reason
	let mut texts: Vec<ArmText> = Vec::new();
	// the first arm's entry, catalogue lines and consuming paths
	type FirstArm = (crate::emit::Entry, Vec<(String, String)>, Vec<String>);
	let mut first: Option<FirstArm> = None;
	let mut fn_index = out.fn_index;
	let mut key_taken = None;
	for (i, a) in arms.iter().enumerate() {
		let mut s = crate::emit::Emitted {
			from_names: out.from_names.clone(),
			functions: String::new(),
			registrations: vec![],
			catalogue: vec![],
			consuming: vec![],
			entries: vec![],
			taken: out.taken.clone(),
			fn_index,
			frozen_ids: out.frozen_ids.clone(),
		};
		crate::emit::callable::emit_method(world, &mut s, &a.callable, owner, Some(tspell), false);
		fn_index = s.fn_index;
		let e = s.entries.pop().unwrap();
		if e.status != "generated" {
			return refuse(
				out,
				format!(
					"arm `{}`: {}",
					a.callable.params[k].ty_canonical,
					e.reason.unwrap_or_default()
				),
			);
		}
		let Some(ident) = s
			.registrations
			.first()
			.and_then(|r| r.strip_prefix("m.function_meta("))
			.and_then(|r| r.strip_suffix(")?;"))
			.map(str::to_string)
		else {
			return refuse(out, "an arm registered no function".into());
		};
		let Some(t) = arm_text(&s.functions, &ident, &format!("{ident}_arm{i}")) else {
			return refuse(
				out,
				"an arm's generated text has no recognizable signature".into(),
			);
		};
		if i == 0 {
			key_taken = s
				.taken
				.iter()
				.find(|(key, _)| !out.taken.contains_key(*key))
				.map(|(key, v)| (key.clone(), v.clone(), ident.clone()));
			first = Some((e, s.catalogue, s.consuming));
		}
		texts.push(t);
	}
	let (Some((mut entry, catalogue, consuming)), Some((key, took, ident))) = (first, key_taken)
	else {
		return refuse(out, "the first arm took no name".into());
	};
	// everything but the dispatched parameter agrees
	let recv = usize::from(arms[0].callable.receiver != "none");
	for t in &texts[1..] {
		let a: Vec<&String> = texts[0]
			.sig
			.iter()
			.enumerate()
			.filter(|(j, _)| *j != k + recv)
			.map(|(_, s)| s)
			.collect();
		let b: Vec<&String> = t
			.sig
			.iter()
			.enumerate()
			.filter(|(j, _)| *j != k + recv)
			.map(|(_, s)| s)
			.collect();
		if a != b || t.attr != texts[0].attr {
			return refuse(out, "arms differ outside the dispatched parameter".into());
		}
		if t.ret != texts[0].ret {
			return refuse(
				out,
				format!(
					"arms return different types (`{}`, `{}`): one Rune slot cannot hold them",
					texts[0].ret, t.ret
				),
			);
		}
	}
	let names: Vec<String> = texts[0]
		.sig
		.iter()
		.map(|p| p.split(':').next().unwrap_or("").trim().to_string())
		.collect();
	let mut sig = texts[0].sig.clone();
	sig[k + recv] = format!("{pname}: rune::Value");
	let mut body = String::new();
	for (t, (kind, rust_ty)) in texts.iter().zip(&kinds) {
		let (test, pass) = kind.test(&pname, rust_ty);
		let mut args = names.clone();
		args[k + recv] = pass;
		let call = format!("{}({})", t.helper, args.join(", "));
		let call = if t.fallible {
			call
		} else {
			format!("Ok({call})")
		};
		body.push_str(&format!("if {test} {{ return {call}; }} "));
	}
	let expected: Vec<String> = kinds.iter().map(|(k, _)| k.doc()).collect();
	let mut functions = String::new();
	for t in &texts {
		functions.push_str(&t.text);
	}
	use std::fmt::Write as _;
	writeln!(
		functions,
		"/// Polars: `{}`. One Rune function over {} impls of the generic trait: `{pname}` dispatches on its kind ({}).\n{}\nfn {ident}({}) -> Result<{}, Error> {{ {body}Err(Error::conversion(\"{pname}: expected one of {}\")) }}",
		c.canonical_path,
		arms.len(),
		expected.join(", "),
		texts[0].attr,
		sig.join(", "),
		texts[0].ret,
		expected.join(", ")
	)
	.unwrap();
	out.functions.push_str(&functions);
	out.registrations
		.push(format!("m.function_meta({ident})?;"));
	out.catalogue.extend(catalogue);
	out.consuming.extend(consuming);
	out.taken.insert(key, took);
	out.fn_index = fn_index;
	entry.fallible = Some(true);
	if let Some(info) = entry.oracle.as_mut() {
		info.fallible = true;
	}
	entry.note = Some(format!(
		"{}one Rune function over {} impls: `{pname}` dispatches on its kind ({}); a value of no listed kind is a ConversionError",
		entry.note.map(|n| n + "; ").unwrap_or_default(),
		arms.len(),
		expected.join(", ")
	));
	out.entries.push(entry);
}

/// Record 0117 (stage B): generic-trait arms, controls that fail closed. Two
/// disjoint arms become one dispatching function (a duplicated impl record
/// is one arm); proven scalar natives are arms of their own kinds; arms that
/// overlap in script, an impl generic with an unproven bound, and an impl
/// whose trait arguments do not fit the trait's parameters are refused by
/// name; an impl on another type is never an arm of this receiver.
pub(crate) fn generic_traits_self_test() {
	use crate::emit::Emitted;
	use crate::emit::callable::emit_callable;
	use crate::model::{Inventory, Param, Supporting, TraitImpl};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	let sup = |path: &str, kind: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: kind.to_string(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let series = "polars_core::series::Series";
	let frame = "polars_core::frame::DataFrame";
	let imp = |head: &str, args: &[&str], bounds: &[(&str, &str)]| TraitImpl {
		trait_args: args.iter().map(|a| a.to_string()).collect(),
		for_type: head.to_string(),
		blanket: false,
		bounds: bounds
			.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		where_predicates: vec![],
		assoc_types: vec![],
	};
	let gtrait = |name: &str, params: &[&str], impls: Vec<TraitImpl>| {
		let mut t = sup(&format!("polars_core::ops::{name}"), "trait");
		t.generic = true;
		t.trait_params = params.iter().map(|p| p.to_string()).collect();
		t.impls = impls;
		t
	};
	let traits = vec![
		// a wrapped arm and a string arm, the string impl recorded twice; an
		// impl on another type
		gtrait(
			"Cmp",
			&["Rhs"],
			vec![
				imp(series, &["&polars_core::series::Series"], &[]),
				imp(series, &["&str"], &[]),
				imp(series, &["&str"], &[]),
				imp(frame, &["i64"], &[]),
			],
		),
		// a scalar generic proven for both natives, and a wrapped arm
		gtrait(
			"Num",
			&["Rhs"],
			vec![
				imp(
					series,
					&["Rhs"],
					&[("Rhs", "num_traits::cast::ToPrimitive")],
				),
				imp(series, &["&polars_core::series::Series"], &[]),
			],
		),
		// two arms a script cannot tell apart
		gtrait(
			"Over",
			&["Rhs"],
			vec![
				imp(series, &["&str"], &[]),
				imp(series, &["alloc::string::String"], &[]),
			],
		),
		// an impl generic whose bound no native is proven for
		gtrait(
			"Gen",
			&["Rhs"],
			vec![imp(
				series,
				&["Rhs"],
				&[("Rhs", "polars_core::ops::Unproven")],
			)],
		),
		// one trait argument for two parameters
		gtrait("Bad", &["A", "B"], vec![imp(series, &["i64"], &[])]),
		// record 0118: methods with a `Self` bound, proven for the receiver
		gtrait("Sz", &["Rhs"], vec![imp(series, &["&str"], &[])]),
		gtrait("Uns", &["Rhs"], vec![imp(series, &["&str"], &[])]),
		gtrait("Gfn", &["Rhs"], vec![imp(series, &["&str"], &[])]),
	];
	let method = |tr: &str, name: &str, param: &str| Callable {
		key: format!("k:{tr}"),
		kind: "trait_method".into(),
		krate: "polars_core".into(),
		owner: format!("polars_core::ops::{tr}"),
		name: name.into(),
		canonical_path: format!("polars_core::ops::{tr}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![Param {
			name: "rhs".into(),
			ty: param.into(),
			ty_canonical: param.into(),
		}],
		ret: Some("bool".into()),
		ret_canonical: Some("bool".into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![],
		impl_head: None,
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: false,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![series.into(), frame.into()],
		trait_reachable: true,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let callables = vec![
		method("Cmp", "cmp_to", "Rhs"),
		method("Num", "num_to", "Rhs"),
		method("Over", "over_to", "Rhs"),
		method("Gen", "gen_to", "Rhs"),
		method("Bad", "bad_to", "A"),
		{
			let mut m = method("Sz", "sz_to", "Rhs");
			m.generics_canonical = vec![("Self".into(), "core::marker::Sized".into())];
			m
		},
		{
			let mut m = method("Uns", "uns_to", "Rhs");
			m.generics_canonical = vec![("Self".into(), "polars_core::ops::Unrecorded".into())];
			m
		},
		{
			let mut m = method("Gfn", "gfn_to", "Rhs");
			m.generics_canonical = vec![("G".into(), "core::marker::Send".into())];
			m
		},
	];
	let mut supporting = vec![sup(series, "struct"), sup(frame, "struct")];
	supporting.extend(traits);
	let inv = Inventory {
		callables,
		supporting,
		provenance: None,
	};
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let buckets = ["mechanical", "generic"];
	let world = World::new(&inv, &release, &buckets);
	let emit = |tr: &str| {
		let mut e = Emitted {
			from_names: BTreeMap::new(),
			functions: String::new(),
			registrations: vec![],
			catalogue: vec![],
			consuming: vec![],
			entries: vec![],
			taken: BTreeMap::new(),
			fn_index: 0,
			frozen_ids: Default::default(),
		};
		emit_callable(
			&world,
			&mut e,
			inv.callables
				.iter()
				.find(|c| c.key == format!("k:{tr}"))
				.unwrap(),
			&buckets,
		);
		(e.entries[0].clone(), e.functions, e.registrations)
	};
	// one dispatching function per receiver; the duplicate record is no arm
	let (e, f, r) = emit("Cmp");
	assert_eq!(
		e.status,
		"generated",
		"{:?} {:?}",
		e.reason,
		e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>()
	);
	let receivers: BTreeSet<String> = e
		.bindings
		.iter()
		.filter_map(|b| b.receiver.clone())
		.collect();
	assert_eq!(
		receivers,
		[frame.to_string(), series.to_string()].into(),
		"each impl is an arm of its own head only"
	);
	assert_eq!(r.len(), 2, "one registration per receiver: {r:?}");
	assert_eq!(
		f.matches("#[rune::function(instance, path = cmp_to)]")
			.count(),
		2,
		"{f}"
	);
	assert!(
		f.contains("_arm0(") && f.contains("_arm1(") && !f.contains("_arm2("),
		"two arms on Series, the duplicate record folded: {f}"
	);
	assert!(
		f.contains("rhs.borrow_ref::<W_polars_core__series__Series>()")
			&& f.contains("rune::from_value::<String>(rhs.clone())")
			&& f.contains("Error::conversion(\"rhs: expected one of Series, string\")"),
		"the wrapped and string kinds, then a ConversionError: {f}"
	);
	assert!(
		f.contains("rhs: i64) -> bool"),
		"the frame's single arm is an ordinary binding: {f}"
	);
	// proven natives are arms of their own kinds
	let (e, f, _) = emit("Num");
	assert_eq!(e.status, "generated", "{:?}", e.reason);
	assert!(
		f.contains("rune::from_value::<i64>(rhs.clone())")
			&& f.contains("rune::from_value::<f64>(rhs.clone())")
			&& f.contains("_arm2("),
		"i64, f64 and the wrapped arm: {f}"
	);
	// refused by name, with no text
	for (tr, want) in [
		("Over", "arms overlap in script"),
		("Gen", "is not instantiable"),
		("Bad", "1 trait arguments for the trait's 2 parameters"),
	] {
		let (e, f, r) = emit(tr);
		assert_eq!(e.status, "unsupported", "{tr} must be refused");
		let why = format!(
			"{} {}",
			e.reason.clone().unwrap_or_default(),
			e.exceptions
				.iter()
				.map(|x| x.reason.clone())
				.collect::<Vec<_>>()
				.join("; ")
		);
		assert!(why.contains(want), "{tr}: {why}");
		assert!(f.is_empty() && r.is_empty(), "{tr}: no binding text");
	}
	// record 0118: `Self: Sized` is proven for the sized receiver and dropped;
	// an unproven `Self` bound refuses the arm with no text; a method generic
	// no argument binds stays refused
	let (e, f, _) = emit("Sz");
	assert_eq!(
		e.status,
		"generated",
		"{:?} {:?}",
		e.reason,
		e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>()
	);
	assert!(f.contains("path = sz_to"), "{f}");
	for (tr, want) in [
		("Uns", "`Self` bound"),
		("Gfn", "not inferable from arguments: G"),
	] {
		let (e, f, r) = emit(tr);
		assert_eq!(e.status, "unsupported", "{tr} must be refused");
		let why = format!(
			"{} {}",
			e.reason.clone().unwrap_or_default(),
			e.exceptions
				.iter()
				.map(|x| x.reason.clone())
				.collect::<Vec<_>>()
				.join("; ")
		);
		assert!(why.contains(want), "{tr}: {why}");
		assert!(f.is_empty() && r.is_empty(), "{tr}: no binding text");
	}
	println!("generic-traits self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn generic_traits() {
		super::generic_traits_self_test();
	}
}
