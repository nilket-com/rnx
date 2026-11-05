use super::tests::{bin, build, lit, loc};
use super::tests::{compile, main_of, Arg};
use super::{analyze, commit, simulate, Region, Scalar};
use crate::experimental_core::{
	arithmetic::Binary,
	code::{Code, Function, Op},
	heap::{Limits, Store},
	value::{Type, Value},
};

fn append_code(boolean: bool) -> Code {
	let mut f = Function::new(5, 3).unwrap();
	bin(&mut f, 3, Binary::Lt, 1, 2, 0);
	f.push(
		Op::Branch {
			condition: 3,
			yes: 2,
			no: 6,
		},
		loc(1),
	)
	.unwrap();
	f.push(
		Op::Push {
			value: 0,
			new: if boolean { 3 } else { 1 },
		},
		loc(2),
	)
	.unwrap();
	lit(&mut f, 4, Value::I64(1), 3);
	f.push(
		Op::AssignBinary {
			out: 1,
			op: Binary::Add,
			lhs: 1,
			rhs: 4,
		},
		loc(4),
	)
	.unwrap();
	f.push(Op::Jump { target: 0 }, loc(5)).unwrap();
	f.push(Op::Return { value: 1 }, loc(6)).unwrap();
	build(vec![f])
}

#[derive(Debug, PartialEq)]
struct Observation {
	len: usize,
	bytes: usize,
	values: std::vec::Vec<Value>,
	error: Option<crate::experimental_core::error::Error>,
	pc: usize,
	fuel: usize,
	counter: Value,
}

/// Execute the actual Store::push and scalar operations of this hand-built loop.
/// A native exit resumes at its exact PC. The returned list remains live so cleanup
/// cannot hide a wrong prefix, capacity, charge or error.
fn run_prefix(
	region: &Region,
	bytes: usize,
	quota: usize,
	initial: usize,
	count: usize,
	mut native: Option<
		&mut dyn FnMut(&Region, &mut [i64], &mut super::abi::ExitRecord, &mut [i64], usize) -> u32,
	>,
) -> (Observation, usize) {
	let mut s = Store::new(Limits { entries: 16, bytes }).unwrap();
	let boolean = region.collection.unwrap().ty == Scalar::Bool;
	let seed: std::vec::Vec<_> = (0..initial)
		.map(|i| {
			if boolean {
				Value::Bool(false)
			} else {
				Value::I64(i as i64)
			}
		})
		.collect();
	let v = s.list(Type::Vec, &seed).unwrap();
	let mut w = [
		v,
		Value::I64(0),
		Value::I64(count as i64),
		Value::Unit,
		Value::Unit,
	];
	let mut pc = 0;
	let mut fuel = count + 2;
	let mut entries = 0;
	let error = rune_alloc::limit::with(quota, || loop {
		if pc == 0 {
			if let Some(run) = native.as_mut() {
				match commit::enter_append(
					region,
					&mut w,
					&mut s,
					false,
					fuel,
					commit::ROOM_FOR_TEST,
					|r, b, e, a, n| run(r, b, e, a, n),
				)
				.unwrap()
				{
					Ok(exit) => {
						entries += 1;
						pc = exit.pc;
						fuel = exit.fuel;
					}
					Err(commit::Refused::InvalidExit | commit::Refused::Window) => {
						panic!("invalid append record")
					}
					Err(_) => (),
				}
			}
		}
		match pc {
			0 => {
				assert!(fuel > 0);
				fuel -= 1;
				w[3] = Value::Bool(matches!(w[1], Value::I64(i) if i < count as i64));
				pc = 1;
			}
			1 => {
				pc = if w[3] == Value::Bool(true) { 2 } else { 6 };
			}
			2 => {
				if let Err(e) = s.push(v, if boolean { w[3] } else { w[1] }) {
					break Some(e);
				}
				pc = 3;
			}
			3 => {
				w[4] = Value::I64(1);
				pc = 4;
			}
			4 => {
				let Value::I64(i) = w[1] else { unreachable!() };
				w[1] = Value::I64(i + 1);
				pc = 5;
			}
			5 => pc = 0,
			6 => break None,
			_ => unreachable!(),
		}
	})
	.call();
	let len = s.len(v).unwrap();
	let values = (0..len).map(|i| s.get(v, i).unwrap()).collect();
	let result = Observation {
		len,
		bytes: s.payload_bytes(),
		values,
		error,
		pc,
		fuel,
		counter: w[1],
	};
	s.release(v).unwrap();
	(result, entries)
}

#[test]
fn n4_growth_thresholds_and_preparation_fallback() {
	for boolean in [false, true] {
		let code = append_code(boolean);
		let mut regions = analyze(&code, 0).unwrap();
		let region = regions.remove(0).1.unwrap();
		let mut runner = simulate::simulate_collection;
		for initial in [0usize, 1, 3, 8] {
			for bytes in [16, 32, 63, 64, 95, 96, 127, 128, 255, 256, 511, 512, 4096] {
				if initial.saturating_mul(16) > bytes && initial != 0 {
					continue;
				}
				for quota in [
					0,
					1,
					63,
					64,
					95,
					96,
					127,
					128,
					191,
					192,
					255,
					256,
					511,
					512,
					1 << 20,
				] {
					let plain = run_prefix(&region, bytes, quota, initial, 33, None).0;
					let (candidate, _) =
						run_prefix(&region, bytes, quota, initial, 33, Some(&mut runner));
					assert_eq!(
						plain, candidate,
						"boolean{boolean} initial{initial} bytes{bytes} quota{quota}"
					);
				}
			}
		}
		assert!(run_prefix(&region, 4096, 1 << 20, 0, 33, Some(&mut runner)).1 > 0);
	}
}

#[test]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
fn n4_native_growth_thresholds() {
	for boolean in [false, true] {
		let code = append_code(boolean);
		let region = analyze(&code, 0).unwrap().remove(0).1.unwrap();
		let compiled = crate::experimental_native::lower::compile(&region, 0).unwrap();
		let native = crate::experimental_native::boundary::NativeRegion::load(&compiled).unwrap();
		let mut runner =
			|_: &Region, b: &mut [i64], e: &mut super::abi::ExitRecord, a: &mut [i64], n| {
				native.call_collection(b, e, a, n).unwrap()
			};
		for bytes in 0..=512 {
			let plain = run_prefix(&region, bytes, 1 << 20, 0, 33, None).0;
			let candidate = run_prefix(&region, bytes, 1 << 20, 0, 33, Some(&mut runner)).0;
			assert_eq!(plain, candidate, "boolean{boolean} bytes{bytes}");
		}
		for quota in 0..=768 {
			let plain = run_prefix(&region, 4096, quota, 0, 33, None).0;
			let candidate = run_prefix(&region, 4096, quota, 0, 33, Some(&mut runner)).0;
			assert_eq!(plain, candidate, "boolean{boolean} quota{quota}");
		}
		assert!(run_prefix(&region, 4096, 1 << 20, 0, 33, Some(&mut runner)).1 > 0);
	}
}

#[test]
#[cfg_attr(miri, ignore = "Miri: million-element machine-code witness")]
fn n4_million_element_native_builder() {
	for boolean in [false, true] {
		let code = append_code(boolean);
		let region = analyze(&code, 0).unwrap().remove(0).1.unwrap();
		let compiled = crate::experimental_native::lower::compile(&region, 0).unwrap();
		let native = crate::experimental_native::boundary::NativeRegion::load(&compiled).unwrap();
		let mut runner =
			|_: &Region, b: &mut [i64], e: &mut super::abi::ExitRecord, a: &mut [i64], n| {
				native.call_collection(b, e, a, n).unwrap()
			};
		let plain = run_prefix(&region, 1 << 30, 1 << 30, 0, 1_000_000, None).0;
		let (candidate, entries) =
			run_prefix(&region, 1 << 30, 1 << 30, 0, 1_000_000, Some(&mut runner));
		assert_eq!(entries, 1);
		assert_eq!(candidate.len, 1_000_000);
		assert_eq!(candidate.bytes, 1_048_576 * 16);
		assert_eq!(plain, candidate);
	}
}

#[test]
fn n4_invalid_records_and_leases_publish_nothing() {
	let code = append_code(true);
	let region = analyze(&code, 0).unwrap().remove(0).1.unwrap();
	for mode in 0..9 {
		let mut s = Store::new(Limits::default()).unwrap();
		let v = s.list(Type::Vec, &[]).unwrap();
		let mut w = [v, Value::I64(0), Value::I64(8), Value::Unit, Value::Unit];
		let iterator = (mode == 6).then(|| s.iterator(v).unwrap());
		if mode == 7 {
			s.invalidate(v).unwrap();
		}
		if mode == 8 {
			s.close().unwrap();
		}
		let before = s.state_for_test();
		let old = w;
		let outcome = rune_alloc::limit::with(if mode == 5 { 0 } else { 1 << 20 }, || {
			commit::enter_append(
				&region,
				&mut w,
				&mut s,
				mode == 4,
				20,
				commit::ROOM_FOR_TEST,
				|r, b, e, a, n| {
					let status = simulate::simulate_collection(r, b, e, a, n);
					match mode {
						0 => e.pc = u32::MAX,
						1 => b[r.cells()] = -1,
						2 => b[r.cells()] = n as i64 + 1,
						3 => a[0] = 2,
						_ => panic!("entry should refuse"),
					}
					status
				},
			)
		})
		.call()
		.unwrap();
		assert!(outcome.is_err(), "mode{mode}");
		assert_eq!(w, old, "mode{mode}");
		assert_eq!(s.state_for_test(), before, "mode{mode}");
		if let Some(i) = iterator {
			s.release(i).unwrap();
		}
		if mode < 7 {
			s.release(v).unwrap();
		}
	}
}

#[test]
#[cfg_attr(miri, ignore = "Miri: source compilation and every-fuel sweep")]
fn n4_append_full_state_every_fuel() {
	for text in [
		"pub fn main(n) { let v=[]; for i in 0..n { v.push(i); } v.len() }",
		"pub fn main(n) { let v=[]; for i in 0..n { v.push(i==0); } v.len() }",
		"pub fn main(n) { let v=[]; let i=0; while i<n { v.push(i); i+=1; } v.len() }",
		"pub fn main(n) { let v=[n]; for i in 0..3 { v.push(i); v[i]=v[i]+1; } v.len() }",
		"pub fn main(n) { let v=[]; let alias=v; for i in 0..n { alias.push(i); } v.len() }",
		"pub fn main(n) { let v=[]; for i in 0..n { if i<2 { v.push(i); } v.push(i); } v.len() }",
		"pub fn main(n) { let v=[]; for i in 0..n { v.push(i); if i==2 { break; } } v.len() }",
		"pub fn main(n) { let v=[]; for i in 0..n { v.push(i); if i==2 { return v.len(); } } v.len() }",
		"pub fn main(n) { let v=[]; for i in 0..n { v.push(i); let x=v[-1]; } v.len() }",
	] {
		let p = compile(text);
		let stats = super::p2b_tests::sweep_inputs(
			&p.code,
			main_of(&p),
			&[vec![Arg::I(0)], vec![Arg::I(7)]],
		);
		assert!(stats.entries > 0, "{text}");
	}
}
