//! ADR 0266: target scope, diagnostics, cleanup and unchanged ordinary loops.
use crate::experimental_core::{
	execute::Executor,
	frontend,
	heap::{Limits, Store},
	value::{Scalar, Value},
};
use crate::{Context, Diagnostics, Source, Sources};

fn compile(source: &str) -> frontend::Program {
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let mut diagnostics = Diagnostics::new();
	frontend::compile(&mut sources, &context, &mut diagnostics, 0)
		.unwrap_or_else(|e| panic!("{e:?}: {:?}", diagnostics.diagnostics()))
}
#[test]
#[cfg_attr(
	miri,
	ignore = "listing fixture compiles multiple independent functions; native control"
)]
fn ordinary_loop_listing() {
	let source = r#"
pub fn plain(){let n=0;while n<3{n+=1;}n}
pub fn pattern(){let v=Some(3);let n=0;while let Some(x)=v{n+=x;v=None;}n}
pub fn nested(){let n=0;while n<3{let j=0;while j<2{j+=1;}n+=1;}n}
pub fn direct_for(){let n=0;for x in [1,2]{n+=x;}n}
pub fn adapted_for(){let n=0;for x in [1,2].iter().take(1){n+=x;}n}
pub fn text_for(){let n=0;for x in "ab".chars().take(1){n+=1;}n}
"#;
	let program = compile(source);
	let mut listing = std::string::String::new();
	for id in 0..program.code.functions() {
		let f = program.code.function(id as u32).unwrap();
		listing += &std::format!(
			"function {id}: slots={} parameters={} args={:?} strings={:?}\n",
			f.slots,
			f.parameters,
			f.args,
			f.strings
		);
		for (pc, instruction) in f.instructions().iter().enumerate() {
			listing += &std::format!("{pc}: {instruction:?}\n");
		}
	}
	if let Some(path) = std::env::var_os("RNX_LOOP_LISTING") {
		std::fs::write(path, &listing).unwrap();
	} else {
		assert_eq!(listing, include_str!("loop_control_listing.txt"));
	}
}

fn semantic(body: &str, expected: i64) {
	let program = compile(&std::format!("pub fn main(){{{body}}}"));
	let function = program.function(crate::Hash::type_hash(["main"])).unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let root = Executor::new()
		.unwrap()
		.invoke(&program, &mut store, function, &[], 10000)
		.unwrap();
	assert_eq!(
		store.root_scalar(&root).unwrap(),
		Some(Scalar::I64(expected))
	);
	store.root_release(root).unwrap();
	assert_eq!(store.live_entries(), 0);
}
fn rejected(body: &str, kind: &str) {
	use crate::experimental_core::error::{ErrorKind, Refusal};
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources
		.insert(Source::memory(std::format!("pub fn main(){{{body}}}")).unwrap())
		.unwrap();
	let mut diagnostics = Diagnostics::new();
	let error = frontend::compile(&mut sources, &context, &mut diagnostics, 0)
		.err()
		.expect("invalid loop control admitted");
	assert_eq!(error.kind, ErrorKind::Unsupported);
	assert_eq!(error.refusal, Some(Refusal::FrontendDiagnostic));
	assert!(diagnostics.has_error());
	let text = std::format!("{:?}", diagnostics.diagnostics());
	assert!(text.contains(kind), "{text}");
	assert!(error.location.is_some());
}
macro_rules! value_cases {($($name:ident => ($body:expr,$value:expr);)*)=>{$(
	#[test] fn $name(){semantic($body,$value);}
)*};}
macro_rules! reject_cases {($($name:ident => ($body:expr,$kind:expr);)*)=>{$(
	#[test] fn $name(){rejected($body,$kind);}
)*};}

value_cases! {
	valued_while => ("while true{break 7}",7);
	valued_while_let => ("while let Some(x)=Some(1){break 7}",7);
	while_break_plain => ("{let n=0;loop{n+=1;while {break;false} {}if n==3{break;}}n}",3);
	while_break_self => ("{let n=0;loop{n+=1;'w: while {break 'w;false} {}if n==3{break;}}n}",3);
	while_break_outer => ("{let n=0;'outer: loop{n+=1;if n==3{break;}while {break 'outer;false} {}}n}",1);
	while_continue_plain => ("{let n=0;while {n+=1;if n<3{continue;}false} {}n}",3);
	while_continue_self => ("{let n=0;'w: while {n+=1;if n<3{continue 'w;}false} {}n}",3);
	while_continue_outer => ("{let n=0;'outer: loop{n+=1;if n==3{break;}while {n+=1;if n<3{continue 'outer;}false} {}}n}",3);
	while_let_break_plain => ("{let n=0;loop{n+=1;while let Some(x)={break;None} {}if n==3{break;}}n}",3);
	while_let_break_self => ("{let n=0;loop{n+=1;'w: while let Some(x)={break 'w;None} {}if n==3{break;}}n}",3);
	while_let_break_outer => ("{let n=0;'outer: loop{n+=1;if n==3{break;}while let Some(x)={break 'outer;None} {}}n}",1);
	while_let_continue_plain => ("{let n=0;while let Some(x)={n+=1;if n<3{continue;}None} {}n}",3);
	while_let_continue_self => ("{let n=0;'w: while let Some(x)={n+=1;if n<3{continue 'w;}None} {}n}",3);
	while_let_continue_outer => ("{let n=0;'outer: loop{n+=1;if n==3{break;}while let Some(x)={n+=1;if n<3{continue 'outer;}None} {}}n}",3);
	for_iter_break_plain => ("{let n=0;'outer: loop{n+=1;for x in {if n<3{break;}[1]} {}if n==3{break;}}n}",1);
	for_iter_break_outer => ("{let n=0;'outer: loop{n+=1;for x in {if n<3{break 'outer;}[1]} {}if n==3{break;}}n}",1);
	for_iter_continue_plain => ("{let n=0;'outer: loop{n+=1;for x in {if n<3{continue;}[1]} {}if n==3{break;}}n}",3);
	for_iter_continue_outer => ("{let n=0;'outer: loop{n+=1;for x in {if n<3{continue 'outer;}[1]} {}if n==3{break;}}n}",3);
	while_inner_break => ("{let n=0;while {let x=loop{n+=1;break true;};n<3} {} n}",3);
	while_let_inner_break => ("{let n=0;while let Some(x)={loop{n+=1;if n<3{break Some(n);}break None;}} {} n}",3);
	for_inner_break => ("{let n=0;for x in loop{break [1,2,3];}{n+=x;}n}",6);
	body_label_outer_valued_for => ("'outer:loop{for x in [1]{break 'outer 7;}}",7);
	body_label_while_value => ("'w:while true{break 'w 7}",7);
}
reject_cases! {
	valued_for => ("for x in [1]{break 7}","BreakUnsupportedValue");
	for_iter_break_self => ("{let n=0;'outer: loop{n+=1;'f: for x in {if n<3{break 'f;}[1]} {}if n==3{break;}}n}","MissingLabel");
	for_iter_continue_self => ("{let n=0;'outer: loop{n+=1;'f: for x in {if n<3{continue 'f;}[1]} {}if n==3{break;}}n}","MissingLabel");
	while_unknown_label => ("while {break 'missing;false} {}","MissingLabel");
	while_let_unknown_label => ("while let Some(x)={continue 'missing;None} {}","MissingLabel");
	for_unknown_label => ("for x in {break 'missing;[1]} {}","MissingLabel");
	body_unknown_label => ("loop{break 'missing}","MissingLabel");
}
reject_cases! {
	for_value_discarded => ("for x in [1]{break 7;}0", "BreakUnsupportedValue");
	adapted_for_value => ("for x in [1].iter().take(1){break 7;}0", "BreakUnsupportedValue");
	text_for_value => (r#"for x in "ab".chars().take(1){break 7;}0"#, "BreakUnsupportedValue");
}

fn cleanup(body: &str, fails: bool) {
	let program = compile(&std::format!("pub fn main(z){{{body}}}"));
	let function = program.function(crate::Hash::type_hash(["main"])).unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	for _ in 0..3 {
		let result = Executor::new().unwrap().run_values(
			&program.code,
			&mut store,
			function,
			&[Value::I64(0)],
			10000,
		);
		if fails {
			let error = result.unwrap_err();
			assert_eq!(
				error.error.kind,
				crate::experimental_core::error::ErrorKind::DivisionByZero
			);
			assert!(error.cleanup_error.is_none(), "{error:?}");
		} else {
			assert_eq!(result.unwrap(), Value::I64(3));
		}
		assert_eq!(store.live_entries(), 0);
		assert_eq!(store.payload_bytes(), 0);
	}
}
macro_rules! cleanup_cases {($($name:ident => ($body:expr,$fails:expr);)*)=>{$(
	#[test] fn $name(){cleanup($body,$fails);}
)*};}

cleanup_cases! {
	cleanup_text_break => ("{let n=0;while {let temp=\"a\"+\"b\";n+=1;if n==3{break;}true}{}n}",false);
	cleanup_text_continue => ("{let n=0;while {let temp=\"a\"+\"b\";n+=1;if n<3{continue;}false}{}n}",false);
	cleanup_text_outer => ("{let n=0;'outer:loop{while {let temp=\"a\"+\"b\";n+=1;if n==3{break 'outer;}true}{}break;}n}",false);
	cleanup_text_error => ("{let n=0;while {let temp=\"a\"+\"b\";1/z;true}{}n}",true);
	cleanup_text_nested => ("{let n=0;while {let temp=\"a\"+\"b\";n+=1;loop{let inside=[3,4];break;}if n==3{break;}true}{}n}",false);
	cleanup_vector_break => ("{let n=0;while {let temp=[1,2];n+=1;if n==3{break;}true}{}n}",false);
	cleanup_vector_continue => ("{let n=0;while {let temp=[1,2];n+=1;if n<3{continue;}false}{}n}",false);
	cleanup_vector_outer => ("{let n=0;'outer:loop{while {let temp=[1,2];n+=1;if n==3{break 'outer;}true}{}break;}n}",false);
	cleanup_vector_error => ("{let n=0;while {let temp=[1,2];1/z;true}{}n}",true);
	cleanup_vector_nested => ("{let n=0;while {let temp=[1,2];n+=1;loop{let inside=[3,4];break;}if n==3{break;}true}{}n}",false);
	cleanup_lease_break => ("{let n=0;while {let temp=[1,2];let lease=temp.iter();n+=1;if n==3{break;}true}{}n}",false);
	cleanup_lease_continue => ("{let n=0;while {let temp=[1,2];let lease=temp.iter();n+=1;if n<3{continue;}false}{}n}",false);
	cleanup_lease_outer => ("{let n=0;'outer:loop{while {let temp=[1,2];let lease=temp.iter();n+=1;if n==3{break 'outer;}true}{}break;}n}",false);
	cleanup_lease_error => ("{let n=0;while {let temp=[1,2];let lease=temp.iter();1/z;true}{}n}",true);
	cleanup_lease_nested => ("{let n=0;while {let temp=[1,2];let lease=temp.iter();n+=1;loop{let inside=[3,4];break;}if n==3{break;}true}{}n}",false);
	cleanup_pattern_break => ("{let n=0;let result=while let Some(x)={let temp=[1,2];let lease=temp.iter();n+=1;if n==3{break;}Some(lease)}{x.next();};n}",false);
	cleanup_pattern_continue => ("{let n=0;let result=while let Some(x)={let temp=[1,2];let lease=temp.iter();n+=1;if n==2{continue;}if n==4{break 3;}Some(lease)}{x.next();};result}",false);
	cleanup_pattern_outer => ("{let n=0;'outer:loop{while let Some(x)={let temp=[1,2];let lease=temp.iter();n+=1;if n==3{break 'outer;}Some(lease)}{x.next();}break;}n}",false);
	cleanup_pattern_error => ("{let n=0;let result=while let Some(x)={let temp=[1,2];let lease=temp.iter();1/z;Some(lease)}{x.next();};n}",true);
	cleanup_pattern_nested => ("{let n=0;let result=while let Some(x)={let temp=[1,2];let lease=temp.iter();n+=1;loop{let inside=[3,4];break;}if n==3{break;}Some(lease)}{x.next();};n}",false);
}

cleanup_cases! {
	operand_text_break => ("{let n=0;while {n+=1;(\"a\"+\"b\").len()+{if n==3{break;}0}>0}{}n}",false);
	operand_text_continue => ("{let n=0;while {n+=1;(\"a\"+\"b\").len()+{if n<3{continue;}0}>0 && n<3}{}n}",false);
	operand_text_outer => ("{let n=0;'outer:loop{while {n+=1;(\"a\"+\"b\").len()+{if n==3{break 'outer;}0}>0}{}break;}n}",false);
	operand_pattern_break => ("{let n=0;let result=while let Some(x)={n+=1;Some([(\"a\"+\"b\"),{if n==3{break;}\"c\"}])}{x.len();};n}",false);
	operand_pattern_continue => ("{let n=0;let result=while let Some(x)={n+=1;Some([(\"a\"+\"b\"),{if n==2{continue;}if n==4{break 3;}\"c\"}])}{x.len();};result}",false);
	operand_pattern_outer => ("{let n=0;'outer:loop{while let Some(x)={n+=1;Some([(\"a\"+\"b\"),{if n==3{break 'outer;}\"c\"}])}{x.len();}break;}n}",false);
	operand_pattern_nested => ("{let n=0;let result=while let Some(x)={n+=1;Some([(\"a\"+\"b\"),{loop{let v=[4];break;}if n==3{break;}\"c\"}])}{x.len();};n}",false);
	operand_pattern_error => ("{let n=0;let result=while let Some(x)={n+=1;Some([(\"a\"+\"b\"),{1/z;\"c\"}])}{x.len();};n}",true);
}
reject_cases! {
	for_driver_selection_error => ("for x in [1].iter(){break 7;}async {}", "BreakUnsupportedValue");
}

#[test]
fn unrelated_refusal_before_for_is_not_a_break_error() {
	let context = Context::with_default_modules().unwrap();
	let mut sources = Sources::new();
	sources
		.insert(Source::memory("pub fn main(){async {};for x in [1].iter(){break 7;}}").unwrap())
		.unwrap();
	let error = frontend::compile(&mut sources, &context, &mut Diagnostics::new(), 0)
		.err()
		.unwrap();
	assert_eq!(
		error.refusal,
		Some(crate::experimental_core::error::Refusal::AsyncBlock)
	);
}
