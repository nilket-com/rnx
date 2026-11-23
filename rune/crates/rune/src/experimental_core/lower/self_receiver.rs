//! Provisional facts for direct immutable self reads at formerly refused sites.
use super::*;

fn ungroup(exprs: &hir::Exprs<'_>, mut id: hir::ExprId) -> hir::ExprId {
	while let hir::ExprKind::Group(inner) = exprs.get(id).kind {
		id = inner;
	}
	id
}

pub(super) fn binding(exprs: &hir::Exprs<'_>, item: &hir::ItemFn<'_>) -> Option<hir::Variable> {
	let hir::FnArg::Pat(pat) = item.args.first()?;
	let hir::PatKind::Path(hir::PatPathKind::Ident {
		variable,
		name: hir::Name::SelfValue,
	}) = pat.pat.kind
	else {
		return None;
	};
	let variable = *variable;
	// Future closure capture assignment must keep every closure arena in this scan.
	let safe = exprs.expressions().all(|expr| {
		let lhs = match expr.kind {
			hir::ExprKind::Assign(assign) => assign.lhs,
			hir::ExprKind::Binary(binary) if binary.op.is_assign() => binary.lhs,
			_ => return true,
		};
		match exprs.get(ungroup(exprs, lhs)).kind {
			hir::ExprKind::Variable(v) => v != variable,
			hir::ExprKind::FieldAccess(_) | hir::ExprKind::Index(_) => true,
			_ => false,
		}
	});
	safe.then_some(variable)
}

impl Lower<'_, '_> {
	pub(super) fn self_method(
		&self,
		target: hir::ExprId,
		hash: Hash,
		name: Hash,
	) -> Option<(Hash, static_methods::StaticMethod)> {
		let (binding, receiver) = self.self_binding?;
		if hash != name
			|| !matches!(self.exprs.get(ungroup(self.exprs, target)).kind, hir::ExprKind::Variable(v) if v == binding)
		{
			return None;
		}
		if !self.constants.method_script.closed(hash) {
			return None;
		}
		let associated = Hash::associated_function(receiver, hash);
		let item = self.constants.instances.get(associated)?;
		(self.constants.self_method(item, receiver) == Some(associated))
			.then_some((receiver, static_methods::StaticMethod { associated, item }))
	}
}
