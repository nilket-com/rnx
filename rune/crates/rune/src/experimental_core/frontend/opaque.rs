//! Bounded host-value use proof for the checked session envelope. No runtime lookup.
use super::*;
use crate as rune;
use crate::hir;

const WORK: usize = 2_000_000;
const NODES: usize = 262_144;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fact {
	Bottom,
	Plain,
	Exact(Hash),
	Maybe,
}
impl Fact {
	fn join(self, other: Self) -> Self {
		match (self, other) {
			(Self::Bottom, v) | (v, Self::Bottom) => v,
			(Self::Plain, Self::Plain) => Self::Plain,
			(Self::Exact(a), Self::Exact(b)) if a == b => Self::Exact(a),
			_ => Self::Maybe,
		}
	}
	fn opaque(self) -> bool {
		!matches!(self, Self::Plain | Self::Bottom)
	}
}

/// Constructed only by ProgramDomain from its own checked roots.
pub(crate) struct Envelope {
	pub(crate) bindings: Vec<(String, Option<Hash>)>,
	pub(crate) delta: Vec<String>,
}

struct Node {
	seed: Fact,
	edges: Vec<usize>,
}
pub(crate) struct Proof {
	facts: Vec<Fact>,
}
impl Proof {
	pub(crate) fn exact(&self, id: hir::ExprId) -> Option<Hash> {
		match self.facts.get(id.index()).copied().unwrap_or(Fact::Plain) {
			Fact::Exact(hash) => Some(hash),
			_ => None,
		}
	}
}
fn refusal(kind: Refusal) -> Error {
	Error::refused(kind)
}
fn limit() -> Error {
	refusal(Refusal::OpaqueProofLimit)
}
fn simple(pat: &hir::Pat<'_>) -> Option<hir::Variable> {
	match pat.kind {
		hir::PatKind::Path(hir::PatPathKind::Ident { variable, .. }) => Some(*variable),
		_ => None,
	}
}

struct Builder<'a, 'hir> {
	exprs: &'a hir::Exprs<'hir>,
	context: &'a Context,
	owned: Vec<bool>,
	nodes: Vec<Node>,
	variables: crate::alloc::HashMap<hir::Variable, usize>,
	bounded: bool,
	work: usize,
	restores: Vec<(hir::ExprId, Fact)>,
	state_uses: Vec<bool>,
	restored: Vec<bool>,
	state: Option<hir::Variable>,
	envelope: Option<(hir::ExprId, hir::ExprId)>,
}
impl Builder<'_, '_> {
	fn charge(&mut self, n: usize) -> Result<()> {
		if !self.bounded {
			return Ok(());
		}
		self.work = self.work.checked_add(n).ok_or_else(limit)?;
		if self.work > WORK {
			return Err(limit());
		}
		Ok(())
	}
	fn node(&mut self, seed: Fact) -> Result<usize> {
		if self.nodes.len() >= NODES {
			return Err(limit());
		}
		let id = self.nodes.len();
		self.nodes.try_push(Node {
			seed,
			edges: Vec::new(),
		})?;
		Ok(id)
	}
	fn edge(&mut self, to: usize, from: usize) -> Result<()> {
		self.charge(1)?;
		self.nodes[to].edges.try_push(from)?;
		Ok(())
	}
	fn variable(&mut self, variable: hir::Variable) -> Result<usize> {
		self.charge(1)?;
		if let Some(id) = self.variables.get(&variable) {
			return Ok(*id);
		}
		let id = self.node(Fact::Bottom)?;
		self.variables.try_insert(variable, id)?;
		Ok(id)
	}
	fn local(&mut self, pat: &hir::PatBinding<'_>, rhs: hir::ExprId) -> Result<()> {
		for variable in pat.names {
			let node = self.variable(*variable)?;
			self.edge(node, rhs.index())?;
		}
		Ok(())
	}
	fn block(&mut self, block: &hir::Block<'_>) -> Result<()> {
		for stmt in block.statements {
			if let hir::Stmt::Local(local) = stmt {
				self.local(&local.pat, local.expr)?;
			}
		}
		Ok(())
	}
	fn condition(&mut self, c: &hir::Condition<'_>) -> Result<()> {
		if let hir::Condition::ExprLet(local) = c {
			self.local(&local.pat, local.expr)?;
		}
		Ok(())
	}
	fn tail(&mut self, node: usize, block: &hir::Block<'_>) -> Result<()> {
		self.block(block)?;
		if block.label.is_some() {
			self.nodes[node].seed = Fact::Plain;
		}
		if let Some(value) = block.value {
			self.edge(node, value.index())?;
		} else {
			self.nodes[node].seed = Fact::Plain;
		}
		Ok(())
	}
	fn envelope(&mut self, item: &hir::ItemFn<'_>, contract: &Envelope) -> Result<()> {
		let fail = || refusal(Refusal::OpaqueEnvelope);
		let [hir::FnArg::Pat(arg)] = item.args else {
			return Err(fail());
		};
		let state = simple(&arg.pat).ok_or_else(fail)?;
		self.state = Some(state);
		if item.body.label.is_some() || item.body.statements.len() < contract.bindings.len() + 1 {
			return Err(fail());
		}
		for (stmt, (name, hash)) in item.body.statements.iter().zip(&contract.bindings) {
			let hir::Stmt::Local(local) = stmt else {
				return Err(fail());
			};
			if !matches!(local.pat.pat.kind, hir::PatKind::Path(hir::PatPathKind::Ident { name:hir::Name::Str(n),.. }) if *n == name.as_str())
			{
				return Err(fail());
			}
			let hir::ExprKind::Index(index) = self.exprs.get(local.expr).kind else {
				return Err(fail());
			};
			if !matches!(self.exprs.get(index.target).kind, hir::ExprKind::Variable(v) if v == state)
				|| !matches!(self.exprs.get(index.index).kind, hir::ExprKind::Lit(hir::Lit::Str(n)) if n == name.as_str())
			{
				return Err(fail());
			}
			self.restores
				.try_push((local.expr, hash.map_or(Fact::Plain, Fact::Exact)))?;
			self.state_uses[index.target.index()] = true;
			self.restored[local.expr.index()] = true;
		}
		let pair = item.body.value.ok_or_else(fail)?;
		let hir::ExprKind::Tuple(seq) = self.exprs.get(pair).kind else {
			return Err(fail());
		};
		let [delta, result] = seq.items else {
			return Err(fail());
		};
		let hir::ExprKind::Object(object) = self.exprs.get(*delta).kind else {
			return Err(fail());
		};
		if !matches!(object.kind, hir::ExprObjectKind::Anonymous)
			|| object.assignments.len() != contract.delta.len()
		{
			return Err(fail());
		}
		for (field, name) in object.assignments.iter().zip(&contract.delta) {
			if field.key.1 != name.as_str()
				|| !matches!(
					self.exprs.get(field.assign).kind,
					hir::ExprKind::Variable(_)
				) {
				return Err(fail());
			}
		}
		// The result is the final local, not an arbitrary second expression that
		// can use the state record or smuggle another unchecked envelope.
		let Some(hir::Stmt::Local(last)) = item.body.statements.last() else {
			return Err(fail());
		};
		if !matches!(self.exprs.get(*result).kind,hir::ExprKind::Variable(v) if Some(v) == simple(&last.pat.pat))
		{
			return Err(fail());
		}
		self.envelope = Some((pair, *delta));
		Ok(())
	}
	fn own(&mut self, item: &hir::ItemFn<'_>) -> Result<()> {
		let mut pending = Vec::new();
		let block = |b: &hir::Block<'_>, pending: &mut Vec<hir::ExprId>| -> Result<()> {
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
		};
		let condition = |c: &hir::Condition<'_>, pending: &mut Vec<hir::ExprId>| -> Result<()> {
			match c {
				hir::Condition::Expr(_, v) => pending.try_push(*v)?,
				hir::Condition::ExprLet(l) => pending.try_push(l.expr)?,
			}
			Ok(())
		};
		block(&item.body, &mut pending)?;
		while let Some(id) = pending.pop() {
			self.charge(1)?;
			if self.owned[id.index()] {
				continue;
			}
			self.owned[id.index()] = true;
			use hir::ExprKind as E;
			match self.exprs.get(id).kind {
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
				E::Block(b) => block(b, &mut pending)?,
				E::Loop(l) => {
					if let Some(c) = l.condition {
						condition(c, &mut pending)?;
					}
					block(&l.body, &mut pending)?;
				}
				E::For(f) => {
					pending.try_push(f.iter)?;
					block(&f.body, &mut pending)?;
				}
				E::If(c) => {
					for b in c.branches {
						condition(b.condition, &mut pending)?;
						block(&b.block, &mut pending)?;
					}
					if let Some(b) = c.fallback {
						block(b, &mut pending)?;
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
						R::RangeTo { end } | R::RangeToInclusive { end } => {
							pending.try_push(end)?
						}
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
			if self.bounded && pending.len() > NODES {
				return Err(limit());
			}
		}
		Ok(())
	}
	fn graph(&mut self, item: &hir::ItemFn<'_>) -> Result<()> {
		self.block(&item.body)?;
		for arg in item.args {
			let hir::FnArg::Pat(arg) = arg;
			for v in arg.names {
				let node = self.variable(*v)?;
				self.nodes[node].seed = Fact::Plain;
			}
		}
		for (i, expr) in self.exprs.expressions().enumerate() {
			if !self.owned[i] {
				self.nodes[i].seed = Fact::Plain;
				continue;
			}
			self.charge(1)?;
			use hir::ExprKind as E;
			match expr.kind {
				E::Variable(v) => {
					let n = self.variable(v)?;
					self.edge(i, n)?;
				}
				E::Group(v) | E::Try(v) | E::Await(v) => self.edge(i, v.index())?,
				E::Return(Some(v)) | E::Yield(Some(v)) => self.edge(i, v.index())?,
				E::Assign(a) => {
					if let E::Variable(v) = self.exprs.get(a.lhs).kind {
						let n = self.variable(v)?;
						self.edge(n, a.rhs.index())?;
					}
					self.nodes[i].seed = Fact::Plain;
				}
				E::Block(b) => self.tail(i, b)?,
				E::If(c) => {
					for b in c.branches {
						self.condition(b.condition)?;
						self.tail(i, &b.block)?;
					}
					if let Some(b) = c.fallback {
						self.tail(i, b)?;
					} else {
						self.nodes[i].seed = Fact::Plain;
					}
				}
				E::Match(m) => {
					for b in m.branches {
						self.local(&b.pat, m.expr)?;
						self.edge(i, b.body.index())?;
					}
				}
				E::Loop(l) => {
					if let Some(c) = l.condition {
						self.condition(c)?;
					}
					self.block(&l.body)?;
					self.nodes[i].seed = Fact::Plain;
				}
				E::For(f) => {
					self.local(&f.binding, f.iter)?;
					self.block(&f.body)?;
					self.nodes[i].seed = Fact::Plain;
				}
				E::Call(c) => {
					self.nodes[i].seed = match c.call {
						hir::Call::Meta { hash } => self
							.context
							.opaque
							.factories
							.get(&hash)
							.map_or(Fact::Plain, |t| Fact::Exact(t.hash)),
						_ => Fact::Plain,
					};
				}
				E::Tuple(s) | E::Vec(s) => {
					self.nodes[i].seed = Fact::Plain;
					for v in s.items {
						self.edge(i, v.index())?;
					}
				}
				E::Object(o) => {
					self.nodes[i].seed = Fact::Plain;
					for a in o.assignments {
						self.edge(i, a.assign.index())?;
					}
				}
				E::Break(b) => {
					if let Some(v) = b.expr {
						self.edge(i, v.index())?;
					} else {
						self.nodes[i].seed = Fact::Plain;
					}
				}
				// All supported projection sources are plain: host value projection
				// is refused below. Restored state reads are seeded separately.
				_ => self.nodes[i].seed = Fact::Plain,
			}
		}
		for (id, fact) in &self.restores {
			self.nodes[id.index()].seed = *fact;
			self.nodes[id.index()].edges.clear();
		}
		Ok(())
	}
	fn solve(&mut self) -> Result<Vec<Fact>> {
		let mut facts = Vec::try_with_capacity(self.nodes.len())?;
		let mut reverse = Vec::try_with_capacity(self.nodes.len())?;
		let mut pending = Vec::new();
		for (i, n) in self.nodes.iter().enumerate() {
			facts.try_push(n.seed)?;
			reverse.try_push(Vec::new())?;
			if n.seed != Fact::Bottom {
				pending.try_push(i)?;
			}
		}
		for (to, node) in self.nodes.iter().enumerate() {
			for &from in &node.edges {
				reverse[from].try_push(to)?;
			}
		}
		// Each fact rises at most twice. Propagate only the new fact along
		// reverse edges instead of rescanning the whole graph each round.
		loop {
			while let Some(from) = pending.pop() {
				self.charge(reverse[from].len() + 1)?;
				for &to in &reverse[from] {
					let next = facts[to].join(facts[from]);
					if next != facts[to] {
						facts[to] = next;
						pending.try_push(to)?;
					}
				}
			}
			// An unseeded cycle does not establish a native identity. Treat it
			// as unknown and propagate that uncertainty to its dependants too.
			for (i, fact) in facts.iter_mut().enumerate() {
				if *fact == Fact::Bottom {
					*fact = Fact::Maybe;
					pending.try_push(i)?;
				}
			}
			if pending.is_empty() {
				break;
			}
		}
		Ok(facts)
	}
	fn validate(&self, item: &hir::ItemFn<'_>, facts: &[Fact], is_entry: bool) -> Result<()> {
		let host = |id: hir::ExprId| facts.get(id.index()).is_some_and(|f| f.opaque());
		let variable = |v: hir::Variable| {
			!facts.is_empty() && self.variables.get(&v).is_none_or(|i| facts[*i].opaque())
		};
		let fail = |span: Span, kind| {
			refusal(kind).at(Location {
				source: 0,
				revision: 0,
				start: span.start.0,
				end: span.end.0,
			})
		};
		let plain = |id, span, kind| {
			if host(id) {
				Err(fail(span, kind))
			} else {
				Ok(())
			}
		};
		let local = |p: &hir::PatBinding<'_>, id, span| {
			if host(id) && simple(&p.pat).is_none() && !matches!(p.pat.kind, hir::PatKind::Ignore) {
				Err(fail(span, Refusal::OpaquePattern))
			} else {
				Ok(())
			}
		};
		let condition = |c: &hir::Condition<'_>| match c {
			hir::Condition::Expr(span, id) => plain(*id, *span, Refusal::OpaqueOperation),
			hir::Condition::ExprLet(l) => plain(l.expr, l.pat.pat.span, Refusal::OpaquePattern),
		};
		let block = |b: &hir::Block<'_>| -> Result<()> {
			for s in b.statements {
				if let hir::Stmt::Local(l) = s {
					local(&l.pat, l.expr, l.span)?;
				}
			}
			Ok(())
		};
		block(&item.body)?;
		if !is_entry {
			if let Some(v) = item.body.value {
				plain(v, self.exprs.get(v).span, Refusal::OpaqueReturn)?;
			}
		}
		for (i, e) in self.exprs.expressions().enumerate() {
			if !self.owned[i] {
				continue;
			}
			use hir::ExprKind as E;
			let p = |id, kind| plain(id, e.span, kind);
			match e.kind {
				E::Variable(v) if Some(v) == self.state => {
					if !self.state_uses[i] {
						return Err(fail(e.span, Refusal::OpaqueEnvelope));
					}
				}
				E::Fn(hash)
					if hash == Hash::type_hash(["main"])
						|| self.context.opaque.factories.contains_key(&hash)
						|| self.context.opaque.methods.contains_key(&hash) =>
				{
					return Err(fail(e.span, Refusal::OpaqueIngress));
				}
				E::Call(c) => {
					let introduced = match c.call {
						hir::Call::Meta { hash } => {
							self.context.opaque.factories.get(&hash).map(|t| t.hash)
						}
						hir::Call::Associated { target, .. } => {
							match facts.get(target.index()).copied().unwrap_or(Fact::Plain) {
								Fact::Exact(h) => Some(h),
								_ => None,
							}
						}
						_ => None,
					};
					if let Some(hash) = introduced {
						let registered = self
							.context
							.opaque
							.types
							.get(&hash)
							.ok_or_else(|| fail(e.span, Refusal::OpaqueReceiver))?;
						let effective = self.context.lookup_function(Hash::associated_function(
							hash,
							&crate::runtime::Protocol::DISPLAY_FMT,
						));
						if !effective
							.is_some_and(|h| h.same_allocation(&registered.display_handler))
						{
							return Err(fail(e.span, Refusal::OpaqueReceiver));
						}
					}
					let receiver = match c.call {
						hir::Call::Associated { target, .. } => Some(target),
						_ => None,
					};
					if let Some(target) = receiver {
						if host(target) {
							let Fact::Exact(ty) = facts[target.index()] else {
								return Err(fail(e.span, Refusal::OpaqueReceiver));
							};
							let hir::Call::Associated {
								hash, name_hash, ..
							} = c.call
							else {
								return Err(fail(e.span, Refusal::OpaqueReceiver));
							};
							let key = Hash::associated_function(ty, hash);
							if hash != name_hash
								|| !c.args.is_empty() || !self
								.context
								.opaque
								.methods
								.get(&key)
								.is_some_and(|t| t.hash == ty)
							{
								return Err(fail(e.span, Refusal::OpaqueReceiver));
							}
						}
					}
					let drop = matches!(c.call,hir::Call::Meta{hash} if hash==Hash::type_hash(crate::item!(::std::mem::drop)));
					if !drop {
						for a in c.args {
							p(*a, Refusal::OpaqueArgument)?;
						}
					}
					match c.call {
						hir::Call::Meta { hash } if hash == Hash::type_hash(["main"]) => {
							return Err(fail(e.span, Refusal::OpaqueEnvelope));
						}
						hir::Call::Var { name } if variable(name) => {
							return Err(fail(e.span, Refusal::OpaqueOperation));
						}
						hir::Call::Expr { expr } => p(expr, Refusal::OpaqueOperation)?,
						_ => {}
					}
				}
				E::CallClosure(c) => {
					if c.captures.iter().any(|v| Some(*v) == self.state) {
						return Err(fail(e.span, Refusal::OpaqueEnvelope));
					}
					if c.captures.iter().any(|v| variable(*v)) {
						return Err(fail(e.span, Refusal::OpaqueCapture));
					}
				}
				E::AsyncBlock(c) => {
					if c.captures.iter().any(|v| Some(*v) == self.state) {
						return Err(fail(e.span, Refusal::OpaqueEnvelope));
					}
					if c.captures.iter().any(|v| variable(*v)) {
						return Err(fail(e.span, Refusal::OpaqueCapture));
					}
				}
				E::Tuple(s) | E::Vec(s)
					if !self.envelope.is_some_and(|(pair, _)| pair.index() == i) =>
				{
					for v in s.items {
						p(*v, Refusal::OpaqueContainer)?;
					}
				}
				E::Object(o) if !self.envelope.is_some_and(|(_, delta)| delta.index() == i) => {
					for a in o.assignments {
						p(a.assign, Refusal::OpaqueContainer)?;
					}
				}
				E::Assign(a) => {
					if host(a.rhs) && !matches!(self.exprs.get(a.lhs).kind, E::Variable(_)) {
						return Err(fail(e.span, Refusal::OpaqueContainer));
					}
					if !matches!(self.exprs.get(a.lhs).kind, E::Variable(_)) {
						p(a.lhs, Refusal::OpaqueOperation)?;
					}
				}
				E::Binary(b) => {
					p(b.lhs, Refusal::OpaqueOperation)?;
					p(b.rhs, Refusal::OpaqueOperation)?;
				}
				E::Unary(u) => p(u.expr, Refusal::OpaqueOperation)?,
				E::Index(x) if !self.restored[i] => {
					p(x.target, Refusal::OpaqueOperation)?;
					p(x.index, Refusal::OpaqueOperation)?;
				}
				E::FieldAccess(f) => p(f.expr, Refusal::OpaqueOperation)?,
				E::Format(f) if host(f.value) => {
					let spec = f.spec;
					if spec.format_type == Some(crate::runtime::format::Type::Debug) {
						return Err(fail(e.span, Refusal::OpaqueDebug));
					}
					if !matches!(facts[f.value.index()], Fact::Exact(_)) {
						return Err(fail(e.span, Refusal::OpaqueReceiver));
					}
					if spec.fill.is_some()
						|| spec.align.is_some()
						|| spec.width.is_some()
						|| spec.precision.is_some()
						|| spec.flags.is_some()
						|| spec.format_type.is_some()
					{
						return Err(fail(e.span, Refusal::FormatSpec));
					}
				}
				E::Template(t) => {
					for id in t.exprs {
						let value = match self.exprs.get(*id).kind {
							E::Format(f) => f.value,
							_ => *id,
						};
						if host(value) && !matches!(facts[value.index()], Fact::Exact(_)) {
							return Err(fail(e.span, Refusal::OpaqueOperation));
						}
					}
				}
				E::Range(range) => {
					use hir::ExprRange as R;
					match *range {
						R::RangeFrom { start } => p(start, Refusal::OpaqueOperation)?,
						R::RangeTo { end } | R::RangeToInclusive { end } => {
							p(end, Refusal::OpaqueOperation)?
						}
						R::Range { start, end } | R::RangeInclusive { start, end } => {
							p(start, Refusal::OpaqueOperation)?;
							p(end, Refusal::OpaqueOperation)?;
						}
						R::RangeFull => {}
					}
				}
				E::Try(v) => p(v, Refusal::OpaqueOperation)?,
				E::Return(Some(v)) => p(v, Refusal::OpaqueReturn)?,
				E::Break(b) => {
					if let Some(v) = b.expr {
						p(v, Refusal::OpaqueOperation)?;
					}
				}
				E::If(c) => {
					for b in c.branches {
						condition(b.condition)?;
						block(&b.block)?;
					}
					if let Some(b) = c.fallback {
						block(b)?;
					}
				}
				E::Match(m) => {
					p(m.expr, Refusal::OpaquePattern)?;
					for b in m.branches {
						if let Some(c) = b.condition {
							p(c, Refusal::OpaqueOperation)?;
						}
					}
				}
				E::Loop(l) => {
					if let Some(c) = l.condition {
						condition(c)?;
					}
					block(&l.body)?;
				}
				E::For(f) => {
					p(f.iter, Refusal::OpaqueOperation)?;
					block(&f.body)?;
				}
				E::Block(b) => block(b)?,
				E::Await(v) | E::Yield(Some(v)) => p(v, Refusal::OpaqueOperation)?,
				_ => {}
			}
		}
		Ok(())
	}
}

pub(crate) fn analyze(
	exprs: &hir::Exprs<'_>,
	item: &hir::ItemFn<'_>,
	context: &Context,
	contract: Option<&Envelope>,
	is_entry: bool,
	captures: &[hir::Variable],
) -> Result<Proof> {
	// No host fact can arise without a verified host binding or an enrolled
	// reference in this input. Still validate the wrapper and state escape.
	let bounded = contract.is_some_and(|c| c.bindings.iter().any(|(_, h)| h.is_some()))
		|| exprs.expressions().any(|e| match e.kind {
			hir::ExprKind::Fn(h) => context.opaque.factories.contains_key(&h) || context.opaque.methods.contains_key(&h),
			hir::ExprKind::Type(t) => context.opaque.types.contains_key(&t.into_hash()),
			hir::ExprKind::Call(c) => matches!(c.call, hir::Call::Meta { hash } if context.opaque.factories.contains_key(&hash) || context.opaque.methods.contains_key(&hash)),
			_ => false,
		});
	let mut b = Builder {
		exprs,
		context,
		owned: Vec::new(),
		nodes: Vec::new(),
		variables: crate::alloc::HashMap::new(),
		bounded,
		work: 0,
		restores: Vec::new(),
		state_uses: Vec::new(),
		restored: Vec::new(),
		state: None,
		envelope: None,
	};
	for _ in exprs.expressions() {
		b.owned.try_push(false)?;
		b.state_uses.try_push(false)?;
		b.restored.try_push(false)?;
		if bounded {
			b.node(Fact::Bottom)?;
		}
	}
	if let Some(c) = contract {
		if is_entry {
			b.envelope(item, c)?;
		}
	}
	b.own(item)?;
	if !bounded {
		b.validate(item, &[], is_entry)?;
		return Ok(Proof { facts: Vec::new() });
	}
	// Host value captures are rejected by the creating body's proof. Captures in
	// an admitted closure are therefore plain, including across old programs.
	for capture in captures {
		let id = b.variable(*capture)?;
		b.nodes[id].seed = Fact::Plain;
	}
	b.graph(item)?;
	let facts = b.solve()?;
	b.validate(item, &facts, is_entry)?;
	Ok(Proof { facts })
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn worklist_propagates_conflicts_and_unseeded_cycles_and_limit_is_named() {
		let expressions = hir::Exprs::default();
		let context = Context::new();
		let mut b = Builder {
			exprs: &expressions,
			context: &context,
			owned: Vec::new(),
			nodes: Vec::new(),
			variables: crate::alloc::HashMap::new(),
			bounded: true,
			work: 0,
			restores: Vec::new(),
			state_uses: Vec::new(),
			restored: Vec::new(),
			state: None,
			envelope: None,
		};
		let a = b.node(Fact::Exact(Hash::new(1))).unwrap();
		let c = b.node(Fact::Plain).unwrap();
		let joined = b.node(Fact::Bottom).unwrap();
		b.edge(joined, a).unwrap();
		b.edge(joined, c).unwrap();
		let cycle = b.node(Fact::Bottom).unwrap();
		b.edge(cycle, cycle).unwrap();
		let dependent = b.node(Fact::Plain).unwrap();
		b.edge(dependent, cycle).unwrap();
		let facts = b.solve().unwrap();
		assert_eq!(facts[joined], Fact::Maybe);
		assert_eq!(facts[dependent], Fact::Maybe);
		assert_eq!(
			b.charge(WORK + 1).unwrap_err().refusal,
			Some(Refusal::OpaqueProofLimit)
		);
	}
}
