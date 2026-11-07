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

#[test]
fn borrowed_optimistic_null_overflow_in_later_node_and_output_reuse() {
	let p = program(
		1,
		4,
		&[
			Op::Literal {
				out: 1,
				value: Value::I64(3),
			},
			Op::Binary {
				out: 2,
				op: Binary::Add,
				lhs: 0,
				rhs: 0,
			},
			Op::Binary {
				out: 3,
				op: Binary::Mul,
				lhs: 2,
				rhs: 1,
			},
			Op::Return { value: 2 },
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
		for n in [1usize, 4, 63, 64, 65, 1030] {
			let mut x = vec![7; n];
			let mut bits = vec![0xff; (n + 3).div_ceil(8)];
			for i in (1..n).step_by(2) {
				x[i] = i64::MAX / 4; // doubling succeeds; later *3 overflows
				bits[(i + 3) / 8] &= !(1 << ((i + 3) % 8));
			}
			let chunks = [ColumnChunk::new(
				ColumnSlice::I64(&x),
				Some(BitSlice::new(&bits, 3, n).unwrap()),
			)
			.unwrap()];
			let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
			let ColumnOutcome::Complete(out) =
				plan.execute(&cols, options(100000, force), || false)
			else {
				panic!("null-only later overflow");
			};
			assert_eq!(out.remaining_work, 100000 - plan.row_charge * n.div_ceil(2));
			let ColumnValues::I64(values) = out.values else {
				panic!("type");
			};
			for (i, value) in values.iter().enumerate() {
				assert_eq!(*value, if i % 2 == 0 { 14 } else { 0 });
			}
		}
		let x = [7, i64::MAX / 4, i64::MAX];
		let bits = [0b101]; // row1 null; row2 valid add overflow must be reported
		let chunks = [ColumnChunk::new(
			ColumnSlice::I64(&x),
			Some(BitSlice::new(&bits, 0, 3).unwrap()),
		)
		.unwrap()];
		let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
		assert!(matches!(
			plan.execute(&cols, options(100000, force), || false),
			ColumnOutcome::Replay {
				reason: ColumnRefusal::Arithmetic,
				fault: Some(Fault { row: 2, pc: 1 })
			}
		));
	}
}

#[test]
fn borrowed_optimistic_constant_add_sub_replays_only_valid_faults() {
	for (op, reversed, constant, valid, invalid) in [
		(Binary::Add, false, 1, 7, i64::MAX),
		(Binary::Add, true, i64::MIN, 7, -1),
		(Binary::Sub, false, i64::MIN, -7, 1),
		(Binary::Sub, true, i64::MAX, 7, -1),
	] {
		let (lhs, rhs) = if reversed { (1, 0) } else { (0, 1) };
		let p = program(
			1,
			4,
			&[
				Op::Literal {
					out: 1,
					value: Value::I64(constant),
				},
				Op::Binary {
					out: 2,
					op,
					lhs,
					rhs,
				},
				// Read the returned Output again; zeroing must wait until completion.
				Op::Binary {
					out: 3,
					op: Binary::Sub,
					lhs: 2,
					rhs: 2,
				},
				Op::Return { value: 2 },
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
		let expected = match (op, reversed) {
			(Binary::Add, _) => valid + constant,
			(_, false) => valid - constant,
			(_, true) => constant - valid,
		};
		for force in [true, false] {
			for n in [4usize, 63, 64, 65, 1030] {
				let x: Vec<_> = (0..n)
					.map(|i| if i % 2 == 0 { valid } else { invalid })
					.collect();
				let mut bits = vec![0; (n + 3).div_ceil(8)];
				for i in (0..n).step_by(2) {
					bits[(i + 3) / 8] |= 1 << ((i + 3) % 8);
				}
				let chunks = [ColumnChunk::new(
					ColumnSlice::I64(&x),
					Some(BitSlice::new(&bits, 3, n).unwrap()),
				)
				.unwrap()];
				let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
				let ColumnOutcome::Complete(out) =
					plan.execute(&cols, options(100000, force), || false)
				else {
					panic!("null-only constant overflow");
				};
				assert_eq!(out.remaining_work, 100000 - plan.row_charge * n.div_ceil(2));
				let ColumnValues::I64(values) = out.values else {
					panic!("type");
				};
				for (i, &value) in values.iter().enumerate() {
					assert_eq!(value, if i % 2 == 0 { expected } else { 0 });
				}
			}
			let x = [valid, invalid, invalid];
			let bits = [0b101];
			let chunks = [ColumnChunk::new(
				ColumnSlice::I64(&x),
				Some(BitSlice::new(&bits, 0, 3).unwrap()),
			)
			.unwrap()];
			let cols = [Column::new(ColumnType::I64, &chunks).unwrap()];
			assert!(matches!(
				plan.execute(&cols, options(100000, force), || false),
				ColumnOutcome::Replay {
					reason: ColumnRefusal::Arithmetic,
					fault: Some(Fault { row: 2, pc: 1 })
				}
			));
		}
	}
}

#[test]
fn flat_output_sliced_single_chunk_tails_and_cancellation() {
	let p = axpy();
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::I64; 2],
		ColumnType::I64,
		Limits::default(),
	)
	.unwrap();
	for n in [0, 1, 3, 4, 5, 1023, 1024, 1025] {
		let mut x = std::vec![i64::MAX; n + 9];
		let mut y = std::vec![i64::MIN; n + 13];
		for i in 0..n {
			x[3 + i] = i as i64;
			y[7 + i] = -(i as i64);
		}
		// Different non-zero offsets and poisonous prefix/suffix catch incorrect
		// base-pointer use. ColumnSlice already carries the adjusted input slice.
		let a = [i64_chunk(&x[3..3 + n])];
		let b = [i64_chunk(&y[7..7 + n])];
		let columns = [
			Column::new(ColumnType::I64, &a).unwrap(),
			Column::new(ColumnType::I64, &b).unwrap(),
		];
		for force in [true, false] {
			let work = plan.row_charge() * n;
			let batch = plan
				.prepare_batch(&columns, options(work, force), || false)
				.unwrap();
			// Dropping an unstarted batch must not expose its output or consume work.
			drop(batch);
			let ColumnOutcome::Complete(result) =
				plan.execute(&columns, options(work, force), || false)
			else {
				panic!("flat sliced output")
			};
			assert_eq!(result.remaining_work, 0);
			assert_eq!(result.valid_rows, n);
			let ColumnValues::I64(values) = result.values else {
				panic!("type")
			};
			assert_eq!(
				values,
				(0..n).map(|i| i as i64).collect::<std::vec::Vec<_>>()
			);
			// Invoke checks before each block and once after all writes. Every
			// cancellation position, including finalization, must discard output.
			for stop in 0..=n.div_ceil(1024) {
				let batch = plan
					.prepare_batch(&columns, options(work, force), || false)
					.unwrap();
				let mut checks = 0;
				assert!(matches!(
					batch.invoke(|| {
						let hit = checks == stop;
						checks += 1;
						hit
					}),
					ColumnOutcome::Replay {
						reason: ColumnRefusal::Canceled,
						fault: None
					}
				));
			}
		}
	}
}

#[test]
fn boolean_plan_chunks_masks_and_consumers() {
	let left = [0xa5; 140];
	let right = [0x3c; 140];
	let masks = [[0xff; 140], [0; 140], [0x69; 140]];
	for op in [Binary::BitAnd, Binary::BitOr] {
		for consumer in [false, true] {
			let mut ops = vec![Op::Binary {
				out: 2,
				op,
				lhs: 0,
				rhs: 1,
			}];
			if consumer {
				ops.push(Op::Binary {
					out: 3,
					op: Binary::BitOr,
					lhs: 2,
					rhs: 0,
				});
			}
			ops.push(Op::Return {
				value: if consumer { 3 } else { 2 },
			});
			let p = program(2, 4, &ops);
			let plan = PreparedColumn::prepare(
				&p,
				0,
				&[ColumnType::Bool; 2],
				ColumnType::Bool,
				Limits::default(),
			)
			.unwrap();
			assert!(plan.boolean_plan);

			let lengths: &[usize] = if cfg!(miri) {
				&[0, 7, 65]
			} else {
				&[0, 1, 7, 8, 63, 64, 65, 1024, 1031]
			};
			for &len in lengths {
				for offset in [0, 3, 7] {
					for mode in 0..4 {
						let x = BitSlice::new(&left, offset, len).unwrap();
						let y = BitSlice::new(&right, 7 - offset, len).unwrap();
						let mx =
							(mode != 0).then(|| BitSlice::new(&masks[mode - 1], 1, len).unwrap());
						let my = (mode == 3).then(|| BitSlice::new(&masks[2], 4, len).unwrap());
						let cut = len / 3;
						let xc = [
							ColumnChunk::new(
								ColumnSlice::Bool(BitSlice::new(&left, offset, cut).unwrap()),
								mx.map(|_| BitSlice::new(&masks[mode - 1], 1, cut).unwrap()),
							)
							.unwrap(),
							ColumnChunk::new(
								ColumnSlice::Bool(
									BitSlice::new(&left, offset + cut, len - cut).unwrap(),
								),
								mx.map(|_| {
									BitSlice::new(&masks[mode - 1], 1 + cut, len - cut).unwrap()
								}),
							)
							.unwrap(),
						];
						let yc = [ColumnChunk::new(ColumnSlice::Bool(y), my).unwrap()];
						let columns = [
							Column::new(ColumnType::Bool, &xc).unwrap(),
							Column::new(ColumnType::Bool, &yc).unwrap(),
						];
						for same in [false, true] {
							let inputs = [columns[0], if same { columns[0] } else { columns[1] }];
							for force in [false, true] {
								let ColumnOutcome::Complete(result) =
									plan.execute(&inputs, options(100000, force), || false)
								else {
									panic!("refused");
								};
								let ColumnValues::Bool(values) = &result.values else {
									panic!("type");
								};
								let bits = BitSlice::new(values, 0, len).unwrap();
								let mut valid = 0;
								for i in 0..len {
									let present = mx.is_none_or(|m| m.bit(i))
										&& (same || my.is_none_or(|m| m.bit(i)));
									valid += usize::from(present);
									let a = x.bit(i);
									let b = if same { a } else { y.bit(i) };
									let expected = match op {
										Binary::BitAnd => a & b,
										Binary::BitOr => a | b,
										_ => unreachable!(),
									};
									assert_eq!(
										bits.bit(i),
										present && (expected || (consumer && a))
									);
									assert_eq!(
										result.validity.as_ref().is_none_or(|v| BitSlice::new(
											v, 0, len
										)
										.unwrap()
										.bit(i)),
										present
									);
								}
								assert_eq!(result.valid_rows, valid);
								assert_eq!(
									result.remaining_work,
									100000 - valid * plan.row_charge()
								);
								if !len.is_multiple_of(8) {
									assert_eq!(values[len / 8] >> (len % 8), 0);
								}
								assert!(matches!(
									plan.execute(&inputs, options(100000, force), || true),
									ColumnOutcome::Replay {
										reason: ColumnRefusal::Canceled,
										..
									}
								));
								if valid != 0 {
									assert!(matches!(
										plan.execute(
											&inputs,
											options(valid * plan.row_charge() - 1, force),
											|| false
										),
										ColumnOutcome::Replay {
											reason: ColumnRefusal::Fuel,
											..
										}
									));
								}
							}
						}
					}
				}
			}
		}
	}
}

#[test]
fn boolean_plan_constants_not_and_classification() {
	use crate::experimental_core::arithmetic::Unary;
	let variants = [
		// Constant source, NOT inside a packed chain, then a second consumer.
		vec![
			Op::Literal {
				out: 2,
				value: Value::Bool(true),
			},
			Op::Binary {
				out: 3,
				op: Binary::BitAnd,
				lhs: 0,
				rhs: 2,
			},
			Op::Unary {
				out: 4,
				op: Unary::Not,
				value: 3,
			},
			Op::Binary {
				out: 5,
				op: Binary::BitOr,
				lhs: 4,
				rhs: 1,
			},
			Op::Return { value: 5 },
		],
		// A result that is an input despite otherwise dead computed nodes.
		vec![
			Op::Binary {
				out: 2,
				op: Binary::BitOr,
				lhs: 0,
				rhs: 1,
			},
			Op::Return { value: 0 },
		],
		// Folded constant result still needs to be written and null-masked.
		vec![
			Op::Literal {
				out: 2,
				value: Value::Bool(true),
			},
			Op::Literal {
				out: 3,
				value: Value::Bool(false),
			},
			Op::Binary {
				out: 4,
				op: Binary::BitOr,
				lhs: 2,
				rhs: 3,
			},
			Op::Return { value: 4 },
		],
		// The output is produced before a later dead node reads it.
		vec![
			Op::Binary {
				out: 2,
				op: Binary::BitAnd,
				lhs: 0,
				rhs: 1,
			},
			Op::Unary {
				out: 3,
				op: Unary::Not,
				value: 2,
			},
			Op::Return { value: 2 },
		],
	];
	for ops in variants {
		let p = program(2, 6, &ops);
		let plan = PreparedColumn::prepare(
			&p,
			0,
			&[ColumnType::Bool; 2],
			ColumnType::Bool,
			Limits::default(),
		)
		.unwrap();
		assert!(plan.boolean_plan);
		let mut reference = PreparedColumn::prepare(
			&p,
			0,
			&[ColumnType::Bool; 2],
			ColumnType::Bool,
			Limits::default(),
		)
		.unwrap();
		reference.boolean_plan = false;
		reference.special = false;
		let lengths: &[usize] = if cfg!(miri) {
			&[7, 65]
		} else {
			&[0, 1, 7, 64, 65, 1024, 1031]
		};
		for &len in lengths {
			let values = [0xa5; 140];
			let other = [0x3c; 140];
			let masks = [0x69; 140];
			for masked in [false, true] {
				let validity = masked.then(|| BitSlice::new(&masks, 1, len).unwrap());
				let x = [ColumnChunk::new(
					ColumnSlice::Bool(BitSlice::new(&values, 3, len).unwrap()),
					validity,
				)
				.unwrap()];
				let y = [ColumnChunk::new(
					ColumnSlice::Bool(BitSlice::new(&other, 7, len).unwrap()),
					None,
				)
				.unwrap()];
				let columns = [
					Column::new(ColumnType::Bool, &x).unwrap(),
					Column::new(ColumnType::Bool, &y).unwrap(),
				];
				for force in [false, true] {
					let ColumnOutcome::Complete(a) =
						plan.execute(&columns, options(100000, force), || false)
					else {
						panic!("packed refusal")
					};
					let ColumnOutcome::Complete(b) =
						reference.execute(&columns, options(100000, force), || false)
					else {
						panic!("reference refusal")
					};
					let (ColumnValues::Bool(a_values), ColumnValues::Bool(b_values)) =
						(&a.values, &b.values)
					else {
						panic!("type")
					};
					assert_eq!(a_values, b_values);
					assert_eq!(a.validity, b.validity);
					assert_eq!(a.valid_rows, b.valid_rows);
					assert_eq!(a.remaining_work, b.remaining_work);
					assert_eq!(a.path, Path::Scalar);
				}
			}
		}
	}
	for op in [Binary::Eq, Binary::BitXor] {
		let p = program(
			2,
			4,
			&[
				Op::Binary {
					out: 2,
					op: Binary::BitAnd,
					lhs: 0,
					rhs: 1,
				},
				Op::Binary {
					out: 3,
					op,
					lhs: 2,
					rhs: 1,
				},
				Op::Return { value: 3 },
			],
		);
		assert!(
			!PreparedColumn::prepare(
				&p,
				0,
				&[ColumnType::Bool; 2],
				ColumnType::Bool,
				Limits::default()
			)
			.unwrap()
			.boolean_plan
		);
	}
	let p = program(
		1,
		2,
		&[
			Op::Unary {
				out: 1,
				op: Unary::Not,
				value: 0,
			},
			Op::Return { value: 1 },
		],
	);
	assert!(
		!PreparedColumn::prepare(
			&p,
			0,
			&[ColumnType::Bool],
			ColumnType::Bool,
			Limits::default()
		)
		.unwrap()
		.boolean_plan
	);
}

#[test]
fn boolean_plan_memory_and_mid_batch_cancel() {
	let p = program(
		2,
		4,
		&[
			Op::Binary {
				out: 2,
				op: Binary::BitAnd,
				lhs: 0,
				rhs: 1,
			},
			Op::Unary {
				out: 3,
				op: crate::experimental_core::arithmetic::Unary::Not,
				value: 2,
			},
			Op::Return { value: 3 },
		],
	);
	let mut plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::Bool; 2],
		ColumnType::Bool,
		Limits::default(),
	)
	.unwrap();
	assert!(plan.boolean_plan);
	let data = [0xa5; 257];
	let chunks = [ColumnChunk::new(
		ColumnSlice::Bool(BitSlice::new(&data, 0, 2051).unwrap()),
		None,
	)
	.unwrap()];
	let column = Column::new(ColumnType::Bool, &chunks).unwrap();
	let inputs = [column, column];
	let batch = plan
		.prepare_batch(&inputs, options(100000, true), || false)
		.unwrap();
	let bytes = batch.owned_bytes() + plan.retained_bytes();
	drop(batch);
	for limit in [bytes - 1, bytes, bytes + 1] {
		plan.limit = limit;
		let result = plan.prepare_batch(&inputs, options(100000, true), || false);
		if limit < bytes {
			assert!(matches!(result, Err(ColumnRefusal::Memory)));
		} else {
			let batch = result.unwrap();
			assert_eq!(batch.owned_bytes() + plan.retained_bytes(), bytes);
			let mut calls = 0;
			assert!(matches!(
				batch.invoke(|| {
					calls += 1;
					calls == 2
				}),
				ColumnOutcome::Replay {
					reason: ColumnRefusal::Canceled,
					fault: None
				}
			));
			assert_eq!(calls, 2);
		}
	}
}

#[test]
fn boolean_plan_invalid_payload_is_zero_at_unaligned_chunk_edges() {
	let p = program(
		2,
		3,
		&[
			Op::Binary {
				out: 2,
				op: Binary::BitAnd,
				lhs: 0,
				rhs: 1,
			},
			Op::Return { value: 2 },
		],
	);
	let plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::Bool; 2],
		ColumnType::Bool,
		Limits::default(),
	)
	.unwrap();
	assert!(plan.boolean_plan);
	let ones = [0xff; 140];
	let masks = [0x55; 140];
	let lengths: &[usize] = if cfg!(miri) { &[7, 65] } else { &[7, 65, 1031] };
	for &len in lengths {
		for offset in 0..8 {
			for cut in [1, 3, 7] {
				let cut = cut.min(len);
				let chunks = [
					ColumnChunk::new(
						ColumnSlice::Bool(BitSlice::new(&ones, offset, cut).unwrap()),
						Some(BitSlice::new(&masks, 1, cut).unwrap()),
					)
					.unwrap(),
					ColumnChunk::new(
						ColumnSlice::Bool(BitSlice::new(&ones, offset + cut, len - cut).unwrap()),
						Some(BitSlice::new(&masks, 1 + cut, len - cut).unwrap()),
					)
					.unwrap(),
				];
				let other = [ColumnChunk::new(
					ColumnSlice::Bool(BitSlice::new(&ones, 7 - offset, len).unwrap()),
					None,
				)
				.unwrap()];
				let columns = [
					Column::new(ColumnType::Bool, &chunks).unwrap(),
					Column::new(ColumnType::Bool, &other).unwrap(),
				];
				let ColumnOutcome::Complete(result) =
					plan.execute(&columns, options(100000, true), || false)
				else {
					panic!("refusal")
				};
				let ColumnValues::Bool(payload) = result.values else {
					panic!("type")
				};
				let output = BitSlice::new(&payload, 0, len).unwrap();
				let validity = BitSlice::new(&masks, 1, len).unwrap();
				// Inputs are all one, so every invalid zero proves the final
				// validity mask cleared payload rather than merely hiding it.
				for row in 0..len {
					assert_eq!(output.bit(row), validity.bit(row));
				}
				if !len.is_multiple_of(8) {
					assert_eq!(payload[len / 8] >> (len % 8), 0);
				}
			}
		}
	}
}

#[test]
fn f64_plan_bitwise_chunks_masks_and_output_reuse() {
	let bits = [
		0,
		1 << 63,
		1,
		0x8000000000000001,
		0x7ff0000000000000,
		0xfff0000000000000,
		0x7ff8000000000123,
		0xfff8000000000456,
		0x7ff0000000000123,
		0x3ff0000000000000,
		0x4000000000000000,
	];
	let len = if cfg!(miri) { 19 } else { 1031 };
	let x: Vec<_> = (0..len)
		.map(|i| f64::from_bits(bits[i % bits.len()]))
		.collect();
	let y: Vec<_> = (0..len)
		.map(|i| f64::from_bits(bits[(i / bits.len() + 3) % bits.len()]))
		.collect();
	let masks = vec![0x55; (len + 7).div_ceil(8)];
	for op in [Binary::Add, Binary::Sub, Binary::Mul] {
		for returned in [0, 2, 3] {
			let p = program(
				2,
				4,
				&[
					Op::Binary {
						out: 2,
						op,
						lhs: 0,
						rhs: 1,
					},
					Op::Binary {
						out: 3,
						op: Binary::Sub,
						lhs: 2,
						rhs: 1,
					},
					Op::Return { value: returned },
				],
			);
			let mut plan = PreparedColumn::prepare(
				&p,
				0,
				&[ColumnType::F64; 2],
				ColumnType::F64,
				Limits::default(),
			)
			.unwrap();
			assert!(plan.special);
			for masked in [false, true] {
				let chunks = [
					ColumnChunk::new(
						ColumnSlice::F64(&x[..3]),
						masked.then(|| BitSlice::new(&masks, 2, 3).unwrap()),
					)
					.unwrap(),
					ColumnChunk::new(
						ColumnSlice::F64(&x[3..]),
						masked.then(|| BitSlice::new(&masks, 5, len - 3).unwrap()),
					)
					.unwrap(),
				];
				let other = [ColumnChunk::new(ColumnSlice::F64(&y), None).unwrap()];
				let columns = [
					Column::new(ColumnType::F64, &chunks).unwrap(),
					Column::new(ColumnType::F64, &other).unwrap(),
				];
				let mut results = Vec::new();
				for enabled in [true, false] {
					plan.special = enabled;
					let ColumnOutcome::Complete(result) =
						plan.execute(&columns, options(100000, true), || false)
					else {
						panic!("refused");
					};
					let ColumnValues::F64(values) = result.values else {
						panic!("type");
					};
					results.push((
						values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
						result.validity,
						result.remaining_work,
						result.valid_rows,
					));
				}
				assert_eq!(
					results[0], results[1],
					"op {op:?}, result {returned}, masked {masked}"
				);
			}
		}
	}
}

#[test]
fn f64_plan_dying_slots_constants_and_limits() {
	let p = program(
		2,
		8,
		&[
			Op::Literal {
				out: 2,
				value: Value::F64(2.0),
			},
			Op::Literal {
				out: 3,
				value: Value::F64(3.0),
			},
			Op::Binary {
				out: 4,
				op: Binary::Mul,
				lhs: 0,
				rhs: 2,
			},
			Op::Binary {
				out: 5,
				op: Binary::Add,
				lhs: 4,
				rhs: 1,
			},
			Op::Binary {
				out: 6,
				op: Binary::Mul,
				lhs: 3,
				rhs: 5,
			},
			Op::Binary {
				out: 7,
				op: Binary::Sub,
				lhs: 6,
				rhs: 0,
			},
			Op::Return { value: 7 },
		],
	);
	let mut plan = PreparedColumn::prepare(
		&p,
		0,
		&[ColumnType::F64; 2],
		ColumnType::F64,
		Limits::default(),
	)
	.unwrap();
	assert!(plan.special);
	// Allocation assigns a destination BEFORE releasing the dying operands:
	// plan.rs interval loop assigns nodes[i].operand, then decrements uses.
	let mut slots = Vec::new();
	for node in &plan.nodes {
		if let plan::Operand::Slot(slot) = node.operand {
			if let plan::Step::Binary(_, a, b) = node.step {
				for index in [a, b] {
					if let plan::Operand::Slot(source) = plan.nodes[index].operand {
						assert_ne!(slot, source);
					}
				}
			}
			slots.push(slot);
		}
	}
	assert!(slots.len() > plan.scratch_count, "exercise slot recycling");
	for len in [0, 1, 7, 65, if cfg!(miri) { 71 } else { 2051 }] {
		let x = vec![1.25; len];
		let y = vec![-0.5; len];
		let a = vec![0x55; (len + 7).div_ceil(8)];
		let b = vec![0x33; (len + 7).div_ceil(8)];
		for masks in [false, true] {
			let xs = [ColumnChunk::new(
				ColumnSlice::F64(&x),
				masks.then(|| BitSlice::new(&a, 2, len).unwrap()),
			)
			.unwrap()];
			let ys = [ColumnChunk::new(
				ColumnSlice::F64(&y),
				masks.then(|| BitSlice::new(&b, 3, len).unwrap()),
			)
			.unwrap()];
			let columns = [
				Column::new(ColumnType::F64, &xs).unwrap(),
				Column::new(ColumnType::F64, &ys).unwrap(),
			];
			let mut states = Vec::new();
			for enabled in [true, false] {
				plan.special = enabled;
				let ColumnOutcome::Complete(r) =
					plan.execute(&columns, options(100000, true), || false)
				else {
					panic!("refusal");
				};
				let ColumnValues::F64(v) = r.values else {
					panic!("type");
				};
				states.push((
					v.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
					r.validity,
					r.valid_rows,
					r.remaining_work,
				));
			}
			assert_eq!(states[0], states[1]);
			plan.special = true;
			let charge = 100000 - states[0].3;
			assert!(matches!(
				plan.execute(&columns, options(charge, true), || false),
				ColumnOutcome::Complete(_)
			));
			if charge > 0 {
				assert!(matches!(
					plan.execute(&columns, options(charge - 1, true), || false),
					ColumnOutcome::Replay {
						reason: ColumnRefusal::Fuel,
						..
					}
				));
			}
			let token = plan
				.prepare_batch(&columns, options(100000, true), || false)
				.unwrap();
			let bytes = token.owned_bytes() + plan.retained_bytes();
			drop(token);
			let limit = plan.limit;
			for bound in [bytes - 1, bytes] {
				plan.limit = bound;
				assert_eq!(
					plan.prepare_batch(&columns, options(100000, true), || false)
						.is_ok(),
					bound == bytes
				);
			}
			plan.limit = limit;
			let token = plan
				.prepare_batch(&columns, options(100000, true), || false)
				.unwrap();
			let mut calls = 0;
			assert!(matches!(
				token.invoke(|| {
					calls += 1;
					calls == 1
				}),
				ColumnOutcome::Replay {
					reason: ColumnRefusal::Canceled,
					..
				}
			));
		}
	}
}
