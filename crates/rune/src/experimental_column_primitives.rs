//! CK1 precompiled primitives: std/core only, independently compilable.
//! Safe wrappers prove equal lengths and exclusive output access. The only
//! target-feature entry is dominated by runtime detection (disabled in Miri).

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
