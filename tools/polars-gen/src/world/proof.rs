use crate::emit::generics_map;
use crate::families::bounds::external_bound_entry;
use crate::model;
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::text::{split_top, split_top_plus};
use crate::ty::last;
use crate::world::World;
use crate::world::mapping::SCALARS;
use std::collections::{BTreeMap, BTreeSet};

/// Record 0075 gate 3 controls: equivalent aliases resolve to one wrapper;
/// same-name distinct types resolve to two Rune paths.
// ---------------------------------------------------------------- instantiation (record 0076)

/// The result of checking one impl against one alias instantiation. Only
/// `Proven` yields a binding; the other two are counted exceptions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "result", content = "reason")]
pub(crate) enum Applicability {
	Proven,
	Rejected(String),
	Unresolved(String),
}

impl Applicability {
	pub(crate) fn and(self, other: Applicability) -> Applicability {
		match (self, other) {
			(Applicability::Proven, o) => o,
			(Applicability::Rejected(r), _) => Applicability::Rejected(r),
			(Applicability::Unresolved(_), Applicability::Rejected(r)) => {
				Applicability::Rejected(r)
			}
			(u @ Applicability::Unresolved(_), _) => u,
		}
	}
}

/// `Base<args>` split at the top level: the base path and its arguments.
pub(crate) fn split_head(t: &str) -> (String, Vec<String>) {
	let t = t.trim();
	match t.find('<') {
		Some(i) if t.ends_with('>') => (t[..i].to_string(), split_top(&t[i + 1..t.len() - 1])),
		_ => (t.to_string(), vec![]),
	}
}

/// A generic parameter name: one identifier, no path.
pub(crate) fn is_param(s: &str) -> bool {
	!s.is_empty()
		&& !s.contains("::")
		&& s.chars().all(|c| c.is_alphanumeric() || c == '_')
		&& s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// The generic parameters a type leaves open: identifier tokens of
/// parameter form that start a path (`T`, `T::Native`), never a trailing
/// path segment (`datatypes::Int64Type`).
pub(crate) fn open_params(s: &str) -> Vec<String> {
	let mut out = Vec::new();
	let b = s.as_bytes();
	let mut i = 0;
	while i < b.len() {
		let c = b[i] as char;
		if c.is_alphanumeric() || c == '_' {
			let start = i;
			while i < b.len() && ((b[i] as char).is_alphanumeric() || b[i] == b'_') {
				i += 1;
			}
			let tok = &s[start..i];
			let after_path = start >= 2 && &s[start - 2..start] == "::";
			if !after_path && is_param(tok) {
				out.push(tok.to_string());
			}
		} else {
			i += 1;
		}
	}
	out
}

/// Record 0117 (stage D): whether a bare-parameter impl head `impl<T: B>
/// Trait for T` constrains `T` at all: a bound other than `?Sized`, or a
/// where-predicate on `T`. An unbounded one would claim every type.
pub(crate) fn blanket_is_bounded(p: &str, bounds: &[(String, String)], wh: &[String]) -> bool {
	bounds.iter().any(|(n, b)| {
		n == p
			&& split_top_plus(b)
				.iter()
				.any(|x| !x.is_empty() && x != "?core::marker::Sized" && x != "?Sized")
	}) || wh.iter().any(|w| w.starts_with(&format!("{p}: ")))
}

/// Replace whole-identifier generic parameters by their bindings.
pub(crate) fn subst_params(s: &str, subst: &BTreeMap<String, String>) -> String {
	let mut out = String::new();
	let b = s.as_bytes();
	let mut i = 0;
	while i < b.len() {
		if b[i].is_ascii_alphabetic() || b[i] == b'_' {
			let start = i;
			while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
				i += 1;
			}
			let tok = &s[start..i];
			let prev = if start >= 2 { &s[start - 2..start] } else { "" };
			let next = if i + 2 <= b.len() { &s[i..i + 2] } else { "" };
			// a later path segment is not a parameter (`foo::T`); a leading one
			// is (`T::Native` becomes `Int64Type::Native` for the projection step)
			let _ = next;
			if prev != "::" {
				if let Some(v) = subst.get(tok) {
					out.push_str(v);
					continue;
				}
			}
			out.push_str(tok);
		} else {
			out.push(b[i] as char);
			i += 1;
		}
	}
	out
}

/// A trait bound `path<A, B, Assoc = X, …>` split into the trait path, its
/// positional trait arguments (record 0117) and its associated-type
/// constraints. `Err` for any other form (a lifetime, a parenthesized
/// signature): anything the grammar does not model is unresolved, never
/// weakened.
pub(crate) fn split_bound(
	bound: &str,
) -> Result<(String, Vec<String>, Vec<(String, String)>), String> {
	if bound.contains('(') || bound.starts_with('\'') {
		return Err(format!("bound syntax `{bound}` is not modelled"));
	}
	let (path, args) = split_head(bound);
	let mut positional = Vec::new();
	let mut cons = Vec::new();
	for a in &args {
		match a.split_once(" = ") {
			Some((k, v)) => cons.push((k.trim().to_string(), v.trim().to_string())),
			None if a.starts_with('\'') => {
				return Err(format!(
					"lifetime argument `{a}` in `{bound}` is not modelled"
				));
			}
			None => positional.push(a.trim().to_string()),
		}
	}
	Ok((path, positional, cons))
}

/// The core derivable traits a builtin type implements, by type: floats
/// have no `Eq`, `Ord` or `Hash`; the owned string is not `Copy`; unit
/// has no `Display`; unsized `str` is neither `Clone` nor `Default`, and
/// a `&T` is `Copy` and `Clone` while a `&mut T` is neither. Anything
/// else is not a builtin and gets no answer here.
pub(crate) fn core_trait_on_builtin(ty: &str, short: &str) -> Option<bool> {
	let float = matches!(ty, "f32" | "f64");
	let owned_string = ty == "alloc::string::String";
	let str_slice = ty == "str";
	let unit = ty == "()";
	let shared_ref = ty.starts_with('&') && !ty.starts_with("&mut ");
	let mut_ref = ty.starts_with("&mut ");
	if shared_ref || mut_ref {
		return match short {
			"Copy" | "Clone" => Some(shared_ref),
			_ => None,
		};
	}
	if !(float || owned_string || str_slice || unit || SCALARS.contains(&ty)) {
		return None;
	}
	Some(match short {
		"Debug" | "PartialEq" | "PartialOrd" => true,
		"Clone" | "Default" => !str_slice,
		"Eq" | "Ord" | "Hash" => !float,
		"Copy" => !(owned_string || str_slice),
		"Display" => !unit,
		_ => return None,
	})
}

/// Whether a canonical type is `Sized`: references, scalars, unit, the
/// owned string, `PlSmallStr`, and reachable structs, enums and unions
/// (bare or instantiated) are; `str`, slices and trait objects are not;
/// anything else is unknown.
pub(crate) fn sizedness(world: &World, ty: &str) -> Option<bool> {
	let t = ty.trim();
	if t.starts_with('&') {
		return Some(true);
	}
	if t == "str" || t.starts_with('[') || t.starts_with("dyn ") || t.starts_with("impl ") {
		return Some(false);
	}
	if t == "()"
		|| SCALARS.contains(&t)
		|| t == "alloc::string::String"
		|| t == "polars_utils::pl_str::PlSmallStr"
		|| t.starts_with('(')
	{
		return Some(true);
	}
	let (base, _) = split_head(t);
	match world.types.get(&base) {
		Some(s) if matches!(s.kind.as_str(), "struct" | "enum" | "union") => Some(true),
		Some(s) if s.kind == "type_alias" => s
			.alias_target
			.as_deref()
			.and_then(|target| sizedness(world, target)),
		_ => None,
	}
}

impl World {
	/// The associated type `assoc` that some recorded impl on `ty` binds;
	/// `None` when no impl binds it or two impls disagree.
	pub(crate) fn assoc_of(
		&self,
		ty: &str,
		assoc: &str,
		trait_hint: Option<&str>,
	) -> Option<String> {
		let mut found: BTreeSet<String> = BTreeSet::new();
		for (tp, t) in &self.types {
			if t.kind != "trait" {
				continue;
			}
			if let Some(h) = trait_hint {
				// rustdoc spells some qualified projections with the trait's bare
				// name (`<T as PolarsNumericType>::Native`); a bare hint matches by
				// last segment, and the value must still be unique
				let bare = !h.contains("::") && tp.rsplit("::").next() == Some(h);
				if tp != h && !bare {
					continue;
				}
			}
			for i in &t.impls {
				if i.for_type == ty {
					if let Some((_, v)) = i.assoc_types.iter().find(|(n, _)| n == assoc) {
						found.insert(v.clone());
					}
				}
			}
		}
		if found.len() == 1 {
			found.into_iter().next()
		} else {
			None
		}
	}

	/// Resolve `<P as Trait>::Assoc` and `P::Assoc` projections after
	/// parameter substitution; `Err` names the first one left open.
	pub(crate) fn resolve_projections(&self, s: &str) -> Result<String, String> {
		let mut cur = s.to_string();
		for _ in 0..8 {
			let mut changed = false;
			// qualified projection: <X as Trait>::Assoc, at any depth (record 0117:
			// inside generic arguments too, `Option<<T as Tr>::Native>`); an
			// inner projection is resolved before the one that contains it
			let mut search = 0;
			while let Some(off) = cur[search..].find('<') {
				let i = search + off;
				let rest = &cur[i..];
				// the matching '>' for this '<', and ` as ` at its own depth
				let mut depth = 0;
				let mut close = None;
				let mut as_i = None;
				for (k, ch) in rest.char_indices() {
					match ch {
						'<' => depth += 1,
						'>' => {
							depth -= 1;
							if depth == 0 {
								close = Some(k);
								break;
							}
						}
						' ' if depth == 1 && as_i.is_none() && rest[k..].starts_with(" as ") => {
							as_i = Some(k)
						}
						_ => {}
					}
				}
				let Some(close) = close else { break };
				let (Some(as_i), true) = (as_i, rest[close + 1..].starts_with("::")) else {
					search = i + 1;
					continue;
				};
				let inner_ty = rest[1..as_i].trim().to_string();
				let trait_path = rest[as_i + 4..close].trim().to_string();
				if inner_ty.contains(" as ") {
					search = i + 1; // the inner projection first
					continue;
				}
				let after = &rest[close + 3..];
				let assoc: String = after
					.chars()
					.take_while(|c| c.is_alphanumeric() || *c == '_')
					.collect();
				if assoc.is_empty() {
					search = i + 1;
					continue;
				}
				if is_param(&inner_ty) {
					return Err(format!(
						"projection on an unbound parameter: <{inner_ty} as {trait_path}>::{assoc}"
					));
				}
				let Some(v) = self.assoc_of(&inner_ty, &assoc, Some(&trait_path)) else {
					return Err(format!(
						"no recorded `{assoc}` for `{inner_ty}` in `{trait_path}`"
					));
				};
				let end = i + close + 3 + assoc.len();
				cur = format!("{}{}{}", &cur[..i], v, &cur[end..]);
				changed = true;
				search = i;
			}
			// short projection: X::Assoc where X is a concrete path known to some impl
			let mut from = 0;
			loop {
				let Some(off) = cur[from..].find("::") else {
					break;
				};
				let i = from + off;
				let before = &cur[..i];
				let lhs_start = before
					.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
					.map(|k| k + 1)
					.unwrap_or(0);
				let lhs = before[lhs_start..].trim_start_matches(':').to_string();
				let assoc: String = cur[i + 2..]
					.chars()
					.take_while(|c| c.is_alphanumeric() || *c == '_')
					.collect();
				let upper = assoc.chars().next().is_some_and(|c| c.is_ascii_uppercase());
				if !lhs.is_empty() && upper {
					if let Some(v) = self.assoc_of(&lhs, &assoc, None) {
						let end = i + 2 + assoc.len();
						cur = format!("{}{}{}", &cur[..lhs_start], v, &cur[end..]);
						changed = true;
						from = lhs_start;
						continue;
					}
					if is_param(&lhs) {
						return Err(format!(
							"projection on an unbound parameter: {lhs}::{assoc}"
						));
					}
					// a projection on a known type that no recorded impl binds
					if self.types.get(&lhs).is_some_and(|t| t.kind != "trait") {
						return Err(format!("no recorded `{assoc}` for `{lhs}`"));
					}
				}
				from = i + 2;
			}
			let out = cur.clone();
			cur = out;
			if !changed {
				break;
			}
		}
		Ok(cur)
	}

	/// Whether `ty` satisfies one bound, from the inventory's recorded impls.
	pub(crate) fn holds(&self, ty: &str, bound: &str, depth: usize) -> Applicability {
		if depth > 4 {
			return Applicability::Unresolved(format!("nesting limit at `{ty}: {bound}`"));
		}
		let (tpath, targs, cons) = match split_bound(bound) {
			Ok(x) => x,
			Err(e) => return Applicability::Unresolved(e),
		};
		let short = last(&tpath);
		// record 0118: the closed std-facts table decides its own keys first
		let key = (ty.to_string(), bound.to_string());
		// record 0120: a listed concrete array's own parameter bound
		if self.std_facts.contains(&key) || self.array_facts.contains(&key) {
			return Applicability::Proven;
		}
		if tpath.starts_with("core::") || tpath.starts_with("alloc::") || tpath.starts_with("std::")
		{
			if let Some(a) = targs.first() {
				return Applicability::Unresolved(format!(
					"trait argument `{a}` in `{bound}` is not modelled"
				));
			}
			if !cons.is_empty() {
				return Applicability::Unresolved(format!(
					"core trait `{tpath}` with associated-type constraints is not modelled"
				));
			}
			return match short {
				"Sized" => match sizedness(self, ty) {
					Some(true) => Applicability::Proven,
					Some(false) => Applicability::Rejected(format!("`{ty}` is not `Sized`")),
					None => Applicability::Unresolved(format!(
						"the sizedness of `{ty}` is not established"
					)),
				},
				"Send" | "Sync" | "Unpin" | "UnwindSafe" | "RefUnwindSafe" => {
					Applicability::Unresolved(format!("auto trait `{short}` is not recorded"))
				}
				"Clone" | "Debug" | "Default" | "PartialEq" | "Eq" | "Hash" | "Copy"
				| "PartialOrd" | "Ord" | "Display" => {
					let (base, _) = split_head(ty);
					if ty == "polars_utils::pl_str::PlSmallStr" {
						// a polars type: only its recorded derives and impls count
					} else if let Some(holds) = core_trait_on_builtin(ty, short) {
						return if holds {
							Applicability::Proven
						} else {
							Applicability::Rejected(format!("`{ty}` does not implement `{short}`"))
						};
					}
					let derived = self
						.types
						.get(&base)
						.is_some_and(|t| !ty.contains('<') && t.derived.iter().any(|d| d == short));
					let implemented = self
						.impls
						.get(&base)
						.is_some_and(|v| v.iter().any(|(n, f, _)| n == short && f == ty));
					if derived || implemented {
						Applicability::Proven
					} else {
						Applicability::Unresolved(format!(
							"no recorded impl of `{short}` for `{ty}`"
						))
					}
				}
				_ => Applicability::Unresolved(format!("core trait `{tpath}` is not modelled")),
			};
		}
		let Some(t) = self.types.get(&tpath).filter(|t| t.kind == "trait") else {
			return Applicability::Unresolved(format!("trait `{tpath}` is outside the inventory"));
		};
		// record 0117 (stage C): every recorded impl whose head matches `ty`
		// and whose trait arguments match the bound's is a candidate; the
		// bound holds only when exactly one candidate is proven (the order of
		// the records never decides), and several proven are ambiguous
		// the bound's arguments, with any projection resolved; one that does
		// not resolve (or names a parameter no caller bound) is unresolved
		let mut targs = targs;
		for a in targs.iter_mut() {
			match self.resolve_projections(a) {
				Ok(x) => *a = x,
				Err(e) => {
					return Applicability::Unresolved(format!(
						"trait argument `{a}` in `{bound}`: {e}"
					));
				}
			}
			if !open_params(a).is_empty() {
				return Applicability::Unresolved(format!(
					"trait argument `{a}` in `{bound}` names an unbound parameter"
				));
			}
		}
		// the bound's associated-type constraints, resolved the same way; a
		// comparison with an unresolved projection would reject falsely
		let mut cons = cons;
		for (a, x) in cons.iter_mut() {
			match self.resolve_projections(x) {
				Ok(v) if open_params(&v).is_empty() => *x = v,
				Ok(v) => {
					return Applicability::Unresolved(format!(
						"constraint `{a} = {v}` in `{bound}` names an unbound parameter"
					));
				}
				Err(e) => {
					return Applicability::Unresolved(format!(
						"constraint `{a} = {x}` in `{bound}`: {e}"
					));
				}
			}
		}
		if t.trait_params.len() != targs.len() {
			// the bound's arguments cannot be bound by position (an inventory
			// that predates record 0117 records no trait parameters)
			return Applicability::Unresolved(match targs.first() {
				Some(a) => format!(
					"trait argument `{a}` in `{bound}` is not modelled: `{tpath}` records {} parameters",
					t.trait_params.len()
				),
				None => format!("`{tpath}` has trait parameters; the bound `{bound}` names none"),
			});
		}
		let mut seen: BTreeSet<String> = BTreeSet::new();
		let mut proven: Vec<String> = Vec::new();
		let mut unresolved: Vec<String> = Vec::new();
		let mut rejected: Vec<String> = Vec::new();
		let mut matched_head = false;
		for i in t.impls.iter().filter(|i| !i.blanket) {
			let identity = format!(
				"{}|{}|{:?}|{:?}",
				i.for_type,
				i.trait_args.join(","),
				i.bounds,
				i.where_predicates
			);
			if !seen.insert(identity) {
				continue; // the same impl recorded twice
			}
			let params: BTreeSet<String> = i.bounds.iter().map(|(n, _)| n.clone()).collect();
			let mut subst = if i.for_type == ty {
				BTreeMap::new()
			} else if i.for_type.contains('<') {
				match unify(&i.for_type, ty, &params) {
					Ok(s) => s,
					Err(_) => continue,
				}
			} else if is_param(&i.for_type) && params.contains(&i.for_type) {
				// record 0117 (stage D): `impl<T: B> Trait for T` is a candidate
				// for every type, decided by its bounds; an unbounded one is open
				if !blanket_is_bounded(&i.for_type, &i.bounds, &i.where_predicates) {
					matched_head = true;
					unresolved.push(format!(
						"unbounded blanket impl of `{tpath}` would claim `{ty}`"
					));
					continue;
				}
				[(i.for_type.clone(), ty.to_string())].into()
			} else {
				continue;
			};
			matched_head = true;
			// the bound's trait arguments against this impl's
			if i.trait_args.len() != targs.len() {
				unresolved.push(format!(
					"`{ty}: {tpath}` impl records {} trait arguments, the bound {}",
					i.trait_args.len(),
					targs.len()
				));
				continue;
			}
			let mut args_ok = true;
			let mut args_open = false;
			for (pat, want) in i.trait_args.iter().zip(&targs) {
				let pat_s = subst_params(pat, &subst);
				let pat_s = match self.resolve_projections(&pat_s) {
					Ok(x) => x,
					Err(e) => {
						unresolved.push(format!("`{ty}: {tpath}` impl argument `{pat}`: {e}"));
						args_ok = false;
						args_open = true;
						break;
					}
				};
				if params.contains(&pat_s) {
					subst.insert(pat_s.clone(), want.clone());
				} else if pat_s == *want {
				} else if pat_s.contains('<') && want.contains('<') {
					match unify(&pat_s, want, &params) {
						Ok(more) => {
							for (k, v) in more {
								match subst.get(&k) {
									Some(prev) if *prev != v => args_ok = false,
									_ => {
										subst.insert(k, v);
									}
								}
							}
						}
						Err(_) => args_ok = false,
					}
				} else {
					args_ok = false;
				}
			}
			if args_open {
				continue;
			}
			if !args_ok {
				rejected.push(format!(
					"`{ty}: {tpath}<{}>` is not this impl's `<{}>`",
					targs.join(", "),
					i.trait_args.join(", ")
				));
				continue;
			}
			let mut r = Applicability::Proven;
			for (p, b) in &i.bounds {
				let Some(pt) = subst.get(p) else { continue };
				let b = subst_params(b, &subst);
				r = r.and(self.holds_all(pt, &b, depth + 1));
			}
			for w in &i.where_predicates {
				r = r.and(self.predicate(w, &subst, ty, depth + 1));
			}
			for (a, x) in &cons {
				match i.assoc_types.iter().find(|(n, _)| n == a) {
					Some((_, v)) => {
						let v = subst_params(v, &subst);
						let v = self.resolve_projections(&v).unwrap_or(v);
						if &v != x {
							r = r.and(Applicability::Rejected(format!(
								"`{ty}: {tpath}` binds `{a} = {v}`, not `{x}`"
							)));
						}
					}
					None => {
						r = r.and(Applicability::Unresolved(format!(
							"`{ty}: {tpath}` does not record `{a}`"
						)))
					}
				}
			}
			match r {
				Applicability::Proven => proven.push(format!(
					"{} as {tpath}<{}>",
					i.for_type,
					i.trait_args.join(", ")
				)),
				Applicability::Unresolved(e) => unresolved.push(e),
				Applicability::Rejected(e) => rejected.push(e),
			}
		}
		match proven.len() {
			1 => return Applicability::Proven,
			0 => {}
			_ => {
				return Applicability::Unresolved(format!(
					"`{ty}: {bound}` is ambiguous: {} impls are proven ({})",
					proven.len(),
					proven.join("; ")
				));
			}
		}
		if let Some(e) = unresolved.into_iter().next() {
			return Applicability::Unresolved(e);
		}
		if matched_head {
			if let Some(e) = rejected.into_iter().next() {
				return Applicability::Rejected(e);
			}
		}
		if t.impls.iter().any(|i| i.blanket) {
			return Applicability::Unresolved(format!(
				"`{tpath}` has a blanket impl; `{ty}` is not proven by a direct one"
			));
		}
		if t.impls.is_empty() {
			return Applicability::Unresolved(format!(
				"no impl of `{tpath}` is recorded under this configuration"
			));
		}
		Applicability::Rejected(format!("no recorded impl of `{tpath}` for `{ty}`"))
	}

	pub(crate) fn holds_all(&self, ty: &str, bounds: &str, depth: usize) -> Applicability {
		let mut r = Applicability::Proven;
		for b in split_top_plus(bounds) {
			if b.is_empty() {
				continue;
			}
			r = r.and(self.holds(ty, &b, depth));
		}
		r
	}

	/// One `where` predicate under a substitution: `X: bounds` or `A = B`.
	pub(crate) fn predicate(
		&self,
		w: &str,
		subst: &BTreeMap<String, String>,
		self_ty: &str,
		depth: usize,
	) -> Applicability {
		let mut s2 = subst.clone();
		s2.insert("Self".into(), self_ty.to_string());
		if let Some((lhs, bounds)) = w.split_once(": ") {
			let lhs = subst_params(lhs, &s2);
			let lhs = match self.resolve_projections(&lhs) {
				Ok(x) => x,
				Err(e) => return Applicability::Unresolved(e),
			};
			if lhs
				.split(|c: char| !(c.is_alphanumeric() || c == '_'))
				.any(|tok| is_param(tok) && !s2.contains_key(tok) && tok != "Self")
			{
				// a parameter the substitution leaves open
				let open: Vec<&str> = lhs
					.split(|c: char| !(c.is_alphanumeric() || c == '_'))
					.filter(|tok| is_param(tok) && !s2.contains_key(*tok))
					.collect();
				if !open.is_empty() && open.iter().any(|o| !lhs.contains(&format!("::{o}"))) {
					return Applicability::Unresolved(format!(
						"`{w}` involves a parameter the head does not bind"
					));
				}
			}
			let bounds = subst_params(bounds, &s2);
			self.holds_all(&lhs, &bounds, depth)
		} else if let Some((a, b)) = w.split_once(" = ") {
			let a = subst_params(a, &s2);
			let b = subst_params(b, &s2);
			match (self.resolve_projections(&a), self.resolve_projections(&b)) {
				(Ok(x), Ok(y)) if x == y => Applicability::Proven,
				(Ok(x), Ok(y)) => Applicability::Rejected(format!("`{w}`: `{x}` is not `{y}`")),
				(Err(e), _) | (_, Err(e)) => Applicability::Unresolved(e),
			}
		} else {
			Applicability::Unresolved(format!("predicate form `{w}` is not modelled"))
		}
	}

	/// Whether a method's impl applies to an alias instantiation, with the
	/// substitution of the impl's parameters that makes it apply.
	pub(crate) fn applicability(
		&self,
		c: &Callable,
		identity: &str,
	) -> (Applicability, BTreeMap<String, String>) {
		let Some(head) = &c.impl_head else {
			return (
				Applicability::Unresolved("no impl head recorded".into()),
				BTreeMap::new(),
			);
		};
		let mut params: BTreeSet<String> = c
			.impl_bounds
			.iter()
			.map(|(n, _)| n.clone())
			.filter(|n| n != "Self")
			.collect();
		// parameters that appear in the head without a bound
		let (_, hargs) = split_head(head);
		for a in &hargs {
			if is_param(a) {
				params.insert(a.clone());
			}
		}
		// record 0117 (stage D): a bare-parameter head binds the whole type; its
		// bounds and where-predicates below decide, and an unbounded one is refused
		if is_param(head) && params.contains(head) {
			if !blanket_is_bounded(head, &c.impl_bounds, &c.impl_where) {
				return (
					Applicability::Unresolved(format!(
						"unbounded blanket impl `impl<{head}> for {head}` would claim every type"
					)),
					BTreeMap::new(),
				);
			}
		}
		let subst = if is_param(head) && params.contains(head) {
			Ok([(head.clone(), identity.to_string())].into())
		} else {
			unify_with(self, head, identity, &params)
		};
		let subst = match subst {
			Ok(s) => s,
			Err(e) if e.starts_with("unresolved: ") => {
				return (Applicability::Unresolved(e), BTreeMap::new());
			}
			Err(e) => return (Applicability::Rejected(e), BTreeMap::new()),
		};
		let mut r = Applicability::Proven;
		for (p, b) in &c.impl_bounds {
			if b.is_empty() {
				continue;
			}
			let pt = if p == "Self" {
				identity.to_string()
			} else {
				match subst.get(p) {
					Some(t) => t.clone(),
					None => {
						return (
							Applicability::Unresolved(format!(
								"bound on `{p}`, which the head does not bind"
							)),
							subst,
						);
					}
				}
			};
			let b = subst_params(b, &subst);
			r = r.and(self.holds_all(&pt, &b, 0));
		}
		// record 0098: a listed, well-formed entry discharges its cited clauses
		// on its listed float pairs, once `T::Native` resolves to that native
		let external = external_bound_entry(&self.release, c);
		if let Some(Err(why)) = &external {
			return (
				Applicability::Unresolved(format!("external bound: {why}")),
				subst,
			);
		}
		let external = external.and_then(Result::ok);
		for w in &c.impl_where {
			if let Some(e) = external.filter(|e| e.discharge.contains(w)) {
				let owner = subst.get("T").cloned().unwrap_or_default();
				let Some((_, native)) = e.pairs.iter().find(|(t, _)| *t == owner) else {
					// an unlisted type keeps the ordinary proof (a nonfloat is rejected by its owner bound)
					r = r.and(match self.predicate(w, &subst, identity, 0) {
						Applicability::Unresolved(_) => Applicability::Unresolved(format!(
							"external bound: `{identity}` is not a listed pair"
						)),
						other => other,
					});
					continue;
				};
				let lhs = subst_params("T::Native", &subst);
				match self.resolve_projections(&lhs) {
					Ok(n) if n == *native => {}
					Ok(n) => {
						r = r.and(Applicability::Unresolved(format!(
							"external bound: `T::Native` is `{n}`, not the listed `{native}`"
						)));
					}
					Err(e) => {
						r = r.and(Applicability::Unresolved(format!("external bound: {e}")));
					}
				}
				continue;
			}
			r = r.and(self.predicate(w, &subst, identity, 0));
		}
		(r, subst)
	}

	/// The method's signature under the substitution, every projection
	/// resolved; `Err` names what stays open.
	pub(crate) fn substitute_signature(
		&self,
		c: &Callable,
		identity: &str,
		subst: &BTreeMap<String, String>,
	) -> Result<(Vec<String>, Option<String>), String> {
		let mut s2 = subst.clone();
		s2.insert("Self".into(), identity.to_string());
		let generic_bounds = generics_map(c);
		let one = |t: &str| -> Result<String, String> {
			let t = generic_bounds.get(t).map(String::as_str).unwrap_or(t);
			let t = subst_params(t, &s2);
			let t = self.resolve_projections(&t)?;
			// an associated-type constraint key (`Item = …`) is not a parameter
			let stripped = {
				let mut r = String::new();
				let mut rest = t.as_str();
				while let Some(i) = rest.find(" = ") {
					let (pre, post) = rest.split_at(i);
					let k = pre
						.rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
						.map(|x| x + 1)
						.unwrap_or(0);
					r.push_str(&pre[..k]);
					r.push_str(" = ");
					rest = &post[3..];
				}
				r.push_str(rest);
				r
			};
			let mut left: Vec<&str> = stripped
				.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == ':'))
				.filter(|tok| is_param(tok))
				.collect();
			left.sort();
			left.dedup();
			if !left.is_empty() {
				return Err(format!("`{}` left open in `{t}`", left.join(", ")));
			}
			Ok(t)
		};
		let params = c
			.params
			.iter()
			.map(|p| one(&p.ty_canonical))
			.collect::<Result<Vec<_>, _>>()?;
		let ret = match &c.ret_canonical {
			Some(r) => Some(one(r)?),
			None => None,
		};
		Ok((params, ret))
	}
}

/// Unify an impl head with a concrete instantiation: `params` are the
/// head's generic names. Repeated parameters must bind consistently; a
/// concrete argument must be equal. Projections in the head are checked
/// after the parameters they mention are bound.
pub(crate) fn unify(
	head: &str,
	ty: &str,
	params: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, String> {
	let (hb, ha) = split_head(head);
	let (tb, ta) = split_head(ty);
	if hb != tb {
		return Err(format!("head `{hb}` is not `{tb}`"));
	}
	if ha.len() != ta.len() {
		return Err(format!(
			"head `{head}` has {} arguments, `{ty}` has {}",
			ha.len(),
			ta.len()
		));
	}
	let mut subst: BTreeMap<String, String> = BTreeMap::new();
	let mut deferred: Vec<(String, String)> = Vec::new();
	for (h, t) in ha.iter().zip(&ta) {
		if params.contains(h) {
			match subst.get(h) {
				Some(prev) if prev != t => {
					return Err(format!("`{h}` would be both `{prev}` and `{t}`"));
				}
				Some(_) => {}
				None => {
					subst.insert(h.clone(), t.clone());
				}
			}
		} else if h.contains("::")
			&& (h.starts_with('<') || h.split("::").next().is_some_and(is_param))
		{
			deferred.push((h.clone(), t.clone()));
		} else if h.contains('<') {
			let inner = unify(h, t, params)?;
			for (k, v) in inner {
				match subst.get(&k) {
					Some(prev) if *prev != v => {
						return Err(format!("`{k}` would be both `{prev}` and `{v}`"));
					}
					_ => {
						subst.insert(k, v);
					}
				}
			}
		} else if h != t {
			return Err(format!("head argument `{h}` is not `{t}`"));
		}
	}
	for (h, t) in deferred {
		// recorded as a check to run with the world's assoc facts
		subst.insert(format!("?{h}"), t);
	}
	Ok(subst)
}

/// `unify`, then the deferred projection checks against recorded
/// associated types.
pub(crate) fn unify_with(
	world: &World,
	head: &str,
	ty: &str,
	params: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, String> {
	let mut subst = unify(head, ty, params)?;
	let deferred: Vec<(String, String)> = subst
		.iter()
		.filter(|(k, _)| k.starts_with('?'))
		.map(|(k, v)| (k[1..].to_string(), v.clone()))
		.collect();
	subst.retain(|k, _| !k.starts_with('?'));
	for (proj, want) in deferred {
		let p = subst_params(&proj, &subst);
		match world.resolve_projections(&p) {
			Ok(v) if v == want => {}
			Ok(v) => return Err(format!("head projection `{proj}` is `{v}`, not `{want}`")),
			Err(e) => return Err(format!("unresolved: {e}")),
		}
	}
	Ok(subst)
}

/// The family an alias instantiation belongs to, for the shipping order.
pub(crate) fn family(world: &World, identity: &str) -> &'static str {
	let (base, args) = split_head(identity);
	if base.ends_with("::Logical") {
		return "logical";
	}
	let arg = args.first().map(|s| s.as_str()).unwrap_or("");
	let numeric = world
		.types
		.get("polars_core::datatypes::PolarsNumericType")
		.is_some_and(|t| t.implementors.iter().any(|i| i == arg));
	if numeric {
		return "numeric";
	}
	match last(arg) {
		"BooleanType" => "boolean",
		"StringType" | "BinaryType" | "BinaryOffsetType" => "string-binary",
		"ListType" | "StructType" | "ArrayType" => "list-struct",
		_ => "other",
	}
}

/// Record 0076 gate 3 controls, from a synthetic inventory: a specialized
/// head matches only its alias; repeated parameters reject; `Self: Trait`
/// proves only for recorded implementors; a bound on a parameter the head
/// leaves open is unresolved; an associated-type equality proves and
/// rejects by the recorded binding; an unresolved projection is an
/// exception, never a binding.
pub(crate) fn applicability_self_test() {
	fn sup(path: &str, kind: &str, target: Option<&str>) -> Supporting {
		Supporting {
			trait_params: vec![],
			key: path.to_string(),
			kind: kind.to_string(),
			canonical_path: path.to_string(),
			found_paths: vec![format!(
				"polars::{}",
				path.split("::").skip(1).collect::<Vec<_>>().join("::")
			)],
			crate_paths: vec![path.to_string()],
			public_fields: 0,
			fields_canonical: vec![],
			variant_shapes: vec![],
			variant_payloads: vec![],
			generic: false,
			lifetime: false,
			hidden: false,
			derived: vec![],
			alias_target: target.map(|t| t.to_string()),
			implementors: vec![],
			impls: vec![],
		}
	}
	fn method(
		name: &str,
		head: &str,
		bounds: &[(&str, &str)],
		wh: &[&str],
		params: &[&str],
		ret: &str,
	) -> Callable {
		Callable {
			key: format!("k:{name}"),
			kind: "inherent".into(),
			krate: "polars_core".into(),
			owner: "polars_core::chunked_array::ChunkedArray".into(),
			name: name.into(),
			canonical_path: format!("polars_core::chunked_array::ChunkedArray::{name}"),
			found_paths: vec![],
			crate_paths: vec![],
			receiver: "&self".into(),
			params: params
				.iter()
				.enumerate()
				.map(|(i, t)| Param {
					name: format!("a{i}"),
					ty: t.to_string(),
					ty_canonical: t.to_string(),
				})
				.collect(),
			ret: Some(ret.into()),
			ret_canonical: Some(ret.into()),
			generics_canonical: vec![],
			impl_for: None,
			impl_bounds: bounds
				.iter()
				.map(|(a, b)| (a.to_string(), b.to_string()))
				.collect(),
			impl_head: Some(head.into()),
			impl_where: wh.iter().map(|w| w.to_string()).collect(),
			impl_assoc: vec![],
			docs_first: None,
			owner_generic: true,
			is_unsafe: false,
			is_async: false,
			deprecated: false,
			hidden: false,
			implementors: vec![],
			trait_reachable: false,
			derived: false,
			bucket: "generic".into(),
			rules: vec![],
			trait_lifetimes: vec![],
		}
	}
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mut base = sup(ca, "struct", None);
	base.generic = true;
	let mut numeric = sup("polars_core::datatypes::PolarsNumericType", "trait", None);
	numeric.implementors = vec!["polars_core::datatypes::Int64Type".into()];
	numeric.impls = vec![model::TraitImpl {
		trait_args: vec![],
		for_type: "polars_core::datatypes::Int64Type".into(),
		blanket: false,
		bounds: vec![],
		where_predicates: vec![],
		assoc_types: vec![("Native".into(), "i64".into())],
	}];
	let mut data = sup("polars_core::datatypes::PolarsDataType", "trait", None);
	data.implementors = vec![
		"polars_core::datatypes::Int64Type".into(),
		"polars_core::datatypes::BooleanType".into(),
	];
	data.impls = vec![
		model::TraitImpl {
			trait_args: vec![],
			for_type: "polars_core::datatypes::Int64Type".into(),
			blanket: false,
			bounds: vec![],
			where_predicates: vec![],
			assoc_types: vec![("Physical".into(), "i64".into())],
		},
		model::TraitImpl {
			trait_args: vec![],
			for_type: "polars_core::datatypes::BooleanType".into(),
			blanket: false,
			bounds: vec![],
			where_predicates: vec![],
			assoc_types: vec![("Physical".into(), "bool".into())],
		},
	];
	let mut logical = sup(
		"polars_core::chunked_array::logical::LogicalType",
		"trait",
		None,
	);
	logical.impls = vec![model::TraitImpl {
		trait_args: vec![],
		for_type: format!("{ca}<polars_core::datatypes::Int64Type>"),
		blanket: false,
		bounds: vec![],
		where_predicates: vec![],
		assoc_types: vec![],
	}];
	// record 0117 (stage C): generic traits whose impls differ in trait
	// arguments. `Take`'s first recorded impl has the wrong argument and the
	// second (recorded twice) is right; `Dup` has two indistinguishable impls.
	let imp = |args: &[&str], head: &str, bounds: &[(&str, &str)]| model::TraitImpl {
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
	let num_bound = [("T", "polars_core::datatypes::PolarsNumericType")];
	let mut take = sup("polars_core::chunked_array::ops::Take", "trait", None);
	take.trait_params = vec!["Idx".into()];
	take.impls = vec![
		imp(
			&["alloc::string::String"],
			&format!("{ca}<polars_core::datatypes::Int64Type>"),
			&[],
		),
		imp(&["i64"], &format!("{ca}<T>"), &num_bound),
		imp(&["i64"], &format!("{ca}<T>"), &num_bound),
	];
	let mut dup = sup("polars_core::chunked_array::ops::Dup", "trait", None);
	dup.trait_params = vec!["A".into()];
	dup.impls = vec![
		imp(
			&["i64"],
			&format!("{ca}<polars_core::datatypes::Int64Type>"),
			&[],
		),
		imp(&["A"], &format!("{ca}<T>"), &[("T", ""), ("A", "")]),
	];
	// record 0117 (stage D): bare-parameter impl heads
	let mut blank = sup("polars_core::chunked_array::ops::Blank", "trait", None);
	blank.impls = vec![imp(&[], "T", &num_bound)];
	// record 0118: an I/O trait whose one recorded impl over `Cursor<T>` needs
	// `T: AsRef<[u8]> + Send + Sync`, decidable only from the std facts
	let mut mmap = sup("polars_io::mmap::MmapBytesReader", "trait", None);
	mmap.impls = vec![imp(
		&[],
		"core::io::cursor::Cursor<T>",
		&[(
			"T",
			"core::convert::AsRef<[u8]> + core::marker::Send + core::marker::Sync",
		)],
	)];
	let mut unbounded = sup("polars_core::chunked_array::ops::Unbounded", "trait", None);
	unbounded.impls = vec![imp(&[], "T", &[("T", "?core::marker::Sized")])];
	let inv = Inventory {
		callables: vec![],
		provenance: None,
		supporting: vec![
			take,
			dup,
			blank,
			unbounded,
			mmap,
			base,
			numeric,
			data,
			logical,
			sup("polars_core::datatypes::Int64Type", "struct", None),
			sup("polars_core::datatypes::BooleanType", "struct", None),
			sup("polars_core::datatypes::StringType", "struct", None),
			sup(
				"polars_core::datatypes::Int64Chunked",
				"type_alias",
				Some(&format!("{ca}<polars_core::datatypes::Int64Type>")),
			),
			sup(
				"polars_core::datatypes::BooleanChunked",
				"type_alias",
				Some(&format!("{ca}<polars_core::datatypes::BooleanType>")),
			),
			sup(
				"polars_core::datatypes::StringChunked",
				"type_alias",
				Some(&format!("{ca}<polars_core::datatypes::StringType>")),
			),
		],
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
	let w = World::new(&inv, &release, &["mechanical"]);
	let i64c = format!("{ca}<polars_core::datatypes::Int64Type>");
	let boolc = format!("{ca}<polars_core::datatypes::BooleanType>");
	let strc = format!("{ca}<polars_core::datatypes::StringType>");
	let ok = |r: &Applicability| matches!(r, Applicability::Proven);
	// a specialized head matches only its alias
	let m = method("all", &boolc, &[], &[], &[], "bool");
	assert!(ok(&w.applicability(&m, &boolc).0));
	assert!(
		matches!(w.applicability(&m, &i64c).0, Applicability::Rejected(_)),
		"a specialized head must reject another alias"
	);
	// a generic head with a local-trait bound proves by recorded implementors and rejects otherwise
	let m = method(
		"sum",
		&format!("{ca}<T>"),
		&[("T", "polars_core::datatypes::PolarsNumericType")],
		&[],
		&[],
		"T::Native",
	);
	assert!(ok(&w.applicability(&m, &i64c).0));
	assert!(
		matches!(w.applicability(&m, &boolc).0, Applicability::Rejected(_)),
		"a bound the alias does not satisfy must reject"
	);
	// the substituted signature resolves the projection; an unrecorded one is an exception
	let (_, subst) = w.applicability(&m, &i64c);
	assert_eq!(
		w.substitute_signature(&m, &i64c, &subst)
			.unwrap()
			.1
			.as_deref(),
		Some("i64")
	);
	let m2 = method(
		"weird",
		&format!("{ca}<T>"),
		&[("T", "polars_core::datatypes::PolarsNumericType")],
		&[],
		&[],
		"T::Unknown",
	);
	let (_, subst) = w.applicability(&m2, &i64c);
	assert!(
		w.substitute_signature(&m2, &i64c, &subst).is_err(),
		"an unresolved projection is an exception"
	);
	// repeated type parameters must bind consistently
	let m = method("pair", &format!("{ca}<T>"), &[("T", "")], &[], &[], "bool");
	let mut params = BTreeSet::new();
	params.insert("T".to_string());
	assert!(
		unify(
			"polars_core::chunked_array::logical::Logical<T, T>",
			"polars_core::chunked_array::logical::Logical<A, B>",
			&params
		)
		.is_err(),
		"repeated parameters with different arguments must reject"
	);
	assert!(
		unify(
			"polars_core::chunked_array::logical::Logical<T, T>",
			"polars_core::chunked_array::logical::Logical<A, A>",
			&params
		)
		.is_ok()
	);
	let _ = m;
	// Self: LocalTrait proves only for recorded implementors
	let m = method(
		"logical",
		&format!("{ca}<T>"),
		&[("T", "polars_core::datatypes::PolarsDataType")],
		&["Self: polars_core::chunked_array::logical::LogicalType"],
		&[],
		"bool",
	);
	assert!(
		ok(&w.applicability(&m, &i64c).0),
		"Self: LogicalType holds for the recorded impl"
	);
	assert!(
		matches!(w.applicability(&m, &boolc).0, Applicability::Rejected(_)),
		"Self: LogicalType rejects an alias without an impl"
	);
	// a bound involving a parameter the head does not bind is unresolved
	let m = method(
		"open",
		&format!("{ca}<T>"),
		&[
			("T", "polars_core::datatypes::PolarsDataType"),
			("U", "polars_core::datatypes::PolarsDataType"),
		],
		&[],
		&[],
		"bool",
	);
	assert!(
		matches!(w.applicability(&m, &i64c).0, Applicability::Unresolved(_)),
		"a bound on an unbound parameter is unresolved"
	);
	// an associated-type equality proves and rejects by the recorded binding
	let m = method(
		"phys",
		&format!("{ca}<T>"),
		&[(
			"T",
			"polars_core::datatypes::PolarsDataType<Physical = i64>",
		)],
		&[],
		&[],
		"bool",
	);
	assert!(ok(&w.applicability(&m, &i64c).0));
	assert!(
		matches!(w.applicability(&m, &boolc).0, Applicability::Rejected(_)),
		"a differing associated type must reject"
	);
	// a trait with no recorded impl is unresolved, an alias outside every impl of a trait that has some is rejected
	let m = method(
		"num",
		&format!("{ca}<T>"),
		&[("T", "polars_core::datatypes::PolarsNumericType")],
		&[],
		&[],
		"bool",
	);
	assert!(matches!(
		w.applicability(&m, &strc).0,
		Applicability::Rejected(_)
	));
	let m = method(
		"foreign",
		&format!("{ca}<T>"),
		&[("T", "num_traits::float::Float")],
		&[],
		&[],
		"bool",
	);
	assert!(
		matches!(w.applicability(&m, &i64c).0, Applicability::Unresolved(_)),
		"a trait outside the inventory is unresolved"
	);
	// core traits by type: floats have no Eq/Ord/Hash; a trait argument is never erased
	assert!(
		matches!(
			w.holds("f64", "core::cmp::Eq", 0),
			Applicability::Rejected(_)
		),
		"f64: Eq must be rejected"
	);
	assert!(matches!(
		w.holds("f64", "core::cmp::Ord", 0),
		Applicability::Rejected(_)
	));
	assert!(matches!(
		w.holds("f64", "core::cmp::PartialEq", 0),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds("i64", "core::cmp::Eq", 0),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds("alloc::string::String", "core::marker::Copy", 0),
		Applicability::Rejected(_)
	));
	assert!(
		matches!(
			w.holds(
				"polars_core::datatypes::Int64Type",
				"polars_core::datatypes::PolarsNumericType<unmodeled::Argument>",
				0
			),
			Applicability::Unresolved(_)
		),
		"a generic trait argument must not be erased"
	);
	assert!(matches!(
		w.holds("i64", "core::ops::function::Fn(i64) -> i64", 0),
		Applicability::Unresolved(_)
	));
	assert!(matches!(
		w.holds(
			"polars_core::datatypes::Int64Type",
			"polars_core::datatypes::PolarsNumericType",
			0
		),
		Applicability::Proven
	));
	// unsized str: neither Clone, Default nor Sized; sizedness by type
	assert!(
		matches!(
			w.holds("str", "core::clone::Clone", 0),
			Applicability::Rejected(_)
		),
		"str: Clone must be rejected"
	);
	assert!(
		matches!(
			w.holds("str", "core::default::Default", 0),
			Applicability::Rejected(_)
		),
		"str: Default must be rejected"
	);
	assert!(
		matches!(
			w.holds("str", "core::marker::Sized", 0),
			Applicability::Rejected(_)
		),
		"str: Sized must be rejected"
	);
	assert!(matches!(
		w.holds("str", "core::fmt::Debug", 0),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds("&str", "core::marker::Sized", 0),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds("&str", "core::clone::Clone", 0),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds("&mut str", "core::clone::Clone", 0),
		Applicability::Rejected(_)
	));
	assert!(matches!(
		w.holds("[u8]", "core::marker::Sized", 0),
		Applicability::Rejected(_)
	));
	assert!(matches!(
		w.holds(
			"dyn polars_core::series::series_trait::SeriesTrait",
			"core::marker::Sized",
			0
		),
		Applicability::Rejected(_)
	));
	assert!(matches!(
		w.holds("alloc::string::String", "core::marker::Sized", 0),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds(
			"polars_core::datatypes::Int64Type",
			"core::marker::Sized",
			0
		),
		Applicability::Proven
	));
	assert!(matches!(
		w.holds(&i64c, "core::marker::Sized", 0),
		Applicability::Proven
	));
	assert!(
		matches!(
			w.holds("unknown::Type", "core::marker::Sized", 0),
			Applicability::Unresolved(_)
		),
		"an unknown type's sizedness is unresolved"
	);
	// record 0117: a qualified projection resolves inside generic arguments,
	// and a bare trait name is the recorded trait of that name
	assert_eq!(
		w.resolve_projections(
			"core::option::Option<<polars_core::datatypes::Int64Type as polars_core::datatypes::PolarsNumericType>::Native>"
		)
		.as_deref(),
		Ok("core::option::Option<i64>")
	);
	assert_eq!(
		w.resolve_projections("<polars_core::datatypes::Int64Type as PolarsNumericType>::Native")
			.as_deref(),
		Ok("i64")
	);
	assert!(
		w.resolve_projections(
			"core::option::Option<<polars_core::datatypes::BooleanType as PolarsNumericType>::Native>"
		)
		.is_err(),
		"a nested projection with no recorded binding is an error, never left in place"
	);
	// record 0117: an associated-type constraint is compared resolved
	// (`Native = <Int64Type as PolarsNumericType>::Native` is `Native = i64`)
	assert!(ok(&w.holds(
		"polars_core::datatypes::Int64Type",
		"polars_core::datatypes::PolarsNumericType<Native = <polars_core::datatypes::Int64Type as polars_core::datatypes::PolarsNumericType>::Native>",
		0
	)));
	assert!(matches!(
		w.holds(
			"polars_core::datatypes::Int64Type",
			"polars_core::datatypes::PolarsNumericType<Native = T::Native>",
			0
		),
		Applicability::Unresolved(_)
	));
	// record 0117 (stage C): every candidate is enumerated and exactly one
	// must be proven; the record order never decides
	let take = "polars_core::chunked_array::ops::Take";
	assert!(
		ok(&w.holds(&i64c, &format!("{take}<i64>"), 0)),
		"the second candidate is the right one; the first must not decide"
	);
	assert!(
		matches!(
			w.holds(&i64c, &format!("{take}<alloc::string::String>"), 0),
			Applicability::Proven
		),
		"the first candidate proves its own argument"
	);
	assert!(
		matches!(
			w.holds(&i64c, &format!("{take}<bool>"), 0),
			Applicability::Rejected(_)
		),
		"a trait argument no impl records is rejected"
	);
	assert!(
		matches!(
			w.holds(&boolc, &format!("{take}<i64>"), 0),
			Applicability::Rejected(_)
		),
		"a candidate whose bound fails is not proven"
	);
	assert!(
		ok(&w.holds(
			&i64c,
			&format!(
				"{take}<<polars_core::datatypes::Int64Type as polars_core::datatypes::PolarsNumericType>::Native>"
			),
			0
		)),
		"a projection argument that resolves is matched as its resolution"
	);
	assert!(
		matches!(
			w.holds(
				&i64c,
				&format!(
					"{take}<<polars_core::datatypes::BooleanType as polars_core::datatypes::PolarsNumericType>::Native>"
				),
				0
			),
			Applicability::Unresolved(_)
		),
		"a projection argument that does not resolve is unresolved"
	);
	assert!(
		matches!(
			w.holds(&i64c, &format!("{take}<T::Native>"), 0),
			Applicability::Unresolved(_)
		),
		"an argument naming an unbound parameter is unresolved"
	);
	match w.holds(&i64c, "polars_core::chunked_array::ops::Dup<i64>", 0) {
		Applicability::Unresolved(e) => assert!(e.contains("ambiguous: 2 impls"), "{e}"),
		other => panic!("two proven candidates must be ambiguous, got {other:?}"),
	}
	assert!(
		matches!(w.holds(&i64c, take, 0), Applicability::Unresolved(_)),
		"a generic trait bound with no arguments is unresolved"
	);
	// record 0117 (stage D): `impl<T: B> Trait for T` holds exactly where `B`
	// does; an unbounded one claims nothing
	let blank = "polars_core::chunked_array::ops::Blank";
	assert!(
		ok(&w.holds("polars_core::datatypes::Int64Type", blank, 0)),
		"a bounded blanket impl holds where its bound does"
	);
	assert!(
		matches!(
			w.holds("polars_core::datatypes::BooleanType", blank, 0),
			Applicability::Rejected(_)
		),
		"a bounded blanket impl whose bound fails does not hold"
	);
	match w.holds(
		"polars_core::datatypes::Int64Type",
		"polars_core::chunked_array::ops::Unbounded",
		0,
	) {
		Applicability::Unresolved(e) => assert!(e.contains("unbounded blanket impl"), "{e}"),
		other => panic!("an unbounded blanket impl must stay open, got {other:?}"),
	}
	// and as a method's impl head
	let int64 = "polars_core::datatypes::Int64Type";
	let m = method(
		"blank",
		"T",
		&[("T", "polars_core::datatypes::PolarsNumericType")],
		&[],
		&[],
		"bool",
	);
	assert!(ok(&w.applicability(&m, int64).0));
	assert!(matches!(
		w.applicability(&m, "polars_core::datatypes::BooleanType").0,
		Applicability::Rejected(_)
	));
	for bounds in [vec![("T", "")], vec![("T", "?core::marker::Sized")]] {
		let m = method("blank", "T", &bounds, &[], &[], "bool");
		match w.applicability(&m, int64).0 {
			Applicability::Unresolved(e) => assert!(e.contains("unbounded blanket impl"), "{e}"),
			other => panic!("{bounds:?}: an unbounded head must be refused, got {other:?}"),
		}
	}
	// record 0118: the std facts decide exactly their own keys
	let cursor = crate::families::std_facts::CURSOR;
	let mmap = "polars_io::mmap::MmapBytesReader";
	assert!(
		matches!(w.holds(cursor, mmap, 0), Applicability::Unresolved(_)),
		"without the facts, the cursor's reader bound stays open"
	);
	let mut wf = World::new(&inv, &release, &["mechanical"]);
	let vec_u8 = crate::families::std_facts::VEC_U8;
	for tr in [
		"core::convert::AsRef<[u8]>",
		"core::marker::Send",
		"core::marker::Sync",
	] {
		wf.std_facts.insert((vec_u8.to_string(), tr.to_string()));
	}
	assert!(
		ok(&wf.holds(cursor, mmap, 0)),
		"the one recorded impl, discharged by the facts"
	);
	assert!(
		!ok(&wf.holds("alloc::vec::Vec<u16>", "core::marker::Send", 0)),
		"a fact is its exact key, never a neighbour"
	);
	wf.std_facts
		.remove(&(vec_u8.to_string(), "core::marker::Sync".to_string()));
	assert!(
		!ok(&wf.holds(cursor, mmap, 0)),
		"one missing fact leaves the bound unproven"
	);
	println!("applicability self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn applicability() {
		super::applicability_self_test();
	}
}
