use super::*;
use rune::experimental_core::value::Scalar;

/// Physical values and a separately offset validity mask.
#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
	pub values: ColumnSlice<'a>,
	pub validity: Option<BitSlice<'a>>,
}

/// Borrowed descriptors; their owner includes capacity in Options::adapter_bytes.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
	pub ty: ColumnType,
	pub chunks: &'a [Chunk<'a>],
}

pub(super) fn validate(inputs: &[Input<'_>], types: &[ColumnType]) -> Result<usize, Cause> {
	if inputs.len() != types.len() {
		return Err(Cause::Invalid("input arity"));
	}
	let mut length = None;
	for (input, ty) in inputs.iter().zip(types) {
		if input.ty != *ty {
			return Err(Cause::Invalid("input type"));
		}
		let mut len = 0usize;
		for chunk in input.chunks {
			if chunk.values.column_type() != *ty {
				return Err(Cause::Invalid("chunk type"));
			}
			if chunk
				.validity
				.is_some_and(|bits| bits.len() != chunk.values.len())
			{
				return Err(Cause::Invalid("validity length"));
			}
			len = len
				.checked_add(chunk.values.len())
				.ok_or(Cause::Invalid("length overflow"))?;
		}
		if length.is_some_and(|expected| expected != len) {
			return Err(Cause::Invalid("input lengths"));
		}
		length = Some(len);
	}
	Ok(length.unwrap_or(0))
}

pub(super) fn reserve<T>(len: usize, room: &mut usize) -> Result<Vec<T>, Cause> {
	let bytes = len.checked_mul(size_of::<T>()).ok_or(Cause::Memory)?;
	if bytes > *room {
		return Err(Cause::Memory);
	}
	let mut values = Vec::new();
	values.try_reserve_exact(len).map_err(|_| Cause::Memory)?;
	*room = room
		.checked_sub(
			values
				.capacity()
				.checked_mul(size_of::<T>())
				.ok_or(Cause::Memory)?,
		)
		.ok_or(Cause::Memory)?;
	Ok(values)
}

pub(super) fn filled<T: Clone>(len: usize, value: T, room: &mut usize) -> Result<Vec<T>, Cause> {
	let mut out = reserve(len, room)?;
	out.resize(len, value);
	Ok(out)
}

pub(super) fn descriptors<'a>(
	inputs: &[Input<'a>],
	room: usize,
) -> Result<([Vec<core::ColumnChunk<'a>>; 3], usize), ColumnRefusal> {
	let mut left = room;
	let mut all = [Vec::new(), Vec::new(), Vec::new()];
	for (index, input) in inputs.iter().enumerate() {
		let mut chunks =
			reserve(input.chunks.len(), &mut left).map_err(|_| ColumnRefusal::Memory)?;
		for chunk in input.chunks {
			chunks.push(core::ColumnChunk::new(chunk.values, chunk.validity)?);
		}
		all[index] = chunks;
	}
	Ok((all, room - left))
}

#[derive(Clone, Copy, Default)]
pub(super) struct Cursor {
	chunk: usize,
	offset: usize,
}
impl Cursor {
	fn current<'a>(&mut self, input: Input<'a>) -> Chunk<'a> {
		while self.offset == input.chunks[self.chunk].values.len() {
			self.chunk += 1;
			self.offset = 0;
		}
		input.chunks[self.chunk]
	}
	pub fn valid(&mut self, input: Input<'_>) -> bool {
		let chunk = self.current(input);
		chunk
			.validity
			.is_none_or(|bits| bits.get(self.offset) == Some(true))
	}
	pub fn value(&mut self, input: Input<'_>) -> Scalar {
		match self.current(input).values {
			ColumnSlice::I64(values) => Scalar::I64(values[self.offset]),
			ColumnSlice::F64(values) => Scalar::F64(values[self.offset]),
			ColumnSlice::Bool(values) => {
				Scalar::Bool(values.get(self.offset).expect("validated bit index"))
			}
		}
	}
	pub fn advance(&mut self) {
		self.offset += 1;
	}
}
