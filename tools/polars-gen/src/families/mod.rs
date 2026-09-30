//! Record 0115: the family registry. A listed family (a rule the release
//! file enables per callable or per instantiation pair) implements the
//! hooks of `Family` in its own module; the shared code consults the
//! ordered lists below at each site instead of carrying the rule inline.
//! Every list is in the order the site's inline blocks had at `32d1b40`,
//! and each site keeps its semantics (first match, or run all).
pub(crate) mod bounds;
pub(crate) mod callbacks;
pub(crate) mod concrete_arrays;
pub(crate) mod conversions;
pub(crate) mod dtype_owners;
pub(crate) mod free_instantiations;
pub(crate) mod generic_impls;
pub(crate) mod generic_inputs;
pub(crate) mod generic_traits;
pub(crate) mod into_arguments;
pub(crate) mod protocol_instantiations;
pub(crate) mod protocols;
pub(crate) mod receiver_guards;
pub(crate) mod receivers;
pub(crate) mod returns;
pub(crate) mod routes;
pub(crate) mod serde;
pub(crate) mod snapshots;
pub(crate) mod std_facts;

use crate::census::PairRecord;
use crate::emit::Emitted;
use crate::families::generic_impls::OpArm;
use crate::model::Callable;
use crate::oracle::Oracle;
use crate::release::Release;
use crate::ty::Ty;
use crate::world::World;
use crate::world::mapping::{Arg, Ret, Unsupported};
use std::collections::BTreeMap;

/// A family's state for the callable or pair being emitted. Each family
/// reads only its own state, in the shape it stored.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum State {
	/// listed, with nothing more to carry
	On,
	/// one value (the operation name, or an item type)
	One(String),
	/// the operation name and the pair's native or kind
	Two(String, String),
	/// the operation name and two values
	Three(String, String, String),
	/// a hash token: operation, token return, token parameter
	Hash(String, bool, Option<String>),
}

/// The outcome of a family's listing gate for one callable or pair.
pub(crate) enum Listed {
	Unlisted,
	Active(State),
	/// refused before emission: the family's label and the reason
	Refused(&'static str, String),
}

/// The outcome of a family's consistency check in `emit_method_with`.
pub(crate) enum Check {
	Pass,
	/// preflight code prepended to the call, and whether it makes the
	/// binding fallible on its own
	Pre(String, bool),
	/// refused: the category and the reason
	Refuse(&'static str, String),
}

/// Where in `World::ret` a family arm sits.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum RetSite {
	/// before any shared mapping (the listed-shape arms)
	Top,
	/// before `match t`: a `Cow` path (no earlier arm matches a path)
	Cow,
	/// the head of the path arm, for a scalar the listing re-maps
	Scalar,
	/// a path with no wrapper, before the unwrapped-type refusal
	Unwrapped,
}

/// Where in `World::arg` a family arm sits.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ArgSite {
	/// before any shared mapping
	Top,
	/// the head of the path arm, for a scalar the listing re-maps
	Scalar,
}

/// Where in the oracle's formatters a family arm sits.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum OracleSite {
	/// `oracle_fmt` before `match t`
	Top,
	/// `oracle_fmt` after the `&str` reference arm
	Ref,
	/// `oracle_fmt` at the head of the path arm
	Scalar,
}

pub(crate) trait Family: Sync {
	/// the key of this family's state
	fn name(&self) -> &'static str;
	/// the pipeline's dispatch (`CLAIM`): take the callable, or leave it
	fn claim<'a>(&self, _cx: &mut Claims<'a, '_>, _c: &'a Callable) -> bool {
		false
	}
	/// the callable scope (`emit_callable`, `emit_method_with`)
	fn listed(&self, _world: &World, _c: &Callable) -> Listed {
		Listed::Unlisted
	}
	/// the pair scope, before the pair's return is substituted
	fn listed_pair_pre(
		&self,
		_world: &World,
		_c: &Callable,
		_p: &PairRecord,
		_syn: &mut Callable,
		_ret: Option<&str>,
	) -> Listed {
		Listed::Unlisted
	}
	/// the pair scope, after the pair's return is substituted
	fn listed_pair_post(
		&self,
		_world: &World,
		_c: &Callable,
		_p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		Listed::Unlisted
	}
	fn ret(
		&self,
		_world: &World,
		_state: &State,
		_site: RetSite,
		_t: &Ty,
		_owner: Option<&str>,
		_depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		Ok(None)
	}
	fn arg(
		&self,
		_world: &World,
		_state: &State,
		_site: ArgSite,
		_t: &Ty,
		_name: &str,
		_owner: Option<&str>,
	) -> Result<Option<Arg>, Unsupported> {
		Ok(None)
	}
	fn check(
		&self,
		_world: &World,
		_state: &State,
		_c: &Callable,
		_owner: &str,
		_ret: &Ret,
	) -> Check {
		Check::Pass
	}
	/// the oracle's state for an entry, from the release listing
	fn oracle_state(
		&self,
		_world: &World,
		_key: &str,
		_path: &str,
		_owner: Option<&str>,
	) -> Option<State> {
		None
	}
	fn oracle_fmt(
		&self,
		_o: &Oracle,
		_state: &State,
		_site: OracleSite,
		_t: &Ty,
		_owner: Option<&str>,
		_depth: u8,
	) -> Option<Option<String>> {
		None
	}
	fn script_fmt(
		&self,
		_o: &Oracle,
		_state: &State,
		_r: &str,
		_owner: Option<&str>,
		_depth: u8,
	) -> Option<Option<String>> {
		None
	}
}

/// Record 0115: what a `claim` may use and fill: the pipeline's output and
/// the census pairs, and the deferred operator and `INDEX_GET` groups the
/// 0113 families collect for `finish`.
pub(crate) struct Claims<'a, 'w> {
	pub(crate) world: &'w World,
	pub(crate) out: &'w mut Emitted,
	pub(crate) release: &'w Release,
	pub(crate) buckets: &'w Vec<&'w str>,
	pub(crate) census_by_method: &'w BTreeMap<String, Vec<&'a PairRecord>>,
	pub(crate) op_rows: Vec<&'a Callable>,
	pub(crate) op_pending: Vec<OpArm>,
	pub(crate) index_rows: Vec<&'a Callable>,
	pub(crate) index_pending: Vec<(String, String, &'static str, String)>,
}

/// The first family in `CLAIM` that takes `c`.
pub(crate) fn claim<'a>(cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
	for f in CLAIM {
		if f.claim(cx, c) {
			trace("claim", f.name(), "claimed");
			return true;
		}
	}
	false
}

/// The families' work after every callable is claimed or emitted: the
/// 0113 operator and `INDEX_GET` groups.
pub(crate) fn finish(cx: Claims<'_, '_>) {
	generic_impls::finish(cx)
}

// Record 0115: the callable (`key`) or pair (`key|alias`) being emitted,
// written with every trace line; nested targets restore the outer one.
thread_local! {
	static TARGET: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

pub(crate) struct Target(String);

pub(crate) fn target(t: String) -> Target {
	Target(TARGET.with(|x| std::mem::replace(&mut *x.borrow_mut(), t)))
}

impl Drop for Target {
	fn drop(&mut self) {
		let prev = std::mem::take(&mut self.0);
		TARGET.with(|x| *x.borrow_mut() = prev);
	}
}

use bounds::{HASH_TOKEN, NULL_AWARE, SCALAR_GENERIC, SIZED_SELF};
use generic_impls::{FAMILY_REFUSAL, FROM_ITER, INDEX, OPERATOR, UNARY};
use routes::{INSTANTIATION_ROUTE, ITERATOR_RETURN_ROUTE, OUT_OF_SCOPE};

/// The pipeline's dispatch chain, in the inline chain's order at 32d1b40.
pub(crate) static CLAIM: &[&dyn Family] = &[
	&INSTANTIATION_ROUTE,
	&ITERATOR_RETURN_ROUTE,
	&OUT_OF_SCOPE,
	&INDEX,
	&FROM_ITER,
	&UNARY,
	&OPERATOR,
	&protocol_instantiations::PROTOCOL_INSTANTIATION,
	&FAMILY_REFUSAL,
];
use free_instantiations::ARG_GUARD;
use generic_inputs::{BITMAP_INPUT, BITMAP_RETURN};
use returns::{BOUNDED_READBACK, COW_RETURN, ITERATOR_RETURN};
use snapshots::{ARRAY, CHUNK, INDEXED, ITER, LAYOUT, OWNED_ITER, VIEW};

/// The callable scope, in the order `emit_callable` set its states.
pub(crate) static CALLABLE: &[&dyn Family] = &[
	&COW_RETURN,
	&BOUNDED_READBACK,
	&BITMAP_RETURN,
	&BITMAP_INPUT,
	&ITERATOR_RETURN,
	&HASH_TOKEN,
];
/// The pair scope before the return is substituted (0092, then 0096).
pub(crate) static PAIR_PRE: &[&dyn Family] = &[&SCALAR_GENERIC, &NULL_AWARE];
/// The pair scope after it, in `emit_instantiations`' order.
pub(crate) static PAIR_POST: &[&dyn Family] = &[
	&CHUNK,
	&INDEXED,
	&LAYOUT,
	&OWNED_ITER,
	&VIEW,
	&ITER,
	&ARRAY,
	&SIZED_SELF,
];
/// Every family whose state the pair scope sets and clears.
pub(crate) static PAIR: &[&dyn Family] = &[
	&SCALAR_GENERIC,
	&NULL_AWARE,
	&SIZED_SELF,
	&CHUNK,
	&INDEXED,
	&ARRAY,
	&ITER,
	&VIEW,
	&OWNED_ITER,
	&LAYOUT,
];
/// The free-instantiation scope: the guarded parameter.
pub(crate) static FREE: &[&dyn Family] = &[&ARG_GUARD];
pub(crate) static RET_TOP: &[&dyn Family] = &[
	&LAYOUT,
	&OWNED_ITER,
	&VIEW,
	&ITER,
	&ARRAY,
	&INDEXED,
	&CHUNK,
	&NULL_AWARE,
	&ITERATOR_RETURN,
];
pub(crate) static RET_COW: &[&dyn Family] = &[&COW_RETURN];
pub(crate) static RET_SCALAR: &[&dyn Family] = &[&HASH_TOKEN, &BOUNDED_READBACK];
pub(crate) static RET_UNWRAPPED: &[&dyn Family] = &[&BITMAP_RETURN];
pub(crate) static ARG_TOP: &[&dyn Family] = &[&BITMAP_INPUT];
pub(crate) static ARG_SCALAR: &[&dyn Family] = &[&ARG_GUARD, &HASH_TOKEN];
/// `emit_method_with`'s checks, all run, the first refusal ending emission.
pub(crate) static CHECK: &[&dyn Family] = &[
	&LAYOUT,
	&OWNED_ITER,
	&VIEW,
	&ITER,
	&ARRAY,
	&INDEXED,
	&CHUNK,
	&SIZED_SELF,
	&NULL_AWARE,
];
/// The oracle state visible to `script_fmt` (set before it).
pub(crate) static ORACLE_SCRIPT_STATE: &[&dyn Family] = &[&LAYOUT];
/// The oracle state visible to `oracle_fmt` (set after `script_fmt`).
pub(crate) static ORACLE_STATE: &[&dyn Family] = &[
	&HASH_TOKEN,
	&LAYOUT,
	&OWNED_ITER,
	&VIEW,
	&ITER,
	&ARRAY,
	&INDEXED,
	&CHUNK,
];
pub(crate) static ORACLE_TOP: &[&dyn Family] =
	&[&LAYOUT, &OWNED_ITER, &VIEW, &ITER, &ARRAY, &INDEXED];
pub(crate) static ORACLE_REF: &[&dyn Family] = &[&CHUNK];
pub(crate) static ORACLE_SCALAR: &[&dyn Family] = &[&HASH_TOKEN];
pub(crate) static SCRIPT_TUPLE: &[&dyn Family] = &[&LAYOUT];

impl State {
	pub(crate) fn one(&self) -> Option<String> {
		match self {
			State::One(a) => Some(a.clone()),
			_ => None,
		}
	}
	pub(crate) fn two(&self) -> Option<(String, String)> {
		match self {
			State::Two(a, b) => Some((a.clone(), b.clone())),
			_ => None,
		}
	}
	pub(crate) fn three(&self) -> Option<(String, String, String)> {
		match self {
			State::Three(a, b, c) => Some((a.clone(), b.clone(), c.clone())),
			_ => None,
		}
	}
	pub(crate) fn hash(&self) -> Option<(String, bool, Option<String>)> {
		match self {
			State::Hash(a, b, c) => Some((a.clone(), *b, c.clone())),
			_ => None,
		}
	}
}

/// Active family states: set for one callable or pair, cleared on drop.
#[derive(Default)]
pub(crate) struct Active(std::cell::RefCell<BTreeMap<&'static str, State>>);

impl Active {
	pub(crate) fn get(&self, name: &str) -> Option<State> {
		self.0.borrow().get(name).cloned()
	}
	pub(crate) fn is_set(&self, name: &str) -> bool {
		self.0.borrow().contains_key(name)
	}
	/// Test support: set one family's state directly.
	pub(crate) fn set(&self, name: &'static str, state: Option<State>) {
		let mut m = self.0.borrow_mut();
		match state {
			Some(s) => m.insert(name, s),
			None => m.remove(name),
		};
	}
	/// Replace the states of `scope`'s families with `states` (a family
	/// not in `states` is cleared), until the returned guard drops, which
	/// clears every family of `scope`.
	pub(crate) fn enter<'a>(
		&'a self,
		scope: &'static [&'static dyn Family],
		states: Vec<(&'static str, State)>,
	) -> Scope<'a> {
		let mut m = self.0.borrow_mut();
		for f in scope {
			m.remove(f.name());
		}
		if !states.is_empty() {
			let names: Vec<&str> = states.iter().map(|(n, _)| *n).collect();
			trace("scope", &names.join("+"), "entered");
		}
		for (name, s) in states {
			m.insert(name, s);
		}
		Scope {
			active: self,
			scope,
		}
	}
}

pub(crate) struct Scope<'a> {
	active: &'a Active,
	scope: &'static [&'static dyn Family],
}

impl Drop for Scope<'_> {
	fn drop(&mut self) {
		let mut m = self.active.0.borrow_mut();
		for f in self.scope {
			m.remove(f.name());
		}
	}
}

/// Run a listing gate over `list` in order: every applicable family's
/// state is collected until the first refusal, which ends the scan.
pub(crate) fn collect(
	list: &'static [&'static dyn Family],
	hook: &'static str,
	mut gate: impl FnMut(&dyn Family) -> Listed,
) -> Result<Vec<(&'static str, State)>, (&'static str, String)> {
	let mut states = Vec::new();
	for f in list {
		match gate(*f) {
			Listed::Unlisted => {}
			Listed::Active(s) => {
				trace(hook, f.name(), "active");
				states.push((f.name(), s));
			}
			Listed::Refused(label, why) => {
				trace(hook, f.name(), "refused");
				return Err((label, why));
			}
		}
	}
	Ok(states)
}

/// Record 0115: with `POLARS_GEN_TRACE=<file>`, every hook a family takes
/// is appended to the file as `hook<TAB>family<TAB>outcome<TAB>target`;
/// unset, nothing
/// is written.
pub(crate) fn trace(hook: &str, family: &str, outcome: &str) {
	use std::io::Write as _;
	static FILE: std::sync::OnceLock<Option<std::sync::Mutex<std::fs::File>>> =
		std::sync::OnceLock::new();
	let f = FILE.get_or_init(|| {
		std::env::var_os("POLARS_GEN_TRACE").map(|p| {
			std::sync::Mutex::new(
				std::fs::OpenOptions::new()
					.create(true)
					.append(true)
					.open(p)
					.expect("POLARS_GEN_TRACE file"),
			)
		})
	});
	if let Some(f) = f {
		let target = TARGET.with(|x| x.borrow().clone());
		writeln!(f.lock().unwrap(), "{hook}\t{family}\t{outcome}\t{target}").unwrap();
	}
}

/// The first family arm in `list` whose state is active and which answers.
pub(crate) fn first<T>(
	active: &Active,
	list: &'static [&'static dyn Family],
	hook: &'static str,
	mut arm: impl FnMut(&dyn Family, &State) -> Option<T>,
) -> Option<T> {
	for f in list {
		if let Some(s) = active.get(f.name()) {
			if let Some(r) = arm(*f, &s) {
				trace(hook, f.name(), "taken");
				return Some(r);
			}
		}
	}
	None
}

/// Record 0115: `emit_method_with`'s checks (`CHECK`): every active family
/// in order; each preflight goes before the ones already placed (so the
/// last family's runs first, as the inline blocks' `insert_str(0, …)` did);
/// the first refusal ends emission with its category and reason.
pub(crate) fn run_checks(
	world: &World,
	c: &Callable,
	owner: &str,
	ret: &Ret,
) -> Result<(String, bool), (&'static str, String)> {
	let mut preflight = String::new();
	let mut fallible = false;
	for f in CHECK {
		if let Some(s) = world.active.get(f.name()) {
			match f.check(world, &s, c, owner, ret) {
				Check::Pass => trace("check", f.name(), "pass"),
				Check::Pre(p, makes_fallible) => {
					trace("check", f.name(), "preflight");
					preflight.insert_str(0, &p);
					fallible |= makes_fallible;
				}
				Check::Refuse(category, why) => {
					trace("check", f.name(), "refused");
					return Err((category, why));
				}
			}
		}
	}
	Ok((preflight, fallible))
}

/// The oracle states of `list` for one entry, in order.
pub(crate) fn oracle_states(
	list: &'static [&'static dyn Family],
	world: &World,
	key: &str,
	path: &str,
	owner: Option<&str>,
) -> Vec<(&'static str, State)> {
	list.iter()
		.filter_map(|f| {
			f.oracle_state(world, key, path, owner).map(|s| {
				trace("oracle", f.name(), "active");
				(f.name(), s)
			})
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn names(list: &[&dyn Family]) -> Vec<&'static str> {
		list.iter().map(|f| f.name()).collect()
	}

	/// Record 0115: every hook's order is the order of its site's inline
	/// blocks at 32d1b40 (the evidence derives the same lists from that
	/// source); a reorder must fail here, not pass silently.
	#[test]
	fn hook_orders_are_pinned() {
		let snapshots_top = [
			"layout",
			"owned_iter",
			"view_snapshot",
			"iter_snapshot",
			"array_snapshot",
			"indexed_chunk",
		];
		assert_eq!(
			names(CALLABLE),
			[
				"cow_return",
				"bounded_readback",
				"bitmap_return",
				"bitmap_input",
				"iterator_return",
				"hash_token"
			]
		);
		assert_eq!(names(PAIR_PRE), ["scalar_generic", "null_aware"]);
		assert_eq!(
			names(PAIR_POST),
			[
				"chunk_snapshot",
				"indexed_chunk",
				"layout",
				"owned_iter",
				"view_snapshot",
				"iter_snapshot",
				"array_snapshot",
				"sized_self"
			]
		);
		let mut ret_top = snapshots_top.to_vec();
		ret_top.extend(["chunk_snapshot", "null_aware", "iterator_return"]);
		assert_eq!(names(RET_TOP), ret_top);
		assert_eq!(names(RET_COW), ["cow_return"]);
		assert_eq!(names(RET_SCALAR), ["hash_token", "bounded_readback"]);
		assert_eq!(names(RET_UNWRAPPED), ["bitmap_return"]);
		assert_eq!(names(ARG_TOP), ["bitmap_input"]);
		assert_eq!(names(ARG_SCALAR), ["arg_guard", "hash_token"]);
		let mut check = snapshots_top.to_vec();
		check.extend(["chunk_snapshot", "sized_self", "null_aware"]);
		assert_eq!(names(CHECK), check);
		assert_eq!(names(ORACLE_SCRIPT_STATE), ["layout"]);
		assert_eq!(
			names(ORACLE_STATE),
			[
				"hash_token",
				"layout",
				"owned_iter",
				"view_snapshot",
				"iter_snapshot",
				"array_snapshot",
				"indexed_chunk",
				"chunk_snapshot"
			]
		);
		assert_eq!(names(ORACLE_TOP), snapshots_top);
		assert_eq!(names(ORACLE_REF), ["chunk_snapshot"]);
		assert_eq!(names(ORACLE_SCALAR), ["hash_token"]);
		assert_eq!(names(SCRIPT_TUPLE), ["layout"]);
		assert_eq!(names(FREE), ["arg_guard"]);
		assert_eq!(
			names(CLAIM),
			[
				"instantiation_route",
				"iterator_return_route",
				"out_of_scope",
				"index",
				"from_iter",
				"unary",
				"operator",
				"protocol_instantiation",
				"generic_impl_refusal"
			]
		);
		// every family whose state a pair gate sets is cleared by the pair scope
		for f in PAIR_PRE.iter().chain(PAIR_POST) {
			assert!(
				names(PAIR).contains(&f.name()),
				"{} is not cleared",
				f.name()
			);
		}
	}

	/// Record 0115: the scopes keep 32d1b40's set-and-clear semantics. An
	/// inner callable scope replaces the callable families and clears them
	/// on drop (the old guard set them to `None`); the pair scope's states
	/// survive it; nothing survives the outer drops.
	#[test]
	fn scopes_replace_and_clear() {
		let a = Active::default();
		{
			let _pair = a.enter(
				PAIR,
				vec![("chunk_snapshot", State::Two("chunks".into(), "i64".into()))],
			);
			{
				let _outer = a.enter(CALLABLE, vec![("cow_return", State::On)]);
				{
					let _inner = a.enter(CALLABLE, vec![("bounded_readback", State::On)]);
					assert!(
						!a.is_set("cow_return"),
						"entering replaces the scope's families"
					);
					assert!(a.is_set("bounded_readback") && a.is_set("chunk_snapshot"));
				}
				assert!(
					!a.is_set("bounded_readback") && !a.is_set("cow_return"),
					"dropping clears them"
				);
				assert!(a.is_set("chunk_snapshot"), "another scope's state survives");
			}
		}
		assert!(!a.is_set("chunk_snapshot"));
		assert!(a.0.borrow().is_empty(), "nothing survives the scopes");
	}

	/// Record 0115: `collect` gathers every applicable state in order and
	/// stops at the first refusal, which discards what it gathered.
	#[test]
	fn collect_gathers_until_the_first_refusal() {
		struct F(&'static str, u8);
		impl Family for F {
			fn name(&self) -> &'static str {
				self.0
			}
		}
		static A: F = F("a", 0);
		static B: F = F("b", 1);
		static C: F = F("c", 2);
		static D: F = F("d", 3);
		static L: &[&dyn Family] = &[&A, &B, &C, &D];
		let outcome = |refuse_at: Option<&str>| {
			collect(L, "t", |f| match (f.name(), refuse_at) {
				(n, Some(r)) if n == r => Listed::Refused("t", format!("at {n}")),
				("b", _) => Listed::Unlisted,
				(n, _) => Listed::Active(State::One(n.to_string())),
			})
		};
		let all = outcome(None).unwrap();
		assert_eq!(
			all.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
			["a", "c", "d"]
		);
		let refused = outcome(Some("c")).err().unwrap();
		assert_eq!(refused, ("t", "at c".to_string()));
	}

	/// Record 0115: family outcomes no inventory can reach (an earlier gate
	/// refuses first: the pair gate's own shape check, the census's
	/// applicability, the free instantiation's guard shape), asserted on
	/// the hook itself with the exact category and reason `emit_method_with`
	/// or the pair loop reports.
	#[test]
	fn unreachable_refusals_are_named() {
		use crate::model::{Callable, Inventory, Param};
		use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
		let ca = "polars_core::chunked_array::ChunkedArray";
		let c =
			|receiver: &str, params: Vec<&str>, generics: Vec<(&str, &str)>, ret: &str| Callable {
				key: "k".into(),
				kind: "inherent".into(),
				krate: "polars_core".into(),
				owner: ca.into(),
				name: "m".into(),
				canonical_path: format!("{ca}::m"),
				found_paths: vec![],
				crate_paths: vec![],
				receiver: receiver.into(),
				params: params
					.into_iter()
					.enumerate()
					.map(|(i, t)| Param {
						name: format!("p{i}"),
						ty: t.into(),
						ty_canonical: t.into(),
					})
					.collect(),
				ret: None,
				ret_canonical: Some(ret.into()),
				generics_canonical: generics
					.into_iter()
					.map(|(a, b)| (a.into(), b.into()))
					.collect(),
				impl_for: None,
				impl_bounds: vec![],
				impl_head: None,
				impl_where: vec![],
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
			};
		let sized = c(
			"&self",
			vec!["usize"],
			vec![("Self", "core::marker::Sized")],
			"Self",
		);
		let mut release = Release {
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
		release
			.families
			.sized_self_methods
			.push(crate::families::bounds::SizedSelfMethod {
				key: "k".into(),
				path: format!("{ca}::m"),
				params: vec!["usize".into()],
				cite: "t".into(),
			});
		let inv = Inventory {
			callables: vec![],
			supporting: vec![],
			provenance: None,
		};
		let world = World::new(&inv, &release, &["mechanical"]);
		// the checks: a conversion that is not the family's (or the wrong
		// receiver) is refused by name
		let plain = Ret {
			rust_ty: "i64".into(),
			conv: "__r".into(),
			fallible: false,
			doc: String::new(),
			materialize: None,
		};
		let borrowed = c("&self", vec![], vec![], "Self");
		for (f, state, category) in [
			(
				&LAYOUT as &dyn Family,
				State::Three("layout".into(), "i64".into(), "Int64Type".into()),
				"layout snapshot",
			),
			(
				&VIEW,
				State::Two("downcast_chunks".into(), "i64".into()),
				"view snapshot",
			),
			(
				&ITER,
				State::Two("downcast_iter".into(), "i64".into()),
				"iterator snapshot",
			),
			(
				&ARRAY,
				State::Two("downcast_as_array".into(), "i64".into()),
				"array snapshot",
			),
			(
				&INDEXED,
				State::Two("downcast_get".into(), "i64".into()),
				"indexed chunk snapshot",
			),
			(
				&CHUNK,
				State::Two("chunks".into(), "i64".into()),
				"chunk snapshot",
			),
		] {
			match f.check(&world, &state, &borrowed, ca, &plain) {
				Check::Refuse(cat, why) => {
					assert_eq!(cat, category);
					assert!(why.starts_with("needs an unrouted"), "{why}");
				}
				_ => panic!("{} admitted a foreign conversion", f.name()),
			}
		}
		// the sized-self pair gate: `Sized` left in a parameter after the
		// family substitution
		let p = PairRecord {
			key: "k".into(),
			method: "m".into(),
			identity: format!("{ca}<polars_core::datatypes::Int64Type>"),
			alias: "polars_core::datatypes::Int64Chunked".into(),
			family: "numeric",
			result: crate::world::proof::Applicability::Proven,
			signature: None,
			eligible: true,
			disposition: None,
		};
		let mut syn = sized.clone();
		syn.params[0].ty_canonical = "core::marker::Sized".into();
		match SIZED_SELF.listed_pair_post(&world, &sized, &p, &mut syn) {
			Listed::Refused(label, why) => {
				assert_eq!(label, "sized-self method");
				assert_eq!(why, "`Sized` remains in a parameter");
			}
			_ => panic!("sized-self admitted a residual Sized"),
		}
		// the argument guard: an unknown check is refused before any code
		let usize_ty = crate::ty::parse("usize");
		match ARG_GUARD.arg(
			&world,
			&State::Three("op".into(), "n".into(), "bogus".into()),
			ArgSite::Scalar,
			&usize_ty,
			"n",
			None,
		) {
			Err(Unsupported(cat, why)) => {
				assert_eq!((cat, why.as_str()), ("unknown argument guard", "bogus"))
			}
			_ => panic!("an unknown guard was admitted"),
		}
	}

	fn world_with(active: Vec<(&'static str, State)>) -> World {
		use crate::model::Inventory;
		use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
		let release = Release {
			name: "t".into(),
			source: "t".into(),
			provenance: ReleaseProvenance::default(),
			instantiation: InstantiationScope::default(),
			api_crates: vec![],
			unordered: vec![],
			excluded_oracle: vec![],
			refused: vec![],
			families: FamilyTables::default(),
		};
		let world = World::new(
			&Inventory {
				callables: vec![],
				supporting: vec![],
				provenance: None,
			},
			&release,
			&["mechanical"],
		);
		for (n, s) in active {
			world.active.set(n, Some(s));
		}
		world
	}

	/// Record 0115 (review round 1): two `ret` families active on one return
	/// whose shape only the later one admits. The earlier answers first (the
	/// array snapshot refuses `Option<&T::Array>`), exactly as the inline
	/// arms did; alone, the later one converts it.
	#[test]
	fn ret_first_answer_wins() {
		let array = crate::families::snapshots::indexed_array("i64").unwrap();
		let t = crate::ty::parse(&format!("core::option::Option<&{array}>"));
		let state = || State::Two("downcast_get".into(), "i64".into());
		let both = world_with(vec![
			("array_snapshot", state()),
			("indexed_chunk", state()),
		]);
		match both.family_ret(RetSite::Top, &t, None, 0) {
			Err(Unsupported(cat, _)) => {
				assert_eq!(cat, "array snapshot", "the earlier family answers")
			}
			other => panic!(
				"expected the array snapshot's refusal, got {:?}",
				other.map(|r| r.map(|r| r.conv))
			),
		}
		let alone = world_with(vec![("indexed_chunk", state())]);
		let r = alone
			.family_ret(RetSite::Top, &t, None, 0)
			.unwrap()
			.unwrap();
		assert!(
			r.conv.starts_with("support::indexed_snapshot"),
			"{}",
			r.conv
		);
		// and the reverse order of activation changes nothing: order is the list's
		let rev = world_with(vec![
			("indexed_chunk", state()),
			("array_snapshot", state()),
		]);
		assert!(matches!(
			rev.family_ret(RetSite::Top, &t, None, 0),
			Err(Unsupported("array snapshot", _))
		));
	}

	/// Record 0115 (review round 1): `check` runs every active family. Two
	/// preflights are both placed, the later family's first; a later refusal
	/// after an earlier preflight ends emission with the later category.
	#[test]
	fn checks_run_all_until_a_refusal() {
		let ca = "polars_core::chunked_array::ChunkedArray";
		let mut c: crate::model::Callable = serde_json::from_value(serde_json::json!({
			"key": "k", "kind": "inherent", "krate": "polars_core", "owner": ca, "name": "m",
			"canonical_path": format!("{ca}::m"), "found_paths": [], "crate_paths": [],
			"receiver": "&self", "params": [], "ret": null, "ret_canonical": ca,
			"generics_canonical": [], "impl_for": null, "impl_bounds": [], "impl_head": null,
			"impl_where": [], "impl_assoc": [], "docs_first": null, "owner_generic": false,
			"is_unsafe": false, "is_async": false, "deprecated": false, "hidden": false,
			"implementors": [], "trait_reachable": false, "derived": false, "bucket": "mechanical", "rules": []
		}))
		.unwrap();
		let layout_ret = Ret {
			rust_ty: "x".into(),
			conv: "support::layout_snapshot(__r)".into(),
			fallible: true,
			doc: String::new(),
			materialize: None,
		};
		let layout = State::Three("layout".into(), "i64".into(), "Int64Type".into());
		// two preflights: layout's, then sized-self's placed before it
		let w = world_with(vec![
			("layout", layout.clone()),
			("sized_self", State::One("layout".into())),
		]);
		let (pre, fallible) = run_checks(&w, &c, ca, &layout_ret).unwrap();
		assert!(
			pre.starts_with("support::signed_len(")
				&& pre.contains("support::preflight_numeric::<i64>"),
			"{pre}"
		);
		assert!(pre.find("signed_len").unwrap() < pre.find("preflight_numeric").unwrap());
		assert!(
			fallible,
			"the sized-self preflight makes the binding fallible"
		);
		// an earlier preflight, then a later refusal: the refusal is the outcome
		let w = world_with(vec![
			("layout", layout),
			(
				"view_snapshot",
				State::Two("downcast_chunks".into(), "i64".into()),
			),
		]);
		assert_eq!(
			run_checks(&w, &c, ca, &layout_ret).unwrap_err().0,
			"view snapshot"
		);
		// and with nothing active, nothing is added
		c.receiver = "self".into();
		assert_eq!(
			run_checks(&world_with(vec![]), &c, ca, &layout_ret).unwrap(),
			(String::new(), false)
		);
	}
}
