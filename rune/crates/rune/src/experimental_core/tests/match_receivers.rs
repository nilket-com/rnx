//! The fallthrough of a partial match reaches the call with a unit receiver.
use super::super::code::{Op, INSTANCE_METHOD};
use super::super::error::{Error, Refusal};
use super::super::frontend::{self, Program};
use crate::{Context, Diagnostics, Hash, Source, Sources};

fn compile(context: &Context, source: &str) -> Result<Program, Error> {
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	frontend::compile(&mut sources, context, &mut Diagnostics::new(), 1)
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn local_match_dispatches_only_when_it_can_miss() {
	let context = Context::with_default_modules().unwrap();
	for (arms, dynamic) in [
		("1=>S{}", true),
		("_ if false=>S{}", true),
		("_=>S{}", false),
		("bound=>S{}", false),
	] {
		let source = std::format!("struct S{{}} impl S{{fn m(self){{7}}}} pub fn main(v){{let x=match v {{{arms}}}; x.m()}}");
		let program = compile(&context, &source).unwrap();
		let main = program
			.code
			.function(program.function(Hash::type_hash(["main"])).unwrap())
			.unwrap();
		let method = program.function(Hash::type_hash(["S", "m"])).unwrap();
		let calls = (0..main.instructions().len())
			.filter_map(|pc| match main.operation(pc).unwrap() {
				Op::Call { function, .. } => Some(function),
				_ => None,
			})
			.collect::<std::vec::Vec<_>>();
		assert_eq!(calls.contains(&INSTANCE_METHOD), dynamic, "{source}");
		assert_eq!(calls.contains(&method), !dynamic, "{source}");
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn receiver_join_distinguishes_fallthrough_from_return() {
	let context = Context::with_default_modules().unwrap();
	for (body, covered) in [
		("match v {1=>S{}}", false),
		("match v {_ if false=>S{}}", false),
		("match v {_=> match v {1=>S{}}}", false),
		("match v {1=>return S{}};", false),
		("match v {_=>return S{}};", true),
		("match v {bound=>S{}}", true),
	] {
		for (main, helper, call) in [
			("let x=make(v); x.capacity()", "", "x.capacity()"),
			(
				"through(make(v))",
				"fn through(x){x.capacity()}",
				"x.capacity()",
			),
		] {
			let src = std::format!("struct S{{}} impl S{{fn capacity(self){{7}}}} fn make(v){{{body}}} {helper} pub fn main(v){{{main}}}");
			let result = compile(&context, &src);
			if covered {
				result.unwrap_or_else(|e| panic!("{src}: {e:?}"));
			} else {
				let err = result
					.err()
					.expect("partial match must lose its nominal proof");
				assert_eq!(err.refusal, Some(Refusal::Method), "{src}");
				let at = err.location.unwrap();
				assert_eq!(&src[at.start as usize..at.end as usize], call, "{src}");
			}
		}
	}
}
