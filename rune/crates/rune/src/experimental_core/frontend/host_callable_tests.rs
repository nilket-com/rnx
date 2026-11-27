use super::vector_sort_and_held::check;
use super::*;

#[test]
fn by_value_callbacks_take_the_payload_on_hit_and_miss_but_comparators_borrow() {
	for source in [
		"pub fn main(){let f=Vec::len;let g=f;let r=Ok([1,2]).map(f);if std::is_readable(g){0}else{r.unwrap() as i64}}",
		"pub fn main(){let f=Vec::len;let g=f;let r=Result::map(Err(4),f);if std::is_readable(g){0}else{match r{Err(x)=>x,_=>0}}}",
		"pub fn main(){let f=Vec::len;let g=f;let r=[[3]].iter().map(f).collect::<Vec>();if std::is_readable(g){0}else{r[0] as i64}}",
		"pub fn main(){let f=i64::cmp;let g=f;let v=[2,1];v.sort_by(f);if std::is_readable(g){v[0]}else{0}}",
		"pub fn main(){let f=Vec::len;let g=f.clone();drop(f);if std::is_readable(f){0}else{g([1,2]) as i64}}",
	] {
		check(source);
	}
}

#[test]
fn qualified_receivers_include_unit_and_concrete_iterator_generics() {
	for source in [
		"pub fn main(){std::tuple::Tuple::len(()) as i64}",
		"pub fn main(){let f=i64::clone;f(-3)}",
		"pub fn main(){let f=std::ops::RangeIter::<u64>::next;f((1u64..3u64).iter()).unwrap() as i64}",
		"pub fn main(){let f=std::string::Split::<char>::count;f(\"x.y\".split('.')) as i64}",
		"pub fn main(){let f=Vec::cmp;if f([1],[2])==std::cmp::Ordering::Less{1}else{0}}",
		"pub fn main(){let f=String::push;let s=String::new();f(s,'é');s.len() as i64}",
	] {
		check(source);
	}
}

#[test]
fn function_values_that_need_a_static_argument_or_cursor_shape_decline() {
	let context = Context::with_default_modules().unwrap();
	for name in [
		"Vec::resize",
		"String::split",
		"std::slice::Iter::map",
		"std::string::Chars::skip",
		"std::string::Split::<char>::take",
		"std::iter::Skip::count",
		"std::iter::Take::collect::<Vec>",
	] {
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(std::format!("pub fn main(){{let f={name};f}}")).unwrap())
			.unwrap();
		let error = super::compile(&mut sources, &context, &mut Diagnostics::new(), 30)
			.err()
			.expect("unproven function value");
		assert_eq!(
			error.refusal,
			Some(super::super::error::Refusal::Native),
			"{name}: {error:?}"
		);
	}
}
#[test]
fn host_functions_in_mixed_containers_callbacks_and_path_calls() {
	for source in [
		"fn id(x){x} pub fn main(){let n=2;let fs=[i64::abs,id,|x|x+n];let total=0;for f in fs {total+=f(-3)} total}",
		"pub fn main(){let f=Some;f(5).unwrap()}",
		"pub fn main(){let f=Vec::len;let v=[1,2];f(v) as i64+Vec::len(v) as i64}",
		"pub fn main(){[[0],[1,2]].iter().map(Vec::len).sum::<u64>() as i64}",
		"fn apply(f){[[2,3]].iter().map(f).sum::<u64>() as i64} pub fn main(){apply(Vec::len)}",
		"pub fn main(){let fs=[Vec::len];[[1,2]].iter().map(fs[0]).sum::<u64>() as i64}",
		"pub fn main(){let v=[3,1,2];let sort=Vec::sort;sort(v); v[0]}",
		"pub fn main(){use std::slice::Iter;let f=Iter::collect::<Vec>; f([4,5].iter())[0]}",
		"pub fn main(){use std::slice::Iter;let f=Iter::fold; f([4,5].iter(),0,|a,b|a+b)}",
	] {
		check(source);
	}
}

#[test]
fn iterator_callback_conversion_reports_the_native_argument_index() {
	use crate::experimental_core::{
		execute::Executor,
		heap::{Limits, Store},
	};
	use crate::runtime::InvocationContext::BadArgument;
	use std::string::ToString;

	for (source, arg) in [
		("pub fn main(){[1].iter().filter(None).count()}", 1),
		("pub fn main(){let f=[None];[1].iter().fold(0,f[0])}", 2),
		(
			"pub fn main(){let f=std::slice::Iter::fold;f([1].iter(),0,None)}",
			2,
		),
		(
			"pub fn main(){let f=Vec::len;let g=f;drop(f);[[2]].iter().map(g).count()}",
			1,
		),
	] {
		let context = Context::with_default_modules().unwrap();
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(source).unwrap())
			.unwrap();
		let mut vm = crate::prepare(&mut sources)
			.with_context(&context)
			.build_vm()
			.unwrap();
		let stock = vm.call(["main"], ()).unwrap_err();
		let program = super::compile(&mut sources, &context, &mut Diagnostics::new(), 30).unwrap();
		let mut store = Store::new(Limits::default()).unwrap();
		let mut executor = Executor::new().unwrap();
		let failure = executor
			.invoke(
				&program,
				&mut store,
				program.function(Hash::type_hash(["main"])).unwrap(),
				&[],
				100000,
			)
			.unwrap_err();
		assert_eq!(
			failure.message(&program).to_string(),
			stock.to_string(),
			"{source}"
		);
		assert!(
			failure.invocation_context().eq(stock.invocation_context()),
			"{source}"
		);
		assert!(
			failure.invocation_context().eq([BadArgument { arg }]),
			"{source}"
		);
		assert!(executor.is_idle());
		assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
	}
}
