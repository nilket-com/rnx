//! Direct expression children in one HIR body. Nested closures and async blocks own separate bodies.
use super::error::Result;
use crate::{alloc::Vec, hir};

pub(super) fn block(b: &hir::Block<'_>, pending: &mut Vec<hir::ExprId>) -> Result<()> {
	for s in b.statements {
		match s {
			hir::Stmt::Local(l) => pending.try_push(l.expr)?,
			hir::Stmt::Expr(_, id) => pending.try_push(*id)?,
		}
	}
	if let Some(v) = b.value {
		pending.try_push(v)?;
	}
	Ok(())
}

pub(super) fn children(kind: hir::ExprKind<'_>, pending: &mut Vec<hir::ExprId>) -> Result<()> {
	let condition = |c: &hir::Condition<'_>, pending: &mut Vec<hir::ExprId>| -> Result<()> {
		match c {
			hir::Condition::Expr(_, v) => pending.try_push(*v)?,
			hir::Condition::ExprLet(l) => pending.try_push(l.expr)?,
		}
		Ok(())
	};
	use hir::ExprKind as E;
	match kind {
		E::Assign(a) => {
			pending.try_push(a.lhs)?;
			pending.try_push(a.rhs)?;
		}
		E::Binary(b) => {
			pending.try_push(b.lhs)?;
			pending.try_push(b.rhs)?;
		}
		E::Unary(u) => pending.try_push(u.expr)?,
		E::Group(v) | E::Try(v) | E::Await(v) => pending.try_push(v)?,
		E::Return(v) | E::Yield(v) => {
			if let Some(v) = v {
				pending.try_push(v)?;
			}
		}
		E::Block(b) => block(b, pending)?,
		E::Loop(l) => {
			if let Some(c) = l.condition {
				condition(c, pending)?;
			}
			block(&l.body, pending)?;
		}
		E::For(f) => {
			pending.try_push(f.iter)?;
			block(&f.body, pending)?;
		}
		E::If(c) => {
			for b in c.branches {
				condition(b.condition, pending)?;
				block(&b.block, pending)?;
			}
			if let Some(b) = c.fallback {
				block(b, pending)?;
			}
		}
		E::Match(m) => {
			pending.try_push(m.expr)?;
			for b in m.branches {
				if let Some(v) = b.condition {
					pending.try_push(v)?;
				}
				pending.try_push(b.body)?;
			}
		}
		E::Call(c) => {
			match c.call {
				hir::Call::Associated { target, .. } => pending.try_push(target)?,
				hir::Call::Expr { expr } => pending.try_push(expr)?,
				_ => {}
			}
			pending.try_extend_from_slice(c.args)?;
		}
		E::FieldAccess(f) => pending.try_push(f.expr)?,
		E::Index(x) => {
			pending.try_push(x.target)?;
			pending.try_push(x.index)?;
		}
		E::Object(o) => {
			for a in o.assignments {
				pending.try_push(a.assign)?;
			}
		}
		E::Tuple(s) | E::Vec(s) => pending.try_extend_from_slice(s.items)?,
		E::Break(b) => {
			if let Some(v) = b.expr {
				pending.try_push(v)?;
			}
		}
		E::Template(t) => pending.try_extend_from_slice(t.exprs)?,
		E::Format(f) => pending.try_push(f.value)?,
		E::Range(r) => {
			use hir::ExprRange as R;
			match *r {
				R::RangeFrom { start } => pending.try_push(start)?,
				R::RangeTo { end } | R::RangeToInclusive { end } => pending.try_push(end)?,
				R::Range { start, end } | R::RangeInclusive { start, end } => {
					pending.try_push(start)?;
					pending.try_push(end)?;
				}
				R::RangeFull => {}
			}
		}
		E::Select(s) => {
			pending.try_extend_from_slice(s.exprs)?;
			for b in s.branches {
				pending.try_push(b.body)?;
			}
			if let Some(v) = s.default {
				pending.try_push(v)?;
			}
		}
		E::Variable(_)
		| E::Type(_)
		| E::Fn(_)
		| E::AsyncBlock(_)
		| E::CallClosure(_)
		| E::Lit(_)
		| E::Continue(_)
		| E::Const(_)
		| E::Static(_) => {}
	}
	Ok(())
}
