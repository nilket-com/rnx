//! Record 0115: the pipeline's routes that are not admission families of
//! their own: the 0076 instantiation route, the 0077 iterator-return route,
//! and the out-of-scope entry for internal crates. Each is a `claim`, in
//! `families::CLAIM`'s order (the inline chain's at 32d1b40).
use crate::emit::Entry;
use crate::emit::callable::emit_callable;
use crate::emit::instantiations::emit_instantiations;
use crate::emit::signature_of;
use crate::families::{Claims, Family};
use crate::model::Callable;
use crate::ty;
use crate::world::mapping::iterator_return;

/// Record 0076: a generic-bucket inherent method with census pairs is
/// instantiated per proven pair.
pub(crate) struct InstantiationRoute;
pub(crate) static INSTANTIATION_ROUTE: InstantiationRoute = InstantiationRoute;

impl Family for InstantiationRoute {
	fn name(&self) -> &'static str {
		"instantiation_route"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		let api = cx.release.is_api(&c.krate);
		if api && c.kind == "inherent" && c.bucket == "generic" {
			if let Some(pairs) = cx.census_by_method.get(&c.key) {
				emit_instantiations(cx.world, cx.out, c, pairs);
				return true;
			}
		}
		false
	}
}

/// Record 0077: a callable in the generic bucket only because its return is
/// an iterator (rule T3) is handled by the mapping rules, which materialize
/// it or refuse it with the item named.
pub(crate) struct IteratorReturnRoute;
pub(crate) static ITERATOR_RETURN_ROUTE: IteratorReturnRoute = IteratorReturnRoute;

impl Family for IteratorReturnRoute {
	fn name(&self) -> &'static str {
		"iterator_return_route"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		let api = cx.release.is_api(&c.krate);
		if api
			&& c.bucket == "generic"
			&& !c.owner_generic
			&& c.generics_canonical.is_empty()
			&& c.ret_canonical
				.as_deref()
				.is_some_and(|r| iterator_return(&ty::parse(r)).is_some())
		{
			let mut with_generic: Vec<&str> = cx.buckets.clone();
			with_generic.push("generic");
			emit_callable(cx.world, cx.out, c, &with_generic);
			return true;
		}
		false
	}
}

/// A callable of an internal crate reachable through the prelude is
/// accounted for as out of scope.
pub(crate) struct OutOfScope;
pub(crate) static OUT_OF_SCOPE: OutOfScope = OutOfScope;

impl Family for OutOfScope {
	fn name(&self) -> &'static str {
		"out_of_scope"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		// record 0119: a deferred type's own callables stay out of scope, as in
		// 0118 (a trait's methods are decided by their receivers instead)
		let deferred_owner =
			cx.release.deferred_type(&c.owner) && !matches!(c.kind.as_str(), "trait_method");
		if cx.release.is_api(&c.krate) && !deferred_owner {
			return false;
		}
		cx.out.entries.push(Entry {
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
		true
	}
}
