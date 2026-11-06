//! CK1 precompiled primitives: std/core only, independently compilable.
//! Safe wrappers prove equal lengths and exclusive output access. The only
//! target-feature entry is dominated by runtime detection (disabled in Miri).

// Bit-preserving views used by the typed operand dispatcher. These are the
// only representation casts: all three scalar types have identical size and alignment,
// every bit pattern is valid, and returned borrows retain the input lifetime.
// Mutable views require an exclusive initialized slice; no length is changed.
const _: () = {
	assert!(core::mem::size_of::<i64>() == core::mem::size_of::<u64>());
	assert!(core::mem::align_of::<i64>() == core::mem::align_of::<u64>());
	assert!(core::mem::size_of::<f64>() == core::mem::size_of::<u64>());
	assert!(core::mem::align_of::<f64>() == core::mem::align_of::<u64>());
};
pub(crate) fn i64_words(values: &[i64]) -> &[u64] {
	// SAFETY: identical layout and validity, shared borrow with unchanged extent.
	unsafe { core::slice::from_raw_parts(values.as_ptr().cast(), values.len()) }
}
pub(crate) fn f64_words(values: &[f64]) -> &[u64] {
	// SAFETY: identical layout and validity, shared borrow with unchanged extent.
	unsafe { core::slice::from_raw_parts(values.as_ptr().cast(), values.len()) }
}
pub(crate) fn i64_words_mut(values: &mut [i64]) -> &mut [u64] {
	// SAFETY: identical layout/validity; exclusive borrow with unchanged extent.
	unsafe { core::slice::from_raw_parts_mut(values.as_mut_ptr().cast(), values.len()) }
}
pub(crate) fn f64_words_mut(values: &mut [f64]) -> &mut [u64] {
	// SAFETY: identical layout/validity; exclusive borrow with unchanged extent.
	unsafe { core::slice::from_raw_parts_mut(values.as_mut_ptr().cast(), values.len()) }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FloatOp {
	Add,
	Sub,
	Mul,
}

/// Constant-add and column arithmetic have distinct shapes.
#[derive(Clone, Copy)]
pub(crate) enum FloatRhs<'a> {
	AddConstant(f64),
	Column(FloatOp, &'a [f64]),
}

// Typed general multiplication, also used by the verified scratch chain. The
// u64 storage carries i64 bits; conversion is numeric and needs no alias cast.
pub(crate) fn checked_mul(x: &[u64], y: &[u64], out: &mut [u64]) -> bool {
	assert_eq!(x.len(), y.len());
	assert_eq!(x.len(), out.len());
	let mut failed = false;
	for ((&x, &y), out) in x.iter().zip(y).zip(out) {
		let (v, f) = (x as i64).overflowing_mul(y as i64);
		*out = v as u64;
		failed |= f;
	}
	failed
}

#[inline(always)]
fn float_pair(op: FloatOp, x: &[f64], y: &[f64], out: &mut [f64]) {
	macro_rules! apply {
		($op:tt) => {
			for ((&x, &y), out) in x.iter().zip(y).zip(out) {
				*out = x $op y;
			}
		};
	}
	match op {
		FloatOp::Add => apply!(+),
		FloatOp::Sub => apply!(-),
		FloatOp::Mul => apply!(*),
	}
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Engine {
	avx2: bool,
}
impl Engine {
	pub(crate) fn new(force_scalar: bool) -> Self {
		#[cfg(all(target_arch = "x86_64", not(miri)))]
		let avx2 = !force_scalar && std::is_x86_feature_detected!("avx2");
		#[cfg(not(all(target_arch = "x86_64", not(miri))))]
		let avx2 = {
			let _ = force_scalar;
			false
		};
		Self { avx2 }
	}
	pub(crate) fn avx2(self) -> bool {
		self.avx2
	}
	pub(crate) fn add(self, x: &[i64], c: i64, out: &mut [i64]) -> bool {
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: detection, accessible equal-length slices, no output alias.
			return unsafe { simd::add_constant(x.as_ptr(), out.as_mut_ptr(), x.len(), c) };
		}
		let mut failed = false;
		for (&x, out) in x.iter().zip(out) {
			let (v, f) = x.overflowing_add(c);
			*out = v;
			failed |= f;
		}
		failed
	}
	pub(crate) fn add_pair(self, x: &[u64], y: &[u64], out: &mut [u64]) -> bool {
		assert_eq!(x.len(), y.len());
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: detected AVX2, equal accessible slices, exclusive output.
			return unsafe { simd::add_pair(x.as_ptr(), y.as_ptr(), out.as_mut_ptr(), x.len()) };
		}
		let mut failed = false;
		for ((&a, &b), out) in x.iter().zip(y).zip(out) {
			let (v, f) = (a as i64).overflowing_add(b as i64);
			*out = v as u64;
			failed |= f;
		}
		failed
	}

	pub(crate) fn mul_constant(self, x: &[u64], c: i64, out: &mut [u64]) -> bool {
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 && c != 0 && c.unsigned_abs() < (1u64 << 32) {
			let (lo, hi) = if c > 0 {
				(
					i128::from(i64::MIN) / i128::from(c),
					i128::from(i64::MAX) / i128::from(c),
				)
			} else {
				(
					i128::from(i64::MAX) / i128::from(c),
					i128::from(i64::MIN) / i128::from(c),
				)
			};
			let lo = lo.max(i128::from(i64::MIN)) as i64;
			let hi = hi.min(i128::from(i64::MAX)) as i64;
			// SAFETY: detected AVX2, equal accessible slices, exclusive output;
			// signed bounds computed without MIN/-1 overflow, magnitude <2^32.
			return unsafe { simd::mul_constant(x.as_ptr(), out.as_mut_ptr(), x.len(), c, lo, hi) };
		}
		let mut failed = false;
		for (&x, out) in x.iter().zip(out) {
			let (v, f) = (x as i64).overflowing_mul(c);
			*out = v as u64;
			failed |= f;
		}
		failed
	}

	pub(crate) fn fused(self, x: &[i64], y: &[i64], out: &mut [i64]) -> bool {
		assert_eq!(x.len(), y.len());
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: detection and immutable inputs disjoint from mutable output.
			return unsafe { simd::fused(x.as_ptr(), y.as_ptr(), out.as_mut_ptr(), x.len()) };
		}
		let mut failed = false;
		for ((&x, &y), out) in x.iter().zip(y).zip(out) {
			let (p, a) = x.overflowing_mul(2);
			let (v, b) = p.overflowing_add(y);
			*out = v;
			failed |= a | b;
		}
		failed
	}
	pub(crate) fn floats(self, x: &[f64], c: f64, out: &mut [f64]) {
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: runtime detection. Safe Rust body has no raw accesses.
			unsafe { simd::floats(x, c, out) };
			return;
		}
		for (&x, out) in x.iter().zip(out) {
			*out = x + c;
		}
	}
	pub(crate) fn float_pair(self, op: FloatOp, x: &[f64], y: &[f64], out: &mut [f64]) {
		assert_eq!(x.len(), y.len());
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: runtime detection; the body uses only safe slice loops.
			unsafe { simd::float_pair(op, x, y, out) };
			return;
		}
		float_pair(op, x, y, out);
	}
	/// Fused arithmetic and deterministic null zeroing for a physical block.
	pub(crate) fn masked_floats(
		self,
		x: &[f64],
		rhs: FloatRhs<'_>,
		out: &mut [f64],
		masks: &[u64],
	) {
		assert_eq!(x.len(), out.len());
		assert_eq!(masks.len(), x.len().div_ceil(64));
		if let FloatRhs::Column(_, y) = rhs {
			assert_eq!(x.len(), y.len());
		}
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: runtime AVX2 check; equal accessible slices (or a scalar
			// constant), exclusive output, and one mask per 64 initialized lanes.
			unsafe {
				match rhs {
					FloatRhs::AddConstant(c) => simd::masked_floats::<0>(x, &[], c, out, masks),
					FloatRhs::Column(FloatOp::Add, y) => {
						simd::masked_floats::<1>(x, y, 0.0, out, masks)
					}
					FloatRhs::Column(FloatOp::Sub, y) => {
						simd::masked_floats::<2>(x, y, 0.0, out, masks)
					}
					FloatRhs::Column(FloatOp::Mul, y) => {
						simd::masked_floats::<3>(x, y, 0.0, out, masks)
					}
				}
			}
			return;
		}
		for (i, (&a, value)) in x.iter().zip(out).enumerate() {
			let result = match rhs {
				FloatRhs::AddConstant(c) => a + c,
				FloatRhs::Column(FloatOp::Add, y) => a + y[i],
				FloatRhs::Column(FloatOp::Sub, y) => a - y[i],
				FloatRhs::Column(FloatOp::Mul, y) => a * y[i],
			};
			*value =
				f64::from_bits(result.to_bits() & 0u64.wrapping_sub(masks[i / 64] >> (i % 64) & 1));
		}
	}
	/// Clear null payloads after optimistic arithmetic, without changing valid bits.
	pub(crate) fn zero_i64(self, out: &mut [i64], mask: u64) {
		assert!(out.len() <= 64);
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: runtime detection, exclusive initialized 64-bit elements,
			// identical size/alignment and all i64 bit patterns are valid.
			unsafe { simd::zero_null(out.as_mut_ptr().cast(), out.len(), mask) };
			return;
		}
		for (i, value) in out.iter_mut().enumerate() {
			*value &= 0i64.wrapping_sub((mask >> i & 1) as i64);
		}
	}
	pub(crate) fn masked_add(self, x: &[i64], c: i64, out: &mut [i64], mask: u64) -> bool {
		assert_eq!(x.len(), out.len());
		assert!(x.len() <= 64);
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: detection, equal accessible slices, <=64 mask lanes, no alias.
			return unsafe {
				simd::masked::<false>(x.as_ptr(), x.as_ptr(), out.as_mut_ptr(), x.len(), c, mask)
			};
		}
		let mut failed = false;
		for (i, (&x, out)) in x.iter().zip(out).enumerate() {
			let valid = mask >> i & 1 != 0;
			let (v, f) = x.overflowing_add(c);
			*out = if valid { v } else { 0 };
			failed |= f && valid;
		}
		failed
	}
	pub(crate) fn masked_fused(self, x: &[i64], y: &[i64], out: &mut [i64], mask: u64) -> bool {
		assert_eq!(x.len(), y.len());
		assert_eq!(x.len(), out.len());
		assert!(x.len() <= 64);
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: detection, equal accessible slices, <=64 mask lanes, no alias.
			return unsafe {
				simd::masked::<true>(x.as_ptr(), y.as_ptr(), out.as_mut_ptr(), x.len(), 0, mask)
			};
		}
		let mut failed = false;
		for (i, ((&x, &y), out)) in x.iter().zip(y).zip(out).enumerate() {
			let valid = mask >> i & 1 != 0;
			let (p, m) = x.overflowing_mul(2);
			let (v, a) = p.overflowing_add(y);
			*out = if valid { v } else { 0 };
			failed |= (m | a) && valid;
		}
		failed
	}

	pub(crate) fn bits(self, x: &[u8], out: &mut [u8]) {
		assert_eq!(x.len(), out.len());
		#[cfg(target_arch = "x86_64")]
		if self.avx2 {
			// SAFETY: runtime detection. Safe Rust loop handles tails.
			unsafe { simd::bits(x, out) };
			return;
		}
		for (&x, out) in x.iter().zip(out) {
			*out = !x;
		}
	}
}

#[cfg(target_arch = "x86_64")]
mod simd {
	use core::arch::x86_64::*;
	// Raw-pointer helpers are private to the safe wrapper. Each full vector is
	// inside the provided range, accesses are unaligned, and tails are scalar.
	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) unsafe fn mul_constant(
		x: *const u64,
		out: *mut u64,
		n: usize,
		c: i64,
		lo: i64,
		hi: i64,
	) -> bool {
		let cv = _mm256_set1_epi64x(c.unsigned_abs() as i64);
		let low = _mm256_set1_epi64x(lo);
		let high = _mm256_set1_epi64x(hi);
		let zero = _mm256_setzero_si256();
		let mut acc = zero;
		let mut i = 0;
		while i + 4 <= n {
			let a = _mm256_loadu_si256(x.add(i).cast());
			acc = _mm256_or_si256(
				acc,
				_mm256_or_si256(_mm256_cmpgt_epi64(low, a), _mm256_cmpgt_epi64(a, high)),
			);
			let bottom = _mm256_mul_epu32(a, cv);
			let top = _mm256_slli_epi64::<32>(_mm256_mul_epu32(_mm256_srli_epi64::<32>(a), cv));
			let product = _mm256_add_epi64(bottom, top);
			let product = if c < 0 {
				_mm256_sub_epi64(zero, product)
			} else {
				product
			};
			_mm256_storeu_si256(out.add(i).cast(), product);
			i += 4;
		}
		let mut failed = _mm256_movemask_pd(_mm256_castsi256_pd(acc)) != 0;
		while i < n {
			let (v, f) = (*x.add(i) as i64).overflowing_mul(c);
			*out.add(i) = v as u64;
			failed |= f;
			i += 1;
		}
		failed
	}

	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) unsafe fn add_pair(x: *const u64, y: *const u64, out: *mut u64, n: usize) -> bool {
		let mut acc = _mm256_setzero_si256();
		let mut i = 0;
		while i + 4 <= n {
			let a = _mm256_loadu_si256(x.add(i).cast());
			let b = _mm256_loadu_si256(y.add(i).cast());
			let sum = _mm256_add_epi64(a, b);
			let overflow = _mm256_and_si256(_mm256_xor_si256(a, sum), _mm256_xor_si256(b, sum));
			acc = _mm256_or_si256(acc, overflow);
			_mm256_storeu_si256(out.add(i).cast(), sum);
			i += 4;
		}
		let mut failed = _mm256_movemask_pd(_mm256_castsi256_pd(acc)) != 0;
		while i < n {
			let (v, f) = (*x.add(i) as i64).overflowing_add(*y.add(i) as i64);
			*out.add(i) = v as u64;
			failed |= f;
			i += 1;
		}
		failed
	}

	#[target_feature(enable = "avx2")]
	pub(super) unsafe fn masked_floats<const KIND: u8>(
		x: &[f64],
		y: &[f64],
		c: f64,
		out: &mut [f64],
		masks: &[u64],
	) {
		let mut i = 0;
		let constant = _mm256_set1_pd(c);
		while i + 16 <= out.len() {
			let bits = _mm256_set1_epi64x((masks[i / 64] >> (i % 64)) as i64);
			macro_rules! vector {
				($offset:expr, $shift:expr) => {{
					let lanes =
						_mm256_set_epi64x(8 << $shift, 4 << $shift, 2 << $shift, 1 << $shift);
					let valid = _mm256_cmpeq_epi64(_mm256_and_si256(bits, lanes), lanes);
					let a = _mm256_loadu_pd(x.as_ptr().add(i + $offset));
					let b = if KIND == 0 {
						constant
					} else {
						_mm256_loadu_pd(y.as_ptr().add(i + $offset))
					};
					let value = match KIND {
						0 | 1 => _mm256_add_pd(a, b),
						2 => _mm256_sub_pd(a, b),
						_ => _mm256_mul_pd(a, b),
					};
					_mm256_storeu_pd(
						out.as_mut_ptr().add(i + $offset),
						_mm256_and_pd(value, _mm256_castsi256_pd(valid)),
					);
				}};
			}
			vector!(0, 0);
			vector!(4, 4);
			vector!(8, 8);
			vector!(12, 12);
			i += 16;
		}
		while i < out.len() {
			let a = x[i];
			let b = if KIND == 0 { c } else { y[i] };
			let value = match KIND {
				0 | 1 => a + b,
				2 => a - b,
				_ => a * b,
			};
			out[i] =
				f64::from_bits(value.to_bits() & 0u64.wrapping_sub(masks[i / 64] >> (i % 64) & 1));
			i += 1;
		}
	}

	#[target_feature(enable = "avx2")]
	pub(super) unsafe fn zero_null(out: *mut u64, n: usize, mask: u64) {
		let zero = _mm256_setzero_si256();
		let one = _mm256_set1_epi64x(1);
		let bits = _mm256_set1_epi64x(mask as i64);
		let mut shifts = _mm256_set_epi64x(3, 2, 1, 0);
		let four = _mm256_set1_epi64x(4);
		let mut i = 0;
		while i + 4 <= n {
			let valid =
				_mm256_sub_epi64(zero, _mm256_and_si256(_mm256_srlv_epi64(bits, shifts), one));
			let value = _mm256_loadu_si256(out.add(i).cast());
			_mm256_storeu_si256(out.add(i).cast(), _mm256_and_si256(value, valid));
			shifts = _mm256_add_epi64(shifts, four);
			i += 4;
		}
		while i < n {
			*out.add(i) &= 0u64.wrapping_sub(mask >> i & 1);
			i += 1;
		}
	}

	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) unsafe fn masked<const FUSED: bool>(
		x: *const i64,
		y: *const i64,
		out: *mut i64,
		n: usize,
		c: i64,
		mask: u64,
	) -> bool {
		let zero = _mm256_setzero_si256();
		let one = _mm256_set1_epi64x(1);
		let bits = _mm256_set1_epi64x(mask as i64);
		let mut shifts = _mm256_set_epi64x(3, 2, 1, 0);
		let four = _mm256_set1_epi64x(4);
		let cv = _mm256_set1_epi64x(c);
		let mut acc = zero;
		let mut i = 0;
		while i + 4 <= n {
			// Expand four packed validity bits entirely in vector registers.
			let valid =
				_mm256_sub_epi64(zero, _mm256_and_si256(_mm256_srlv_epi64(bits, shifts), one));
			let a = _mm256_loadu_si256(x.add(i).cast());
			let (value, overflow) = if FUSED {
				let b = _mm256_loadu_si256(y.add(i).cast());
				let product = _mm256_add_epi64(a, a);
				let sum = _mm256_add_epi64(product, b);
				let mul = _mm256_xor_si256(a, product);
				let add =
					_mm256_and_si256(_mm256_xor_si256(product, sum), _mm256_xor_si256(b, sum));
				(sum, _mm256_or_si256(mul, add))
			} else {
				let sum = _mm256_add_epi64(a, cv);
				(
					sum,
					_mm256_and_si256(_mm256_xor_si256(a, sum), _mm256_xor_si256(cv, sum)),
				)
			};
			acc = _mm256_or_si256(acc, _mm256_and_si256(overflow, valid));
			_mm256_storeu_si256(out.add(i).cast(), _mm256_and_si256(value, valid));
			shifts = _mm256_add_epi64(shifts, four);
			i += 4;
		}
		let mut failed = _mm256_movemask_pd(_mm256_castsi256_pd(acc)) != 0;
		while i < n {
			let valid = mask >> i & 1 != 0;
			let (v, f) = if FUSED {
				let (p, m) = (*x.add(i)).overflowing_mul(2);
				let (v, a) = p.overflowing_add(*y.add(i));
				(v, m | a)
			} else {
				(*x.add(i)).overflowing_add(c)
			};
			*out.add(i) = if valid { v } else { 0 };
			failed |= f && valid;
			i += 1;
		}
		failed
	}

	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) fn float_pair(op: super::FloatOp, x: &[f64], y: &[f64], out: &mut [f64]) {
		// Inlining the safe operation-specific loop here permits AVX2 codegen.
		super::float_pair(op, x, y, out);
	}

	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) unsafe fn add_constant(x: *const i64, out: *mut i64, n: usize, c: i64) -> bool {
		let cv = _mm256_set1_epi64x(c);
		let mut acc = _mm256_setzero_si256();
		let mut i = 0;
		if c >= 0 {
			let limit = _mm256_set1_epi64x(i64::MAX - c);
			while i + 4 <= n {
				let a = _mm256_loadu_si256(x.add(i).cast());
				acc = _mm256_or_si256(acc, _mm256_cmpgt_epi64(a, limit));
				_mm256_storeu_si256(out.add(i).cast(), _mm256_add_epi64(a, cv));
				i += 4;
			}
		} else {
			let limit = _mm256_set1_epi64x(i64::MIN - c);
			while i + 4 <= n {
				let a = _mm256_loadu_si256(x.add(i).cast());
				acc = _mm256_or_si256(acc, _mm256_cmpgt_epi64(limit, a));
				_mm256_storeu_si256(out.add(i).cast(), _mm256_add_epi64(a, cv));
				i += 4;
			}
		}
		let mut fail = _mm256_movemask_pd(_mm256_castsi256_pd(acc)) != 0;
		while i < n {
			let (v, f) = (*x.add(i)).overflowing_add(c);
			*out.add(i) = v;
			fail |= f;
			i += 1;
		}
		fail
	}
	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) unsafe fn fused(x: *const i64, y: *const i64, out: *mut i64, n: usize) -> bool {
		let mut acc = _mm256_setzero_si256();
		let mut i = 0;
		while i + 4 <= n {
			let a = _mm256_loadu_si256(x.add(i).cast());
			let b = _mm256_loadu_si256(y.add(i).cast());
			let product = _mm256_add_epi64(a, a);
			let sum = _mm256_add_epi64(product, b);
			let mul = _mm256_xor_si256(a, product);
			let add = _mm256_and_si256(_mm256_xor_si256(product, sum), _mm256_xor_si256(b, sum));
			acc = _mm256_or_si256(acc, _mm256_or_si256(mul, add));
			_mm256_storeu_si256(out.add(i).cast(), sum);
			i += 4;
		}
		let mut fail = _mm256_movemask_pd(_mm256_castsi256_pd(acc)) != 0;
		while i < n {
			let (p, m) = (*x.add(i)).overflowing_mul(2);
			let (v, a) = p.overflowing_add(*y.add(i));
			*out.add(i) = v;
			fail |= m | a;
			i += 1;
		}
		fail
	}

	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) fn floats(x: &[f64], c: f64, out: &mut [f64]) {
		for (&x, out) in x.iter().zip(out) {
			*out = x + c;
		}
	}
	#[target_feature(enable = "avx2")]
	#[inline(never)]
	pub(super) fn bits(x: &[u8], out: &mut [u8]) {
		for (&x, out) in x.iter().zip(out) {
			*out = !x;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::Engine;
	use std::vec::Vec;
	#[test]
	fn extrema_fault_positions_and_output_canaries() {
		for force in [true, false] {
			let engine = Engine::new(force);
			if force {
				assert!(!engine.avx2());
			}
			for size in [0, 1, 3, 4, 7, 31, 32, 33, 1024, 1025] {
				for c in [i64::MIN, -37, -1, 0, 1, 37, i64::MAX] {
					let input: Vec<_> = (0..size)
						.map(|i| [i64::MIN, -1, 0, 1, i64::MAX][i % 5])
						.collect();
					let mut out = std::vec![719; size + 2];
					let failed = engine.add(&input, c, &mut out[1..size + 1]);
					assert_eq!(failed, input.iter().any(|x| x.checked_add(c).is_none()));
					for (x, actual) in input.iter().zip(&out[1..size + 1]) {
						assert_eq!(*actual, x.wrapping_add(c));
					}
					assert_eq!((out[0], out[size + 1]), (719, 719));
				}
			}
			for fault in 0..if cfg!(miri) { 9 } else { 1027 } {
				let mut x = std::vec![1; 1027];
				let y = std::vec![1; 1027];
				let mut out = std::vec![0; 1027];
				x[fault] = i64::MAX;
				assert!(engine.fused(&x, &y, &mut out));
			}
		}
	}
	#[test]
	fn safe_float_and_bit_loops_match_portable() {
		for force in [true, false] {
			let engine = Engine::new(force);
			let x = [
				0.0,
				-0.0,
				f64::INFINITY,
				f64::NEG_INFINITY,
				f64::NAN,
				3.5,
				-2.0,
			];
			let mut out = [0.0; 7];
			engine.floats(&x, 1.0, &mut out);
			for (x, out) in x.into_iter().zip(out) {
				if x.is_nan() {
					assert!(out.is_nan());
				} else {
					assert_eq!(out.to_bits(), (x + 1.0).to_bits());
				}
			}
			for len in 0..67 {
				let x = std::vec![0xa5; len];
				let mut out = std::vec![0; len];
				engine.bits(&x, &mut out);
				assert!(out.iter().all(|&x| x == 0x5a));
			}
		}
	}
}

#[cfg(test)]
mod masked_tests {
	use super::*;

	#[test]
	fn masked_extrema_every_lane_tail_and_canaries() {
		for force in [true, false] {
			let engine = Engine::new(force);
			let lengths: std::vec::Vec<usize> = if cfg!(miri) {
				std::vec![1, 3, 4, 7, 31, 32, 63, 64]
			} else {
				(1..=64).collect()
			};
			for n in lengths {
				for fault in 0..n {
					let mut x = std::vec![1; n];
					let y = std::vec![0; n];
					x[fault] = i64::MAX;
					for mask in [u64::MAX, !(1u64 << fault), 0, 0xaaaaaaaaaaaaaaaa] {
						let mut out = std::vec![719; n + 2];
						for fused in [false, true] {
							let failed = if fused {
								engine.masked_fused(&x, &y, &mut out[1..n + 1], mask)
							} else {
								engine.masked_add(&x, 1, &mut out[1..n + 1], mask)
							};
							assert_eq!(failed, mask >> fault & 1 != 0);
							for (i, &x) in x.iter().enumerate() {
								let expected = if mask >> i & 1 == 0 {
									0
								} else if fused {
									x.wrapping_mul(2)
								} else {
									x.wrapping_add(1)
								};
								assert_eq!(out[1 + i], expected);
							}
							assert_eq!((out[0], out[n + 1]), (719, 719));
						}
					}
				}
			}
			for c in [i64::MIN, -37, -1, 0, 1, 37, i64::MAX] {
				let x = [i64::MIN, i64::MAX, 0, -1, 1, 3, i64::MIN];
				for mask in [0, 0x55, 0xaa, u64::MAX] {
					let mut out = [0; 7];
					let failed = engine.masked_add(&x, c, &mut out, mask);
					assert_eq!(
						failed,
						x.iter()
							.enumerate()
							.any(|(i, x)| mask >> i & 1 != 0 && x.checked_add(c).is_none())
					);
				}
			}
		}
	}

	#[test]
	fn typed_general_multiply_and_float_pairs() {
		let x = [i64::MIN, i64::MAX, 0, -1, 1, 1 << 40, -(1 << 40)];
		let y = [-1, 3, 7, i64::MIN, i64::MAX, 3, 3];
		let a = x.map(|v| v as u64);
		let b = y.map(|v| v as u64);
		let mut output = [0; 7];
		assert!(checked_mul(&a, &b, &mut output));
		for i in 0..x.len() {
			assert_eq!(output[i] as i64, x[i].wrapping_mul(y[i]));
		}
		for force in [true, false] {
			let engine = Engine::new(force);
			let x = [
				0.0,
				-0.0,
				f64::INFINITY,
				f64::NEG_INFINITY,
				f64::NAN,
				3.5,
				-2.0,
			];
			let y = [-0.0, 0.0, f64::NEG_INFINITY, -1.0, 1.0, 1.25, -3.5];
			for op in [FloatOp::Add, FloatOp::Sub, FloatOp::Mul] {
				let mut out = [719.0; 9];
				engine.float_pair(op, &x, &y, &mut out[1..8]);
				for i in 0..7 {
					let expected = match op {
						FloatOp::Add => x[i] + y[i],
						FloatOp::Sub => x[i] - y[i],
						FloatOp::Mul => x[i] * y[i],
					};
					if expected.is_nan() {
						assert!(out[i + 1].is_nan());
					} else {
						assert_eq!(out[i + 1].to_bits(), expected.to_bits());
					}
				}
				assert_eq!((out[0], out[8]), (719.0, 719.0));
			}
		}
	}
}

#[cfg(test)]
mod pair_tests {
	use super::Engine;
	#[test]
	fn checked_add_pairs_extrema_fault_lanes_and_tails() {
		for force in [true, false] {
			let engine = Engine::new(force);
			for n in [0, 1, 3, 4, 5, 7, 63, 64, 65, 1024, 1025] {
				let extrema = [i64::MIN, -1, 0, 1, i64::MAX];
				for shift in 0..5 {
					let x: std::vec::Vec<_> = (0..n).map(|i| extrema[i % 5] as u64).collect();
					let y: std::vec::Vec<_> =
						(0..n).map(|i| extrema[(i + shift) % 5] as u64).collect();
					let mut out = std::vec![719; n + 2];
					let failed = engine.add_pair(&x, &y, &mut out[1..n + 1]);
					let mut direct = std::vec![719i64; n + 2];
					assert_eq!(
						engine.add_pair(&x, &y, super::i64_words_mut(&mut direct[1..n + 1])),
						failed
					);
					assert_eq!((direct[0], direct[n + 1]), (719, 719));
					for (a, b) in direct[1..n + 1].iter().zip(&out[1..n + 1]) {
						assert_eq!(*a as u64, *b);
					}
					assert_eq!(
						failed,
						x.iter()
							.zip(&y)
							.any(|(&a, &b)| (a as i64).checked_add(b as i64).is_none())
					);
					for (i, (&a, &b)) in x.iter().zip(&y).enumerate() {
						assert_eq!(out[i + 1], a.wrapping_add(b));
					}
					assert_eq!((out[0], out[n + 1]), (719, 719));
				}
			}
			for fault in 0..67 {
				for (a, b) in [(i64::MAX, 1), (i64::MIN, -1)] {
					let mut x = [0; 67];
					let mut y = [0; 67];
					x[fault] = a as u64;
					y[fault] = b as u64;
					assert!(engine.add_pair(&x, &y, &mut [0; 67]));
				}
			}
		}
	}
}

#[cfg(test)]
mod constant_tests {
	use super::Engine;
	#[test]
	fn literal_multiply_signed_boundaries_and_tails() {
		for c in [
			i64::MIN,
			-(1i64 << 32),
			-4294967295,
			-37,
			-3,
			-1,
			0,
			1,
			3,
			37,
			4294967295,
			1i64 << 32,
			i64::MAX,
		] {
			let mut samples = std::vec![i64::MIN, i64::MAX, -1, 0, 1];
			if c != 0 {
				for bound in [
					i128::from(i64::MIN) / i128::from(c),
					i128::from(i64::MAX) / i128::from(c),
				] {
					for delta in -1..=1 {
						if let Ok(value) = i64::try_from(bound + delta) {
							samples.push(value);
						}
					}
				}
			}
			for force in [true, false] {
				let engine = Engine::new(force);
				for n in [0, 1, 3, 4, 7, 31, 64, 67] {
					for &value in &samples {
						let x = std::vec![value as u64; n];
						let mut out = std::vec![719; n + 2];
						assert_eq!(
							engine.mul_constant(&x, c, &mut out[1..n + 1]),
							n != 0 && value.checked_mul(c).is_none()
						);
						assert!(out[1..n + 1]
							.iter()
							.all(|&v| v == value.wrapping_mul(c) as u64));
						assert_eq!((out[0], out[n + 1]), (719, 719));
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod zero_null_tests {
	use super::Engine;

	#[test]
	fn preserves_all_valid_bits_and_clears_nulls_with_tails() {
		for force in [true, false] {
			let engine = Engine::new(force);
			for n in 0..=64 {
				for mask in [0, u64::MAX, 0xaaaaaaaaaaaaaaaa, 0x3141592653589793] {
					let mut integers = [i64::MIN + 1; 66];
					engine.zero_i64(&mut integers[1..n + 1], mask);
					for (i, &integer) in integers.iter().enumerate() {
						let keep = i == 0 || i > n || mask >> (i - 1) & 1 != 0;
						assert_eq!(integer, if keep { i64::MIN + 1 } else { 0 });
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod masked_float_block_tests {
	use super::{Engine, FloatOp, FloatRhs};
	use std::vec::Vec;

	#[test]
	fn fused_masked_float_blocks_match_portable_bits_and_canaries() {
		for n in [0usize, 1, 3, 4, 15, 16, 17, 63, 64, 65, 1023, 1024] {
			let patterns = [0, u64::MAX, 0xaaaaaaaaaaaaaaaa, 0x3141592653589793];
			for pattern in patterns {
				let masks = std::vec![pattern; n.div_ceil(64)];
				let samples = [
					0.0,
					-0.0,
					1.25,
					-3.5,
					f64::INFINITY,
					f64::NEG_INFINITY,
					f64::from_bits(0xfff8000000000001),
				];
				let x: Vec<_> = (0..n).map(|i| samples[i % samples.len()]).collect();
				let y: Vec<_> = (0..n).map(|i| samples[(i + 2) % samples.len()]).collect();
				for op in [FloatOp::Add, FloatOp::Sub, FloatOp::Mul] {
					for rhs in [FloatRhs::Column(op, &y), FloatRhs::AddConstant(1.0)] {
						let mut portable = std::vec![719.0; n + 2];
						let mut auto = portable.clone();
						Engine::new(true).masked_floats(&x, rhs, &mut portable[1..n + 1], &masks);
						Engine::new(false).masked_floats(&x, rhs, &mut auto[1..n + 1], &masks);
						for i in 0..n + 2 {
							// Arithmetic NaN payload/sign bits are not deterministic in Rust.
							// Keep exact bits for finite values, infinities, signed zero,
							// null payloads and canaries.
							if portable[i].is_nan() {
								assert!(auto[i].is_nan(), "lane {i}, n {n}");
							} else {
								assert_eq!(
									auto[i].to_bits(),
									portable[i].to_bits(),
									"lane {i}, n {n}"
								);
							}
						}
						assert_eq!((auto[0], auto[n + 1]), (719.0, 719.0));
						for i in 0..n {
							if masks[i / 64] >> (i % 64) & 1 == 0 {
								assert_eq!(auto[i + 1].to_bits(), 0);
							}
						}
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod borrowed_view_tests {
	use super::*;
	#[test]
	fn bit_views_preserve_storage_extents_and_all_patterns() {
		let mut ints = [719, i64::MIN, -1, 0, i64::MAX, 719];
		let source = &ints[1..5];
		assert_eq!(i64_words(source).as_ptr().cast::<i64>(), source.as_ptr());
		assert_eq!(i64_words(source), &[1 << 63, u64::MAX, 0, i64::MAX as u64]);
		i64_words_mut(&mut ints[1..5]).copy_from_slice(&[0, u64::MAX, 1 << 63, 1]);
		assert_eq!(ints, [719, 0, -1, i64::MIN, 1, 719]);
		let patterns = [0u64, 1 << 63, 0x7ff0000000000001, 0xffffffffffffffff];
		let mut floats = [719.0; 6];
		f64_words_mut(&mut floats[1..5]).copy_from_slice(&patterns);
		assert_eq!(f64_words(&floats[1..5]), patterns);
		assert_eq!(f64_words(&floats).as_ptr().cast::<f64>(), floats.as_ptr());
		assert_eq!((floats[0], floats[5]), (719.0, 719.0));
		assert!(i64_words(&[]).is_empty());
		assert!(f64_words_mut(&mut []).is_empty());
	}
}
