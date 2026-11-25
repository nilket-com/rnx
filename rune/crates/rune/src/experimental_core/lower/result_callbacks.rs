//! A Result callback may use ordinary script frames only if taking its function,
//! borrowing its receiver and (for and_then) taking its return cannot affect an alias.
use super::*;
use crate::function::variant::Callback;
use hir::ExprKind as E;

// Only immediate closures of the current HIR arena are candidates. A missing
// entry means neither method can use the value as a known closure.
#[derive(Default)]
pub(crate) struct Proofs {
	closures: Vec<(Hash, bool)>,
}

impl Proofs {
	pub(crate) fn prepare(
		&mut self,
		exprs: &hir::Exprs<'_>,
		bodies: impl Iterator<Item = (Hash, hir::ExprId)>,
	) -> Result<()> {
		self.closures.clear();
		for (hash, body) in bodies {
			if let Some(fact) = closure_exits(exprs, body)? {
				self.closures.try_push((hash, fact))?;
			}
		}
		Ok(())
	}

	fn require(&self, hash: Hash, method: Callback, at: Location) -> Result<()> {
		let fact = self.closures.iter().find(|(id, _)| *id == hash);
		match fact {
			Some((_, true)) => Ok(()),
			Some((_, false)) if method == Callback::Map => Ok(()),
			_ => Err(refusal(method, at)),
		}
	}
}

fn closure_exits(exprs: &hir::Exprs<'_>, body: hir::ExprId) -> Result<Option<bool>> {
	let Ok(proved) = fresh(exprs, body, 0) else {
		return Ok(None);
	};
	if !proved {
		return Ok(Some(false));
	}
	let mut pending = Vec::new();
	pending.try_push(body)?;
	while let Some(id) = pending.pop() {
		let Some(expr) = exprs.get_checked(id) else {
			return Ok(None);
		};
		match expr.kind {
			E::Return(Some(v)) => match fresh(exprs, v, 0) {
				Ok(true) => (),
				Ok(false) => return Ok(Some(false)),
				Err(InvalidReference) => return Ok(None),
			},
			E::Return(None) | E::Try(_) => return Ok(Some(false)),
			E::Break(b) if b.label.is_some() && b.expr.is_some() => return Ok(Some(false)),
			_ => (),
		}
		super::super::hir_walk::children(expr.kind, &mut pending)?;
	}
	Ok(Some(true))
}

#[derive(Debug)]
pub(super) struct InvalidReference;

type Checked<T> = core::result::Result<T, InvalidReference>;

pub(super) fn refusal(method: Callback, at: Location) -> Error {
	let mut error = Error::refused(Refusal::Method).at(at);
	error.operation = method.refusal();
	error
}

fn constructor(call: &hir::ExprCall<'_>) -> Option<bool> {
	let hir::Call::Meta { hash } = call.call else {
		return None;
	};
	if call.args.len() != 1 {
		return None;
	}
	if hash == crate::hash!(::std::result::Result::Ok) {
		Some(true)
	} else if hash == crate::hash!(::std::result::Result::Err) {
		Some(false)
	} else {
		None
	}
}

/// Only unlabeled block tails and parentheses forward this proof. A labelled
/// break can produce a different value without evaluating the tail.
fn tail(exprs: &hir::Exprs<'_>, mut id: hir::ExprId) -> Checked<Option<hir::ExprId>> {
	for _ in 0..128 {
		match exprs.get_checked(id).ok_or(InvalidReference)?.kind {
			E::Group(inner) => id = inner,
			E::Block(b) if b.label.is_none() => match b.value {
				Some(v) => id = v,
				None => return Ok(None),
			},
			_ => return Ok(Some(id)),
		}
	}
	Ok(None)
}

fn fresh_block(exprs: &hir::Exprs<'_>, block: &hir::Block<'_>, depth: usize) -> Checked<bool> {
	if block.label.is_some() || depth >= 128 {
		return Ok(false);
	}
	if let Some(id) = block.value {
		return fresh(exprs, id, depth + 1);
	}
	// A trailing expression statement normally discards its value and returns unit.
	let Some(hir::Stmt::Expr(_, id)) = block.statements.last() else {
		return Ok(false);
	};
	Ok(diverges(
		exprs.get_checked(*id).ok_or(InvalidReference)?.kind,
	))
}

fn diverges(kind: E<'_>) -> bool {
	match kind {
		E::Return(_) => true, // closure_exits checks its operand independently
		E::Loop(l) => {
			l.condition.is_none() && l.body.statements.is_empty() && l.body.value.is_none()
		}
		E::Call(call) => {
			matches!(call.call, hir::Call::Meta{hash} if hash == crate::hash!(::std::panic))
		}
		_ => false,
	}
}

fn fresh(exprs: &hir::Exprs<'_>, id: hir::ExprId, depth: usize) -> Checked<bool> {
	if depth >= 128 {
		return Ok(false);
	}
	match exprs.get_checked(id).ok_or(InvalidReference)?.kind {
		kind if diverges(kind) => Ok(true),
		E::Call(call) => Ok(constructor(call).is_some()),
		E::Group(inner) => fresh(exprs, inner, depth + 1),
		E::Block(block) => fresh_block(exprs, block, depth + 1),
		E::If(c) => {
			let Some(fallback) = c.fallback else {
				return Ok(false);
			};
			for branch in c.branches {
				if !fresh_block(exprs, &branch.block, depth + 1)? {
					return Ok(false);
				}
			}
			fresh_block(exprs, fallback, depth + 1)
		}
		_ => Ok(false),
	}
}

impl Lower<'_, '_> {
	/// Narrow the diagnostic for a stored Result or a value returned by a
	/// script call. Other receiver families retain their own Method refusal.
	pub(super) fn result_refusal_site(&self, target: hir::ExprId) -> bool {
		let Some(expr) = self.exprs.get_checked(target) else {
			return true;
		};
		match expr.kind {
			E::Variable(v) => self
				.text_init(v)
				.is_some_and(|id| !matches!(self.result_receiver(id), Ok(None))),
			E::Call(call) => matches!(call.call, hir::Call::Meta { hash }
				if self.context.lookup_function(hash).is_none() && self.nominal_type(hash).is_none()),
			_ => false,
		}
	}
	pub(super) fn result_receiver(&self, target: hir::ExprId) -> Checked<Option<bool>> {
		let Some(id) = tail(self.exprs, target)? else {
			return Ok(None);
		};
		let E::Call(call) = self.exprs.get_checked(id).ok_or(InvalidReference)?.kind else {
			return Ok(None);
		};
		Ok(constructor(call))
	}

	#[cold]
	#[inline(never)]
	pub(super) fn result_callback(
		&mut self,
		call: &hir::ExprCall<'_>,
		target: hir::ExprId,
		ok: bool,
		method: Callback,
		out: u32,
		span: Span,
	) -> Result<()> {
		let at = self.location(span);
		let denied = || refusal(method, at);
		let &[argument] = call.args else {
			return Err(denied());
		};
		if !self.context.result_callback(method) {
			return Err(denied());
		}
		let literal = tail(self.exprs, argument)
			.map_err(|_| denied())?
			.ok_or_else(denied)?;
		let (hash, captures) = match self.exprs.get_checked(literal).ok_or_else(denied)?.kind {
			E::Fn(hash) => (hash, 0),
			E::CallClosure(c) => (c.hash, c.captures.len()),
			_ => return Err(denied()),
		};
		self.transaction
			.result_callbacks
			.require(hash, method, at)?;
		let receiver = self.expr(target)?;
		let function = self.expr(argument)?;
		let id = (self.resolve)(Resolve::Script(hash), at)?;
		let arity = u32::try_from(captures + 1).map_err(|_| denied())?;
		self.transaction.require_parameters(id, arity, at)?;
		let index = self.field_index(0, span)?;
		let payload = self.slot()?;
		self.emit(
			Op::TupleField {
				out: payload,
				value: receiver,
				index,
			},
			span,
		)?;
		// The receiver proof also establishes the discriminant. Its constructor
		// and the callback expression have both run, even when the call is skipped.
		if ok {
			let (args_start, args_len) = self.function.arguments(&[payload])?;
			let result = if method == Callback::Map {
				payload
			} else {
				out
			};
			self.emit(
				Op::CallValue {
					out: result,
					value: function,
					args_start,
					args_len,
				},
				span,
			)?;
			if method == Callback::AndThen {
				return Ok(());
			}
		}
		self.emit(
			Op::Variant {
				out,
				tag: Type::Result,
				success: ok,
				payload: Some(payload),
			},
			span,
		)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn require_refusal(proofs: &Proofs, hash: Hash) {
		let at = Location {
			source: 3,
			revision: 7,
			start: 10,
			end: 31,
		};
		for method in [Callback::Map, Callback::AndThen] {
			let err = proofs.require(hash, method, at).unwrap_err();
			assert_eq!(err.refusal, Some(Refusal::Method));
			assert_eq!(err.operation, method.refusal());
			assert_eq!(err.location, Some(at));
		}
	}

	#[test]
	fn result_callback_facts_are_local_to_the_arena() {
		let mut exprs = hir::Exprs::new();
		let body = exprs
			.insert(hir::Expr {
				span: Span::empty(),
				kind: E::Lit(hir::Lit::Bool(true)),
			})
			.unwrap();
		let hash = Hash::new(42);
		let mut proofs = Proofs::default();
		proofs
			.prepare(&exprs, [(hash, body), (Hash::new(3), body)].into_iter())
			.unwrap();
		let at = Location {
			source: 0,
			revision: 0,
			start: 0,
			end: 1,
		};
		proofs.require(hash, Callback::Map, at).unwrap();
		require_refusal(&proofs, Hash::new(77));
		proofs
			.prepare(&hir::Exprs::new(), core::iter::empty())
			.unwrap();
		require_refusal(&proofs, hash);
	}

	#[test]
	#[cfg_attr(
		debug_assertions,
		should_panic(expected = "invalid HIR expression reference")
	)]
	fn result_callback_invalid_reference_refuses_at_the_call() {
		let mut other = hir::Exprs::new();
		let invalid = other
			.insert(hir::Expr {
				span: Span::empty(),
				kind: E::Lit(hir::Lit::Bool(true)),
			})
			.unwrap();
		let hash = Hash::new(74);
		let mut proofs = Proofs::default();
		proofs
			.prepare(&hir::Exprs::new(), [(hash, invalid)].into_iter())
			.unwrap();
		require_refusal(&proofs, hash);
	}
}
