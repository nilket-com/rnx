//! F1: both engines agree on evaluated boolean operands, effects and locations.
use crate::experimental_core::{
	error::ErrorKind,
	execute::Executor,
	heap::{Limits, Store},
	value::Type,
};
use crate::runtime::VmErrorKind;
use crate::{Context, Hash};

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn both_engines_agree_on_strict_conditions_locations_and_effects() {
	let context = Context::with_default_modules().unwrap();
	for (body, operand, marks) in [
		("1 && mark(w, true)", "1", 0),
		("1 || mark(w, true)", "1", 0),
		("true && 5", "5", 0),
		("false || 7", "7", 0),
		("true && mark(w, 1)", "mark(w, 1)", 1),
		("false || mark(w, 1)", "mark(w, 1)", 1),
		("true && mark(w, 1); ()", "mark(w, 1)", 1),
		("false || mark(w, 1); ()", "mark(w, 1)", 1),
		("(true && 1) || true", "1", 0),
		("true && (false || 2)", "2", 0),
		("!(1 && true)", "1", 0),
		("if 1 {}", "1", 0),
		("if false {} else if 1 {}", "1", 0),
		("while 0 {}", "0", 0),
		("\"x\" || false", "\"x\"", 0),
	] {
		let source =
			std::format!("pub fn target(w) {{ {body} }}\nfn mark(w, x) {{ w.push(0); x }}");
		let (mut stock, program) =
			super::differential::compile_unit(&context, "strict", &source, "target");
		let legacy_w = crate::to_value(crate::runtime::Vec::new()).unwrap();
		let error = stock.call(["target"], (legacy_w.clone(),)).unwrap_err();
		let at = error.first_location().unwrap();
		let span = at
			.unit
			.debug_info()
			.unwrap()
			.instruction_at(at.ip)
			.unwrap()
			.span;
		assert!(
			matches!(error.into_kind(), VmErrorKind::ExpectedType { .. }),
			"{body}"
		);
		let start = source.find(operand).unwrap();
		assert_eq!(span.range(), start..start + operand.len(), "{body}");
		assert_eq!(
			legacy_w.borrow_ref::<crate::runtime::Vec>().unwrap().len(),
			marks
		);
		let mut store = Store::new(Limits::default()).unwrap();
		let witness = store.root_list(Type::Vec, &[]).unwrap();
		let mut executor = Executor::new().unwrap();
		let function = program.function(Hash::type_hash(["target"])).unwrap();
		let failure = executor
			.invoke(&program, &mut store, function, &[&witness], 100_000)
			.unwrap_err();
		assert_eq!(failure.error.kind, ErrorKind::Type, "{body}");
		let location = failure.error.location.unwrap();
		assert_eq!(
			location.start as usize..location.end as usize,
			span.range(),
			"{body}"
		);
		assert_eq!(store.root_len(&witness).unwrap(), marks);
		assert!(executor.is_idle());
		store.root_release(witness).unwrap();
		assert_eq!(store.live_entries(), 0);
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn both_engines_preserve_short_circuit_and_single_rhs_evaluation() {
	let context = Context::with_default_modules().unwrap();
	for (body, expected, marks) in [
		("false && mark(w, 1)", false, 0),
		("true || mark(w, 1)", true, 0),
		("true && mark(w, true)", true, 1),
		("false || mark(w, false)", false, 1),
		("false && mark(w, 1); true", true, 0),
		("true || mark(w, 1); false", false, 0),
		("true && { return false; }; true", false, 0),
		("(true && mark(w, true)) || mark(w, true); true", true, 1),
		("true && (false || mark(w, true)); true", true, 1),
		("if true { true && mark(w, true); } true", true, 1),
		("for _ in 0..3 { false || mark(w, false); } true", true, 3),
		("(return true) && mark(w, 1); false", true, 0),
		("false || { return true; }; false", true, 0),
	] {
		let source =
			std::format!("pub fn target(w) {{ {body} }}\nfn mark(w, x) {{ w.push(0); x }}");
		let (mut stock, program) =
			super::differential::compile_unit(&context, "strict", &source, "target");
		let legacy_w = crate::to_value(crate::runtime::Vec::new()).unwrap();
		let value = stock.call(["target"], (legacy_w.clone(),)).unwrap();
		assert_eq!(
			crate::from_value::<bool>(value).unwrap(),
			expected,
			"{body}"
		);
		assert_eq!(
			legacy_w.borrow_ref::<crate::runtime::Vec>().unwrap().len(),
			marks
		);
		let mut store = Store::new(Limits::default()).unwrap();
		let witness = store.root_list(Type::Vec, &[]).unwrap();
		let mut executor = Executor::new().unwrap();
		let function = program.function(Hash::type_hash(["target"])).unwrap();
		let root = executor
			.invoke(&program, &mut store, function, &[&witness], 100_000)
			.unwrap();
		assert_eq!(
			store.root_scalar(&root).unwrap(),
			Some(crate::experimental_core::value::Scalar::Bool(expected))
		);
		assert_eq!(store.root_len(&witness).unwrap(), marks);
		store.root_release(root).unwrap();
		store.root_release(witness).unwrap();
		assert_eq!(store.live_entries(), 0);
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn no_else_instruction_counts() {
	let context = Context::with_default_modules().unwrap();
	for (body, expected) in [
		("if x {7}; 0", 5),
		("if x {7}else if y {8}; 0", 8),
		("if let Some(a)=x {a}; 0", 18),
		("if x {7}", 6),
		("if x {7}else if y {8}", 10),
		("if let Some(a)=x {a}", 19),
	] {
		let source = std::format!("pub fn target(x,y) {{ {body} }}");
		let (_, program) =
			super::differential::compile_unit(&context, "unused-no-else", &source, "target");
		let counts: std::vec::Vec<_> = (0..program.code.functions())
			.map(|i| {
				program
					.code
					.function(i as u32)
					.unwrap()
					.instructions()
					.len()
			})
			.collect();
		// Statement counts were measured on published 485babed; value-position
		// counts pin discard-then-unit lowering without a dead tail store.
		assert_eq!(counts, [expected], "{body}");
	}
}

#[test]
#[cfg_attr(
	miri,
	ignore = "full default-module frontend compilation; opt in explicitly"
)]
fn no_else_is_unit_and_effect_occurs_once() {
	let context = Context::with_default_modules().unwrap();
	for (body, marks) in [
		("if true {mark(w);7}", 1),
		("if false {mark(w);7}", 0),
		("if true {mark(w);7}else if true {mark(w);8}", 1),
		("if false {mark(w);7}else if true {mark(w);8}", 1),
		("if false {mark(w);7}else if false {mark(w);8}", 0),
		("if let Some(x)=Some(7) {mark(w);x}", 1),
		("if let Some(x)=None {mark(w);x}", 0),
		("if true {mark(w);[1,2,3]}", 1),
		("if let Some(x)=Some([1,2]) {mark(w);x}", 1),
	] {
		let source = std::format!("pub fn target(w){{{body}}} fn mark(w){{w.push(0);}}");
		let (mut stock, program) =
			super::differential::compile_unit(&context, "no-else", &source, "target");
		let legacy_w = crate::to_value(crate::runtime::Vec::new()).unwrap();
		let value = stock.call(["target"], (legacy_w.clone(),)).unwrap();
		crate::from_value::<()>(value).unwrap();
		assert_eq!(
			legacy_w.borrow_ref::<crate::runtime::Vec>().unwrap().len(),
			marks
		);
		let mut store = Store::new(Limits::default()).unwrap();
		let witness = store.root_list(Type::Vec, &[]).unwrap();
		let mut executor = Executor::new().unwrap();
		let function = program.function(Hash::type_hash(["target"])).unwrap();
		let root = executor
			.invoke(&program, &mut store, function, &[&witness], 100_000)
			.unwrap();
		assert_eq!(
			store.root_scalar(&root).unwrap(),
			Some(crate::experimental_core::value::Scalar::Unit),
			"{body}"
		);
		assert_eq!(store.root_len(&witness).unwrap(), marks);
		store.root_release(root).unwrap();
		store.root_release(witness).unwrap();
		assert_eq!(store.live_entries(), 0);
	}
}
