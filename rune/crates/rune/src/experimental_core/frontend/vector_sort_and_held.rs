//! Compare script-visible behavior with the installed stock providers.
use super::*;
use crate::experimental_core::{
	execute::Executor,
	heap::{Limits, Store},
	value::Value,
};
use std::string::{String, ToString};

fn check(source: &str) {
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources
		.insert(crate::Source::memory(source).unwrap())
		.unwrap();
	let mut vm = crate::prepare(&mut sources)
		.with_context(&context)
		.build_vm()
		.unwrap();
	let expected = vm
		.call(["main"], ())
		.map(|v| v.as_integer::<i64>().unwrap())
		.map_err(|e| e.to_string());
	let program = super::compile(&mut sources, &context, &mut Diagnostics::new(), 30)
		.unwrap_or_else(|e| panic!("{source}: {e:?}"));
	let mut store = Store::new(Limits::default()).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let result: core::result::Result<i64, String> = match executor.invoke(
		&program,
		&mut store,
		program.function(Hash::type_hash(["main"])).unwrap(),
		&[],
		100000,
	) {
		Ok(root) => {
			let Value::I64(value) = store.import(&root).unwrap() else {
				panic!("expected i64")
			};
			store.root_release(root).unwrap();
			Ok(value)
		}
		Err(e) => Err(e.message(&program).to_string()),
	};
	assert_eq!(result, expected, "{source}");
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
}

#[test]
fn default_sort_uses_protocol_order_and_mutates_the_receiver() {
	for body in [
		"let a=[3,1,2];let b=a;if b.sort()!=(){return 1} a[0]*100+a[1]*10+a[2]",
		"let a=[];a.sort();a.len() as i64",
		"let a=[\"b\",\"a\"];a.sort();if a[0]==\"a\"{1}else{0}",
		"let a=[[3],[1]];a.sort();a[0][0]",
		"let a=[false,true,false];a.sort();if a[1]{0}else{1}",
		"let a=[0.0,-0.0,-1.0];a.sort();if 1.0/a[1]>0.0{1}else{0}",
		"let a=[1i64,2u64];a.sort();0",
		"let a=[1,\"a\"];a.sort();0",
		"let a=[0.0/0.0,1.0];a.sort();0",
		"let a=[1,2];let it=a.iter();a.sort();it.next();0",
		"let a=[\"a\",1];a.sort();0",
		"let a=[[1],1];a.sort();0",
		"let a=[Ok(1),1];a.sort();0",
		"let a=[Some(1),1];a.sort();0",
		"let a=[(1,2),1];a.sort();0",
		"let a=[(1..4),1];a.sort();0",
		"let a=[(),(1,2)];a.sort();0",
		"let a=[(1,2),()];a.sort();0",
		"let a=[(1..),(2..)];a.sort();0",
		"let a=[(..),(..)];a.sort();0",
		"let a=[Ok(2),Err(1)];a.sort();0",
		"let a=[None,Some(1)];a.sort();0",
		"let s=String::from(\"a\");let a=[1,s];drop(s);a.sort();0",
		"let s=[1];let a=[1,s];drop(s);a.sort();0",
		"let s=#{x:1};let a=[s,1];drop(s);a.sort();0",
		"let a=[#{},#{}];a.sort();0",
	] {
		check(&std::format!("pub fn main(){{{body}}}"));
	}
	check("fn call(x){x.sort()} pub fn main(){let obj=#{v:[3,1]};call(obj.v);obj.v[0]}");
}

#[test]
fn a_local_cursor_shares_progress_with_its_alias() {
	for source in [
		"pub fn main(){let it=[1,2,3].iter();let a=it.collect::<Vec>();let b=it.collect::<Vec>();(a.len()+b.len()) as i64}",
		"pub fn main(){let it=[1,2,3].iter();it.count() as i64}",
		"pub fn main(){let it=[1,2,3].iter();let b=it;it.find(|x|{b.next();true});b.next().unwrap()}",
		"pub fn main(){let it=[(\"a\",1),(\"a\",2)].iter();let obj=it.collect::<Object>();obj.a}",
		"pub fn main(){let a=[1].iter();let b=a;drop(a);b.collect::<Vec>();0}",
		"pub fn main(){let a=[1].iter();let b=a;drop(a);b.count();0}",
		"pub fn main(){let it=[1].iter();it.collect::<Object>();0}",
		"pub fn main(){let a=[1];let it=a.iter();it.count();a.push(2);it.next();0}",
		"pub fn main(){let a=[1];let it=a.iter();drop(it);a.push(2);a[1]}",
	] {check(source);}
}

#[test]
fn each_forward_consumer_accepts_proved_iterators() {
	for (values, end, result) in [
		("[2,3]", "count()", "r as i64"),
		("[2,3]", "collect::<Vec>()", "r[1]"),
		("[2,3]", "collect::<Tuple>()", "r.1"),
		("[('k',3)]", "collect::<Object>()", "0"),
		("[(\"k\",3)]", "collect::<Object>()", "r.k"),
		("['a','é']", "collect::<String>()", "r.len() as i64"),
		("[2,3]", "sum::<i64>()", "r"),
		("[2u64,3u64]", "product::<u64>()", "r as i64"),
		("[2.0,3.0]", "sum::<f64>()", "r as i64"),
		("[2,3]", "find(|x|x>2)", "r.unwrap()"),
		("[2,3]", "any(|x|x==2)", "if r {1} else {0}"),
		("[2,3]", "all(|x|x==2)", "if r {1} else {0}"),
		("[2,3]", "reduce(|a,b|a+b)", "r.unwrap()"),
		("[2,3]", "fold(2,|a,b|a*b)", "r"),
	] {
		check(&std::format!(
			"pub fn main(){{let it={values}.iter();let r=it.{end};{result}}}"
		));
	}
}

#[test]
fn unproved_cursor_locations_and_mutable_bindings_keep_the_base_refusal() {
	let context = Context::with_default_modules().unwrap();
	for source in [
		"pub fn main(it){it.collect::<Vec>()}",
		"fn consume(it){it.collect::<Vec>()} pub fn main(){consume([1,2].iter())}",
		"pub fn main(){let it=[1].iter();let f=||it.collect::<Vec>();f()}",
		"pub fn main(){let it=[1].iter();let a=#{x:it};a.x.collect::<Vec>()}",
		"pub fn main(){let it=[1].iter();let a=[it];a[0].collect::<Vec>()}",
		"fn take(it){it.collect::<Vec>()} pub fn main(){take([1].iter());take(1)}",
		"pub fn main(){let x=[1].iter();x=1;x.collect::<Vec>()}",
		"pub fn main(){let holder=[[1].iter()];let alias=holder;alias[0]=0;holder[0].collect::<Vec>()}",
		"pub fn main(){let holder=#{v:[1].iter()};let f=||holder.v=0;f();holder.v.collect::<Vec>()}",
		"struct S; impl S {fn iter(self){1}} pub fn main(){let it=S.iter();it.collect::<Vec>()}",
	] {
		let mut sources = Sources::new();
		sources.insert(crate::Source::memory(source).unwrap()).unwrap();
		let error = super::compile(&mut sources, &context, &mut Diagnostics::new(), 30).err().expect(source);
		assert_eq!(error.refusal, Some(super::super::error::Refusal::Method), "{source}");
	}
	check("struct S; impl S {fn count(self){9}} pub fn main(){S.count()}");
}

#[test]
fn held_char_and_string_fragments_retain_the_stock_capacity() {
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources
		.insert(
			crate::Source::memory(
				"pub fn main(){let it=['a',\"é\",'ç'].iter();it.collect::<String>()}",
			)
			.unwrap(),
		)
		.unwrap();
	let mut vm = crate::prepare(&mut sources)
		.with_context(&context)
		.build_vm()
		.unwrap();
	let stock = vm.call(["main"], ()).unwrap();
	let expected = stock.borrow_ref::<crate::alloc::String>().unwrap();
	let program = super::compile(&mut sources, &context, &mut Diagnostics::new(), 30).unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let root = executor
		.invoke(
			&program,
			&mut store,
			program.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			100000,
		)
		.unwrap();
	let value = store.import(&root).unwrap();
	assert_eq!(store.text(value).unwrap(), expected.as_str());
	let capacity = store
		.string_builder(
			crate::function::string_builder::Operation::Capacity,
			value,
			Value::Unit,
		)
		.unwrap();
	assert_eq!(capacity, Value::U64(expected.capacity() as u64));
	store.root_release(root).unwrap();
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
}

#[test]
fn touched_sort_and_comparison_handlers_decline_before_execution() {
	let source = "struct S;pub fn main(){let a=[S,1];a.sort()}";
	for key in [
		super::super::code::VEC_SORT,
		Hash::associated_function(Hash::type_hash(["S"]), &crate::runtime::Protocol::CMP),
	] {
		let mut context = Context::with_default_modules().unwrap();
		let handler = context
			.lookup_function(super::super::code::VEC_SORT)
			.unwrap()
			.clone();
		let _ = context.c13_insert_raw_fixture(key, handler);
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(source).unwrap())
			.unwrap();
		let error = super::compile(&mut sources, &context, &mut Diagnostics::new(), 30)
			.err()
			.expect("untrusted key");
		assert_eq!(error.refusal, Some(super::super::error::Refusal::Method));
	}
}
