//! One bounded, non-recursive pattern walker for constant evaluation.
use super::*;
use crate::ast::Span;
use crate::runtime::ConstFieldsIter;
use crate::Hash;

type Bindings = Vec<(hir::Variable, ConstValueBuf)>;
type Pending<'a> = Vec<(hir::Variable, &'a ConstValue, usize, Span)>;

pub(super) enum Input<'a> {
	Owned(ConstValueBuf),
	Borrowed(&'a ConstValue),
}

// Continuations schedule one child at a time. Wide patterns do not create a
// wide work stack, and nested patterns do not recurse on the native stack.
enum Walk<'h, 'v> {
	Node(hir::Pat<'h>, &'v ConstValue, usize),
	Sequence(
		core::slice::Iter<'h, hir::Pat<'h>>,
		ConstFieldsIter<'v>,
		usize,
	),
	Object(
		core::slice::Iter<'h, hir::Binding<'h>>,
		&'v ConstValue,
		usize,
	),
}

#[inline(never)]
fn text_eq(work: &Work, a: &str, b: &str, span: Span) -> crate::compile::Result<bool> {
	if a.len() != b.len() {
		return Ok(false);
	}
	for (a, b) in a.bytes().zip(b.bytes()) {
		work.charge(span)?;
		if a != b {
			return Ok(false);
		}
	}
	Ok(true)
}

#[inline(never)]
fn kind_matches(
	kind: hir::PatSequenceKind,
	value: &ConstValue,
	span: Span,
) -> crate::compile::Result<bool> {
	match kind {
		hir::PatSequenceKind::Sequence {
			hash,
			count,
			is_open,
		} => {
			let len = match value.kind() {
				ConstNodeKind::Inline(Inline::Unit) if hash == crate::runtime::OwnedTuple::HASH => {
					0
				}
				ConstNodeKind::Instance {
					hash: actual,
					fields,
					..
				} if *actual == hash
					&& (hash == crate::runtime::OwnedTuple::HASH
						|| hash == crate::runtime::Vec::HASH) =>
				{
					*fields as usize
				}
				ConstNodeKind::Object { keys } if hash == crate::runtime::Object::HASH => {
					keys.len()
				}
				_ => return Ok(false),
			};
			Ok(if is_open { len >= count } else { len == count })
		}
		hir::PatSequenceKind::Type { hash, variant_hash } => {
			if hash == Hash::type_hash(crate::item!(::std::result::Result)) {
				return Err(refusal(span, "constant Result patterns"));
			}
			if hash == Hash::type_hash(crate::item!(::std::cmp::Ordering)) {
				return Err(refusal(span, "constant Ordering patterns"));
			}
			if hash != Hash::type_hash(crate::item!(::std::option::Option)) {
				return Err(refusal(span, "constant user-type patterns"));
			}
			Ok(
				matches!(value.kind(), ConstNodeKind::Instance {hash: a, variant_hash: b, ..} if *a == hash && *b == variant_hash),
			)
		}
	}
}

#[inline(never)]
fn literal_matches(
	work: &Work,
	expr: &hir::Expr<'_>,
	value: &ConstValue,
	span: Span,
) -> crate::compile::Result<bool> {
	let E::Lit(lit) = expr.kind else {
		return Err(refusal(span, "constant non-literal patterns"));
	};
	Ok(match (lit, value.kind()) {
		(hir::Lit::Bool(a), ConstNodeKind::Inline(Inline::Bool(b))) => a == *b,
		(hir::Lit::Signed(a), ConstNodeKind::Inline(Inline::Signed(b))) => a == *b,
		(hir::Lit::Unsigned(a), ConstNodeKind::Inline(Inline::Unsigned(b))) => a == *b,
		(hir::Lit::Char(a), ConstNodeKind::Inline(Inline::Char(b))) => a == *b,
		(hir::Lit::Str(a), ConstNodeKind::String(b)) => text_eq(work, a, b, span)?,
		(hir::Lit::ByteStr(_), _) => return Err(refusal(span, "constant byte-string patterns")),
		(hir::Lit::Float(_), _) => return Err(refusal(span, "constant floating-point patterns")),
		_ => false,
	})
}

#[inline(never)]
fn walk<'h, 'v>(
	work: &Work,
	exprs: &hir::Exprs<'h>,
	pat: hir::Pat<'h>,
	value: &'v ConstValue,
	depth: usize,
	budget: &Budget,
) -> crate::compile::Result<Option<Pending<'v>>> {
	let mut stack = Vec::new();
	let mut pending = Vec::new();
	checked!(pat.span, stack.try_push(Walk::Node(pat, value, depth)));
	while let Some(step) = stack.pop() {
		#[cfg(test)]
		let marker = core::hint::black_box(0u8);
		#[cfg(test)]
		function_tests::observe(&marker, depth, budget);
		match step {
			Walk::Node(pat, value, depth) => {
				work.charge(pat.span)?;
				check_depth(depth, budget, pat.span)?;
				match pat.kind {
					hir::PatKind::Ignore => {}
					hir::PatKind::Path(hir::PatPathKind::Ident { variable, .. }) => {
						checked!(
							pat.span,
							pending.try_push((*variable, value, depth, pat.span))
						);
					}
					hir::PatKind::Path(hir::PatPathKind::Kind(kind)) => {
						if !kind_matches(**kind, value, pat.span)? {
							return Ok(None);
						}
					}
					hir::PatKind::Lit(id) => {
						if !literal_matches(work, exprs.get(id), value, pat.span)? {
							return Ok(None);
						}
					}
					hir::PatKind::Sequence(s) => {
						if !kind_matches(s.kind, value, pat.span)? {
							return Ok(None);
						}
						checked!(
							pat.span,
							stack.try_push(Walk::Sequence(
								s.items.iter(),
								value.fields().iter(),
								depth + 1
							))
						);
					}
					hir::PatKind::Object(o) => {
						if !kind_matches(o.kind, value, pat.span)? {
							return Ok(None);
						}
						checked!(
							pat.span,
							stack.try_push(Walk::Object(o.bindings.iter(), value, depth + 1))
						);
					}
				}
			}
			Walk::Sequence(mut pats, mut fields, depth) => {
				if let Some(pat) = pats.next() {
					let Some(value) = fields.next() else {
						return Ok(None);
					};
					checked!(
						pat.span,
						stack.try_push(Walk::Sequence(pats, fields, depth))
					);
					checked!(pat.span, stack.try_push(Walk::Node(*pat, value, depth)));
				}
			}
			Walk::Object(mut bindings, value, depth) => {
				if let Some(binding) = bindings.next() {
					use crate::ast::Spanned;
					let span = binding.span();
					let ConstNodeKind::Object { keys } = value.kind() else {
						return Ok(None);
					};
					let mut found = None;
					for (key, value) in keys.iter().zip(value.fields()) {
						work.charge(span)?;
						if text_eq(work, binding.key(), key, span)? {
							found = Some(value);
							break;
						}
					}
					let Some(child) = found else {
						return Ok(None);
					};
					checked!(span, stack.try_push(Walk::Object(bindings, value, depth)));
					match binding {
						hir::Binding::Binding(_, _, pat) => {
							checked!(span, stack.try_push(Walk::Node(**pat, child, depth)))
						}
						hir::Binding::Ident(_, _, variable) => {
							work.charge(span)?;
							check_depth(depth, budget, span)?;
							checked!(span, pending.try_push((*variable, child, depth, span)));
						}
					}
				}
			}
		}
	}
	Ok(Some(pending))
}

// No caller sees partial bindings, even if copying a later bound subtree fails.
#[inline(never)]
pub(super) fn bind(
	work: &Work,
	exprs: &hir::Exprs<'_>,
	pat: hir::Pat<'_>,
	input: Input<'_>,
	depth: usize,
	budget: &mut Budget,
) -> crate::compile::Result<Option<Bindings>> {
	let value = match &input {
		Input::Owned(v) => v.as_ref(),
		Input::Borrowed(v) => *v,
	};
	let Some(pending) = walk(work, exprs, pat, value, depth, budget)? else {
		return Ok(None);
	};
	let mut bindings = Vec::new();
	if matches!(input, Input::Owned(_)) {
		if let hir::PatKind::Path(hir::PatPathKind::Ident { variable, .. }) = pat.kind {
			drop(pending);
			if let Input::Owned(value) = input {
				checked!(pat.span, bindings.try_push((*variable, value)));
				return Ok(Some(bindings));
			}
			unreachable!();
		}
	}
	// A child of a flat buffer is a borrowed subtree, even when its owner
	// will be dropped after this call; materializing it is a charged copy.
	for (var, part, depth, span) in pending {
		checked!(
			span,
			bindings.try_push((var, copy(part, depth, budget, span)?))
		);
	}
	Ok(Some(bindings))
}

#[cfg(test)]
mod tests {
	use super::*;
	fn variable() -> hir::Variable {
		hir::Variable(crate::shared::Gen::new().next())
	}
	fn name(variable: hir::Variable) -> hir::PatPathKind<'static> {
		hir::PatPathKind::Ident {
			variable,
			name: hir::Name::Str("x"),
		}
	}
	#[test]
	fn pattern_move_copy_and_partial_failure() {
		let span = Span::empty();
		let exprs = hir::Exprs::new();
		let var = variable();
		let path = name(var);
		let pat = hir::Pat {
			span,
			kind: hir::PatKind::Path(&path),
		};
		let work = Work::default();
		let mut budget = Budget::default();
		let make = || ConstValueBuf::tuple([ConstValueBuf::string("abc").unwrap()]).unwrap();
		let moved = bind(&work, &exprs, pat, Input::Owned(make()), 0, &mut budget)
			.unwrap()
			.unwrap();
		assert_eq!((budget.nodes, budget.bytes), (0, 0));
		drop(moved);
		budget.nodes = MAX_NODES - 1;
		budget.bytes = MAX_BYTES - 3;
		let value = make();
		let copied = bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget)
			.unwrap()
			.unwrap();
		assert_eq!((budget.nodes, budget.bytes), (MAX_NODES, MAX_BYTES));
		drop(copied);
		assert!(bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget).is_err());
		let mut exprs = hir::Exprs::new();
		let lit = exprs
			.insert(hir::Expr {
				span,
				kind: E::Lit(hir::Lit::Signed(1)),
			})
			.unwrap();
		let children = [
			pat,
			hir::Pat {
				span,
				kind: hir::PatKind::Lit(lit),
			},
		];
		let sequence = hir::PatSequence {
			kind: hir::PatSequenceKind::Sequence {
				hash: crate::runtime::OwnedTuple::HASH,
				count: 2,
				is_open: false,
			},
			items: &children,
		};
		let pat = hir::Pat {
			span,
			kind: hir::PatKind::Sequence(&sequence),
		};
		let value = ConstValueBuf::tuple([
			ConstValueBuf::string("abc").unwrap(),
			ConstValueBuf::from(Inline::Signed(2)),
		])
		.unwrap();
		let mut budget = Budget::default();
		for _ in 0..8 {
			assert!(
				bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget)
					.unwrap()
					.is_none()
			);
			assert_eq!((budget.nodes, budget.bytes), (0, 0));
		}
		// Copy one subtree, then exhaust the data bound on the next. All
		// temporary owned bindings must drop, and the borrowed root survives.
		let children = [hir::Pat {
			span,
			kind: hir::PatKind::Path(&path),
		}; 2];
		let sequence = hir::PatSequence {
			items: &children,
			..sequence
		};
		let pat = hir::Pat {
			span,
			kind: hir::PatKind::Sequence(&sequence),
		};
		let value = ConstValueBuf::tuple([
			ConstValueBuf::string("a").unwrap(),
			ConstValueBuf::string("bb").unwrap(),
		])
		.unwrap();
		let mut budget = Budget {
			bytes: MAX_BYTES - 1,
			..Budget::default()
		};
		assert!(bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget).is_err());
		assert_eq!(
			value.fields().get(1).unwrap().as_string().ok().unwrap(),
			"bb"
		);
	}
	#[test]
	fn pattern_work_text_and_object_search() {
		let span = Span::empty();
		let work = Work(core::cell::Cell::new(3));
		assert!(text_eq(&work, "abc", "abc", span).unwrap());
		assert_eq!(work.0.get(), 0);
		assert!(text_eq(&work, "abc", "abc", span).is_err());
		let work = Work(core::cell::Cell::new(0));
		assert!(!text_eq(&work, "a", "bb", span).unwrap());
		let work = Work(core::cell::Cell::new(1));
		assert!(!text_eq(&work, "abc", "xbc", span).unwrap());
		assert_eq!(work.0.get(), 0);
		let exprs = hir::Exprs::new();
		let bindings = [hir::Binding::Ident(span, "b", variable())];
		let object = hir::PatObject {
			kind: hir::PatSequenceKind::Sequence {
				hash: crate::runtime::Object::HASH,
				count: 1,
				is_open: true,
			},
			bindings: &bindings,
		};
		let mut fields = Vec::new();
		fields
			.try_push(("a", ConstValueBuf::from(Inline::Signed(1))))
			.unwrap();
		fields
			.try_push(("b", ConstValueBuf::from(Inline::Signed(2))))
			.unwrap();
		let value = object_finish(fields, 0, &mut Budget::default(), span).unwrap();
		let pat = hir::Pat {
			span,
			kind: hir::PatKind::Object(&object),
		};
		let mut budget = Budget::default();
		// root + two candidate fields + two key bytes + shorthand binding.
		let work = Work(core::cell::Cell::new(6));
		assert!(
			bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget)
				.unwrap()
				.is_some()
		);
		assert_eq!(work.0.get(), 0);
		let work = Work(core::cell::Cell::new(5));
		assert!(bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget).is_err());
	}
	#[test]
	#[cfg_attr(miri, ignore)]
	fn pattern_depth_boundary_2mib() {
		std::thread::Builder::new()
			.stack_size(2 << 20)
			.spawn(|| {
				let arena = hir::Arena::new();
				let exprs = hir::Exprs::new();
				let span = Span::empty();
				let mut pat = hir::Pat {
					span,
					kind: hir::PatKind::Ignore,
				};
				let mut value = ConstValueBuf::from(Inline::Signed(1));
				for _ in 1..MAX_DEPTH {
					let items = arena.alloc([pat]).unwrap();
					let sequence = arena
						.alloc(hir::PatSequence {
							kind: hir::PatSequenceKind::Sequence {
								hash: crate::runtime::OwnedTuple::HASH,
								count: 1,
								is_open: false,
							},
							items,
						})
						.unwrap();
					pat = hir::Pat {
						span,
						kind: hir::PatKind::Sequence(sequence),
					};
					value = ConstValueBuf::tuple([value]).unwrap();
				}
				let work = Work::default();
				let mut budget = Budget::default();
				assert!(
					bind(&work, &exprs, pat, Input::Borrowed(&value), 0, &mut budget)
						.unwrap()
						.is_some()
				);
				let e = bind(&work, &exprs, pat, Input::Borrowed(&value), 1, &mut budget)
					.err()
					.unwrap();
				assert!(rust_alloc::format!("{e}").contains("expression nesting"));
				budget.body_base = MAX_COMBINED_DEPTH - MAX_DEPTH;
				assert!(bind(
					&work,
					&exprs,
					pat,
					Input::Borrowed(&value),
					budget.body_base,
					&mut budget
				)
				.unwrap()
				.is_some());
				let e = bind(
					&work,
					&exprs,
					pat,
					Input::Borrowed(&value),
					budget.body_base + 1,
					&mut budget,
				)
				.err()
				.unwrap();
				assert!(rust_alloc::format!("{e}").contains("combined nesting"));
			})
			.unwrap()
			.join()
			.unwrap();
	}
}
