//! Whole-plan floating arithmetic isolated from the generic executor.
use super::*;

#[derive(Clone, Copy, Default)]
struct Cursor {
	chunk: usize,
	offset: usize,
}

#[derive(Clone, Copy)]
enum Value<'a> {
	Floats(&'a [f64]),
	Words(&'a [u64]),
	Constant(f64),
}
trait Src {
	fn check_len(&self, n: usize);
	fn get(&self, i: usize) -> f64;
}
impl Src for &[f64] {
	#[inline(always)]
	fn check_len(&self, n: usize) {
		assert!(self.len() >= n);
	}
	#[inline(always)]
	fn get(&self, i: usize) -> f64 {
		self[i]
	}
}
impl Src for &[u64] {
	#[inline(always)]
	fn check_len(&self, n: usize) {
		assert!(self.len() >= n);
	}
	#[inline(always)]
	fn get(&self, i: usize) -> f64 {
		f64::from_bits(self[i])
	}
}
impl Src for f64 {
	#[inline(always)]
	fn check_len(&self, _: usize) {}
	#[inline(always)]
	fn get(&self, _: usize) -> f64 {
		*self
	}
}

struct Sources<'a> {
	inputs: &'a [&'a [f64]; 3],
	before: &'a [u64],
	after: &'a [u64],
	split: usize,
	output: Option<&'a [u64]>,
	len: usize,
}
impl<'a> Sources<'a> {
	fn get(&self, operand: Operand) -> Value<'a> {
		match operand {
			Operand::Input(i) => Value::Floats(self.inputs[i]),
			Operand::Constant(v) => Value::Constant(f64::from_bits(v)),
			Operand::Output => Value::Words(self.output.expect("acyclic output")),
			Operand::Slot(slot) => {
				let start = slot * BLOCK;
				debug_assert_ne!(start, self.split, "destination excludes live operands");
				Value::Words(if start < self.split {
					&self.before[start..start + self.len]
				} else {
					let start = start - self.split - BLOCK;
					&self.after[start..start + self.len]
				})
			}
		}
	}
}

// Separate arithmetic expressions preserve node order; never fuse a chain.
// Eight source-explicit lanes and a tail give each new loop a recorded shape.
#[inline(never)]
fn lanes<const OP: u8>(out: &mut [u64], a: impl Src, b: impl Src) {
	a.check_len(out.len());
	b.check_len(out.len());
	let value = |i| {
		let (a, b) = (a.get(i), b.get(i));
		match OP {
			0 => (a + b).to_bits(),
			1 => (a - b).to_bits(),
			2 => (a * b).to_bits(),
			_ => unreachable!("arithmetic operation"),
		}
	};
	let end = out.len() / 8 * 8;
	let mut i = 0;
	while i < end {
		out[i] = value(i);
		out[i + 1] = value(i + 1);
		out[i + 2] = value(i + 2);
		out[i + 3] = value(i + 3);
		out[i + 4] = value(i + 4);
		out[i + 5] = value(i + 5);
		out[i + 6] = value(i + 6);
		out[i + 7] = value(i + 7);
		i += 8;
	}
	for (i, out) in out.iter_mut().enumerate().skip(end) {
		*out = value(i);
	}
}

fn arithmetic<const OP: u8>(out: &mut [u64], a: Value<'_>, b: Value<'_>) {
	macro_rules! right {
		($a:expr) => {
			match b {
				Value::Floats(v) => lanes::<OP>(out, $a, v),
				Value::Words(v) => lanes::<OP>(out, $a, v),
				Value::Constant(v) => lanes::<OP>(out, $a, v),
			}
		};
	}
	match a {
		Value::Floats(v) => right!(v),
		Value::Words(v) => right!(v),
		Value::Constant(v) => right!(v),
	}
}

// Resolve the representation once, then mask a word at a time. Null values
// are published as positive zero without evaluating their source again.
fn publish(out: &mut [f64], value: impl Src, validity: Option<BitSlice<'_>>) {
	value.check_len(out.len());
	for (word, block) in out.chunks_mut(64).enumerate() {
		let start = word * 64;
		let all = low_mask(block.len());
		let mask = validity.map_or(all, |v| v.word(start, block.len()));
		if mask == 0 {
			block.fill(0.0);
		} else if mask == all {
			for (i, output) in block.iter_mut().enumerate() {
				*output = value.get(start + i);
			}
		} else {
			for (i, output) in block.iter_mut().enumerate() {
				*output = if mask >> i & 1 != 0 {
					value.get(start + i)
				} else {
					0.0
				};
			}
		}
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
	let ColumnValues::F64(output) = &mut values else {
		unreachable!("verified float output");
	};
	let mut cursors = [Cursor::default(); 3];
	// Every live result lane is overwritten before use; retain initialized
	// storage across blocks, including a shorter final block.
	let mut result = [0u64; BLOCK];
	let mut row = 0;
	while row < len {
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		let mut inputs: [&[f64]; 3] = [&[]; 3];
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
			let ColumnSlice::F64(v) = chunk.values else {
				unreachable!("verified float input");
			};
			inputs[i] = &v[cursor.offset..];
			count = count.min(inputs[i].len());
		}
		for node in &plan.nodes {
			let Step::Binary(op, a, b) = node.step else {
				continue;
			};
			let (before, after, split, previous, destination) = match node.operand {
				Operand::Slot(slot) => {
					let split = slot * BLOCK;
					let (before, rest) = scratch.split_at_mut(split);
					let (destination, after) = rest.split_at_mut(BLOCK);
					(
						&*before,
						&*after,
						split,
						Some(&result[..count]),
						&mut destination[..count],
					)
				}
				Operand::Output => (
					&scratch[..],
					&[][..],
					scratch.len(),
					None,
					&mut result[..count],
				),
				_ => unreachable!("computed destination"),
			};
			let source = Sources {
				inputs: &inputs,
				before,
				after,
				split,
				output: previous,
				len: count,
			};
			let (a, b) = (
				source.get(plan.nodes[a].operand),
				source.get(plan.nodes[b].operand),
			);
			match op {
				Binary::Add => arithmetic::<0>(destination, a, b),
				Binary::Sub => arithmetic::<1>(destination, a, b),
				Binary::Mul => arithmetic::<2>(destination, a, b),
				_ => unreachable!("verified float arithmetic"),
			}
		}
		let source = Sources {
			inputs: &inputs,
			before: &scratch,
			after: &[],
			split: scratch.len(),
			output: Some(&result[..count]),
			len: count,
		};
		let value = source.get(plan.nodes[plan.result].operand);
		let valid = validity
			.as_ref()
			.map(|v| BitSlice::new(v, row, count).expect("prepared validity"));
		match value {
			Value::Floats(v) => publish(&mut output[row..row + count], v, valid),
			Value::Words(v) => publish(&mut output[row..row + count], v, valid),
			Value::Constant(v) => publish(&mut output[row..row + count], v, valid),
		}
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
