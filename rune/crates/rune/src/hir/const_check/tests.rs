use crate::ast::Spanned;
use crate::diagnostics::Diagnostic;
use crate::{Context, Diagnostics, Source, Sources};
use rust_alloc::{
	format,
	string::{String, ToString},
};

fn compile(context: &Context, source: &str, execution: bool) -> Option<(String, crate::ast::Span)> {
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let mut diagnostics = Diagnostics::new();
	let ok = if execution {
		crate::experimental_core::frontend::compile(&mut sources, context, &mut diagnostics, 0)
			.is_ok()
	} else {
		crate::prepare(&mut sources)
			.with_context(context)
			.with_diagnostics(&mut diagnostics)
			.build()
			.is_ok()
	};
	if ok {
		return None;
	}
	let error = diagnostics
		.diagnostics()
		.iter()
		.find_map(|d| match d {
			Diagnostic::Fatal(f) => f.as_compile_error(),
			_ => None,
		})
		.unwrap_or_else(|| panic!("{source}: {diagnostics:?}"));
	Some((error.to_string(), error.span()))
}

#[test]
fn dead_forms_report_the_same_error_and_span() {
	let cx = Context::with_default_modules().unwrap();
	for (invalid, message) in [
		("break;", "Break outside of loop"),
		("continue;", "Continue outside of loop"),
		("1=2;", "Cannot assign to expression"),
		("1+=2;", "Unsupported binary expression"),
		("for i in [1] {break 3;}", "Can only break with a value"),
		("'b: {continue 'b;}", "Labeled blocks cannot be"),
		(
			"match 1.2 {1.2=>0,_=>1};",
			"This kind of expression is not supported as a pattern",
		),
		("let a=*4;", "Unsupported unary operator"),
		(
			"let x=#{a:2}; let a=x.a::<i64>;",
			"Unsupported field access",
		),
	] {
		for dead in [
			format!("if false {{{invalid}}}7"),
			format!("match 0 {{1=>{{{invalid}0}},_=>7}}"),
			format!("if false {{{{{invalid}}}}}7"),
		] {
			for entry in [
				format!("const V={{{dead}}};"),
				format!("const fn f(){{{dead}}} const V=f();"),
				format!("const fn f(){{{dead}}}const fn g(){{if false {{f();}}7}} const V=g();"),
			] {
				let source = format!("{entry}pub fn main(){{V}}");
				let legacy = compile(&cx, &source, false)
					.unwrap_or_else(|| panic!("legacy accepted {source}"));
				assert!(legacy.0.contains(message), "{source}: {legacy:?}");
				assert_eq!(compile(&cx, &source, true), Some(legacy), "{source}");
			}
		}
	}
}

#[test]
fn structural_exits_preserve_accepted_constants() {
	let cx = Context::with_default_modules().unwrap();
	for (body, value) in [
		("return 7;1=2;", 7),
		("loop{break 7;1=2;}", 7),
		("(return 7,{1=2;});2", 7),
		("(return 7)+{1=2;};2", 7),
		("g(return 7,{1=2;});2", 7),
		("if x{return 7;}else{return 3;}1=2;0", 7),
		("return 7;bad()", 7),
		("if {return 7;} {1=2;} else {2;} 3", 7),
		("match x {_=>7, false=>{1=2;0}}", 7),
		("for y in {return 7;} {1=2;}0", 7),
		("while x {return 7;1=2;}0", 7),
		("match 0 {0 if {return 7;}=>5, _=>{1=2;9}}", 7),
		(
			"let a=1;a=7;if false {let b=#{x:0};b.x=2;let c=[0];c[0]=2;}a",
			7,
		),
		("while true {break 7;}", 7),
		("'outer: loop{loop{break 'outer 7}}", 7),
	] {
		let source=format!("const fn bad(){{if false{{1=2;}}0}} const fn g(a,b){{0}}const fn f(x){{{body}}}const V=f(true);const CHECK=if V=={value}{{0}}else{{1/0}};pub fn main(){{CHECK}}");
		assert_eq!(compile(&cx, &source, false), None, "{source}");
		assert_eq!(compile(&cx, &source, true), None, "{source}");
	}
}

#[test]
fn selection_and_diagnostic_order_follow_each_root() {
	let cx = Context::with_default_modules().unwrap();
	for (defs, root) in [
		("", "const V={if false {1=2;}7};pub fn main(){7}"),
		("const fn bad(){if false {1=2;}7}", "pub fn main(){7}"),
		(
			"const fn bad(){if false {1=2;}7}",
			"const V={if false {bad();}7};pub fn main(){7}",
		),
		("", "const V={if true {1=2;}else {3+=4;}7};pub fn main(){V}"),
		("", "const V={if true {3+=4;}else {1=2;}7};pub fn main(){V}"),
		(
			"",
			"const V={if {return 7;} {1=2;}else {3+=4;}7};pub fn main(){V}",
		),
		(
			"const fn f(x){if x{return 7;}1=2;3}",
			"const V=f(true);pub fn main(){V}",
		),
		(
			"const fn a(){1=2;}const fn b(){3+=4;}",
			"const V={if false {a();b();1=3;}7};pub fn main(){V}",
		),
		(
			"const fn a(){1=2;}const fn b(){3+=4;}",
			"const V={if false {a();b();}7};pub fn main(){V}",
		),
		(
			"const fn a(){1=2;}const fn b(){3+=4;}",
			"const V={if false {b();a();}7};pub fn main(){V}",
		),
		(
			"const A={if false {1=2;}7};const B={if false {3+=4;}7};",
			"pub fn main(){(A,B)}",
		),
		(
			"const A={if false {1=2;}7};const B={if false {3+=4;}7};",
			"pub fn main(){(B,A)}",
		),
		("", "const V={if false {1=2;missing;}7};pub fn main(){V}"),
		(
			"",
			"const V={if false {select{default=>{1=2;}};}7};pub fn main(){V}",
		),
		(
			"const fn f(){while {return 7;} {1=2;}8}",
			"const V=f();pub fn main(){V}",
		),
	] {
		let source = format!("{defs}{root}");
		assert_eq!(
			compile(&cx, &source, true),
			compile(&cx, &source, false),
			"{source}"
		);
	}
}
