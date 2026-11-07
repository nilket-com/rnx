//! Numeric predicates and packed Boolean intermediates, isolated from generic execution.
use super::super::lease::append_words;
use super::*;

const WORDS: usize = BLOCK / 64;

#[derive(Clone, Copy, Default)]
struct Cursor {
	chunk: usize,
	offset: usize,
}

#[derive(Clone, Copy)]
enum Value<'a> {
	Integers(&'a [i64]),
	Floats(&'a [f64]),
	Words(&'a [u64]),
	Constant(u64),
}
trait Lanes {
	fn lane(&self, i: usize) -> u64;
}

trait Src {
	fn block<const N: usize>(&self, start: usize) -> impl Lanes;
	fn check_len(&self, len: usize);
	fn get(&self, i: usize) -> u64;
}
macro_rules! source {
	($ty:ty, $convert:expr) => {
		impl<const N: usize> Lanes for &[$ty; N] {
			#[inline(always)]
			fn lane(&self, i: usize) -> u64 {
				($convert)(self[i])
			}
		}
		impl Src for &[$ty] {
			#[inline(always)]
			fn block<const N: usize>(&self, start: usize) -> impl Lanes {
				let block: &[$ty; N] = self[start..start + N].try_into().expect("fixed lane block");
				block
			}
			#[inline(always)]
			fn check_len(&self, len: usize) {
				assert!(self.len() >= len);
			}
			#[inline(always)]
			fn get(&self, i: usize) -> u64 {
				($convert)(self[i])
			}
		}
	};
}
source!(i64, |v: i64| v as u64);
source!(f64, |v: f64| v.to_bits());
source!(u64, |v: u64| v);
impl Lanes for u64 {
	#[inline(always)]
	fn lane(&self, _: usize) -> u64 {
		*self
	}
}
impl Src for u64 {
	#[inline(always)]
	fn block<const N: usize>(&self, _: usize) -> impl Lanes {
		*self
	}
	#[inline(always)]
	fn check_len(&self, _: usize) {}
	#[inline(always)]
	fn get(&self, _: usize) -> u64 {
		*self
	}
}

// Resolve both operand representations once, outside every lane loop.
macro_rules! dispatch {
	($a:expr, $b:expr, $fun:ident $(::<$($generic:tt),*>)?, $($arg:expr),*) => {{
		macro_rules! right {
			($left:expr) => {
				match $b {
					Value::Integers(v) => $fun $(::<$($generic),*>)?($left, v, $($arg),*),
					Value::Floats(v) => $fun $(::<$($generic),*>)?($left, v, $($arg),*),
					Value::Words(v) => $fun $(::<$($generic),*>)?($left, v, $($arg),*),
					Value::Constant(v) => $fun $(::<$($generic),*>)?($left, v, $($arg),*),
				}
			};
		}
		match $a {
			Value::Integers(v) => right!(v),
			Value::Floats(v) => right!(v),
			Value::Words(v) => right!(v),
			Value::Constant(v) => right!(v),
		}
	}};
}

#[inline(always)]
fn compare<const FLOAT: bool, const OP: u8>(a: u64, b: u64) -> bool {
	macro_rules! apply {
		($a:expr, $b:expr) => {
			match OP {
				0 => $a == $b,
				1 => $a != $b,
				2 => $a < $b,
				3 => $a <= $b,
				4 => $a > $b,
				5 => $a >= $b,
				_ => unreachable!("comparison"),
			}
		};
	}
	if FLOAT {
		apply!(f64::from_bits(a), f64::from_bits(b))
	} else {
		apply!(a as i64, b as i64)
	}
}

#[inline(never)]
fn comparisons<const FLOAT: bool, const OP: u8>(
	a: impl Src,
	b: impl Src,
	out: &mut [u64],
	len: usize,
) {
	a.check_len(len);
	b.check_len(len);
	assert!(out.len() >= len.div_ceil(64));
	// Select the measured form by type at compile time, not per row.
	if FLOAT {
		let mut compared = [0u64; 64];
		for (word, value) in out[..len.div_ceil(64)].iter_mut().enumerate() {
			let start = word * 64;
			let count = (len - start).min(64);
			let mut bits = 0;
			if count == 64 {
				let a = a.block::<64>(start);
				let b = b.block::<64>(start);
				// Elementwise comparison and packing are separate loops. The
				// initialized staging word is reused across all complete words.
				for (i, lane) in compared.iter_mut().enumerate() {
					*lane = u64::from(compare::<FLOAT, OP>(a.lane(i), b.lane(i)));
				}
				for (i, lane) in compared.iter().enumerate() {
					bits |= lane << i;
				}
			} else {
				for lane in 0..count {
					bits |= u64::from(compare::<FLOAT, OP>(
						a.get(start + lane),
						b.get(start + lane),
					)) << lane;
				}
			}
			*value = bits;
		}
	} else {
		for (word, value) in out[..len.div_ceil(64)].iter_mut().enumerate() {
			let start = word * 64;
			let count = (len - start).min(64);
			let mut bits = 0;
			let mut lane = 0;
			while lane + 8 <= count {
				let a = a.block::<8>(start + lane);
				let b = b.block::<8>(start + lane);
				// Fixed-size views establish bounds once; keep comparison lanes
				// separate from the reduction so packing does not force scalar loads.
				let compared: [bool; 8] =
					core::array::from_fn(|i| compare::<FLOAT, OP>(a.lane(i), b.lane(i)));
				macro_rules! bit {
					($n:expr) => {
						u64::from(compared[$n]) << $n
					};
				}
				bits |=
					(bit!(0) | bit!(1) | bit!(2) | bit!(3) | bit!(4) | bit!(5) | bit!(6) | bit!(7))
						<< lane;
				lane += 8;
			}
			while lane < count {
				bits |= u64::from(compare::<FLOAT, OP>(
					a.get(start + lane),
					b.get(start + lane),
				)) << lane;
				lane += 1;
			}
			*value = bits;
		}
	}
}

#[inline(never)]
fn arithmetic<const OP: u8>(a: impl Src, b: impl Src, out: &mut [u64]) {
	a.check_len(out.len());
	b.check_len(out.len());
	for (i, value) in out.iter_mut().enumerate() {
		let a = f64::from_bits(a.get(i));
		let b = f64::from_bits(b.get(i));
		*value = match OP {
			0 => a + b,
			1 => a - b,
			2 => a * b,
			_ => unreachable!("arithmetic"),
		}
		.to_bits();
	}
}

struct Sources<'a> {
	inputs: &'a [ColumnSlice<'a>; 3],
	offsets: &'a [usize; 3],
	before: &'a [u64],
	after: &'a [u64],
	split: usize,
	output: &'a [u64],
	len: usize,
}
impl<'a> Sources<'a> {
	fn slot(&self, slot: usize) -> &'a [u64] {
		let start = slot * BLOCK;
		assert_ne!(start, self.split, "destination excludes live operands");
		if start < self.split {
			&self.before[start..start + self.len]
		} else {
			let start = start - self.split - BLOCK;
			&self.after[start..start + self.len]
		}
	}
	fn numeric(&self, operand: Operand) -> Value<'a> {
		match operand {
			Operand::Input(i) => {
				let start = self.offsets[i];
				match self.inputs[i] {
					ColumnSlice::I64(v) => Value::Integers(&v[start..start + self.len]),
					ColumnSlice::F64(v) => Value::Floats(&v[start..start + self.len]),
					ColumnSlice::Bool(_) => unreachable!("numeric input"),
				}
			}
			Operand::Constant(v) => Value::Constant(v),
			Operand::Slot(slot) => Value::Words(self.slot(slot)),
			Operand::Output => unreachable!("predicate result is Boolean"),
		}
	}
	fn boolean(&self, operand: Operand, out: &mut [u64]) {
		match operand {
			Operand::Input(i) => {
				let ColumnSlice::Bool(bits) = self.inputs[i] else {
					unreachable!("Boolean input")
				};
				bits.read_words(self.offsets[i], self.len, out);
			}
			Operand::Constant(v) => out.fill(if v == 0 { 0 } else { u64::MAX }),
			Operand::Slot(slot) => out.copy_from_slice(&self.slot(slot)[..out.len()]),
			Operand::Output => out.copy_from_slice(&self.output[..out.len()]),
		}
	}
}

fn numeric_comparison(
	op: Binary,
	ty: ColumnType,
	a: Value<'_>,
	b: Value<'_>,
	out: &mut [u64],
	len: usize,
) {
	macro_rules! compare_type {
		($float:tt) => {
			match op {
				Binary::Eq => dispatch!(a, b, comparisons::<$float, 0>, out, len),
				Binary::Ne => dispatch!(a, b, comparisons::<$float, 1>, out, len),
				Binary::Lt => dispatch!(a, b, comparisons::<$float, 2>, out, len),
				Binary::Le => dispatch!(a, b, comparisons::<$float, 3>, out, len),
				Binary::Gt => dispatch!(a, b, comparisons::<$float, 4>, out, len),
				Binary::Ge => dispatch!(a, b, comparisons::<$float, 5>, out, len),
				_ => unreachable!("verified comparison"),
			}
		};
	}
	match ty {
		ColumnType::I64 => compare_type!(false),
		ColumnType::F64 => compare_type!(true),
		ColumnType::Bool => unreachable!("numeric comparison"),
	}
}

#[inline(never)]
pub(super) fn run(
	batch: PreparedBatch<'_, '_>,
	canceled: &mut impl FnMut() -> bool,
) -> core::result::Result<ColumnResult, (ColumnRefusal, Option<Fault>)> {
	let PreparedBatch {
		plan,
		columns,
		options,
		mut values,
		validity,
		len,
		charge,
		valid_rows,
		mut scratch,
		row_values: _,
	} = batch;
	let ColumnValues::Bool(output) = &mut values else {
		unreachable!("predicate output")
	};
	let mut cursors = [Cursor::default(); 3];
	let mut left = [0; WORDS];
	let mut right = [0; WORDS];
	let mut result = [0; WORDS];
	let mut row = 0;
	while row < len {
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		let mut inputs = [ColumnSlice::I64(&[]); 3];
		let mut offsets = [0; 3];
		let mut count = (len - row).min(BLOCK);
		for (i, column) in columns.iter().enumerate() {
			let cursor = &mut cursors[i];
			while let Some(chunk) = column.chunks.get(cursor.chunk) {
				if cursor.offset < chunk.values.len() {
					break;
				}
				cursor.chunk += 1;
				cursor.offset = 0;
			}
			let chunk = column.chunks.get(cursor.chunk).expect("verified length");
			inputs[i] = chunk.values;
			offsets[i] = cursor.offset;
			count = count.min(chunk.values.len() - cursor.offset);
		}
		let words = count.div_ceil(64);
		for node in &plan.nodes {
			if matches!(node.step, Step::Input(..) | Step::Constant(..)) {
				continue;
			}
			// Interval allocation assigns the destination before releasing sources.
			// Excluding its entire BLOCK slot makes that invariant a safe borrow.
			let (before, destination, after, split) = match node.operand {
				Operand::Slot(slot) => {
					let split = slot * BLOCK;
					let (before, rest) = scratch.split_at_mut(split);
					let (destination, after) = rest.split_at_mut(BLOCK);
					(before as &[_], Some(destination), after as &[_], split)
				}
				Operand::Output => (&scratch[..], None, &[][..], usize::MAX),
				_ => unreachable!("computed destination"),
			};
			let source = Sources {
				inputs: &inputs,
				offsets: &offsets,
				before,
				after,
				split,
				output: &result,
				len: count,
			};
			match node.step {
				Step::Binary(op, a, b) if node.ty == ColumnType::F64 => {
					let a = source.numeric(plan.nodes[a].operand);
					let b = source.numeric(plan.nodes[b].operand);
					let out = &mut destination.expect("numeric scratch")[..count];
					match op {
						Binary::Add => dispatch!(a, b, arithmetic::<0>, out),
						Binary::Sub => dispatch!(a, b, arithmetic::<1>, out),
						Binary::Mul => dispatch!(a, b, arithmetic::<2>, out),
						_ => unreachable!("verified F64 arithmetic"),
					}
					continue;
				}
				Step::Binary(op, a, b) if plan.nodes[a].ty != ColumnType::Bool => {
					numeric_comparison(
						op,
						plan.nodes[a].ty,
						source.numeric(plan.nodes[a].operand),
						source.numeric(plan.nodes[b].operand),
						&mut left[..words],
						count,
					);
				}
				Step::Binary(op, a, b) => {
					source.boolean(plan.nodes[a].operand, &mut left[..words]);
					source.boolean(plan.nodes[b].operand, &mut right[..words]);
					for i in 0..words {
						left[i] = match op {
							Binary::BitAnd => left[i] & right[i],
							Binary::BitOr => left[i] | right[i],
							Binary::BitXor | Binary::Ne => left[i] ^ right[i],
							Binary::Eq => !(left[i] ^ right[i]),
							_ => unreachable!("verified Boolean operation"),
						};
					}
				}
				Step::Not(a) => {
					source.boolean(plan.nodes[a].operand, &mut left[..words]);
					for v in &mut left[..words] {
						*v = !*v;
					}
				}
				_ => unreachable!("computed node"),
			}
			left[words - 1] &= low_mask((count - 1) % 64 + 1);
			if let Some(destination) = destination {
				destination[..words].copy_from_slice(&left[..words]);
			} else {
				result[..words].copy_from_slice(&left[..words]);
			}
		}
		Sources {
			inputs: &inputs,
			offsets: &offsets,
			before: &scratch,
			after: &[],
			split: usize::MAX,
			output: &result,
			len: count,
		}
		.boolean(plan.nodes[plan.result].operand, &mut left[..words]);
		if let Some(validity) = &validity {
			BitSlice::new(validity, row, count)
				.expect("prepared validity")
				.read_words(0, count, &mut right[..words]);
			for i in 0..words {
				left[i] &= right[i];
			}
		}
		left[words - 1] &= low_mask((count - 1) % 64 + 1);
		append_words(output, row, count, &left[..words]);
		row += count;
		for cursor in &mut cursors[..columns.len()] {
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
		valid_rows,
		remaining_work: options.work - charge,
		path: Path::Scalar,
	})
}
