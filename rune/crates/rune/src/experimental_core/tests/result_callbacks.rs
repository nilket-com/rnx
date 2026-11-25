//! Admission is local to the receiver, callback literal and owned closure exits.
use super::super::error::{Error, Refusal};
use super::super::frontend::{self, Program};
use crate::{Context, Diagnostics, Source, Sources};

fn compile(context: &Context, source: &str) -> Result<Program, Error> {
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	frontend::compile(&mut sources, context, &mut Diagnostics::new(), 1)
}

fn refuses(context: &Context, setup: &str, call: &str, method: &str) {
	let src = std::format!("fn named(x) {{ Ok(x) }} pub fn main() {{ {setup} {call} }}");
	let error = compile(context, &src).err().expect("must refuse");
	assert_eq!(error.refusal, Some(Refusal::Method), "{src}: {error:?}");
	assert_eq!(error.operation, method, "{src}");
	let at = error.location.unwrap();
	assert_eq!(&src[at.start as usize..at.end as usize], call);
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn result_callback_freshness_and_early_exits() {
	let context = Context::with_default_modules().unwrap();
	for (setup, call) in [
		("let r=Ok(1);", "r.and_then(|x| Ok(x))"),
		("", "named(1).and_then(|x| Ok(x))"),
		("let f=|x| Ok(x);", "Ok(1).and_then(f)"),
		("", "Ok(1).and_then(named)"),
		("", "Err(1).and_then(7)"),
		("let saved=Err(9);", "Ok(1).and_then(|_| saved)"),
		(
			"let saved=Err(9);",
			"Ok(1).and_then(|x| Ok({ if x == 1 { return saved; } x }))",
		),
		(
			"let saved=Err(9);",
			"Ok(1).and_then(|x| Ok({ let n=saved?; x+n }))",
		),
		("", "Ok(1).and_then(|x| 'a: { break 'a x; Ok(x) })"),
	] {
		refuses(&context, setup, call, "Result::and_then");
	}
	for (setup, call) in [
		("let r=Err(1);", "r.map(|x| x)"),
		("let f=|x| x;", "Ok(1).map({ f })"),
		("", "Err(1).map(named)"),
		("", "Err(1).map(7)"),
	] {
		refuses(&context, setup, call, "Result::map");
	}
	for call in [
		"Ok(1).and_then(|x| if x==1 { Ok(x) } else { Err(x) })",
		"Ok(1).and_then(|x| { if x==1 { return Err(x); } Ok(x) })",
		"Ok(1).and_then(|x| { let f=||{ return x; }; Ok(f()) })",
		"Ok(1).and_then(|_| panic!(\"body\"))",
		"Ok(1).and_then(|_| loop {})",
		"({ let k=1; Ok(k) }).map({ let n=4; |x| x+n })",
	] {
		let src = std::format!("pub fn main() {{ {call} }}");
		compile(&context, &src).unwrap_or_else(|err| panic!("{src}: {err:?}"));
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn result_callbacks_require_the_standard_associated_function() {
	use crate::runtime::Value;
	use crate::{Hash, Module};
	for (name, other) in [("map", "and_then"), ("and_then", "map")] {
		let call =
			std::format!("::std::result::Result::Ok(1).{name}(|x| ::std::result::Result::Ok(x))");
		let source = std::format!("pub fn main() {{ {call} }}");
		let check = |cx: &Context| {
			let err = compile(cx, &source).err().expect("untrusted provider");
			assert_eq!(err.refusal, Some(Refusal::Method), "{err:?}");
			assert_eq!(err.operation, std::format!("Result::{name}"));
			let at = err.location.unwrap();
			assert_eq!(&source[at.start as usize..at.end as usize], call);
		};
		let foreign = || {
			let mut m = Module::new();
			m.associated_function(name, |_: &Result<Value, Value>, _: Value| 123i64)
				.unwrap();
			m
		};
		let mut missing = Context::new();
		let mut result = crate::modules::result::module().unwrap();
		let named = Hash::ident(name);
		result.associated.retain(|a| !matches!(&a.name.kind, crate::compile::meta::AssociatedKind::Instance(n) if Hash::ident(n)==named));
		result.trait_impls.clear();
		missing.install(result).unwrap();
		check(&missing);
		missing.install(foreign()).unwrap();
		check(&missing);
		let mut overridden = Context::with_default_modules().unwrap();
		compile(&overridden, &source).unwrap();
		assert!(overridden.install(foreign()).is_err());
		check(&overridden);
		compile(
			&overridden,
			&source.replace(&std::format!(".{name}("), &std::format!(".{other}(")),
		)
		.unwrap();
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn result_callbacks_preserve_capture_order_nesting_and_identity() {
	use crate::experimental_core::{
		execute::Executor,
		heap::{Limits, Store},
		value::Scalar,
	};
	use crate::Hash;
	let cx = Context::with_default_modules().unwrap();
	for (body, expected) in [
		("let w=[]; let k=5; let r=({w.push(1); Ok(k)}).map({w.push(2); |x|{w.push(3); x+k}}); w[0]*1000+w[1]*100+w[2]*10+r.unwrap()",1240),
		("let w=[]; let r=({w.push(1); Err(w)}).map({w.push(2); |x|{w.push(3); x}}); w.push(4); match r { Err(a)=>a[0]*100+a[1]*10+a[2], _=>0 }",124),
		("let w=[]; let k=5; let r=({w.push(1); Ok(k)}).and_then({w.push(2); |x|{w.push(3); Err(x+k)}}); match r { Err(n)=>w[0]*1000+w[1]*100+w[2]*10+n, _=>0 }",1240),
		("let w=[]; let r=({w.push(1); Err(w)}).and_then({w.push(2); |x|{w.push(3); Ok(x)}}); w.push(4); match r { Err(a)=>a[0]*100+a[1]*10+a[2], _=>0 }",124),
		("match Ok(3).map(|x| Err(Ok(x))) { Ok(Err(Ok(n)))=>n, _=>0 }",3),
	] {
		let source=std::format!("pub fn target() {{ {body} }}");
		let (mut vm,program)=super::differential::compile_unit(&cx,"result-callback",&source,"target");
		assert_eq!(vm.call(["target"],()).unwrap().as_integer::<i64>().unwrap(),expected,"{body}");
		let mut store=Store::new(Limits::default()).unwrap();
		let value=Executor::new().unwrap().invoke(&program,&mut store,program.function(Hash::type_hash(["target"])).unwrap(),&[],10000).unwrap();
		assert!(matches!(store.root_scalar(&value).unwrap(),Some(Scalar::I64(n)) if n==expected),"{body}");
		store.root_release(value).unwrap();
		assert_eq!(store.live_entries(),0);
	}
}
