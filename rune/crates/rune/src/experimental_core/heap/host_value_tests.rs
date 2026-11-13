//! Minimal native ownership/bridge fixtures; one Code build per case, no compiler.
use super::*;
use crate as rune;
use crate::experimental_core::{
	arithmetic::{Binary, Unary},
	code::{Builder, CloneMode, Code, Function, Op},
	execute::Executor,
};
use crate::runtime::{Formatter, VmError};
use crate::{Context, Hash, Module, TypeHash};
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc as Shared,
};

#[derive(crate::Any)]
#[rune(item = ::opaque_test)]
struct Tracked {
	drops: Shared<AtomicUsize>,
	panic_drop: bool,
}
impl Drop for Tracked {
	fn drop(&mut self) {
		self.drops.fetch_add(1, Ordering::SeqCst);
		if self.panic_drop {
			panic!("native destructor witness");
		}
	}
}
#[derive(crate::Any)]
#[rune(item = ::opaque_test)]
struct Other;
fn display(_: &Tracked, f: &mut Formatter) -> core::result::Result<(), VmError> {
	use crate::alloc::fmt::TryWrite;
	f.try_write_str("tracked")?;
	Ok(())
}
fn other_display(_: &Other, f: &mut Formatter) -> core::result::Result<(), VmError> {
	use crate::alloc::fmt::TryWrite;
	f.try_write_str("other")?;
	Ok(())
}
fn read(_: &Tracked) -> i64 {
	7
}
fn read_other(_: &Other) -> i64 {
	9
}
fn shape(_: &Tracked) -> (i64, i64) {
	(2, 3)
}
fn names(_: &Tracked) -> std::vec::Vec<std::string::String> {
	std::vec![
		std::string::String::from("x"),
		std::string::String::from("y")
	]
}
fn host_panic(_: &Tracked) -> i64 {
	panic!("native method witness")
}
fn at() -> super::super::error::Location {
	super::super::error::Location {
		source: 0,
		revision: 1,
		start: 0,
		end: 1,
	}
}
struct Fixture {
	context: Context,
	code: Code,
	executor: Executor,
	drops: Shared<AtomicUsize>,
	made: Shared<AtomicUsize>,
}
fn fixture(panic_drop: bool, failing_entry: bool) -> Fixture {
	fixture_op(panic_drop, failing_entry, None)
}
fn fixture_op(panic_drop: bool, failing_entry: bool, backstop: Option<Op>) -> Fixture {
	let drops = Shared::new(AtomicUsize::new(0));
	let captured = drops.clone();
	let made = Shared::new(AtomicUsize::new(0));
	let observed = made.clone();
	let mut module = Module::with_crate("opaque_test").unwrap();
	module.opaque_type::<Tracked>(|_| 64, display).unwrap();
	module.opaque_type::<Other>(|_| 1, other_display).unwrap();
	module
		.opaque_method::<Tracked, _, _>("read", read, None)
		.unwrap();
	module
		.opaque_method::<Other, _, _>("read", read_other, None)
		.unwrap();
	module
		.opaque_method::<Tracked, _, _>("panic", host_panic, None)
		.unwrap();
	module
		.opaque_method::<Tracked, _, _>(
			"shape",
			shape,
			Some(crate::host::OutputCertificate::I64_PAIR),
		)
		.unwrap();
	module
		.opaque_method::<Tracked, _, _>(
			"names",
			names,
			Some(crate::host::OutputCertificate::STRING_LIST),
		)
		.unwrap();
	let mut context = Context::new();
	context.install(module).unwrap();
	let mut factories = Module::with_crate("rnx_test").unwrap();
	factories
		.opaque_factory::<Tracked, _, _>("make", move || {
			observed.fetch_add(1, Ordering::SeqCst);
			Tracked {
				drops: captured.clone(),
				panic_drop,
			}
		})
		.unwrap();
	factories
		.opaque_factory::<Other, _, _>("other", || Other)
		.unwrap();
	context.install(factories).unwrap();
	let keys = [
		Hash::type_hash(crate::item!(::rnx_test::make)),
		Hash::associated_function(Tracked::HASH, Hash::ident("read")),
		Hash::type_hash(crate::item!(::rnx_test::other)),
		Hash::associated_function(Other::HASH, Hash::ident("read")),
		Hash::associated_function(Tracked::HASH, Hash::ident("panic")),
		Hash::associated_function(Tracked::HASH, Hash::ident("shape")),
		Hash::associated_function(Tracked::HASH, Hash::ident("names")),
	];
	let mut builder = Builder::default();
	for (n, key) in keys.into_iter().enumerate() {
		let id = builder.host(context.host(key).unwrap()).unwrap();
		let parameters = u32::from(matches!(n, 1 | 3 | 4 | 5 | 6));
		let mut function = Function::new(4, parameters).unwrap();
		let ids: std::vec::Vec<_> = (0..parameters).collect();
		let (args_start, args_len) = function.arguments(&ids).unwrap();
		function
			.push(
				Op::Call {
					out: parameters,
					function: id,
					args_start,
					args_len,
				},
				at(),
			)
			.unwrap();
		if n == 0 && failing_entry {
			function
				.push(
					Op::Literal {
						out: 1,
						value: Value::I64(1),
					},
					at(),
				)
				.unwrap();
			function
				.push(
					Op::Literal {
						out: 2,
						value: Value::I64(0),
					},
					at(),
				)
				.unwrap();
			function
				.push(
					Op::Binary {
						out: 3,
						op: Binary::Div,
						lhs: 1,
						rhs: 2,
					},
					at(),
				)
				.unwrap();
		}
		function
			.push(Op::Return { value: parameters }, at())
			.unwrap();
		builder.push(function).unwrap();
	}
	if let Some(op) = backstop {
		let mut function = Function::new(4, 1).unwrap();
		if matches!(op, Op::Template { .. }) {
			function.arguments(&[0, 0]).unwrap();
		}
		function
			.strings
			.try_push(String::try_from("field").unwrap())
			.unwrap();
		function
			.push(
				Op::Literal {
					out: 2,
					value: Value::I64(0),
				},
				at(),
			)
			.unwrap();
		function.push(op, at()).unwrap();
		function.push(Op::Return { value: 1 }, at()).unwrap();
		builder.push(function).unwrap();
	}
	// A second charged point immediately after the factory lets cancellation
	// observe ownership without executing a return or an unrelated error.
	let factory = builder
		.host(
			context
				.host(Hash::type_hash(crate::item!(::rnx_test::make)))
				.unwrap(),
		)
		.unwrap();
	let mut looping = Function::new(1, 0).unwrap();
	looping
		.push(
			Op::Call {
				out: 0,
				function: factory,
				args_start: 0,
				args_len: 0,
			},
			at(),
		)
		.unwrap();
	looping.push(Op::Jump { target: 1 }, at()).unwrap();
	builder.push(looping).unwrap();
	let code = builder.finish().unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	Fixture {
		context,
		code,
		executor,
		drops,
		made,
	}
}
fn store() -> Store {
	Store::new(Limits::default()).unwrap()
}
fn make(f: &mut Fixture, s: &mut Store, entry: u32) -> crate::experimental_core::value::Root {
	f.executor
		.invoke_code(&f.code, s, entry, &[], 1000)
		.unwrap()
}

#[test]
fn native_layout_does_not_grow() {
	assert_eq!(size_of::<Value>(), 16);
	assert_eq!(size_of::<Entry>(), 72);
	assert_eq!(size_of::<Payload>(), 40);
	assert_eq!(size_of::<Slot>(), 80);
	eprintln!("layout Value=16 Entry=72 Payload=40 Slot=80");
}
#[test]
fn shared_receiver_aliases_and_last_release() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	let alias = s.root_retain(&root).unwrap();
	for _ in 0..3 {
		let output = f
			.executor
			.invoke_code(&f.code, &mut s, 1, &[&root], 1000)
			.unwrap();
		assert_eq!(output.value, Value::I64(7));
		s.root_release(output).unwrap();
	}
	assert_eq!(f.drops.load(Ordering::SeqCst), 0);
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 0);
	s.root_release(alias).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
}
#[test]
fn different_native_receivers_name_both_types() {
	let mut f = fixture(false, false);
	let mut s = store();
	let a = make(&mut f, &mut s, 0);
	let b = make(&mut f, &mut s, 2);
	for (entry, root) in [(1, &b), (3, &a)] {
		let failure = f
			.executor
			.invoke_code(&f.code, &mut s, entry, &[root], 1000)
			.unwrap_err();
		let text = failure.host_message.as_deref().unwrap();
		assert!(text.contains("Tracked") && text.contains("Other"), "{text}");
	}
	s.root_release(a).unwrap();
	s.root_release(b).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}
#[test]
fn explicit_drop_has_a_truthful_moved_receiver() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	s.invalidate(root.value).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 1, &[&root], 1000)
		.unwrap_err();
	assert!(failure.host_message.as_deref().unwrap().contains("moved"));
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}
#[test]
fn failed_output_admission_drops_once() {
	let mut f = fixture(false, false);
	let mut s = Store::new(Limits {
		entries: 8,
		bytes: 1,
	})
	.unwrap();
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 0, &[], 1000)
		.unwrap_err();
	assert_eq!(failure.error.kind, ErrorKind::StoreCapacity);
	assert!(failure.cleanup_error.is_none());
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
}
#[test]
fn output_refusal_keeps_destructor_panic_secondary() {
	let mut f = fixture(true, false);
	let mut s = Store::new(Limits {
		entries: 8,
		bytes: 1,
	})
	.unwrap();
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 0, &[], 1000)
		.unwrap_err();
	assert_eq!(failure.error.kind, ErrorKind::StoreCapacity);
	assert_eq!(failure.cleanup_error.unwrap().kind, ErrorKind::Host);
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!(s.live_entries(), 0);
}
#[test]
fn runtime_error_releases_the_created_object() {
	let mut f = fixture(false, true);
	let mut s = store();
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 0, &[], 1000)
		.unwrap_err();
	assert_eq!(failure.error.kind, ErrorKind::DivisionByZero);
	assert!(failure.cleanup_error.is_none());
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
}
#[test]
fn runtime_error_keeps_destructor_panic_secondary() {
	let mut f = fixture(true, true);
	let mut s = store();
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 0, &[], 1000)
		.unwrap_err();
	assert_eq!(failure.error.kind, ErrorKind::DivisionByZero);
	assert_eq!(failure.cleanup_error.unwrap().kind, ErrorKind::Host);
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
}
#[test]
fn close_keeps_draining_after_native_destructor_panics() {
	let mut f = fixture(true, false);
	let mut s = store();
	let _a = make(&mut f, &mut s, 0);
	let _b = make(&mut f, &mut s, 0);
	assert_eq!(s.close().unwrap_err().kind, ErrorKind::Host);
	assert_eq!(f.drops.load(Ordering::SeqCst), 2);
	assert!(s.is_closed());
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
	s.close().unwrap();
}
#[test]
fn native_panic_releases_shared_guard_and_preserves_receiver() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 4, &[&root], 1000)
		.unwrap_err();
	assert_eq!(failure.error.kind, ErrorKind::Host);
	let output = f
		.executor
		.invoke_code(&f.code, &mut s, 1, &[&root], 1000)
		.unwrap();
	assert_eq!(output.value, Value::I64(7));
	s.root_release(output).unwrap();
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}
#[test]
fn native_view_is_borrowed_and_display_is_bounded() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	let view = s.root_view(&root, ViewLimits::default()).unwrap();
	let node = view.root().unwrap();
	assert!(node.borrow_host::<Tracked>().is_ok());
	assert_eq!(
		node.borrow_host::<Other>().err().unwrap().kind,
		ErrorKind::Type
	);
	assert_eq!(node.display_host(7).unwrap().as_str(), "tracked");
	assert_eq!(
		node.display_host(6).unwrap_err().kind,
		ErrorKind::FormatLimit
	);
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}
#[test]
#[cfg_attr(
	miri,
	ignore = "native frontend ingress controls; ownership Miri uses minimal Code fixtures"
)]
fn ordinary_frontend_refuses_factories_and_function_value_ingress() {
	let f = fixture(false, false);
	for body in [
		"rnx_test::make()",
		"rnx_test::make",
		"let f=rnx_test::make;f()",
		"[rnx_test::make][0]()",
		"#{f:rnx_test::make}.f()",
		"opaque_test::Tracked",
	] {
		let mut sources = crate::Sources::new();
		sources
			.insert(crate::Source::memory(format!("pub fn main(){{{body}}}")).unwrap())
			.unwrap();
		let result = crate::experimental_core::frontend::compile(
			&mut sources,
			&f.context,
			&mut crate::Diagnostics::new(),
			1,
		);
		assert_eq!(
			result.err().unwrap().refusal,
			Some(crate::experimental_core::error::Refusal::OpaqueEntry),
			"{body}"
		);
	}
	assert_eq!(f.made.load(Ordering::SeqCst), 0);
}

macro_rules! backstop {
	($name:ident, $op:expr) => {
		backstop!($name, $op, ErrorKind::Type);
	};
	($name:ident, $op:expr, $kind:expr) => {
		#[test]
		fn $name() {
			let mut f = fixture_op(false, false, Some($op));
			let mut s = store();
			let root = make(&mut f, &mut s, 0);
			let failure = f
				.executor
				.invoke_code(&f.code, &mut s, 7, &[&root], 1000)
				.unwrap_err();
			assert_eq!(failure.error.kind, $kind, "{:?}", failure.error);
			assert_eq!(f.drops.load(Ordering::SeqCst), 0);
			s.root_release(root).unwrap();
			assert_eq!(f.drops.load(Ordering::SeqCst), 1);
			assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
		}
	};
}
backstop!(
	backstop_clone,
	Op::Clone {
		out: 1,
		value: 0,
		mode: CloneMode::Global
	}
);
backstop!(
	backstop_equality,
	Op::Binary {
		out: 1,
		op: Binary::Eq,
		lhs: 0,
		rhs: 0
	}
);
backstop!(
	backstop_arithmetic,
	Op::Binary {
		out: 1,
		op: Binary::Add,
		lhs: 0,
		rhs: 2
	}
);
backstop!(
	backstop_unary,
	Op::Unary {
		out: 1,
		op: Unary::Neg,
		value: 0
	}
);
backstop!(
	backstop_cast,
	Op::Cast {
		out: 1,
		value: 0,
		tag: Type::I64
	},
	ErrorKind::InvalidCast
);
backstop!(
	backstop_index,
	Op::Get {
		out: 1,
		value: 0,
		index: 2
	}
);
backstop!(
	backstop_index_write,
	Op::Set {
		value: 0,
		index: 2,
		new: 2
	}
);
backstop!(
	backstop_field,
	Op::Field {
		out: 1,
		value: 0,
		key: 0
	}
);
backstop!(
	backstop_field_write,
	Op::SetField {
		value: 0,
		key: 0,
		new: 2
	}
);
backstop!(
	backstop_tuple_field,
	Op::TupleField {
		out: 1,
		value: 0,
		index: 0
	}
);
backstop!(backstop_iteration, Op::IterStart { out: 1, value: 0 });
backstop!(
	backstop_callable,
	Op::CallValue {
		out: 1,
		value: 0,
		args_start: 0,
		args_len: 0
	}
);
backstop!(backstop_try, Op::Try { out: 1, value: 0 });
backstop!(
	backstop_condition,
	Op::Branch {
		condition: 0,
		yes: 2,
		no: 2
	}
);
backstop!(backstop_len, Op::Len { out: 1, value: 0 });
backstop!(backstop_push, Op::Push { value: 0, new: 2 });

backstop!(
	backstop_template,
	Op::Template {
		out: 1,
		args_start: 0,
		args_len: 2
	}
);

#[test]
fn internal_session_envelope_retains_opaque_edges() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	let envelope = s.root_record(None, &[("value", &root)]).unwrap();
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 0);
	let value = s.root_field(&envelope, "value").unwrap();
	s.root_release(envelope).unwrap();
	let output = f
		.executor
		.invoke_code(&f.code, &mut s, 1, &[&value], 1000)
		.unwrap();
	assert_eq!(output.value, Value::I64(7));
	s.root_release(output).unwrap();
	s.root_release(value).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}

#[test]
fn allocation_sweep_never_loses_a_native_output() {
	let mut f = fixture(false, false);
	let mut s = store();
	let primed = make(&mut f, &mut s, 0);
	s.root_release(primed).unwrap();
	let mut store_refusal = false;
	let mut host_refusal = false;
	for budget in (0..512).step_by(8) {
		let result = rune_alloc::limit::with(budget, || {
			f.executor
				.invoke_code(&f.code, &mut s, 0, &[], 1000)
				.map_err(|failure| (failure.error.kind, failure.cleanup_error.is_none()))
		})
		.call();
		match result {
			Ok(root) => s.root_release(root).unwrap(),
			Err((kind, cleanup_ok)) => {
				assert!(cleanup_ok, "{kind:?}");
				store_refusal |= kind == ErrorKind::Allocation;
				host_refusal |= kind == ErrorKind::Host;
			}
		}
		assert_eq!(
			f.drops.load(Ordering::SeqCst),
			f.made.load(Ordering::SeqCst),
			"budget {budget}"
		);
		assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
		assert!(f.executor.is_idle());
	}
	assert!(store_refusal && host_refusal);
}
#[test]
fn cancellation_after_factory_releases_once() {
	use crate::experimental_core::{execute::SliceOutcome, frontend::Program};
	let Fixture {
		context,
		code,
		mut executor,
		drops,
		..
	} = fixture(false, true);
	let mut program = Program::for_diagnostics_test(code);
	program.formatting =
		crate::experimental_core::frontend::FormatAuthority::new(context.format_snapshot(), false);
	let mut s = store();
	let token = executor.prepare(&program, &mut s, 7, &[]).unwrap();
	let mut invocation = token.start_sliced(1000).unwrap();
	assert!(matches!(
		invocation.resume(core::num::NonZeroUsize::new(1).unwrap()),
		SliceOutcome::Limited
	));
	assert_eq!(drops.load(Ordering::SeqCst), 0);
	let failure = invocation.cancel();
	assert_eq!(failure.error.kind, ErrorKind::Cancelled);
	assert!(failure.cleanup_error.is_none());
	drop(invocation);
	assert_eq!(drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
}
#[test]
fn foreign_store_refuses_before_native_borrow() {
	let mut f = fixture(false, false);
	let mut a = store();
	let root = make(&mut f, &mut a, 0);
	let mut b = store();
	let failure = f
		.executor
		.invoke_code(&f.code, &mut b, 1, &[&root], 1000)
		.unwrap_err();
	assert_eq!(failure.error.kind, ErrorKind::InvalidHandle);
	assert_eq!(f.drops.load(Ordering::SeqCst), 0);
	a.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}
#[derive(crate::Any)]
#[rune(item = ::opaque_test, name = Tracked)]
struct HashTwin;
#[test]
fn a_rune_hash_collision_never_authorizes_a_native_cast() {
	assert_eq!(Tracked::HASH, HashTwin::HASH);
	let descriptor =
		crate::function::host::opaque::Descriptor::new::<Tracked>(|_| 0, display).unwrap();
	let twin = crate::Value::new(HashTwin).unwrap();
	assert!(descriptor.inspect.call(&twin).is_err());
	let mut module = Module::with_crate("opaque_test").unwrap();
	module.opaque_type::<Tracked>(|_| 64, display).unwrap();
	assert!(module
		.opaque_factory::<HashTwin, _, _>("wrong", || HashTwin)
		.is_err());
}

#[test]
fn exclusive_native_borrow_blocks_a_shared_method() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	let native = s.host_snapshot(root.value).unwrap();
	let lease = native.borrow_mut::<Tracked>().unwrap();
	let failure = f
		.executor
		.invoke_code(&f.code, &mut s, 1, &[&root], 1000)
		.unwrap_err();
	assert!(failure
		.host_message
		.as_deref()
		.unwrap()
		.contains("Cannot read"));
	assert_eq!(f.drops.load(Ordering::SeqCst), 0);
	drop(lease);
	drop(native);
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}
#[test]
fn a_factory_without_a_descriptor_cannot_install() {
	let mut module = Module::with_crate("missing_descriptor").unwrap();
	module
		.opaque_factory::<Other, _, _>("make", || Other)
		.unwrap();
	let mut context = Context::new();
	let message = format!("{}", context.install(module).unwrap_err());
	assert!(
		message.contains("Other") && message.contains("enrolled descriptor"),
		"{message}"
	);
}

#[test]
fn certified_shape_and_names_import_builtin_results() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	let shape = f
		.executor
		.invoke_code(&f.code, &mut s, 5, &[&root], 1000)
		.unwrap();
	assert_eq!(s.root_tag(&shape).unwrap(), Type::Tuple);
	assert_eq!(s.root_len(&shape).unwrap(), 2);
	for (n, expected) in [(0, Value::I64(2)), (1, Value::I64(3))] {
		let child = s.root_get(&shape, n).unwrap();
		assert_eq!(child.value, expected);
		s.root_release(child).unwrap();
	}
	let names = f
		.executor
		.invoke_code(&f.code, &mut s, 6, &[&root], 1000)
		.unwrap();
	assert_eq!(s.root_tag(&names).unwrap(), Type::Vec);
	assert_eq!(s.root_len(&names).unwrap(), 2);
	for (n, expected) in [(0, "x"), (1, "y")] {
		let child = s.root_get(&names, n).unwrap();
		assert_eq!(s.root_text(&child).unwrap(), expected);
		s.root_release(child).unwrap();
	}
	s.root_release(shape).unwrap();
	s.root_release(names).unwrap();
	s.root_release(root).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
}

#[test]
fn builtin_and_unrepresented_hashes_cannot_gain_an_opaque_representation() {
	let mut module = Module::with_crate("builtin_shadow").unwrap();
	module.opaque_type::<Tracked>(|_| 8, display).unwrap();
	let registered = module.opaque.types.remove(&Tracked::HASH).unwrap();
	module
		.opaque
		.types
		.try_insert(i64::HASH, registered)
		.unwrap();
	let failure = module.opaque.validate().unwrap_err();
	assert!(matches!(failure, crate::ContextError::OpaqueBuiltin { .. }));
	assert!(crate::function::host::opaque::reserved(
		crate::runtime::Bytes::HASH
	));
	assert!(crate::function::host::opaque::reserved(
		crate::modules::collections::HashMap::HASH
	));
	assert!(crate::function::host::opaque::reserved(
		crate::modules::collections::HashSet::HASH
	));
}

#[test]
fn debug_root_format_is_a_named_refusal() {
	let mut f = fixture(false, false);
	let mut s = store();
	let root = make(&mut f, &mut s, 0);
	assert_eq!(
		s.root_format(&root, true).unwrap_err().refusal,
		Some(super::super::error::Refusal::OpaqueDebug)
	);
	assert_eq!(
		s.root_format_with_limits(&root, true, &Default::default())
			.unwrap_err()
			.refusal,
		Some(super::super::error::Refusal::OpaqueDebug)
	);
	s.root_release(root).unwrap();
}
#[test]
fn ordinary_associated_registrations_remain_unenrolled() {
	let mut m = Module::with_crate("ordinary").unwrap();
	m.ty::<Other>().unwrap();
	m.associated_function("read", read_other).unwrap();
	for a in &m.associated {
		if let crate::module::ModuleAssociatedKind::Function(f) = &a.kind {
			assert!(f.host.is_none());
		}
	}
	let mut c = Context::new();
	c.install(m).unwrap();
	assert!(c
		.host(Hash::associated_function(Other::HASH, Hash::ident("read")))
		.is_none());
}

#[test]
fn restored_host_identity_cannot_be_claimed_plain_or_hidden_in_a_container() {
	let mut f = fixture(false, false);
	let mut s = store();
	let host = make(&mut f, &mut s, 0);
	let identity = s.session_identity(&host).unwrap();
	assert!(identity.is_some());
	let good = s.root_record(None, &[("x", &host)]).unwrap();
	let plain = s.root_i64(7);
	let bad = s.root_record(None, &[("x", &plain)]).unwrap();
	let expected = [(crate::alloc::String::try_from("x").unwrap(), identity)];
	s.validate_session_record(&good, &expected).unwrap();
	assert_eq!(
		s.validate_session_record(&bad, &expected)
			.unwrap_err()
			.refusal,
		Some(super::super::error::Refusal::OpaqueEnvelope)
	);
	let false_claim = [(crate::alloc::String::try_from("x").unwrap(), None)];
	assert_eq!(
		s.validate_session_record(&good, &false_claim)
			.unwrap_err()
			.refusal,
		Some(super::super::error::Refusal::OpaqueEnvelope)
	);
	assert_eq!(
		s.session_identity(&good).unwrap_err().refusal,
		Some(super::super::error::Refusal::OpaqueContainer)
	);
	s.root_release(good).unwrap();
	s.root_release(bad).unwrap();
	s.root_release(host).unwrap();
	s.root_release(plain).unwrap();
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
}

#[test]
fn construction_marks_are_transitive_and_public_mutation_cannot_hide_a_host() {
	let mut f = fixture(false, false);
	let mut s = store();
	let host = make(&mut f, &mut s, 0);
	let nested = s.root_list(Type::Tuple, &[&host]).unwrap();
	let outer = s.root_record(None, &[("nested", &nested)]).unwrap();
	for root in [&nested, &outer] {
		assert_eq!(
			s.session_identity(root).unwrap_err().refusal,
			Some(super::super::error::Refusal::OpaqueContainer)
		);
	}
	let plain = s.root_i64(7);
	let list = s.root_list(Type::Vec, &[&plain]).unwrap();
	let record = s.root_record(None, &[("x", &plain)]).unwrap();
	for error in [
		s.root_push(&list, &host).unwrap_err(),
		s.root_set(&list, 0, &nested).unwrap_err(),
		s.root_set_field(&record, "x", &outer).unwrap_err(),
	] {
		assert_eq!(
			error.refusal,
			Some(super::super::error::Refusal::OpaqueContainer)
		);
	}
	// Cycles stay plain: the only mutation that could taint an already
	// retained ancestor is refused before retaining or changing either edge.
	s.root_push(&list, &list).unwrap();
	assert_eq!(s.session_identity(&list).unwrap(), None);
	s.root_invalidate(&list).unwrap();
	for root in [outer, nested, host, list, record, plain] {
		s.root_release(root).unwrap();
	}
	assert_eq!(f.drops.load(Ordering::SeqCst), 1);
	assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
	// Reused slots must replace marks, including a slot that held the host.
	let plain = s.root_list(Type::Vec, &[]).unwrap();
	assert_eq!(s.session_identity(&plain).unwrap(), None);
	s.root_release(plain).unwrap();
}
