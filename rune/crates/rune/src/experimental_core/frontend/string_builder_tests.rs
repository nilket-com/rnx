//! Provider identity, observable growth, aliasing and terminal access errors.
use super::*;
use crate::experimental_core::{
	execute::Executor,
	heap::{Limits, Store},
	value::Value,
};
use crate::function::string_builder::Operation;
use std::string::{String, ToString};

fn compile_main(body: &str, context: &Context) -> Result<Program> {
	let mut sources = Sources::new();
	sources
		.insert(crate::Source::memory(body).unwrap())
		.unwrap();
	super::compile(&mut sources, context, &mut Diagnostics::new(), 30)
}

fn run(source: &str, context: &Context) -> core::result::Result<i64, String> {
	let program = compile_main(source, context).unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let result = match executor.invoke(
		&program,
		&mut store,
		program.function(Hash::type_hash(["main"])).unwrap(),
		&[],
		100000,
	) {
		Ok(root) => {
			let Value::I64(value) = store.import(&root).unwrap() else {
				panic!("expected an integer")
			};
			store.root_release(root).unwrap();
			Ok(value)
		}
		Err(failure) => Err(failure.message(&program).to_string()),
	};
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
	assert!(executor.is_idle());
	result
}

#[test]
fn each_builder_requires_its_own_installed_provider() {
	use Operation::*;
	let witnesses = [
		(New, "String::new()"),
		(WithCapacity, "String::with_capacity(8)"),
		(PushStr, "\"a\".push_str(\"x\")"),
		(Capacity, "\"a\".capacity()"),
		(Clear, "\"a\".clear()"),
		(Reserve, "\"a\".reserve(9)"),
		(ReserveExact, "\"a\".reserve_exact(9)"),
		(ShrinkToFit, "\"a\".shrink_to_fit()"),
	];
	for (omitted, _) in witnesses {
		let mut context = Context::with_default_modules().unwrap();
		context.omit_string_builder(omitted);
		for (tested, body) in witnesses {
			let source = std::format!("pub fn main(){{{body}}}");
			assert_eq!(
				compile_main(&source, &context).is_ok(),
				tested != omitted,
				"{omitted:?}, {tested:?}"
			);
		}
	}
}

#[test]
fn a_string_grows_at_the_standard_capacities_and_drops_its_allocation() {
	let context = Context::with_default_modules().unwrap();
	let source = r#"pub fn main(){
		for n in 0..40 {
			let s=String::new(); let other=s;
			s.push('a'); if s.capacity()!=8u64 {return 1}
			other.push_str("12345678"); if s.capacity()!=16u64 {return 2}
			s.reserve_exact(20); if s.capacity()!=29u64 {return 3}
			s.clear(); if s.capacity()!=29u64 {return 4}
			s.shrink_to_fit(); if s.capacity()!=0u64 {return 5}
		} 0
	}"#;
	assert_eq!(run(source, &context), Ok(0));
}

#[test]
fn self_append_and_unproved_receiver_calls_use_the_actual_receiver() {
	let context = Context::with_default_modules().unwrap();
	let source = r#"fn change(s){s.push_str(s)} pub fn main(){let s="ab";let alias=s;change(alias);s.push_str(alias);s+=s; s.len() as i64}"#;
	assert_eq!(run(source, &context), Ok(16));
	for (value, method) in [
		("2", "push_str(\"x\")"),
		("[]", "shrink_to_fit()"),
		("[]", "reserve_exact(2)"),
	] {
		let source = std::format!("fn change(x){{x.{method}}} pub fn main(){{change({value});0}}");
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(&source).unwrap())
			.unwrap();
		let mut stock = crate::prepare(&mut sources)
			.with_context(&context)
			.build_vm()
			.unwrap();
		let expected = stock.call(["main"], ()).unwrap_err().to_string();
		assert_eq!(run(&source, &context), Err(expected), "{method}");
	}
	for method in ["clear()", "capacity()", "reserve(3)"] {
		let source = std::format!("fn change(s){{s.{method}}}pub fn main(){{change([])}}");
		assert_eq!(
			compile_main(&source, &context).err().unwrap().refusal,
			Some(super::super::error::Refusal::Method)
		);
	}
}

#[test]
fn unproved_reserve_can_dispatch_when_no_other_receiver_provides_it() {
	use crate::modules::{any, clone, cmp, core, hash, iter, num, string};
	let mut context = Context::new();
	for module in [
		iter::module(),
		core::module(),
		cmp::module(),
		any::module(),
		clone::module(),
		num::module(),
		hash::module(),
		string::module(),
	] {
		context.install(module.unwrap()).unwrap();
	}
	let source = "fn call(x){x.reserve(3)} pub fn main(){call(());0}";
	let mut sources = Sources::new();
	sources
		.insert(crate::Source::memory(source).unwrap())
		.unwrap();
	let mut stock = crate::prepare(&mut sources)
		.with_context(&context)
		.build_vm()
		.unwrap();
	let expected = stock.call(["main"], ()).unwrap_err().to_string();
	assert_eq!(run(source, &context), Err(expected));
}

#[test]
fn budget_refuses_actual_growth_before_modifying_a_string() {
	use Operation::*;
	let mut store = Store::new(Limits {
		entries: 8,
		bytes: 7,
	})
	.unwrap();
	let text = store.string("ab").unwrap();
	for op in [PushStr, Reserve, ReserveExact] {
		let rhs = if op == PushStr { text } else { Value::I64(8) };
		assert_eq!(
			store.string_builder(op, text, rhs).unwrap_err().kind,
			ErrorKind::StoreCapacity
		);
		assert_eq!(store.text(text).unwrap(), "ab");
		assert_eq!((store.live_entries(), store.payload_bytes()), (1, 2));
	}
	store.release(text).unwrap();
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
}

#[test]
fn capacity_of_a_host_string_reports_the_imported_allocation() {
	let mut module = crate::Module::with_item(["host"]).unwrap();
	module
		.host_function("text", || {
			let mut s = String::with_capacity(64);
			s.push_str("ab");
			s
		})
		.unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(module).unwrap();
	assert_eq!(
		run(
			"pub fn main(){let s=host::text();s.capacity() as i64}",
			&context
		),
		Ok(2)
	);
}
