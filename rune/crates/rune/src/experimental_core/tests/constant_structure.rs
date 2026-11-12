//! ADR0257: selection, lexical scope and independent runtime materialization.
use crate::experimental_core::{
	execute::Executor,
	heap::{Limits, Store},
	value::Scalar,
};
use crate::{Context, Hash};

#[test]
#[cfg_attr(
	miri,
	ignore = "Miri: source frontend; run via the exact constant inventory"
)]
fn selection_blocks_and_branches_materialize_independently() {
	let source = r#"
const V=[#{key:([1,2],"s")},#{key:([3],"t")}];
const T=([4,5],[6]);
const O=#{a:[7,8],b:[9]};
const A=V[0];
const B=T[0u64];
const C=O["a"];
const D={let x=2;let y={let x=x+3;x};let _=4;x+y};
const U={let x=1; x;};
const E=if false {1/0}else if true {D}else{Some(1)};
const F=if false {if true {Some(1)}else{Some(2)}}else{3};
const G=if true {7};
const H=({let x=[11,12];x})[1];
const I=(if true {[13]}else{[14]})[0];
const J=if true {15}else if let Some(x)=Some(2){x}else{3};
pub fn target(){
 let a=A;let b=A;a.key.0.push(3);
 let t=B;t.push(6);let o=C;o.push(9);
 let inline=const {let x=2;{let x=x+1;x}};
 a.key.0.len()==3 && b.key.0.len()==2 && A.key.0.len()==2 &&
 t.len()==3 && B.len()==2 && o.len()==3 && C.len()==2 &&
 D==7 && U==() && E==7 && F==3 && G==() && H==12 && I==13 && J==15 && inline==3
}
"#;
	let context = Context::with_default_modules().unwrap();
	let (mut stock, program) =
		super::differential::compile_unit(&context, "constant-structure", source, "target");
	assert!(crate::from_value::<bool>(stock.call(["target"], ()).unwrap()).unwrap());
	let mut store = Store::new(Limits::default()).unwrap();
	let mut executor = Executor::new().unwrap();
	for _ in 0..2 {
		let root = executor
			.invoke(
				&program,
				&mut store,
				program.function(Hash::type_hash(["target"])).unwrap(),
				&[],
				100_000,
			)
			.unwrap();
		assert_eq!(store.root_scalar(&root).unwrap(), Some(Scalar::Bool(true)));
		store.root_release(root).unwrap();
		assert_eq!(store.live_entries(), 0);
	}
}

#[test]
#[cfg_attr(miri, ignore = "large admission budget fixtures; native controls")]
fn new_forms_share_limits_and_preserve_cycle_detection() {
	use crate::experimental_core::frontend;
	use crate::{Diagnostics, Source, Sources};
	let context = Context::with_default_modules().unwrap();
	let fail = |source: &str, reason: &str| {
		let mut sources = Sources::new();
		sources.insert(Source::memory(source).unwrap()).unwrap();
		let mut diagnostics = Diagnostics::new();
		assert!(frontend::compile(&mut sources, &context, &mut diagnostics, 0).is_err());
		let message = super::compile_diagnostics::nonwarning_diagnostics(&diagnostics);
		assert!(message.contains(reason), "{reason}: {message}");
	};
	let data = std::iter::repeat_n("0", 8192)
		.collect::<std::vec::Vec<_>>()
		.join(",");
	for expression in [
		"{let x=BASE;[x,x,x,x,x,x,x,x]}",
		"if true {[BASE,BASE,BASE,BASE,BASE,BASE,BASE,BASE]}else{[]}",
		"[BASE,BASE,BASE,BASE,BASE,BASE,BASE,BASE][0]",
	] {
		fail(
			&std::format!("const BASE=[{data}];const A={expression};pub fn target(){{A}}"),
			"expanded-node",
		);
	}
	// The same oversized expansion is harmless in an untaken arm.
	let source = std::format!("const BASE=[{data}];const A=if false {{[BASE,BASE,BASE,BASE,BASE,BASE,BASE,BASE]}}else{{[1]}};pub fn target(){{A}}");
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	assert!(frontend::compile(&mut sources, &context, &mut Diagnostics::new(), 0).is_ok());
	// Each predecessor is below the per-constant limit; this block's local
	// reads share one byte budget with indexing and the selected branch.
	let mut prefix = std::format!("const S0=\"{}\";", "x".repeat(16384));
	for n in 1..=6 {
		prefix.push_str(&std::format!("const S{n}=S{}+S{};", n - 1, n - 1));
	}
	fail(&std::format!("{prefix} const A={{let x=S6;if true {{[x,x,x,x,x,x,x,x][0]}}else{{\"\"}}}};pub fn target(){{A}}"), "byte");
	let nested = std::format!("{}1{}", "{let x=0;".repeat(70), "}".repeat(70));
	fail(
		&std::format!("const A={nested};pub fn target(){{A}}"),
		"MaxConstDepth",
	);
	for expression in ["[A][0]", "{A}", "if true {A}else{1}"] {
		let mut sources = Sources::new();
		sources
			.insert(
				Source::memory(std::format!("const A={expression};pub fn target(){{A}}")).unwrap(),
			)
			.unwrap();
		let mut diagnostics = Diagnostics::new();
		assert!(frontend::compile(&mut sources, &context, &mut diagnostics, 0).is_err());
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "Miri: source frontend; run via the exact constant inventory"
)]
fn constant_local_failure_drops_partial_values() {
	use crate::experimental_core::frontend;
	use crate::{Diagnostics, Source, Sources};
	let context = Context::with_default_modules().unwrap();
	for (source, reason) in [
		(
			"const A={let x=[1,2];{let y=x;y[9]}};\npub fn target(){A}",
			"Index out of bounds",
		),
		(
			"const A=const {let x=#{a:[1]};if true {x[\"missing\"]}else{[]}};\npub fn target(){A}",
			"Missing field",
		),
	] {
		let mut sources = Sources::new();
		sources.insert(Source::memory(source).unwrap()).unwrap();
		let mut diagnostics = Diagnostics::new();
		assert!(frontend::compile(&mut sources, &context, &mut diagnostics, 0).is_err());
		assert!(super::compile_diagnostics::nonwarning_diagnostics(&diagnostics).contains(reason));
	}
	let (mut stock, program) = super::differential::compile_unit(
		&context,
		"constant-recovery",
		"const A={let x=[1];let y={let x=[2];x[0]};y+x[0]};\npub fn target(){A}",
		"target",
	);
	assert_eq!(
		crate::from_value::<i64>(stock.call(["target"], ()).unwrap()).unwrap(),
		3
	);
	let mut store = Store::new(Limits::default()).unwrap();
	let root = Executor::new()
		.unwrap()
		.invoke(
			&program,
			&mut store,
			program.function(Hash::type_hash(["target"])).unwrap(),
			&[],
			100_000,
		)
		.unwrap();
	assert_eq!(store.root_scalar(&root).unwrap(), Some(Scalar::I64(3)));
	store.root_release(root).unwrap();
	assert_eq!(store.live_entries(), 0);
}
