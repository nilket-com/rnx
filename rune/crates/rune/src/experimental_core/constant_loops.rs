//! Constant-only control targets and iterative cursors. No executor dependency.
use super::*;
use crate::ast::Span;

pub(super) struct Target {
	label: Option<String>,
	is_loop: bool,
	valued: bool,
}
pub(super) fn push_target(
	budget: &mut Budget,
	label: Option<&str>,
	is_loop: bool,
	valued: bool,
	span: Span,
) -> crate::compile::Result<usize> {
	let id = budget.targets.len();
	let label = label
		.map(String::try_from)
		.transpose()
		.map_err(|e| diagnostic(span, e))?;
	checked!(
		span,
		budget.targets.try_push(Target {
			label,
			is_loop,
			valued
		})
	);
	Ok(id)
}
fn resolve(
	budget: &Budget,
	label: Option<&str>,
	continuing: bool,
	span: Span,
) -> crate::compile::Result<usize> {
	for id in (budget.target_base..budget.targets.len()).rev() {
		let t = &budget.targets[id];
		if label.map_or(t.is_loop, |s| t.label.as_deref() == Some(s)) {
			if continuing && !t.is_loop {
				return Err(diagnostic(span, "cannot continue a block"));
			}
			return Ok(id);
		}
	}
	Err(diagnostic(
		span,
		if continuing {
			"continue outside a matching loop"
		} else {
			"break outside a matching loop or block"
		},
	))
}
#[inline(never)]
pub(super) fn transfer(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	match expr.kind {
		E::Continue(c) => Ok(Flow::Continue(resolve(budget, c.label, true, expr.span)?)),
		E::Break(b) => {
			let id = resolve(budget, b.label, false, expr.span)?;
			if b.expr.is_some() && !budget.targets[id].valued {
				return Err(diagnostic(
					expr.span,
					"Can only break with a value inside `loop` or breakable block",
				));
			}
			let value = match b.expr {
				Some(v) => constant_value!(evaluate(
					inner,
					exprs,
					exprs.get(v),
					depth + 1,
					budget,
					locals
				)),
				None => unit(budget, expr.span)?,
			};
			Ok(Flow::Break(id, value))
		}
		_ => unreachable!(),
	}
}
#[inline(never)]
pub(super) fn assign(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let (lhs, rhs, op) = match expr.kind {
		E::Assign(a) => (a.lhs, a.rhs, None),
		E::Binary(b) => (b.lhs, b.rhs, Some(b.op)),
		_ => unreachable!(),
	};
	let E::Variable(var) = exprs.get(lhs).kind else {
		refuse!(
			expr.span,
			"constant assignment to a part or non-local target"
		);
	};
	let value = constant_value!(evaluate(
		inner,
		exprs,
		exprs.get(rhs),
		depth + 1,
		budget,
		locals
	));
	let index = locals
		.iter()
		.rposition(|(v, _)| *v == var)
		.ok_or_else(|| diagnostic(expr.span, "constant local unavailable"))?;
	let value = if let Some(op) = op {
		let old = copy(&locals[index].1, depth + 1, budget, expr.span)?;
		binary_finish(old, value, op, depth, budget, expr.span)?
	} else {
		value
	};
	locals[index].1 = value;
	unit(budget, expr.span).map(Flow::Value)
}
fn condition(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	cond: &hir::Condition<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
	span: Span,
) -> crate::compile::Result<Flow<bool>> {
	match cond {
		hir::Condition::Expr(_, id) => {
			let v = constant_value!(evaluate(
				inner,
				exprs,
				exprs.get(*id),
				depth,
				budget,
				locals
			));
			if let ConstNodeKind::Inline(Inline::Bool(v)) = v.kind() {
				Ok(Flow::Value(*v))
			} else {
				Err(diagnostic(
					span,
					format_args!("Expected type `std::bool` but found `{}`", type_name(&v)),
				))
			}
		}
		hir::Condition::ExprLet(b) => {
			let v = constant_value!(evaluate(
				inner,
				exprs,
				exprs.get(b.expr),
				depth,
				budget,
				locals
			));
			let Some(bindings) = patterns::bind(
				&inner.constant_work,
				exprs,
				b.pat.pat,
				patterns::Input::Owned(v),
				depth,
				budget,
			)?
			else {
				return Ok(Flow::Value(false));
			};
			for binding in bindings {
				checked!(span, locals.try_push(binding));
			}
			Ok(Flow::Value(true))
		}
	}
}
#[inline(never)]
pub(super) fn run(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;
	match expr.kind {
		E::Loop(l) => {
			let id = push_target(budget, l.label, true, true, span)?;
			let result = (|| {
				loop {
					inner.constant_work.charge(span)?;
					let mark = locals.len();
					let attempt = (|| {
						if let Some(c) = l.condition {
							if !constant_value!(condition(
								inner,
								exprs,
								c,
								depth + 1,
								budget,
								locals,
								span
							)) {
								return Ok(Flow::Break(id, unit(budget, span)?));
							}
						}
						block_value(inner, exprs, &l.body, depth + 1, budget, locals)
					})();
					locals.truncate(mark);
					match attempt? {
						Flow::Value(_) => {}
						Flow::Continue(found) if found == id => {}
						Flow::Break(found, v) if found == id => return Ok(Flow::Value(v)),
						other => return Ok(other),
					}
				}
			})();
			budget.targets.truncate(id);
			result
		}
		E::For(f) => {
			let iterable =
				constant_value!(iterable(inner, exprs, f.iter, depth + 1, budget, locals));
			let id = push_target(budget, f.label, true, false, span)?;
			let result = for_body(inner, exprs, f, iterable, id, depth, budget, locals, span);
			budget.targets.truncate(id);
			result
		}
		_ => unreachable!(),
	}
}
// A range cursor does not materialize an unbounded sequence or enter ConstValue.
enum Iterable {
	Data(ConstValueBuf),
	Range {
		next: Option<i128>,
		end: Option<i128>,
		inclusive: bool,
		character: bool,
		unsigned: bool,
	},
}
#[inline(never)]
fn iterable(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	id: hir::ExprId,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow<Iterable>> {
	let expr = exprs.get(id);
	if let E::Group(inner_id) = expr.kind {
		inner.constant_work.charge(expr.span)?;
		check_depth(depth, budget, expr.span)?;
		return iterable(inner, exprs, inner_id, depth + 1, budget, locals);
	}
	let E::Range(r) = expr.kind else {
		return Ok(Flow::Value(Iterable::Data(constant_value!(evaluate(
			inner, exprs, expr, depth, budget, locals
		)))));
	};
	inner.constant_work.charge(expr.span)?;
	check_depth(depth, budget, expr.span)?;
	let (start, end, inclusive) = match r {
		hir::ExprRange::Range { start, end } => (*start, Some(*end), false),
		hir::ExprRange::RangeInclusive { start, end } => (*start, Some(*end), true),
		hir::ExprRange::RangeFrom { start } => (*start, None, false),
		_ => refuse!(expr.span, "constant range without a start"),
	};
	let a = constant_value!(evaluate(
		inner,
		exprs,
		exprs.get(start),
		depth + 1,
		budget,
		locals
	));
	let (a, character, unsigned) = match a.kind() {
		ConstNodeKind::Inline(Inline::Signed(n)) => (*n as i128, false, false),
		ConstNodeKind::Inline(Inline::Unsigned(n)) => (*n as i128, false, true),
		ConstNodeKind::Inline(Inline::Char(c)) => (*c as i128, true, false),
		_ => refuse!(expr.span, "constant range endpoint type"),
	};
	let end = if let Some(end) = end {
		let b = constant_value!(evaluate(
			inner,
			exprs,
			exprs.get(end),
			depth + 1,
			budget,
			locals
		));
		Some(match b.kind() {
			ConstNodeKind::Inline(Inline::Signed(n)) if !character && !unsigned => *n as i128,
			ConstNodeKind::Inline(Inline::Unsigned(n)) if unsigned => *n as i128,
			ConstNodeKind::Inline(Inline::Char(c)) if character => *c as i128,
			_ => return Err(diagnostic(expr.span, "range endpoint types differ")),
		})
	} else {
		None
	};
	Ok(Flow::Value(Iterable::Range {
		next: Some(a),
		end,
		inclusive,
		character,
		unsigned,
	}))
}
#[inline(never)]
fn for_body(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	f: &hir::ExprFor<'_>,
	iterable: Iterable,
	id: usize,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
	span: Span,
) -> crate::compile::Result<Flow> {
	// Borrow each flat subtree once; never restart a linear nth() walk per step.
	let mut fields = match &iterable {
		Iterable::Data(v) => Some(v.fields().into_iter()),
		_ => None,
	};
	if let Iterable::Data(v) = &iterable {
		match v.kind() {
			ConstNodeKind::Object { .. } => refuse!(span, "constant object iteration"),
			ConstNodeKind::Inline(Inline::Unit) => {}
			ConstNodeKind::Instance { hash, .. }
				if *hash == crate::runtime::Vec::HASH
					|| *hash == crate::runtime::OwnedTuple::HASH
					|| *hash == Option::<crate::runtime::Value>::HASH => {}
			_ => {
				return Err(diagnostic(
					span,
					format_args!("Cannot iterate over `{}`", type_name(v)),
				));
			}
		}
	}
	// Range state is independent of the owned data that the iterators borrow.
	let mut range = match &iterable {
		Iterable::Range {
			next,
			end,
			inclusive,
			character,
			unsigned,
		} => Some((*next, *end, *inclusive, *character, *unsigned)),
		_ => None,
	};
	loop {
		inner.constant_work.charge(span)?;
		let value = if let Some((next, end, inclusive, character, unsigned)) = &mut range {
			let Some(n) = *next else { break };
			if end.is_some_and(|end| n > end || (!*inclusive && n == end)) {
				break;
			}
			*next = if *character {
				let n = if n == 0xd7ff { 0xe000 } else { n + 1 };
				(n <= 0x10ffff).then_some(n)
			} else {
				(n < if *unsigned {
					u64::MAX as i128
				} else {
					i64::MAX as i128
				})
				.then_some(n + 1)
			};
			// Open cursors advance before yielding, exactly as execution runtime.
			// Inclusive MAX endpoints finish normally without a further step.
			if end.is_none() && next.is_none() {
				return Err(diagnostic(exprs.get(f.iter).span, "Numerical overflow"));
			}
			let v = if *character {
				Inline::Char(
					char::from_u32(n as u32)
						.ok_or_else(|| diagnostic(span, "invalid character range cursor"))?,
				)
			} else if *unsigned {
				Inline::Unsigned(n as u64)
			} else {
				Inline::Signed(n as i64)
			};
			finish_value(ConstValueBuf::from(v), depth + 1, budget, span)?
		} else {
			let Some(v) = fields.as_mut().and_then(Iterator::next) else {
				break;
			};
			copy(v, depth + 1, budget, span)?
		};
		let mark = locals.len();
		let result = (|| {
			let bindings = patterns::bind(
				&inner.constant_work,
				exprs,
				f.binding.pat,
				patterns::Input::Owned(value),
				depth + 1,
				budget,
			)?
			.ok_or_else(|| diagnostic(f.binding.pat.span, "Panicked: pattern did not match"))?;
			for b in bindings {
				checked!(span, locals.try_push(b));
			}
			block_value(inner, exprs, &f.body, depth + 1, budget, locals)
		})();
		locals.truncate(mark);
		match result? {
			Flow::Value(_) => {}
			Flow::Continue(found) if found == id => {}
			Flow::Break(found, v) if found == id => return Ok(Flow::Value(v)),
			other => return Ok(other),
		}
	}
	unit(budget, span).map(Flow::Value)
}
