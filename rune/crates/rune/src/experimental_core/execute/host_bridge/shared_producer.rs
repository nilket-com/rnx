use super::*;
use crate::host::{shared_producer::ProducerReceiver, ColumnSelection, FloatDtype, SelectionError};
use crate::TypeHash;
use std::string::String as Text;
use std::vec::Vec as List;

fn cap() -> Error {
	error(ErrorKind::StoreCapacity)
}

fn moved(arg: u8) -> Error {
	error(ErrorKind::AccessConflict).with_detail(Detail::MovedArgument {
		arg,
		access: super::super::super::error::MovedAccess::Read,
	})
}

// Reserve a conservative upper bound on the bridge's simultaneous live
// scratch. Allocation is done only after its charge has succeeded.
fn charge(credit: &mut usize, bytes: usize) -> Result<()> {
	*credit = credit.checked_sub(bytes).ok_or_else(cap)?;
	Ok(())
}

struct QuotedLen(usize);
impl core::fmt::Write for QuotedLen {
	fn write_str(&mut self, text: &str) -> core::fmt::Result {
		self.0 = self.0.checked_add(text.len()).ok_or(core::fmt::Error)?;
		Ok(())
	}
}

fn copy_name(name: &str) -> Result<Text> {
	let mut copy = Text::new();
	copy.try_reserve_exact(name.len()).map_err(|_| cap())?;
	copy.push_str(name);
	Ok(copy)
}

fn project(
	store: &Store,
	names: Value,
	dtype: &str,
	credit: &mut usize,
) -> Result<ColumnSelection> {
	use core::fmt::Write;
	use SelectionError::*;
	let invalid = |reason| Ok(ColumnSelection(Err(reason)));
	let Some(items) = store.producer_names(names)? else {
		return invalid(NotNames);
	};
	if items.is_empty() || items.len() > 4096 {
		return invalid(Count(items.len()));
	}
	let mut strings = 0usize;
	for &item in items {
		match store.host_tag(item)? {
			(_, false) => return Err(moved(1)),
			(Type::String, true) => (),
			_ => return invalid(NotNames),
		}
		let n = store.text(item)?.len();
		if !(1..=256).contains(&n) {
			return invalid(NameLength(n));
		}
		strings = strings.checked_add(n).ok_or_else(cap)?;
	}
	// Sort borrowed references to find the first repeated name in input order.
	let scratch = items.len() * size_of::<(&str, usize)>();
	charge(credit, scratch)?;
	let mut ordered = List::new();
	ordered.try_reserve_exact(items.len()).map_err(|_| cap())?;
	for (position, &item) in items.iter().enumerate() {
		ordered.push((store.text(item)?, position));
	}
	ordered.sort_unstable();
	let duplicate = ordered
		.windows(2)
		.filter(|p| p[0].0 == p[1].0)
		.map(|p| p[1])
		.min_by_key(|(_, position)| *position)
		.map(|(name, _)| name);
	drop(ordered);
	*credit += scratch;
	if let Some(name) = duplicate {
		charge(credit, name.len())?;
		return invalid(Duplicate(copy_name(name)?));
	}
	let dtype = match dtype {
		"f32" => FloatDtype::F32,
		"f64" => FloatDtype::F64,
		other => {
			let mut size = QuotedLen(0);
			write!(&mut size, "{other:?}").map_err(|_| cap())?;
			charge(credit, size.0)?;
			let mut quoted = Text::new();
			quoted.try_reserve_exact(size.0).map_err(|_| cap())?;
			write!(&mut quoted, "{other:?}").map_err(|_| cap())?;
			return invalid(Dtype(quoted));
		}
	};
	charge(credit, items.len() * size_of::<Text>() + strings)?;
	let mut selected = List::new();
	selected.try_reserve_exact(items.len()).map_err(|_| cap())?;
	for &item in items {
		selected.push(copy_name(store.text(item)?)?);
	}
	Ok(ColumnSelection(Ok((selected, dtype))))
}

// No slot or Store borrow escapes projection. The stock receiver is retained,
// never cloned through the native payload's Clone implementation.
struct Request {
	receiver: stock::Value,
	columns: ColumnSelection,
	reserved: usize,
}

impl Executor {
	#[cold]
	#[inline(never)]
	#[allow(clippy::too_many_arguments)]
	pub(in super::super) fn sliced_shared_producer(
		&mut self,
		store: &mut Store,
		binding: &crate::host::HostBinding,
		function: &super::super::super::code::Function,
		base: usize,
		out: u32,
		start: u32,
		count: u32,
	) -> Result<()> {
		let invalid = || error(ErrorKind::InvalidCode);
		let runtime = self.active_host_runtime.as_ref().ok_or_else(invalid)?;
		if count != 3
			|| !binding.signature.shared_producer()
			|| !runtime
				.function(&binding.hash)
				.is_some_and(|handler| binding.handler.same_allocation(handler))
		{
			return Err(invalid());
		}
		let native = runtime
			.opaque
			.methods
			.get(&binding.hash)
			.ok_or_else(invalid)?;
		let output = runtime
			.opaque
			.producer_outputs
			.get(&binding.hash)
			.ok_or_else(invalid)?;
		if native.hash == output.hash
			|| [native, output].iter().any(|ty| {
				!runtime
					.opaque
					.types
					.get(&ty.hash)
					.is_some_and(|r| r.descriptor.type_id == ty.id)
			}) {
			return Err(invalid());
		}
		let end = start.checked_add(count).ok_or_else(invalid)?;
		let slot = |id: u32| {
			let abs = base.checked_add(id as usize).ok_or_else(invalid)?;
			if id >= function.slots || abs >= self.slots.len() {
				return Err(invalid());
			}
			Ok(abs)
		};
		let destination = slot(out)?;
		let [r, n, t] = function
			.args
			.get(start as usize..end as usize)
			.ok_or_else(invalid)?
		else {
			return Err(invalid());
		};
		let (receiver, names, dtype) = (
			self.slots[slot(*r)?],
			self.slots[slot(*n)?],
			self.slots[slot(*t)?],
		);
		if !store.host_tag(receiver)?.1 {
			return Err(moved(0));
		}
		let receiver = store.host_snapshot(receiver)?;
		if receiver.type_hash() != native.hash {
			return Err(
				error(ErrorKind::Host).with_detail(Detail::HostExpectedType {
					expected: native.hash,
					actual: receiver.type_hash(),
					arg: Some(0),
				}),
			);
		}
		if !store.host_tag(names)?.1 {
			return Err(moved(1));
		}
		let (tag, readable) = store.host_tag(dtype)?;
		if !readable {
			return Err(moved(2));
		}
		if tag != Type::String {
			let actual = match dtype {
				Value::Heap(_) if matches!(tag, Type::Host(_)) => {
					store.host_snapshot(dtype)?.type_hash()
				}
				Value::Heap(_) => witness(tag)?.type_hash(),
				Value::Function { .. } => stock::Function::HASH,
				Value::Type(_) => stock::Type::HASH,
				_ => snapshot(store, dtype)?.type_hash(),
			};
			return Err(
				error(ErrorKind::Host).with_detail(Detail::HostExpectedType {
					expected: crate::alloc::String::HASH,
					actual,
					arg: Some(2),
				}),
			);
		}
		let initial = store.host_remaining_bytes()?;
		let mut available = initial;
		// Two native argument boxes and their headers, plus the native/result
		// Store wrappers. No general-purpose tree is traversed or snapshotted.
		charge(
			&mut available,
			stock::AnyObj::owned_allocation_bytes::<ProducerReceiver>()
				+ stock::AnyObj::owned_allocation_bytes::<ColumnSelection>()
				+ store.producer_wrapper_bytes(),
		)?;
		let columns = project(store, names, store.text(dtype)?, &mut available)?;
		let request = Request {
			receiver,
			columns,
			reserved: initial - available,
		};
		let value = self.materialize_shared_producer(store, binding, request)?;
		self.put(store, destination, value)
	}

	#[cold]
	fn materialize_shared_producer(
		&mut self,
		store: &mut Store,
		binding: &crate::host::HostBinding,
		request: Request,
	) -> Result<Value> {
		let remaining = store
			.host_remaining_bytes()?
			.checked_sub(request.reserved)
			.ok_or_else(cap)?;
		let mut scratch = [
			stock::Value::new(ProducerReceiver {
				receiver: request.receiver,
				remaining,
			})?,
			stock::Value::new(request.columns)?,
			stock::Value::unit(),
			stock::Value::empty(),
		];
		match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			binding.handler.call(
				&mut scratch,
				stock::Address::new(0),
				3,
				stock::Output::keep(3),
			)
		})) {
			Ok(Ok(())) => (),
			Ok(Err(native)) => return Err(self.host_error(native)),
			Err(payload) => {
				let text = payload.downcast_ref::<&str>().copied().or_else(|| {
					payload
						.downcast_ref::<std::string::String>()
						.map(|s| s.as_str())
				});
				self.pending_host_message =
					text.and_then(|s| owned_message(&s, self.format_limits.bytes));
				return Err(error(ErrorKind::Host).with_detail(Detail::HostPanicked));
			}
		}
		let output = core::mem::replace(&mut scratch[3], stock::Value::empty());
		drop(scratch);
		self.host_opaque_result(store, &output)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate as rune;
	use crate::experimental_core::{code::Function, heap::Limits};
	use crate::{Context, Hash, Module};
	use std::sync::{
		atomic::{AtomicUsize, Ordering::SeqCst},
		Arc,
	};
	#[derive(crate::Any)]
	#[rune(item = ::matrix_fixture)]
	struct Source;
	#[derive(crate::Any)]
	#[rune(item = ::matrix_fixture)]
	struct Matrix;
	const BYTES: usize = 4096;
	fn context(calls: Arc<AtomicUsize>) -> Context {
		let mut m = Module::with_crate("matrix_fixture").unwrap();
		m.opaque_type::<Source>(|_| 0, |_, _| Ok(())).unwrap();
		m.opaque_type::<Matrix>(|_| BYTES, |_, _| Ok(())).unwrap();
		m.opaque_columns_producer(
			"matrix",
			|_: &Source, _: stock::Value, _: &str| -> core::result::Result<Matrix, Text> {
				panic!("legacy")
			},
			move |_: &Source, columns, credit| {
				assert!(columns.0.is_ok());
				if credit < BYTES {
					return Err(crate::host::OpaqueProducerLimit);
				}
				calls.fetch_add(1, SeqCst);
				Ok(Ok(Matrix))
			},
		)
		.unwrap();
		super::super::minimal_tests::installed(m)
	}
	#[test]
	fn a_late_output_registration_must_match_the_rust_type() {
		#[derive(crate::Any)]
		#[rune(item = ::matrix_fixture, name = Matrix)]
		struct OtherMatrix;
		let mut modules = Module::with_crate("matrix_fixture")
			.unwrap()
			.with_unique("producer");
		modules.opaque_type::<Source>(|_| 0, |_, _| Ok(())).unwrap();
		modules
			.opaque_columns_producer(
				"matrix",
				|_: &Source, _: stock::Value, _: &str| -> core::result::Result<Matrix, Text> {
					Ok(Matrix)
				},
				|_: &Source, _, _| Ok(Ok(Matrix)),
			)
			.unwrap();
		let wrong = || {
			let mut module = Module::with_crate("matrix_fixture")
				.unwrap()
				.with_unique("output");
			module
				.opaque_type::<OtherMatrix>(|_| 0, |_, _| Ok(()))
				.unwrap();
			module.host_function("sentinel", || 3i64).unwrap();
			module
		};
		let mut right = Module::with_crate("matrix_fixture")
			.unwrap()
			.with_unique("output");
		right
			.opaque_type::<Matrix>(|_| BYTES, |_, _| Ok(()))
			.unwrap();
		let key = Hash::associated_function(Source::HASH, Hash::ident("matrix"));
		let sentinel = crate::hash!(::matrix_fixture::sentinel);

		// Producer first. A rejected output module must leave even its
		// unrelated function and unique ID available for a corrected install.
		let mut ctx = Context::new();
		ctx.install(&modules).unwrap();
		assert!(matches!(
			ctx.install(wrong()),
			Err(crate::ContextError::OpaqueType { .. })
		));
		assert!(!ctx.opaque.types.contains_key(&Matrix::HASH));
		assert!(ctx.lookup_function(sentinel).is_none());
		ctx.install(&right).unwrap();
		assert!(ctx.opaque.types.contains_key(&Matrix::HASH));

		// Output first. A conflicting producer cannot install the receiver or
		// its method, or prevent a valid module from claiming the producer ID.
		let mut ctx = Context::new();
		ctx.install(wrong()).unwrap();
		assert!(matches!(
			ctx.install(&modules),
			Err(crate::ContextError::OpaqueType { .. })
		));
		assert!(ctx.host(key).is_none());
		assert!(!ctx.opaque.types.contains_key(&Source::HASH));
		assert!(ctx.lookup_function(sentinel).is_some());
		let mut replacement = Module::with_crate("matrix_fixture")
			.unwrap()
			.with_unique("producer");
		replacement
			.opaque_type::<Source>(|_| 0, |_, _| Ok(()))
			.unwrap();
		ctx.install(replacement).unwrap();
		assert!(ctx.opaque.types.contains_key(&Source::HASH));

		let mut ctx = Context::new();
		ctx.install(&right).unwrap();
		ctx.install(&modules).unwrap();
		assert!(ctx.host(key).is_some());

		// A module that brings both descriptors cannot mask an already
		// registered, incompatible output while constructing its merged view.
		let mut ctx = Context::new();
		ctx.install(wrong()).unwrap();
		modules
			.opaque_type::<Matrix>(|_| BYTES, |_, _| Ok(()))
			.unwrap();
		assert!(matches!(
			ctx.install(modules),
			Err(crate::ContextError::OpaqueType { .. })
		));
		assert!(!ctx.opaque.types.contains_key(&Source::HASH));
		assert!(ctx.host(key).is_none());
	}
	#[test]
	fn authenticates_the_handler_and_accounts_for_retained_results() {
		let calls = Arc::new(AtomicUsize::new(0));
		let c = context(calls.clone());
		let foreign = context(calls.clone());
		let mut store = Store::new(Limits {
			bytes: BYTES + 1024,
			..Limits::default()
		})
		.unwrap();
		let mut vm = Executor::new().unwrap();
		vm.active_host_runtime = Some(c.execution_runtime().unwrap().inner);
		let desc = c
			.opaque
			.types
			.get(&Source::HASH)
			.unwrap()
			.descriptor
			.clone();
		let source = store
			.host_value(&stock::Value::new(Source).unwrap(), desc)
			.unwrap();
		let name = store.string("x").unwrap();
		let names = store.list(Type::Vec, &[name]).unwrap();
		let dtype = store.string("f32").unwrap();
		for slot in [source, names, dtype, Value::Unit] {
			vm.slots.try_push(slot).unwrap();
		}
		let mut f = Function::new(4, 3).unwrap();
		let (start, n) = f.arguments(&[0, 1, 2]).unwrap();
		let key = Hash::associated_function(Source::HASH, Hash::ident("matrix"));
		let invoke = |vm: &mut Executor, store: &mut Store, ctx: &Context| {
			vm.sliced_shared_producer(store, ctx.host(key).unwrap(), &f, 0, 3, start, n)
		};
		assert_eq!(
			invoke(&mut vm, &mut store, &foreign).unwrap_err().kind,
			ErrorKind::InvalidCode
		);
		assert_eq!(calls.load(SeqCst), 0);
		let base = store.payload_bytes();
		invoke(&mut vm, &mut store, &c).unwrap();
		let charged = store.payload_bytes() - base;
		assert_eq!(charged, BYTES + store.producer_wrapper_bytes());
		assert_eq!(
			invoke(&mut vm, &mut store, &c).unwrap_err().kind,
			ErrorKind::StoreCapacity
		);
		assert_eq!(calls.load(SeqCst), 1);
		vm.put(&mut store, 3, Value::Unit).unwrap();
		assert_eq!(store.payload_bytes(), base);
		invoke(&mut vm, &mut store, &c).unwrap();
		assert_eq!(calls.load(SeqCst), 2);
	}
	#[test]
	fn oversized_rejected_arguments_require_no_copy_credit() {
		let mut store = Store::new(Limits::default()).unwrap();
		let giant = store.string(&"x".repeat(4096)).unwrap();
		let names = store.list(Type::Vec, &[giant]).unwrap();
		assert!(matches!(
			project(&store, names, "f64", &mut 0).unwrap().0,
			Err(SelectionError::NameLength(4096))
		));
		let many = store.list(Type::Vec, &vec![Value::Unit; 4097]).unwrap();
		assert!(matches!(
			project(&store, many, "f64", &mut 0).unwrap().0,
			Err(SelectionError::Count(4097))
		));
		let x = store.string("a").unwrap();
		let valid = store.list(Type::Vec, &[x]).unwrap();
		assert_eq!(
			project(&store, valid, "f32", &mut 0).unwrap_err().kind,
			ErrorKind::StoreCapacity
		);
	}
}
