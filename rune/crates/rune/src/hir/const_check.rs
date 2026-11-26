//! Structural checks on a constant before its value is needed.
use super::{self as hir, ExprKind as E};
use crate::alloc::{HashSet, Vec};
use crate::ast::{Span, UnOp};
use crate::compile::{Error, ErrorKind, ItemId, Result};
use crate::query::QueryInner;

#[derive(Clone, Copy)]
pub(crate) enum Root<'h> {
	Expr(&'h hir::Expr<'h>),
	Block(&'h hir::Block<'h>),
}

pub(crate) fn validate(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	root: Root<'_>,
) -> Result<()> {
	let mut calls = Calls::default();
	Walker::new(exprs, &mut calls).body(root, &[])?;
	calls.finish(inner)
}

/// Each argument in a runtime body is a separate constant entry. A return in
/// one argument cannot make the following argument or the callee unreachable.
pub(crate) fn validate_body_call(
	inner: &QueryInner<'_>,
	exprs: &hir::Exprs<'_>,
	args: &[hir::ExprId],
	callee: ItemId,
	runtime_locals: &[(hir::Variable, u32)],
) -> Result<()> {
	let mut calls = Calls::default();
	calls.enqueue(callee)?;
	for &id in args {
		let mut walker = Walker::new(exprs, &mut calls);
		walker.runtime_locals = runtime_locals;
		walker.body(Root::Expr(walker.get(id)?), &[])?;
	}
	calls.finish(inner)
}

#[derive(Default)]
struct Calls {
	seen: HashSet<ItemId>,
	pending: Vec<ItemId>,
}

impl Calls {
	fn enqueue(&mut self, id: ItemId) -> Result<()> {
		if self.seen.try_insert(id)? {
			self.pending.try_push(id)?;
		}
		Ok(())
	}

	fn finish(&mut self, inner: &QueryInner<'_>) -> Result<()> {
		while let Some(id) = self.pending.pop() {
			let Some(f) = inner.constant_function(id) else {
				continue;
			};
			Walker::new(&f.exprs, self).body(Root::Block(&f.hir.body), f.hir.args)?;
		}
		Ok(())
	}
}

#[derive(Clone, Copy, PartialEq)]
enum Target {
	Loop,
	For,
	Block,
}

/// Ordered siblings share a fallthrough bit. Alternative arms each start with
/// a fresh bit and join their outcomes before the enclosing expression resumes.
enum Task<'h> {
	Visit(Root<'h>),
	Pattern(&'h hir::Pat<'h>),
	Always(bool),
	EndTarget,
	ForBody(&'h hir::ExprFor<'h>),
	BadField(Span),
	BadUnary(Span, UnOp),
	ShortCircuit(hir::ExprId),
	IfTests(
		&'h [hir::ConditionalBranch<'h>],
		Option<&'h hir::Block<'h>>,
		Vec<Root<'h>>,
	),
	IfTestDone(
		&'h [hir::ConditionalBranch<'h>],
		Option<&'h hir::Block<'h>>,
		Vec<Root<'h>>,
	),
	Arms(Vec<Root<'h>>, bool),
	Match(&'h hir::ExprMatch<'h>),
	SelectArms(&'h hir::ExprSelect<'h>, usize),
	MatchTest(&'h [hir::ExprMatchBranch<'h>], Vec<Root<'h>>),
	MatchGuard(
		&'h hir::ExprMatchBranch<'h>,
		&'h [hir::ExprMatchBranch<'h>],
		Vec<Root<'h>>,
	),
}

struct Walker<'a, 'h> {
	exprs: &'h hir::Exprs<'h>,
	calls: &'a mut Calls,
	runtime_locals: &'a [(hir::Variable, u32)],
	work: Vec<Task<'h>>,
	targets: Vec<(Option<&'h str>, Target)>,
	falls: bool,
	selects: usize,
}

impl<'a, 'h> Walker<'a, 'h> {
	fn new(exprs: &'h hir::Exprs<'h>, calls: &'a mut Calls) -> Self {
		Self {
			exprs,
			calls,
			runtime_locals: &[],
			work: Vec::new(),
			targets: Vec::new(),
			falls: true,
			selects: 0,
		}
	}

	fn get(&self, id: hir::ExprId) -> Result<&'h hir::Expr<'h>> {
		self.exprs
			.get_checked(id)
			.ok_or_else(|| Error::msg(Span::empty(), "missing HIR expression in constant"))
	}

	fn expr(&mut self, id: hir::ExprId) -> Result<()> {
		self.work.try_push(Task::Visit(Root::Expr(self.get(id)?)))?;
		Ok(())
	}

	fn operands(&mut self, ids: &[hir::ExprId]) -> Result<()> {
		for &id in ids.iter().rev() {
			self.expr(id)?;
		}
		Ok(())
	}

	fn block(&mut self, b: &'h hir::Block<'h>) -> Result<()> {
		if b.label.is_some() {
			self.targets.try_push((b.label, Target::Block))?;
			self.work.try_push(Task::EndTarget)?;
		}
		if let Some(id) = b.value {
			self.expr(id)?
		}
		for stmt in b.statements.iter().rev() {
			match stmt {
				hir::Stmt::Expr(_, id) => self.expr(*id)?,
				hir::Stmt::Local(local) => {
					self.work.try_push(Task::Pattern(&local.pat.pat))?;
					self.expr(local.expr)?;
				}
			}
		}
		Ok(())
	}

	fn condition(&mut self, c: &'h hir::Condition<'h>) -> Result<()> {
		match c {
			hir::Condition::Expr(_, id) => self.expr(*id),
			hir::Condition::ExprLet(l) => {
				self.work.try_push(Task::Pattern(&l.pat.pat))?;
				self.expr(l.expr)
			}
		}
	}

	fn pattern(&mut self, p: &'h hir::Pat<'h>) -> Result<()> {
		match p.kind {
			hir::PatKind::Lit(id) => {
				let e = self.get(id)?;
				if !matches!(
					e.kind,
					E::Lit(
						hir::Lit::Bool(_)
							| hir::Lit::Signed(_) | hir::Lit::Unsigned(_)
							| hir::Lit::Char(_) | hir::Lit::Str(_)
							| hir::Lit::ByteStr(_)
					)
				) {
					return Err(Error::new(e, ErrorKind::UnsupportedPatternExpr));
				}
			}
			hir::PatKind::Sequence(s) => {
				for p in s.items.iter().rev() {
					self.work.try_push(Task::Pattern(p))?;
				}
			}
			hir::PatKind::Object(o) => {
				for b in o.bindings.iter().rev() {
					if let hir::Binding::Binding(_, _, p) = b {
						self.work.try_push(Task::Pattern(p))?;
					}
				}
			}
			hir::PatKind::Ignore | hir::PatKind::Path(_) => {}
		}
		Ok(())
	}

	fn target(&self, label: Option<&str>) -> Option<Target> {
		self.targets
			.iter()
			.rev()
			.find(|(name, kind)| match label {
				Some(_) => *name == label,
				None => *kind != Target::Block,
			})
			.map(|(_, kind)| *kind)
	}

	fn check_variable(&self, name: hir::Variable, span: Span) -> Result<()> {
		if self.runtime_locals.iter().any(|(outer, _)| *outer == name) {
			return Err(Error::msg(span, "runtime variable in constant argument"));
		}
		Ok(())
	}

	fn assign(
		&mut self,
		lhs: hir::ExprId,
		rhs: hir::ExprId,
		span: Span,
		compound: bool,
	) -> Result<()> {
		match self.get(lhs)?.kind {
			E::Variable(name) => {
				self.check_variable(name, span)?;
				self.expr(rhs)?;
			}
			E::Static(_) => self.expr(rhs)?,
			E::Index(i) => self.operands(&[i.target, i.index, rhs])?,
			E::FieldAccess(f) => {
				if compound {
					self.expr(rhs)?;
				}
				if matches!(f.expr_field, hir::ExprField::IdentGenerics(..)) {
					self.work.try_push(Task::BadField(span))?;
				}
				if !compound {
					self.expr(rhs)?;
				}
				self.expr(f.expr)?;
			}
			_ => {
				return Err(Error::new(
					span,
					if compound {
						ErrorKind::UnsupportedBinaryExpr
					} else {
						ErrorKind::UnsupportedAssignExpr
					},
				))
			}
		}
		Ok(())
	}

	fn leave(&mut self, value: Option<hir::ExprId>) -> Result<()> {
		self.work.try_push(Task::Always(false))?;
		if let Some(id) = value {
			self.expr(id)?;
		}
		Ok(())
	}

	fn visit(&mut self, e: &'h hir::Expr<'h>) -> Result<()> {
		match e.kind {
			E::Group(id) | E::Await(id) | E::Try(id) => self.expr(id)?,
			E::Block(b) => self.block(b)?,
			E::Unary(u) => {
				if !matches!(u.op, UnOp::Not(_) | UnOp::Neg(_)) {
					self.work.try_push(Task::BadUnary(e.span, u.op))?;
				}
				self.expr(u.expr)?;
			}
			E::Assign(a) => self.assign(a.lhs, a.rhs, e.span, false)?,
			E::Binary(b) if b.op.is_assign() => self.assign(b.lhs, b.rhs, e.span, true)?,
			E::Binary(b) if b.op.is_conditional() => {
				self.work.try_push(Task::ShortCircuit(b.rhs))?;
				self.expr(b.lhs)?;
			}
			E::Binary(b) => self.operands(&[b.lhs, b.rhs])?,
			E::FieldAccess(f) => {
				if matches!(f.expr_field, hir::ExprField::IdentGenerics(..)) {
					self.work.try_push(Task::BadField(e.span))?;
				}
				self.expr(f.expr)?;
			}
			E::Index(i) => self.operands(&[i.target, i.index])?,
			E::If(i) => self
				.work
				.try_push(Task::IfTests(i.branches, i.fallback, Vec::new()))?,
			E::Match(m) => {
				self.work.try_push(Task::Match(m))?;
				self.expr(m.expr)?;
			}
			E::For(f) => {
				self.work.try_push(Task::ForBody(f))?;
				self.expr(f.iter)?;
			}
			E::Loop(l) => {
				self.targets.try_push((l.label, Target::Loop))?;
				self.work.try_push(Task::EndTarget)?;
				self.work.try_push(Task::Visit(Root::Block(&l.body)))?;
				if let Some(c) = l.condition {
					// The while body is checked even if its condition leaves.
					self.work.try_push(Task::Always(true))?;
					self.condition(c)?;
				}
			}
			E::Call(c) => {
				self.operands(c.args)?;
				match c.call {
					hir::Call::ConstFn { id } => self.calls.enqueue(id)?,
					hir::Call::Expr { expr } => self.expr(expr)?,
					hir::Call::Associated { target, .. } => self.expr(target)?,
					hir::Call::Var { name } => self.check_variable(name, e.span)?,
					hir::Call::Meta { .. } => {}
				}
			}
			E::Return(arg) => self.leave(arg)?,
			E::Break(b) => match self.target(b.label) {
				None => return Err(Error::new(e, ErrorKind::BreakUnsupported)),
				Some(Target::For) if b.expr.is_some() => {
					return Err(Error::new(e, ErrorKind::BreakUnsupportedValue))
				}
				_ => self.leave(b.expr)?,
			},
			E::Continue(c) => match self.target(c.label) {
				None => return Err(Error::new(e, ErrorKind::ContinueUnsupported)),
				Some(Target::Block) => {
					return Err(Error::new(e, ErrorKind::ContinueUnsupportedBlock))
				}
				Some(_) => self.falls = false,
			},
			E::Yield(Some(id)) => self.expr(id)?,
			E::Tuple(s) | E::Vec(s) => self.operands(s.items)?,
			E::Object(o) => {
				for field in o.assignments.iter().rev() {
					self.expr(field.assign)?;
				}
			}
			E::Template(t) => self.operands(t.exprs)?,
			E::Format(f) => self.expr(f.value)?,
			E::Range(r) => match *r {
				hir::ExprRange::Range { start, end }
				| hir::ExprRange::RangeInclusive { start, end } => self.operands(&[start, end])?,
				hir::ExprRange::RangeFrom { start } => self.expr(start)?,
				hir::ExprRange::RangeTo { end } | hir::ExprRange::RangeToInclusive { end } => {
					self.expr(end)?
				}
				hir::ExprRange::RangeFull => {}
			},
			E::Select(s) => {
				self.selects += 1;
				self.work.try_push(Task::SelectArms(s, self.selects))?;
				self.operands(s.exprs)?;
			}
			E::Variable(v) => self.check_variable(v, e.span)?,
			E::AsyncBlock(b) => {
				for &v in b.captures {
					self.check_variable(v, e.span)?;
				}
			}
			E::CallClosure(c) => {
				for &v in c.captures {
					self.check_variable(v, e.span)?;
				}
			}
			E::Type(_) | E::Fn(_) | E::Lit(_) | E::Static(_) | E::Const(_) | E::Yield(None) => {}
		}
		Ok(())
	}

	fn match_arms(&mut self, mut bodies: Vec<Root<'h>>, implicit_unit: bool) -> Result<()> {
		bodies.reverse();
		self.falls = implicit_unit;
		self.work.try_push(Task::Arms(bodies, false))?;
		Ok(())
	}

	fn body(&mut self, root: Root<'h>, args: &'h [hir::FnArg<'h>]) -> Result<()> {
		use Task::*;
		self.work.try_push(Visit(root))?;
		for hir::FnArg::Pat(p) in args.iter().rev() {
			self.work.try_push(Pattern(&p.pat))?;
		}
		while let Some(task) = self.work.pop() {
			match task {
				Visit(Root::Expr(e)) if self.falls => self.visit(e)?,
				Visit(Root::Block(b)) if self.falls => self.block(b)?,
				Pattern(p) if self.falls => self.pattern(p)?,
				BadField(s) if self.falls => return Err(Error::new(s, ErrorKind::BadFieldAccess)),
				BadUnary(s, op) if self.falls => {
					return Err(Error::new(s, ErrorKind::UnsupportedUnaryOp { op }))
				}
				Always(falls) => self.falls = falls,
				EndTarget => {
					self.targets.pop();
					self.falls = true;
				}
				ForBody(f) if self.falls => {
					self.targets.try_push((f.label, Target::For))?;
					self.work.try_push(EndTarget)?;
					self.work.try_push(Visit(Root::Block(&f.body)))?;
					self.work.try_push(Pattern(&f.binding.pat))?;
				}
				ShortCircuit(rhs) if self.falls => {
					self.work.try_push(Always(true))?;
					self.expr(rhs)?;
				}
				IfTests(branches, fallback, mut bodies) => {
					if let Some(first) = branches.first() {
						self.work.try_push(IfTestDone(branches, fallback, bodies))?;
						self.falls = true;
						self.condition(first.condition)?;
					} else {
						let implicit_unit = fallback.is_none() && !bodies.is_empty();
						bodies.reverse();
						if let Some(b) = fallback {
							bodies.try_push(Root::Block(b))?;
						}
						self.falls = implicit_unit;
						self.work.try_push(Arms(bodies, false))?;
					}
				}
				IfTestDone(branches, fallback, mut bodies) => {
					if let Some((first, rest)) = branches.split_first() {
						if self.falls {
							bodies.try_push(Root::Block(&first.block))?;
						}
						self.work.try_push(IfTests(rest, fallback, bodies))?;
					}
				}
				Arms(mut bodies, prior) => {
					let falls = prior || self.falls;
					if let Some(body) = bodies.pop() {
						self.work.try_push(Arms(bodies, falls))?;
						self.falls = true;
						self.work.try_push(Visit(body))?;
					} else {
						self.falls = falls;
					}
				}
				SelectArms(select, stamp) if self.falls => {
					self.work.try_push(Always(true))?;
					if let Some(id) = select.default {
						self.expr(id)?;
						self.work.try_push(Always(true))?;
					}
					// A nested select in an operand can displace the outer arm
					// table in the reference compiler. Its default stays local.
					if self.selects == stamp {
						for branch in select.branches.iter().rev() {
							self.expr(branch.body)?;
							self.work.try_push(Pattern(&branch.pat.pat))?;
							self.work.try_push(Always(true))?;
						}
					}
				}
				Match(m) if self.falls => self.work.try_push(MatchTest(m.branches, Vec::new()))?,
				MatchTest(branches, bodies) => {
					if let Some((branch, rest)) = branches.split_first() {
						self.falls = true;
						self.work.try_push(MatchGuard(branch, rest, bodies))?;
						if let Some(guard) = branch.condition {
							self.expr(guard)?;
						}
						self.work.try_push(Pattern(&branch.pat.pat))?;
					} else {
						self.match_arms(bodies, true)?;
					}
				}
				MatchGuard(branch, rest, mut bodies) => {
					if !self.falls {
						// After a guard leaves, the old compiler may pair later
						// labels with earlier bodies. Only the preceding bodies
						// are known to be visited under either interpretation.
						self.match_arms(bodies, false)?;
					} else {
						bodies.try_push(Root::Expr(self.get(branch.body)?))?;
						let catch_all = matches!(
							branch.pat.pat.kind,
							hir::PatKind::Ignore
								| hir::PatKind::Path(hir::PatPathKind::Ident { .. })
						);
						if catch_all && branch.condition.is_none() {
							self.match_arms(bodies, false)?;
						} else {
							self.work.try_push(MatchTest(rest, bodies))?;
						}
					}
				}
				Visit(_) | Pattern(_) | BadField(_) | BadUnary(..) | ForBody(_) | Match(_)
				| ShortCircuit(_) | SelectArms(..) => {}
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests;
