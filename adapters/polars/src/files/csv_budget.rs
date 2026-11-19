use polars::prelude::{DataType, Schema};
use polars_io::csv::read::SplitLines;

// The pinned CSV splitter uses u32 offsets. The stricter signed Arrow
// binview buffer limit also bounds each field and the row/index arithmetic.
pub(super) const MAX_BYTES: usize = i32::MAX as usize - 1;
const MIB: usize = 1024 * 1024;
const INITIAL_STRING_BLOCK: usize = 8192;
const INLINE_STRING: usize = 12;

pub(super) struct Snapshot {
	lines: usize,
	width: usize,
	max_record: usize,
	quoted: bool,
	rectangular: bool,
	long_bytes: usize,
	long_fields: usize,
	long_columns: usize,
	long_string_columns: usize,
	bytes: usize,
}

impl Snapshot {
	pub(super) fn scan(input: &[u8], schema: &Schema, scratch: usize) -> Option<Self> {
		if input.len() > MAX_BYTES || schema.len() > MAX_BYTES {
			return None;
		}
		let quoted = input.contains(&b'"');
		let mut layout = Self {
			lines: input.iter().filter(|&&byte| byte == b'\n').count()
				+ usize::from(!input.is_empty() && !input.ends_with(b"\n")),
			width: 0,
			max_record: 0,
			quoted,
			rectangular: !quoted,
			long_bytes: 0,
			long_fields: 0,
			long_columns: 0,
			long_string_columns: 0,
			bytes: input.len(),
		};
		// One byte per declared column. No width-dependent scratch on the quoted
		// fallback, and no allocation depending on inferred/untrusted width.
		let mut long = Vec::new();
		if !quoted {
			if schema.len() > scratch {
				return None;
			}
			long.try_reserve_exact(schema.len()).ok()?;
			if long.capacity() > scratch {
				return None;
			}
			long.resize(schema.len(), false);
		}
		for record in SplitLines::new(input, Some(b'"'), b'\n', None) {
			layout.max_record = layout.max_record.max(record.len());
			let mut width = 1;
			if quoted {
				width += record.iter().filter(|&&byte| byte == b',').count();
			} else {
				for (position, field) in record.split(|&byte| byte == b',').enumerate() {
					width = position + 1;
					if field.len() > INLINE_STRING {
						layout.long_bytes += field.len();
						layout.long_fields += 1;
						if let Some(slot) = long.get_mut(position) {
							*slot = true;
						}
					}
				}
			}
			layout.width = layout.width.max(width);
			layout.rectangular &= width == schema.len();
		}
		layout.long_columns = long.iter().filter(|&&found| found).count();
		layout.long_string_columns = schema
			.iter_values()
			.zip(long)
			.filter(|(dtype, found)| *found && **dtype == DataType::String)
			.count();
		Some(layout)
	}

	pub(super) fn phase(&self, schema: &Schema, threads: usize, header: bool) -> Option<usize> {
		let width = if header {
			self.width.max(1)
		} else {
			schema.len().max(1)
		};
		let chunks = self.chunks(width, threads)?;
		let mut stride = 0usize;
		let mut strings = 0usize;
		let mut name_bytes = 0usize;
		let mut name_max = 0usize;
		for (name, dtype) in schema.iter() {
			name_bytes = name_bytes.checked_add(name.len())?;
			name_max = name_max.max(name.len());
			let cell = match dtype {
				DataType::Int64 | DataType::Float64 => 8,
				DataType::Boolean => 1,
				DataType::String => {
					strings += 1;
					16
				}
				_ => return None,
			};
			stride = stride.checked_add(cell)?;
		}
		if header {
			stride = 16usize.checked_mul(width)?;
			strings = width;
		}
		// read_chunk reserves count + 1 per builder. On a quote disagreement,
		// charge up to twice the physical rows and the previous live buffer.
		let slots = self
			.lines
			.checked_add(chunks)?
			.checked_mul(if self.quoted { 3 } else { 1 })?;
		let validity = slots.div_ceil(8).checked_add(chunks.checked_mul(16)?)?;
		let rows = slots
			.checked_mul(stride)?
			.checked_add(width.checked_mul(validity)?)?;

		let long_bytes = if self.quoted {
			self.bytes
		} else {
			self.long_bytes
		};
		let starts = if self.rectangular {
			let columns = if header {
				self.long_columns
			} else {
				self.long_string_columns
			};
			chunks.checked_mul(columns)?.min(self.long_fields)
		} else {
			chunks.checked_mul(strings)?
		};
		let strings_bytes = if long_bytes == 0 || strings == 0 {
			0
		} else {
			let block_data = long_bytes.checked_mul(4)?;
			let block_metadata = long_bytes
				.div_ceil(INITIAL_STRING_BLOCK / 2)
				.checked_mul(512)?;
			block_data
				.checked_add(block_metadata)?
				.checked_add(starts.checked_mul(INITIAL_STRING_BLOCK + 512)?)?
		};
		let escape = if self.quoted && strings > 0 {
			self.bytes.checked_mul(3)?
		} else {
			0
		};

		// The 1 KiB column slot covers the Builder, resulting Column and Series,
		// Arrow array and storage wrappers, append-time pointers and coexistence.
		// Field text is separate: CompactString clones may allocate.
		let column_chunks = width.checked_mul(chunks)?;
		let metadata = column_chunks
			.checked_mul(1024)?
			.checked_add(width.max(self.width).checked_mul(4096)?)?
			.checked_add(chunks.checked_mul(1024)?)?
			.checked_add(threads.checked_mul(4096)?)?
			.checked_add(512 * 1024)?;
		let names = self
			.max_record
			.checked_mul(32)?
			.checked_add(name_bytes.checked_mul(8usize.checked_mul(chunks.checked_add(1)?)?)?)?;
		// One result/error per chunk. The outer CSV error repeats the lossy
		// field twice and can include an inner error containing it again.
		// Include growth, the retained error collection and copying to String.
		let error_input = if self.quoted {
			self.bytes
		} else {
			self.max_record.checked_mul(chunks)?.min(self.bytes)
		};
		let errors = error_input.checked_mul(64)?.checked_add(
			name_max
				.checked_add(2048)?
				.checked_mul(chunks)?
				.checked_mul(4)?,
		)?;

		rows.checked_add(strings_bytes)?
			.checked_add(escape)?
			.checked_add(metadata)?
			.checked_add(names)?
			.checked_add(errors)
	}

	fn chunks(&self, width: usize, threads: usize) -> Option<usize> {
		let minimum = if cfg!(debug_assertions) { 64 } else { 4096 };
		let hint = (500_000 / width.max(1))
			.max(threads)
			.min(threads.checked_mul(16)?)
			.max(1);
		// The first cut consumes through the last eligible newline in its
		// search window. Its successor reaches past that window. Bound pairs
		// by the minimum initial window, the 16 MiB cap and rounding down.
		let windows = self
			.bytes
			.div_ceil(16 * MIB)
			.max(hint.checked_add(hint.div_ceil(minimum))?);
		Some(self.lines.min(windows.checked_mul(2)?.checked_add(2)?))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polars::prelude::PlSmallStr;

	#[test]
	fn string_block_positions_apply_only_to_rectangular_unquoted_input() {
		let schema = Schema::from_iter([
			(PlSmallStr::from_static("a"), DataType::String),
			(PlSmallStr::from_static("b"), DataType::String),
			(PlSmallStr::from_static("c"), DataType::Int64),
		]);
		let plain = Snapshot::scan(b"a,b,c\n0123456789012,012345678901,1\n", &schema, 3).unwrap();
		assert!(plain.rectangular);
		assert_eq!(
			(
				plain.long_fields,
				plain.long_columns,
				plain.long_string_columns
			),
			(1, 1, 1)
		);
		let estimate = plain.phase(&schema, 28, true).unwrap();
		let mut fallback = plain;
		fallback.rectangular = false;
		assert!(fallback.phase(&schema, 28, true).unwrap() > estimate);
		let ragged = Snapshot::scan(b"a,b,c\n0123456789012,1\n", &schema, 3).unwrap();
		assert!(!ragged.rectangular);
		let quoted = Snapshot::scan(b"a,b,c\n\"x,\ny\",z,1\n", &schema, 0).unwrap();
		assert!(quoted.quoted);
		assert_eq!(quoted.lines, 3);
		assert_eq!(quoted.width, 4);
		assert!(Snapshot::scan(b"a,b,c\n", &schema, 2).is_none());
		assert!(fallback.phase(&schema, usize::MAX, true).is_none());
	}
}
