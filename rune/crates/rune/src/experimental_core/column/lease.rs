use super::{ColumnRefusal, ColumnType, Result};

/// A checked immutable bit range. Offsets need not be byte aligned.
#[derive(Clone, Copy, Debug)]
pub struct BitSlice<'a> {
	bytes: &'a [u8],
	offset: usize,
	len: usize,
}

impl<'a> BitSlice<'a> {
	/// Validate that every logical bit is accessible; reject overflow.
	pub fn new(bytes: &'a [u8], offset: usize, len: usize) -> Result<Self> {
		let end = offset.checked_add(len).ok_or(ColumnRefusal::Bounds)?;
		if end.div_ceil(8) > bytes.len() {
			return Err(ColumnRefusal::Bounds);
		}
		Ok(Self { bytes, offset, len })
	}
	/// Logical length, independent of byte padding.
	pub fn len(self) -> usize {
		self.len
	}
	/// Whether there are no logical bits.
	pub fn is_empty(self) -> bool {
		self.len == 0
	}
	/// Read one logical bit; out-of-range indices return None.
	pub fn get(self, index: usize) -> Option<bool> {
		if index >= self.len {
			return None;
		}
		let bit = self.offset + index;
		Some(self.bytes[bit / 8] & (1 << (bit % 8)) != 0)
	}
	pub(super) fn bytes_at(self, start: usize, len: usize) -> Option<&'a [u8]> {
		let bit = self.offset.checked_add(start)?;
		if start.checked_add(len)? > self.len || !bit.is_multiple_of(8) || !len.is_multiple_of(8) {
			return None;
		}
		self.bytes.get(bit / 8..bit / 8 + len / 8)
	}
	pub(super) fn bit(self, index: usize) -> bool {
		self.get(index).expect("verified bit range")
	}
	/// Copy a validated physical block into little-endian words. Bounds and
	/// byte alignment are resolved once, not once per logical word.
	pub(super) fn read_words(self, start: usize, len: usize, out: &mut [u64]) {
		assert!(start <= self.len && len <= self.len - start);
		assert_eq!(out.len(), len.div_ceil(64));
		let full = len / 64;
		let bit = self.offset + start;
		let shift = bit % 8;
		let bytes = &self.bytes[bit / 8..(bit + len).div_ceil(8)];
		if shift == 0 {
			for (bytes, word) in bytes[..full * 8]
				.as_chunks::<8>()
				.0
				.iter()
				.zip(&mut out[..full])
			{
				*word = u64::from_le_bytes(*bytes);
			}
		} else {
			for (bytes, word) in bytes.windows(9).step_by(8).zip(&mut out[..full]) {
				*word = (u64::from_le_bytes(bytes[..8].try_into().expect("word")) >> shift)
					| (u64::from(bytes[8]) << (64 - shift));
			}
		}
		if !len.is_multiple_of(64) {
			out[full] = self.word(start + full * 64, len % 64);
		}
	}
	/// Read up to one word without touching bytes beyond the checked bit range.
	#[inline]
	pub(super) fn word(self, start: usize, len: usize) -> u64 {
		assert!(len <= 64 && start <= self.len && len <= self.len - start);
		if len == 0 {
			return 0;
		}
		let bit = self.offset + start;
		let shift = bit % 8;
		let bytes = &self.bytes[bit / 8..(bit + len).div_ceil(8)];
		let mut word = if bytes.len() >= 8 {
			u64::from_le_bytes(bytes[..8].try_into().expect("word"))
		} else {
			let mut tail = [0; 8];
			tail[..bytes.len()].copy_from_slice(bytes);
			u64::from_le_bytes(tail)
		};
		word >>= shift;
		if bytes.len() == 9 {
			word |= u64::from(bytes[8]) << (64 - shift);
		}
		word & low_mask(len)
	}
}

#[inline]
pub(super) fn low_mask(len: usize) -> u64 {
	assert!(len <= 64);
	u64::MAX.checked_shr((64 - len) as u32).unwrap_or(0)
}

// Append into a zero-initialized packed output, preserving earlier bits of a
// partial byte. Each logical range is written exactly once in ascending order.
#[inline]
pub(super) fn append_word(out: &mut [u8], start: usize, len: usize, word: u64) {
	assert!(len <= 64);
	if len == 0 {
		return;
	}
	if len == 64 && start.is_multiple_of(8) {
		out[start / 8..start / 8 + 8].copy_from_slice(&word.to_le_bytes());
		return;
	}
	let shift = start % 8;
	let bytes = &mut out[start / 8..(start + len).div_ceil(8)];
	let word = word & low_mask(len);
	let low = (word << shift).to_le_bytes();
	bytes[0] |= low[0];
	let count = bytes.len().min(8);
	bytes[1..count].copy_from_slice(&low[1..count]);
	if bytes.len() == 9 {
		bytes[8] = (word >> (64 - shift)) as u8;
	}
}

/// Append a physical block, carrying unaligned prefix bits between stores.
pub(super) fn append_words(out: &mut [u8], start: usize, len: usize, words: &[u64]) {
	assert_eq!(words.len(), len.div_ceil(64));
	if len == 0 {
		return;
	}
	let full = len / 64;
	let shift = start % 8;
	let bytes = &mut out[start / 8..(start + len).div_ceil(8)];
	if shift == 0 {
		for (dst, word) in bytes[..full * 8]
			.as_chunks_mut::<8>()
			.0
			.iter_mut()
			.zip(words)
		{
			dst.copy_from_slice(&word.to_le_bytes());
		}
	} else {
		let mut carry = u64::from(bytes[0]);
		for (dst, word) in bytes[..full * 8]
			.as_chunks_mut::<8>()
			.0
			.iter_mut()
			.zip(words)
		{
			dst.copy_from_slice(&((word << shift) | carry).to_le_bytes());
			carry = word >> (64 - shift);
		}
		bytes[full * 8] = carry as u8;
	}
	if !len.is_multiple_of(64) {
		append_word(out, start + full * 64, len % 64, words[full]);
	}
}

/// Physical values borrowed from one immutable input chunk.
#[derive(Clone, Copy, Debug)]
pub enum ColumnSlice<'a> {
	/// Numeric slices are already offset by their owner.
	I64(&'a [i64]),
	/// Floating point slices are already offset by their owner.
	F64(&'a [f64]),
	/// Packed values, with an offset independent of validity.
	Bool(BitSlice<'a>),
}
impl ColumnSlice<'_> {
	/// Logical element count.
	pub fn len(self) -> usize {
		match self {
			Self::I64(v) => v.len(),
			Self::F64(v) => v.len(),
			Self::Bool(v) => v.len(),
		}
	}
	/// Whether this chunk has no elements.
	pub fn is_empty(self) -> bool {
		self.len() == 0
	}
	/// Physical scalar type.
	pub fn column_type(self) -> ColumnType {
		match self {
			Self::I64(_) => ColumnType::I64,
			Self::F64(_) => ColumnType::F64,
			Self::Bool(_) => ColumnType::Bool,
		}
	}
}

/// Values and validity with equal logical lengths. Safe borrowing keeps the
/// owner alive and prevents simultaneous mutable access to an input buffer.
#[derive(Clone, Copy, Debug)]
pub struct ColumnChunk<'a> {
	pub(super) values: ColumnSlice<'a>,
	pub(super) validity: Option<BitSlice<'a>>,
}
impl<'a> ColumnChunk<'a> {
	/// Pair a values slice and optional null mask without reading null payloads.
	pub fn new(values: ColumnSlice<'a>, validity: Option<BitSlice<'a>>) -> Result<Self> {
		if validity.is_some_and(|v| v.len() != values.len()) {
			return Err(ColumnRefusal::Bounds);
		}
		Ok(Self { values, validity })
	}
}

/// A typed, independently chunked input. No concatenation or copy is needed.
#[derive(Clone, Copy, Debug)]
pub struct Column<'a> {
	pub(super) chunks: &'a [ColumnChunk<'a>],
	pub(super) ty: ColumnType,
	pub(super) len: usize,
}
impl<'a> Column<'a> {
	/// Verify the physical dtype of every chunk, including empty chunks.
	pub fn new(ty: ColumnType, chunks: &'a [ColumnChunk<'a>]) -> Result<Self> {
		let mut len = 0usize;
		for chunk in chunks {
			if chunk.values.column_type() != ty {
				return Err(ColumnRefusal::Type);
			}
			len = len
				.checked_add(chunk.values.len())
				.ok_or(ColumnRefusal::Bounds)?;
		}
		Ok(Self { chunks, ty, len })
	}
	/// Total logical length across all chunks.
	pub fn len(self) -> usize {
		self.len
	}
	/// Whether all chunks are empty.
	pub fn is_empty(self) -> bool {
		self.len == 0
	}
	/// Physical type shared by every chunk.
	pub fn column_type(self) -> ColumnType {
		self.ty
	}
}

#[cfg(test)]
mod word_tests {
	use super::*;

	#[test]
	fn exact_ranges_offsets_tails_and_adjacent_writes() {
		let input: std::vec::Vec<_> = (0..40).map(|i| (i * 79 + 23) as u8).collect();
		let lengths: std::vec::Vec<usize> = if cfg!(miri) {
			std::vec![0, 1, 7, 8, 9, 63, 64, 65, 129]
		} else {
			(0..=129).collect()
		};
		for offset in 0usize..8 {
			for &len in &lengths {
				let bits =
					BitSlice::new(&input[..(offset + len).div_ceil(8)], offset, len).unwrap();
				for start in 0..=len {
					let n = (len - start).min(64);
					let expected = (0..n).fold(0, |v, i| v | (u64::from(bits.bit(start + i)) << i));
					assert_eq!(bits.word(start, n), expected);
				}
				for chunk in [1, 7, 8, 9, 31, 63, 64] {
					let mut out = std::vec![0; (len + offset).div_ceil(8) + 2];
					out[0] = 0xa5;
					let end = out.len() - 1;
					out[end] = 0x5a;
					for start in (0..len).step_by(chunk) {
						let n = (len - start).min(chunk);
						append_word(&mut out[1..end], offset + start, n, bits.word(start, n));
					}
					let actual = BitSlice::new(&out[1..end], offset, len).unwrap();
					for i in 0..len {
						assert_eq!(actual.bit(i), bits.bit(i));
					}
					assert_eq!((out[0], out[end]), (0xa5, 0x5a));
					if !(offset + len).is_multiple_of(8) {
						assert_eq!(out[end - 1] >> ((offset + len) % 8), 0);
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod block_word_tests {
	use super::{append_words, BitSlice};
	use std::vec::Vec;

	#[test]
	fn physical_block_copy_offsets_tails_and_prefix_canaries() {
		let input: Vec<u8> = (0..140).map(|i| (i * 73 + 19) as u8).collect();
		for offset in 0..64 {
			for len in [0usize, 1, 7, 8, 9, 63, 64, 65, 127, 128, 1023, 1024] {
				let source = BitSlice::new(&input, offset, len).unwrap();
				let mut words = std::vec![0; len.div_ceil(64)];
				source.read_words(0, len, &mut words);
				for (i, word) in words.iter().enumerate() {
					assert_eq!(*word, source.word(i * 64, (len - i * 64).min(64)));
				}
				for prefix in 0..8 {
					let mut output = std::vec![0u8; (len + prefix).div_ceil(8) + 1];
					output[0] = (1u16 << prefix).wrapping_sub(1) as u8;
					let last = output.len() - 1;
					output[last] = 0x5a;
					append_words(&mut output[..last], prefix, len, &words);
					assert_eq!(output[last], 0x5a);
					let view = BitSlice::new(&output, 0, len + prefix).unwrap();
					for i in 0..prefix {
						assert!(view.bit(i));
					}
					for i in 0..len {
						assert_eq!(view.bit(prefix + i), source.bit(i));
					}
				}
			}
		}
	}
}
