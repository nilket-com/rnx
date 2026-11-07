//! Whole-plan packed Boolean execution, with no generic executor calls.
use super::super::lease::append_words;
use super::*;

const WORDS: usize = BLOCK / 64;

#[derive(Clone, Copy, Default)]
struct Cursor {
	chunk: usize,
	offset: usize,
}

struct Sources<'a> {
	inputs: &'a [BitSlice<'a>; 3],
	offsets: &'a [usize; 3],
	scratch: &'a [u64],
	output: &'a [u64],
	len: usize,
}
impl Sources<'_> {
	fn read(&self, operand: Operand, out: &mut [u64]) {
		match operand {
			Operand::Input(i) => self.inputs[i].read_words(self.offsets[i], self.len, out),
			Operand::Constant(value) => out.fill(if value == 0 { 0 } else { u64::MAX }),
			Operand::Slot(slot) => {
				out.copy_from_slice(&self.scratch[slot * WORDS..slot * WORDS + out.len()])
			}
			Operand::Output => out.copy_from_slice(&self.output[..out.len()]),
		}
	}
}

// Keep the packed buffers and traversal outside every generic execution frame.
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
		unreachable!("verified Boolean plan");
	};
	let mut cursors = [Cursor::default(); 3];
	let mut row = 0;
	while row < len {
		if canceled() {
			return Err((ColumnRefusal::Canceled, None));
		}
		let empty = BitSlice::new(&[], 0, 0).expect("empty range");
		let mut inputs = [empty; 3];
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
			let ColumnSlice::Bool(bits) = chunk.values else {
				unreachable!("verified Boolean input");
			};
			inputs[i] = bits;
			offsets[i] = cursor.offset;
			count = count.min(bits.len() - cursor.offset);
		}
		let words = count.div_ceil(64);
		let mut left = [0; WORDS];
		let mut right = [0; WORDS];
		// The append primitives require a zero destination range. Keep the
		// speculative result private until validity is applied, then append once.
		let mut result_words = [0; WORDS];
		for node in &plan.nodes {
			let source = Sources {
				inputs: &inputs,
				offsets: &offsets,
				scratch: &scratch,
				output: &result_words,
				len: count,
			};
			match node.step {
				Step::Input(..) | Step::Constant(..) => continue,
				Step::Not(a) => {
					source.read(plan.nodes[a].operand, &mut left[..words]);
					for value in &mut left[..words] {
						*value = !*value;
					}
				}
				Step::Binary(op, a, b) => {
					source.read(plan.nodes[a].operand, &mut left[..words]);
					source.read(plan.nodes[b].operand, &mut right[..words]);
					match op {
						Binary::BitAnd => {
							for i in 0..words {
								left[i] &= right[i];
							}
						}
						Binary::BitOr => {
							for i in 0..words {
								left[i] |= right[i];
							}
						}
						Binary::BitXor | Binary::Ne => {
							for i in 0..words {
								left[i] ^= right[i];
							}
						}
						Binary::Eq => {
							for i in 0..words {
								left[i] = !(left[i] ^ right[i]);
							}
						}
						_ => unreachable!("verified Boolean operation"),
					}
				}
			}
			left[words - 1] &= low_mask((count - 1) % 64 + 1);
			match node.operand {
				Operand::Slot(slot) => {
					scratch[slot * WORDS..slot * WORDS + words].copy_from_slice(&left[..words])
				}
				Operand::Output => result_words[..words].copy_from_slice(&left[..words]),
				_ => unreachable!("computed destination"),
			}
		}
		Sources {
			inputs: &inputs,
			offsets: &offsets,
			scratch: &scratch,
			output: &result_words,
			len: count,
		}
		.read(plan.nodes[plan.result].operand, &mut left[..words]);
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
	Ok(ColumnResult {
		values,
		validity,
		len,
		valid_rows,
		remaining_work: options.work - charge,
		path: Path::Scalar,
	})
}
