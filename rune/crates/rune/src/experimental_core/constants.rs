//! Bounded constant data evaluation, independent of the legacy evaluator.

use super::{
	arithmetic,
	error::{Error, ErrorKind, Result},
	lower::binary_op,
	value::Value,
};
use crate as rune;
use crate::alloc::Box;
use crate::alloc::prelude::*;
use crate::runtime::TypeHash;
use crate::runtime::{ConstNodeKind, ConstValue, ConstValueBuf, Inline};
use crate::{
	alloc::{String, Vec},
	ast::{BinOp, UnOp},
	hir,
	query::QueryInner,
};
use hir::ExprKind as E;

// Normal return is control flow, never an error. Helpers propagate it until the
// callee boundary, including when it originates in a caller's argument.
enum Flow<T = ConstValueBuf> {
	Value(T),
	Return(ConstValueBuf),
	Break(usize, ConstValueBuf),
	Continue(usize),
}
macro_rules! constant_value {
	($e:expr) => {
		match $e? {
			Flow::Value(v) => v,
			Flow::Return(v) => return Ok(Flow::Return(v)),
			Flow::Break(id, v) => return Ok(Flow::Break(id, v)),
			Flow::Continue(id) => return Ok(Flow::Continue(id)),
		}
	};
}
macro_rules! checked {
	($span:expr,$e:expr) => {
		$e.map_err(|e| diagnostic($span, e))?
	};
}
macro_rules! refuse {
	($span:expr,$name:expr) => {
		return Err(refusal($span, $name))
	};
}
#[path = "constant_loops.rs"]
mod loops;
#[path = "constant_patterns.rs"]
mod patterns;
const MAX_WORK: usize = 2_000_000;
const MAX_COMBINED_DEPTH: usize = 896;
const MAX_CALLS: usize = 224;
/// One instance belongs to the query, not an initializer or a recursive call.
pub(crate) struct Work(core::cell::Cell<usize>);
impl Default for Work {
	fn default() -> Self {
		Self(core::cell::Cell::new(MAX_WORK))
	}
}
impl Work {
	fn charge(&self, span: crate::ast::Span) -> crate::compile::Result<()> {
		let left = self.0.get();
		if left == 0 {
			refuse!(span, "constant work bound (2000000 units) exceeded");
		}
		self.0.set(left - 1);
		#[cfg(test)]
		function_tests::work_observed();
		Ok(())
	}
}
fn check_depth(
	depth: usize,
	budget: &Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<()> {
	if depth >= MAX_COMBINED_DEPTH {
		refuse!(span, "constant combined nesting bound (896) exceeded");
	}
	if depth - budget.body_base >= MAX_DEPTH {
		refuse!(span, "constant expression nesting bound (128) exceeded");
	}
	Ok(())
}
fn unit(budget: &mut Budget, span: crate::ast::Span) -> crate::compile::Result<ConstValueBuf> {
	finish_value(ConstValueBuf::from(Inline::Unit), 0, budget, span)
}

// Keep recursive frames thin in unoptimized builds: dispatch, traversal and
// post-order construction are separate non-inlined helpers. The explicit 2 MiB
// tests below protect this source shape; inline changes need remeasurement.
pub(crate) enum Scalar {
	Primitive(Value),
	Text(String),
}

pub(crate) fn read(value: &ConstValue) -> Result<Scalar> {
	Ok(match value.kind() {
		ConstNodeKind::Inline(value) => Scalar::Primitive(match *value {
			Inline::Unit => Value::Unit,
			Inline::Bool(v) => Value::Bool(v),
			Inline::Char(v) => Value::Char(v),
			Inline::Ordering(v) => Value::Ordering(v),
			Inline::Signed(v) => Value::I64(v),
			Inline::Unsigned(v) => Value::U64(v),
			Inline::Float(v) => Value::F64(v),
			_ => return Err(refused()),
		}),
		ConstNodeKind::String(v) => Scalar::Text(String::try_from(v.as_ref())?),
		_ => return Err(refused()),
	})
}

fn refused() -> Error {
	Error::new(ErrorKind::Unsupported, "restricted scalar constant")
}

// Compile-time limits, independent of runtime store limits. References are charged
// for their expanded trees, so repeated references cannot bypass the budget.
const MAX_NODES: usize = 65_536;
const MAX_BYTES: usize = 16 << 20;
const MAX_DEPTH: usize = 128;

#[derive(Default)]
struct Budget {
	targets: Vec<loops::Target>,
	target_base: usize,
	body_base: usize,
	calls: usize,
	source: Option<crate::SourceId>,
	nodes: usize,
	bytes: usize,
}

fn diagnostic(span: crate::ast::Span, error: impl core::fmt::Display) -> crate::compile::Error {
	crate::compile::Error::msg(span, error)
}

pub(crate) fn eval(
	source: crate::SourceId,
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
) -> crate::compile::Result<ConstValueBuf> {
	evaluate(
		inner,
		exprs,
		expr,
		0,
		&mut Budget {
			source: Some(source),
			..Budget::default()
		},
		&mut Vec::new(),
	)
	.map(|flow| match flow {
		Flow::Value(value) => value,
		Flow::Return(_) | Flow::Break(..) | Flow::Continue(..) => {
			unreachable!("top-level control refused")
		}
	})
}

#[inline(never)]
fn evaluate(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	inner.constant_work.charge(span)?;
	check_depth(depth, budget, span)?;
	#[cfg(test)]
	let marker = core::hint::black_box(0u8);
	#[cfg(test)]
	function_tests::observe(&marker, depth, budget);
	if budget.nodes > MAX_NODES {
		refuse!(span, "constant expanded-node bound (65536) exceeded");
	}
	match expr.kind {
		E::Group(_) => evaluate_group(inner, exprs, expr, depth, budget, locals),
		E::Const(_) => evaluate_reference(inner, exprs, expr, depth, budget, locals),
		E::Variable(_) => evaluate_local(inner, exprs, expr, depth, budget, locals),
		E::Lit(_) => evaluate_literal(inner, exprs, expr, depth, budget, locals),
		E::Vec(_) | E::Tuple(_) => evaluate_sequence(inner, exprs, expr, depth, budget, locals),
		E::Object(_) => evaluate_object(inner, exprs, expr, depth, budget, locals),
		E::Unary(_) => evaluate_unary(inner, exprs, expr, depth, budget, locals),
		E::Binary(_) => evaluate_binary(inner, exprs, expr, depth, budget, locals),
		E::Index(_) => evaluate_index(inner, exprs, expr, depth, budget, locals),
		E::Block(_) => evaluate_block(inner, exprs, expr, depth, budget, locals),
		E::If(_) => evaluate_conditional(inner, exprs, expr, depth, budget, locals),
		E::Call(call) if matches!(call.call, hir::Call::ConstFn { .. }) => {
			evaluate_const_call(inner, exprs, expr, depth, budget, locals)
		}
		E::Call(_) => evaluate_option(inner, exprs, expr, depth, budget, locals),
		E::Assign(_) => loops::assign(inner, exprs, expr, depth, budget, locals),
		E::Loop(_) | E::For(_) => loops::run(inner, exprs, expr, depth, budget, locals),
		E::Break(_) | E::Continue(_) => loops::transfer(inner, exprs, expr, depth, budget, locals),
		E::Return(_) => evaluate_return(inner, exprs, expr, depth, budget, locals),
		E::AsyncBlock(_) | E::Await(_) | E::Select(_) => {
			Err(refusal(span, "constant async expressions"))
		}
		E::Yield(_) => Err(refusal(span, "constant yield")),
		E::CallClosure(_) | E::Fn(_) => Err(refusal(span, "constant closures and function values")),
		E::FieldAccess(_) => evaluate_field(inner, exprs, expr, depth, budget, locals),
		E::Match(_) => evaluate_match(inner, exprs, expr, depth, budget, locals),
		E::Try(_) => Err(refusal(span, "constant try")),
		E::Range(_) => Err(refusal(span, "constant ranges")),
		E::Template(_) | E::Format(_) => Err(refusal(span, "constant formatting")),
		E::Static(_) => Err(refusal(span, "constant static reads")),
		E::Type(_) => Err(refusal(span, "constant type values")),
	}
}

#[inline(never)]
fn evaluate_group(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::Group(id) => next(id),

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_reference(
	inner: &QueryInner<'_>,
	_exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	_locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	match expr.kind {
		E::Const(hash) => {
			let value = inner
				.get_const_value(hash)
				.ok_or_else(|| diagnostic(span, "unresolved constant"))?;
			copy(value, depth, budget, span).map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_local(
	_inner: &QueryInner<'_>,
	_exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	match expr.kind {
		E::Variable(variable) => {
			let value = locals
				.iter()
				.rev()
				.find(|(id, _)| *id == variable)
				.ok_or_else(|| diagnostic(span, "unresolved constant local"))?;
			copy(&value.1, depth, budget, span).map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_literal(
	_inner: &QueryInner<'_>,
	_exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	_locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let value: ConstValueBuf = match expr.kind {
		E::Lit(lit) => match lit {
			hir::Lit::Str(s) => {
				if budget.bytes.saturating_add(s.len()) > MAX_BYTES {
					refuse!(span, "constant byte bound (16 MiB) exceeded");
				}
				checked!(span, ConstValueBuf::string(s))
			}
			hir::Lit::Bool(v) => ConstValueBuf::from(Inline::Bool(v)),
			hir::Lit::Signed(v) => ConstValueBuf::from(Inline::Signed(v)),
			hir::Lit::Unsigned(v) => ConstValueBuf::from(Inline::Unsigned(v)),
			hir::Lit::Float(v) => ConstValueBuf::from(Inline::Float(v)),
			hir::Lit::Char(v) => ConstValueBuf::from(Inline::Char(v)),
			hir::Lit::ByteStr(_) => {
				refuse!(span, "byte-string constants")
			}
		},

		_ => unreachable!(),
	};
	finish_value(value, depth, budget, span).map(Flow::Value)
}

#[inline(never)]
fn evaluate_sequence(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::Vec(seq) | E::Tuple(seq) => {
			if seq.items.len() > MAX_NODES {
				refuse!(span, "constant expanded-node bound (65536) exceeded");
			}
			// Temporary child owners are bounded by seq.items.len() <= MAX_NODES;
			// child construction/copy charges remain cumulative while they are held.
			let mut children = Vec::new();
			for id in seq.items {
				checked!(span, children.try_push(constant_value!(next(*id))));
			}
			sequence_finish(
				children,
				matches!(expr.kind, E::Vec(_)),
				depth,
				budget,
				span,
			)
			.map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_object(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::Object(object) => {
			if !matches!(object.kind, hir::ExprObjectKind::Anonymous) {
				refuse!(span, "typed object constants");
			}
			if object.assignments.len() > MAX_NODES {
				refuse!(span, "constant expanded-node bound (65536) exceeded");
			}
			let mut fields = Vec::new();
			for field in object.assignments {
				let child = constant_value!(next(field.assign));
				checked!(span, fields.try_push((field.key.1, child)));
			}
			object_finish(fields, depth, budget, span).map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_unary(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::Unary(unary) => {
			let value = constant_value!(next(unary.expr));
			unary_finish(value, unary.op, depth, budget, span).map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_binary(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::Binary(binary) => {
			if binary.op.is_assign() {
				return loops::assign(inner, exprs, expr, depth, budget, locals);
			}
			let a = constant_value!(next(binary.lhs));
			if let ConstNodeKind::Inline(Inline::Bool(v)) = a.kind() {
				if matches!(binary.op, BinOp::And(..)) && !v
					|| matches!(binary.op, BinOp::Or(..)) && *v
				{
					return Ok(Flow::Value(a));
				}
			}
			let b = constant_value!(next(binary.rhs));
			binary_finish(a, b, binary.op, depth, budget, span).map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_index(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::Index(index) => {
			let target = constant_value!(next(index.target));
			let index = constant_value!(next(index.index));
			return copy(select(&target, &index, span)?, depth, budget, span).map(Flow::Value);
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_block(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	match expr.kind {
		E::Block(block) => block_value(inner, exprs, block, depth + 1, budget, locals),

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_conditional(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let E::If(conditional) = expr.kind else {
		unreachable!()
	};
	for branch in conditional.branches {
		inner.constant_work.charge(span)?;
		let mark = locals.len();
		let result = (|| {
			let taken = match branch.condition {
				hir::Condition::Expr(_, id) => {
					let value = constant_value!(evaluate(
						inner,
						exprs,
						exprs.get(*id),
						depth + 1,
						budget,
						locals
					));
					let ConstNodeKind::Inline(Inline::Bool(taken)) = value.kind() else {
						return Err(diagnostic(
							span,
							format_args!(
								"Expected type `std::bool` but found `{}`",
								type_name(&value)
							),
						));
					};
					*taken
				}
				hir::Condition::ExprLet(binding) => {
					let value = constant_value!(evaluate(
						inner,
						exprs,
						exprs.get(binding.expr),
						depth + 1,
						budget,
						locals
					));
					match patterns::bind(
						&inner.constant_work,
						exprs,
						binding.pat.pat,
						patterns::Input::Owned(value),
						depth + 1,
						budget,
					)? {
						Some(bindings) => {
							for b in bindings {
								checked!(span, locals.try_push(b));
							}
							true
						}
						None => false,
					}
				}
			};
			if !taken {
				return Ok(Flow::Value(None));
			}
			let value = constant_value!(block_value(
				inner,
				exprs,
				&branch.block,
				depth + 1,
				budget,
				locals
			));
			Ok(Flow::Value(Some(if conditional.fallback.is_some() {
				value
			} else {
				ConstValueBuf::from(Inline::Unit)
			})))
		})();
		locals.truncate(mark);
		match result? {
			Flow::Return(v) => return Ok(Flow::Return(v)),
			Flow::Break(id, v) => return Ok(Flow::Break(id, v)),
			Flow::Continue(id) => return Ok(Flow::Continue(id)),
			Flow::Value(Some(v)) => return Ok(Flow::Value(v)),
			Flow::Value(None) => {}
		}
	}
	match conditional.fallback {
		Some(block) => block_value(inner, exprs, block, depth + 1, budget, locals),
		None => unit(budget, span).map(Flow::Value),
	}
}

#[inline(never)]
fn evaluate_match(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let E::Match(m) = expr.kind else {
		unreachable!()
	};
	let value = constant_value!(evaluate(
		inner,
		exprs,
		exprs.get(m.expr),
		depth + 1,
		budget,
		locals
	));
	for branch in m.branches {
		let Some(bindings) = patterns::bind(
			&inner.constant_work,
			exprs,
			branch.pat.pat,
			patterns::Input::Borrowed(&value),
			depth + 1,
			budget,
		)?
		else {
			continue;
		};
		let mark = locals.len();
		let result = (|| {
			for b in bindings {
				checked!(branch.span, locals.try_push(b));
			}
			if let Some(id) = branch.condition {
				let guard = constant_value!(evaluate(
					inner,
					exprs,
					exprs.get(id),
					depth + 1,
					budget,
					locals
				));
				let ConstNodeKind::Inline(Inline::Bool(taken)) = guard.kind() else {
					return Err(diagnostic(
						exprs.get(id).span,
						format_args!(
							"Expected type `std::bool` but found `{}`",
							type_name(&guard)
						),
					));
				};
				if !taken {
					return Ok(Flow::Value(None));
				}
			}
			let v = constant_value!(evaluate(
				inner,
				exprs,
				exprs.get(branch.body),
				depth + 1,
				budget,
				locals
			));
			Ok(Flow::Value(Some(v)))
		})();
		locals.truncate(mark);
		match result? {
			Flow::Return(v) => return Ok(Flow::Return(v)),
			Flow::Break(id, v) => return Ok(Flow::Break(id, v)),
			Flow::Continue(id) => return Ok(Flow::Continue(id)),
			Flow::Value(Some(v)) => return Ok(Flow::Value(v)),
			Flow::Value(None) => {}
		}
	}
	unit(budget, expr.span).map(Flow::Value)
}

#[inline(never)]
fn evaluate_option(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	let value: ConstValueBuf = match expr.kind {
		E::Call(call) => {
			if matches!(call.call, hir::Call::ConstFn { .. }) {
				return evaluate_const_call(inner, exprs, expr, depth, budget, locals);
			}

			let hir::Call::Meta { hash } = call.call else {
				refuse!(span, "constant calls and option construction");
			};
			let some = crate::Hash::type_hash(crate::item!(::std::option::Option::Some));
			let none = crate::Hash::type_hash(crate::item!(::std::option::Option::None));
			if hash == crate::Hash::type_hash(crate::item!(::std::result::Result::Ok))
				|| hash == crate::Hash::type_hash(crate::item!(::std::result::Result::Err))
			{
				refuse!(span, "Result constants");
			}
			if hash != some && hash != none {
				refuse!(span, "constant calls and option construction");
			}
			if call.args.len() != usize::from(hash == some) {
				refuse!(span, "constant Option constructor arity");
			}
			let payload = if hash == some {
				Some(constant_value!(next(call.args[0])))
			} else {
				None
			};
			let copied_bytes = payload.as_ref().map_or(0, |v| data_bytes(v));
			if budget.bytes.saturating_add(copied_bytes) > MAX_BYTES {
				refuse!(span, "constant byte-copy bound (16 MiB) exceeded");
			}
			check_materialization(
				payload
					.as_ref()
					.map_or(1, |v| v.as_nodes().len().saturating_add(1)),
				copied_bytes,
				budget,
				span,
			)?;
			checked!(
				span,
				ConstValueBuf::instance(
					Option::<crate::runtime::Value>::HASH,
					hash,
					payload.iter()
				)
			)
		}

		_ => unreachable!(),
	};
	finish_value(value, depth, budget, span).map(Flow::Value)
}

#[inline(never)]
fn evaluate_return(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	match expr.kind {
		E::Return(value) => {
			if budget.calls == 0 {
				refuse!(span, "constant return outside function");
			}
			let value = match value {
				Some(id) => constant_value!(evaluate(
					inner,
					exprs,
					exprs.get(id),
					depth + 1,
					budget,
					locals
				)),
				None => unit(budget, span)?,
			};
			Ok(Flow::Return(value))
		}

		_ => unreachable!(),
	}
}

#[inline(never)]
fn evaluate_field(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	match expr.kind {
		E::FieldAccess(access) => {
			let target = constant_value!(next(access.expr));
			let selected = field(&target, access.expr_field, span)?;
			copy(selected, depth, budget, span).map(Flow::Value)
		}

		_ => unreachable!(),
	}
}

// Validate the complete aggregate before cloning children or constructing the
// bounded ConstValueBuf. Its own representational limit includes the root.
fn check_materialization(
	nodes: usize,
	bytes: usize,
	budget: &Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<()> {
	if nodes > MAX_NODES || budget.nodes.saturating_add(nodes.saturating_sub(1)) > MAX_NODES {
		refuse!(span, "constant expanded-node bound (65536) exceeded");
	}
	if budget.bytes.saturating_add(bytes) > MAX_BYTES {
		refuse!(span, "constant byte-copy bound (16 MiB) exceeded");
	}
	Ok(())
}

#[inline(never)]
fn finish_value(
	value: ConstValueBuf,
	depth: usize,
	budget: &mut Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<ConstValueBuf> {
	budget.nodes = budget
		.nodes
		.saturating_add(value.as_nodes().len().saturating_sub(1));
	// Work pays for the root; charge additional materialized nodes. Children
	// have already been charged; text/key bytes are conservatively charged again
	// at aggregate boundaries to bound total compile-time copying as well.
	for kind in value.kinds() {
		match kind {
			ConstNodeKind::String(s) => budget.bytes = budget.bytes.saturating_add(s.len()),
			ConstNodeKind::Object { keys } => {
				for key in keys.iter() {
					budget.bytes = budget.bytes.saturating_add(key.len());
				}
			}
			_ => {}
		}
	}
	if budget.nodes > MAX_NODES {
		refuse!(span, "constant expanded-node bound (65536) exceeded");
	}
	if budget.bytes > MAX_BYTES {
		refuse!(span, "constant byte-copy bound (16 MiB) exceeded");
	}
	if value.height() as usize + depth.saturating_sub(budget.body_base) > MAX_DEPTH {
		refuse!(span, "constant nesting bound (128) exceeded");
	}
	#[cfg(test)]
	function_tests::data_observed(budget);
	Ok(value)
}

#[inline(never)]
fn binary_finish(
	a: ConstValueBuf,
	b: ConstValueBuf,
	op: BinOp,
	depth: usize,
	budget: &mut Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<ConstValueBuf> {
	let value = {
		if a.type_info() != b.type_info() {
			let expected = rust_alloc::format!("{}", a.type_info());
			let found = rust_alloc::format!("{}", b.type_info());
			return Err(diagnostic(
				span,
				format_args!(
					"Expected type `{}` but found `{}`",
					expected.trim_start_matches("::"),
					found.trim_start_matches("::")
				),
			));
		}
		match (
			checked!(span, read(&a).map_err(|e| e.operation)),
			checked!(span, read(&b).map_err(|e| e.operation)),
		) {
			(Scalar::Text(mut a), Scalar::Text(b))
				if matches!(op, BinOp::Add(..) | BinOp::AddAssign(..)) =>
			{
				if budget.bytes.saturating_add(a.len()).saturating_add(b.len()) > MAX_BYTES {
					refuse!(span, "constant byte bound (16 MiB) exceeded");
				}
				checked!(span, a.try_push_str(&b));
				checked!(span, ConstValueBuf::string(a))
			}
			(Scalar::Primitive(Value::Bool(a)), Scalar::Primitive(Value::Bool(b)))
				if matches!(op, BinOp::And(..) | BinOp::Or(..)) =>
			{
				ConstValueBuf::from(Inline::Bool(if matches!(op, BinOp::And(..)) {
					a && b
				} else {
					a || b
				}))
			}
			(Scalar::Primitive(a), Scalar::Primitive(b)) => {
				let op =
					binary_op(op).ok_or_else(|| diagnostic(span, "constant binary operator"))?;
				primitive(
					checked!(span, arithmetic::binary(op, a, b).map_err(|e| e.operation)),
					span,
				)?
			}
			_ => refuse!(
				span,
				"constant binary operator is not available for these operands"
			),
		}
	};
	finish_value(value, depth, budget, span)
}

#[inline(never)]
fn evaluate_const_call(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = expr.span;
	let E::Call(call) = expr.kind else {
		unreachable!()
	};
	let hir::Call::ConstFn { id } = call.call else {
		unreachable!()
	};
	let function = inner
		.constant_function(id)
		.ok_or_else(|| refusal(span, "constant function body unavailable"))?;
	let mut frame = constant_value!(prepare_const_call(
		inner, exprs, expr, depth, budget, locals
	));
	inner.constant_work.charge(span)?;
	if budget.calls >= MAX_CALLS {
		refuse!(span, "constant live-call bound (224) exceeded");
	}
	let saved = (budget.body_base, budget.source, budget.target_base);
	budget.target_base = budget.targets.len();
	budget.calls += 1;
	budget.body_base = depth + 1;
	budget.source = Some(function.item_meta.location.source_id);
	let result = block_value(
		inner,
		&function.exprs,
		&function.hir.body,
		depth + 1,
		budget,
		&mut frame,
	);
	budget.calls -= 1;
	budget.body_base = saved.0;
	budget.source = saved.1;
	budget.targets.truncate(budget.target_base);
	budget.target_base = saved.2;
	let result = match result {
		Ok(Flow::Value(v) | Flow::Return(v)) => Ok(Flow::Value(v)),
		Ok(Flow::Break(..) | Flow::Continue(..)) => Err(diagnostic(
			span,
			"loop control cannot cross a constant function call",
		)),
		Err(error) => {
			if saved.1 == Some(function.item_meta.location.source_id) {
				Err(error)
			} else if let Some(construct) = error.execution_refusal() {
				Err(refusal(span, construct))
			} else if let Some(name) = function.execution_name.as_deref() {
				Err(diagnostic(
					span,
					format_args!("in constant function `{name}`: {error}"),
				))
			} else {
				Err(diagnostic(
					span,
					"constant function origin name unavailable",
				))
			}
		}
	};
	result
}

#[inline(never)]
fn prepare_const_call(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow<Vec<(hir::Variable, ConstValueBuf)>>> {
	let span = expr.span;

	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	let E::Call(call) = expr.kind else {
		unreachable!()
	};
	let hir::Call::ConstFn { id } = call.call else {
		unreachable!()
	};
	let function = inner
		.constant_function(id)
		.ok_or_else(|| diagnostic(span, "constant function body unavailable"))?;
	if function.hir.args.len() != call.args.len() {
		return Err(crate::compile::Error::new(
			span,
			crate::compile::ErrorKind::BadArgumentCount {
				actual: call.args.len(),
				expected: function.hir.args.len(),
			},
		));
	}
	let mut values = Vec::new();
	for id in call.args {
		checked!(span, values.try_push(constant_value!(next(*id))));
	}
	let mut frame = Vec::new();
	for (param, value) in function.hir.args.iter().zip(values) {
		let hir::FnArg::Pat(binding) = param;
		let bindings = patterns::bind(
			&inner.constant_work,
			&function.exprs,
			binding.pat,
			patterns::Input::Owned(value),
			depth + 1,
			budget,
		)
		.and_then(|found| {
			found.ok_or_else(|| diagnostic(binding.pat.span, "Panicked: pattern did not match"))
		})
		.map_err(|e| {
			if budget.source == Some(function.item_meta.location.source_id) {
				e
			} else if let Some(construct) = e.execution_refusal() {
				refusal(span, construct)
			} else if let Some(name) = function.execution_name.as_deref() {
				diagnostic(span, format_args!("in constant function `{name}`: {e}"))
			} else {
				diagnostic(span, "constant function origin name unavailable")
			}
		})?;
		for binding in bindings {
			checked!(span, frame.try_push(binding));
		}
	}

	Ok(Flow::Value(frame))
}

#[inline(never)]
fn block_statements(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	block: &hir::Block<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow<()>> {
	let span = block.span;
	for statement in block.statements {
		match statement {
			hir::Stmt::Local(local) => {
				let value = constant_value!(evaluate(
					inner,
					exprs,
					exprs.get(local.expr),
					depth + 1,
					budget,
					locals,
				));
				let bindings = patterns::bind(
					&inner.constant_work,
					exprs,
					local.pat.pat,
					patterns::Input::Owned(value),
					depth + 1,
					budget,
				)?
				.ok_or_else(|| diagnostic(local.pat.pat.span, "Panicked: pattern did not match"))?;
				for binding in bindings {
					checked!(span, locals.try_push(binding));
				}
			}
			hir::Stmt::Expr(_, id) => {
				constant_value!(evaluate(
					inner,
					exprs,
					exprs.get(*id),
					depth + 1,
					budget,
					locals
				));
			}
		}
	}
	Ok(Flow::Value(()))
}

#[inline(never)]
fn object_finish(
	mut fields: Vec<(&str, ConstValueBuf)>,
	depth: usize,
	budget: &mut Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<ConstValueBuf> {
	let value = {
		fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
		let copied_bytes = fields.iter().fold(0usize, |n, (key, child)| {
			n.saturating_add(key.len())
				.saturating_add(data_bytes(child))
		});
		if budget.bytes.saturating_add(copied_bytes) > MAX_BYTES {
			refuse!(span, "constant byte-copy bound (16 MiB) exceeded");
		}
		check_materialization(
			fields
				.iter()
				.fold(1usize, |n, (_, v)| n.saturating_add(v.as_nodes().len())),
			copied_bytes,
			budget,
			span,
		)?;
		let mut keys = Vec::new();
		for (key, _) in &fields {
			checked!(
				span,
				keys.try_push(checked!(span, Box::<str>::try_from(*key)))
			);
		}
		let mut nodes = Vec::new();
		checked!(
			span,
			nodes.try_push(ConstNodeKind::Object {
				keys: checked!(span, keys.try_into_boxed_slice())
			})
		);
		for (_, child) in fields {
			for kind in child.kinds() {
				checked!(span, nodes.try_push(checked!(span, kind.try_clone())));
			}
		}
		checked!(span, ConstValueBuf::from_kinds(nodes))
	};
	finish_value(value, depth, budget, span)
}

#[inline(never)]
fn unary_finish(
	value: ConstValueBuf,
	op: UnOp,
	depth: usize,
	budget: &mut Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<ConstValueBuf> {
	let value = {
		let Scalar::Primitive(value) = checked!(span, read(&value).map_err(|e| e.operation)) else {
			refuse!(span, "constant unary operand requires a primitive value");
		};
		let op = match op {
			UnOp::Neg(..) => arithmetic::Unary::Neg,
			UnOp::Not(..) => arithmetic::Unary::Not,
			_ => refuse!(span, "constant unary operator"),
		};
		primitive(
			checked!(span, arithmetic::unary(op, value).map_err(|e| e.operation)),
			span,
		)?
	};
	finish_value(value, depth, budget, span)
}

#[inline(never)]
fn sequence_finish(
	children: Vec<ConstValueBuf>,
	is_vec: bool,
	depth: usize,
	budget: &mut Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<ConstValueBuf> {
	if !is_vec && children.is_empty() {
		return finish_value(ConstValueBuf::from(Inline::Unit), depth, budget, span);
	}
	check_materialization(
		children
			.iter()
			.fold(1usize, |n, v| n.saturating_add(v.as_nodes().len())),
		children
			.iter()
			.fold(0usize, |n, v| n.saturating_add(data_bytes(v))),
		budget,
		span,
	)?;
	let mut nodes = Vec::new();
	let mut copied_bytes = 0usize;
	checked!(
		span,
		nodes.try_push(ConstNodeKind::Instance {
			hash: if is_vec {
				crate::runtime::Vec::HASH
			} else {
				crate::runtime::OwnedTuple::HASH
			},
			variant_hash: crate::Hash::EMPTY,
			fields: children.len() as u32
		})
	);
	for child in children {
		copied_bytes = copied_bytes.saturating_add(data_bytes(&child));
		if budget.bytes.saturating_add(copied_bytes) > MAX_BYTES {
			refuse!(span, "constant byte-copy bound (16 MiB) exceeded");
		}
		for kind in child.kinds() {
			checked!(span, nodes.try_push(checked!(span, kind.try_clone())));
		}
	}
	let value = checked!(span, ConstValueBuf::from_kinds(nodes));
	finish_value(value, depth, budget, span)
}

#[inline(never)]
fn block_value(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	block: &hir::Block<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<Flow> {
	let span = block.span;

	inner.constant_work.charge(span)?;
	check_depth(depth, budget, span)?;
	#[cfg(test)]
	let marker = core::hint::black_box(0u8);
	#[cfg(test)]
	function_tests::observe(&marker, depth, budget);
	if budget.nodes > MAX_NODES {
		return Err(refusal(
			span,
			"constant expanded-node bound (65536) exceeded",
		));
	}
	let target = block
		.label
		.map(|label| loops::push_target(budget, Some(label), false, true, span))
		.transpose()?;
	let mark = locals.len();
	let result = match block_statements(inner, exprs, block, depth, budget, locals) {
		Err(e) => Err(e),
		Ok(Flow::Return(value)) => Ok(Flow::Return(value)),
		Ok(Flow::Break(id, value)) => Ok(Flow::Break(id, value)),
		Ok(Flow::Continue(id)) => Ok(Flow::Continue(id)),
		Ok(Flow::Value(())) => match block.value {
			Some(id) => evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals),
			None => unit(budget, span).map(Flow::Value),
		},
	};
	locals.truncate(mark);
	if let Some(id) = target {
		budget.targets.truncate(id);
		if let Ok(Flow::Break(found, value)) = result {
			return Ok(if found == id {
				Flow::Value(value)
			} else {
				Flow::Break(found, value)
			});
		}
	}
	result
}

fn primitive(value: Value, span: crate::ast::Span) -> crate::compile::Result<ConstValueBuf> {
	Ok(ConstValueBuf::from(match value {
		Value::Unit => Inline::Unit,
		Value::Bool(v) => Inline::Bool(v),
		Value::I64(v) => Inline::Signed(v),
		Value::U64(v) => Inline::Unsigned(v),
		Value::F64(v) => Inline::Float(v),
		Value::Char(v) => Inline::Char(v),
		Value::Ordering(v) => Inline::Ordering(v),
		_ => return Err(diagnostic(span, "unsupported constant primitive")),
	}))
}

fn refusal(span: crate::ast::Span, construct: &'static str) -> crate::compile::Error {
	crate::compile::Error::new(
		span,
		crate::compile::ErrorKind::ExecutionRefusal { construct },
	)
}

fn data_bytes(value: &ConstValue) -> usize {
	value
		.as_nodes()
		.iter()
		.map(|node| &node.kind)
		.fold(0usize, |bytes, kind| {
			bytes.saturating_add(match kind {
				ConstNodeKind::String(s) => s.len(),
				ConstNodeKind::Object { keys } => {
					keys.iter().fold(0usize, |n, k| n.saturating_add(k.len()))
				}
				_ => 0,
			})
		})
}

// Work pays for the root. Charge all other copied nodes and bytes before allocation.
fn copy(
	value: &ConstValue,
	depth: usize,
	budget: &mut Budget,
	span: crate::ast::Span,
) -> crate::compile::Result<ConstValueBuf> {
	let nodes = budget
		.nodes
		.saturating_add(value.as_nodes().len().saturating_sub(1));
	let bytes = budget.bytes.saturating_add(data_bytes(value));
	if nodes > MAX_NODES {
		return Err(refusal(
			span,
			"constant expanded-node bound (65536) exceeded",
		));
	}
	if bytes > MAX_BYTES {
		return Err(refusal(span, "constant byte-copy bound (16 MiB) exceeded"));
	}
	if value.height() as usize + depth.saturating_sub(budget.body_base) > MAX_DEPTH {
		return Err(refusal(span, "constant nesting bound (128) exceeded"));
	}
	budget.nodes = nodes;
	budget.bytes = bytes;
	#[cfg(test)]
	function_tests::data_observed(budget);
	value.try_to_owned().map_err(|e| diagnostic(span, e))
}

fn type_name(value: &ConstValue) -> rust_alloc::string::String {
	rust_alloc::format!("{}", value.type_info())
		.trim_start_matches("::")
		.into()
}

// Validate before lowering allocates slots or emits any payload instructions.
pub(crate) fn option_shape(value: &ConstValue) -> Result<Option<bool>> {
	use crate::runtime::TypeHash;
	let ConstNodeKind::Instance {
		hash,
		variant_hash,
		fields,
	} = value.kind()
	else {
		return Ok(None);
	};
	let some = crate::Hash::type_hash(crate::item!(::std::option::Option::Some));
	let none = crate::Hash::type_hash(crate::item!(::std::option::Option::None));
	if *hash != Option::<crate::runtime::Value>::HASH
		&& *variant_hash != some
		&& *variant_hash != none
	{
		return Ok(None);
	}
	let success = *variant_hash == some;
	if *hash != Option::<crate::runtime::Value>::HASH
		|| (*variant_hash != some && *variant_hash != none)
		|| *fields != u32::from(success)
	{
		return Err(Error::new(
			ErrorKind::InvalidCode,
			"malformed Option constant",
		));
	}
	Ok(Some(success))
}

fn field<'a>(
	target: &'a ConstValue,
	field: hir::ExprField<'_>,
	span: crate::ast::Span,
) -> crate::compile::Result<&'a ConstValue> {
	use crate::runtime::TypeHash;
	match field {
		hir::ExprField::Index(index) => {
			match target.kind() {
				ConstNodeKind::Instance { hash, fields, .. }
					if *hash == crate::runtime::Vec::HASH
						|| *hash == crate::runtime::OwnedTuple::HASH =>
				{
					return target.fields().get(index).ok_or_else(|| {
						diagnostic(
							span,
							format_args!(
								"Index out of bounds, the length is `{fields}` but the index is `{index}`"
							),
						)
					});
				}
				ConstNodeKind::Inline(Inline::Unit) => {}
				ConstNodeKind::Instance { hash, .. }
					if *hash == Option::<crate::runtime::Value>::HASH =>
				{
					option_shape(target).map_err(|e| diagnostic(span, e.operation))?;
					if let Some(value) = target.fields().get(index) {
						return Ok(value);
					}
				}
				_ => {
					return Err(diagnostic(
						span,
						format_args!(
							"The tuple index get {index} operation is not supported on `{}`",
							type_name(target)
						),
					));
				}
			}
			Err(diagnostic(
				span,
				format_args!(
					"Type `{}` missing integer index `{index}`",
					type_name(target)
				),
			))
		}
		hir::ExprField::Ident(key) => {
			let ConstNodeKind::Object { keys } = target.kind() else {
				return Err(diagnostic(
					span,
					format_args!("Field `{key}` not available on `{}`", type_name(target)),
				));
			};
			keys.iter()
				.zip(target.fields())
				.find(|(k, _)| k.as_ref() == key)
				.map(|(_, v)| v)
				.ok_or_else(|| {
					diagnostic(
						span,
						format_args!("Missing field `{key}` on `std::object::Object`"),
					)
				})
		}
		hir::ExprField::IdentGenerics(..) => Err(refusal(span, "constant generic field access")),
	}
}

fn select<'a>(
	target: &'a ConstValue,
	index: &ConstValue,
	span: crate::ast::Span,
) -> crate::compile::Result<&'a ConstValue> {
	use crate::runtime::TypeHash;
	match (target.kind(), index.kind()) {
		(ConstNodeKind::Instance { hash, fields, .. }, _)
			if *hash == crate::runtime::Vec::HASH || *hash == crate::runtime::OwnedTuple::HASH =>
		{
			let index = match index.kind() {
				ConstNodeKind::Inline(Inline::Signed(i)) => usize::try_from(*i).map_err(|_| {
					diagnostic(
						span,
						format_args!("Failed to convert value `{i}` to integer `usize`"),
					)
				})?,
				ConstNodeKind::Inline(Inline::Unsigned(i)) => {
					usize::try_from(*i).map_err(|_| {
						diagnostic(
							span,
							format_args!("Failed to convert value `{i}` to integer `usize`"),
						)
					})?
				}
				_ => {
					return Err(diagnostic(
						span,
						format_args!("Expected number type, but found `{}`", type_name(index)),
					));
				}
			};
			target.fields().get(index).ok_or_else(|| {
				diagnostic(
					span,
					format_args!(
						"Index out of bounds, the length is `{fields}` but the index is `{index}`"
					),
				)
			})
		}
		(ConstNodeKind::Object { keys }, ConstNodeKind::String(key)) => keys
			.iter()
			.zip(target.fields())
			.find(|(k, _)| k.as_ref() == key.as_ref())
			.map(|(_, v)| v)
			.ok_or_else(|| {
				diagnostic(
					span,
					format_args!("Missing field `{key}` on `std::object::Object`"),
				)
			}),
		_ => Err(diagnostic(
			span,
			format_args!(
				"The index get operation `{}[{}]` is not supported",
				type_name(target),
				type_name(index)
			),
		)),
	}
}

pub(crate) fn eval_block(
	source: crate::SourceId,
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	block: &hir::Block<'_>,
) -> crate::compile::Result<ConstValueBuf> {
	block_value(
		inner,
		exprs,
		block,
		0,
		&mut Budget {
			source: Some(source),
			..Budget::default()
		},
		&mut Vec::new(),
	)
	.map(|flow| match flow {
		Flow::Value(value) => value,
		Flow::Return(_) | Flow::Break(..) | Flow::Continue(..) => {
			unreachable!("top-level control refused")
		}
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn option_shapes_and_copy_limits() {
		use crate::runtime::TypeHash;
		let some = crate::Hash::type_hash(crate::item!(::std::option::Option::Some));
		let none = crate::Hash::type_hash(crate::item!(::std::option::Option::None));
		let family = Option::<crate::runtime::Value>::HASH;
		for (hash, variant, count) in [
			(family, some, 0),
			(family, none, 1),
			(family, crate::Hash::EMPTY, 0),
			(crate::runtime::Vec::HASH, some, 1),
			(family, some, 2),
		] {
			let fields = (0..count)
				.map(|_| ConstValueBuf::from(Inline::Signed(1)))
				.collect::<rust_alloc::vec::Vec<_>>();
			let value = ConstValueBuf::instance(hash, variant, fields.iter()).unwrap();
			assert_eq!(
				option_shape(&value).unwrap_err().kind,
				ErrorKind::InvalidCode
			);
		}
		let value =
			ConstValueBuf::instance(family, some, [ConstValueBuf::string("abc").unwrap()]).unwrap();
		assert_eq!(option_shape(&value).unwrap(), Some(true));
		let empty =
			ConstValueBuf::instance(family, none, core::iter::empty::<&ConstValue>()).unwrap();
		assert_eq!(option_shape(&empty).unwrap(), Some(false));
		let span = crate::ast::Span::empty();
		for (nodes, bytes, depth) in [
			(MAX_NODES, 0, 0),
			(1, MAX_BYTES - 2, 0),
			(1, 0, MAX_DEPTH - 1),
		] {
			let mut budget = Budget {
				nodes,
				bytes,
				..Budget::default()
			};
			assert!(copy(&value, depth, &mut budget, span).is_err());
			assert_eq!((budget.nodes, budget.bytes), (nodes, bytes));
		}
	}

	#[test]
	fn selected_copy_checks_limits_before_copying() {
		let value = ConstValueBuf::tuple([ConstValueBuf::string("abc").unwrap()]).unwrap();
		let span = crate::ast::Span::empty();
		let mut budget = Budget {
			nodes: MAX_NODES - 1,
			bytes: MAX_BYTES - 3,
			..Budget::default()
		};
		let owned = copy(&value, MAX_DEPTH - 2, &mut budget, span).unwrap();
		assert!(
			matches!(owned.fields().get(0).unwrap().kind(), ConstNodeKind::String(s) if s.as_ref() == "abc")
		);
		assert_eq!(budget.nodes, MAX_NODES);
		assert_eq!(budget.bytes, MAX_BYTES);
		for (nodes, bytes, depth, message) in [
			(MAX_NODES, 0, 0, "expanded-node"),
			(1, MAX_BYTES - 2, 0, "byte-copy"),
			(1, 0, MAX_DEPTH - 1, "nesting"),
		] {
			let mut budget = Budget {
				nodes,
				bytes,
				..Budget::default()
			};
			let error = copy(&value, depth, &mut budget, span).unwrap_err();
			assert!(rust_alloc::format!("{error}").contains(message));
			assert_eq!((budget.nodes, budget.bytes), (nodes, bytes));
		}
		// A local read and a selection use this same charged copy. The second
		// copy cannot reset the first one's consumption.
		let mut budget = Budget {
			nodes: 1,
			bytes: MAX_BYTES - 3,
			..Budget::default()
		};
		copy(&value, 0, &mut budget, span).unwrap();
		assert!(copy(&value, 0, &mut budget, span).is_err());
	}
}

#[cfg(test)]
mod function_tests {
	use super::*;
	use rust_alloc::{borrow::ToOwned, string::ToString};
	#[derive(Clone, Copy, Debug, Default)]
	struct Stats {
		entry: usize,
		root: usize,
		span: usize,
		prefix: usize,
		total: usize,
		depth: usize,
		calls: usize,
		nodes: usize,
		bytes: usize,
		work: usize,
	}
	std::thread_local! {static STATS:core::cell::Cell<Stats>=core::cell::Cell::new(Stats::default());}
	pub(super) fn work_observed() {
		STATS.with(|s| {
			let mut v = s.get();
			v.work += 1;
			s.set(v);
		});
	}
	pub(super) fn data_observed(b: &Budget) {
		STATS.with(|s| {
			let mut v = s.get();
			v.nodes = v.nodes.max(b.nodes);
			v.bytes = v.bytes.max(b.bytes);
			s.set(v);
		});
	}
	pub(super) fn observe(marker: &u8, depth: usize, b: &Budget) {
		#[cfg(miri)]
		let _ = (marker, depth, b);
		#[cfg(not(miri))]
		STATS.with(|s| {
			let mut v = s.get();
			let addr = marker as *const u8 as usize;
			if v.root == 0 {
				v.root = addr;
				v.prefix = v.entry.abs_diff(addr);
			}
			v.span = v.span.max(v.root.abs_diff(addr));
			v.total = v.total.max(v.entry.abs_diff(addr));
			v.depth = v.depth.max(depth);
			v.calls = v.calls.max(b.calls);
			s.set(v);
		});
	}

	fn compile(source: &str) -> (bool, rust_alloc::string::String) {
		#[cfg(not(miri))]
		let marker = core::hint::black_box(0u8);
		#[cfg(miri)]
		STATS.with(|s| s.set(Stats::default()));
		#[cfg(not(miri))]
		STATS.with(|s| {
			s.set(Stats {
				entry: &marker as *const u8 as usize,
				..Stats::default()
			})
		});
		let context = crate::Context::with_default_modules().unwrap();
		let mut sources = crate::Sources::new();
		sources
			.insert(crate::Source::memory(source).unwrap())
			.unwrap();
		let mut diagnostics = crate::Diagnostics::new();
		let ok =
			super::super::frontend::compile(&mut sources, &context, &mut diagnostics, 0).is_ok();
		(ok, rust_alloc::format!("{:?}", diagnostics.diagnostics()))
	}
	fn constant_patterns_semantics(row: usize) {
		let values = [
			("{let (a,b)=(2,3);a+b}", "5"),
			("{let #{a,b}=#{a:2,b:3};a+b}", "5"),
			("{let #{a,..}=#{a:2,b:3};a}", "2"),
			("{let [a,..]=[2,3];a}", "2"),
			("{let [a,b]=[2,3];a+b}", "5"),
			("if let Some(x)=Some(7){x}else{0}", "7"),
			("if let Some(x)=None{x}else{0}", "0"),
			("match Some(7){Some(x) if x<4=>1,Some(x)=>x,None=>0}", "7"),
			("match true {1=>2,_=>9}", "9"),
			(
				"match (Some([2,3]),#{a:4}) {(Some([x,..]),#{a})=>x+a,_=>0}",
				"6",
			),
			("match (){()=>7,_=>0}", "7"),
			("match b'a'{b'a'=>7,_=>0}", "7"),
			("match 'a'{'a'=>7,_=>0}", "7"),
			("match \"abc\"{\"abc\"=>7,_=>0}", "7"),
			("match 2{1 if 1/0==0=>3,_=>7}", "7"),
			("match #{b:2}{#{a,..}=>a,_=>9}", "9"),
			("match [2,3]{[a]=>a,_=>9}", "9"),
			("match (2,3){(a,..)=>a,_=>9}", "2"),
			("match (\"kept\",2){(a,1)=>0,(_,2)=>7,_=>8}", "7"),
		];
		if row < values.len() {
			let (body, expected) = values[row];
			let src = rust_alloc::format!(
				"const A={body};const CHECK=if A=={expected}{{0}}else{{1/0}};pub fn main(){{CHECK}}"
			);
			let (ok, d) = compile(&src);
			assert!(ok, "{src}: {d}");
			return;
		}
		let programs = [
			"const A=match 7{1=>2};const CHECK=match A{()=>7,_=>1/0};pub fn main(){CHECK}",
			"const A=if let Some(x)=Some(7){x};const CHECK=match A{()=>7,_=>1/0};pub fn main(){CHECK}",
			"const fn f((a,b)){a+b} const A=f((2,3));const CHECK=if A==5{0}else{1/0};pub fn main(){CHECK}",
			"const fn f(){Some(7)} const fn g(){match f(){Some(x)=>x,None=>0}} const A=g();const CHECK=if A==7{0}else{1/0};pub fn main(){CHECK}",
			"const fn f(){match Some(7){Some(x) if {return x;false}=>99,_=>0}}const A=f();const CHECK=if A==7{0}else{1/0};pub fn main(){CHECK}",
			"const fn f(){let x=11;match Some(2){Some(x) if false=>99,_=>x}} const A=f();const CHECK=if A==11{0}else{1/0};pub fn main(){CHECK}",
		];
		{
			let src = programs[row - values.len()];
			let (ok, d) = compile(src);
			assert!(ok, "{src}: {d}");
		}
	}

	// Separate exact tests make every compiler invocation durably resumable in Miri.
	macro_rules! pattern_semantic_cases {
		($($name:ident => $row:expr),* $(,)?) => {$(
			#[test]
			fn $name() { constant_patterns_semantics($row); }
		)*};
	}
	pattern_semantic_cases! {
		constant_patterns_semantics_00 => 0,
		constant_patterns_semantics_01 => 1,
		constant_patterns_semantics_02 => 2,
		constant_patterns_semantics_03 => 3,
		constant_patterns_semantics_04 => 4,
		constant_patterns_semantics_05 => 5,
		constant_patterns_semantics_06 => 6,
		constant_patterns_semantics_07 => 7,
		constant_patterns_semantics_08 => 8,
		constant_patterns_semantics_09 => 9,
		constant_patterns_semantics_10 => 10,
		constant_patterns_semantics_11 => 11,
		constant_patterns_semantics_12 => 12,
		constant_patterns_semantics_13 => 13,
		constant_patterns_semantics_14 => 14,
		constant_patterns_semantics_15 => 15,
		constant_patterns_semantics_16 => 16,
		constant_patterns_semantics_17 => 17,
		constant_patterns_semantics_18 => 18,
		constant_patterns_semantics_19 => 19,
		constant_patterns_semantics_20 => 20,
		constant_patterns_semantics_21 => 21,
		constant_patterns_semantics_22 => 22,
		constant_patterns_semantics_23 => 23,
		constant_patterns_semantics_24 => 24,
	}

	#[test]
	fn constant_patterns_miss_matrix_and_work_exhaustion() {
		for body in [
			"match 1{2=>0,_=>7}",
			"match true{false=>0,_=>7}",
			"match 'a'{'b'=>0,_=>7}",
			"match \"abc\"{\"abd\"=>0,_=>7}",
			"match 1{()=>0,_=>7}",
			"match (1,){(a,b)=>0,_=>7}",
			"match []{[a,..]=>0,_=>7}",
			"match #{b:1}{#{a,..}=>0,_=>7}",
			"match Some(2){None=>0,Some(1)=>0,_=>7}",
			"match None{Some(x)=>0,_=>7}",
			"match Some([2]){Some([1])=>0,_=>7}",
			"if let Some(1)=Some(2){0}else{7}",
			"match 1{_=>7}",
			"match 1{x=>x+6}",
		] {
			let source = rust_alloc::format!(
				"const A={body};const CHECK=if A==7{{0}}else{{1/0}};pub fn main(){{CHECK}}"
			);
			let (ok, d) = compile(&source);
			assert!(ok, "{source}: {d}");
		}
		let (ok, d) =
			compile("const fn f(n){match n{0=>1,x=>f(x-1)+f(x-1)}} const A=f(18);pub fn main(){A}");
		assert!(!ok && d.contains("work bound"), "{d}");
		STATS.with(|s| assert_eq!(s.get().work, MAX_WORK));
	}

	#[test]
	fn constant_patterns_failures_and_reached_refusals() {
		for (body, error) in [
			("{let (a,b)=(2,);a}", "pattern did not match"),
			("{let (a,b)=true;a}", "pattern did not match"),
			("{let 1=2;7}", "pattern did not match"),
			("{let #{a}=#{a:2,b:3};a}", "pattern did not match"),
			("match 7{Ok(x)=>x,_=>0}", "constant Result patterns"),
			("match 7{b\"a\"=>1,_=>0}", "constant byte-string patterns"),
			("match 7{1.5=>1,_=>0}", "constant floating-point patterns"),
			("match 7{_ if 3=>1}", "Expected type"),
		] {
			let src = rust_alloc::format!("const A={body};pub fn main(){{A}}");
			let (ok, d) = compile(&src);
			assert!(!ok && d.contains(error), "{src}: {d}");
		}
		for src in [
			"const A=if false{match 7{Ok(x)=>x,_=>0}}else{7};pub fn main(){A}",
			"const A=match 7{_=>7,Ok(x)=>x};pub fn main(){A}",
		] {
			let (ok, d) = compile(src);
			assert!(ok, "{src}: {d}");
		}
		let (ok, d) = compile("const fn f((a,b)){a+b} const A=f((2,));pub fn main(){A}");
		assert!(!ok && d.contains("pattern did not match"), "{d}");
	}

	macro_rules! loop_cases { ($($name:ident => ($body:expr,$expected:expr)),* $(,)?) => {$(#[test] fn $name(){let src=rust_alloc::format!("const A={};const CHECK=if A=={}{{0}}else{{1/0}};pub fn main(){{CHECK}}",$body,$expected);let(ok,d)=compile(&src);assert!(ok,"{src}: {d}");})*};}
	loop_cases! {
		constant_loop_00 => (r#"loop {break 7}"#, "7"),
		constant_loop_01 => (r#"{let n=0;while n<3{n+=1;}n}"#, "3"),
		constant_loop_02 => (r#"{let v=Some(3);let n=0;while let Some(x)=v{n+=x;v=None;}n}"#, "3"),
		constant_loop_03 => (r#"{let n=0;for x in 0..4{n+=x;}n}"#, "6"),
		constant_loop_04 => (r#"{let n=0;for x in 0..=4{n+=x;}n}"#, "10"),
		constant_loop_05 => (r#"{let n=0;for x in [1,2,3]{n+=x;}n}"#, "6"),
		constant_loop_06 => (r#"{let n=0;for x in (1,2){n+=x;}n}"#, "3"),
		constant_loop_07 => (r#"{let n=0;for x in Some(7){n+=x;}n}"#, "7"),
		constant_loop_08 => (r#"{let n=0;for x in None{n+=1;}n}"#, "0"),
		constant_loop_10 => (r#"{let n=0;for (a,b) in [(1,2),(3,4)]{n+=a+b;}n}"#, "10"),
		constant_loop_11 => (r#"{let n=0;let s=0;while n<5{n+=1;if n==3{continue;}s+=n;}s}"#, "12"),
		constant_loop_12 => (r#"'outer: loop{loop{break 'outer 7}}"#, "7"),
		constant_loop_13 => (r#"{let n=0;'outer: while n<3{n+=1;loop{continue 'outer;}}n}"#, "3"),
		constant_loop_14 => (r#"while true{break 7}"#, "7"),
		constant_loop_15 => (r#"'out:{break 'out 7}"#, "7"),
		constant_loop_16 => (r#"{let x=1;x+={x=10;2};x}"#, "12"),
		constant_loop_17 => (r#"{let x=5;x*=3;x-=1;x/=2;x%=4;x}"#, "3"),
		constant_loop_18 => (r#"{let n=0;for x in 3..1{n+=1;}n}"#, "0"),
		constant_loop_19 => (r#"{let n=0;for x in 0..{n+=1;if n==3{break;}}n}"#, "3"),
		constant_loop_20 => (r#"{let n=0;for x in 'a'..'d'{n+=1;}n}"#, "3"),
		constant_loop_21 => (r#"{let x=11;for x in [2,3]{if x==2{continue;}break;}x}"#, "11"),
		constant_loop_22 => (r#"{let n=0;for x in 9223372036854775807..=9223372036854775807{n+=1;}n}"#, "1"),
		constant_loop_23 => (r#"{let n=0;for x in (){n+=1;}n}"#, "0"),
		constant_loop_24 => (r#"{let n=0;for x in (0..3){n+=1;}n}"#, "3"),
		constant_loop_25 => (r#"{let n=0;for x in 1u64..3u64{n+=1;}n}"#, "2"),
		constant_loop_26 => (r#"{let n=0;for x in '\u{d7ff}'..='\u{e000}'{n+=1;}n}"#, "2"),
		constant_loop_27 => (r#"{let x=1;x<<=3;x|=2;x^=3;x&=7;x>>=1;x}"#, "0"),
		constant_loop_28 => (r#"{let x=9;while let Some(x)=Some(2){break;}x}"#, "9"),
		constant_loop_29 => (r#"'out: {let x=7;loop{break 'out x}}"#, "7"),
	}

	macro_rules! range_error_cases { ($($name:ident => $body:expr),* $(,)?) => {$(
        #[test] fn $name() {
            let src=rust_alloc::format!("const A={};pub fn main(){{A}}",$body);
            let(ok,d)=compile(&src);
            assert!(!ok && d.contains("Numerical overflow"),"{src}: {d}");
        }
    )*}; }
	range_error_cases! {
		constant_loop_open_signed_max => "for x in 9223372036854775807..{break;}",
		constant_loop_open_signed_second => "for x in 9223372036854775806..{}",
		constant_loop_open_unsigned_max => "for x in 18446744073709551615u64..{break;}",
		constant_loop_open_unsigned_second => "for x in 18446744073709551614u64..{}",
		constant_loop_open_char_max => r"for x in '\u{10ffff}'..{break;}",
		constant_loop_open_char_second => r"for x in '\u{10fffe}'..{}",
	}
	loop_cases! {
		constant_loop_open_signed_before => ("{let n=0;for x in 9223372036854775806..{n+=1;break;}n}","1"),
		constant_loop_open_unsigned_before => ("{let n=0;for x in 18446744073709551614u64..{n+=1;break;}n}","1"),
		constant_loop_open_char_before => (r"{let n=0;for x in '\u{10fffe}'..{n+=1;break;}n}","1"),
		constant_loop_inclusive_unsigned_max => ("{let n=0;for x in 18446744073709551615u64..=18446744073709551615u64{n+=1;}n}","1"),
		constant_loop_inclusive_char_max => (r"{let n=0;for x in '\u{10ffff}'..='\u{10ffff}'{n+=1;}n}","1"),
		constant_loop_reassign_iterable => ("{let v=[1,2,3];let n=0;for x in v{v=[9];n+=x;}n}","6"),
		constant_loop_assign_shadow => ("{let x=11;for x in [1,2]{x=9;}x}","11"),
	}

	loop_cases! {
		constant_loop_condition_break => ("{let n=0;loop{n+=1;while {break} {} if n==3{break}}n}","3"),
		constant_loop_condition_continue => ("{let n=0;while {n+=1;if n<3{continue;}false} {} n}","3"),
		constant_loop_let_break => ("{let n=0;loop{n+=1;while let Some(x)={break} {} if n==3{break}}n}","3"),
		constant_loop_let_continue => ("{let n=0;while let Some(x)={n+=1;if n<3{continue;}None} {} n}","3"),
	}
	#[test]
	fn constant_loop_object_first_refused() {
		let (ok, d) = compile(
			r#"const A={let s="";for(k,v) in #{z:1,a:2,m:3,b:4}{s=k;break;}s};pub fn main(){A}"#,
		);
		assert!(
			!ok && d.contains("ExecutionRefusal") && d.contains("constant object iteration"),
			"{d}"
		);
	}
	#[test]
	fn constant_loop_object_all_refused() {
		let (ok, d) = compile(
			r#"const A={let s="";for(k,v) in #{z:1,a:2,m:3,b:4}{s+=k;}s};pub fn main(){A}"#,
		);
		assert!(
			!ok && d.contains("ExecutionRefusal") && d.contains("constant object iteration"),
			"{d}"
		);
	}

	#[test]
	fn constant_loop_argument_control() {
		let (ok, d) = compile(
			"const fn id(x){x} const A=loop{id(break 7)};const CHECK=if A==7{0}else{1/0};pub fn main(){CHECK}",
		);
		assert!(ok, "{d}");
	}
	#[test]
	fn constant_loop_cross_call_refused() {
		let (ok, d) = compile("const fn f(){break;}const A=loop{f();};pub fn main(){A}");
		assert!(!ok, "cross-call break admitted");
		assert!(d.to_lowercase().contains("break"), "{d}");
	}
	#[test]
	fn constant_loop_part_assignment_refused() {
		let (ok, d) = compile("const A={let a=[1,2];let b=a;b[0]=7;a};pub fn main(){A}");
		assert!(
			!ok && d.contains("ExecutionRefusal") && d.contains("constant assignment to a part"),
			"{d}"
		);
	}
	#[test]
	fn constant_loop_pattern_failure() {
		let (ok, d) = compile("const A=for (a,b) in [(1,)]{};pub fn main(){A}");
		assert!(!ok && d.contains("pattern did not match"), "{d}");
	}
	#[test]
	#[cfg_attr(miri, ignore)]
	fn constant_loop_data_boundaries() {
		for (n, expected) in [(32768, true), (32769, false)] {
			let src = rust_alloc::format!(
				"const A={{let v=[];for _ in 0..{n}{{v=[0,0];}}0}};pub fn main(){{A}}"
			);
			let (ok, d) = compile(&src);
			assert_eq!(ok, expected, "{d}");
			assert_eq!(STATS.with(|s| s.get().nodes), MAX_NODES);
			if !ok {
				assert!(d.contains("expanded-node bound"), "{d}");
			}
		}
		for n in [122, 123] {
			let src = rust_alloc::format!(
				"const A={{let v=[];for _ in 0..{n}{{v=[v,0];}}0}};pub fn main(){{A}}"
			);
			let (ok, d) = compile(&src);
			std::eprintln!(
				"0264_NEST_DATA n={n} ok={ok} stats={:?} d={d}",
				STATS.with(|s| s.get())
			);
			assert_eq!(ok, n == 122, "{d}");
			if !ok {
				assert!(d.contains("nesting bound (128)"), "{d}");
			}
		}
	}

	#[test]
	fn constant_loop_return() {
		let (ok, d) = compile(
			"const fn f(){let n=0;loop{while n<5{n+=1;if n==3{return n;}}}}const A=f();const CHECK=if A==3{0}else{1/0};pub fn main(){CHECK}",
		);
		assert!(ok, "{d}");
	}
	#[test]
	fn constant_loop_refusals() {
		for (body, expected) in [
			("for x in [1]{break 7}", "Can only break with a value"),
			(
				"{let a=[1,2];let b=a;b[0]=7;a}",
				"constant assignment to a part",
			),
			("{let a=#{x:1};a.x+=1;a}", "constant assignment to a part"),
			("{let r=0..3;for x in r{}}", "constant ranges"),
			("for x in ..3{}", "constant range without a start"),
			("for x in \"ab\"{}", "Cannot iterate over"),
			("for (a,b) in [(1,)]{}", "pattern did not match"),
			("{let n=0;while n<3{n+=1;1/0;}}", "primitive operation"),
		] {
			let src = rust_alloc::format!("const A={body};pub fn main(){{A}}");
			let (ok, d) = compile(&src);
			assert!(!ok && d.contains(expected), "{src}: {d}");
		}
	}
	#[test]
	#[cfg_attr(miri, ignore)]
	fn constant_loop_finite_boundary() {
		for (n, ok_expected) in [(666665, true), (666666, false)] {
			let src = rust_alloc::format!("const A=for _ in 0..{n}{{}};pub fn main(){{A}}");
			let start = std::time::Instant::now();
			let (ok, d) = compile(&src);
			let stats = STATS.with(|s| s.get());
			std::eprintln!(
				"0264_FINITE n={n} seconds={} stats={stats:?} ok={ok}",
				start.elapsed().as_secs_f64()
			);
			assert_eq!(ok, ok_expected, "{d}");
			assert_eq!(stats.work, if ok_expected { 3 * n + 5 } else { MAX_WORK });
			if !ok {
				assert!(d.contains("work"), "{d}");
			}
		}
	}
	#[test]
	#[cfg_attr(miri, ignore)]
	fn constant_loop_stack_boundary_2mib() {
		std::thread::Builder::new()
			.stack_size(2 << 20)
			.spawn(|| {
				for n in [21, 22] {
					let src = rust_alloc::format!(
						"const A={}7{};pub fn main(){{A}}",
						"loop{break ".repeat(n),
						"}".repeat(n)
					);
					let (ok, d) = compile(&src);
					let stats = STATS.with(|s| s.get());
					std::eprintln!("0264_STACK nesting={n} ok={ok} stats={stats:?} d={d}");
					assert_eq!(ok, n == 21, "{d}");
					if !ok {
						assert!(d.contains("MaxConstDepth"), "{d}");
					}
					assert!(
						stats.total + (512 << 10) < (2 << 20),
						"reserve consumed: {stats:?}"
					);
				}
				let (ok, d) = compile(
					"const fn f(n){if n==0{loop{break 7}}else{1+f(n-1)}}const A=f(170);pub fn main(){A}",
				);
				let stats = STATS.with(|s| s.get());
				std::eprintln!("0264_STACK loop_calls {stats:?}");
				assert!(ok, "{d}");
				assert!(stats.total + (512 << 10) < (2 << 20));
			})
			.unwrap()
			.join()
			.unwrap();
	}

	#[test]
	#[cfg_attr(miri, ignore)]
	fn constant_loop_measurements() {
		for (name, source, success) in [
			(
				"empty_infinite",
				"const A=loop{};pub fn main(){A}".to_owned(),
				false,
			),
			(
				"empty_10000",
				"const A=for _ in 0..10000{};pub fn main(){A}".to_owned(),
				true,
			),
			(
				"count_10000",
				"const fn f(){let n=0;while n<10000{n+=1;}n}const A=f();pub fn main(){A}"
					.to_owned(),
				true,
			),
			(
				"replaced_data",
				"const A={let v=[];for _ in 0..32769{v=[0,0];}0};pub fn main(){A}".to_owned(),
				false,
			),
			(
				"nested_data",
				"const A={let v=[];for _ in 0..128{v=[v,0];}0};pub fn main(){A}".to_owned(),
				false,
			),
		] {
			let start = std::time::Instant::now();
			let (ok, d) = compile(&source);
			std::eprintln!(
				"0264_MEASURE {name} seconds={} stats={:?} ok={ok} diagnostics={d}",
				start.elapsed().as_secs_f64(),
				STATS.with(|s| s.get())
			);
			assert_eq!(ok, success, "{name}: {d}");
		}
	}

	#[test]
	fn constant_function_calls_and_return() {
		for source in [
			"const fn f(x){x+1} const A=f(2);pub fn main(){A}",
			"const fn g(x){x} const fn f(){g(return 7)} const A=f();pub fn main(){A}",
			"const fn f(n){if n==0{0}else{1+f(n-1)}} const A=f(8);pub fn main(){A}",
		] {
			let (ok, diagnostics) = compile(source);
			assert!(ok, "{diagnostics}");
		}
	}
	#[test]
	fn constant_function_reference_budget() {
		let (ok, diagnostics) =
			compile("const fn f(n){if n==0{1}else{f(n-1)+f(n-1)}} const A=f(14);pub fn main(){A}");
		assert!(ok, "accepted calibration reference must fit: {diagnostics}");
		STATS.with(|s| {
			assert_eq!(s.get().nodes, 0);
			// One newly charged parameter-pattern node for each of 32,767 calls.
			assert_eq!(s.get().work, 425969 + 32767);
		});
	}
	#[test]
	fn scalar_work_and_data_are_independent() {
		let (ok, diagnostics) = compile(
			"const fn f(n){if n==0{1}else{f(n-1)+f(n-1)}} const A=[f(14),f(14),f(14),f(14),f(14)];pub fn main(){A}",
		);
		assert!(!ok && diagnostics.contains("work bound"), "{diagnostics}");
		STATS.with(|s| {
			assert_eq!(s.get().work, MAX_WORK);
			assert_eq!(s.get().nodes, 0);
		});
		let span = crate::ast::Span::empty();
		let mut b = Budget::default();
		let scalar = ConstValueBuf::from(Inline::Signed(3));
		for _ in 0..100 {
			let v = copy(&scalar, 0, &mut b, span).unwrap();
			let _moved = v;
		}
		assert_eq!((b.nodes, b.bytes), (0, 0));
		let aggregate =
			ConstValueBuf::tuple((0..1024).map(|_| ConstValueBuf::from(Inline::Signed(1))))
				.unwrap();
		for _ in 0..64 {
			copy(&aggregate, 0, &mut b, span).unwrap();
		}
		assert_eq!(b.nodes, MAX_NODES);
		assert!(
			copy(&aggregate, 0, &mut b, span)
				.unwrap_err()
				.to_string()
				.contains("expanded-node")
		);
	}
	#[test]
	#[cfg_attr(miri, ignore)]
	fn constant_function_stack_boundary_2mib() {
		std::thread::Builder::new().stack_size(2<<20).spawn(||{
let cases=[
("pending_add","const fn f(n){if n==0{0}else{1+f(n-1)}} const A=f(170);pub fn main(){A}".to_owned(),None),
("tail","const fn f(n){if n==0{0}else{f(n-1)}} const A=f(200);pub fn main(){A}".to_owned(),None),
("pending_add_limit","const fn f(n){if n==0{0}else{1+f(n-1)}} const A=f(200);pub fn main(){A}".to_owned(),Some("combined nesting")),
("call_limit","const fn f(){f()} const A=f();pub fn main(){A}".to_owned(),Some("live-call")),
("object_limit",rust_alloc::format!("const fn f(n){{if n==0{{0}}else{{{}f(n-1){}}}}} const A=f(60);pub fn main(){{A}}","#{a:".repeat(16),"}".repeat(16)),Some("combined nesting")),
("vector_limit",rust_alloc::format!("const fn f(n){{if n==0{{0}}else{{{}f(n-1){}}}}} const A=f(60);pub fn main(){{A}}","[".repeat(16),"]".repeat(16)),Some("combined nesting")),
("failure_unwind","const fn f(n){if n==0{1/0}else{1+f(n-1)}} const A=f(170);pub fn main(){A}".to_owned(),Some("primitive operation")),
("pattern_nested",rust_alloc::format!("const A=match {}7{}{{{}x{}=>x,_=>0}};pub fn main(){{A}}","[".repeat(50),"]".repeat(50),"[".repeat(50),"]".repeat(50)),None),
("pattern_calls","const fn f(n){if n==0{match Some((7,2)){Some((x,_))=>x,_=>0}}else{1+f(n-1)}}const A=f(170);pub fn main(){A}".to_owned(),None),
("pattern_failure_calls","const fn f(n){if n==0{let (a,b)=(1,);a}else{1+f(n-1)}}const A=f(170);pub fn main(){A}".to_owned(),Some("pattern did not match")),

];
for (name,source,error) in cases {
let (ok,d)=compile(&source);let stats=STATS.with(|s|s.get());
std::eprintln!("0259_STACK {name} {stats:?}");
assert_eq!(ok,error.is_none(),"{name}: {d}");
if let Some(e)=error{assert!(d.contains(e),"{name}: {d}");}
assert!(stats.span+(512<<10)<(2<<20),"reserve consumed: {name}: {stats:?}");
let (ok,d)=compile("const A=7;pub fn main(){A}");assert!(ok,"recovery: {d}");
}
}).unwrap().join().unwrap();
	}
	#[test]
	fn constant_function_control_flow_values() {
		for (body, expected) in [
			(
				"const fn g(x){x} const fn f(){g(return 7)} const A=f();",
				"7",
			),
			(
				"const fn g(x){let x=x+1;x} const fn f(x){let y=g(2);x+y} const A=f(9);",
				"12",
			),
			(
				"const fn f(){if {return 7;false}{99}else{100}} const A=f();",
				"7",
			),
			("const fn f(){1+{return 7;99}} const A=f();", "7"),
			(
				"const fn g(){return 2} const fn f(){g()+3} const A=f();",
				"5",
			),
			("const fn f(_,x){x+1} const A=f(99,2);", "3"),
			(
				"const fn even(n){if n==0{true}else{odd(n-1)}} const fn odd(n){if n==0{false}else{even(n-1)}} const A=even(8);",
				"true",
			),
			(
				"const fn f(){Some([2,3])} const V=f(); const A=V.0[1];",
				"3",
			),
			("const fn f(){if false{loop{}}else{7}} const A=f();", "7"),
		] {
			let source = rust_alloc::format!(
				"{body} const CHECK=if A=={expected}{{0}}else{{1/0}};pub fn main(){{CHECK}}"
			);
			let (ok, d) = compile(&source);
			assert!(ok, "{source}: {d}");
		}
	}
	#[test]
	fn constant_function_data_bounds() {
		let values = rust_alloc::vec!["0";1024].join(",");
		for (n, expect) in [(63, true), (64, false)] {
			let source = rust_alloc::format!(
				"const V=[{values}];const fn f(n){{let x=V;if n==0{{0}}else{{f(n-1)}}}} const A=f({n});pub fn main(){{A}}"
			);
			let (ok, d) = compile(&source);
			assert_eq!(ok, expect, "{d}");
			if !ok {
				assert!(d.contains("expanded-node"), "{d}");
			}
		}
		let (ok, d) = compile(
			"const fn f(n){if n==0{[0]}else{let v=f(n-1);[v,v]}} const A=f(16);pub fn main(){A}",
		);
		assert!(!ok && d.contains("expanded-node"), "{d}");
		let (ok, d) = compile(
			"const fn f(n){if n==0{\"x\"}else{let v=f(n-1);v+v}} const A=f(25);pub fn main(){A}",
		);
		assert!(!ok && d.contains("byte"), "{d}");
	}
	#[test]
	fn constant_function_work_shared_across_initializers() {
		let (ok, d) = compile(
			"const fn f(n){if n==0{1}else{f(n-1)+f(n-1)}} const A=f(14);const B=f(14);const C=f(14);const D=f(14);const E=f(14);pub fn main(){(A,B,C,D,E)}",
		);
		assert!(!ok && d.contains("work bound"), "{d}");
		STATS.with(|s| assert_eq!(s.get().work, MAX_WORK));
		let work = Work(core::cell::Cell::new(2));
		let span = crate::ast::Span::empty();
		work.charge(span).unwrap();
		work.charge(span).unwrap();
		assert!(work.charge(span).is_err());
		assert_eq!(work.0.get(), 0);
	}
	#[test]
	fn constant_function_small_limits_and_failure_cleanup() {
		let span = crate::ast::Span::empty();
		assert!(
			check_depth(MAX_DEPTH, &Budget::default(), span)
				.unwrap_err()
				.to_string()
				.contains("expression nesting")
		);
		assert!(
			check_depth(
				MAX_COMBINED_DEPTH,
				&Budget {
					body_base: MAX_COMBINED_DEPTH,
					..Budget::default()
				},
				span
			)
			.unwrap_err()
			.to_string()
			.contains("combined nesting")
		);
		let work = Work(core::cell::Cell::new(2));
		work.charge(span).unwrap();
		work.charge(span).unwrap();
		assert!(work.charge(span).is_err());
		for source in [
			"const fn f(x){let a=Some([x]);a.0[9]} const A=f(2);pub fn main(){A}",
			"const fn f(x,y){x} const A=f(1/0,Some(1)+1);pub fn main(){A}",
		] {
			let (ok, d) = compile(source);
			assert!(!ok, "{d}");
			let (ok, d) = compile("const fn f(){Some([3])} const A=f();pub fn main(){A}");
			assert!(ok, "{d}");
		}
	}
	#[test]
	fn constant_function_origin_and_argument_order() {
		let (ok, d) = compile("const fn f(x,y){x} const A=f(1/0,Some(1)+1);pub fn main(){A}");
		assert!(
			!ok && d.contains("span: 29:32") && d.contains("primitive operation"),
			"{d}"
		);
		let context = crate::Context::with_default_modules().unwrap();
		let mut sources = crate::Sources::new();
		sources
			.insert(crate::Source::new("helper", "const fn f(){1/0}").unwrap())
			.unwrap();
		sources
			.insert(crate::Source::new("entry", "const A=f();pub fn main(){A}").unwrap())
			.unwrap();
		let mut diagnostics = crate::Diagnostics::new();
		assert!(
			super::super::frontend::compile(&mut sources, &context, &mut diagnostics, 0).is_err()
		);
		let d = rust_alloc::format!("{:?}", diagnostics.diagnostics());
		assert!(
			d.contains("source_id: 1")
				&& d.contains("in constant function")
				&& d.contains("f")
				&& d.contains("primitive operation"),
			"{d}"
		);
	}
}
