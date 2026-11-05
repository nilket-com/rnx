//! CK2 nullable execution. Kept outside the established flat traversal.
use super::super::lease::append_words;
use super::*;
use crate::experimental_column_primitives::{Engine, FloatOp, FloatRhs};

type Failure = (ColumnRefusal, Option<Fault>);

pub(super) fn intersect_validity(
	columns: &[Column<'_>],
	out: &mut [u8],
	len: usize,
	canceled: &mut impl FnMut() -> bool,
) -> core::result::Result<usize, ColumnRefusal> {
	let mut cursors = [Cursor::default(); 3];
	let mut row = 0;
	let mut valid = 0;
	while row < len {
		if canceled() {
			return Err(ColumnRefusal::Canceled);
		}
		let mut count = (len - row).min(BLOCK);
		let mut masks = [None; 3];
		for (i, column) in columns.iter().enumerate() {
			let chunk = cursors[i].current(*column).expect("verified length");
			count = count.min(chunk.values.len() - cursors[i].offset);
			masks[i] = chunk.validity;
		}
		let nwords = count.div_ceil(64);
		let mut words = [u64::MAX; BLOCK / 64];
		let mut next = [0; BLOCK / 64];
		let mut copied = false;
		for (mask, cursor) in masks[..columns.len()].iter().zip(&cursors) {
			if let Some(mask) = mask {
				if !copied {
					mask.read_words(cursor.offset, count, &mut words[..nwords]);
					copied = true;
				} else {
					mask.read_words(cursor.offset, count, &mut next[..nwords]);
					for (word, rhs) in words[..nwords].iter_mut().zip(&next) {
						*word &= rhs;
					}
				}
			}
		}
		if !count.is_multiple_of(64) {
			words[nwords - 1] &= low_mask(count % 64);
		}
		append_words(out, row, count, &words[..nwords]);
		valid += words[..nwords]
			.iter()
			.map(|v| v.count_ones() as usize)
			.sum::<usize>();
		row += count;
		for cursor in &mut cursors[..columns.len()] {
			cursor.offset += count;
		}
	}
	Ok(valid)
}

pub(super) fn float_operation(fast: Fast) -> (FloatOp, usize, usize) {
	match fast {
		Fast::F64PairAdd(a, b) => (FloatOp::Add, a, b),
		Fast::F64PairSub(a, b) => (FloatOp::Sub, a, b),
		Fast::F64PairMul(a, b) => (FloatOp::Mul, a, b),
		_ => unreachable!("float pair"),
	}
}

pub(super) fn copy_input(values: ColumnSlice<'_>, start: usize, out: &mut [u64]) {
	match values {
		ColumnSlice::I64(x) => {
			for (&x, out) in x[start..start + out.len()].iter().zip(out) {
				*out = x as u64;
			}
		}
		ColumnSlice::F64(x) => {
			for (&x, out) in x[start..start + out.len()].iter().zip(out) {
				*out = x.to_bits();
			}
		}
		ColumnSlice::Bool(x) => {
			for (i, out) in out.iter_mut().enumerate() {
				*out = u64::from(x.bit(start + i));
			}
		}
	}
}

pub(super) fn copy_output(input: &[u64], values: &mut ColumnValues, row: usize) {
	match values {
		ColumnValues::I64(out) => {
			for (&x, out) in input.iter().zip(&mut out[row..row + input.len()]) {
				*out = x as i64;
			}
		}
		ColumnValues::F64(out) => {
			for (&x, out) in input.iter().zip(&mut out[row..row + input.len()]) {
				*out = f64::from_bits(x);
			}
		}
		ColumnValues::Bool(out) => {
			for (i, &x) in input.iter().enumerate() {
				out[(row + i) / 8] |= (x as u8) << ((row + i) % 8);
			}
		}
	}
}

// The verifier allocates the destination before releasing either operand.
// Thus it is a disjoint BLOCK-sized range. Operands may equal each other.
// Express that proof through safe split_at_mut, never aliased raw slices.
pub(super) fn split_binary(
	slots: &mut [u64],
	a: usize,
	b: usize,
	out: usize,
	len: usize,
) -> (&[u64], &[u64], &mut [u64]) {
	assert!(len <= BLOCK && a != out && b != out);
	let (before, rest) = slots.split_at_mut(out);
	let (output, after) = rest.split_at_mut(BLOCK);
	let input = |index: usize| {
		if index < out {
			&before[index..index + len]
		} else {
			&after[index - out - BLOCK..index - out - BLOCK + len]
		}
	};
	(input(a), input(b), &mut output[..len])
}

pub(super) fn first_fault(
	plan: &PreparedColumn,
	inputs: &[ColumnSlice<'_>; 3],
	offsets: &[usize; 3],
	row: usize,
	count: usize,
	mask: Option<u64>,
	row_values: &mut [u64],
) -> Failure {
	for j in 0..count {
		if mask.is_some_and(|m| m >> j & 1 == 0) {
			continue;
		}
		for (i, node) in plan.nodes.iter().enumerate() {
			let (v, fault) = match node.step {
				Step::Input(a) => (input(inputs[a], offsets[a] + j), false),
				Step::Constant(v) => (v, false),
				Step::Not(a) => (u64::from(row_values[a] == 0), false),
				Step::Binary(op, a, b) => {
					binary(op, plan.nodes[a].ty, row_values[a], row_values[b])
				}
			};
			if fault {
				return (
					ColumnRefusal::Arithmetic,
					Some(Fault {
						row: row + j,
						pc: node.pc,
					}),
				);
			}
			row_values[i] = v;
		}
	}
	unreachable!("overflow flag has a valid scalar witness");
}

impl PreparedBatch<'_, '_> {
	#[inline(never)]
	pub(super) fn run_masked(
		self,
		canceled: &mut impl FnMut() -> bool,
	) -> core::result::Result<ColumnResult, Failure> {
		let Self {
			plan,
			columns,
			options,
			mut values,
			validity,
			len,
			charge,
			mut scratch,
			mut row_values,
		} = self;
		let validity_bits = BitSlice::new(validity.as_ref().expect("masked batch"), 0, len)
			.expect("allocated validity");
		let engine = Engine::new(options.force_scalar);
		let mut cursors = [Cursor::default(); 3];
		let mut path = Path::Scalar;
		let mut row = 0;
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
			if let (
				Some(
					fast @ (Fast::F64Add(..)
					| Fast::F64PairAdd(..)
					| Fast::F64PairSub(..)
					| Fast::F64PairMul(..)),
				),
				ColumnValues::F64(out),
			) = (plan.fast, &mut values)
			{
				let nwords = count.div_ceil(64);
				let mut masks = [0; BLOCK / 64];
				validity_bits.read_words(row, count, &mut masks[..nwords]);
				if masks[..nwords].iter().any(|&v| v != 0) {
					let (op, i, j, constant) = match fast {
						Fast::F64Add(i, c) => (FloatOp::Add, i, i, Some(c)),
						_ => {
							let (op, i, j) = float_operation(fast);
							(op, i, j, None)
						}
					};
					let (ColumnSlice::F64(x), ColumnSlice::F64(y)) = (inputs[i], inputs[j]) else {
						unreachable!("verified float inputs")
					};
					let x = &x[offsets[i]..offsets[i] + count];
					let y = &y[offsets[j]..offsets[j] + count];
					let output = &mut out[row..row + count];
					let all = masks[..nwords]
						.iter()
						.enumerate()
						.all(|(word, &mask)| mask == low_mask((count - word * 64).min(64)));
					if all {
						if let Some(c) = constant {
							engine.floats(x, c, output);
						} else {
							engine.float_pair(op, x, y, output);
						}
					} else {
						let rhs = match constant {
							Some(c) => FloatRhs::AddConstant(c),
							None => FloatRhs::Column(op, y),
						};
						engine.masked_floats(x, rhs, output, &masks[..nwords]);
					}
					if engine.avx2() {
						path = Path::Avx2;
					}
				}
			} else if let (Some(Fast::BoolNot(i)), ColumnValues::Bool(out)) =
				(plan.fast, &mut values)
			{
				let ColumnSlice::Bool(input) = inputs[i] else {
					unreachable!("verified Boolean input")
				};
				let nwords = count.div_ceil(64);
				let mut bits = [0; BLOCK / 64];
				let mut masks = [0; BLOCK / 64];
				input.read_words(offsets[i], count, &mut bits[..nwords]);
				validity_bits.read_words(row, count, &mut masks[..nwords]);
				for (word, mask) in bits[..nwords].iter_mut().zip(&masks) {
					*word = !*word & mask;
				}
				append_words(out, row, count, &bits[..nwords]);
			} else {
				// Words are internal mask units, not validity-run boundaries. Block
				// shape and cancellation cadence depend only on physical chunks.
				for start in (0..count).step_by(64) {
					let n = (count - start).min(64);
					let mask = validity_bits.word(row + start, n);
					if mask == 0 {
						continue;
					}
					let current = offsets.map(|v| v + start);
					let output_row = row + start;
					let all = mask == low_mask(n);
					let fast = masked_fast(
						plan.fast,
						&inputs,
						&current,
						&mut values,
						output_row,
						n,
						mask,
						all,
						engine,
					);
					if fast.is_some_and(|(_, simd)| simd) {
						path = Path::Avx2;
					}
					let mut failed = fast.is_some_and(|(failed, _)| failed);
					if fast.is_none() {
						for node in &plan.nodes {
							let out = node.buffer * BLOCK;
							match node.step {
								Step::Input(i) => {
									copy_input(inputs[i], current[i], &mut scratch[out..out + n])
								}
								Step::Constant(v) => scratch[out..out + n].fill(v),
								Step::Not(a) => {
									let a = plan.nodes[a].buffer * BLOCK;
									for j in 0..n {
										scratch[out + j] = u64::from(scratch[a + j] == 0);
									}
								}
								Step::Binary(op, a, b) => {
									let ty = plan.nodes[a].ty;
									let a = plan.nodes[a].buffer * BLOCK;
									let b = plan.nodes[b].buffer * BLOCK;
									if all {
										failed |= apply_binary(
											op,
											ty,
											&mut scratch,
											[a, b, out],
											n,
											engine,
										);
										if ty == ColumnType::I64
											&& op == Binary::Add && engine.avx2()
										{
											path = Path::Avx2;
										}
									} else {
										failed |= apply_masked_binary(
											op,
											ty,
											&mut scratch,
											[a, b, out],
											n,
											mask,
										);
									}
								}
							}
						}
					}
					if failed {
						return Err(first_fault(
							plan,
							&inputs,
							&current,
							output_row,
							n,
							Some(mask),
							&mut row_values,
						));
					}
					if fast.is_none() {
						let start = plan.nodes[plan.result].buffer * BLOCK;
						let output = &mut scratch[start..start + n];
						if !all {
							for (i, value) in output.iter_mut().enumerate() {
								*value &= 0u64.wrapping_sub(mask >> i & 1);
							}
						}
						copy_output(output, &mut values, output_row);
					}
				}
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
			values,
			validity,
			len,
			remaining_work: options.work - charge,
			path,
		})
	}
}

#[allow(clippy::too_many_arguments)]
fn masked_fast(
	fast: Option<Fast>,
	inputs: &[ColumnSlice<'_>; 3],
	offsets: &[usize; 3],
	values: &mut ColumnValues,
	row: usize,
	n: usize,
	mask: u64,
	all: bool,
	engine: Engine,
) -> Option<(bool, bool)> {
	let failed = match (fast?, values) {
		(Fast::I64Add(i, c), ColumnValues::I64(out)) => {
			let ColumnSlice::I64(x) = inputs[i] else {
				unreachable!("verified type")
			};
			let x = &x[offsets[i]..offsets[i] + n];
			let out = &mut out[row..row + n];
			let failed = engine.add(x, c, out);
			if all {
				failed
			} else if failed {
				// Only a flagged block needs validity-aware overflow detection.
				engine.masked_add(x, c, out, mask)
			} else {
				engine.zero_i64(out, mask);
				false
			}
		}
		(Fast::Fused(i, j), ColumnValues::I64(out)) => {
			let (ColumnSlice::I64(x), ColumnSlice::I64(y)) = (inputs[i], inputs[j]) else {
				unreachable!("verified type")
			};
			let x = &x[offsets[i]..offsets[i] + n];
			let y = &y[offsets[j]..offsets[j] + n];
			let out = &mut out[row..row + n];
			let failed = engine.fused(x, y, out);
			if all {
				failed
			} else if failed {
				engine.masked_fused(x, y, out, mask)
			} else {
				engine.zero_i64(out, mask);
				false
			}
		}
		_ => unreachable!("verified fast output"),
	};
	Some((failed, engine.avx2()))
}

fn apply_masked_binary(
	op: Binary,
	ty: ColumnType,
	slots: &mut [u64],
	buffers: [usize; 3],
	n: usize,
	mask: u64,
) -> bool {
	let [a, b, out] = buffers;
	let (x, y, out) = split_binary(slots, a, b, out, n);
	macro_rules! integer {
		($method:ident) => {{
			let mut failed = false;
			for (i, ((&a, &b), out)) in x.iter().zip(y).zip(out).enumerate() {
				let (v, f) = (a as i64).$method(b as i64);
				*out = v as u64;
				failed |= f && (mask >> i & 1 != 0);
			}
			failed
		}};
	}
	match (ty, op) {
		(ColumnType::I64, Binary::Add) => integer!(overflowing_add),
		(ColumnType::I64, Binary::Sub) => integer!(overflowing_sub),
		(ColumnType::I64, Binary::Mul) => integer!(overflowing_mul),
		_ => {
			// Non-integer arithmetic cannot overflow. This uncommon generic
			// path preserves every admitted comparison/Boolean operation.
			for ((&a, &b), out) in x.iter().zip(y).zip(out) {
				*out = binary(op, ty, a, b).0;
			}
			false
		}
	}
}
