//! Bounded constant data evaluation, independent of the legacy evaluator.

use super::{
	arithmetic,
	error::{Error, ErrorKind, Result},
	lower::binary_op,
	value::Value,
};
use crate::runtime::{ConstNodeKind, ConstValue, ConstValueBuf, Inline};
use crate::{
	alloc::String,
	ast::{BinOp, UnOp},
	hir,
	query::QueryInner,
};

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
	nodes: usize,
	bytes: usize,
}

fn diagnostic(span: crate::ast::Span, error: impl core::fmt::Display) -> crate::compile::Error {
	crate::compile::Error::msg(span, error)
}

pub(crate) fn eval(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
) -> crate::compile::Result<ConstValueBuf> {
	evaluate(inner, exprs, expr, 0, &mut Budget::default())
}

fn evaluate(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
) -> crate::compile::Result<ConstValueBuf> {
	use crate::alloc::prelude::*;
	use crate::alloc::{Box, Vec};
	use crate::runtime::TypeHash;
	use hir::ExprKind as E;
	let span = expr.span;
	macro_rules! check {
		($e:expr) => {
			$e.map_err(|e| diagnostic(span, e))?
		};
	}
	macro_rules! refuse {
		($s:expr) => {
			return Err(crate::compile::Error::new(
				span,
				crate::compile::ErrorKind::ExecutionRefusal { construct: $s },
			))
		};
	}
	if depth >= MAX_DEPTH {
		refuse!("constant nesting bound (128) exceeded");
	}
	budget.nodes += 1;
	if budget.nodes > MAX_NODES {
		refuse!("constant expanded-node bound (65536) exceeded");
	}
	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget);
	let value = match expr.kind {
		E::Group(id) => return next(id),
		E::Const(hash) => {
			let value = inner
				.get_const_value(hash)
				.ok_or_else(|| diagnostic(span, "unresolved constant"))?;
			if budget
				.nodes
				.saturating_add(value.as_nodes().len().saturating_sub(1))
				> MAX_NODES
			{
				refuse!("constant expanded-node bound (65536) exceeded");
			}
			if value.height() as usize + depth > MAX_DEPTH {
				refuse!("constant nesting bound (128) exceeded");
			}
			check!(value.try_to_owned())
		}
		E::Lit(lit) => match lit {
			hir::Lit::Str(s) => {
				if s.len() > MAX_BYTES {
					refuse!("constant byte bound (16 MiB) exceeded");
				}
				check!(ConstValueBuf::string(s))
			}
			hir::Lit::Bool(v) => ConstValueBuf::from(Inline::Bool(v)),
			hir::Lit::Signed(v) => ConstValueBuf::from(Inline::Signed(v)),
			hir::Lit::Unsigned(v) => ConstValueBuf::from(Inline::Unsigned(v)),
			hir::Lit::Float(v) => ConstValueBuf::from(Inline::Float(v)),
			hir::Lit::Char(v) => ConstValueBuf::from(Inline::Char(v)),
			hir::Lit::ByteStr(_) => {
				refuse!("byte-string constants")
			}
		},
		E::Vec(seq) | E::Tuple(seq) => {
			if seq.items.len() > MAX_NODES {
				refuse!("constant expanded-node bound (65536) exceeded");
			}
			if matches!(expr.kind, E::Tuple(_)) && seq.items.is_empty() {
				ConstValueBuf::from(Inline::Unit)
			} else {
				let mut nodes = Vec::new();
				check!(nodes.try_push(ConstNodeKind::Instance {
					hash: if matches!(expr.kind, E::Vec(_)) {
						crate::runtime::Vec::HASH
					} else {
						crate::runtime::OwnedTuple::HASH
					},
					variant_hash: crate::Hash::EMPTY,
					fields: seq.items.len() as u32,
				}));
				for id in seq.items {
					let child = next(*id)?;
					for kind in child.kinds() {
						check!(nodes.try_push(check!(kind.try_clone())));
					}
				}
				check!(ConstValueBuf::from_kinds(nodes))
			}
		}
		E::Object(object) => {
			if !matches!(object.kind, hir::ExprObjectKind::Anonymous) {
				refuse!("typed object constants");
			}
			if object.assignments.len() > MAX_NODES {
				refuse!("constant expanded-node bound (65536) exceeded");
			}
			let mut fields = Vec::new();
			for field in object.assignments {
				let child = next(field.assign)?;
				check!(fields.try_push((field.key.1, child)));
			}
			fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
			let mut keys = Vec::new();
			for (key, _) in &fields {
				check!(keys.try_push(check!(Box::<str>::try_from(*key))));
			}
			let mut nodes = Vec::new();
			check!(nodes.try_push(ConstNodeKind::Object {
				keys: check!(keys.try_into_boxed_slice())
			}));
			for (_, child) in fields {
				for kind in child.kinds() {
					check!(nodes.try_push(check!(kind.try_clone())));
				}
			}
			check!(ConstValueBuf::from_kinds(nodes))
		}
		E::Unary(unary) => {
			let value = next(unary.expr)?;
			let Scalar::Primitive(value) = check!(read(&value).map_err(|e| e.operation)) else {
				refuse!("constant unary operand requires a primitive value");
			};
			let op = match unary.op {
				UnOp::Neg(..) => arithmetic::Unary::Neg,
				UnOp::Not(..) => arithmetic::Unary::Not,
				_ => refuse!("constant unary operator"),
			};
			primitive(
				check!(arithmetic::unary(op, value).map_err(|e| e.operation)),
				span,
			)?
		}
		E::Binary(binary) => {
			if binary.op.is_assign() {
				refuse!("constant assignment");
			}
			let a = next(binary.lhs)?;
			if let ConstNodeKind::Inline(Inline::Bool(v)) = a.kind() {
				if matches!(binary.op, BinOp::And(..)) && !v
					|| matches!(binary.op, BinOp::Or(..)) && *v
				{
					return Ok(a);
				}
			}
			let b = next(binary.rhs)?;
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
				check!(read(&a).map_err(|e| e.operation)),
				check!(read(&b).map_err(|e| e.operation)),
			) {
				(Scalar::Text(mut a), Scalar::Text(b)) if matches!(binary.op, BinOp::Add(..)) => {
					if a.len().saturating_add(b.len()) > MAX_BYTES {
						refuse!("constant byte bound (16 MiB) exceeded");
					}
					check!(a.try_push_str(&b));
					check!(ConstValueBuf::string(a))
				}
				(Scalar::Primitive(Value::Bool(a)), Scalar::Primitive(Value::Bool(b)))
					if matches!(binary.op, BinOp::And(..) | BinOp::Or(..)) =>
				{
					ConstValueBuf::from(Inline::Bool(if matches!(binary.op, BinOp::And(..)) {
						a && b
					} else {
						a || b
					}))
				}
				(Scalar::Primitive(a), Scalar::Primitive(b)) => {
					let op = binary_op(binary.op)
						.ok_or_else(|| diagnostic(span, "constant binary operator"))?;
					primitive(
						check!(arithmetic::binary(op, a, b).map_err(|e| e.operation)),
						span,
					)?
				}
				_ => refuse!("constant binary operator is not available for these operands"),
			}
		}
		E::Index(_) => refuse!("constant indexing"),
		E::Block(_) => refuse!("constant blocks"),
		E::If(_) => refuse!("constant if expressions"),
		E::Call(_) => refuse!("constant calls and option construction"),
		_ => refuse!("this constant expression"),
	};
	// Charge referenced data at its expanded size. Newly constructed children
	// have already been charged; text/key bytes are conservatively charged again
	// at aggregate boundaries to bound total compile-time copying as well.
	if matches!(expr.kind, E::Const(_)) {
		budget.nodes = budget
			.nodes
			.saturating_add(value.as_nodes().len().saturating_sub(1));
	}
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
		refuse!("constant expanded-node bound (65536) exceeded");
	}
	if budget.bytes > MAX_BYTES {
		refuse!("constant byte-copy bound (16 MiB) exceeded");
	}
	if value.height() as usize + depth > MAX_DEPTH {
		refuse!("constant nesting bound (128) exceeded");
	}
	Ok(value)
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
