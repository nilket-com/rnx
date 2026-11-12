//! Bounded constant data evaluation, independent of the legacy evaluator.

use super::{
	arithmetic,
	error::{Error, ErrorKind, Result},
	lower::binary_op,
	value::Value,
};
use crate as rune;
use crate::alloc::prelude::*;
use crate::runtime::{ConstNodeKind, ConstValue, ConstValueBuf, Inline};
use crate::{
	alloc::{String, Vec},
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
	evaluate(
		inner,
		exprs,
		expr,
		0,
		&mut Budget::default(),
		&mut Vec::new(),
	)
}

fn evaluate(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	expr: &hir::Expr<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
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
	let mut next = |id| evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals);
	let value = match expr.kind {
		E::Group(id) => return next(id),
		E::Const(hash) => {
			let value = inner
				.get_const_value(hash)
				.ok_or_else(|| diagnostic(span, "unresolved constant"))?;
			return copy(value, depth, budget, span);
		}
		E::Variable(variable) => {
			let value = locals
				.iter()
				.rev()
				.find(|(id, _)| *id == variable)
				.ok_or_else(|| diagnostic(span, "unresolved constant local"))?;
			return copy(&value.1, depth, budget, span);
		}
		E::Lit(lit) => match lit {
			hir::Lit::Str(s) => {
				if budget.bytes.saturating_add(s.len()) > MAX_BYTES {
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
				let mut copied_bytes = 0usize;
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
					let child = evaluate(inner, exprs, exprs.get(*id), depth + 1, budget, locals)?;
					copied_bytes = copied_bytes.saturating_add(data_bytes(&child));
					if budget.bytes.saturating_add(copied_bytes) > MAX_BYTES {
						refuse!("constant byte-copy bound (16 MiB) exceeded");
					}
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
			let copied_bytes = fields.iter().fold(0usize, |n, (key, child)| {
				n.saturating_add(key.len())
					.saturating_add(data_bytes(child))
			});
			if budget.bytes.saturating_add(copied_bytes) > MAX_BYTES {
				refuse!("constant byte-copy bound (16 MiB) exceeded");
			}
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
					if budget.bytes.saturating_add(a.len()).saturating_add(b.len()) > MAX_BYTES {
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
		E::Index(index) => {
			let target = next(index.target)?;
			let index = next(index.index)?;
			return copy(select(&target, &index, span)?, depth, budget, span);
		}
		E::Block(block) => return block_value(inner, exprs, block, depth + 1, budget, locals),
		E::If(conditional) => {
			for branch in conditional.branches {
				let hir::Condition::Expr(_, id) = branch.condition else {
					refuse!("constant if pattern conditions");
				};
				let condition = evaluate(inner, exprs, exprs.get(*id), depth + 1, budget, locals)?;
				let ConstNodeKind::Inline(Inline::Bool(taken)) = condition.kind() else {
					return Err(diagnostic(
						span,
						format_args!(
							"Expected type `std::bool` but found `{}`",
							type_name(&condition)
						),
					));
				};
				if *taken {
					let value =
						block_value(inner, exprs, &branch.block, depth + 1, budget, locals)?;
					return Ok(if conditional.fallback.is_some() {
						value
					} else {
						ConstValueBuf::from(Inline::Unit)
					});
				}
			}
			return match conditional.fallback {
				Some(block) => block_value(inner, exprs, block, depth + 1, budget, locals),
				None => Ok(ConstValueBuf::from(Inline::Unit)),
			};
		}
		E::Call(call) => {
			let hir::Call::Meta { hash } = call.call else {
				refuse!("constant calls and option construction");
			};
			let some = crate::Hash::type_hash(crate::item!(::std::option::Option::Some));
			let none = crate::Hash::type_hash(crate::item!(::std::option::Option::None));
			if hash == crate::Hash::type_hash(crate::item!(::std::result::Result::Ok))
				|| hash == crate::Hash::type_hash(crate::item!(::std::result::Result::Err))
			{
				refuse!("Result constants");
			}
			if hash != some && hash != none {
				refuse!("constant calls and option construction");
			}
			if call.args.len() != usize::from(hash == some) {
				refuse!("constant Option constructor arity");
			}
			let payload = if hash == some {
				Some(next(call.args[0])?)
			} else {
				None
			};
			let copied_bytes = payload.as_ref().map_or(0, |v| data_bytes(v));
			if budget.bytes.saturating_add(copied_bytes) > MAX_BYTES {
				refuse!("constant byte-copy bound (16 MiB) exceeded");
			}
			check!(ConstValueBuf::instance(
				Option::<crate::runtime::Value>::HASH,
				hash,
				payload.iter()
			))
		}
		E::Assign(_) => refuse!("constant assignment"),
		E::Loop(_) | E::For(_) => refuse!("constant loops"),
		E::Break(_) => refuse!("constant break"),
		E::Continue(_) => refuse!("constant continue"),
		E::Return(_) => refuse!("constant return"),
		E::AsyncBlock(_) | E::Await(_) | E::Select(_) => refuse!("constant async expressions"),
		E::Yield(_) => refuse!("constant yield"),
		E::CallClosure(_) | E::Fn(_) => refuse!("constant closures and function values"),
		E::FieldAccess(access) => {
			let target = next(access.expr)?;
			let selected = field(&target, access.expr_field, span)?;
			return copy(selected, depth, budget, span);
		}
		E::Match(_) => refuse!("constant match"),
		E::Try(_) => refuse!("constant try"),
		E::Range(_) => refuse!("constant ranges"),
		E::Template(_) | E::Format(_) => refuse!("constant formatting"),
		E::Static(_) => refuse!("constant static reads"),
		E::Type(_) => refuse!("constant type values"),
	};
	// Charge referenced data at its expanded size. Newly constructed children
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

// The expression already charged its root node. Charge the rest of the selected
// tree, its height and all copied text/key bytes before allocating its copy.
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
	if depth.saturating_add(value.height() as usize) > MAX_DEPTH {
		return Err(refusal(span, "constant nesting bound (128) exceeded"));
	}
	budget.nodes = nodes;
	budget.bytes = bytes;
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
						diagnostic(span,
						format_args!("Index out of bounds, the length is `{fields}` but the index is `{index}`"))
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
					))
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
					))
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
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	block: &hir::Block<'_>,
) -> crate::compile::Result<ConstValueBuf> {
	block_value(
		inner,
		exprs,
		block,
		0,
		&mut Budget::default(),
		&mut Vec::new(),
	)
}

fn block_value(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	block: &hir::Block<'_>,
	depth: usize,
	budget: &mut Budget,
	locals: &mut Vec<(hir::Variable, ConstValueBuf)>,
) -> crate::compile::Result<ConstValueBuf> {
	let span = block.span;
	if depth >= MAX_DEPTH {
		return Err(refusal(span, "constant nesting bound (128) exceeded"));
	}
	budget.nodes = budget.nodes.saturating_add(1);
	if budget.nodes > MAX_NODES {
		return Err(refusal(
			span,
			"constant expanded-node bound (65536) exceeded",
		));
	}
	if block.label.is_some() {
		return Err(refusal(span, "constant block labels"));
	}
	let mark = locals.len();
	let result = (|| {
		for statement in block.statements {
			match statement {
				hir::Stmt::Local(local) => {
					let binding = match local.pat.pat.kind {
						hir::PatKind::Ignore => None,
						hir::PatKind::Path(hir::PatPathKind::Ident { variable, .. }) => {
							Some(variable)
						}
						_ => return Err(refusal(local.span, "constant binding patterns")),
					};
					let value = evaluate(
						inner,
						exprs,
						exprs.get(local.expr),
						depth + 1,
						budget,
						locals,
					)?;
					if let Some(variable) = binding {
						locals
							.try_push((*variable, value))
							.map_err(|e| diagnostic(span, e))?;
					}
				}
				hir::Stmt::Expr(_, id) => {
					evaluate(inner, exprs, exprs.get(*id), depth + 1, budget, locals)?;
				}
			}
		}
		match block.value {
			Some(id) => evaluate(inner, exprs, exprs.get(id), depth + 1, budget, locals),
			None => Ok(ConstValueBuf::from(Inline::Unit)),
		}
	})();
	locals.truncate(mark);
	result
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
			let mut budget = Budget { nodes, bytes };
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
			let mut budget = Budget { nodes, bytes };
			let error = copy(&value, depth, &mut budget, span).unwrap_err();
			assert!(rust_alloc::format!("{error}").contains(message));
			assert_eq!((budget.nodes, budget.bytes), (nodes, bytes));
		}
		// A local read and a selection use this same charged copy. The second
		// copy cannot reset the first one's consumption.
		let mut budget = Budget {
			nodes: 1,
			bytes: MAX_BYTES - 3,
		};
		copy(&value, 0, &mut budget, span).unwrap();
		assert!(copy(&value, 0, &mut budget, span).is_err());
	}
}
