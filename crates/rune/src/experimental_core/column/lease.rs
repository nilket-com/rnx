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
