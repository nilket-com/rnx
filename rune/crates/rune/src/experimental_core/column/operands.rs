//! Borrowed operands and disjoint intermediate destinations for generic blocks.
use super::*;
use crate::experimental_column_primitives::{self as primitive, Engine};

#[derive(Clone, Copy)]
enum Value<'a> {
	Words(&'a [u64]),
	Bits(BitSlice<'a>, usize),
	Constant(u64),
}
impl Value<'_> {
	#[inline]
	fn at(self, i: usize) -> u64 {
		match self {
			Self::Words(v) => v[i],
			Self::Bits(v, start) => u64::from(v.bit(start + i)),
			Self::Constant(v) => v,
		}
	}
}

enum Destination<'a> {
	Words(&'a mut [u64]),
	Bits(&'a mut [u8], usize, usize),
}
impl Destination<'_> {
	fn len(&self) -> usize {
		match self {
			Self::Words(v) => v.len(),
			Self::Bits(_, _, len) => *len,
		}
	}
	#[inline]
	fn put(&mut self, i: usize, value: u64) {
		match self {
			Self::Words(v) => v[i] = value,
			Self::Bits(v, start, _) => {
				let bit = *start + i;
				let mask = 1 << (bit % 8);
				v[bit / 8] = (v[bit / 8] & !mask) | (u8::from(value != 0) << (bit % 8));
			}
		}
	}
}

fn input_value(values: ColumnSlice<'_>, offset: usize, n: usize) -> Value<'_> {
	match values {
		ColumnSlice::I64(v) => Value::Words(primitive::i64_words(&v[offset..offset + n])),
		ColumnSlice::F64(v) => Value::Words(primitive::f64_words(&v[offset..offset + n])),
		ColumnSlice::Bool(v) => Value::Bits(v, offset),
	}
}
fn output_value(values: &ColumnValues, row: usize, n: usize) -> Value<'_> {
	match values {
		ColumnValues::I64(v) => Value::Words(primitive::i64_words(&v[row..row + n])),
		ColumnValues::F64(v) => Value::Words(primitive::f64_words(&v[row..row + n])),
		ColumnValues::Bool(v) => Value::Bits(BitSlice::new(v, row, n).expect("owned output"), 0),
	}
}
fn destination(values: &mut ColumnValues, row: usize, n: usize) -> Destination<'_> {
	match values {
		ColumnValues::I64(v) => Destination::Words(primitive::i64_words_mut(&mut v[row..row + n])),
		ColumnValues::F64(v) => Destination::Words(primitive::f64_words_mut(&mut v[row..row + n])),
		ColumnValues::Bool(v) => Destination::Bits(v, row, n),
	}
}

// The destination slot is excluded from these shared views. Slot indices are
// absolute so a recycled slot can lie on either side without copying operands.
struct Sources<'a> {
	inputs: &'a [ColumnSlice<'a>; 3],
	offsets: &'a [usize; 3],
	before: &'a [u64],
	after: &'a [u64],
	split: usize,
	output: Option<Value<'a>>,
	n: usize,
}
impl<'a> Sources<'a> {
	fn get(&self, operand: Operand) -> Value<'a> {
		match operand {
			Operand::Input(i) => input_value(self.inputs[i], self.offsets[i], self.n),
			Operand::Constant(v) => Value::Constant(v),
			Operand::Output => self.output.expect("acyclic result destination"),
			Operand::Slot(slot) => {
				let start = slot * BLOCK;
				let v = if start < self.split {
					&self.before[start..start + self.n]
				} else {
					&self.after[start - self.split - BLOCK..start - self.split - BLOCK + self.n]
				};
				Value::Words(v)
			}
		}
	}
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
	plan: &PreparedColumn,
	inputs: &[ColumnSlice<'_>; 3],
	offsets: &[usize; 3],
	scratch: &mut [u64],
	values: &mut ColumnValues,
	row: usize,
	n: usize,
	mask: Option<u64>,
	engine: Engine,
) -> (bool, bool) {
	match mask {
		None => {
			execute_inner::<false>(plan, inputs, offsets, scratch, values, row, n, None, engine)
		}
		Some(mask) => execute_inner::<true>(
			plan,
			inputs,
			offsets,
			scratch,
			values,
			row,
			n,
			Some(mask),
			engine,
		),
	}
}

#[allow(clippy::too_many_arguments)]
fn execute_inner<const MASKED: bool>(
	plan: &PreparedColumn,
	inputs: &[ColumnSlice<'_>; 3],
	offsets: &[usize; 3],
	scratch: &mut [u64],
	values: &mut ColumnValues,
	row: usize,
	n: usize,
	mask: Option<u64>,
	engine: Engine,
) -> (bool, bool) {
	let mut failed = false;
	let mut simd = false;
	for node in &plan.nodes {
		if matches!(node.step, Step::Input(_) | Step::Constant(_)) {
			continue;
		}
		let (sources, out) = match node.operand {
			Operand::Slot(slot) => {
				let split = slot * BLOCK;
				let (before, rest) = scratch.split_at_mut(split);
				let (out, after) = rest.split_at_mut(BLOCK);
				(
					Sources {
						inputs,
						offsets,
						before,
						after,
						split,
						output: Some(output_value(values, row, n)),
						n,
					},
					Destination::Words(&mut out[..n]),
				)
			}
			Operand::Output => (
				Sources {
					inputs,
					offsets,
					before: scratch,
					after: &[],
					split: scratch.len(),
					output: None,
					n,
				},
				destination(values, row, n),
			),
			_ => unreachable!("computed node destination"),
		};
		let (fault, vector) = match node.step {
			Step::Not(a) => {
				let x = sources.get(plan.nodes[a].operand);
				(
					apply(out, mask, |i| (u64::from(x.at(i) == 0), false)),
					false,
				)
			}
			Step::Binary(op, a, b) => operation::<MASKED>(
				op,
				plan.nodes[a].ty,
				sources.get(plan.nodes[a].operand),
				sources.get(plan.nodes[b].operand),
				out,
				mask,
				engine,
			),
			_ => unreachable!("computed node"),
		};
		failed |= fault;
		simd |= vector;
	}
	// Identity and constant results have no producing operation. This is the
	// required output write, not a scratch-to-output copy.
	let result = plan.nodes[plan.result].operand;
	if !matches!(result, Operand::Output) {
		let sources = Sources {
			inputs,
			offsets,
			before: scratch,
			after: &[],
			split: scratch.len(),
			output: None,
			n,
		};
		let x = sources.get(result);
		apply(destination(values, row, n), mask, |i| (x.at(i), false));
	}
	// Invalid intermediate lanes may contain wrapped speculative values. Publish
	// deterministic null payloads only after all later uses of Output are done.
	if MASKED {
		if let (Some(mask), ColumnValues::I64(output)) = (mask, values) {
			engine.zero_i64(&mut output[row..row + n], mask);
		}
	}
	(failed, simd)
}

fn apply(
	mut out: Destination<'_>,
	mask: Option<u64>,
	mut f: impl FnMut(usize) -> (u64, bool),
) -> bool {
	let mut failed = false;
	for i in 0..out.len() {
		let valid = mask.is_none_or(|mask| mask >> i & 1 != 0);
		let (v, fault) = f(i);
		out.put(i, if valid { v } else { 0 });
		failed |= fault && valid;
	}
	failed
}

// Resolve storage once per block. Each instantiated loop has concrete slice or
// constant sources and a concrete word destination; no enum dispatch per lane.
trait Src {
	fn check_len(&self, len: usize);
	fn get(&self, index: usize) -> u64;
}
impl Src for &[u64] {
	#[inline(always)]
	fn check_len(&self, len: usize) {
		assert_eq!(self.len(), len);
	}
	#[inline(always)]
	fn get(&self, index: usize) -> u64 {
		self[index]
	}
}
struct Const(u64);
impl Src for Const {
	#[inline(always)]
	fn check_len(&self, _: usize) {}
	#[inline(always)]
	fn get(&self, _: usize) -> u64 {
		self.0
	}
}

fn integer_kernel<L: Src, R: Src>(
	op: Binary,
	left: L,
	right: R,
	output: &mut [u64],
	mask: Option<u64>,
) -> bool {
	left.check_len(output.len());
	right.check_len(output.len());
	match mask {
		Some(mask) => {
			assert!(output.len() <= 64);
			integer_loop::<_, _, true>(op, left, right, output, mask)
		}
		None => integer_loop::<_, _, false>(op, left, right, output, 0),
	}
}

fn integer_loop<L: Src, R: Src, const MASKED: bool>(
	op: Binary,
	left: L,
	right: R,
	output: &mut [u64],
	mask: u64,
) -> bool {
	macro_rules! run {
		($method:ident) => {{
			let mut failed = false;
			for (i, out) in output.iter_mut().enumerate() {
				let (value, fault) = (left.get(i) as i64).$method(right.get(i) as i64);
				let valid = !MASKED || mask >> i & 1 != 0;
				*out = if valid { value as u64 } else { 0 };
				failed |= fault && valid;
			}
			failed
		}};
	}
	match op {
		Binary::Add => run!(overflowing_add),
		Binary::Sub => run!(overflowing_sub),
		Binary::Mul => run!(overflowing_mul),
		_ => unreachable!("checked integer arithmetic"),
	}
}

// Keep constant dispatch out of the existing pair/multiply executor's inline body.
#[cold]
#[inline(never)]
fn constant_optimistic(
	op: Binary,
	x: Value<'_>,
	y: Value<'_>,
	output: &mut [u64],
	mask: u64,
	engine: Engine,
) -> Option<bool> {
	use Binary::*;
	match (op, x, y) {
		(Add, Value::Words(a), Value::Constant(c)) | (Add, Value::Constant(c), Value::Words(a)) => {
			Some(engine.add_words_constant(a, c as i64, output, Some(mask)))
		}
		(Sub, Value::Words(a), Value::Constant(c)) => {
			Some(engine.sub_constant::<false>(a, c as i64, output, Some(mask)))
		}
		(Sub, Value::Constant(c), Value::Words(a)) => {
			Some(engine.sub_constant::<true>(a, c as i64, output, Some(mask)))
		}
		_ => None,
	}
}

fn operation<const MASKED: bool>(
	op: Binary,
	ty: ColumnType,
	x: Value<'_>,
	y: Value<'_>,
	mut out: Destination<'_>,
	mask: Option<u64>,
	engine: Engine,
) -> (bool, bool) {
	use Binary::*;
	// Optimistic validity: do not feed a null-only overflow flag to first_fault.
	// A flagged node gets at most one validity-aware scalar rescan.
	if let (Some(mask), ColumnType::I64, Destination::Words(output)) = (mask, ty, &mut out) {
		if MASKED && engine.avx2() {
			let optimistic = match (op, x, y) {
				(Add, Value::Words(a), Value::Words(b)) => Some(engine.add_pair(a, b, output)),
				(Sub, Value::Words(a), Value::Words(b)) => Some(engine.sub_pair(a, b, output)),
				(Mul, Value::Words(a), Value::Constant(c))
				| (Mul, Value::Constant(c), Value::Words(a))
					if c != 0 && (c as i64).unsigned_abs() < (1u64 << 32) =>
				{
					Some(engine.mul_constant(a, c as i64, output))
				}
				_ => constant_optimistic(op, x, y, output, mask, engine),
			};
			if let Some(flagged) = optimistic {
				let failed = flagged
					&& match (x, y) {
						(Value::Words(a), Value::Words(b)) => {
							integer_kernel(op, a, b, output, Some(mask))
						}
						(Value::Words(a), Value::Constant(b)) => {
							integer_kernel(op, a, Const(b), output, Some(mask))
						}
						(Value::Constant(a), Value::Words(b)) => {
							integer_kernel(op, Const(a), b, output, Some(mask))
						}
						_ => unreachable!("optimistic integer shape"),
					};
				return (failed, true);
			}
		}
	}
	if mask.is_none() && ty == ColumnType::I64 {
		if let Destination::Words(output) = &mut out {
			match (op, x, y) {
				(Add, Value::Words(x), Value::Words(y)) => {
					return (engine.add_pair(x, y, output), engine.avx2())
				}
				(Mul, Value::Words(x), Value::Words(y)) => {
					return (primitive::checked_mul(x, y, output), false);
				}
				(Mul, Value::Words(x), Value::Constant(c))
				| (Mul, Value::Constant(c), Value::Words(x)) => {
					return (
						engine.mul_constant(x, c as i64, output),
						engine.avx2() && c != 0 && (c as i64).unsigned_abs() < (1u64 << 32),
					);
				}
				_ => {}
			}
		}
	}
	if ty == ColumnType::I64 && matches!(op, Add | Sub | Mul) {
		if let Destination::Words(output) = &mut out {
			let failed = match (x, y) {
				(Value::Words(x), Value::Words(y)) => integer_kernel(op, x, y, output, mask),
				(Value::Words(x), Value::Constant(y)) => {
					integer_kernel(op, x, Const(y), output, mask)
				}
				(Value::Constant(x), Value::Words(y)) => {
					integer_kernel(op, Const(x), y, output, mask)
				}
				(Value::Constant(x), Value::Constant(y)) => {
					integer_kernel(op, Const(x), Const(y), output, mask)
				}
				_ => unreachable!("integer operands are slices or constants"),
			};
			return (failed, false);
		}
	}
	// Select arithmetic once per primitive; masks affect faults and null output,
	// not operation selection. Mixed-mask SIMD remains a separate record.
	macro_rules! int {
		($method:ident) => {
			apply(out, mask, |i| {
				let (v, fault) = (x.at(i) as i64).$method(y.at(i) as i64);
				(v as u64, fault)
			})
		};
	}
	let failed = match (ty, op) {
		(ColumnType::I64, Add) => int!(overflowing_add),
		(ColumnType::I64, Sub) => int!(overflowing_sub),
		(ColumnType::I64, Mul) => int!(overflowing_mul),
		_ => apply(out, mask, |i| binary(op, ty, x.at(i), y.at(i))),
	};
	(failed, false)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn borrowed_integer_storage_combinations_masks_and_faults() {
		let extrema = [i64::MIN, -3, -1, 0, 1, 3, i64::MAX];
		for n in [0usize, 1, 4, 7, 63, 64] {
			let x: Vec<_> = (0..n).map(|i| extrema[i % 7] as u64).collect();
			let y: Vec<_> = (0..n).map(|i| extrema[(i + 3) % 7] as u64).collect();
			for op in [Binary::Add, Binary::Sub, Binary::Mul] {
				for mask in [None, Some(0), Some(u64::MAX), Some(0xaaaaaaaaaaaaaaaa)] {
					for shape in 0..4 {
						let mut out = vec![719; n + 2];
						let failed = match shape {
							0 => integer_kernel(
								op,
								x.as_slice(),
								y.as_slice(),
								&mut out[1..n + 1],
								mask,
							),
							1 => {
								integer_kernel(op, x.as_slice(), Const(3), &mut out[1..n + 1], mask)
							}
							2 => integer_kernel(
								op,
								Const(i64::MIN as u64),
								y.as_slice(),
								&mut out[1..n + 1],
								mask,
							),
							_ => integer_kernel(
								op,
								Const(i64::MAX as u64),
								Const(3),
								&mut out[1..n + 1],
								mask,
							),
						};
						let mut expected_fault = false;
						for i in 0..n {
							let (a, b) = match shape {
								0 => (x[i], y[i]),
								1 => (x[i], 3),
								2 => (i64::MIN as u64, y[i]),
								_ => (i64::MAX as u64, 3),
							};
							let (v, fault) = binary(op, ColumnType::I64, a, b);
							let valid = mask.is_none_or(|m| m >> i & 1 != 0);
							assert_eq!(out[i + 1], if valid { v } else { 0 });
							expected_fault |= fault && valid;
						}
						assert_eq!(failed, expected_fault);
						assert_eq!((out[0], out[n + 1]), (719, 719));
					}
				}
			}
		}
	}
	#[test]
	fn optimistic_nodes_match_valid_scalar_lanes() {
		let extrema = [i64::MIN, i64::MAX, -1, 0, 1, 7, -7];
		for n in [0usize, 1, 3, 4, 7, 63, 64] {
			let x: Vec<_> = (0..n + 2).map(|i| extrema[i % 7] as u64).collect();
			let y: Vec<_> = (0..n + 2).map(|i| extrema[(i + 2) % 7] as u64).collect();
			for mask in [0, u64::MAX, 0xaaaaaaaaaaaaaaaa, 0x831471bb02f356ad] {
				for op in [Binary::Add, Binary::Sub, Binary::Mul] {
					for constant in [
						None,
						Some(0),
						Some(1),
						Some(-1),
						Some(3),
						Some(i64::MIN),
						Some(i64::MAX),
					] {
						for reversed in [false, true] {
							let a = Value::Words(&x[1..n + 1]);
							let b = constant
								.map_or(Value::Words(&y[1..n + 1]), |c| Value::Constant(c as u64));
							let (a, b) = if reversed { (b, a) } else { (a, b) };
							let mut out = vec![719; n + 2];
							let (failed, _) = operation::<true>(
								op,
								ColumnType::I64,
								a,
								b,
								Destination::Words(&mut out[1..n + 1]),
								Some(mask),
								Engine::new(false),
							);
							let mut expected = false;
							for i in 0..n {
								if mask >> i & 1 != 0 {
									let (v, f) = binary(op, ColumnType::I64, a.at(i), b.at(i));
									assert_eq!(out[i + 1], v);
									expected |= f;
								}
							}
							assert_eq!(failed, expected);
							assert_eq!((out[0], out[n + 1]), (719, 719));
						}
					}
				}
			}
		}
	}
}
