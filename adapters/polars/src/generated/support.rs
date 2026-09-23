//! Hand-written support for the generated bindings: the error value,
//! checked integer narrowing, and taking wrapped values out of Rune
//! containers. Not generated; lives beside the generated files because
//! only they use it.
#![allow(dead_code)]
use polars::prelude as p;
use rnx::rune;
use rune::runtime::Formatter;
use rune::Any;

/// A Polars error as a Rune value: `kind()` is the variant name Rust
/// matches on, display is Polars's message.
#[derive(Any, Debug, Clone)]
#[rune(item = ::polars)]
pub struct Error(pub(crate) String, pub(crate) String);

impl From<p::PolarsError> for Error {
	fn from(e: p::PolarsError) -> Self {
		// The kind is the innermost error's: Polars wraps a failure in
		// `Context`/`ExprContext` on some paths (`compute_schema`) and not
		// on others (`collect`), and the wrapper is not a kind of its own.
		let mut inner = &e;
		while let p::PolarsError::Context { error, .. } | p::PolarsError::ExprContext { error, .. } = inner { inner = error; }
		let kind = format!("{inner:?}");
		let kind = kind.split(['(', ' ', '{']).next().unwrap_or("Unknown").to_string();
		let message = e.to_string();
		let kind = if kind == "ComputeError" && message.starts_with("callback ") { "CallbackError".into() } else { kind };
		Error(kind, message)
	}
}

/// Record 0077: iterator returns are materialized into vectors inside the
/// binding, under an item-count bound. The contract is inclusive: exactly
/// `limit` items succeed, `limit + 1` refuse with a `MaterializeLimit`
/// error and no prefix. A length is trusted only from `ExactSizeIterator`
/// (`len()`), never from a `size_hint`; an unknown-length iterator is
/// driven one item at a time, at most `limit` items are converted, and
/// excess is detected by one further `next` whose item is discarded
/// without conversion. Nothing is reserved from a hint.
pub(crate) const MATERIALIZE_LIMIT: usize = 1 << 20;

#[cfg(feature = "test-support")]
static TEST_LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Unit tests that set or read `TEST_LIMIT` hold this lock, so a test that
/// lowers the bound cannot race one that expects the production value.
#[cfg(all(test, feature = "test-support"))]
pub(crate) static LIMIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The bound in force: the production constant, or under `test-support`
/// the override set by `polars::set_materialize_limit` (0 = production).
pub(crate) fn materialize_limit() -> usize {
	#[cfg(feature = "test-support")]
	{
		let t = TEST_LIMIT.load(std::sync::atomic::Ordering::SeqCst);
		if t > 0 {
			return t;
		}
	}
	MATERIALIZE_LIMIT
}

/// Test-support only: a low bound for the controls; 0 restores the
/// production bound.
#[cfg(feature = "test-support")]
#[rune::function(path = set_materialize_limit)]
pub(crate) fn set_materialize_limit(n: i64) {
	TEST_LIMIT.store(n.max(0) as usize, std::sync::atomic::Ordering::SeqCst);
}

/// Materialize an iterator whose length is known exactly: a longer one
/// refuses before any `next` call; the count guard stays in place after.
pub(crate) fn materialize_exact<I: ExactSizeIterator, T>(it: I, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	materialize_exact_with(it, materialize_limit(), method, conv)
}

/// Materialize an iterator of unknown length: at most `limit` items are
/// converted; one further `next` decides, its item discarded.
pub(crate) fn materialize_unknown<I: Iterator, T>(it: I, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	materialize_unknown_with(it, materialize_limit(), method, conv)
}

/// Materialize an iterator that exposes only `TrustedLen`, whose contract
/// makes `size_hint`'s upper bound the exact length or `None` when the
/// length is not representable: an upper bound over the limit, or no
/// upper bound, refuses before any `next` call; the count guard stays.
pub(crate) fn materialize_trusted<I: polars_arrow::trusted_len::TrustedLen, T>(it: I, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	materialize_trusted_with(it, materialize_limit(), method, conv)
}

pub(crate) fn materialize_trusted_with<I: polars_arrow::trusted_len::TrustedLen, T>(it: I, limit: usize, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	match it.size_hint().1 {
		Some(n) if n > limit => return Err(Error("MaterializeLimit".into(), format!("{method}: {n} items (TrustedLen upper bound), more than the bound of {limit}"))),
		Some(_) => {}
		None => return Err(Error("MaterializeLimit".into(), format!("{method}: the TrustedLen upper bound is not representable"))),
	}
	materialize_unknown_with(it, limit, method, conv)
}

pub(crate) fn materialize_exact_with<I: ExactSizeIterator, T>(it: I, limit: usize, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	let n = it.len();
	if n > limit {
		return Err(Error("MaterializeLimit".into(), format!("{method}: {n} items, more than the bound of {limit}")));
	}
	materialize_unknown_with(it, limit, method, conv)
}

/// Record 0099: the whole cost of a chunk snapshot, one slot per chunk and
/// one per cell, against the inclusive materialize bound, with checked
/// arithmetic, before anything is allocated.
pub(crate) fn snapshot_budget(chunks: usize, cells: usize, method: &str) -> Result<usize, Error> {
	let limit = materialize_limit();
	match chunks.checked_add(cells) {
		Some(t) if t <= limit => Ok(t),
		Some(t) => Err(Error("MaterializeLimit".into(), format!("{method}: {chunks} chunks and {cells} cells ({t} slots), more than the bound of {limit}"))),
		None => Err(Error("MaterializeLimit".into(), format!("{method}: {chunks} chunks and {cells} cells overflow the slot count, more than the bound of {limit}"))),
	}
}

/// Record 0099: `ChunkedArray::chunks` as owned nested options: one inner
/// vector per chunk (empty chunks kept), values and nulls in order, each
/// value through the scalar rule. The total is bounded before allocation and
/// counted again while copying; a chunk that is not `PrimitiveArray<N>` is a
/// typed error, and nothing partial is returned.
pub(crate) fn chunk_snapshot<N: polars_arrow::types::NativeType, U>(chunks: &[polars_arrow::array::ArrayRef], method: &str, mut conv: impl FnMut(N) -> Result<U, Error>) -> Result<Vec<Vec<Option<U>>>, Error> {
	let cells = chunks.iter().try_fold(0usize, |acc, a| acc.checked_add(a.len()));
	let cells = match cells {
		Some(c) => c,
		None => return Err(Error("MaterializeLimit".into(), format!("{method}: the cell count of {} chunks overflows, more than the bound of {}", chunks.len(), materialize_limit()))),
	};
	let total = snapshot_budget(chunks.len(), cells, method)?;
	let mut out = Vec::with_capacity(chunks.len());
	let mut used = 0usize;
	for arr in chunks {
		let arr = arr.as_any().downcast_ref::<polars_arrow::array::PrimitiveArray<N>>().ok_or_else(|| Error::conversion(&format!("{method}: a chunk is not a PrimitiveArray<{}>", std::any::type_name::<N>())))?;
		used = used.checked_add(1 + arr.len()).filter(|u| *u <= total).ok_or_else(|| Error("MaterializeLimit".into(), format!("{method}: more slots copied than the {total} counted, within the bound of {}", materialize_limit())))?;
		let mut v = Vec::with_capacity(arr.len());
		for x in arr.iter() {
			v.push(match x { Some(x) => Some(conv(*x)?), None => None });
		}
		out.push(v);
	}
	Ok(out)
}

/// Record 0100: the whole cost of a Boolean, string or binary chunk
/// snapshot: one slot per chunk, per cell and per payload byte, checked
/// arithmetic, against the inclusive bound, before anything is allocated.
pub(crate) fn payload_snapshot_budget(chunks: usize, cells: usize, bytes: usize, method: &str) -> Result<usize, Error> {
	let limit = materialize_limit();
	match chunks.checked_add(cells).and_then(|t| t.checked_add(bytes)) {
		Some(t) if t <= limit => Ok(t),
		Some(t) => Err(Error("MaterializeLimit".into(), format!("{method}: {chunks} chunks, {cells} cells and {bytes} bytes ({t} slots), more than the bound of {limit}"))),
		None => Err(Error("MaterializeLimit".into(), format!("{method}: {chunks} chunks, {cells} cells and {bytes} bytes overflow the slot count, more than the bound of {limit}"))),
	}
}

/// Record 0100: which running sum a preflight step adds to.
#[derive(Clone, Copy)]
pub(crate) enum PayloadCount {
	Cells,
	Bytes,
}
/// Record 0100: one checked preflight addition; an overflow names the
/// operation, every count so far, what was being added and the bound.
pub(crate) fn payload_count_step(chunks: usize, cells: usize, bytes: usize, add: usize, to: PayloadCount, method: &str) -> Result<usize, Error> {
	let (sum, what) = match to {
		PayloadCount::Cells => (cells.checked_add(add), "cells"),
		PayloadCount::Bytes => (bytes.checked_add(add), "bytes"),
	};
	sum.ok_or_else(|| Error("MaterializeLimit".into(), format!("{method}: {chunks} chunks, {cells} cells and {bytes} bytes counted; adding {add} {what} overflows, more than the bound of {}", materialize_limit())))
}

/// Record 0100: the shared core of the payload snapshots. A preflight over
/// the borrowed chunks downcasts each (a typed error names the expected
/// array) and sums cells and payload bytes with checked arithmetic, without
/// allocating; the whole result is bounded before any allocation; the copy
/// downcasts again and counts every chunk, cell and byte a second time;
/// nothing partial is returned.
fn payload_snapshot<A: polars_arrow::array::Array + 'static, V: ?Sized, U>(
	chunks: &[polars_arrow::array::ArrayRef],
	method: &str,
	expected: &str,
	iter: impl for<'a> Fn(&'a A) -> Box<dyn Iterator<Item = Option<&'a V>> + 'a>,
	size: impl Fn(&V) -> usize,
	copy: impl Fn(&V) -> U,
) -> Result<Vec<Vec<Option<U>>>, Error> {
	// preflight over the borrowed chunks: nothing proportional to the input
	// is allocated until the whole result is within the bound
	fn downcast<'a, A: 'static>(a: &'a polars_arrow::array::ArrayRef, method: &str, expected: &str) -> Result<&'a A, Error> {
		a.as_any().downcast_ref::<A>().ok_or_else(|| Error::conversion(&format!("{method}: a chunk is not a {expected}")))
	}
	let mut cells = 0usize;
	let mut bytes = 0usize;
	for a in chunks {
		let a = downcast::<A>(a, method, expected)?;
		cells = payload_count_step(chunks.len(), cells, bytes, a.len(), PayloadCount::Cells, method)?;
		for v in iter(a).flatten() {
			bytes = payload_count_step(chunks.len(), cells, bytes, size(v), PayloadCount::Bytes, method)?;
		}
	}
	let total = payload_snapshot_budget(chunks.len(), cells, bytes, method)?;
	let recount = || Error("MaterializeLimit".into(), format!("{method}: more slots copied than the {total} counted, within the bound of {}", materialize_limit()));
	let mut used = 0usize;
	let mut out = Vec::with_capacity(chunks.len());
	for a in chunks {
		let a = downcast::<A>(a, method, expected)?;
		used = used.checked_add(1 + a.len()).filter(|u| *u <= total).ok_or_else(recount)?;
		let mut row = Vec::with_capacity(a.len());
		for v in iter(a) {
			row.push(match v {
				Some(v) => {
					used = used.checked_add(size(v)).filter(|u| *u <= total).ok_or_else(recount)?;
					Some(copy(v))
				}
				None => None,
			});
		}
		out.push(row);
	}
	Ok(out)
}

/// Record 0100: `BooleanChunked::chunks` as owned nested options (no payload bytes).
pub(crate) fn chunk_snapshot_bool(chunks: &[polars_arrow::array::ArrayRef], method: &str) -> Result<Vec<Vec<Option<bool>>>, Error> {
	const T: bool = true;
	const F: bool = false;
	payload_snapshot::<polars_arrow::array::BooleanArray, bool, bool>(chunks, method, "BooleanArray", |a| Box::new(a.iter().map(|b| b.map(|b| if b { &T } else { &F }))), |_| 0, |b| *b)
}
/// Record 0100: `StringChunked::chunks`: owned UTF-8 strings, bytes counted.
pub(crate) fn chunk_snapshot_str(chunks: &[polars_arrow::array::ArrayRef], method: &str) -> Result<Vec<Vec<Option<String>>>, Error> {
	payload_snapshot::<polars_arrow::array::Utf8ViewArray, str, String>(chunks, method, "Utf8ViewArray", |a| Box::new(a.iter()), str::len, str::to_string)
}
/// Record 0100: `BinaryChunked::chunks`: raw bytes as script integers.
pub(crate) fn chunk_snapshot_binview(chunks: &[polars_arrow::array::ArrayRef], method: &str) -> Result<Vec<Vec<Option<Vec<i64>>>>, Error> {
	payload_snapshot::<polars_arrow::array::BinaryViewArray, [u8], Vec<i64>>(chunks, method, "BinaryViewArray", |a| Box::new(a.iter()), <[u8]>::len, |b| b.iter().map(|x| *x as i64).collect())
}
/// Record 0100: `BinaryOffsetChunked::chunks`: raw bytes as script integers.
pub(crate) fn chunk_snapshot_binary_offset(chunks: &[polars_arrow::array::ArrayRef], method: &str) -> Result<Vec<Vec<Option<Vec<i64>>>>, Error> {
	payload_snapshot::<polars_arrow::array::BinaryArray<i64>, [u8], Vec<i64>>(chunks, method, "BinaryArray<i64>", |a| Box::new(a.iter()), <[u8]>::len, |b| b.iter().map(|x| *x as i64).collect())
}

/// Record 0101: one selected numeric chunk from `downcast_get`, owned. An
/// absent chunk is `None` and copies nothing; a present one costs one chunk
/// slot plus its cells against the inclusive bound, checked before
/// allocation and re-counted while copying. Only this chunk is touched.
pub(crate) fn indexed_snapshot<N: polars_arrow::types::NativeType, U>(a: Option<&polars_arrow::array::PrimitiveArray<N>>, method: &str, mut conv: impl FnMut(N) -> Result<U, Error>) -> Result<Option<Vec<Option<U>>>, Error> {
	let Some(a) = a else { return Ok(None) };
	let total = snapshot_budget(1, a.len(), method)?;
	let mut used = 1usize;
	let mut out = Vec::with_capacity(a.len());
	for x in a.iter() {
		used = used.checked_add(1).filter(|u| *u <= total).ok_or_else(|| Error("MaterializeLimit".into(), format!("{method}: more slots copied than the {total} counted, within the bound of {}", materialize_limit())))?;
		out.push(match x { Some(x) => Some(conv(*x)?), None => None });
	}
	Ok(Some(out))
}

/// Record 0101: the single-array core of the indexed payload snapshots:
/// cells and payload bytes of this one chunk summed with checked steps,
/// bounded (one chunk slot + cells + bytes) before allocation, re-counted
/// while copying; nothing partial.
fn payload_one<A, V: ?Sized, U>(a: &A, method: &str, cells: usize, iter: impl for<'a> Fn(&'a A) -> Box<dyn Iterator<Item = Option<&'a V>> + 'a>, size: impl Fn(&V) -> usize, copy: impl Fn(&V) -> U) -> Result<Vec<Option<U>>, Error> {
	let mut bytes = 0usize;
	for v in iter(a).flatten() {
		bytes = payload_count_step(1, cells, bytes, size(v), PayloadCount::Bytes, method)?;
	}
	let total = payload_snapshot_budget(1, cells, bytes, method)?;
	let recount = || Error("MaterializeLimit".into(), format!("{method}: more slots copied than the {total} counted, within the bound of {}", materialize_limit()));
	let mut used = 1usize;
	let mut out = Vec::with_capacity(cells);
	for v in iter(a) {
		used = used.checked_add(1).filter(|u| *u <= total).ok_or_else(recount)?;
		out.push(match v {
			Some(v) => {
				used = used.checked_add(size(v)).filter(|u| *u <= total).ok_or_else(recount)?;
				Some(copy(v))
			}
			None => None,
		});
	}
	Ok(out)
}
/// Record 0101: one selected Boolean chunk (no payload bytes).
pub(crate) fn indexed_snapshot_bool(a: Option<&polars_arrow::array::BooleanArray>, method: &str) -> Result<Option<Vec<Option<bool>>>, Error> {
	const T: bool = true;
	const F: bool = false;
	a.map(|a| payload_one::<polars_arrow::array::BooleanArray, bool, bool>(a, method, a.len(), |a| Box::new(a.iter().map(|b| b.map(|b| if b { &T } else { &F }))), |_| 0, |b| *b)).transpose()
}
/// Record 0101: one selected string chunk, UTF-8 bytes counted.
pub(crate) fn indexed_snapshot_str(a: Option<&polars_arrow::array::Utf8ViewArray>, method: &str) -> Result<Option<Vec<Option<String>>>, Error> {
	a.map(|a| payload_one::<polars_arrow::array::Utf8ViewArray, str, String>(a, method, a.len(), |a| Box::new(a.iter()), str::len, str::to_string)).transpose()
}
/// Record 0101: one selected binary-view chunk, raw bytes as script integers.
pub(crate) fn indexed_snapshot_binview(a: Option<&polars_arrow::array::BinaryViewArray>, method: &str) -> Result<Option<Vec<Option<Vec<i64>>>>, Error> {
	a.map(|a| payload_one::<polars_arrow::array::BinaryViewArray, [u8], Vec<i64>>(a, method, a.len(), |a| Box::new(a.iter()), <[u8]>::len, |b| b.iter().map(|x| *x as i64).collect())).transpose()
}
/// Record 0101: one selected offset-binary chunk, raw bytes as script integers.
pub(crate) fn indexed_snapshot_binary_offset(a: Option<&polars_arrow::array::BinaryArray<i64>>, method: &str) -> Result<Option<Vec<Option<Vec<i64>>>>, Error> {
	a.map(|a| payload_one::<polars_arrow::array::BinaryArray<i64>, [u8], Vec<i64>>(a, method, a.len(), |a| Box::new(a.iter()), <[u8]>::len, |b| b.iter().map(|x| *x as i64).collect())).transpose()
}

/// Record 0097: `head`, `limit` and `tail` reach Polars's `slice_offsets`,
/// which panics when the receiver is longer than `i64::MAX` (possible with
/// shared-buffer appends, record 0093). Checked before the call.
pub(crate) fn signed_len(n: usize, method: &str) -> Result<(), Error> {
	if n > i64::MAX as usize {
		return Err(Error::conversion(&format!("{method}: receiver length {n} is beyond i64::MAX, the range of Polars's slice offsets")));
	}
	Ok(())
}

/// Record 0096: the whole result of a null-aware vector return, checked
/// against the (inclusive) bound from the receiver's length before Polars
/// allocates either branch.
pub(crate) fn null_aware_bound(n: usize, method: &str) -> Result<(), Error> {
	let limit = materialize_limit();
	if n > limit {
		return Err(Error("MaterializeLimit".into(), format!("{method}: {n} items, more than the bound of {limit}")));
	}
	Ok(())
}

// Record 0082: a borrowed slice is copied into an owned vector under the
// materialize bound, counted cumulatively over every slice copied while
// the outermost guard is held: one binding's direct return, its nested
// slices, or every item of one materialized iterator. The bound is
// checked before any allocation; nothing partial is returned.
thread_local! { static SLICE_BUDGET: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) }; }
pub(crate) struct SliceBudget;
impl SliceBudget {
	pub(crate) fn enter() -> Self {
		SLICE_BUDGET.with(|b| { let (depth, used) = b.get(); b.set((depth + 1, if depth == 0 { 0 } else { used })); });
		Self
	}
}
impl Drop for SliceBudget {
	fn drop(&mut self) { SLICE_BUDGET.with(|b| { let (depth, used) = b.get(); b.set((depth - 1, used)); }); }
}
/// Reserve `n` elements of the cumulative bound, or refuse naming what
/// would have been copied; checked with `n > limit - used`, before any
/// allocation.
fn reserve(n: usize, what: &str, method: &str) -> Result<(), Error> {
	let limit = materialize_limit();
	let used = SLICE_BUDGET.with(|b| b.get().1);
	if used > limit || n > limit - used {
		return Err(Error("MaterializeLimit".into(), format!("{method}: {n} {what} with {used} already copied, more than the bound of {limit}")));
	}
	SLICE_BUDGET.with(|b| { let (depth, _) = b.get(); b.set((depth, used + n)); });
	Ok(())
}
pub(crate) fn copy_slice<T: Clone, U>(slice: &[T], method: &str, mut conv: impl FnMut(T) -> Result<U, Error>) -> Result<Vec<U>, Error> {
	let _guard = SliceBudget::enter();
	let n = slice.len();
	reserve(n, "slice elements", method)?;
	let mut out = Vec::with_capacity(n);
	for e in slice {
		out.push(conv(e.clone())?);
	}
	Ok(out)
}
// Record 0093: the generator reads `IdxSize` back with `as i64`, which is
// exact only while `IdxSize` is `u32` (Polars without `bigidx`); a build that
// widens it must fail here rather than wrap silently.
const _: () = assert!(std::mem::size_of::<p::IdxSize>() <= 4, "IdxSize read-back assumes u32; enable checked widening for bigidx");

// Record 0093: a proven-bounded `usize` (release `[[bounded_readbacks]]`:
// the length of, or an index into, a `Vec`/`IndexMap`, so at most
// `isize::MAX`) converts exactly with `as i64` on a target whose `usize` is at
// most 64 bits; the debug assertion re-checks the proof at run time in tests.
const _: () = assert!(std::mem::size_of::<usize>() <= 8, "bounded_usize assumes usize fits in 64 bits");
pub(crate) fn bounded_usize(v: usize) -> i64 {
	debug_assert!(v <= i64::MAX as usize, "a release-listed bounded read-back exceeded i64::MAX: {v}");
	v as i64
}

/// Record 0091: an index length Polars asserts is below `IdxSize::MAX`,
/// checked first so a script gets an error instead of a panic.
pub(crate) fn below_idx_max(v: usize, method: &str, param: &str) -> Result<usize, Error> {
	let max = p::IdxSize::MAX as usize;
	if v >= max {
		return Err(Error("OutOfBounds".into(), format!("{method}: {param} {v} must be below {max}")));
	}
	Ok(v)
}
/// Record 0087: an unsigned count or size as a script integer, refused
/// rather than wrapped when it exceeds `i64::MAX`.
pub(crate) fn widen<T: TryInto<i64> + std::fmt::Display + Copy>(v: T, method: &str) -> Result<i64, Error> {
	v.try_into().map_err(|_| Error::conversion(&format!("{method}: {v} does not fit a script integer")))
}
/// Record 0094: a categorical hash as its exact token, 16 lowercase
/// hexadecimal digits, so every `u64` reaches the script unchanged.
pub(crate) fn hash_token(v: u64) -> String {
	format!("{v:016x}")
}
/// Record 0094: a hash token back to its `u64`. Only the canonical form is
/// accepted (exactly 16 ASCII digits `0-9a-f`, no prefix or sign); anything
/// else is a `ConversionError` before Polars sees a value.
pub(crate) fn hash_from_token(s: &str, method: &str) -> Result<u64, Error> {
	let b = s.as_bytes();
	if b.len() == 16 && b.iter().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
		return Ok(b.iter().fold(0u64, |acc, c| (acc << 4) | u64::from(if c.is_ascii_digit() { c - b'0' } else { c - b'a' + 10 })));
	}
	let shown: String = s.chars().take(24).collect();
	let more = if s.chars().count() > 24 { "..." } else { "" };
	Err(Error::conversion(&format!("{method}: hash must be 16 lowercase hex digits, got {shown:?}{more}")))
}

/// Record 0098: the exact bits of a script's float or boolean array, read
/// from the wrapper a binding returned, for tests that must not rely on
/// formatted text (NaN payloads, the sign of zero).
#[cfg(feature = "test-support")]
pub mod float_bits {
	use super::super::types::{W_polars_core__datatypes__BooleanChunked, W_polars_core__datatypes__Float32Chunked, W_polars_core__datatypes__Float64Chunked};
	use rnx::rune;
	pub fn f64s(v: &rune::Value) -> Result<Vec<Option<u64>>, String> {
		v.borrow_ref::<W_polars_core__datatypes__Float64Chunked>().map_err(|e| e.to_string()).map(|w| w.0.iter().map(|x| x.map(f64::to_bits)).collect())
	}
	pub fn f32s(v: &rune::Value) -> Result<Vec<Option<u32>>, String> {
		v.borrow_ref::<W_polars_core__datatypes__Float32Chunked>().map_err(|e| e.to_string()).map(|w| w.0.iter().map(|x| x.map(f32::to_bits)).collect())
	}
	pub fn bools(v: &rune::Value) -> Result<Vec<Option<bool>>, String> {
		v.borrow_ref::<W_polars_core__datatypes__BooleanChunked>().map_err(|e| e.to_string()).map(|w| w.0.iter().collect())
	}
	/// The number of chunks of a returned float array.
	pub fn f64_chunks(v: &rune::Value) -> Result<usize, String> {
		v.borrow_ref::<W_polars_core__datatypes__Float64Chunked>().map_err(|e| e.to_string()).map(|w| w.0.chunks().len())
	}
}

/// Record 0094: receiver fixtures for the categorical hash methods. Polars
/// hands these out as `Arc`s from registries that keep only weak
/// references; the wrappers own the value, so each fixture is built and
/// unwrapped under one lock, which leaves no other strong reference for a
/// concurrent build to share. The names are fixed so both sides of a paired
/// case hash alike; the mapping's lookup hasher has a fixed seed for the
/// same reason.
#[cfg(feature = "test-support")]
pub mod categorical_fixtures {
	use polars_dtype::categorical::{CategoricalMapping, CategoricalPhysical, Categories, FrozenCategories};
	use polars_utils::aliases::{PlSeedableRandomStateQuality, SeedableFromU64SeedExt};
	use std::sync::{Arc, Mutex};
	static BUILD: Mutex<()> = Mutex::new(());
	/// A named `Categories`; its stable hash is in the upper half of `u64`.
	pub fn categories() -> Categories {
		let _g = BUILD.lock().unwrap_or_else(|e| e.into_inner());
		Arc::try_unwrap(Categories::new("rnx-0094-5".into(), "rnx".into(), CategoricalPhysical::U32)).unwrap_or_else(|_| panic!("fixture: categories are shared"))
	}
	/// Two frozen categories; the combined hash is in the upper half of `u64`.
	pub fn frozen_categories() -> FrozenCategories {
		let _g = BUILD.lock().unwrap_or_else(|e| e.into_inner());
		Arc::try_unwrap(FrozenCategories::new(["rnx-0094-5", "b"]).expect("fixture: unique strings")).unwrap_or_else(|_| panic!("fixture: frozen categories are shared"))
	}
	/// The lookup hasher every mapping fixture uses.
	pub fn lookup_hasher() -> PlSeedableRandomStateQuality {
		PlSeedableRandomStateQuality::seed_from_u64(94)
	}
	/// A mapping holding `rnx-0094-2` (id 0; stored and lookup hashes in the
	/// upper half) and `rnx-0094-1` (id 1; both in the lower half), room for 16.
	pub fn mapping() -> CategoricalMapping {
		let m = CategoricalMapping::with_hasher(16, lookup_hasher());
		m.insert_cat("rnx-0094-2").expect("fixture: insert");
		m.insert_cat("rnx-0094-1").expect("fixture: insert");
		m
	}
}


pub(crate) fn vec_len(value: &rune::Value, name: &str) -> Result<usize, Error> {
	value.borrow_ref::<rune::runtime::Vec>().map(|v| v.len()).map_err(|_| Error::conversion(&format!("{name}: expected a vector")))
}
/// Record 0086: an Arrow validity bitmap built from a script vector of
/// bools. The length is checked against the bound before anything is
/// copied, then against the length the operation requires, before Polars
/// sees the bitmap; the script's vector is only read.
pub(crate) fn bitmap_from_bools(value: &rune::Value, method: &str, expect: Option<usize>) -> Result<polars_arrow::bitmap::Bitmap, Error> {
	let n = vec_len(value, method)?;
	let _guard = SliceBudget::enter();
	reserve(n, "mask bits", method)?;
	if let Some(want) = expect {
		if n != want {
			return Err(Error("ShapeMismatch".into(), format!("{method}: the mask has {n} bits, expected {want}")));
		}
	}
	let items = borrow_vec(value, method)?;
	let bits: Vec<bool> = items.iter().map(|v| borrow_element::<bool>(v, method)).collect::<Result<_, _>>()?;
	Ok(polars_arrow::bitmap::Bitmap::from_iter(bits))
}
/// Record 0085: a validity bitmap's logical bits, in order, under the same
/// cumulative bound (one call's bitmaps share it).
pub(crate) fn copy_bits(bitmap: &polars_arrow::bitmap::Bitmap, method: &str) -> Result<Vec<bool>, Error> {
	let _guard = SliceBudget::enter();
	reserve(bitmap.len(), "validity bits", method)?;
	Ok(bitmap.iter().collect())
}

pub(crate) fn materialize_unknown_with<I: Iterator, T>(mut it: I, limit: usize, method: &str, mut conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	let mut out: Vec<T> = Vec::new();
	loop {
		let Some(item) = it.next() else { return Ok(out) };
		if out.len() == limit {
			// the one item of lookahead: discarded without conversion
			drop(item);
			return Err(Error("MaterializeLimit".into(), format!("{method}: more than the bound of {limit} items")));
		}
		out.push(conv(item)?);
	}
}

impl Error {
	pub(crate) fn conversion(what: &str) -> Error {
		Error("ConversionError".into(), what.to_string())
	}
	/// The engine thread could not be started or joined.
	pub(crate) fn engine(failure: crate::engine::EngineFailure) -> Error {
		match failure {
			crate::engine::EngineFailure::NoThread(text) => Error("EngineError".into(), text),
			crate::engine::EngineFailure::Callback(text) | crate::engine::EngineFailure::Reentry(text) => Error("CallbackError".into(), text),
		}
	}
	#[rune::function(instance, path = kind)]
	fn kind(&self) -> String {
		self.0.clone()
	}
	#[rune::function(instance, path = message)]
	fn message(&self) -> String {
		self.1.clone()
	}
	#[rune::function(instance, protocol = DISPLAY_FMT)]
	fn display(&self, f: &mut Formatter) -> rune::runtime::VmResult<()> {
		use rune::alloc::fmt::TryWrite;
		let s = &self.1;
		rune::vm_write!(f, "{s}")
	}
}

/// Narrow a Rune integer to the Rust integer a Polars signature wants.
pub(crate) fn narrow<T: TryFrom<i64>>(v: i64, name: &str) -> Result<T, Error> {
	T::try_from(v).map_err(|_| Error::conversion(&format!("{name}: {v} is out of range for {}", std::any::type_name::<T>())))
}

/// A one-character string for a `char` parameter.
pub(crate) fn one_char(s: &str, name: &str) -> Result<char, Error> {
	let mut it = s.chars();
	match (it.next(), it.next()) {
		(Some(c), None) => Ok(c),
		_ => Err(Error::conversion(&format!("{name}: expected exactly one character, got {s:?}"))),
	}
}

/// Take a wrapped value out of a Rune container element.
pub(crate) fn take<W: Any + Clone>(v: &rune::Value, name: &str) -> Result<W, Error> {
	v.borrow_ref::<W>()
		.map(|r| r.clone())
		.map_err(|_| Error::conversion(&format!("{name}: expected {}", std::any::type_name::<W>().rsplit("::").next().unwrap_or("value"))))
}

/// Clone a script vector's elements while preserving its container.
pub(crate) fn borrow_vec(value: &rune::Value, name: &str) -> Result<Vec<rune::Value>, Error> {
	let values = value.borrow_ref::<rune::runtime::Vec>()
		.map_err(|_| Error::conversion(&format!("{name}: expected a vector")))?;
	Ok(values.iter().cloned().collect())
}

pub(crate) trait BorrowRune: Sized {
	fn borrow(value: &rune::Value, name: &str) -> Result<Self, Error>;
}
pub(crate) fn borrow_element<T: BorrowRune>(value: &rune::Value, name: &str) -> Result<T, Error> {
	T::borrow(value, name)
}
impl BorrowRune for rune::Value {
	fn borrow(value: &rune::Value, _: &str) -> Result<Self, Error> { Ok(value.clone()) }
}
macro_rules! borrow_copy {
	($($ty:ty),*) => {$(
		impl BorrowRune for $ty {
			fn borrow(value: &rune::Value, name: &str) -> Result<Self, Error> {
				rune::from_value(value.clone()).map_err(|e| Error::conversion(&format!("{name}: {e}")))
			}
		}
	)*};
}
borrow_copy!(i64, f64, bool);
impl BorrowRune for String {
	fn borrow(value: &rune::Value, name: &str) -> Result<Self, Error> {
		value.borrow_string_ref().map(|s| s.to_string()).map_err(|e| Error::conversion(&format!("{name}: {e}")))
	}
}
/// Record 0084: an optional element of a script vector (`None` or `Some(v)`).
impl<T: BorrowRune> BorrowRune for Option<T> {
	fn borrow(value: &rune::Value, name: &str) -> Result<Self, Error> {
		let inner: Option<rune::Value> = rune::from_value(value.clone()).map_err(|e| Error::conversion(&format!("{name}: {e}")))?;
		inner.map(|v| T::borrow(&v, name)).transpose()
	}
}
impl<A: BorrowRune, B: BorrowRune> BorrowRune for (A, B) {
	fn borrow(value: &rune::Value, name: &str) -> Result<Self, Error> {
		let pair = value.borrow_ref::<rune::runtime::OwnedTuple>().map_err(|e| Error::conversion(&format!("{name}: {e}")))?;
		if pair.len() != 2 { return Err(Error::conversion(&format!("{name}: expected a pair"))); }
		Ok((A::borrow(&pair[0], name)?, B::borrow(&pair[1], name)?))
	}
}

pub(crate) mod callback {
	use super::{Error, rune};
	use crate::engine::{self, CallbackFailure, CallbackGuard};
	use rune::runtime::{Function, GuardedArgs, SyncFunction, FromValue};
	use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

	pub(super) static BUDGET: AtomicUsize = AtomicUsize::new(0);
	const HALT_LIMITED: &str = "Halted for unexpected reason `limited`";

	#[rune::function(path = set_callback_budget)]
	pub(crate) fn set_callback_budget(n: i64) {
		BUDGET.store(n.max(0) as usize, Ordering::SeqCst);
	}
	pub(crate) fn install(op: &str, f: Function) -> Result<Arc<SyncFunction>, Error> {
		f.into_sync().map(Arc::new).map_err(|e| Error("CallbackCapture".into(), format!("callback {op}: a captured value is not a constant: {e}")))
	}
	pub(crate) fn bridge<A: GuardedArgs, R: FromValue>(op: &str, f: &SyncFunction, args: A) -> Result<R, CallbackFailure> {
		if engine::in_callback() {
			return Err(CallbackFailure { op: op.into(), cause: "nested callback: a callback invoked while another is running on this thread".into() });
		}
		let _guard = CallbackGuard::enter();
		let budget = BUDGET.load(Ordering::SeqCst);
		let (result, exhausted) = if budget > 0 {
			rune::runtime::budget::with(budget, || {
				let result = f.call::<rune::Value>(args);
				let spent = result.is_err() && { let mut g = rune::runtime::budget::acquire(); !g.take() };
				(result, spent)
			}).call()
		} else { (f.call::<rune::Value>(args), false) };
		match result {
			rune::runtime::VmResult::Ok(value) => {
				let actual = value.type_info().to_string();
				rune::from_value::<R>(value).map_err(|e| CallbackFailure { op: op.into(), cause: format!("wrong result type: expected {}, got {actual} ({e})", std::any::type_name::<R>()) })
			}
			rune::runtime::VmResult::Err(e) => {
				let text = e.to_string();
				let cause = if exhausted && text == HALT_LIMITED { format!("instruction budget {budget} exhausted") } else { format!("call failed: {text}") };
				Err(CallbackFailure { op: op.into(), cause })
			}
		}
	}
	pub(crate) fn unwind<T>(failure: CallbackFailure) -> T {
		std::panic::resume_unwind(Box::new(failure))
	}
	pub(crate) fn compute_error(failure: CallbackFailure) -> p::PolarsError {
		p::PolarsError::ComputeError(failure.text().into())
	}
	pub(crate) fn convert<T>(op: &str, f: impl FnOnce() -> Result<T, Error>) -> Result<T, CallbackFailure> {
		f().map_err(|e| CallbackFailure { op: op.into(), cause: format!("wrong result type: {}", e.1) })
	}
	use polars::prelude as p;
}

pub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {
	m.ty::<Error>()?;
	m.function_meta(callback::set_callback_budget)?;
	m.function_meta(Error::kind)?;
	m.function_meta(Error::message)?;
	m.function_meta(Error::display)?;
	#[cfg(feature = "test-support")]
	m.function_meta(set_materialize_limit)?;
	Ok(())
}

#[cfg(all(test, feature = "test-support"))]
mod materialize_tests {
	use super::*;
	use std::cell::Cell;
	use std::rc::Rc;

	/// An iterator that counts its `next` calls and reports the size hint
	/// it is told to, of a given true length.
	struct Counting {
		i: usize,
		len: usize,
		hint: (usize, Option<usize>),
		nexts: Rc<Cell<usize>>,
	}
	impl Iterator for Counting {
		type Item = usize;
		fn next(&mut self) -> Option<usize> {
			self.nexts.set(self.nexts.get() + 1);
			if self.i < self.len { self.i += 1; Some(self.i) } else { None }
		}
		fn size_hint(&self) -> (usize, Option<usize>) { self.hint }
	}
	struct Exact(Counting);
	impl Iterator for Exact {
		type Item = usize;
		fn next(&mut self) -> Option<usize> { self.0.next() }
		fn size_hint(&self) -> (usize, Option<usize>) { (self.0.len - self.0.i, Some(self.0.len - self.0.i)) }
	}
	impl ExactSizeIterator for Exact {}
	fn counting(len: usize, hint: (usize, Option<usize>)) -> (Counting, Rc<Cell<usize>>) {
		let nexts = Rc::new(Cell::new(0));
		(Counting { i: 0, len, hint, nexts: nexts.clone() }, nexts)
	}

	#[test]
	fn unknown_length_boundaries() {
		const L: usize = 4;
		for (len, ok) in [(0usize, true), (L - 1, true), (L, true), (L + 1, false)] {
			let (it, nexts) = counting(len, (0, None));
			let convs = Rc::new(Cell::new(0));
			let c2 = convs.clone();
			let r = materialize_unknown_with(it, L, "m", move |x| { c2.set(c2.get() + 1); Ok(x) });
			if ok {
				let v = r.expect("within the bound");
				assert_eq!(v.len(), len);
				assert_eq!(nexts.get(), len + 1, "one next past the last item");
				assert_eq!(convs.get(), len);
			} else {
				let e = r.expect_err("over the bound");
				assert_eq!(e.0, "MaterializeLimit");
				assert_eq!(nexts.get(), L + 1, "exactly L + 1 next calls: the lookahead item is discarded");
				assert_eq!(convs.get(), L, "the lookahead item is not converted");
			}
		}
		// an unbounded iterator refuses at L + 1 calls
		let nexts = Rc::new(Cell::new(0));
		let n2 = nexts.clone();
		let unbounded = std::iter::repeat_with(move || { n2.set(n2.get() + 1); 1usize });
		assert_eq!(materialize_unknown_with(unbounded, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), L + 1);
		// a size hint that overstates is not a length: the items are driven and counted
		let (it, nexts) = counting(2, (0, Some(usize::MAX)));
		assert_eq!(materialize_unknown_with(it, L, "m", Ok).unwrap().len(), 2);
		assert_eq!(nexts.get(), 3);
	}

	#[test]
	fn known_length_over_the_limit_refuses_before_any_next() {
		const L: usize = 4;
		let (c, nexts) = counting(L + 1, (0, None));
		let e = materialize_exact_with(Exact(c), L, "m", Ok).unwrap_err();
		assert_eq!(e.0, "MaterializeLimit");
		assert_eq!(nexts.get(), 0, "a known excess takes no item");
		let (c, nexts) = counting(L, (0, None));
		assert_eq!(materialize_exact_with(Exact(c), L, "m", Ok).unwrap().len(), L);
		assert_eq!(nexts.get(), L + 1);
	}

	// `TrustedLen` without `ExactSizeIterator`: the trait as Polars defines it
	unsafe impl polars_arrow::trusted_len::TrustedLen for Counting {}
	fn trusted_only(len: usize, hint: (usize, Option<usize>)) -> (impl polars_arrow::trusted_len::TrustedLen<Item = usize>, Rc<Cell<usize>>) {
		counting(len, hint)
	}

	#[test]
	fn a_trusted_len_only_return_materializes_by_its_upper_bound() {
		const L: usize = 4;
		// compiles against an opaque TrustedLen-only return with a mappable item
		let (it, nexts) = trusted_only(3, (3, Some(3)));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap().len(), 3);
		assert_eq!(nexts.get(), 4);
		// an upper bound over the limit refuses before any next call
		let (it, nexts) = trusted_only(L + 1, (L + 1, Some(L + 1)));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), 0);
		// an unrepresentable length refuses before any next call
		let (it, nexts) = trusted_only(2, (2, None));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), 0);
		// the count guard stays: a bound that understates is still caught at L + 1
		let (it, nexts) = trusted_only(L + 1, (1, Some(1)));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), L + 1);
		let _: Vec<usize> = materialize_trusted(trusted_only(2, (2, Some(2))).0, "m", Ok).unwrap();
	}

	#[test]
	fn a_conversion_failure_stops_and_returns_the_error() {
		let (it, nexts) = counting(3, (0, None));
		let e = materialize_unknown_with(it, 10, "m", |x| if x == 2 { Err(Error::conversion("two")) } else { Ok(x) }).unwrap_err();
		assert_eq!(e.0, "ConversionError");
		assert_eq!(nexts.get(), 2);
	}

	#[test]
	fn production_limit_is_the_default() {
		let _limit = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		assert_eq!(materialize_limit(), MATERIALIZE_LIMIT);
		let v: Vec<usize> = materialize_unknown(0..10usize, "m", Ok).unwrap();
		assert_eq!(v.len(), 10);
	}

	#[test]
	fn a_routed_iterator_is_driven_on_the_engine_thread_under_a_tokio_runtime() {
		let _limit = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
		let names = rt.block_on(async {
			crate::engine::run("support::borrowed_iterator", move || {
				let owner = [1usize, 2, 3];
				let it = owner.iter().map(|x| { let name = std::thread::current().name().map(|s| s.to_string()); (*x, name) });
				let out: Vec<(usize, Option<String>)> = materialize_unknown(it, "m", Ok).unwrap();
				// borrowed elements are detached into owned values before the owner drops
				out
			})
			.unwrap()
		});
		assert_eq!(names.len(), 3);
		assert!(names.iter().all(|(_, n)| n.as_deref() == Some("rnx-polars-engine")), "next must run on the engine thread: {names:?}");
	}
}

#[cfg(all(test, feature = "test-support"))]
mod callback_tests {
	//! Direct bridge controls (plan 0080, gate 1): nested refusal before any
	//! budget, restoration of the guard and of the outer allowance on the
	//! calling thread after success, VM failure, native panic and typed
	//! unwind under a nonzero inner budget, and exact-halt exhaustion.
	use super::callback;
	use crate::engine::{self, CallbackFailure, CallbackGuard};
	use rnx::rune;
	use rune::runtime::{Function, SyncFunction};
	use std::sync::Arc;

	static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

	#[rune::function(path = native_panic)]
	fn native_panic() { panic!("native marker") }
	#[rune::function(path = typed_unwind)]
	fn typed_unwind() { callback::unwind::<()>(CallbackFailure { op: "inner".into(), cause: "typed marker".into() }) }

	fn closure(body: &str) -> SyncFunction {
		let mut probe = rune::Module::with_crate("probe").unwrap();
		probe.function_meta(native_panic).unwrap();
		probe.function_meta(typed_unwind).unwrap();
		let mut context = rune::Context::with_default_modules().unwrap();
		context.install(probe).unwrap();
		let runtime = Arc::new(context.runtime().unwrap());
		let mut sources = rune::Sources::new();
		sources.insert(rune::Source::memory(format!("pub fn main() {{ {body} }}")).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources).with_context(&context).build().unwrap();
		let mut vm = rune::Vm::new(runtime, Arc::new(unit));
		let f: Function = rune::from_value(vm.call(["main"], ()).unwrap()).unwrap();
		f.into_sync().unwrap()
	}
	/// Runs `body` on this thread under an outer allowance of `outer`
	/// instructions and returns what it produced together with how many
	/// units of that allowance are left afterwards: the bridge must hand
	/// the outer allowance back exactly, whatever happened inside.
	fn under_outer<T>(outer: usize, body: impl FnOnce() -> T) -> (T, usize) {
		rune::runtime::budget::with(outer, || {
			let out = body();
			let mut guard = rune::runtime::budget::acquire();
			let mut left = 0;
			while left <= outer && guard.take() { left += 1; }
			(out, left)
		}).call()
	}
	fn set_budget(n: usize) { callback::BUDGET.store(n, std::sync::atomic::Ordering::SeqCst) }

	#[test]
	fn nested_callback_is_refused_before_any_budget() {
		let _serial = SERIAL.lock().unwrap();
		let f = closure("|x| x + 1");
		set_budget(1);
		let outer = CallbackGuard::enter();
		let ((result, still_inside), left) = under_outer(3, || {
			let r = callback::bridge::<_, i64>("nested", &f, (1i64,));
			(r, engine::in_callback())
		});
		drop(outer);
		set_budget(0);
		let err = result.unwrap_err();
		assert_eq!(err.op, "nested");
		assert!(err.cause.starts_with("nested callback"), "{}", err.cause);
		assert!(still_inside, "the refusal must not drop the running callback's guard");
		assert!(!engine::in_callback());
		assert_eq!(left, 3, "the outer allowance was touched by a refused nested call");
	}

	#[test]
	fn success_restores_guard_and_outer_allowance() {
		let _serial = SERIAL.lock().unwrap();
		let f = closure("|x| x + 1");
		set_budget(1000);
		let ((value, inside_after), left) = under_outer(3, || {
			let r = callback::bridge::<_, i64>("ok", &f, (1i64,));
			(r, engine::in_callback())
		});
		set_budget(0);
		assert_eq!(value.unwrap(), 2);
		assert!(!inside_after);
		assert_eq!(left, 3);
	}

	#[test]
	fn vm_failure_restores_guard_and_outer_allowance() {
		let _serial = SERIAL.lock().unwrap();
		let f = closure(r#"|x| panic("vm marker")"#);
		set_budget(1000);
		let ((result, inside_after), left) = under_outer(3, || {
			let r = callback::bridge::<_, i64>("vm", &f, (1i64,));
			(r, engine::in_callback())
		});
		set_budget(0);
		let err = result.unwrap_err();
		assert!(err.cause.starts_with("call failed:") && err.cause.contains("vm marker"), "{}", err.cause);
		assert!(!inside_after);
		assert_eq!(left, 3);
	}

	#[test]
	fn exhaustion_is_exact_halt_and_restores_outer_allowance() {
		let _serial = SERIAL.lock().unwrap();
		let f = closure("|x| { let n = 0; loop { n = n + 1; } }");
		set_budget(1);
		let ((result, inside_after), left) = under_outer(3, || {
			let r = callback::bridge::<_, i64>("spin", &f, (1i64,));
			(r, engine::in_callback())
		});
		set_budget(0);
		assert_eq!(result.unwrap_err().cause, "instruction budget 1 exhausted");
		assert!(!inside_after);
		assert_eq!(left, 3);
	}

	#[test]
	fn native_panic_under_inner_budget_restores_guard_and_outer_allowance() {
		let _serial = SERIAL.lock().unwrap();
		let f = closure("|x| probe::native_panic()");
		set_budget(1000);
		let ((payload, inside_after), left) = under_outer(3, || {
			let p = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback::bridge::<_, i64>("native", &f, (1i64,)))).unwrap_err();
			(p, engine::in_callback())
		});
		set_budget(0);
		assert!(payload.downcast_ref::<CallbackFailure>().is_none(), "a native panic must not be mistaken for a typed unwind");
		assert!(!inside_after, "the guard must be released while unwinding");
		assert_eq!(left, 3);
	}

	#[test]
	fn typed_unwind_under_inner_budget_crosses_the_bridge_and_restores() {
		let _serial = SERIAL.lock().unwrap();
		let f = closure("|x| probe::typed_unwind()");
		set_budget(1000);
		let ((payload, inside_after), left) = under_outer(3, || {
			let p = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback::bridge::<_, i64>("outer", &f, (1i64,)))).unwrap_err();
			(p, engine::in_callback())
		});
		set_budget(0);
		let failure = payload.downcast::<CallbackFailure>().expect("the typed payload must cross the bridge intact");
		assert_eq!((failure.op.as_str(), failure.cause.as_str()), ("inner", "typed marker"));
		assert!(!inside_after);
		assert_eq!(left, 3);
	}
}

#[cfg(all(test, feature = "test-support"))]
mod bits_tests {
	//! Record 0085: the bit copy shares the cumulative bound, refuses before
	//! allocating, and the guard is restored after success, error and unwind.
	use super::*;
	use polars_arrow::bitmap::Bitmap;
	use super::LIMIT_LOCK as SERIAL;
	fn limit(n: usize) { TEST_LIMIT.store(n, std::sync::atomic::Ordering::SeqCst); }
	fn depth() -> (usize, usize) { SLICE_BUDGET.with(|b| b.get()) }

	#[test]
	fn bits_copy_in_order_under_one_cumulative_bound() {
		let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let a = Bitmap::from([true, false, true]);
		limit(5);
		let outer = SliceBudget::enter();
		assert_eq!(copy_bits(&a, "m").unwrap(), vec![true, false, true]);
		let e = copy_bits(&a, "m").unwrap_err();
		assert_eq!(e.1, "m: 3 validity bits with 3 already copied, more than the bound of 5");
		drop(outer);
		assert_eq!(depth().0, 0);
		assert_eq!(copy_bits(&Bitmap::new(), "m").unwrap(), Vec::<bool>::new(), "an empty bitmap is an empty vector");
		limit(0);
	}

	#[test]
	fn masks_are_built_in_order_and_checked_before_polars() {
		let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		let v = |bits: &[bool]| rune::to_value(bits.iter().map(|b| rune::to_value(*b).unwrap()).collect::<Vec<_>>()).unwrap();
		let m = bitmap_from_bools(&v(&[true, false, true]), "m", Some(3)).unwrap();
		assert_eq!(m.iter().collect::<Vec<_>>(), vec![true, false, true]);
		assert_eq!(m.unset_bits(), 1);
		assert_eq!(bitmap_from_bools(&v(&[]), "m", Some(0)).unwrap().len(), 0, "an explicit empty mask");
		let short = bitmap_from_bools(&v(&[true, false]), "m", Some(3)).unwrap_err();
		assert_eq!((short.0.as_str(), short.1.as_str()), ("ShapeMismatch", "m: the mask has 2 bits, expected 3"));
		let long = bitmap_from_bools(&v(&[true; 4]), "m", Some(3)).unwrap_err();
		assert_eq!(long.1, "m: the mask has 4 bits, expected 3");
		limit(2);
		let over = bitmap_from_bools(&v(&[true; 3]), "m", None).unwrap_err();
		assert_eq!(over.1, "m: 3 mask bits with 0 already copied, more than the bound of 2");
		limit(3);
		assert!(bitmap_from_bools(&v(&[true; 3]), "m", None).is_ok(), "exactly at the bound");
		limit(0);
		let wrong = bitmap_from_bools(&rune::to_value(vec![rune::to_value(1i64).unwrap()]).unwrap(), "m", None).unwrap_err();
		assert_eq!(wrong.0, "ConversionError");
		assert_eq!(depth().0, 0);
	}

	#[test]
	fn the_snapshot_budget_counts_chunks_and_cells() {
		let _s = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		limit(7);
		assert_eq!(snapshot_budget(2, 5, "m").unwrap(), 7, "exactly the bound");
		let e = snapshot_budget(3, 5, "m").unwrap_err();
		assert_eq!((e.0.as_str(), e.1.as_str()), ("MaterializeLimit", "m: 3 chunks and 5 cells (8 slots), more than the bound of 7"));
		assert!(snapshot_budget(8, 0, "m").is_err(), "many empty chunks are counted");
		assert!(snapshot_budget(7, 0, "m").is_ok());
		let o = snapshot_budget(usize::MAX, 1, "m").unwrap_err();
		assert_eq!(o.1, format!("m: {} chunks and 1 cells overflow the slot count, more than the bound of 7", usize::MAX));
		limit(0);
		assert!(snapshot_budget(MATERIALIZE_LIMIT, 0, "m").is_ok() && snapshot_budget(MATERIALIZE_LIMIT, 1, "m").is_err());
	}

	#[test]
	fn chunk_snapshots_keep_boundaries_and_refuse_the_wrong_array() {
		let _s = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		limit(0);
		let a: polars_arrow::array::ArrayRef = Box::new(polars_arrow::array::PrimitiveArray::<i32>::from([Some(1), None]));
		let b: polars_arrow::array::ArrayRef = Box::new(polars_arrow::array::PrimitiveArray::<i32>::from_vec(vec![]));
		let out = chunk_snapshot::<i32, i64>(&[a.clone(), b, a.clone()], "m", |x| Ok(x as i64)).unwrap();
		assert_eq!(out, vec![vec![Some(1), None], vec![], vec![Some(1), None]]);
		let e = chunk_snapshot::<i64, i64>(&[a.clone()], "m", Ok).unwrap_err();
		assert_eq!(e.0, "ConversionError");
		assert!(e.1.starts_with("m: a chunk is not a PrimitiveArray<i64>"), "{}", e.1);
		limit(5);
		assert!(chunk_snapshot::<i32, i64>(&[a.clone(), a.clone()], "m", |x| Ok(x as i64)).is_err(), "2 chunks + 4 cells = 6 > 5");
		limit(6);
		assert!(chunk_snapshot::<i32, i64>(&[a.clone(), a], "m", |x| Ok(x as i64)).is_ok());
		limit(0);
	}

	#[test]
	fn payload_snapshots_count_chunks_cells_and_bytes() {
		let _s = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		limit(0);
		use polars_arrow::array::{ArrayRef, BinaryArray, BinaryViewArray, BooleanArray, Utf8ViewArray};
		let s: ArrayRef = Box::new(Utf8ViewArray::from_slice([Some("ab"), None, Some("é")]));
		let empty: ArrayRef = Box::new(Utf8ViewArray::from_slice::<&str, _>([]));
		let got = chunk_snapshot_str(&[s.clone(), empty.clone(), s.clone()], "m").unwrap();
		assert_eq!(got, vec![vec![Some("ab".to_string()), None, Some("é".to_string())], vec![], vec![Some("ab".to_string()), None, Some("é".to_string())]], "an explicit empty chunk is kept");
		// 3 chunks + 6 cells + 8 bytes (ab, é = 2 + 2, twice) = 17 slots
		limit(16);
		let e = chunk_snapshot_str(&[s.clone(), empty.clone(), s.clone()], "m").unwrap_err();
		assert_eq!((e.0.as_str(), e.1.as_str()), ("MaterializeLimit", "m: 3 chunks, 6 cells and 8 bytes (17 slots), more than the bound of 16"));
		limit(17);
		assert!(chunk_snapshot_str(&[s.clone(), empty, s.clone()], "m").is_ok());
		let b: ArrayRef = Box::new(BinaryViewArray::from_slice([Some(&[0u8, 255][..]), Some(&[][..]), None]));
		let o: ArrayRef = Box::new(BinaryArray::<i64>::from([Some(&[0xc3u8, 0x28][..]), None]));
		let f: ArrayRef = Box::new(BooleanArray::from([Some(true), None, Some(false)]));
		limit(0);
		assert_eq!(chunk_snapshot_binview(&[b.clone()], "m").unwrap(), vec![vec![Some(vec![0, 255]), Some(vec![]), None]]);
		assert_eq!(chunk_snapshot_binary_offset(&[o.clone()], "m").unwrap(), vec![vec![Some(vec![0xc3, 0x28]), None]], "non-UTF-8 bytes stay raw");
		assert_eq!(chunk_snapshot_bool(&[f.clone()], "m").unwrap(), vec![vec![Some(true), None, Some(false)]]);
		// the wrong array is a typed error, not a panic
		let e = chunk_snapshot_binview(&[o], "m").unwrap_err();
		assert_eq!((e.0.as_str(), e.1.as_str()), ("ConversionError", "m: a chunk is not a BinaryViewArray"));
		assert_eq!(chunk_snapshot_str(&[b], "m").unwrap_err().1, "m: a chunk is not a Utf8ViewArray");
		assert_eq!(chunk_snapshot_bool(&[s], "m").unwrap_err().1, "m: a chunk is not a BooleanArray");
		// checked addition of chunks + cells + bytes
		limit(7);
		assert_eq!(payload_snapshot_budget(1, 2, 4, "m").unwrap(), 7);
		assert_eq!(payload_snapshot_budget(usize::MAX, 1, 0, "m").unwrap_err().1, format!("m: {} chunks, 1 cells and 0 bytes overflow the slot count, more than the bound of 7", usize::MAX));
		assert!(payload_snapshot_budget(0, usize::MAX, 1, "m").is_err());
		// the preflight's own additions name every count so far and what overflowed
		assert_eq!(payload_count_step(2, usize::MAX, 5, 1, PayloadCount::Cells, "m").unwrap_err().1, format!("m: 2 chunks, {} cells and 5 bytes counted; adding 1 cells overflows, more than the bound of 7", usize::MAX));
		assert_eq!(payload_count_step(2, 3, usize::MAX - 1, 4, PayloadCount::Bytes, "m").unwrap_err().1, format!("m: 2 chunks, 3 cells and {} bytes counted; adding 4 bytes overflows, more than the bound of 7", usize::MAX - 1));
		assert_eq!(payload_count_step(2, 3, 4, 5, PayloadCount::Bytes, "m").unwrap(), 9);
		// many empty chunks under a small bound are refused by the count alone
		limit(3);
		let empties: Vec<ArrayRef> = (0..10_000).map(|_| -> ArrayRef { Box::new(Utf8ViewArray::from_slice::<&str, _>([])) }).collect();
		assert_eq!(chunk_snapshot_str(&empties, "m").unwrap_err().1, "m: 10000 chunks, 0 cells and 0 bytes (10000 slots), more than the bound of 3");
		// a wrong array is still a typed error, reported by the preflight
		assert_eq!(chunk_snapshot_bool(&empties[..1], "m").unwrap_err().0, "ConversionError");
		limit(0);
		let _ = f;
	}

	#[test]
	fn indexed_snapshots_copy_one_chunk_under_the_bound() {
		let _s = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		use polars_arrow::array::{BinaryArray, BinaryViewArray, BooleanArray, PrimitiveArray, Utf8ViewArray};
		limit(0);
		let n = PrimitiveArray::<i32>::from([Some(1), None, Some(3)]);
		assert_eq!(indexed_snapshot::<i32, i64>(Some(&n), "m", |x| Ok(x as i64)).unwrap(), Some(vec![Some(1), None, Some(3)]));
		assert_eq!(indexed_snapshot::<i32, i64>(None, "m", |x| Ok(x as i64)).unwrap(), None, "an absent chunk copies nothing");
		let empty = PrimitiveArray::<i32>::from_vec(vec![]);
		assert_eq!(indexed_snapshot::<i32, i64>(Some(&empty), "m", |x| Ok(x as i64)).unwrap(), Some(vec![]), "a present empty chunk is Some([])");
		// 1 chunk + 3 cells = 4 slots
		limit(3);
		assert_eq!(indexed_snapshot::<i32, i64>(Some(&n), "m", |x| Ok(x as i64)).unwrap_err().1, "m: 1 chunks and 3 cells (4 slots), more than the bound of 3");
		assert_eq!(indexed_snapshot::<i32, i64>(None, "m", |x| Ok(x as i64)).unwrap(), None, "None under a small bound");
		limit(4);
		assert!(indexed_snapshot::<i32, i64>(Some(&n), "m", |x| Ok(x as i64)).is_ok());
		// payload: "é日本", "", null = 1 + 3 + 8 = 12 slots
		let s = Utf8ViewArray::from_slice([Some("é日本"), Some(""), None]);
		limit(11);
		assert_eq!(indexed_snapshot_str(Some(&s), "m").unwrap_err().1, "m: 1 chunks, 3 cells and 8 bytes (12 slots), more than the bound of 11");
		limit(12);
		assert_eq!(indexed_snapshot_str(Some(&s), "m").unwrap(), Some(vec![Some("é日本".to_string()), Some(String::new()), None]));
		limit(0);
		let b = BinaryViewArray::from_slice([Some(&[0u8, 255][..]), None]);
		assert_eq!(indexed_snapshot_binview(Some(&b), "m").unwrap(), Some(vec![Some(vec![0, 255]), None]));
		let o = BinaryArray::<i64>::from([Some(&[0xc3u8, 0x28][..])]);
		assert_eq!(indexed_snapshot_binary_offset(Some(&o), "m").unwrap(), Some(vec![Some(vec![0xc3, 0x28])]));
		let f = BooleanArray::from([Some(true), None]);
		assert_eq!(indexed_snapshot_bool(Some(&f), "m").unwrap(), Some(vec![Some(true), None]));
		assert_eq!(indexed_snapshot_bool(None, "m").unwrap(), None);
	}

	#[test]
	fn the_signed_length_guard_is_exact() {
		assert!(signed_len(0, "m").is_ok());
		assert!(signed_len(i64::MAX as usize, "m").is_ok(), "exactly i64::MAX is within Polars's contract");
		let e = signed_len(i64::MAX as usize + 1, "m").unwrap_err();
		assert_eq!((e.0.as_str(), e.1.as_str()), ("ConversionError", "m: receiver length 9223372036854775808 is beyond i64::MAX, the range of Polars's slice offsets"));
		assert!(signed_len(usize::MAX, "m").is_err());
	}

	#[test]
	fn the_null_aware_bound_is_inclusive() {
		let _s = LIMIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		limit(3);
		assert!(null_aware_bound(3, "m").is_ok());
		let e = null_aware_bound(4, "m").unwrap_err();
		assert_eq!((e.0.as_str(), e.1.as_str()), ("MaterializeLimit", "m: 4 items, more than the bound of 3"));
		limit(0);
		assert!(null_aware_bound(MATERIALIZE_LIMIT, "m").is_ok() && null_aware_bound(MATERIALIZE_LIMIT + 1, "m").is_err(), "0 restores the production bound");
		assert!(null_aware_bound(0, "m").is_ok());
	}

	#[test]
	fn hash_tokens_are_exact_and_strict() {
		for v in [0u64, 1, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX, 0x0123_4567_89ab_cdef] {
			let t = hash_token(v);
			assert_eq!(t.len(), 16);
			assert_eq!(t, format!("{v:016x}"));
			assert_eq!(hash_from_token(&t, "m").unwrap(), v, "{t}");
		}
		assert_eq!(hash_token(0), "0000000000000000");
		assert_eq!(hash_token(u64::MAX), "ffffffffffffffff");
		assert_eq!(hash_token(i64::MAX as u64 + 1), "8000000000000000");
		for bad in ["", "0", "000000000000000", "00000000000000000", "FFFFFFFFFFFFFFFF", "0x00000000000000", "+000000000000000", "-000000000000001", " 000000000000000", "000000000000000g", "00000000000000é", "０000000000000000"] {
			let e = hash_from_token(bad, "m").unwrap_err();
			assert_eq!(e.0, "ConversionError", "{bad:?}");
			assert!(e.1.starts_with("m: hash must be 16 lowercase hex digits"), "{bad:?}: {}", e.1);
		}
		let long = hash_from_token(&"a".repeat(1000), "m").unwrap_err().1;
		assert_eq!(long, format!("m: hash must be 16 lowercase hex digits, got {:?}...", "a".repeat(24)), "a long token is shown truncated");
	}

	#[test]
	fn counts_widen_with_a_range_check() {
		assert_eq!(widen::<usize>(7, "m").unwrap(), 7);
		assert_eq!(widen::<usize>(i64::MAX as usize, "m").unwrap(), i64::MAX);
		let over = widen::<usize>(i64::MAX as usize + 1, "m").unwrap_err();
		assert_eq!((over.0.as_str(), over.1.as_str()), ("ConversionError", "m: 9223372036854775808 does not fit a script integer"));
		// record 0093: the synthetic extremes of every risky source type
		assert_eq!(widen::<usize>(usize::MAX, "m").unwrap_err().1, format!("m: {} does not fit a script integer", usize::MAX));
		assert_eq!(widen::<u64>(u64::MAX, "m").unwrap_err().0, "ConversionError");
		assert_eq!(widen::<u64>(i64::MAX as u64, "m").unwrap(), i64::MAX);
		assert_eq!(widen::<isize>(isize::MIN, "m").unwrap(), isize::MIN as i64);
		assert_eq!(widen::<i128>(i64::MIN as i128 - 1, "m").unwrap_err().0, "ConversionError");
		assert_eq!(widen::<u128>(u128::MAX, "m").unwrap_err().0, "ConversionError");
		assert_eq!(bounded_usize(i64::MAX as usize), i64::MAX);
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "bounded read-back")]
	fn a_bounded_readback_out_of_range_trips_the_debug_assertion() {
		bounded_usize(usize::MAX);
	}

	#[test]
	fn the_guard_is_restored_after_an_unwind() {
		let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
		limit(3);
		let r = std::panic::catch_unwind(|| {
			let _g = SliceBudget::enter();
			copy_bits(&Bitmap::from([true, true]), "m").unwrap();
			panic!("inside the guarded call");
		});
		assert!(r.is_err());
		assert_eq!(depth().0, 0, "the guard depth is restored while unwinding");
		assert_eq!(copy_bits(&Bitmap::from([true, true, true]), "m").unwrap().len(), 3, "a new call gets the whole bound");
		limit(0);
	}
}
