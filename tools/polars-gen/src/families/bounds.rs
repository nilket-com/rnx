use crate::census::PairRecord;
use crate::emit::Emitted;
use crate::emit::callable::{emit_callable, emit_method};
use crate::families::free_instantiations::FreeInstantiation;
use crate::families::{ArgSite, Check, Family, Listed, OracleSite, RetSite, State};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::oracle::Oracle;
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::text::{mentions, replace_token};
use crate::ty;
use crate::ty::Ty;
use crate::world::World;
use crate::world::mapping::ok_arg;
use crate::world::mapping::{Arg, Ret, Unsupported};
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Clone)]
pub(crate) struct ExternalBound {
	pub(crate) key: String,
	pub(crate) path: String,
	/// the method's exact canonical return
	pub(crate) ret: String,
	/// the method's complete, exact `impl_where` list
	#[serde(rename = "where")]
	pub(crate) where_: Vec<String>,
	/// the clauses of `where` this entry discharges
	pub(crate) discharge: Vec<String>,
	/// `[owner type, native]` pairs the discharge holds for
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0098: the float owner types and their natives; an external float
/// bound is discharged only on these.
pub(crate) const FLOAT_NATIVES: &[(&str, &str)] = &[
	("polars_core::datatypes::Float32Type", "f32"),
	("polars_core::datatypes::Float64Type", "f64"),
];
impl ExternalBound {
	/// Fail closed on anything but the cited shape: the `ChunkedArray<T>`
	/// head with exactly `T: PolarsFloatType`, `&self`, no parameters or
	/// method generics, the listed return, the listed where-clauses exactly,
	/// discharges drawn from them (never the owner bound), and float pairs
	/// from the fixed table without duplicates.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if self.cite.trim().is_empty() {
			return Err("no citation".into());
		}
		if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>") {
			return Err(format!(
				"impl head {} is not ChunkedArray<T>",
				c.impl_head.as_deref().unwrap_or("none")
			));
		}
		if c.impl_bounds
			!= [(
				"T".to_string(),
				"polars_core::datatypes::PolarsFloatType".to_string(),
			)] {
			return Err(format!(
				"impl bounds {:?} are not exactly T: PolarsFloatType",
				c.impl_bounds
			));
		}
		if c.receiver != "&self" || !c.params.is_empty() || !c.generics_canonical.is_empty() {
			return Err("not a `&self` method without parameters or method generics".into());
		}
		if c.ret_canonical.as_deref() != Some(self.ret.as_str()) {
			return Err(format!(
				"return {} is not the listed {}",
				c.ret_canonical.as_deref().unwrap_or("()"),
				self.ret
			));
		}
		if c.impl_where != self.where_ {
			return Err(format!(
				"where-clauses {:?} are not the listed {:?}",
				c.impl_where, self.where_
			));
		}
		if self.discharge.is_empty()
			|| self
				.discharge
				.iter()
				.any(|d| !self.where_.contains(d) || !d.starts_with("T::Native: "))
		{
			return Err(format!(
				"discharged clauses {:?} are not listed `T::Native` where-clauses",
				self.discharge
			));
		}
		if self.pairs.is_empty() {
			return Err("no listed pair".into());
		}
		for (i, (t, n)) in self.pairs.iter().enumerate() {
			if !FLOAT_NATIVES.iter().any(|(ft, fnat)| ft == t && fnat == n) {
				return Err(format!(
					"`{t}` with `{n}` is not a float type and its native"
				));
			}
			if self.pairs[..i].iter().any(|(u, _)| u == t) {
				return Err(format!("`{t}` is listed twice"));
			}
		}
		Ok(())
	}
}
/// Record 0098: the release's external-bound entry for a callable: `None`
/// if unlisted, `Err` naming the fault if malformed or listed twice.
pub(crate) fn external_bound_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a ExternalBound, String>> {
	let listed: Vec<&ExternalBound> = release
		.families
		.external_bounds
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct SizedSelfMethod {
	pub(crate) key: String,
	pub(crate) path: String,
	/// the exact canonical parameter types, in order
	pub(crate) params: Vec<String>,
	pub(crate) cite: String,
}
impl SizedSelfMethod {
	/// Fail closed on anything but the cited shape: a `ChunkedArray`
	/// method taking `&self`, whose only function-level generic is exactly
	/// `Self: Sized`, with the listed parameter types and a `Self` return.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if self.cite.trim().is_empty() {
			return Err("no citation".into());
		}
		if c.owner != "polars_core::chunked_array::ChunkedArray" {
			return Err(format!("owner {} is not ChunkedArray", c.owner));
		}
		if c.receiver != "&self" {
			return Err(format!("receiver {} is not &self", c.receiver));
		}
		if c.generics_canonical != [("Self".to_string(), "core::marker::Sized".to_string())] {
			return Err(format!(
				"method generics {:?} are not exactly Self: Sized",
				c.generics_canonical
			));
		}
		if c.ret_canonical.as_deref() != Some("Self") {
			return Err(format!(
				"return {} is not Self",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		let got: Vec<&str> = c.params.iter().map(|p| p.ty_canonical.as_str()).collect();
		if got != self.params.iter().map(String::as_str).collect::<Vec<_>>() {
			return Err(format!(
				"parameters {got:?} are not the listed {:?}",
				self.params
			));
		}
		Ok(())
	}
}
/// Record 0097: the release's sized-self entry for a callable, validated:
/// `None` if unlisted, `Err` naming the fault if listed but malformed
/// (including listed twice).
pub(crate) fn sized_self_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a SizedSelfMethod, String>> {
	let listed: Vec<&SizedSelfMethod> = release
		.families
		.sized_self_methods
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct NullAwareReturn {
	pub(crate) key: String,
	pub(crate) path: String,
	/// the owner types (`polars_core::datatypes::Int8Type`, ...) it applies to
	pub(crate) types: Vec<String>,
	pub(crate) cite: String,
}
impl NullAwareReturn {
	/// Fail closed before any pair is lifted: a citation, at least one
	/// type, every type numeric with a known native, no duplicates.
	pub(crate) fn check(&self) -> Result<(), String> {
		if self.cite.trim().is_empty() {
			return Err("no citation".into());
		}
		if self.types.is_empty() {
			return Err("no listed type".into());
		}
		for (i, t) in self.types.iter().enumerate() {
			if !NUMERIC_NATIVES.iter().any(|(ty, _)| ty == t) {
				return Err(format!("`{t}` is not a numeric type with a known native"));
			}
			if self.types[..i].contains(t) {
				return Err(format!("`{t}` is listed twice"));
			}
		}
		Ok(())
	}
	/// The native of a pair's `ChunkedArray<T>` identity, if `T` is listed.
	pub(crate) fn native_for(&self, identity: &str) -> Option<&'static str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		if !self.types.iter().any(|x| x == t) {
			return None;
		}
		NUMERIC_NATIVES
			.iter()
			.find(|(ty, _)| *ty == t)
			.map(|(_, n)| *n)
	}
}
/// Record 0096: the one return a listed null-aware pair may have, for its
/// native, compared on the whole substituted type (a nested or different
/// return is refused before any binding text).
pub(crate) fn null_aware_return_matches(ret: Option<&str>, native: &str) -> bool {
	let want = format!(
		"either::Either<alloc::vec::Vec<{native}>, alloc::vec::Vec<core::option::Option<{native}>>>"
	);
	ret.is_some_and(|r| ty::parse(r).render() == ty::parse(&want).render())
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct HashToken {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `return` or `parameter`
	pub(crate) direction: String,
	/// the parameter's name, for `parameter` only
	#[serde(default)]
	pub(crate) param: String,
	/// `u64` or, for a return, `Option<u64>`
	pub(crate) source: String,
	pub(crate) cite: String,
}

/// Record 0094: the hash-token scope of one callable, validated before any
/// binding text: `(return is a token, the token parameter)`. Every listed
/// entry must match the callable's exact types, or the callable is refused.
pub(crate) fn hash_token_scope(
	release: &Release,
	c: &Callable,
) -> Result<Option<(bool, Option<String>)>, String> {
	let listed: Vec<&HashToken> = release
		.families
		.hash_tokens
		.iter()
		.filter(|h| h.key == c.key && h.path == c.canonical_path)
		.collect();
	if listed.is_empty() {
		return Ok(None);
	}
	let (mut ret, mut param) = (false, None);
	for h in listed {
		if h.cite.trim().is_empty() {
			return Err(format!("hash token entry for {} has no citation", h.path));
		}
		match h.direction.as_str() {
			"return" => {
				if !h.param.is_empty() {
					return Err(format!(
						"a return hash token names a parameter ({})",
						h.param
					));
				}
				let want = match h.source.as_str() {
					"u64" => "u64",
					"Option<u64>" => "core::option::Option<u64>",
					other => {
						return Err(format!(
							"hash token return source {other} is not u64 or Option<u64>"
						));
					}
				};
				if c.ret_canonical.as_deref() != Some(want) {
					return Err(format!(
						"hash token return {} does not match the callable's {}",
						h.source,
						c.ret_canonical.as_deref().unwrap_or("()")
					));
				}
				if ret {
					return Err("duplicate return hash token".into());
				}
				ret = true;
			}
			"parameter" => {
				if h.source != "u64" {
					return Err(format!(
						"hash token parameter source {} is not u64",
						h.source
					));
				}
				match c.params.iter().find(|p| p.name == h.param) {
					Some(p) if p.ty_canonical == "u64" => {}
					Some(p) => {
						return Err(format!(
							"hash token parameter {} is {}, not u64",
							h.param, p.ty_canonical
						));
					}
					None => {
						return Err(format!(
							"hash token parameter {} is not a parameter",
							h.param
						));
					}
				}
				if param.is_some() {
					return Err("duplicate parameter hash token".into());
				}
				param = Some(h.param.clone());
			}
			other => {
				return Err(format!(
					"hash token direction {other} is not return or parameter"
				));
			}
		}
	}
	Ok(Some((ret, param)))
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct MethodScalarGeneric {
	pub(crate) key: String,
	pub(crate) path: String,
	/// The method's function generic (`N`), bound to the pair's native.
	pub(crate) generic: String,
	/// Owner type parameters admitted and their natives, in order.
	pub(crate) types: Vec<String>,
	pub(crate) natives: Vec<String>,
	pub(crate) cite: String,
}
/// Record 0092 review: `T::Native` of each Polars physical numeric type
/// (polars-core 0.55.2 `datatypes/mod.rs`, `impl_polars_num_datatype!`).
/// A release entry's native must be exactly this, or it is refused: a
/// wrong native still compiles where the scalar is an independent generic,
/// and Polars's `NumCast::from(..).expect(..)` would panic at run time.
pub(crate) const NUMERIC_NATIVES: &[(&str, &str)] = &[
	("polars_core::datatypes::Int8Type", "i8"),
	("polars_core::datatypes::Int16Type", "i16"),
	("polars_core::datatypes::Int32Type", "i32"),
	("polars_core::datatypes::Int64Type", "i64"),
	("polars_core::datatypes::UInt8Type", "u8"),
	("polars_core::datatypes::UInt16Type", "u16"),
	("polars_core::datatypes::UInt32Type", "u32"),
	("polars_core::datatypes::UInt64Type", "u64"),
	("polars_core::datatypes::Float32Type", "f32"),
	("polars_core::datatypes::Float64Type", "f64"),
];
pub(crate) fn check_natives(
	path: &str,
	types: &[String],
	natives: &[String],
) -> Result<(), String> {
	if types.len() != natives.len() {
		return Err(format!("`{path}`: types and natives must pair up"));
	}
	for (t, n) in types.iter().zip(natives) {
		match NUMERIC_NATIVES.iter().find(|(ty, _)| ty == t) {
			None => return Err(format!("`{path}`: `{t}` is not a known numeric type")),
			Some((_, want)) if want != n => {
				return Err(format!("`{path}`: `{t}`'s native is `{want}`, not `{n}`"));
			}
			Some(_) => {}
		}
	}
	Ok(())
}
impl MethodScalarGeneric {
	/// Fail closed: the entry must name one function generic of the callable,
	/// that generic may appear only as a whole parameter type, and the type
	/// and native lists must pair up.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if self.types.is_empty() {
			return Err(format!("`{}`: no types", self.path));
		}
		check_natives(&self.path, &self.types, &self.natives)?;
		if c.generics_canonical.len() != 1 || c.generics_canonical[0].0 != self.generic {
			return Err(format!(
				"`{}`: `{}` is not the callable's only function generic",
				self.path, self.generic
			));
		}
		if c.ret_canonical
			.as_deref()
			.is_some_and(|r| mentions(r, &self.generic))
		{
			return Err(format!(
				"`{}`: `{}` appears in the return",
				self.path, self.generic
			));
		}
		if !c
			.params
			.iter()
			.any(|p| p.ty_canonical.trim() == self.generic)
		{
			return Err(format!(
				"`{}`: no parameter is exactly `{}`",
				self.path, self.generic
			));
		}
		if c.params.iter().any(|p| {
			p.ty_canonical.trim() != self.generic && mentions(&p.ty_canonical, &self.generic)
		}) {
			return Err(format!(
				"`{}`: `{}` appears inside another parameter type",
				self.path, self.generic
			));
		}
		Ok(())
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		self.types
			.iter()
			.position(|t| identity == format!("polars_core::chunked_array::ChunkedArray<{t}>"))
			.map(|i| self.natives[i].as_str())
	}
}

// ---------------------------------------------------------------- record 0115: this module's families

/// Record 0094: a listed categorical hash, as an exact hex token.
pub(crate) struct HashTokenFamily;
pub(crate) static HASH_TOKEN: HashTokenFamily = HashTokenFamily;

impl Family for HashTokenFamily {
	fn name(&self) -> &'static str {
		"hash_token"
	}
	fn listed(&self, world: &World, c: &Callable) -> Listed {
		// record 0094: a listed hash, as a token return or parameter
		match hash_token_scope(&world.release, c) {
			Ok(Some((ret, param))) => Listed::Active(State::Hash(c.name.clone(), ret, param)),
			Ok(None) => Listed::Unlisted,
			Err(reason) => Listed::Refused("release policy", reason),
		}
	}
	fn ret(
		&self,
		_world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		_owner: Option<&str>,
		_depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Scalar {
			return Ok(None);
		}
		// record 0094: a listed categorical hash is an exact hex token
		let Ty::Path { path, .. } = t else {
			return Ok(None);
		};
		if path == "u64" && state.hash().is_some_and(|h| h.1) {
			return Ok(Some(Ret {
				materialize: None,
				rust_ty: "String".to_string(),
				fallible: false,
				conv: "support::hash_token(__r)".into(),
				doc: "string (a hash token: 16 lowercase hex digits)".to_string(),
			}));
		}
		Ok(None)
	}
	fn arg(
		&self,
		_world: &World,
		state: &State,
		site: ArgSite,
		t: &Ty,
		name: &str,
		_owner: Option<&str>,
	) -> Result<Option<Arg>, Unsupported> {
		if site != ArgSite::Scalar {
			return Ok(None);
		}
		// record 0094: a listed hash parameter is a token, parsed before the call
		let Ty::Path { path, .. } = t else {
			return Ok(None);
		};
		if !(path == "u64" && state.hash().is_some_and(|h| h.2.as_deref() == Some(name))) {
			return Ok(None);
		}
		(|| -> Result<Arg, Unsupported> {
			let op = state.hash().unwrap().0;
			let mut a = ok_arg(
				"&str",
				format!("__hash_{name}"),
				"string (a hash token: 16 lowercase hex digits)",
			)?;
			a.pre.push(format!(
				"let __hash_{name} = support::hash_from_token({name}, \"{op}\")?;"
			));
			a.fallible = true;
			a.shape = "hash".into();
			Ok(a)
		})()
		.map(Some)
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		_owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.hash_tokens
			.iter()
			.any(|h| h.key == key && h.path == path && h.direction == "return")
			.then_some(State::On)
	}
	fn oracle_fmt(
		&self,
		_o: &Oracle,
		_state: &State,
		site: OracleSite,
		t: &Ty,
		_owner: Option<&str>,
		_depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Scalar {
			return None;
		}
		// record 0094: a listed hash return is compared as its token
		match t {
			Ty::Path { path, .. } if path == "u64" => {
				Some(Some("format!(\"{:016x}\", __r)".into()))
			}
			_ => None,
		}
	}
}

/// Record 0092: a listed scalar function generic, bound to the pair's native.
pub(crate) struct ScalarGenericFamily;
pub(crate) static SCALAR_GENERIC: ScalarGenericFamily = ScalarGenericFamily;

impl Family for ScalarGenericFamily {
	fn name(&self) -> &'static str {
		"scalar_generic"
	}
	fn listed_pair_pre(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		syn: &mut Callable,
		_ret: Option<&str>,
	) -> Listed {
		// record 0092: a listed scalar function generic becomes this pair's native
		let Some(m) = world
			.release
			.families
			.method_scalar_generics
			.iter()
			.find(|m| m.key == c.key && m.path == c.canonical_path)
		else {
			return Listed::Unlisted;
		};
		if let Err(why) = m.check(c) {
			return Listed::Refused("method scalar generic", why);
		}
		let Some(native) = m.native_for(&p.identity) else {
			return Listed::Refused(
				"method scalar generic",
				format!("`{}` is not a listed type", p.identity),
			);
		};
		// the family substitution has already spelled `N` as its bound, so the
		// parameters that are exactly `N` in the original signature are
		// bound by position (`check` guarantees `N` appears nowhere else)
		for (q, orig) in syn.params.iter_mut().zip(&c.params) {
			if orig.ty_canonical.trim() == m.generic {
				q.ty_canonical = native.to_string();
			}
		}
		if syn
			.params
			.iter()
			.any(|q| mentions(&q.ty_canonical, &m.generic) || q.ty_canonical.contains("NumCast"))
		{
			return Listed::Refused("method scalar generic", format!("`{}` remains", m.generic));
		}
		Listed::Active(State::On)
	}
}

/// Record 0096: a listed null-aware return, bounded before the call.
pub(crate) struct NullAwareFamily;
pub(crate) static NULL_AWARE: NullAwareFamily = NullAwareFamily;

impl Family for NullAwareFamily {
	fn name(&self) -> &'static str {
		"null_aware"
	}
	fn listed_pair_pre(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
		ret: Option<&str>,
	) -> Listed {
		// record 0096: a listed null-aware return, for this pair's native only
		let Some(n) = world
			.release
			.families
			.null_aware_returns
			.iter()
			.find(|n| n.key == c.key && n.path == c.canonical_path)
		else {
			return Listed::Unlisted;
		};
		if let Err(why) = n.check() {
			return Listed::Refused("null-aware return", why);
		}
		match n.native_for(&p.identity) {
			Some(native) if !null_aware_return_matches(ret, native) => Listed::Refused(
				"null-aware return",
				format!(
					"`{}` is not Either<Vec<{native}>, Vec<Option<{native}>>>",
					ret.unwrap_or("()")
				),
			),
			Some(native) => Listed::Active(State::Two(c.name.clone(), native.to_string())),
			None => Listed::Refused(
				"null-aware return",
				format!("`{}` is not a listed type", p.identity),
			),
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0096: the exact null-aware shape, for the listed pair's native
		if let Ty::Path { path, args } = t {
			if path == "either::Either" {
				if let Some((_, native)) = state.two() {
					if depth > 0 {
						return Err(Unsupported(
							"null-aware return",
							format!("{} is nested in the return", t.render()),
						));
					}
					let elem = Ty::Path {
						path: native.clone(),
						args: vec![],
					};
					let vec_of = |inner: Ty| Ty::Path {
						path: "alloc::vec::Vec".into(),
						args: vec![inner],
					};
					let want = [
						vec_of(elem.clone()),
						vec_of(Ty::Path {
							path: "core::option::Option".into(),
							args: vec![elem.clone()],
						}),
					];
					if args.len() != 2
						|| args[0].render() != want[0].render()
						|| args[1].render() != want[1].render()
					{
						return Err(Unsupported(
							"null-aware return",
							format!(
								"{} is not Either<Vec<{native}>, Vec<Option<{native}>>>",
								t.render()
							),
						));
					}
					let e = world.ret(&elem, owner, depth + 1)?;
					if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
						return Err(Unsupported(
							"null-aware return",
							format!("element {native} is not a scalar"),
						));
					}
					let conv = format!(
						"__r.either(|__v| __v.into_iter().map(|__r| Ok::<_, Error>(Some({c}))).collect::<Result<Vec<_>, Error>>(), |__v| __v.into_iter().map(|__r| Ok::<_, Error>(match __r {{ Some(__r) => Some({c}), None => None }})).collect::<Result<Vec<_>, Error>>())?",
						c = e.conv
					);
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Vec<Option<{}>>", e.rust_ty),
						conv,
						fallible: true,
						doc: format!(
							"vector of option of {} (both Polars branches; bounded before the call)",
							e.doc
						),
					}));
				}
			}
		}
		Ok(None)
	}
	fn check(&self, _world: &World, state: &State, c: &Callable, _owner: &str, ret: &Ret) -> Check {
		// record 0096: the whole null-aware result is bounded before Polars allocates it
		let Some((op, native)) = state.two() else {
			return Check::Pass;
		};
		if c.receiver != "&self"
			|| !ret.fallible
			|| !ret.conv.starts_with("__r.either(")
			|| !null_aware_return_matches(c.ret_canonical.as_deref(), &native)
		{
			return Check::Refuse(
				"null-aware return",
				"needs a `&self` receiver and the null-aware conversion".into(),
			);
		}
		Check::Pre(
			format!("support::null_aware_bound(this.0.len(), \"{op}\")?; "),
			false,
		)
	}
}

/// Record 0097: a listed `Self: Sized` method, guarded by the receiver length.
pub(crate) struct SizedSelfFamily;
pub(crate) static SIZED_SELF: SizedSelfFamily = SizedSelfFamily;

impl Family for SizedSelfFamily {
	fn name(&self) -> &'static str {
		"sized_self"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		syn: &mut Callable,
	) -> Listed {
		// record 0097: a listed `Self: Sized` method (re-checked on the original signature)
		match sized_self_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("sized-self method", why),
			Some(Ok(_)) => {
				// `Self` is the method's generic (bound `Sized`), and the family
				// substitution spelled it as that bound: on a concrete pair it is
				// the receiver itself, and nothing else may mention it
				if syn.params.iter().any(|q| q.ty_canonical.contains("Sized")) {
					return Listed::Refused(
						"sized-self method",
						"`Sized` remains in a parameter".into(),
					);
				}
				syn.ret_canonical = Some(p.alias.clone());
				Listed::Active(State::One(c.name.clone()))
			}
		}
	}
	fn check(&self, _world: &World, state: &State, c: &Callable, owner: &str, _ret: &Ret) -> Check {
		// record 0097: Polars's signed slice offsets need the receiver length within i64
		let Some(op) = state.one() else {
			return Check::Pass;
		};
		if c.receiver != "&self"
			|| c.ret_canonical.as_deref().map(|r| ty::parse(r).render())
				!= Some(ty::parse(owner).render())
		{
			return Check::Refuse(
				"sized-self method",
				format!(
					"needs a `&self` receiver returning the owner, got {}",
					c.ret_canonical.as_deref().unwrap_or("()")
				),
			);
		}
		Check::Pre(
			format!("support::signed_len(this.0.len(), \"{op}\")?; "),
			true,
		)
	}
}

/// Record 0092 gate 2 controls: a `[[method_scalar_generics]]` entry is
/// checked before it can lift a census pair (fail closed): it must name the
/// callable's only function generic, that generic must be a whole parameter
/// type and nowhere else, and types and natives must pair up. A valid entry
/// yields each listed type's native; an unlisted type has none.
pub(crate) fn method_scalar_generic_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |params: Vec<(&str, &str)>, generics: Vec<(&str, &str)>, ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "lhs_op".into(),
		canonical_path: format!("{ca}::lhs_op"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: generics
			.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		impl_for: None,
		impl_bounds: vec![],
		impl_head: Some(format!("{ca}<T>")),
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
	let bound = "num_traits::Num + num_traits::cast::NumCast";
	let entry = |types: Vec<&str>, natives: Vec<&str>| MethodScalarGeneric {
		key: "k".into(),
		path: format!("{ca}::lhs_op"),
		generic: "N".into(),
		types: types.into_iter().map(String::from).collect(),
		natives: natives.into_iter().map(String::from).collect(),
		cite: "t".into(),
	};
	let good = entry(
		vec![
			"polars_core::datatypes::Int8Type",
			"polars_core::datatypes::UInt32Type",
			"polars_core::datatypes::Float32Type",
		],
		vec!["i8", "u32", "f32"],
	);
	let ok = mk(vec![("lhs", "N")], vec![("N", bound)], "Self");
	assert!(good.check(&ok).is_ok());
	assert_eq!(
		good.native_for(&format!("{ca}<polars_core::datatypes::Int8Type>")),
		Some("i8")
	);
	assert_eq!(
		good.native_for(&format!("{ca}<polars_core::datatypes::UInt32Type>")),
		Some("u32"),
		"IdxCa's identity"
	);
	assert_eq!(
		good.native_for(&format!("{ca}<polars_core::datatypes::Float32Type>")),
		Some("f32")
	);
	assert_eq!(
		good.native_for(&format!("{ca}<polars_core::datatypes::Int64Type>")),
		None,
		"an unlisted type has no native and is refused"
	);
	for (label, c, e) in [
		(
			"uneven lists",
			ok.clone(),
			entry(vec!["polars_core::datatypes::Int8Type"], vec![]),
		),
		(
			"another generic name",
			mk(vec![("lhs", "M")], vec![("M", bound)], "Self"),
			good.clone(),
		),
		(
			"two function generics",
			mk(
				vec![("lhs", "N"), ("x", "K")],
				vec![("N", bound), ("K", "")],
				"Self",
			),
			good.clone(),
		),
		(
			"generic in the return",
			mk(
				vec![("lhs", "N")],
				vec![("N", bound)],
				"core::option::Option<N>",
			),
			good.clone(),
		),
		(
			"generic inside another parameter",
			mk(
				vec![("lhs", "N"), ("v", "alloc::vec::Vec<N>")],
				vec![("N", bound)],
				"Self",
			),
			good.clone(),
		),
		(
			"no whole parameter",
			mk(
				vec![("v", "alloc::vec::Vec<N>")],
				vec![("N", bound)],
				"Self",
			),
			good.clone(),
		),
		(
			"Int8Type paired with i64",
			ok.clone(),
			entry(vec!["polars_core::datatypes::Int8Type"], vec!["i64"]),
		),
		(
			"Float32Type paired with f64",
			ok.clone(),
			entry(vec!["polars_core::datatypes::Float32Type"], vec!["f64"]),
		),
		(
			"a type with no known native",
			ok.clone(),
			entry(vec!["polars_core::datatypes::BooleanType"], vec!["bool"]),
		),
		(
			"lists out of order",
			ok.clone(),
			entry(
				vec![
					"polars_core::datatypes::Int8Type",
					"polars_core::datatypes::UInt8Type",
				],
				vec!["u8", "i8"],
			),
		),
	] {
		assert!(e.check(&c).is_err(), "{label} must fail closed");
	}
	// record 0095: lhs_div and lhs_rem reuse the fixed table for all ten
	// numeric types; each scalar class resolves to its own native
	let all_types = [
		"Int8Type",
		"Int16Type",
		"Int32Type",
		"Int64Type",
		"UInt8Type",
		"UInt16Type",
		"UInt32Type",
		"UInt64Type",
		"Float32Type",
		"Float64Type",
	]
	.map(|t| format!("polars_core::datatypes::{t}"));
	let all_natives = [
		"i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
	];
	for name in ["lhs_div", "lhs_rem"] {
		let e = MethodScalarGeneric {
			key: "k".into(),
			path: format!("{ca}::{name}"),
			generic: "N".into(),
			types: all_types.to_vec(),
			natives: all_natives.iter().map(|n| n.to_string()).collect(),
			cite: "t".into(),
		};
		let mut c = ok.clone();
		c.name = name.into();
		c.canonical_path = format!("{ca}::{name}");
		assert!(e.check(&c).is_ok(), "{name}: the shipped shape passes");
		for (t, n) in [
			("Int8Type", "i8"),
			("UInt32Type", "u32"),
			("Int64Type", "i64"),
			("UInt64Type", "u64"),
			("Float32Type", "f32"),
			("Float64Type", "f64"),
		] {
			assert_eq!(
				e.native_for(&format!("{ca}<polars_core::datatypes::{t}>")),
				Some(n),
				"{name}: {t}"
			);
		}
		assert_eq!(
			e.native_for(&format!("{ca}<polars_core::datatypes::BooleanType>")),
			None,
			"{name}: a nonnumeric type has no native"
		);
		let mut swapped = e.clone();
		swapped.natives.swap(0, 3);
		assert!(
			swapped.check(&c).is_err(),
			"{name}: Int8Type with i64 fails closed"
		);
	}
	println!("method-scalar-generic self-test: ok");
}

/// Record 0090 gate 2 controls: `T::Native` and `ChunkedArray<T>` are
/// replaced only as whole tokens, per listed type (a narrow signed integer,
/// `IdxCa`'s `u32`, `f32`), through `Option`; a spelling inside another
/// path is left alone and refused as residual; a listed function whose
/// parameter keeps an unresolved associated type is an exception.
pub(crate) fn native_substitution_self_test() {
	assert_eq!(
		replace_token("core::option::Option<T::Native>", "T::Native", "i8"),
		"core::option::Option<i8>"
	);
	assert_eq!(
		replace_token(
			"polars_core::chunked_array::ChunkedArray<T>",
			"polars_core::chunked_array::ChunkedArray<T>",
			"polars_core::datatypes::Int8Chunked"
		),
		"polars_core::datatypes::Int8Chunked"
	);
	assert_eq!(
		replace_token("my::XT::Native", "T::Native", "i8"),
		"my::XT::Native",
		"a longer identifier is not the token"
	);
	assert_eq!(
		replace_token("T::NativeExt", "T::Native", "i8"),
		"T::NativeExt",
		"a longer name is not the token"
	);
	assert_eq!(
		replace_token("a::T::Native", "T::Native", "i8"),
		"a::T::Native",
		"a path segment is not the generic"
	);
	let ca = "polars_core::chunked_array::ChunkedArray";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
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
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let alias = |path: &str, target: &str| {
		let mut a = sup(path);
		a.kind = "type_alias".into();
		a.alias_target = Some(target.into());
		a
	};
	let mut generic = sup(ca);
	generic.generic = true;
	let mk = |key: &str, name: &str, params: Vec<(&str, &str)>| Callable {
		key: key.into(),
		kind: "free_fn".into(),
		krate: "polars_ops".into(),
		owner: String::new(),
		name: name.into(),
		canonical_path: format!("polars_ops::m::{name}"),
		found_paths: vec![format!("polars::m::{name}")],
		crate_paths: vec![],
		receiver: "none".into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: Some("polars_core::datatypes::BooleanChunked".into()),
		generics_canonical: vec![(
			"T".into(),
			"polars_core::datatypes::PolarsNumericType".into(),
		)],
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
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let arr = format!("&{ca}<T>");
	let inv = Inventory {
		callables: vec![
			mk(
				"peaks",
				"peak",
				vec![
					("ca", &arr),
					("start", "core::option::Option<T::Native>"),
					("end", "core::option::Option<T::Native>"),
				],
			),
			mk(
				"residual",
				"other",
				vec![("ca", &arr), ("v", "T::Physical")],
			),
		],
		supporting: vec![
			generic,
			sup("polars_core::datatypes::BooleanChunked"),
			alias(
				"polars_core::datatypes::Int8Chunked",
				&format!("{ca}<polars_core::datatypes::Int8Type>"),
			),
			alias(
				"polars_core::datatypes::aliases::IdxCa",
				&format!("{ca}<polars_core::datatypes::UInt32Type>"),
			),
			alias(
				"polars_core::datatypes::Float32Chunked",
				&format!("{ca}<polars_core::datatypes::Float32Type>"),
			),
		],
		provenance: None,
	};
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into(), "polars_ops".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let types = vec![
		"polars_core::datatypes::Int8Type".to_string(),
		"polars_core::datatypes::UInt32Type".into(),
		"polars_core::datatypes::Float32Type".into(),
	];
	let natives = vec!["i8".to_string(), "u32".into(), "f32".into()];
	for (k, n) in [("peaks", "peak"), ("residual", "other")] {
		release
			.families
			.free_instantiations
			.push(FreeInstantiation {
				key: k.into(),
				path: format!("polars_ops::m::{n}"),
				callee: format!("polars::m::{n}"),
				generic: "T".into(),
				types: types.clone(),
				natives: natives.clone(),
				guard_param: None,
				guard: None,
				cite: "t".into(),
			});
	}
	let world = World::new(&inv, &release, &["mechanical", "generic_fn"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str| {
		let mut e = empty();
		emit_callable(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical", "generic_fn"],
		);
		(e.entries[0].clone(), e.functions)
	};
	let (e, f) = emit("peaks");
	assert_eq!(
		e.status,
		"generated",
		"{:?} {:?}",
		e.reason,
		e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>()
	);
	assert_eq!(
		e.bindings.len(),
		3,
		"{:?}",
		e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>()
	);
	assert!(
		f.contains("support::narrow::<i8>")
			&& f.contains("support::narrow::<u32>")
			&& f.contains("(v as f32)")
			&& f.contains("None => None"),
		"per-type natives through Option: {f}"
	);
	assert!(
		!f.contains("T::Native") && !f.contains("<T>"),
		"no generic left in any binding: {f}"
	);
	let (e, _) = emit("residual");
	assert_eq!(
		e.status, "unsupported",
		"an unresolved associated type keeps the function refused: {:?}",
		e.reason
	);
	assert!(
		e.exceptions
			.iter()
			.all(|x| x.reason.contains("remains in a parameter")),
		"{:?}",
		e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>()
	);
	// record 0091: a guarded usize parameter is checked in `pre`, only for its listed function
	let mut g = release.clone();
	g.families.free_instantiations.clear();
	let guard_inv = Inventory {
		callables: vec![
			mk(
				"guarded",
				"bound",
				vec![("ca", &arr), ("target_len", "usize"), ("flag", "bool")],
			),
			mk(
				"plain",
				"unguarded",
				vec![("ca", &arr), ("target_len", "usize")],
			),
		],
		supporting: inv.supporting.clone(),
		provenance: None,
	};
	g.families.free_instantiations.push(FreeInstantiation {
		key: "guarded".into(),
		path: "polars_ops::m::bound".into(),
		callee: "polars::m::bound".into(),
		generic: "T".into(),
		types: vec!["polars_core::datatypes::Int8Type".into()],
		natives: vec![],
		guard_param: Some("target_len".into()),
		guard: Some("below_idx_max".into()),
		cite: "t".into(),
	});
	g.families.free_instantiations.push(FreeInstantiation {
		key: "plain".into(),
		path: "polars_ops::m::unguarded".into(),
		callee: "polars::m::unguarded".into(),
		generic: "T".into(),
		types: vec!["polars_core::datatypes::Int8Type".into()],
		natives: vec![],
		guard_param: None,
		guard: None,
		cite: "t".into(),
	});
	let gw = World::new(&guard_inv, &g, &["mechanical", "generic_fn"]);
	let gemit = |key: &str| {
		let mut e = empty();
		emit_callable(
			&gw,
			&mut e,
			guard_inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical", "generic_fn"],
		);
		(e.entries[0].clone(), e.functions)
	};
	let (e, f) = gemit("guarded");
	assert_eq!(e.status, "generated", "{:?}", e.reason);
	assert!(f.contains("let __guarded_target_len = support::below_idx_max(support::narrow::<usize>(target_len, \"target_len\")?, \"bound\", \"target_len\")?;") && f.contains("polars::m::bound(&ca.0, __guarded_target_len, flag)"), "the guard runs before the call: {f}");
	let (e, f) = gemit("plain");
	assert_eq!(e.status, "generated", "{:?}", e.reason);
	assert!(
		!f.contains("below_idx_max"),
		"no guard on an unlisted parameter: {f}"
	);
	assert!(
		!gw.active.is_set("arg_guard"),
		"the guard scope never outlives its instantiation"
	);
	// fail closed: every malformed guard refuses the whole function, nothing unguarded is emitted
	for (label, param, check) in [
		("guard without parameter", None, Some("below_idx_max")),
		("parameter without guard", Some("target_len"), None),
		(
			"parameter that does not exist",
			Some("target_length"),
			Some("below_idx_max"),
		),
		(
			"parameter that is not usize",
			Some("flag"),
			Some("below_idx_max"),
		),
		("unknown guard", Some("target_len"), Some("below_something")),
	] {
		let mut bad = g.clone();
		bad.families.free_instantiations[0].guard_param = param.map(String::from);
		bad.families.free_instantiations[0].guard = check.map(String::from);
		let bw = World::new(&guard_inv, &bad, &["mechanical", "generic_fn"]);
		let mut e = empty();
		emit_callable(
			&bw,
			&mut e,
			guard_inv
				.callables
				.iter()
				.find(|c| c.key == "guarded")
				.unwrap(),
			&["mechanical", "generic_fn"],
		);
		assert_eq!(
			e.entries[0].status, "unsupported",
			"{label}: must refuse, got {:?}",
			e.entries[0].reason
		);
		assert!(
			e.functions.is_empty(),
			"{label}: no binding text at all: {}",
			e.functions
		);
		assert!(
			e.entries[0]
				.reason
				.as_deref()
				.unwrap_or("")
				.contains("guard"),
			"{label}: {:?}",
			e.entries[0].reason
		);
	}
	println!("native-substitution self-test: ok");
}

/// Record 0098 controls: an `[[external_bounds]]` entry admits only the
/// cited shape and fails closed, naming the fault, on a blank citation,
/// another head or owner bound, a receiver, parameter or method generic, an
/// altered return, where-clauses that differ (`Canonical` removed or
/// replaced, an extra external bound), a discharge that is not a listed
/// `T::Native` clause (or is the owner bound), a nonfloat, swapped or
/// duplicated pair, and a double listing.
pub(crate) fn external_bound_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let owner_bound = "T: polars_core::datatypes::PolarsFloatType";
	let float = "T::Native: num_traits::float::Float";
	let canon =
		"T::Native: num_traits::float::Float + polars_core::chunked_array::float::Canonical";
	let mk = |ret: &str, wh: Vec<&str>| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "to_canonical".into(),
		canonical_path: format!("{ca}::to_canonical"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsFloatType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: wh.into_iter().map(String::from).collect(),
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
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry =
		|wh: Vec<&str>, discharge: Vec<&str>, p: Vec<(String, String)>, cite: &str| ExternalBound {
			key: "k".into(),
			path: format!("{ca}::to_canonical"),
			ret: "Self".into(),
			where_: wh.into_iter().map(String::from).collect(),
			discharge: discharge.into_iter().map(String::from).collect(),
			pairs: p,
			cite: cite.into(),
		};
	let floats = pairs(vec![("Float32Type", "f32"), ("Float64Type", "f64")]);
	let good_c = mk("Self", vec![owner_bound, canon]);
	let good = entry(vec![owner_bound, canon], vec![canon], floats.clone(), "t");
	assert!(good.check(&good_c).is_ok());
	let mut other_head = good_c.clone();
	other_head.impl_head = Some(format!("{ca}<U>"));
	let mut other_bound = good_c.clone();
	other_bound.impl_bounds = vec![(
		"T".into(),
		"polars_core::datatypes::PolarsNumericType".into(),
	)];
	let mut owned = good_c.clone();
	owned.receiver = "self".into();
	let mut with_param = good_c.clone();
	with_param.params = vec![Param {
		name: "x".into(),
		ty: "usize".into(),
		ty_canonical: "usize".into(),
	}];
	let mut with_generic = good_c.clone();
	with_generic.generics_canonical = vec![("F".into(), "".into())];
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(vec![owner_bound, canon], vec![canon], floats.clone(), " "),
			"no citation",
		),
		(
			"another head",
			other_head,
			good.clone(),
			"is not ChunkedArray<T>",
		),
		(
			"another owner bound",
			other_bound,
			good.clone(),
			"are not exactly T: PolarsFloatType",
		),
		(
			"an owned receiver",
			owned,
			good.clone(),
			"not a `&self` method",
		),
		(
			"a parameter",
			with_param,
			good.clone(),
			"not a `&self` method",
		),
		(
			"a method generic",
			with_generic,
			good.clone(),
			"not a `&self` method",
		),
		(
			"a fallible return",
			mk("polars_error::PolarsResult<Self>", vec![owner_bound, canon]),
			good.clone(),
			"is not the listed",
		),
		(
			"Canonical removed",
			mk("Self", vec![owner_bound, float]),
			good.clone(),
			"are not the listed",
		),
		(
			"Canonical replaced",
			mk(
				"Self",
				vec![
					owner_bound,
					"T::Native: num_traits::float::Float + polars_core::other::Trait",
				],
			),
			good.clone(),
			"are not the listed",
		),
		(
			"an extra external bound",
			mk(
				"Self",
				vec![owner_bound, canon, "T::Native: core::fmt::LowerExp"],
			),
			good.clone(),
			"are not the listed",
		),
		(
			"a discharge outside where",
			good_c.clone(),
			entry(vec![owner_bound, canon], vec![float], floats.clone(), "t"),
			"are not listed `T::Native` where-clauses",
		),
		(
			"discharging the owner bound",
			good_c.clone(),
			entry(
				vec![owner_bound, canon],
				vec![owner_bound],
				floats.clone(),
				"t",
			),
			"are not listed `T::Native` where-clauses",
		),
		(
			"no pair",
			good_c.clone(),
			entry(vec![owner_bound, canon], vec![canon], vec![], "t"),
			"no listed pair",
		),
		(
			"a nonfloat pair",
			good_c.clone(),
			entry(
				vec![owner_bound, canon],
				vec![canon],
				pairs(vec![("Int32Type", "i32")]),
				"t",
			),
			"is not a float type and its native",
		),
		(
			"swapped natives",
			good_c.clone(),
			entry(
				vec![owner_bound, canon],
				vec![canon],
				pairs(vec![("Float32Type", "f64"), ("Float64Type", "f32")]),
				"t",
			),
			"is not a float type and its native",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(
				vec![owner_bound, canon],
				vec![canon],
				pairs(vec![("Float32Type", "f32"), ("Float32Type", "f32")]),
				"t",
			),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
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
	assert!(
		external_bound_entry(&release, &good_c).is_none(),
		"an unlisted method keeps the ordinary proof"
	);
	release.families.external_bounds = vec![good.clone()];
	assert!(matches!(
		external_bound_entry(&release, &good_c),
		Some(Ok(_))
	));
	let mut other_key = good_c.clone();
	other_key.key = "other".into();
	assert!(
		external_bound_entry(&release, &other_key).is_none(),
		"the same path under another key is not listed"
	);
	release.families.external_bounds.push(good);
	assert!(
		matches!(external_bound_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice")
	);
	println!("external-bound self-test: ok");
}

/// Record 0097 controls: a `[[sized_self_methods]]` entry admits only the
/// cited shape (ChunkedArray owner, `&self`, exactly `Self: Sized`, the
/// listed parameter types, a `Self` return) and fails closed, naming the
/// fault, on a blank citation, another owner or receiver, extra or other
/// generics, a fallible or borrowed return, altered parameters, or a double
/// listing; an unlisted `Self: Sized` method has no entry. With the scope
/// set, the binding checks the receiver length before the call and refuses
/// a return that is not the receiver, with no binding text.
pub(crate) fn sized_self_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |key: &str,
	          owner: &str,
	          receiver: &str,
	          params: Vec<&str>,
	          generics: Vec<(&str, &str)>,
	          ret: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: "head".into(),
		canonical_path: format!("{ca}::head"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params: params
			.iter()
			.enumerate()
			.map(|(i, t)| Param {
				name: format!("p{i}"),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: generics
			.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		impl_for: None,
		impl_bounds: vec![],
		impl_head: Some(format!("{ca}<T>")),
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
	let sized = vec![("Self", "core::marker::Sized")];
	let opt = "core::option::Option<usize>";
	let entry = |params: Vec<&str>, cite: &str| SizedSelfMethod {
		key: "k".into(),
		path: format!("{ca}::head"),
		params: params.into_iter().map(String::from).collect(),
		cite: cite.into(),
	};
	let good = mk("k", ca, "&self", vec![opt], sized.clone(), "Self");
	assert!(entry(vec![opt], "t").check(&good).is_ok());
	for (label, c, e, why) in [
		(
			"blank citation",
			good.clone(),
			entry(vec![opt], " "),
			"no citation",
		),
		(
			"another owner",
			mk(
				"k",
				"polars_core::chunked_array::logical::Logical",
				"&self",
				vec![opt],
				sized.clone(),
				"Self",
			),
			entry(vec![opt], "t"),
			"is not ChunkedArray",
		),
		(
			"owned receiver",
			mk("k", ca, "self", vec![opt], sized.clone(), "Self"),
			entry(vec![opt], "t"),
			"is not &self",
		),
		(
			"an extra generic",
			mk(
				"k",
				ca,
				"&self",
				vec![opt],
				vec![
					("Self", "core::marker::Sized"),
					("F", "core::ops::function::Fn()"),
				],
				"Self",
			),
			entry(vec![opt], "t"),
			"are not exactly Self: Sized",
		),
		(
			"another bound",
			mk(
				"k",
				ca,
				"&self",
				vec![opt],
				vec![("Self", "core::clone::Clone")],
				"Self",
			),
			entry(vec![opt], "t"),
			"are not exactly Self: Sized",
		),
		(
			"no generic",
			mk("k", ca, "&self", vec![opt], vec![], "Self"),
			entry(vec![opt], "t"),
			"are not exactly Self: Sized",
		),
		(
			"a fallible return",
			mk(
				"k",
				ca,
				"&self",
				vec![opt],
				sized.clone(),
				"polars_error::PolarsResult<Self>",
			),
			entry(vec![opt], "t"),
			"is not Self",
		),
		(
			"a borrowed return",
			mk("k", ca, "&self", vec![opt], sized.clone(), "&Self"),
			entry(vec![opt], "t"),
			"is not Self",
		),
		(
			"an altered parameter",
			mk("k", ca, "&self", vec!["usize"], sized.clone(), "Self"),
			entry(vec![opt], "t"),
			"are not the listed",
		),
		(
			"an extra parameter",
			mk("k", ca, "&self", vec![opt, "bool"], sized.clone(), "Self"),
			entry(vec![opt], "t"),
			"are not the listed",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
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
	assert!(
		sized_self_entry(&release, &good).is_none(),
		"an unlisted Self: Sized method has no entry and stays with the generic census"
	);
	release.families.sized_self_methods = vec![entry(vec![opt], "t")];
	assert!(matches!(sized_self_entry(&release, &good), Some(Ok(_))));
	let mut other_key = good.clone();
	other_key.key = "other".into();
	assert!(
		sized_self_entry(&release, &other_key).is_none(),
		"the same path under another key is not listed"
	);
	release
		.families
		.sized_self_methods
		.push(entry(vec![opt], "t"));
	assert!(matches!(sized_self_entry(&release, &good), Some(Err(ref m)) if m == "listed twice"));
	// the emitter: guard before the call, and only on a receiver-typed return
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
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
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk(key, owner, "&self", vec![opt], vec![], ret);
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::head");
		c.impl_head = None;
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let inv = Inventory {
		callables: vec![
			conc("ok", owner),
			conc("other_ret", "polars_core::datatypes::Int16Chunked"),
		],
		supporting: vec![sup(owner), sup("polars_core::datatypes::Int16Chunked")],
		provenance: None,
	};
	let release = Release { ..release };
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str| {
		let mut e = empty();
		world
			.active
			.set("sized_self", (Some("head".into())).map(State::One));
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("sized_self", None);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	let (status, reason, f) = emit("ok");
	assert_eq!(status, "generated", "{reason}");
	let guard = f
		.find("support::signed_len(this.0.len(), \"head\")?;")
		.unwrap_or_else(|| panic!("guarded: {f}"));
	assert!(
		guard < f.find(">::head(").unwrap(),
		"the guard runs before Polars: {f}"
	);
	assert!(
		f.contains("-> Result<W_polars_core__datatypes__Int8Chunked, Error>"),
		"the guard makes the binding fallible: {f}"
	);
	let (status, reason, f) = emit("other_ret");
	assert_eq!(
		status, "unsupported",
		"a return that is not the receiver is refused: {reason}"
	);
	assert!(f.is_empty(), "no binding text: {f}");
	assert!(!world.active.is_set("sized_self"));
	println!("sized-self self-test: ok");
}

/// Record 0096 controls: a `[[null_aware_returns]]` entry fails closed
/// (citation, numeric types, no duplicates) and resolves only listed types;
/// with the scope set, only the exact `Either<Vec<N>, Vec<Option<N>>>` for
/// the pair's native binds, bounded before the call and converting both
/// branches through the scalar rule (`u64` checked); a mismatched branch or
/// native, a non-`&self` receiver, an `Either` nested in the return, a
/// fallible return without the `Either`, and any `Either` outside the scope
/// stay unsupported with no binding text; the pair-level gate rejects every
/// return other than the exact top-level shape; the scope is clear afterwards.
pub(crate) fn null_aware_self_test() {
	let int = |t: &str| format!("polars_core::datatypes::{t}");
	let entry = |types: Vec<String>, cite: &str| NullAwareReturn {
		key: "k".into(),
		path: "p".into(),
		types,
		cite: cite.into(),
	};
	let good = entry(
		vec![int("Int8Type"), int("UInt64Type"), int("Float32Type")],
		"t",
	);
	assert!(good.check().is_ok());
	let ca = |t: &str| format!("polars_core::chunked_array::ChunkedArray<{}>", int(t));
	assert_eq!(good.native_for(&ca("Int8Type")), Some("i8"));
	assert_eq!(good.native_for(&ca("UInt64Type")), Some("u64"));
	assert_eq!(good.native_for(&ca("Float32Type")), Some("f32"));
	assert_eq!(
		good.native_for(&ca("Int64Type")),
		None,
		"an unlisted type is refused by name"
	);
	assert_eq!(good.native_for(&ca("BooleanType")), None);
	for (label, bad) in [
		("no citation", entry(vec![int("Int8Type")], " ")),
		("no type", entry(vec![], "t")),
		("a nonnumeric type", entry(vec![int("BooleanType")], "t")),
		(
			"a duplicate type",
			entry(vec![int("Int8Type"), int("Int8Type")], "t"),
		),
	] {
		assert!(bad.check().is_err(), "{label} must fail closed");
	}
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
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
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let either =
		|l: &str, r: &str| format!("either::Either<alloc::vec::Vec<{l}>, alloc::vec::Vec<{r}>>");
	let mk = |key: &str, receiver: &str, ret: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: "to_vec_null_aware".into(),
		canonical_path: format!("{owner}::to_vec_null_aware"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
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
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "mechanical".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let cases = [
		("i8", "&self", either("i8", "core::option::Option<i8>")),
		("u64", "&self", either("u64", "core::option::Option<u64>")),
		("f32", "&self", either("f32", "core::option::Option<f32>")),
		(
			"wrong_native",
			"&self",
			either("i16", "core::option::Option<i16>"),
		),
		("wrong_right", "&self", either("i8", "i8")),
		("mixed", "&self", either("i8", "core::option::Option<i16>")),
		(
			"owned_receiver",
			"self",
			either("i8", "core::option::Option<i8>"),
		),
		(
			"nested",
			"&self",
			format!(
				"core::option::Option<{}>",
				either("i8", "core::option::Option<i8>")
			),
		),
		(
			"fallible_plain",
			"&self",
			"polars_error::PolarsResult<i64>".to_string(),
		),
		(
			"unscoped",
			"&self",
			either("i8", "core::option::Option<i8>"),
		),
	];
	let inv = Inventory {
		callables: cases.iter().map(|(k, r, t)| mk(k, r, t)).collect(),
		supporting: vec![sup(owner)],
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
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, native: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"null_aware",
			(native.map(|n| ("to_vec_null_aware".to_string(), n.to_string())))
				.map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("null_aware", None);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, native, elem) in [
		("i8", "i8", "(__r as i64)"),
		(
			"u64",
			"u64",
			"support::widen::<u64>(__r, \"to_vec_null_aware\")?",
		),
		("f32", "f32", "(__r as f64)"),
	] {
		let (status, reason, f) = emit(key, Some(native));
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains("-> Result<Vec<Option<"),
			"{key}: one fallible vector of options: {f}"
		);
		let bound = f
			.find("support::null_aware_bound(this.0.len(), \"to_vec_null_aware\")?;")
			.unwrap_or_else(|| panic!("{key}: bounded: {f}"));
		let call = f.find(">::to_vec_null_aware(").unwrap();
		assert!(
			bound < call,
			"{key}: the bound is checked before Polars allocates: {f}"
		);
		assert!(
			f.contains("__r.either(")
				&& f.contains(&format!("Some({elem})"))
				&& f.contains("None => None"),
			"{key}: both branches, each element through the scalar rule: {f}"
		);
	}
	for (key, native) in [
		("wrong_native", Some("i8")),
		("wrong_right", Some("i8")),
		("mixed", Some("i8")),
		("owned_receiver", Some("i8")),
		("nested", Some("i8")),
		("fallible_plain", Some("i64")),
		("unscoped", None),
	] {
		let (status, reason, f) = emit(key, native);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	// the pair-level gate compares the whole substituted return
	assert!(null_aware_return_matches(
		Some(&either("i8", "core::option::Option<i8>")),
		"i8"
	));
	for (label, ret) in [
		(
			"nested in Option",
			Some(format!(
				"core::option::Option<{}>",
				either("i8", "core::option::Option<i8>")
			)),
		),
		(
			"nested in Result",
			Some(format!(
				"polars_error::PolarsResult<{}>",
				either("i8", "core::option::Option<i8>")
			)),
		),
		(
			"fallible without Either",
			Some("polars_error::PolarsResult<i64>".to_string()),
		),
		(
			"another native",
			Some(either("i16", "core::option::Option<i16>")),
		),
		("unit", None),
	] {
		assert!(
			!null_aware_return_matches(ret.as_deref(), "i8"),
			"{label} must not match"
		);
	}
	assert!(!world.active.is_set("null_aware"));
	println!("null-aware self-test: ok");
}

/// Record 0094 controls, from a synthetic inventory: the five exact
/// hash-token shapes (a `u64` return, an `Option<u64>` return, a `u64`
/// parameter parsed before the call) emit tokens; an unlisted same-named
/// method, a listed path under another key, and every malformed entry
/// (return or parameter type altered, missing or wrong parameter, missing
/// citation, unknown direction) either stay on the checked integer rule or
/// are refused with no binding text; the scope is clear afterwards.
pub(crate) fn hash_token_self_test() {
	let cats = "polars_dtype::categorical::Categories";
	let map = "polars_dtype::categorical::mapping::CategoricalMapping";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
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
		derived: vec!["Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let param = |name: &str, ty: &str| Param {
		name: name.into(),
		ty: ty.into(),
		ty_canonical: ty.into(),
	};
	let mk = |key: &str, owner: &str, name: &str, params: Vec<Param>, ret: &str| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_dtype".into(),
		owner: owner.into(),
		name: name.into(),
		canonical_path: format!("{owner}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params,
		ret: None,
		ret_canonical: Some(ret.into()),
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
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "mechanical".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let two = || vec![param("s", "&str"), param("hash", "u64")];
	let inv = Inventory {
		callables: vec![
			mk("ret", cats, "hash", vec![], "u64"),
			mk(
				"opt",
				map,
				"cat_to_hash",
				vec![param("cat", "u32")],
				"core::option::Option<u64>",
			),
			mk(
				"get",
				map,
				"get_cat_with_hash",
				two(),
				"core::option::Option<u32>",
			),
			mk(
				"ins",
				map,
				"insert_cat_with_hash",
				two(),
				"core::option::Option<u32>",
			),
			mk("other_key", cats, "hash", vec![], "u64"),
			mk("unlisted", map, "hash", vec![], "u64"),
			mk("ret_altered", map, "stored_hash", vec![], "u32"),
			mk("opt_altered", map, "maybe_hash", vec![], "u64"),
			mk(
				"param_altered",
				map,
				"get_with_small_hash",
				vec![param("s", "&str"), param("hash", "u32")],
				"core::option::Option<u32>",
			),
			mk(
				"param_missing",
				map,
				"get_without_hash",
				vec![param("s", "&str")],
				"core::option::Option<u32>",
			),
			mk("uncited", cats, "uncited_hash", vec![], "u64"),
			mk("direction", cats, "sideways_hash", vec![], "u64"),
			mk("ret_names_param", cats, "named_hash", vec![], "u64"),
		],
		supporting: vec![sup(cats), sup(map)],
		provenance: None,
	};
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_dtype".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let entry = |key: &str,
	             owner: &str,
	             name: &str,
	             direction: &str,
	             param: &str,
	             source: &str,
	             cite: &str| HashToken {
		key: key.into(),
		path: format!("{owner}::{name}"),
		direction: direction.into(),
		param: param.into(),
		source: source.into(),
		cite: cite.into(),
	};
	release.families.hash_tokens = vec![
		entry("ret", cats, "hash", "return", "", "u64", "t"),
		entry("opt", map, "cat_to_hash", "return", "", "Option<u64>", "t"),
		entry(
			"get",
			map,
			"get_cat_with_hash",
			"parameter",
			"hash",
			"u64",
			"t",
		),
		entry(
			"ins",
			map,
			"insert_cat_with_hash",
			"parameter",
			"hash",
			"u64",
			"t",
		),
		entry("ret_altered", map, "stored_hash", "return", "", "u64", "t"),
		entry(
			"opt_altered",
			map,
			"maybe_hash",
			"return",
			"",
			"Option<u64>",
			"t",
		),
		entry(
			"param_altered",
			map,
			"get_with_small_hash",
			"parameter",
			"hash",
			"u64",
			"t",
		),
		entry(
			"param_missing",
			map,
			"get_without_hash",
			"parameter",
			"hash",
			"u64",
			"t",
		),
		entry("uncited", cats, "uncited_hash", "return", "", "u64", " "),
		entry(
			"direction",
			cats,
			"sideways_hash",
			"sideways",
			"",
			"u64",
			"t",
		),
		entry(
			"ret_names_param",
			cats,
			"named_hash",
			"return",
			"hash",
			"u64",
			"t",
		),
	];
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str| {
		let mut e = empty();
		emit_callable(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			&["mechanical"],
		);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	let (status, reason, f) = emit("ret");
	assert_eq!(status, "generated", "ret: {reason}");
	assert!(
		f.contains("-> String")
			&& f.contains("support::hash_token(__r)")
			&& !f.contains("widen")
			&& !f.contains("Result<"),
		"a u64 hash return is an infallible exact token: {f}"
	);
	let (status, reason, f) = emit("opt");
	assert_eq!(status, "generated", "opt: {reason}");
	assert!(
		f.contains("Option<String>")
			&& f.contains("Some(support::hash_token(__r))")
			&& f.contains("support::narrow::<u32>(cat"),
		"an optional hash is an optional token; other fallibility stays: {f}"
	);
	for (key, op) in [
		("get", "get_cat_with_hash"),
		("ins", "insert_cat_with_hash"),
	] {
		let (status, reason, f) = emit(key);
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains("hash: &str")
				&& f.contains(&format!(
					"let __hash_hash = support::hash_from_token(hash, \"{op}\")?;"
				)),
			"{key}: the hash parameter is a token parsed with the operation's name: {f}"
		);
		let parse = f.find("support::hash_from_token(").unwrap();
		let call = f.find(&format!(">::{op}(")).unwrap();
		assert!(
			parse < call,
			"{key}: the token is parsed before the Polars call: {f}"
		);
		assert!(
			f.contains(", __hash_hash)") && !f.contains("narrow::<u64>"),
			"{key}: the parsed bits reach Polars unchanged: {f}"
		);
	}
	for key in ["other_key", "unlisted"] {
		let (status, reason, f) = emit(key);
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains("support::widen::<u64>(") && !f.contains("hash_token"),
			"{key}: an unlisted or other-key hash stays on the checked integer rule: {f}"
		);
	}
	for (key, why) in [
		("ret_altered", "does not match"),
		("opt_altered", "does not match"),
		("param_altered", "not u64"),
		("param_missing", "is not a parameter"),
		("uncited", "no citation"),
		("direction", "is not return or parameter"),
		("ret_names_param", "names a parameter"),
	] {
		let (status, reason, f) = emit(key);
		assert_eq!(
			status, "unsupported",
			"{key} must be refused, got {status}: {reason}"
		);
		assert!(reason.contains(why), "{key}: {reason}");
		assert!(
			f.is_empty(),
			"{key}: a refused entry emits no binding text: {f}"
		);
	}
	assert!(
		!world.active.is_set("hash_token"),
		"the scope never outlives its callable"
	);
	println!("hash-token self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn native_substitution() {
		super::native_substitution_self_test();
	}
	#[test]
	fn method_scalar_generic() {
		super::method_scalar_generic_self_test();
	}
	#[test]
	fn hash_token() {
		super::hash_token_self_test();
	}
	#[test]
	fn null_aware() {
		super::null_aware_self_test();
	}
	#[test]
	fn sized_self() {
		super::sized_self_self_test();
	}
	#[test]
	fn external_bound() {
		super::external_bound_self_test();
	}
}
