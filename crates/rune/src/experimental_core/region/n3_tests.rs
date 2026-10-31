use super::budget::Budget;
use super::commit::{self, Refused, ROOM_FOR_TEST};
use super::tests::{bin, build, compile, lit, loc, main_of, Arg};
use super::{analyze, FunctionContext, RegionRefusal, Scalar, TRANSFER_FACTOR};
use crate::experimental_core::arithmetic::Binary;
use crate::experimental_core::code::{Function, Op};
use crate::experimental_core::heap::{Limits, Store};
use crate::experimental_core::value::{Type, Value};

pub(crate) const INTEGER: &str = "fn sum(values) { let total=0; for value in values { total+=value; } total } pub fn main(x) { sum([x,1,-1]) }";
pub(crate) const BOOLEAN: &str = "fn count(values) { let total=0; for value in values { if value { total+=1; } } total } pub fn main(x) { count([x<0,false,true,x==0]) }";
pub(crate) const I64_BENCH: &str = include_str!("fixtures/vec_iter_i64.rn");
pub(crate) const BOOL_BENCH: &str = include_str!("fixtures/vec_iter_bool.rn");

#[test]
fn n3_reachability_keeps_live_return_adjacent_side_entries() {
	for live in [false, true] {
		let mut f = Function::new(3, 1).unwrap();
		lit(&mut f, 1, Value::I64(0), 0);
		f.push(
			Op::Jump {
				target: if live { 3 } else { 4 },
			},
			loc(1),
		)
		.unwrap();
		f.push(Op::Return { value: 1 }, loc(2)).unwrap();
		f.push(Op::Jump { target: 6 }, loc(3)).unwrap();
		bin(&mut f, 2, Binary::Lt, 1, 0, 4);
		f.push(
			Op::Branch {
				condition: 2,
				yes: 6,
				no: 9,
			},
			loc(5),
		)
		.unwrap();
		lit(&mut f, 2, Value::I64(1), 6);
		bin(&mut f, 1, Binary::Add, 1, 2, 7);
		f.push(Op::Jump { target: 4 }, loc(8)).unwrap();
		f.push(Op::Return { value: 1 }, loc(9)).unwrap();
		let code = build(vec![f]);
		let regions = analyze(&code, 0).unwrap();
		if live {
			assert_eq!(
				regions[0].1.as_ref().err(),
				Some(&RegionRefusal::SideEntry { pc: 6 })
			);
		} else {
			assert!(regions[0].1.is_ok());
		}
	}
}

#[test]
#[cfg_attr(miri, ignore = "Miri: compiles Rune source")]
fn n3_shared_rows_still_refuses_heap_search() {
	let p = compile(include_str!("fixtures/n3_shared_rows.rn"));
	let regions = analyze(&p.code, 0).unwrap();
	assert_eq!(regions.len(), 1);
	assert!(matches!(
		regions[0].1,
		Err(RegionRefusal::CollectionSlot { pc: 1 })
	));
}

#[test]
#[cfg_attr(miri, ignore = "Miri: source compilation and every-fuel sweep")]
fn n3_scalar_vectors_full_state_every_fuel() {
	for text in [
		INTEGER,
		BOOLEAN,
		"fn sum(v) { let n=0; for x in v { n+=x; } n } pub fn main(x) { sum([]) }",
		"fn sum(v) { let n=0; for x in v { n+=x; } n } pub fn main(x) { sum([x,true]) }",
	] {
		let p = compile(text);
		let stats = super::p2b_tests::sweep_inputs(
			&p.code,
			main_of(&p),
			&[
				vec![Arg::I(0)],
				vec![Arg::I(i64::MAX)],
				vec![Arg::I(i64::MIN)],
			],
		);
		if !text.contains("[x,true]") {
			assert!(stats.entries > 0, "{text}");
		}
	}
}

#[test]
#[cfg_attr(miri, ignore = "Miri: compiles Rune source")]
fn n3_none_preserves_output_and_invalid_exit_rolls_back_cursor() {
	let p =
		compile("pub fn main(values) { let total=0; for value in values { total+=value; } total }");
	let main = main_of(&p);
	let budget = Budget::unlimited();
	let ctx = FunctionContext::new(&p.code, main, &budget, TRANSFER_FACTOR).unwrap();
	let (region, _) = ctx.analyze_vector(2, Scalar::I64).unwrap().unwrap();
	let iterator_slot = region.iter.unwrap().slot as usize;
	for items in [&[][..], &[Value::I64(2), Value::I64(3)][..]] {
		let mut store = Store::new(Limits::default()).unwrap();
		let values = store.list(Type::Vec, items).unwrap();
		let iterator = store.iterator(values).unwrap();
		let mut window = vec![Value::Unit; p.code.function(main).unwrap().slots as usize];
		window[0] = values;
		window[1] = Value::I64(0);
		window[3] = Value::I64(77);
		window[iterator_slot] = iterator;
		let before = store.state_for_test();
		let old_window = window.clone();
		assert_eq!(
			commit::enter_collection(
				&region,
				&mut window,
				&mut store,
				true,
				20,
				ROOM_FOR_TEST,
				|_, _, _, _, _| panic!("cancel must refuse before snapshot or native call")
			)
			.unwrap(),
			Err(Refused::Cancel)
		);
		assert_eq!(window, old_window);
		assert_eq!(store.state_for_test(), before);
		let outcome = commit::enter_collection(
			&region,
			&mut window,
			&mut store,
			false,
			20,
			ROOM_FOR_TEST,
			|r, b, e, snapshot, len| {
				let status = super::simulate::simulate_collection(r, b, e, snapshot, len);
				e.pc = u32::MAX;
				status
			},
		);
		assert!(matches!(outcome, Ok(Err(Refused::InvalidExit))));
		assert_eq!(window, old_window);
		assert_eq!(store.state_for_test(), before);
		if items.is_empty() {
			let outcome = commit::enter_collection(
				&region,
				&mut window,
				&mut store,
				false,
				20,
				ROOM_FOR_TEST,
				super::simulate::simulate_collection,
			)
			.unwrap()
			.unwrap();
			assert_eq!(outcome.pc, 5);
			assert_eq!(window[3], Value::I64(77));
			assert_eq!(store.state_for_test(), before);
		}
		store.release(iterator).unwrap();
		store.release(values).unwrap();
		assert_eq!(store.live_entries(), 0);
	}
}
