//! Complete native get dispatch and refusal before execution.
use super::*;
use crate as rune;
use crate::experimental_core::{
	execute::Executor,
	heap::{Limits, Store},
	value::Value,
};
use std::{format, string::ToString};

fn compile_source(source: &str, context: &Context) -> Result<Program> {
	let mut sources = Sources::new();
	sources
		.insert(crate::Source::memory(source).unwrap())
		.unwrap();
	super::compile(&mut sources, context, &mut Diagnostics::new(), 277)
}

fn both_i64(source: &str, expected: i64) {
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources
		.insert(crate::Source::memory(source).unwrap())
		.unwrap();
	let mut vm = crate::prepare(&mut sources)
		.with_context(&context)
		.build_vm()
		.unwrap();
	let stock = crate::from_value::<i64>(vm.call(["main"], ()).unwrap()).unwrap();
	assert_eq!(stock, expected);
	let program = compile_source(source, &context).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let root = executor
		.invoke(
			&program,
			&mut store,
			program.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			100_000,
		)
		.unwrap();
	assert_eq!(store.import(&root).unwrap(), Value::I64(stock));
	store.root_release(root).unwrap();
	assert!(executor.is_idle());
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
}

#[test]
fn collection_get_all_four_providers_and_all_six_ranges_match_stock() {
	both_i64(
		r#"
		fn get(receiver, index) { receiver.get(index) }
		pub fn main() {
			assert_eq!(get(#{a: 7}, "a"), Some(7));
			assert_eq!(get(#{a: 7}, "b"), None);
			assert_eq!(get([7,8], 0), Some(7));
			assert_eq!(get((7,8), 1), Some(8));
			assert_eq!(get((), 0), None);
			assert_eq!(get((), ..), Some([]));
			assert_eq!(get([7,8], 10), None);
			assert_eq!(get([7,8], 1..0), None);
			assert_eq!(get((7,8), ..1), Some([7]));
			assert_eq!(get((7,8), ..=0), Some([7]));
			assert_eq!(get((7,8), 0..1), Some([7]));
			assert_eq!(get((7,8), 0..=0), Some([7]));
			assert_eq!(get((7,8), 1..), Some([8]));
			assert_eq!(get((7,8), ..), Some([7,8]));
			assert_eq!(get("éx", 0..2), Some("é"));
			assert_eq!(get("éx", 0..=1), Some("é"));
			assert_eq!(get("éx", ..2), Some("é"));
			assert_eq!(get("éx", ..=1), Some("é"));
			assert_eq!(get("éx", 2..), Some("x"));
			assert_eq!(get("éx", ..), Some("éx"));
			assert_eq!(get("éx", 1..), None);
			assert_eq!(get("éx", ..99), None);
			7
		}
	"#,
		7,
	);
}

#[test]
fn collection_get_field_element_and_shared_aliases_match_stock() {
	both_i64(
		r#"
		fn get(x, index) { x.get(index) }
		pub fn main() {
			let holder = #{value: #{a: [1]}};
			let receiver = [holder.value][0];
			let child = get(receiver, "a").unwrap();
			child.push(2);
			assert_eq!(holder.value.a, [1,2]);
			let copied = get((child, child), ..).unwrap();
			copied[0].push(3);
			assert_eq!(copied[1], [1,2,3]);
			let text = "abc";
			let sub = get(text, ..1).unwrap();
			sub.push('x');
			assert_eq!(text, "abc");
			assert_eq!(sub, "ax");
			7
		}
	"#,
		7,
	);
}

#[test]
fn collection_get_evaluates_receiver_and_argument_once_in_order() {
	both_i64(
		r#"
		fn receiver(log, value) { log.push(1); value }
		fn key(log, value) { log.push(2); value }
		pub fn main() {
			let log = [];
			let source = [7];
			let result = receiver(log, source).get(key(log, {source = [9]; 0}));
			assert_eq!(log, [1,2]);
			assert_eq!(result, Some(7));
			7
		}
	"#,
		7,
	);
}

#[test]
fn collection_get_unknown_user_provider_refuses_before_effects() {
	let context = Context::with_default_modules().unwrap();
	let source = "struct S; impl S { fn get(self,index){7} } fn get(x){x.get(0)} pub fn main(){get(S)} pub fn expose(){get}";
	let error = compile_source(source, &context).err().unwrap();
	assert_eq!(error.refusal, Some(Refusal::Method));
	assert!(error.location.is_some());
}

#[test]
fn collection_get_missing_providers_are_run_time_errors_not_refusals() {
	for receiver in [
		"()",
		"true",
		"7",
		"7u64",
		"1.5",
		"'a'",
		"std::cmp::Ordering::Equal",
		"None",
		"Ok(7)",
		"0..1",
		"0..",
		"..",
		"0..=1",
		"..=1",
		"..1",
		"function",
		"|| 7",
		"[7].iter()",
		"#{a:7}.iter()",
		"Some(7).iter()",
		"(0..2).iter()",
		"(0u64..2u64).iter()",
		"('a'..'z').iter()",
		"(0..=2).iter()",
		"(0u64..=2u64).iter()",
		"('a'..='z').iter()",
		"(0..).iter()",
		"(0u64..).iter()",
		"('a'..).iter()",
		"\"abc\".chars()",
		"\"a,b\".split(\",\")",
		"\"a,b\".split(',')",
		"i64",
		"Local",
		"Choice::A",
	] {
		// Unit has Tuple's get, so key zero is simply absent.
		let source = format!("struct Local;enum Choice{{A}}fn function(){{7}}fn get(x){{x.get(0)}}pub fn main(){{get({receiver})}}");
		let context = Context::with_default_modules().unwrap();
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(&source).unwrap())
			.unwrap();
		let mut vm = crate::prepare(&mut sources)
			.with_context(&context)
			.build_vm()
			.unwrap();
		let stock = vm.call(["main"], ());
		let program =
			compile_source(&source, &context).unwrap_or_else(|e| panic!("{receiver}: {e:?}"));
		let mut executor = Executor::new().unwrap();
		executor
			.set_host_runtime(Some(context.execution_runtime().unwrap()))
			.unwrap();
		let mut store = Store::new(Limits::default()).unwrap();
		let result = executor.invoke(
			&program,
			&mut store,
			program.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			10000,
		);
		if receiver == "()" {
			store.root_release(result.unwrap()).unwrap();
		} else {
			let failure = result.unwrap_err();
			let stock = stock.unwrap_err();
			let crate::runtime::VmErrorKind::MissingInstanceFunction { hash, instance } =
				stock.error().kind()
			else {
				panic!("{receiver}: {stock:?}");
			};
			let super::super::detail::Detail::MissingInstanceFunction {
				hash: actual_hash,
				instance: actual_instance,
				..
			} = failure.error.detail
			else {
				panic!("{receiver}: {failure:?}");
			};
			assert_eq!(actual_hash, *hash, "{receiver}");
			assert_eq!(actual_instance, instance.type_hash(), "{receiver}");
			assert!(failure
				.message(&program)
				.to_string()
				.contains("has no method named `get`"));
			assert_eq!(
				failure.error.kind,
				super::super::error::ErrorKind::Type,
				"{receiver}"
			);
		}
		assert!(executor.is_idle());
		assert_eq!(
			(store.live_entries(), store.payload_bytes()),
			(0, 0),
			"{receiver}"
		);
	}
}

#[test]
fn collection_get_refuses_a_cursor_passed_to_an_unknown_receiver() {
	let context = Context::with_default_modules().unwrap();
	for adapter in ["skip(1)", "take(1)"] {
		let source =
			format!(r#"fn get(x){{x.get(0)}} pub fn main(){{get("abc".chars().{adapter})}}"#);
		assert_eq!(
			compile_source(&source, &context)
				.err()
				.and_then(|error| error.refusal),
			Some(Refusal::Method)
		);
	}
}

#[derive(crate::Any)]
#[rune(item = ::collection_get)]
struct ContextOnly;

#[test]
fn collection_get_context_only_provider_defeats_totality() {
	let mut context = Context::with_default_modules().unwrap();
	let mut module = crate::Module::with_item(["collection_get"]).unwrap();
	module.ty::<ContextOnly>().unwrap();
	module
		.function("get", |_: &ContextOnly, _: i64| 7i64)
		.build_associated::<ContextOnly>()
		.unwrap();
	context.install(module).unwrap();
	let source = "fn probe(x){x.get(0)}pub fn main(){probe([7])}pub fn expose(){probe}";
	let error = compile_source(source, &context).err().unwrap();
	assert_eq!(error.refusal, Some(Refusal::Method));
	assert!(error.location.is_some());
}

#[test]
fn collection_get_proven_object_keeps_the_single_frame_direct_route() {
	use crate::experimental_core::execute::ExecutorLimits;
	let context = Context::with_default_modules().unwrap();
	let program = compile_source("pub fn main(){let o=#{a:7};o.get(\"a\")}", &context).unwrap();
	let mut executor = Executor::with_limits(ExecutorLimits {
		frames: 1,
		slots: 64,
	})
	.unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let root = executor
		.invoke(
			&program,
			&mut store,
			program.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			1000,
		)
		.unwrap();
	let (tag, present, value) = store.variant_parts(store.import(&root).unwrap()).unwrap();
	assert_eq!(tag, super::super::value::Type::Option);
	assert!(present);
	assert_eq!(value, Some(Value::I64(7)));
	store.root_release(root).unwrap();
	assert_eq!(store.live_entries(), 0);
}

#[test]
fn collection_get_missing_method_still_evaluates_both_operands_once() {
	let source = "fn receiver(log){log.push(1);7}fn key(log){log.push(2);0}pub fn main(log){receiver(log).get(key(log))}";
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources
		.insert(crate::Source::memory(source).unwrap())
		.unwrap();
	let mut vm = crate::prepare(&mut sources)
		.with_context(&context)
		.build_vm()
		.unwrap();
	let stock_log = crate::to_value(std::vec::Vec::<i64>::new()).unwrap();
	let error = vm.call(["main"], (stock_log.clone(),)).unwrap_err();
	assert!(matches!(
		error.error().kind(),
		crate::runtime::VmErrorKind::MissingInstanceFunction { .. }
	));
	let values: std::vec::Vec<_> = stock_log
		.borrow_ref::<crate::runtime::Vec>()
		.unwrap()
		.iter()
		.map(|v| v.as_integer::<i64>().unwrap())
		.collect();
	assert_eq!(values, [1, 2]);
	let program = compile_source(source, &context).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let log = store
		.root_list(super::super::value::Type::Vec, &[])
		.unwrap();
	let result = executor.invoke(
		&program,
		&mut store,
		program.function(Hash::type_hash(["main"])).unwrap(),
		&[&log],
		10000,
	);
	assert!(matches!(
		result.unwrap_err().error.detail,
		super::super::detail::Detail::MissingInstanceFunction { .. }
	));
	assert_eq!(store.root_len(&log).unwrap(), 2);
	for (index, expected) in [1, 2].into_iter().enumerate() {
		let child = store.root_get(&log, index).unwrap();
		assert_eq!(store.import(&child).unwrap(), Value::I64(expected));
		store.root_release(child).unwrap();
	}
	store.root_release(log).unwrap();
	assert!(executor.is_idle());
	assert_eq!(store.live_entries(), 0);
}

#[test]
fn collection_get_vec_deque_protocol_is_not_a_named_provider() {
	use crate::modules::collections::VecDeque;
	use crate::TypeHash;
	let mut context = Context::with_default_modules().unwrap();
	assert!(context.types().any(|hash| hash == VecDeque::HASH));
	let named = Hash::associated_function(VecDeque::HASH, Hash::ident("get"));
	let indexed = Hash::associated_function(VecDeque::HASH, &crate::runtime::Protocol::INDEX_GET);
	assert!(context.lookup_function(indexed).is_some());
	assert!(context.lookup_function(named).is_none());
	let source = "pub fn probe(x){x.get(0)}pub fn main(){probe([7])}";
	compile_source(source, &context).unwrap();
	let mut module = crate::Module::new();
	module
		.function("get", |_: &VecDeque, _: i64| 7i64)
		.build_associated::<VecDeque>()
		.unwrap();
	context.install(module).unwrap();
	assert!(context.lookup_function(named).is_some());
	let error = compile_source(source, &context).err().unwrap();
	assert_eq!(error.refusal, Some(Refusal::Method));
	assert!(error.location.is_some());
}
