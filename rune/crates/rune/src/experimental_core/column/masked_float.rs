//! Write-only sibling for existing masked fast F64 shapes.
use super::*;
use crate::experimental_column_primitives::{Engine, FloatOp, FloatRhs};

pub(super) fn eligible(plan: &PreparedColumn, masked: bool) -> bool {
	masked
		&& matches!(
			plan.fast,
			Some(
				Fast::F64Add(..)
					| Fast::F64PairAdd(..)
					| Fast::F64PairSub(..)
					| Fast::F64PairMul(..)
			)
		)
}

pub(super) fn run(
	batch: flat::FlatBatch<'_, '_>,
	canceled: &mut impl FnMut() -> bool,
) -> core::result::Result<ColumnResult, (ColumnRefusal, Option<Fault>)> {
	let flat::FlatBatch {
		plan,
		columns,
		options,
		values,
		validity,
		len,
		charge,
		valid_rows,
		scratch: _,
		row_values: _,
	} = batch;
	assert!(eligible(plan, validity.is_some()));
	let Output::F64(mut output) = values else {
		unreachable!("masked F64 owner");
	};
	let validity_bits = BitSlice::new(validity.as_ref().expect("masked validity"), 0, len)
		.expect("prepared validity");
	let engine = Engine::new(options.force_scalar);
	let mut cursors = [Cursor::default(); 3];
	let mut row = 0;
	let mut used_simd = false;
	while row < len {
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		let mut count = (len - row).min(BLOCK);
		let mut inputs = [ColumnSlice::I64(&[]); 3];
		let mut offsets = [0; 3];
		for (i, column) in columns.iter().enumerate() {
			let chunk = cursors[i].current(*column).expect("verified length");
			count = count.min(chunk.values.len() - cursors[i].offset);
			inputs[i] = chunk.values;
			offsets[i] = cursors[i].offset;
		}
		let nwords = count.div_ceil(64);
		let mut masks = [0; BLOCK / 64];
		validity_bits.read_words(row, count, &mut masks[..nwords]);
		if masks[..nwords].iter().all(|&m| m == 0) {
			// The initialized executor skips these blocks. A write-only owner
			// must instead cover them explicitly before final publication.
			output.zero_masked_block(count);
		} else {
			let (op, i, j, constant) = match plan.fast.expect("fast F64") {
				Fast::F64Add(i, c) => (FloatOp::Add, i, i, Some(c)),
				fast => {
					let (op, i, j) = float_operation(fast);
					(op, i, j, None)
				}
			};
			let (ColumnSlice::F64(x), ColumnSlice::F64(y)) = (inputs[i], inputs[j]) else {
				unreachable!("F64 inputs");
			};
			let x = &x[offsets[i]..offsets[i] + count];
			let y = &y[offsets[j]..offsets[j] + count];
			let rhs = match constant {
				Some(c) => FloatRhs::AddConstant(c),
				None => FloatRhs::Column(op, y),
			};
			output.masked_block(engine, x, rhs, &masks[..nwords]);
			used_simd |= engine.avx2();
		}
		row += count;
		for cursor in &mut cursors[..plan.arity] {
			cursor.offset += count;
		}
	}
	if canceled() {
		return Err((ColumnRefusal::Canceled, None));
	}
	Ok(ColumnResult {
		values: ColumnValues::F64(output.finish()),
		validity,
		len,
		valid_rows,
		remaining_work: options.work - charge,
		path: if used_simd { Path::Avx2 } else { Path::Scalar },
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::experimental_core::{
		arithmetic::Binary,
		code::{Builder, Function, Op},
		error::Location,
		frontend::Program,
		value::Value,
	};

	fn program(op: Binary, constant: bool) -> Program {
		let mut f = Function::new(3, if constant { 1 } else { 2 }).unwrap();
		let mut ops = Vec::new();
		if constant {
			ops.push(Op::Literal {
				out: 1,
				value: Value::F64(1.0),
			});
		}
		ops.push(Op::Binary {
			out: 2,
			op,
			lhs: 0,
			rhs: 1,
		});
		ops.push(Op::Return { value: 2 });
		for (pc, op) in ops.into_iter().enumerate() {
			f.push(
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
		let mut b = Builder::default();
		b.push(f).unwrap();
		Program::for_diagnostics_test(b.finish().unwrap())
	}

	#[test]
	fn masked_write_only_parity_limits_and_cancellation() {
		for (op, constant) in [
			(Binary::Add, true),
			(Binary::Add, false),
			(Binary::Sub, false),
			(Binary::Mul, false),
		] {
			let p = program(op, constant);
			let types = vec![ColumnType::F64; if constant { 1 } else { 2 }];
			let mut plan = PreparedColumn::prepare(
				&p,
				0,
				&types,
				ColumnType::F64,
				super::super::super::Limits::default(),
			)
			.unwrap();
			for n in [0, 1, 63, 64, 65, 1023, 1024, 1025] {
				let bit_patterns = [
					0,
					1 << 63,
					1,
					0x7ff0000000000000,
					0xfff0000000000000,
					0x7ff8000000000123,
					0xfff8000000000456,
					0x7ff0000000000123,
				];
				let x: Vec<_> = (0..n + 3)
					.map(|i| f64::from_bits(bit_patterns[i % bit_patterns.len()]))
					.collect();
				let y = vec![2.0; n + 3];
				for mask in [0, 255, 0x55, 0x93] {
					let bits = vec![mask; (n + 10).div_ceil(8)];
					let cut = n / 2;
					let xs = [
						super::super::super::ColumnChunk::new(
							ColumnSlice::F64(&x[1..cut + 1]),
							Some(BitSlice::new(&bits, 3, cut).unwrap()),
						)
						.unwrap(),
						super::super::super::ColumnChunk::new(
							ColumnSlice::F64(&x[cut + 1..n + 1]),
							Some(BitSlice::new(&bits, 3 + cut, n - cut).unwrap()),
						)
						.unwrap(),
					];
					let ys = [super::super::super::ColumnChunk::new(
						ColumnSlice::F64(&y[2..n + 2]),
						None,
					)
					.unwrap()];
					let mut columns = vec![Column::new(ColumnType::F64, &xs).unwrap()];
					if !constant {
						columns.push(Column::new(ColumnType::F64, &ys).unwrap());
					}
					for force_scalar in [true, false] {
						let options = ColumnOptions {
							work: 100000,
							executor_limits: ExecutorLimits::default(),
							force_scalar,
						};
						let mut states = Vec::new();
						for initialized in [true, false] {
							let token = plan.prepare_batch(&columns, options, || false).unwrap();
							let BatchKind::Flat(b) = token.inner else {
								panic!("route");
							};
							let result = if initialized {
								PreparedBatch {
									plan: b.plan,
									columns: b.columns,
									options: b.options,
									values: ColumnValues::F64(vec![0.0; n]),
									validity: b.validity,
									len: b.len,
									charge: b.charge,
									valid_rows: b.valid_rows,
									scratch: b.scratch,
									row_values: b.row_values,
								}
								.run_masked(&mut || false)
							} else {
								run(b, &mut || false)
							}
							.unwrap();
							let ColumnValues::F64(v) = result.values else {
								panic!("type");
							};
							for (i, v) in v.iter().enumerate() {
								if !BitSlice::new(&bits, 3, n).unwrap().bit(i) {
									assert_eq!(v.to_bits(), 0);
								}
							}
							states.push((
								v.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
								result.validity,
								result.valid_rows,
								result.remaining_work,
								result.path,
							));
						}
						assert_eq!(states[0], states[1]);
						let charge = 100000 - states[0].3;
						assert!(matches!(
							plan.execute(
								&columns,
								ColumnOptions {
									work: charge,
									..options
								},
								|| false
							),
							ColumnOutcome::Complete(_)
						));
						if charge > 0 {
							assert!(matches!(
								plan.execute(
									&columns,
									ColumnOptions {
										work: charge - 1,
										..options
									},
									|| false
								),
								ColumnOutcome::Replay {
									reason: ColumnRefusal::Fuel,
									..
								}
							));
						}
						let token = plan.prepare_batch(&columns, options, || false).unwrap();
						let bytes = token.owned_bytes() + plan.retained_bytes();
						drop(token);
						let old = plan.limit;
						plan.limit = bytes - 1;
						assert!(matches!(
							plan.execute(&columns, options, || false),
							ColumnOutcome::Replay {
								reason: ColumnRefusal::Memory,
								..
							}
						));
						plan.limit = old;
						for stop in [0, 1, 2] {
							let token = plan.prepare_batch(&columns, options, || false).unwrap();
							let mut calls = 0;
							let outcome = token.invoke(|| {
								let yes = calls >= stop;
								calls += 1;
								yes
							});
							if stop <= n.div_ceil(1024) {
								assert!(matches!(
									outcome,
									ColumnOutcome::Replay {
										reason: ColumnRefusal::Canceled,
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
