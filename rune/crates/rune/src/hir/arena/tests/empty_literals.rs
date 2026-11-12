//! One compilation per test, including every strict-provenance regression.
#[cfg(feature = "execution")]
use crate::Diagnostics;
use crate::{Context, Source, Sources};

fn legacy(source: &str) {
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let mut vm = crate::prepare(&mut sources)
		.with_context(&context)
		.build_vm()
		.unwrap();
	assert!(crate::from_value::<bool>(vm.call(["main"], ()).unwrap()).unwrap());
}

#[cfg(feature = "execution")]
fn execution(source: &str, refused: Option<crate::experimental_core::error::Refusal>) {
	use crate::experimental_core::{
		execute::Executor,
		frontend,
		heap::{Limits, Store},
		value::Value,
	};
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let mut diagnostics = Diagnostics::new();
	let result = frontend::compile(&mut sources, &context, &mut diagnostics, 0);
	if let Some(expected) = refused {
		let error = result.err().expect("expected admission refusal");
		assert_eq!(
			error.kind,
			crate::experimental_core::error::ErrorKind::Unsupported
		);
		assert_eq!(error.refusal, Some(expected));
		return;
	}
	let program = result.unwrap();
	let function = program.function(crate::Hash::type_hash(["main"])).unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let result = Executor::new()
		.unwrap()
		.run_values(&program.code, &mut store, function, &[], 10000)
		.unwrap();
	assert_eq!(result, Value::Bool(true));
	assert_eq!(store.live_entries(), 0);
}
macro_rules! cases {($($old:ident,$new:ident => $source:expr;)* ) => {$(
	#[test] fn $old(){legacy($source);}
	#[test] #[cfg(feature="execution")] fn $new(){execution($source,None);}
)*};}
cases! {
	legacy_text,execution_text => r#"pub fn main(){"".len()==0}"#;
	legacy_vector,execution_vector => "pub fn main(){[].len()==0}";
	legacy_object,execution_object => "pub fn main(){#{}.len()==0}";
	legacy_tuple,execution_tuple => "pub fn main(){()==()}";
	legacy_block,execution_block => "pub fn main(){let x={};x==()}";
	legacy_function,execution_function => "fn empty(){} pub fn main(){empty()==()}";
	legacy_constant_text,execution_constant_text => r#"const S="";pub fn main(){S.len()==0}"#;
}
#[test]
fn legacy_bytes() {
	legacy(r#"pub fn main(){b"".len()==0}"#);
}
#[test]
#[cfg(feature = "execution")]
fn execution_bytes_refused() {
	execution(
		r#"pub fn main(){b"".len()==0}"#,
		Some(crate::experimental_core::error::Refusal::ByteStr),
	);
}
const POINTER: &str = r#"pub fn main(){format!("{:p}",0)=="0x0" && format!("{:p}",1)=="0x1" && format!("{:p}",4660)=="0x1234" && format!("{:p}",9223372036854775807)=="0x7fffffffffffffff" && format!("{:p}",-1)=="0xffffffffffffffff"}"#;
const POINTER_PADDING: &str = r#"pub fn main(){format!("{:10p}",0)=="0x0       " && format!("{:10p}",1)=="0x1       " && format!("{:10p}",4660)=="0x1234    " && format!("{:10p}",-1)=="0xffffffffffffffff" && format!("{:<10p}",0)=="0x0       " && format!("{:<10p}",1)=="0x1       " && format!("{:<10p}",4660)=="0x1234    " && format!("{:<10p}",-1)=="0xffffffffffffffff" && format!("{:010p}",0)=="00000000x0" && format!("{:010p}",1)=="00000000x1" && format!("{:010p}",4660)=="00000x1234" && format!("{:010p}",-1)=="0xffffffffffffffff" && format!("{:*>10p}",0)=="*******0x0" && format!("{:*>10p}",1)=="*******0x1" && format!("{:*>10p}",4660)=="****0x1234" && format!("{:*>10p}",-1)=="0xffffffffffffffff" && format!("{:^10p}",0)=="   0x0    " && format!("{:^10p}",1)=="   0x1    " && format!("{:^10p}",4660)=="  0x1234  " && format!("{:^10p}",-1)=="0xffffffffffffffff"}"#;
#[test]
fn legacy_pointer_padding() {
	legacy(POINTER_PADDING);
}
#[test]
#[cfg(feature = "execution")]
fn execution_pointer_padding_refused() {
	execution(
		POINTER_PADDING,
		Some(crate::experimental_core::error::Refusal::FormatSpec),
	);
}

#[test]
fn legacy_pointer_format() {
	legacy(POINTER);
}
#[test]
#[cfg(feature = "execution")]
fn execution_pointer_format_refused() {
	execution(
		POINTER,
		Some(crate::experimental_core::error::Refusal::FormatSpec),
	);
}
