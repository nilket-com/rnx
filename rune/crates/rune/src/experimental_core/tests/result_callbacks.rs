//! Result receiver admission, callback conversion and value ownership.
use super::super::error::{Error, Refusal};
use super::super::frontend::{self, Program};
use crate::{Context, Diagnostics, Source, Sources};

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
fn result_callback_expression_admission() {
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
		let src = std::format!("fn named(x) {{ Ok(x) }} pub fn main() {{ {setup} {call} }}");
		compile(&context, &src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
	}
	for (setup, call) in [
		("let r=Err(1);", "r.map(|x| x)"),
		("let f=|x| x;", "Ok(1).map({ f })"),
		("", "Err(1).map(named)"),
		("", "Err(1).map(7)"),
	] {
		let src = std::format!("fn named(x) {{ Ok(x) }} pub fn main() {{ {setup} {call} }}");
		compile(&context, &src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
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

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn result_callback_dispatch_requires_the_receiver_fact() {
	use crate::Hash;
	let cx = Context::with_default_modules().unwrap();
	let src = "fn apply(r){r.map(|v|v+1)} pub fn main(){(apply(Ok(1)),apply(Err(9)))}";
	let program = compile(&cx, src).unwrap();
	assert!(program.entry_eligible(program.function(Hash::type_hash(["main"])).unwrap()));
	assert!(!program.entry_eligible(program.function(Hash::type_hash(["apply"])).unwrap()));
	for (source, call) in [
		("pub fn main(){let r=match false {true => Ok(1)};r.map(|x|x)}","r.map(|x|x)"),
		("pub fn main(){Some(1).map(|x|x)}","Some(1).map(|x|x)"),
		("struct S(i64); impl S{fn map(self,f){f(self.0)}} fn use_it(v){v.map(|x|x)} pub fn main(){(use_it(S(1)), use_it(Ok(4)))}","v.map(|x|x)"),
		("fn get(){Some(1)} pub fn main(){get().map(|x|x)}","get().map(|x|x)"),
	] {
		let error = compile(&cx, source).err().expect("unknown Result receiver");
		assert_eq!(error.operation,"map");
		let at=error.location.unwrap();
		assert_eq!(&source[at.start as usize..at.end as usize],call);
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn result_callback_is_readable_requires_registered_identity() {
	use crate::{runtime::Value, Module};
	let foreign = || {
		let mut module = Module::with_crate("std").unwrap();
		module
			.function("is_readable", |_: Value| false)
			.build()
			.unwrap();
		module
	};
	let source = "pub fn main(){::std::is_readable(1)}";
	let mut cx = Context::new();
	assert!(compile(&cx, source).is_err());
	cx.install(foreign()).unwrap();
	assert_eq!(
		compile(&cx, source).err().unwrap().refusal,
		Some(Refusal::Native)
	);
	let mut cx = Context::with_default_modules().unwrap();
	compile(&cx, source).unwrap();
	assert!(cx.install(foreign()).is_err());
	assert_eq!(
		compile(&cx, source).err().unwrap().refusal,
		Some(Refusal::Native)
	);
}

#[test]
#[cfg_attr(miri, ignore = "both engines compile the ownership witnesses")]
fn result_callback_ownership_contract() {
	use crate::experimental_core::{
		execute::Executor,
		heap::{Limits, Store},
		value::Scalar,
	};
	use crate::Hash;
	let cx = Context::with_default_modules().unwrap();
	// None records a legacy borrow or moved-value error. Other rows return
	// the observed readability of an alias or the callback's result.
	for (body, legacy, execution) in [
		("let r=Ok(1); let a=r; r.map(|x|{drop(a);x}).unwrap()",None,1),
		("let r=Ok(2); let a=r; r.and_then(|_|a).unwrap()",None,2),
		("let r=Ok(2); let a=r; let out=r.map(|x|{a.0=5;x}); out.unwrap()+a.unwrap()",None,7),
		("let k=1;let f=|x|x+k;let a=f; Ok(1).map(f); if is_readable(a){1}else{0}",Some(0),1),
		("let h=[];let f=|_|is_readable(h[0]);h.push(f); if Ok(1).map(f).unwrap(){1}else{0}",Some(0),1),
		("let saved=Ok(9);let alias=saved;let r=Ok(1).and_then(|_|saved);drop(r);if is_readable(alias){alias.unwrap()}else{0}",Some(0),9),
		("let f=|x|x;let g=f;drop(f); Ok(3).map(g).unwrap()",None,3),
		("let r=Ok(1);let out=r.map({r=Err(7);|x|x}); match r {Err(n)=>n+out.unwrap(),_=>0}",Some(8),8),
		("let r=Err([1]);let out=r.and_then(||42);drop(r);match out{Err(v)=>v[0],_=>0}",Some(1),1),
	] {
		let src=std::format!("pub fn target(){{{body}}}");
		let (mut vm, program)=super::differential::compile_unit(&cx,"result ownership",&src,"target");
		let observed=vm.call(["target"],()).map(|v|v.as_integer::<i64>().unwrap());
		assert_eq!(observed.ok(),legacy,"{body}");
		let mut store=Store::new(Limits::default()).unwrap();
		let id=program.function(Hash::type_hash(["target"])).unwrap();
		let root=Executor::new().unwrap().invoke(&program,&mut store,id,&[],10000).unwrap();
		assert!(matches!(store.root_scalar(&root).unwrap(),Some(Scalar::I64(n)) if n==execution),"{body}");
		store.root_release(root).unwrap();
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn result_callback_conversion_errors_are_located_at_the_call() {
	use crate::experimental_core::{
		execute::Executor,
		heap::{Limits, Store},
	};
	use crate::Hash;
	let cx = Context::with_default_modules().unwrap();
	for (setup, call, message) in [
		(
			"",
			"Ok(1).map(0)",
			"Expected type `std::ops::Function` but found `std::i64`",
		),
		(
			"",
			"Err(1).and_then(0)",
			"Expected type `std::ops::Function` but found `std::i64`",
		),
		(
			"let r=Ok(1);",
			"r.map({drop(r);0})",
			"Cannot read the value: it has been moved",
		),
		(
			"let n=1;let f=|x|x+n; let a=f;drop(f);",
			"Err(1).map(a)",
			"Cannot take the value: it has been moved",
		),
		(
			"",
			"Ok(1).map(||2)",
			"Wrong number of arguments 1, expected 0",
		),
		(
			"",
			"Ok(1).and_then(|v|v)",
			"Expected type `std::result::Result` but found `std::i64`",
		),
		(
			"let a=Err(9); let alias=a; drop(a);",
			"Ok(1).and_then(|_|alias)",
			"Cannot take the value: it has been moved",
		),
	] {
		let source = std::format!("pub fn target(){{{setup}{call}}}");
		let program = compile(&cx, &source).unwrap();
		let mut store = Store::new(Limits::default()).unwrap();
		let id = program.function(Hash::type_hash(["target"])).unwrap();
		let failure = Executor::new()
			.unwrap()
			.invoke(&program, &mut store, id, &[], 10000)
			.err()
			.unwrap();
		assert_eq!(
			std::format!("{}", failure.message(&program)),
			message,
			"{source}"
		);
		let at = failure.error.location.unwrap();
		assert_eq!(&source[at.start as usize..at.end as usize], call);
	}
}
