use super::*;
use crate::experimental_core::{
	arithmetic::Binary,
	code::{Builder, Function, Op},
	error::{ErrorKind, Location},
	execute::{Executor, ExecutorLimits},
	frontend::Program,
	heap::Store,
	value::{Scalar, Value},
};

fn program(arity: u32, slots: u32, ops: &[Op]) -> Program {
	let mut function = Function::new(slots, arity).unwrap();
	for (pc, op) in ops.iter().copied().enumerate() {
		function
			.push(
				op,
				Location {
					source: 0,
					revision: 1,
					start: pc as u32,
					end: pc as u32 + 1,
				},
			)
			.unwrap();
	}
	let mut builder = Builder::default();
	builder.push(function).unwrap();
	Program::for_diagnostics_test(builder.finish().unwrap())
}
fn axpy() -> Program {
	program(
		2,
		5,
		&[
			Op::Literal {
				out: 2,
				value: Value::I64(2),
			},
			Op::Binary {
				out: 3,
				op: Binary::Mul,
				lhs: 0,
				rhs: 2,
			},
			Op::Binary {
				out: 4,
				op: Binary::Add,
				lhs: 3,
				rhs: 1,
			},
			Op::Return { value: 4 },
		],
	)
}
fn options(work: usize, force_scalar: bool) -> ColumnOptions {
	ColumnOptions {
		work,
		executor_limits: ExecutorLimits::default(),
		force_scalar,
	}
}
fn i64_chunk(values: &[i64]) -> ColumnChunk<'_> {
	ColumnChunk::new(ColumnSlice::I64(values), None).unwrap()
}

#[test]
fn first_fault_is_row_order_not_operation_order() {
	let p = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	for force in [true, false] {
		let mut x = std::vec![1; 1030];
		let mut y = std::vec![0; 1030];
		// An add failure on the early row must beat a multiply failure later.
		x[900] = i64::MAX;
		y[3] = i64::MAX;
		let a = [i64_chunk(&x)];
		let b = [i64_chunk(&y)];
		let columns = [
			Column::new(ColumnType::I64, &a).unwrap(),
			Column::new(ColumnType::I64, &b).unwrap(),
		];
		assert!(matches!(
			plan.execute(&columns, options(10000, force), || false),
			ColumnOutcome::Replay {
				reason: ColumnRefusal::Arithmetic,
				fault: Some(Fault { row: 3, pc: 2 })
			}
		));
		let mut store = Store::new(Limits::default()).unwrap();
		let mut executor = Executor::new().unwrap();
		let a = store.root_i64(x[3]);
		let b = store.root_i64(y[3]);
		let error = executor
			.invoke(&p, &mut store, 0, &[&a, &b], 1)
			.unwrap_err();
		assert_eq!(error.error.kind, ErrorKind::Overflow);
		assert_eq!(error.error.location.unwrap().start, 2);
		assert_eq!(error.remaining_work, Some(0));
	}
}

#[test]
fn independent_chunks_null_bits_and_exact_fuel() {
	let p = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	let a = [
		ColumnChunk::new(
			ColumnSlice::I64(&[2, i64::MAX]),
			Some(BitSlice::new(&[0b0100], 2, 2).unwrap()),
		)
		.unwrap(),
		i64_chunk(&[]),
		i64_chunk(&[4, 5]),
	];
	let b = [i64_chunk(&[1]), i64_chunk(&[1, 1]), i64_chunk(&[1])];
	let cols = [
		Column::new(ColumnType::I64, &a).unwrap(),
		Column::new(ColumnType::I64, &b).unwrap(),
	];
	for force in [true, false] {
		for work in 0..7 {
			match plan.execute(&cols, options(work, force), || false) {
				ColumnOutcome::Complete(out) => {
					assert!(work >= 3 * plan.row_charge());
					let ColumnValues::I64(out_values) = out.values else {
						panic!("type")
					};
					assert_eq!(out_values, [5, 0, 9, 11]);
					assert_eq!(out.validity.unwrap(), [0b1101]);
					assert_eq!(out.remaining_work, work - 3 * plan.row_charge());
				}
				ColumnOutcome::Replay { reason, fault } => {
					assert!(work < 3 * plan.row_charge());
					assert_eq!(reason, ColumnRefusal::Fuel);
					assert_eq!(fault, None);
				}
			}
		}
	}
}

#[test]
fn composed_chain_copies_redefinitions_and_dead_faults() {
	let p = program(
		2,
		4,
		&[
			Op::Copy { out: 2, value: 0 },
			Op::Binary {
				out: 0,
				op: Binary::Sub,
				lhs: 0,
				rhs: 1,
			},
			Op::Binary {
				out: 3,
				op: Binary::Mul,
				lhs: 0,
				rhs: 1,
			},
			Op::Release { value: 0 },
			Op::Binary {
				out: 0,
				op: Binary::Add,
				lhs: 2,
				rhs: 3,
			},
			Op::Return { value: 0 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	assert!(plan.fast.is_none());
	assert!(plan.scratch_count < plan.nodes.len());
	let rows = if cfg!(miri) { 17 } else { 2051 };
	let x: Vec<_> = (0..rows).map(|v| v - 1025).collect();
	let y: Vec<_> = (0..rows).map(|v| v % 17 - 8).collect();
	let a = [i64_chunk(&x)];
	let b = [i64_chunk(&y)];
	let cols = [
		Column::new(ColumnType::I64, &a).unwrap(),
		Column::new(ColumnType::I64, &b).unwrap(),
	];
	let ColumnOutcome::Complete(out) = plan.execute(&cols, options(10000, true), || false) else {
		panic!("unexpected refusal")
	};
	let ColumnValues::I64(out) = out.values else {
		panic!("type")
	};
	let mut executor = Executor::new().unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	for (i, actual) in out.iter().enumerate() {
		let a = store.root_i64(x[i]);
		let b = store.root_i64(y[i]);
		let expected = executor.invoke(&p, &mut store, 0, &[&a, &b], 10).unwrap();
		assert_eq!(
			store.root_scalar(&expected).unwrap(),
			Some(Scalar::I64(*actual))
		);
		store.root_release(expected).unwrap();
	}
	let p = program(
		1,
		3,
		&[
			Op::Literal {
				out: 1,
				value: Value::I64(1),
			},
			Op::Binary {
				out: 2,
				op: Binary::Add,
				lhs: 0,
				rhs: 1,
			},
			Op::Return { value: 0 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	let a = [i64_chunk(&[i64::MAX])];
	let c = [Column::new(ColumnType::I64, &a).unwrap()];
	assert!(matches!(
		plan.execute(&c, options(10, false), || false),
		ColumnOutcome::Replay {
			reason: ColumnRefusal::Arithmetic,
			..
		}
	));
}

#[test]
fn bounds_and_budget_do_not_publish_partial_output() {
	assert!(BitSlice::new(&[0], usize::MAX, 2).is_err());
	assert!(BitSlice::new(&[0], 7, 2).is_err());
	assert!(ColumnChunk::new(
		ColumnSlice::I64(&[1]),
		Some(BitSlice::new(&[], 0, 0).unwrap())
	)
	.is_err());
	let p = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	let a = [i64_chunk(&[1; 2049])];
	let c = Column::new(ColumnType::I64, &a).unwrap();
	let mut calls = 0;
	assert!(matches!(
		plan.execute(&[c, c], options(3000, false), || {
			calls += 1;
			calls >= 3
		}),
		ColumnOutcome::Replay {
			reason: ColumnRefusal::Canceled,
			fault: None
		}
	));
	let b = [i64_chunk(&[1; 2])];
	assert!(matches!(
		plan.execute(
			&[c, Column::new(ColumnType::I64, &b).unwrap()],
			options(3000, false),
			|| false
		),
		ColumnOutcome::Replay {
			reason: ColumnRefusal::Bounds,
			..
		}
	));
}

#[test]
fn exact_identity_and_entry_types_are_sealed() {
	let p = axpy();
	let other = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	assert!(plan.matches(&p, 0));
	assert!(!plan.matches(&other, 0));
	assert!(PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64, ColumnType::F64],
		ColumnType::I64,
		Limits::default()
	)
	.is_err());
	assert!(PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::Bool,
		Limits::default()
	)
	.is_err());
	assert!(PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits {
			entries: 1,
			bytes: 1
		}
	)
	.is_err());
	let p = program(
		1,
		2,
		&[
			Op::Unary {
				out: 1,
				op: crate::experimental_core::arithmetic::Unary::Neg,
				value: 0,
			},
			Op::Return { value: 1 },
		],
	);
	assert!(matches!(
		PreparedColumn::prepare(
			&p,
			0,
			&[ColumnType::I64],
			ColumnType::I64,
			Limits::default()
		),
		Err(ColumnRefusal::Unsupported)
	));
}

#[test]
fn boolean_offsets_tails_and_independent_validity() {
	let p = program(
		1,
		2,
		&[
			Op::Unary {
				out: 1,
				op: crate::experimental_core::arithmetic::Unary::Not,
				value: 0,
			},
			Op::Return { value: 1 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::Bool],
		ColumnType::Bool,
		Limits::default(),
	)
	.unwrap();
	let values = [0xa5; 140];
	let validity = [0x69; 140];
	for force in [true, false] {
		for bit_offset in 0..8 {
			for validity_offset in 0..8 {
				// Miri retains every independent bit offset and the block tail;
				// native controls cover the full product of lengths as well.
				let lengths: &[usize] = if cfg!(miri) && (bit_offset != 0 || validity_offset != 0) {
					&[0, 7, 8]
				} else {
					&[0, 1, 7, 8, 31, 32, 1024, 1031]
				};
				for &len in lengths {
					let bits = BitSlice::new(&values, bit_offset, len).unwrap();
					let mask = BitSlice::new(&validity, validity_offset, len).unwrap();
					for validity in [None, Some(mask)] {
						let chunks = [ColumnChunk::new(ColumnSlice::Bool(bits), validity).unwrap()];
						let columns = [Column::new(ColumnType::Bool, &chunks).unwrap()];
						let ColumnOutcome::Complete(result) =
							plan.execute(&columns, options(2000, force), || false)
						else {
							panic!("refused")
						};
						let ColumnValues::Bool(output) = result.values else {
							panic!("type")
						};
						let output_bits = BitSlice::new(&output, 0, len).unwrap();
						let mut valid = 0;
						for row in 0..len {
							let present = validity.is_none_or(|v| v.bit(row));
							valid += usize::from(present);
							assert_eq!(output_bits.bit(row), present && !bits.bit(row));
						}
						assert_eq!(result.remaining_work, 2000 - valid * plan.row_charge());
						if !len.is_multiple_of(8) {
							assert_eq!(output.last().unwrap() >> (len % 8), 0);
						}
					}
				}
			}
		}
	}
}

#[test]
fn packed_boolean_chunks_fuel_and_canceled_publication() {
	let p = program(
		1,
		2,
		&[
			Op::Unary {
				out: 1,
				op: crate::experimental_core::arithmetic::Unary::Not,
				value: 0,
			},
			Op::Return { value: 1 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::Bool],
		ColumnType::Bool,
		Limits::default(),
	)
	.unwrap();
	let values = [0xa5; 520];
	// Empty chunks, independent byte offsets, tiny and multiple full blocks,
	// then a partial final byte. Unaligned interior boundaries take the fallback.
	for lengths in [[0, 8, 2048, 1031, 0], [0, 7, 2049, 1031, 0], [0; 5]] {
		let chunks = core::array::from_fn::<_, 5, _>(|i| {
			ColumnChunk::new(
				ColumnSlice::Bool(BitSlice::new(&values, i * 8, lengths[i]).unwrap()),
				None,
			)
			.unwrap()
		});
		let columns = [Column::new(ColumnType::Bool, &chunks).unwrap()];
		let len = columns[0].len();
		let charge = len * plan.row_charge();
		for force in [true, false] {
			if charge != 0 {
				assert!(matches!(
					plan.execute(&columns, options(charge - 1, force), || false),
					ColumnOutcome::Replay {
						reason: ColumnRefusal::Fuel,
						fault: None
					}
				));
			}
			let batch = plan
				.prepare_batch(&columns, options(charge, force), || false)
				.unwrap();
			let mut checks = 0;
			let ColumnOutcome::Complete(result) = batch.invoke(|| {
				checks += 1;
				false
			}) else {
				panic!("refused")
			};
			assert_eq!(result.remaining_work, 0);
			assert!(result.validity.is_none());
			let ColumnValues::Bool(out) = result.values else {
				panic!("type")
			};
			let output = BitSlice::new(&out, 0, len).unwrap();
			let mut row = 0;
			for chunk in chunks {
				let ColumnSlice::Bool(bits) = chunk.values else {
					unreachable!()
				};
				for i in 0..bits.len() {
					assert_eq!(output.get(row), bits.get(i).map(|v| !v));
					row += 1;
				}
			}
			if !len.is_multiple_of(8) {
				assert_eq!(out.last().unwrap() >> (len % 8), 0);
			}
			// Cancel at every actual block boundary, including the final check:
			// the consuming public API must never expose the private prefix.
			for stop in 1..=checks {
				let batch = plan
					.prepare_batch(&columns, options(charge, force), || false)
					.unwrap();
				let mut calls = 0;
				assert!(matches!(
					batch.invoke(|| {
						calls += 1;
						calls == stop
					}),
					ColumnOutcome::Replay {
						reason: ColumnRefusal::Canceled,
						fault: None
					}
				));
			}
		}
	}
	assert_eq!(values, [0xa5; 520]);
}

#[test]
fn all_numeric_primitives_and_floating_point_special_values() {
	let mut executor = Executor::new().unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	for ty in [ColumnType::I64, ColumnType::F64] {
		for op in [
			Binary::Add,
			Binary::Sub,
			Binary::Mul,
			Binary::Eq,
			Binary::Ne,
			Binary::Lt,
			Binary::Le,
			Binary::Gt,
			Binary::Ge,
		] {
			let result_type = if matches!(op, Binary::Add | Binary::Sub | Binary::Mul) {
				ty
			} else {
				ColumnType::Bool
			};
			let p = program(
				2,
				3,
				&[
					Op::Binary {
						out: 2,
						op,
						lhs: 0,
						rhs: 1,
					},
					Op::Return { value: 2 },
				],
			);
			let plan =
				PreparedColumn::prepare(&p, 0, &[ty; 2], result_type, Limits::default()).unwrap();
			let ints = [-20, -1, 0, 1, 37];
			let floats = [-0.0, 0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN];
			let chunk = [ColumnChunk::new(
				match ty {
					ColumnType::I64 => ColumnSlice::I64(&ints),
					_ => ColumnSlice::F64(&floats),
				},
				None,
			)
			.unwrap()];
			let column = Column::new(ty, &chunk).unwrap();
			for force in [true, false] {
				let ColumnOutcome::Complete(result) =
					plan.execute(&[column, column], options(10, force), || false)
				else {
					panic!("refused")
				};
				for row in 0..5 {
					let a = match ty {
						ColumnType::I64 => store.root_i64(ints[row]),
						_ => store.root_f64(floats[row]),
					};
					let oracle = executor.invoke(&p, &mut store, 0, &[&a, &a], 10).unwrap();
					match (&result.values, store.root_scalar(&oracle).unwrap().unwrap()) {
						(ColumnValues::I64(v), Scalar::I64(x)) => assert_eq!(v[row], x),
						(ColumnValues::F64(v), Scalar::F64(x)) if x.is_nan() => {
							assert!(v[row].is_nan())
						}
						(ColumnValues::F64(v), Scalar::F64(x)) => {
							assert_eq!(v[row].to_bits(), x.to_bits())
						}
						(ColumnValues::Bool(v), Scalar::Bool(x)) => {
							assert_eq!(v[row / 8] & (1 << (row % 8)) != 0, x)
						}
						_ => panic!("wrong result type"),
					}
					store.root_release(oracle).unwrap();
				}
			}
		}
	}
}

#[test]
fn finalized_long_path_fuel_matches_every_stopping_point() {
	let mut ops = std::vec![Op::Literal {
		out: 1,
		value: Value::I64(1)
	}];
	for _ in 0..520 {
		ops.push(Op::Binary {
			out: 0,
			op: Binary::Add,
			lhs: 0,
			rhs: 1,
		});
	}
	ops.push(Op::Return { value: 0 });
	let p = program(1, 2, &ops);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	assert!(plan.row_charge() > 1);
	let a = [i64_chunk(&[0])];
	let columns = [Column::new(ColumnType::I64, &a).unwrap()];
	let mut executor = Executor::new().unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	for work in 0..=plan.row_charge() + 1 {
		let input = store.root_i64(0);
		let scalar = executor.invoke(&p, &mut store, 0, &[&input], work);
		let actual = plan.execute(&columns, options(work, true), || false);
		match (scalar, actual) {
			(Ok(expected), ColumnOutcome::Complete(out)) => {
				assert_eq!(
					store.root_scalar(&expected).unwrap(),
					Some(Scalar::I64(520))
				);
				assert_eq!(
					out.remaining_work,
					crate::experimental_core::execute::take_remaining_work_for_test().unwrap()
				);
				store.root_release(expected).unwrap();
			}
			(
				Err(error),
				ColumnOutcome::Replay {
					reason: ColumnRefusal::Fuel,
					..
				},
			) => assert_eq!(error.remaining_work, Some(0)),
			_ => panic!("fuel differs at {work}"),
		}
	}
}

#[test]
fn preparation_memory_boundary_and_batch_drop_are_transactional() {
	let p = axpy();
	let inputs = [ColumnType::I64; 2];
	let baseline =
		PreparedColumn::prepare(&p, 0, &inputs, ColumnType::I64, Limits::default()).unwrap();
	let mut first = None;
	for bytes in 0..baseline.retained_bytes() + 2048 {
		match PreparedColumn::prepare(
			&p,
			0,
			&inputs,
			ColumnType::I64,
			Limits { entries: 1, bytes },
		) {
			Ok(plan) => {
				assert!(plan.retained_bytes() <= bytes);
				first.get_or_insert(bytes);
			}
			Err(reason) => {
				assert_eq!(reason, ColumnRefusal::Memory);
				assert!(first.is_none());
			}
		}
	}
	assert!(first.is_some());
	let values = [7; 4096];
	let a = [i64_chunk(&values)];
	let c = Column::new(ColumnType::I64, &a).unwrap();
	let columns = [c, c];
	let batch = baseline
		.prepare_batch(&columns, options(10000, false), || false)
		.unwrap();
	let bytes = batch.owned_bytes() + baseline.retained_bytes();
	drop(batch); // no fuel mutation and no publication; the same inputs can run.
	for limit in [bytes - 1, bytes, bytes + 1] {
		let mut plan =
			PreparedColumn::prepare(&p, 0, &inputs, ColumnType::I64, Limits::default()).unwrap();
		plan.limit = limit;
		let result = plan.prepare_batch(&columns, options(10000, true), || false);
		if limit < bytes {
			assert!(matches!(result, Err(ColumnRefusal::Memory)));
		} else {
			let batch = result.unwrap();
			assert_eq!(batch.owned_bytes() + plan.retained_bytes(), bytes);
			assert!(matches!(batch.invoke(|| false), ColumnOutcome::Complete(_)));
		}
	}
	assert_eq!(values, [7; 4096]);
}

#[test]
#[cfg_attr(
	miri,
	ignore = "Miri: std-module frontend compilation; hand-built verifier/lease controls run under Miri"
)]
fn actual_frontend_functions_admit_and_refuse_without_source_patterns() {
	use crate::{Context, Diagnostics, Hash, Source, Sources};
	for (body, types, result, admitted) in [
		(
			"pub fn main(x) { x + 1 }",
			&[ColumnType::I64][..],
			ColumnType::I64,
			true,
		),
		(
			"pub fn main(x,y) { x * 2 + y }",
			&[ColumnType::I64; 2][..],
			ColumnType::I64,
			true,
		),
		(
			"pub fn main(x) { let y = x + 2; y * y - x }",
			&[ColumnType::I64][..],
			ColumnType::I64,
			true,
		),
		(
			"pub fn main(x) { x + 1.0 }",
			&[ColumnType::F64][..],
			ColumnType::F64,
			true,
		),
		(
			"pub fn main(x) { !x }",
			&[ColumnType::Bool][..],
			ColumnType::Bool,
			true,
		),
		(
			"pub fn main(x) { if x > 0 { x } else { 0 } }",
			&[ColumnType::I64][..],
			ColumnType::I64,
			false,
		),
		(
			"pub fn main(x) { x / 2 }",
			&[ColumnType::I64][..],
			ColumnType::I64,
			false,
		),
		(
			"pub fn main(x) { x && true }",
			&[ColumnType::Bool][..],
			ColumnType::Bool,
			false,
		),
	] {
		let mut sources = Sources::new();
		sources.insert(Source::memory(body).unwrap()).unwrap();
		let p = crate::experimental_core::frontend::compile(
			&mut sources,
			&Context::with_default_modules().unwrap(),
			&mut Diagnostics::new(),
			1,
		)
		.unwrap();
		let id = p.function(Hash::type_hash(["main"])).unwrap();
		let plan = PreparedColumn::prepare(&p, id, types, result, Limits::default());
		assert_eq!(plan.is_ok(), admitted, "{body}: {plan:?}");
	}
}

#[test]
fn ck2_masks_chunks_offsets_and_every_fuel_stop() {
	let p = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	let len: usize = if cfg!(miri) { 67 } else { 1031 };
	for force in [true, false] {
		for offset in 0..8 {
			for pattern in 0..4 {
				let mut a_bits = std::vec![0; (len + offset).div_ceil(8)];
				let mut b_bits = std::vec![0; (len + 7 - offset).div_ceil(8)];
				let valid_a = |i: usize| match pattern {
					0 => false,
					1 => i.is_multiple_of(2),
					2 => i.wrapping_mul(7919) % 17 < 9,
					_ => i >= len / 2,
				};
				let valid_b = |i: usize| i % 11 != 3;
				for i in 0..len {
					a_bits[(i + offset) / 8] |= u8::from(valid_a(i)) << ((i + offset) % 8);
					b_bits[(i + 7 - offset) / 8] |= u8::from(valid_b(i)) << ((i + 7 - offset) % 8);
				}
				let x: Vec<_> = (0..len)
					.map(|i| {
						if valid_a(i) && valid_b(i) {
							i as i64
						} else {
							i64::MAX
						}
					})
					.collect();
				let y: Vec<_> = (0..len)
					.map(|i| {
						if valid_a(i) && valid_b(i) {
							1
						} else {
							i64::MAX
						}
					})
					.collect();
				let a_ends = [0, 1, 63.min(len), 64.min(len), len];
				let b_ends = [0, 7, 65.min(len), len];
				let a: Vec<_> = a_ends
					.windows(2)
					.map(|w| {
						ColumnChunk::new(
							ColumnSlice::I64(&x[w[0]..w[1]]),
							Some(BitSlice::new(&a_bits, offset + w[0], w[1] - w[0]).unwrap()),
						)
						.unwrap()
					})
					.collect();
				let b: Vec<_> = b_ends
					.windows(2)
					.map(|w| {
						ColumnChunk::new(
							ColumnSlice::I64(&y[w[0]..w[1]]),
							Some(BitSlice::new(&b_bits, 7 - offset + w[0], w[1] - w[0]).unwrap()),
						)
						.unwrap()
					})
					.collect();
				let columns = [
					Column::new(ColumnType::I64, &a).unwrap(),
					Column::new(ColumnType::I64, &b).unwrap(),
				];
				let valid = (0..len).filter(|&i| valid_a(i) && valid_b(i)).count();
				let charge = valid * plan.row_charge();
				// Every stopping fuel value for a bounded mixed-mask batch, plus
				// both sides of the exact charge for all larger layouts.
				let budgets: Vec<_> = if offset == 0 && pattern == 1 {
					(0..=charge + 1).collect()
				} else {
					std::vec![0, charge.saturating_sub(1), charge, charge + 1]
				};
				for work in budgets {
					match plan.execute(&columns, options(work, force), || false) {
						ColumnOutcome::Replay { reason, fault } => {
							assert!(work < charge);
							assert_eq!(reason, ColumnRefusal::Fuel);
							assert_eq!(fault, None);
						}
						ColumnOutcome::Complete(result) => {
							assert_eq!(result.remaining_work, work - charge);
							let ColumnValues::I64(values) = result.values else {
								panic!("type")
							};
							let bits =
								BitSlice::new(result.validity.as_ref().unwrap(), 0, len).unwrap();
							for (i, &value) in values.iter().enumerate() {
								let present = valid_a(i) && valid_b(i);
								assert_eq!(bits.bit(i), present);
								assert_eq!(value, if present { i as i64 * 2 + 1 } else { 0 });
							}
						}
					}
				}
			}
		}
	}
}

#[test]
fn ck2_null_constant_overflow_and_first_valid_fault() {
	// The overflowing expression does not depend on the input. Replacing null
	// inputs with zero cannot suppress this fault; flags themselves must be masked.
	let p = program(
		1,
		4,
		&[
			Op::Literal {
				out: 1,
				value: Value::I64(i64::MAX),
			},
			Op::Literal {
				out: 2,
				value: Value::I64(3),
			},
			Op::Binary {
				out: 3,
				op: Binary::Mul,
				lhs: 1,
				rhs: 2,
			},
			Op::Return { value: 3 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	for force in [true, false] {
		let x = [0; 67];
		for valid in 0..=67 {
			let mut bits = [0; 9];
			if valid < 67 {
				bits[valid / 8] = 1 << (valid % 8);
			}
			let chunks = [ColumnChunk::new(
				ColumnSlice::I64(&x),
				Some(BitSlice::new(&bits, 0, 67).unwrap()),
			)
			.unwrap()];
			let columns = [Column::new(ColumnType::I64, &chunks).unwrap()];
			match plan.execute(&columns, options(100, force), || false) {
				ColumnOutcome::Complete(out) => {
					assert_eq!(valid, 67);
					assert_eq!(out.remaining_work, 100);
					let ColumnValues::I64(out) = out.values else {
						panic!("type")
					};
					assert!(out.iter().all(|&v| v == 0));
				}
				ColumnOutcome::Replay { reason, fault } => {
					assert_eq!(reason, ColumnRefusal::Arithmetic);
					assert_eq!(fault, Some(Fault { row: valid, pc: 2 }));
				}
			}
		}
	}
}

#[test]
fn ck2_nullable_typed_pairs_and_general_multiply() {
	let mask_bytes = [0xa6; 10];
	let len = 67;
	for force in [true, false] {
		for ty in [ColumnType::I64, ColumnType::F64] {
			for op in [Binary::Add, Binary::Sub, Binary::Mul] {
				let p = program(
					2,
					3,
					&[
						Op::Binary {
							out: 2,
							op,
							lhs: 0,
							rhs: 1,
						},
						Op::Return { value: 2 },
					],
				);
				let plan = PreparedColumn::prepare(&p, 0, &[ty; 2], ty, Limits::default()).unwrap();
				for offset in 0..8 {
					let mask = BitSlice::new(&mask_bytes, offset, len).unwrap();
					let ints: Vec<_> = (0..len)
						.map(|i| if mask.bit(i) { i as i64 - 33 } else { i64::MIN })
						.collect();
					let floats: Vec<_> = (0..len)
						.map(|i| {
							if mask.bit(i) {
								(i as f64 - 33.0) / 4.0
							} else {
								f64::NAN
							}
						})
						.collect();
					let int_y = [-1; 67];
					let float_y = [0.125; 67];
					let a = [ColumnChunk::new(
						match ty {
							ColumnType::I64 => ColumnSlice::I64(&ints),
							_ => ColumnSlice::F64(&floats),
						},
						Some(mask),
					)
					.unwrap()];
					let b: Vec<_> = [0, 3, 64, 67]
						.windows(2)
						.map(|w| {
							ColumnChunk::new(
								match ty {
									ColumnType::I64 => ColumnSlice::I64(&int_y[w[0]..w[1]]),
									_ => ColumnSlice::F64(&float_y[w[0]..w[1]]),
								},
								None,
							)
							.unwrap()
						})
						.collect();
					let columns = [Column::new(ty, &a).unwrap(), Column::new(ty, &b).unwrap()];
					let ColumnOutcome::Complete(result) =
						plan.execute(&columns, options(1000, force), || false)
					else {
						panic!("nullable operation refused")
					};
					assert_eq!(
						result.remaining_work,
						1000 - (0..len).filter(|&i| mask.bit(i)).count() * plan.row_charge()
					);
					for i in 0..len {
						match &result.values {
							ColumnValues::I64(out) => {
								let expected = if !mask.bit(i) {
									0
								} else {
									match op {
										Binary::Add => ints[i] + int_y[i],
										Binary::Sub => ints[i] - int_y[i],
										Binary::Mul => ints[i] * int_y[i],
										_ => unreachable!(),
									}
								};
								assert_eq!(out[i], expected);
							}
							ColumnValues::F64(out) => {
								let expected = if !mask.bit(i) {
									0.0
								} else {
									match op {
										Binary::Add => floats[i] + float_y[i],
										Binary::Sub => floats[i] - float_y[i],
										Binary::Mul => floats[i] * float_y[i],
										_ => unreachable!(),
									}
								};
								assert_eq!(out[i].to_bits(), expected.to_bits());
							}
							_ => panic!("numeric result"),
						}
					}
				}
			}
		}
	}
}

#[test]
fn ck2_optimistic_flags_ignore_nulls_and_replay_first_valid_boundary_fault() {
	let p = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	let len: usize = 1031;
	for force in [true, false] {
		for random in [false, true] {
			for fault in [0, 63, 64, 1023, 1024, 1030] {
				for valid_fault in [false, true] {
					let mut bits = std::vec![0u8; len.div_ceil(8)];
					let mut x = std::vec![1; len];
					let y = std::vec![1; len];
					for (i, value) in x.iter_mut().enumerate() {
						let valid = if i == fault {
							valid_fault
						} else if random {
							i * 73 % 251 < 127
						} else {
							i % 2 == 0
						};
						bits[i / 8] |= u8::from(valid) << (i % 8);
						if !valid || i == fault {
							*value = i64::MAX;
						}
					}
					let validity = Some(BitSlice::new(&bits, 0, len).unwrap());
					let a = [ColumnChunk::new(ColumnSlice::I64(&x), validity).unwrap()];
					let b = [ColumnChunk::new(ColumnSlice::I64(&y), validity).unwrap()];
					let columns = [
						Column::new(ColumnType::I64, &a).unwrap(),
						Column::new(ColumnType::I64, &b).unwrap(),
					];
					let work = 100000;
					match plan.execute(&columns, options(work, force), || false) {
						ColumnOutcome::Replay {
							reason,
							fault: actual,
						} => {
							assert!(valid_fault);
							assert_eq!(reason, ColumnRefusal::Arithmetic);
							assert_eq!(actual, Some(Fault { row: fault, pc: 1 }));
						}
						ColumnOutcome::Complete(result) => {
							assert!(!valid_fault);
							let valid = bits.iter().map(|b| b.count_ones() as usize).sum::<usize>();
							assert_eq!(result.remaining_work, work - valid * plan.row_charge());
							let ColumnValues::I64(values) = result.values else {
								panic!("type")
							};
							for (i, value) in values.iter().enumerate() {
								assert_eq!(
									*value,
									if bits[i / 8] >> (i % 8) & 1 != 0 {
										3
									} else {
										0
									}
								);
							}
						}
					}
				}
			}
		}
	}
}

#[test]
fn borrowed_operands_repeated_input_and_result_used_later() {
	// The returned value is also read by a later operation: Output must remain
	// an immutable operand without moving instruction order or hiding faults.
	let p = program(
		1,
		3,
		&[
			Op::Binary {
				out: 1,
				op: Binary::Add,
				lhs: 0,
				rhs: 0,
			},
			Op::Binary {
				out: 2,
				op: Binary::Mul,
				lhs: 1,
				rhs: 0,
			},
			Op::Return { value: 1 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	assert!(matches!(plan.nodes[0].operand, plan::Operand::Input(0)));
	assert!(matches!(
		plan.nodes[plan.result].operand,
		plan::Operand::Output
	));
	assert_eq!(plan.scratch_count, 1);
	for force in [true, false] {
		for masked in [false, true] {
			let x = [2, 3, -4, 5, 6];
			let bits = [0b11110101]; // dirty padding deliberately ignored
			let chunks = [ColumnChunk::new(
				ColumnSlice::I64(&x),
				if masked {
					Some(BitSlice::new(&bits, 0, 5).unwrap())
				} else {
					None
				},
			)
			.unwrap()];
			let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
			let ColumnOutcome::Complete(out) = plan.execute(&cols, options(1000, force), || false)
			else {
				panic!("borrowed result")
			};
			assert_eq!(out.valid_rows, if masked { 3 } else { 5 });
			assert_eq!(out.remaining_work, 1000 - plan.row_charge * out.valid_rows);
			let ColumnValues::I64(values) = out.values else {
				panic!("type")
			};
			assert_eq!(
				values,
				if masked {
					vec![4, 0, -8, 0, 12]
				} else {
					vec![4, 6, -8, 10, 12]
				}
			);
		}
		let x = [i64::MAX / 3];
		let chunks = [i64_chunk(&x)];
		let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
		assert!(matches!(
			plan.execute(&cols, options(1000, force), || false),
			ColumnOutcome::Replay {
				reason: ColumnRefusal::Arithmetic,
				fault: Some(Fault { row: 0, pc: 1 })
			}
		));
	}
}

#[test]
fn borrowed_identity_constant_and_bitmap_count() {
	for constant in [false, true] {
		let ops = if constant {
			vec![
				Op::Literal {
					out: 1,
					value: Value::I64(-7),
				},
				Op::Return { value: 1 },
			]
		} else {
			vec![Op::Return { value: 0 }]
		};
		let p = program(1, 2, &ops);
		let plan = PreparedColumn::prepare(
			&p,
			0,
			&[ColumnType::I64],
			ColumnType::I64,
			Limits::default(),
		)
		.unwrap();
		assert_eq!(plan.scratch_count, 0);
		for offset in 0..8 {
			for len in [0usize, 1, 7, 63, 64, 65, 129] {
				let values: Vec<_> = (0..len).map(|i| i as i64 - 30).collect();
				let bits = vec![0xa5; (offset + len).div_ceil(8)];
				let mask = BitSlice::new(&bits, offset, len).unwrap();
				let chunks = [ColumnChunk::new(ColumnSlice::I64(&values), Some(mask)).unwrap()];
				let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
				let ColumnOutcome::Complete(out) =
					plan.execute(&cols, options(10000, true), || false)
				else {
					panic!("identity/constant")
				};
				assert_eq!(
					out.valid_rows,
					(0..len).filter(|&i| mask.get(i).unwrap()).count()
				);
				let ColumnValues::I64(output) = out.values else {
					panic!("type")
				};
				for i in 0..len {
					assert_eq!(
						output[i],
						if !mask.get(i).unwrap() {
							0
						} else if constant {
							-7
						} else {
							values[i]
						}
					);
				}
			}
		}
	}
}

#[test]
fn borrowed_constant_folding_retains_fuel_and_overflow_pc() {
	for (a, b, overflow) in [(3, 4, false), (i64::MAX, 1, true)] {
		let p = program(
			1,
			4,
			&[
				Op::Literal {
					out: 1,
					value: Value::I64(a),
				},
				Op::Literal {
					out: 2,
					value: Value::I64(b),
				},
				Op::Binary {
					out: 3,
					op: Binary::Add,
					lhs: 1,
					rhs: 2,
				},
				Op::Return { value: 3 },
			],
		);
		let plan = PreparedColumn::prepare(
			&p,
			0,
			&[ColumnType::I64],
			ColumnType::I64,
			Limits::default(),
		)
		.unwrap();
		assert_eq!(
			matches!(plan.nodes[plan.result].step, plan::Step::Constant(_)),
			!overflow
		);
		for valid in [false, true] {
			let bits = [u8::from(valid)];
			let chunks = [ColumnChunk::new(
				ColumnSlice::I64(&[0]),
				Some(BitSlice::new(&bits, 0, 1).unwrap()),
			)
			.unwrap()];
			let columns = [Column::new(ColumnType::I64, &chunks).unwrap()];
			match plan.execute(&columns, options(100, true), || false) {
				ColumnOutcome::Complete(out) => {
					assert!(!overflow || !valid);
					assert_eq!(out.valid_rows, usize::from(valid));
					assert_eq!(
						out.remaining_work,
						100 - usize::from(valid) * plan.row_charge
					);
					let ColumnValues::I64(v) = out.values else {
						panic!("type")
					};
					assert_eq!(v, [if valid { 7 } else { 0 }]);
				}
				ColumnOutcome::Replay { reason, fault } => {
					assert!(overflow && valid);
					assert_eq!(reason, ColumnRefusal::Arithmetic);
					assert_eq!(fault, Some(Fault { row: 0, pc: 2 }));
				}
			}
		}
	}
}
