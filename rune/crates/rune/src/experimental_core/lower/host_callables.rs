//! A qualified host item resolves once. The resulting function has its original
//! argument contract even when the caller stores it or passes it to script code.
use super::super::{code, error::TypeName};
use super::*;

pub(crate) enum Body {
	Generated(crate::alloc::Box<Function>),
	Intrinsic(u32, u32),
	Operation(Op, u32),
	Sort(u32),
	SortBy,
	TypedCmp(Type),
	TypeOf(Type),
	Debug,
}

impl Body {
	pub(in crate::experimental_core) fn function(self) -> Result<Function> {
		let at = code::TRANSPARENT_SORT_LOCATION;
		match self {
			Self::Generated(function) => Ok(crate::alloc::Box::into_inner(function)),
			Self::Intrinsic(id, arity) => native_method_function(id, arity),
			Self::Sort(cmp) => sorting::merge_function(Some(cmp)),
			Self::SortBy => sorting::merge_function(None),
			Self::Operation(op, arity) => {
				let mut f = Function::new(arity + 1, arity)?;
				f.push(op, at)?;
				f.push(Op::Return { value: arity }, at)?;
				Ok(f)
			}
			Self::TypedCmp(tag) | Self::TypeOf(tag) => {
				let cmp = matches!(self, Self::TypedCmp(_));
				let arity = if cmp { 2 } else { 1 };
				let mut f = Function::new(arity + 2, arity)?;
				f.push(
					Op::Literal {
						out: arity,
						value: Value::Type(tag),
					},
					at,
				)?;
				let args: &[u32] = if cmp { &[0, 1, 2] } else { &[0, 1] };
				let (args_start, args_len) = f.arguments(args)?;
				f.push(
					Op::Call {
						out: arity + 1,
						function: if cmp {
							code::QUALIFIED_CMP
						} else {
							code::ANY_TYPE_OF
						},
						args_start,
						args_len,
					},
					at,
				)?;
				f.push(Op::Return { value: arity + 1 }, at)?;
				Ok(f)
			}
			Self::Debug => {
				// Dispatch passes all actual operands to debug_lines.
				let mut f = Function::new(1, 0)?;
				f.push(Op::Return { value: 0 }, at)?;
				Ok(f)
			}
		}
	}
}

impl Lower<'_, '_> {
	/// A newly admitted path uses the same argument conversion and call-site
	/// diagnostics as a value. Existing specialized calls take this branch only
	/// when their static argument count would previously have refused the site.
	#[cold]
	pub(super) fn call_host_path(
		&mut self,
		call: &hir::ExprCall<'_>,
		out: u32,
		span: Span,
	) -> Result<bool> {
		let hir::Call::Meta { hash } = call.call else {
			return Ok(false);
		};
		let value = self.slot()?;
		if !self.host_value(hash, value, span)? {
			return Ok(false);
		}
		let args = self.operands(call.args)?;
		let (args_start, args_len) = self.function.arguments(&args)?;
		self.emit(
			Op::CallValue {
				out,
				value,
				args_start,
				args_len,
			},
			span,
		)?;
		Ok(true)
	}

	#[cold]
	pub(super) fn host_count_error(
		&mut self,
		call: &hir::ExprCall<'_>,
		out: u32,
		span: Span,
	) -> Result<()> {
		if self.call_host_path(call, out, span)? {
			Ok(())
		} else {
			Err(self.refused(span, Refusal::Arity))
		}
	}

	#[cold]
	pub(super) fn host_value(&mut self, hash: Hash, out: u32, span: Span) -> Result<bool> {
		let id = if let Some(&id) = self.transaction.callable_ids.get(&hash) {
			id
		} else {
			let Some(handler) = self.context.lookup_function(hash) else {
				return Ok(false);
			};
			let identity = handler.debug_identity();
			let Some((body, receiver)) = self.host_body(hash, span)? else {
				return Ok(false);
			};
			let descriptor = code::HostCallable {
				identity,
				variadic_debug: matches!(body, Body::Debug),
				receiver,
			};
			let at = self.location(span);
			let id = (self.resolve)(Resolve::HostCallable(body), at)?;
			self.transaction.register_callable(hash, id, descriptor)?;
			id
		};
		let key = self.slot()?;
		self.emit(
			Op::Literal {
				out: key,
				value: Value::U64(u64::from(id)),
			},
			span,
		)?;
		let (args_start, args_len) = self.function.arguments(&[key])?;
		self.emit(
			Op::Call {
				out,
				function: code::HOST_FUNCTION_CREATE,
				args_start,
				args_len,
			},
			span,
		)?;
		Ok(true)
	}

	fn host_body(&mut self, hash: Hash, span: Span) -> Result<Option<(Body, Option<TypeName>)>> {
		use crate::function::{number, numeric, string_builder};
		use Body::{Intrinsic, Operation};
		let builtin = match hash {
			crate::hash_in!(crate, ::std::io::dbg)
				if self.context.stdio_dbg() && self.context.formatting() =>
			{
				Some(Body::Debug)
			}
			crate::hash_in!(crate, ::std::io::println)
				if self.context.stdio_println() && self.context.formatting() =>
			{
				Some(Operation(Op::Print { value: 0 }, 1))
			}
			crate::hash_in!(crate, ::std::io::print)
				if self.context.stdio_print() && self.context.formatting() =>
			{
				Some(Intrinsic(code::PRINT_RAW, 1))
			}
			crate::hash_in!(crate, ::std::panic) if self.context.panic() => {
				Some(Operation(Op::Panic { value: 0 }, 1))
			}
			crate::hash_in!(crate, ::std::mem::drop) if self.context.has_default_modules() => {
				Some(Operation(Op::Drop { value: 0 }, 1))
			}
			crate::hash_in!(crate, ::std::clone::clone) if self.context.has_default_modules() => {
				Some(Operation(
					Op::Clone {
						out: 1,
						value: 0,
						mode: CloneMode::Global,
					},
					1,
				))
			}
			crate::hash_in!(crate, ::std::string::String::from)
			| crate::hash_in!(crate, ::std::string::String.from)
				if self.context.string_from() =>
			{
				Some(Operation(Op::StringFrom { out: 1, value: 0 }, 1))
			}
			crate::hash_in!(crate, ::std::option::Option::Some)
				if self.context.has_default_modules() =>
			{
				Some(Operation(
					Op::Variant {
						out: 1,
						tag: Type::Option,
						success: true,
						payload: Some(0),
					},
					1,
				))
			}
			crate::hash_in!(crate, ::std::result::Result::Ok)
			| crate::hash_in!(crate, ::std::result::Result::Err)
				if self.context.has_default_modules() =>
			{
				Some(Operation(
					Op::Variant {
						out: 1,
						tag: Type::Result,
						success: hash == crate::hash_in!(crate, ::std::result::Result::Ok),
						payload: Some(0),
					},
					1,
				))
			}
			_ => None,
		};
		if let Some(body) = builtin {
			let string_arg = matches!(
				hash,
				crate::hash_in!(crate, ::std::io::println)
					| crate::hash_in!(crate, ::std::io::print)
					| crate::hash_in!(crate, ::std::panic)
					| crate::hash_in!(crate, ::std::string::String::from)
					| crate::hash_in!(crate, ::std::string::String.from)
			);
			return Ok(Some((body, string_arg.then_some(TypeName::String))));
		}
		if let Some(cap) = vec_constructors::constructor(hash) {
			return Ok(self.context.vec_constructor(cap).then_some((
				Intrinsic(
					if cap {
						code::VEC_WITH_CAPACITY
					} else {
						code::VEC_NEW
					},
					u32::from(cap),
				),
				None,
			)));
		}
		if let Some(op) = string_builder::Operation::constructor(hash) {
			return Ok(self
				.context
				.string_builder(op)
				.then_some((Intrinsic(op.id(), op.arity()), None)));
		}
		if let Some(pair) = numeric::Pair::from_item(hash) {
			return Ok(self.context.numeric_direct(pair).then_some((
				Intrinsic(numeric::Call::direct(pair).id(), pair.method().arity()),
				None,
			)));
		}
		if let Some(pair) = number::Pair::from_item(hash) {
			return Ok((self.context.number_pair(pair)
				&& (pair.method() != number::Method::ToString || self.context.formatting()))
			.then_some((Intrinsic(pair.id(), pair.method().arity()), None)));
		}
		if self.context.any_function(hash) {
			use crate::function::pipeline;
			let body = if hash == pipeline::TYPE_NAME {
				self.check_type_names(span)?;
				Intrinsic(code::ANY_TYPE_NAME, 1)
			} else if hash == pipeline::IS_READABLE {
				Intrinsic(crate::function::variant::Operation::Readable.id(), 1)
			} else if pipeline::any_kind(hash) == Some(true) {
				Body::TypeOf(Type::Struct(self.metatype(span)?))
			} else {
				return Ok(None);
			};
			return Ok(Some((body, None)));
		}
		if hash == crate::function::iter_terminal::RANGE {
			self.transaction.iter_range(self.context)?;
			return Ok(Some((
				Intrinsic(crate::function::iter_terminal::Intrinsic::Range.id(), 2),
				None,
			)));
		}
		if let Some(pair) = numeric::Pair::ALL.into_iter().find(|p| {
			p.method() == numeric::Method::Cmp
				&& (p.associated() == hash || p.item() == hash)
				&& self.context.numeric_direct(*p)
		}) {
			use numeric::Pair::*;
			let tag = match pair {
				I64Cmp => Type::I64,
				U64Cmp => Type::U64,
				F64Cmp => Type::F64,
				StringCmp => Type::String,
				_ => return Ok(None),
			};
			return Ok(Some((Body::TypedCmp(tag), None)));
		}
		if let Some(binding) = self.context.host(hash) {
			use crate::function::host::{HostOutputKind as O, HostType};
			let s = binding.signature;
			if !self.context.host_is_wait_adapter(hash)
				&& !s.arguments.contains(&HostType::OpaqueShared)
				&& !matches!(
					s.output,
					O::OpaqueOwned | O::OpaqueResultString | O::ScalarResultNative(_)
				) {
				let id = self
					.transaction
					.host(binding)
					.map_err(|e| e.at(self.location(span)))?;
				return Ok(Some((Intrinsic(id, u32::from(s.arity)), None)));
			}
			return Ok(None);
		}
		self.associated_host_body(hash, span)
	}

	fn associated_host_body(
		&mut self,
		hash: Hash,
		span: Span,
	) -> Result<Option<(Body, Option<TypeName>)>> {
		use crate::function::{iter::Name, object, text, variant};
		let Some(item) = self
			.context
			.lookup_meta_by_hash(hash)
			.find_map(|m| match m.kind {
				crate::compile::meta::Kind::Function {
					associated: Some(crate::compile::meta::AssociatedKind::Instance(_)),
					..
				} => m.item.as_deref(),
				_ => None,
			})
		else {
			return Ok(None);
		};
		let Some(parent) = item.parent() else {
			return Ok(None);
		};
		let Some(name) = item.last().and_then(|n| n.as_str()) else {
			return Ok(None);
		};
		let key = Hash::ident(name);
		let named_owner = Hash::type_hash(parent);
		use crate::function::iter_terminal::Terminal;
		let Some(ty) = [
			TypeName::Vec,
			TypeName::Tuple,
			TypeName::Range,
			TypeName::RangeFrom,
			TypeName::RangeInclusive,
			TypeName::RangeFull,
			TypeName::RangeTo,
			TypeName::RangeToInclusive,
			TypeName::Function,
			TypeName::Bool,
			TypeName::Char,
			TypeName::Ordering,
			TypeName::Type,
			TypeName::String,
			TypeName::Object,
			TypeName::Option,
			TypeName::Result,
			TypeName::VecIter,
			TypeName::ObjectIter,
			TypeName::OptionIter,
			TypeName::RangeIter,
			TypeName::UnsignedRangeIter,
			TypeName::CharRangeIter,
			TypeName::RangeInclusiveIter,
			TypeName::UnsignedRangeInclusiveIter,
			TypeName::CharRangeInclusiveIter,
			TypeName::RangeFromIter,
			TypeName::UnsignedRangeFromIter,
			TypeName::CharRangeFromIter,
			TypeName::Chars,
			TypeName::StringSplit,
			TypeName::CharSplit,
			TypeName::I64,
			TypeName::U64,
			TypeName::F64,
		]
		.into_iter()
		.find(|t| {
			t.builtin_hash().is_some_and(|owner| {
				(owner == named_owner
					|| self
						.context
						.lookup_meta_by_hash(owner)
						.any(|m| m.item.as_deref() == Some(parent)))
					&& (Hash::associated_function(owner, key) == hash
						|| Terminal::ALL
							.into_iter()
							.any(|term| Hash::associated_function(owner, term.hash()) == hash))
			})
		}) else {
			return Ok(None);
		};
		let owner = ty
			.builtin_hash()
			.ok_or_else(|| Error::new(ErrorKind::InvalidCode, "associated receiver"))?;
		let iterator = matches!(
			ty,
			TypeName::VecIter
				| TypeName::ObjectIter
				| TypeName::OptionIter
				| TypeName::RangeIter
				| TypeName::UnsignedRangeIter
				| TypeName::CharRangeIter
				| TypeName::RangeInclusiveIter
				| TypeName::UnsignedRangeInclusiveIter
				| TypeName::CharRangeInclusiveIter
				| TypeName::RangeFromIter
				| TypeName::UnsignedRangeFromIter
				| TypeName::CharRangeFromIter
				| TypeName::Chars
				| TypeName::CharSplit
				| TypeName::StringSplit
		);
		if iterator {
			use crate::function::iter_terminal::{Closure, Terminal};
			let end = Terminal::ALL
				.into_iter()
				.filter(|t| !matches!(t, Terminal::Skip | Terminal::Take))
				.find(|t| Hash::associated_function(owner, t.hash()) == hash)
				.map(iter_terminals::End::Terminal)
				.or_else(|| {
					Closure::from_hash(key)
						.filter(|c| {
							!c.adapter() && Hash::associated_function(owner, c.hash()) == hash
						})
						.map(iter_terminals::End::Closure)
				});
			if let Some(end) = end {
				let body = self.make_terminal_body(end, span)?;
				return Ok(Some((
					Body::Generated(
						crate::alloc::Box::try_new(body).map_err(crate::alloc::Error::from)?,
					),
					Some(ty),
				)));
			}
		}
		if matches!(ty, TypeName::Option | TypeName::Result)
			&& matches!(name, "unwrap" | "expect")
			&& !self.context.formatting()
		{
			return Ok(None);
		}

		let body = match (ty, name) {
			(TypeName::Vec, "sort")
				if self.context.default_sort(
					self.constants
						.items
						.keys()
						.map(|id| self.pool.item_type_hash(*id)),
				) =>
			{
				let body = code::NativeBody::Intrinsic(
					crate::function::numeric::Call::ProtocolCmp.id(),
					2,
				);
				let at = self.location(span);
				let cmp = (self.resolve)(
					Resolve::NativeMethod {
						hash: crate::runtime::Protocol::CMP.hash,
						body,
					},
					at,
				)?;
				Body::Sort(cmp)
			}
			(
				TypeName::Vec
				| TypeName::Tuple
				| TypeName::Option
				| TypeName::Result
				| TypeName::Range
				| TypeName::RangeFrom
				| TypeName::RangeFull
				| TypeName::RangeInclusive
				| TypeName::RangeTo
				| TypeName::RangeToInclusive,
				"cmp",
			) if self.context.has_default_modules()
				&& self
					.context
					.numeric_method(crate::function::numeric::Method::Cmp) =>
			{
				Body::Intrinsic(code::METHOD_CMP, 2)
			}
			(TypeName::Vec, "sort_by") if self.context.sort_by() => Body::SortBy,
			(TypeName::Vec | TypeName::Object | TypeName::String | TypeName::Tuple, "len")
				if self.context.has_default_modules() =>
			{
				Body::Operation(Op::Len { out: 1, value: 0 }, 1)
			}
			(TypeName::Vec | TypeName::String, "push") if self.context.has_default_modules() => {
				Body::Operation(Op::Push { value: 0, new: 1 }, 2)
			}
			(TypeName::I64 | TypeName::U64 | TypeName::F64, "clone")
				if self.context.has_default_modules() =>
			{
				Body::Operation(
					Op::Clone {
						out: 1,
						value: 0,
						mode: CloneMode::Associated,
					},
					1,
				)
			}
			(_, "clone")
				if crate::function::clone::Provider::ALL
					.into_iter()
					.any(|p| p.receiver() == owner && self.context.clone_provider(p)) =>
			{
				Body::Operation(
					Op::Clone {
						out: 1,
						value: 0,
						mode: CloneMode::Associated,
					},
					1,
				)
			}
			(_, "iter")
				if crate::function::iter::Provider::ALL
					.into_iter()
					.any(|p| p.name() == Name::Iter && p.container() == owner)
					&& self.context.iter_method(Name::Iter) =>
			{
				Body::Intrinsic(code::ITER_METHOD, 1)
			}
			(_, "next") if iterator && self.context.iter_method(Name::Next) => {
				Body::Intrinsic(code::NEXT_METHOD, 1)
			}
			_ => {
				if let Some(callback) =
					variant::Callback::associated(hash).filter(|&c| self.context.result_callback(c))
				{
					let mut cx = self.callable_frame(2)?;
					let out = cx.slot()?;
					cx.result_callback(&[0, 1], callback, out, span)?;
					cx.emit(Op::Return { value: out }, span)?;
					cx.function.slots = cx.high;
					cx.function.locations.fill(code::TRANSPARENT_SORT_LOCATION);
					Body::Generated(
						crate::alloc::Box::try_new(cx.function)
							.map_err(crate::alloc::Error::from)?,
					)
				} else if let Some(m) = crate::function::string_builder::Operation::method(key)
					.filter(|&m| ty == TypeName::String && self.context.string_builder(m))
				{
					Body::Intrinsic(m.id(), m.arity())
				} else if let Some(m) = [
					crate::function::pipeline::Method::Chars,
					crate::function::pipeline::Method::Unwrap,
				]
				.into_iter()
				.find(|&m| {
					m.hash() == key
						&& m.provider(owner).is_some()
						&& self.context.pipeline_method(m)
				}) {
					Body::Intrinsic(m.id(), m.arity() as u32)
				} else if let Some(m) = variant::Method::from_hash(key)
					.filter(|m| m.provider(owner).is_some() && self.context.variant_method(*m))
				{
					Body::Intrinsic(m.id(), m.arity())
				} else if let Some(m) = object::Method::for_receiver(key, owner)
					.filter(|m| self.context.object_method(*m))
				{
					Body::Intrinsic(m.id(), m.arity())
				} else if let Some(m) = text::Method::from_hash(key).filter(|m| {
					ty == TypeName::String
						&& *m != text::Method::Split
						&& self.context.text_method(*m)
				}) {
					Body::Intrinsic(m.id(), m.arity())
				} else {
					return Ok(None);
				}
			}
		};
		Ok(Some((body, Some(ty))))
	}
}

impl<'hir> Lower<'_, 'hir> {
	pub(super) fn callable_frame(&mut self, arity: u32) -> Result<Lower<'_, 'hir>> {
		let mut cx = Lower {
			function: Function::new(arity.max(1), arity)?,
			next: 0,
			high: 0,
			depth: 0,
			variables: Vec::new(),
			moved: Vec::new(),
			dirty: Vec::new(),
			format_specs: Vec::new(),
			text_bindings: Vec::new(),
			adapter_sites: Vec::new(),
			loops: Vec::new(),
			opaque: None,
			self_binding: None,
			body_hash: Hash::EMPTY,
			moved_error: self.moved_error,
			invalid_break: self.invalid_break,
			self_proof: self.self_proof,
			exprs: self.exprs,
			resolve: self.resolve,
			context: self.context,
			constants: self.constants,
			pool: self.pool,
			transaction: self.transaction,
			proofs: self.proofs,
			source: self.source,
			revision: self.revision,
			await_policy: AwaitPolicy::Refuse,
		};
		for _ in 0..arity {
			let slot = cx.slot()?;
			cx.raise(slot);
		}
		Ok(cx)
	}
}
