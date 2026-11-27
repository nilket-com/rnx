//! A boxed adapter may be bound and copied locally. Every use of the value
//! must end at a consumer that does not pass it on to arbitrary script code.
//! Unrecognized expression edges are sinks, including function returns.
use super::*;
use crate::alloc::HashSet;
use crate::function::pipeline::Method;
use hir::ExprKind as E;

type Uses = Vec<(hir::ExprId, bool)>;

fn block(uses: &mut Uses, body: &hir::Block<'_>) -> Result<()> {
	for statement in body.statements {
		match statement {
			hir::Stmt::Local(local) => {
				let binding = matches!(
					local.pat.pat.kind,
					hir::PatKind::Ignore | hir::PatKind::Path(hir::PatPathKind::Ident { .. })
				);
				uses.try_push((local.expr, binding))?;
			}
			hir::Stmt::Expr(_, id) => uses.try_push((*id, true))?,
		}
	}
	if let Some(tail) = body.value {
		uses.try_push((tail, false))?;
	}
	Ok(())
}

fn condition(uses: &mut Uses, condition: &hir::Condition<'_>) -> Result<()> {
	let id = match condition {
		hir::Condition::Expr(_, id) => *id,
		hir::Condition::ExprLet(local) => local.expr,
	};
	uses.try_push((id, false))?;
	Ok(())
}

impl Lower<'_, '_> {
	#[cold]
	#[inline(never)]
	pub(super) fn check_adapter_uses(&self, item: &hir::ItemFn<'_>) -> Result<()> {
		let mut sites = HashSet::new();
		for &site in &self.adapter_sites {
			sites.try_insert(site)?;
		}
		let mut aliases = HashSet::new();
		// Simple bindings are recorded in lowering order, after their initializer.
		// An assignment of an adapter is an ordinary unapproved edge below.
		let carries = |mut id, aliases: &HashSet<hir::Variable>| loop {
			match self.exprs.get(id).kind {
				E::Group(inner) => id = inner,
				E::Variable(var) => return aliases.contains(&var),
				E::Call(call) => {
					return matches!(call.call,
						hir::Call::Associated { target, hash, .. } if sites.contains(&(target, Some(hash)))
						&& (hash == Method::Skip.hash() || hash == Method::Take.hash()))
				}
				_ => return false,
			}
		};
		for &(var, init) in &self.text_bindings {
			if carries(init, &aliases) {
				aliases.try_insert(var)?;
			}
		}

		let mut uses = Vec::new();
		let mut children = Vec::new();
		block(&mut uses, &item.body)?;
		while let Some((id, accepted)) = uses.pop() {
			let expr = self.exprs.get(id);
			let refuse = || self.refused(expr.span, Refusal::Method);
			if !accepted && carries(id, &aliases) {
				return Err(refuse());
			}
			match expr.kind {
				E::Group(inner) => uses.try_push((inner, accepted))?,
				E::Block(body) => block(&mut uses, body)?,
				E::If(c) => {
					for branch in c.branches {
						condition(&mut uses, branch.condition)?;
						block(&mut uses, &branch.block)?;
					}
					if let Some(body) = c.fallback {
						block(&mut uses, body)?;
					}
				}
				E::Loop(l) => {
					if let Some(c) = l.condition {
						condition(&mut uses, c)?;
					}
					block(&mut uses, &l.body)?;
				}
				E::For(f) => {
					uses.try_push((f.iter, sites.contains(&(f.iter, None))))?;
					block(&mut uses, &f.body)?;
				}
				E::Call(call) => {
					let mut safe_args = false;
					match call.call {
						hir::Call::Associated { target, hash, .. } => {
							let safe = sites.contains(&(target, Some(hash)))
								|| call.args.is_empty()
									&& !self.constants.method_script.closed(hash)
									&& (hash == Hash::ident("len") || hash == Hash::ident("clone"));
							uses.try_push((target, safe))?;
						}
						hir::Call::Meta { hash } => {
							safe_args = hash == Hash::type_hash(crate::item!(::std::mem::drop))
								|| hash == crate::function::pipeline::IS_READABLE
								|| hash == Hash::type_hash(crate::item!(::std::clone::clone));
						}
						hir::Call::Expr { expr } => uses.try_push((expr, false))?,
						hir::Call::Var { name } if aliases.contains(&name) => return Err(refuse()),
						_ => (),
					}
					for &arg in call.args {
						uses.try_push((arg, safe_args && call.args.len() == 1))?;
					}
				}
				E::CallClosure(c) if c.captures.iter().any(|v| aliases.contains(v)) => {
					return Err(refuse())
				}
				E::AsyncBlock(c) if c.captures.iter().any(|v| aliases.contains(v)) => {
					return Err(refuse())
				}
				kind => {
					super::super::hir_walk::children(kind, &mut children)?;
					for child in children.drain(..) {
						uses.try_push((child, false))?;
					}
				}
			}
		}
		Ok(())
	}
}
