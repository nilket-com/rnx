//! Record 0137: Candle's shape, indexing and composition families, and
//! the passage-to-document mean.
//!
//! 40 core `Tensor` identities (`probes/0137/manifest.tsv`, counted with
//! 0134's by `count.py`) and one derived helper (`segment_mean`), under
//! 0134's contract plus two bounds that hold **independently of the value
//! count**, so a zero-value tensor can't drive a large allocation:
//!
//! - `AXIS`: any one axis length an operation turns into allocations
//!   (index vectors, reference lists, readback nodes);
//! - `PARTS`: a vector of tensors taken or returned.
//!
//! An axis may be negative (counted from the end); an element index (in
//! `get`, `get_on_dim` and inside index tensors) is non-negative and below
//! its axis, and a negative one is refused, not wrapped. Every index tensor
//! is read and bounds-checked in one pass before Candle sees it. Every
//! output, intermediate and expanded operand is checked against the cap
//! before the call, through 0134's `capped` (count and stride products).
use super::{
	MAX_ELEMS, MAX_RANK, axes, axis, broadcast_shape, capped, dtype_from, dtype_name, float_only,
	go, is_float, readback, same, same_dtype, shape_arg, wrap,
};
use crate::Tensor;
use candle_core::{DType, Tensor as CTensor};
use rnx::rune::{self, Value, runtime::Vec as RuneVec};

/// Any one axis an operation turns into allocations.
pub const AXIS: usize = 1 << 24;
/// A vector of tensors, taken or returned.
pub const PARTS: usize = 4096;

/// A non-negative integer argument, named.
fn count(op: &str, v: i64, what: &str) -> Result<usize, String> {
	usize::try_from(v).map_err(|_| format!("{op}: {what} must be non-negative, found {v}"))
}

/// The tensors of a Rune vector: 1 to `PARTS`, one dtype.
fn parts(op: &str, ts: &Value) -> Result<Vec<CTensor>, String> {
	let not = || format!("{op}: needs a vector of tensors");
	let values = ts.borrow_ref::<RuneVec>().map_err(|_| not())?;
	let n = values.len();
	if n == 0 || n > PARTS {
		return Err(format!("{op}: {n} tensors, want 1 to {PARTS}"));
	}
	let mut out = Vec::with_capacity(n);
	for v in values.iter() {
		// an Arc clone of the tensor's handle, not of its values
		let t = v.borrow_ref::<Tensor>().map_err(|_| not())?;
		out.push(t.0.clone());
	}
	for t in &out[1..] {
		same_dtype(op, &out[0], t)?;
	}
	Ok(out)
}

// ---- G: shape and composition ----

fn cat(ts: Value, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::cat";
	let ts = parts(op, &ts)?;
	let first = ts[0].dims().to_vec();
	let a = axis(op, first.len(), dim)?;
	let mut total = 0usize;
	for (k, t) in ts.iter().enumerate() {
		let d = t.dims();
		if d.len() != first.len() || d.iter().enumerate().any(|(i, &x)| i != a && x != first[i]) {
			return Err(format!(
				"{op}: tensor {k} has shape {d:?}, which differs from {first:?} away from axis {a}"
			));
		}
		total = total
			.checked_add(d[a])
			.ok_or_else(|| format!("{op}: the joined axis overflows"))?;
	}
	let mut out = first.clone();
	out[a] = total;
	capped(op, &out, "the result")?;
	wrap(op, move || CTensor::cat(&ts, a))
}

fn stack(ts: Value, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::stack";
	let ts = parts(op, &ts)?;
	let first = ts[0].dims().to_vec();
	// the new axis can go at any position 0..=rank
	let a = axis(op, first.len() + 1, dim)?;
	for (k, t) in ts.iter().enumerate() {
		if t.dims() != first.as_slice() {
			return Err(format!(
				"{op}: tensor {k} has shape {:?}, not {first:?}",
				t.dims()
			));
		}
	}
	let mut out = first.clone();
	out.insert(a, ts.len());
	capped(op, &out, "the result")?;
	wrap(op, move || CTensor::stack(&ts, a))
}

fn chunk(this: &Tensor, n: i64, dim: i64) -> Result<Vec<Tensor>, String> {
	let op = "Tensor::chunk";
	let n = count(op, n, "the chunk count")?;
	if n == 0 || n > PARTS {
		return Err(format!("{op}: {n} chunks, want 1 to {PARTS}"));
	}
	let a = axis(op, this.0.rank(), dim)?;
	// Candle returns min(n, size) views: one per element when n > size
	let returned = n.min(this.0.dims()[a]);
	debug_assert!(returned <= PARTS);
	let t = this.0.clone();
	go(op, move || t.chunk(n, a)).map(|v| v.into_iter().map(Tensor).collect())
}

fn permute(this: &Tensor, dims: Value) -> Result<Tensor, String> {
	let op = "Tensor::permute";
	let r = this.0.rank();
	let not = || format!("{op}: needs a permutation of 0..{r}");
	let values = dims.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.len() != r {
		return Err(not());
	}
	let mut perm = Vec::with_capacity(r);
	for v in values.iter() {
		let d = rune::from_value::<i64>(v.clone()).map_err(|_| not())?;
		let a = axis(op, r, d)?;
		if perm.contains(&a) {
			return Err(format!("{op}: axis {d} is repeated"));
		}
		perm.push(a);
	}
	let src = this.0.dims();
	let out: Vec<usize> = perm.iter().map(|&a| src[a]).collect();
	capped(op, &out, "the permuted shape")?;
	wrap(op, move || this.0.permute(perm))
}

/// Candle's broadcast rule for a view: the target has at least the
/// source's rank, and each trailing source axis equals the target's or is 1.
fn broadcast_to(op: &str, src: &[usize], target: &[usize]) -> Result<(), String> {
	if target.len() < src.len() {
		return Err(format!(
			"{op}: {src:?} can't broadcast to the lower-rank {target:?}"
		));
	}
	let off = target.len() - src.len();
	for (i, &s) in src.iter().enumerate() {
		if s != 1 && s != target[off + i] {
			return Err(format!("{op}: {src:?} doesn't broadcast to {target:?}"));
		}
	}
	// a view, but its readback or any copy materializes it
	capped(op, target, "the broadcast shape").map(|_| ())
}

fn broadcast_as(this: &Tensor, shape: Value) -> Result<Tensor, String> {
	let op = "Tensor::broadcast_as";
	let target = shape_arg(op, &shape)?;
	broadcast_to(op, this.0.dims(), &target)?;
	wrap(op, move || this.0.broadcast_as(target))
}

fn expand(this: &Tensor, shape: Value) -> Result<Tensor, String> {
	let op = "Tensor::expand";
	let target = shape_arg(op, &shape)?;
	broadcast_to(op, this.0.dims(), &target)?;
	wrap(op, move || this.0.expand(target))
}

fn broadcast_left(this: &Tensor, dims: Value) -> Result<Tensor, String> {
	let op = "Tensor::broadcast_left";
	let mut target = shape_arg(op, &dims)?;
	target.extend_from_slice(this.0.dims());
	if target.len() > MAX_RANK {
		return Err(format!(
			"{op}: the result has rank {}, at most {MAX_RANK}",
			target.len()
		));
	}
	let left = target[..target.len() - this.0.rank()].to_vec();
	broadcast_to(op, this.0.dims(), &target)?;
	wrap(op, move || this.0.broadcast_left(left))
}

/// 0134's checked merged product, for `flatten_from` and `flatten_to`.
fn merged(op: &str, dims: &[usize], a: usize, b: usize) -> Result<(), String> {
	let m = dims[a..=b]
		.iter()
		.try_fold(1usize, |p, &x| p.checked_mul(x))
		.ok_or_else(|| format!("{op}: axes {a} to {b} of {dims:?} overflow when merged"))?;
	let mut d = dims[..a].to_vec();
	d.push(m);
	d.extend_from_slice(&dims[b + 1..]);
	capped(op, &d, "the flattened shape").map(|_| ())
}

fn flatten_from(this: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::flatten_from";
	let r = this.0.rank();
	if r == 0 {
		return Err(format!("{op}: needs rank 1 or more"));
	}
	let a = axis(op, r, dim)?;
	merged(op, this.0.dims(), a, r - 1)?;
	wrap(op, move || this.0.flatten_from(a))
}

fn flatten_to(this: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::flatten_to";
	let r = this.0.rank();
	if r == 0 {
		return Err(format!("{op}: needs rank 1 or more"));
	}
	let a = axis(op, r, dim)?;
	merged(op, this.0.dims(), 0, a)?;
	wrap(op, move || this.0.flatten_to(a))
}

fn flip(this: &Tensor, dims: Value) -> Result<Tensor, String> {
	let op = "Tensor::flip";
	let ax = axes(op, this.0.rank(), &dims)?;
	// Candle allocates an i64 index vector the length of each flipped axis
	// and copies the tensor once per axis (a loop of index_select)
	for &a in &ax {
		let n = this.0.dims()[a];
		if n > AXIS {
			return Err(format!(
				"{op}: axis {a} has length {n}, above {AXIS} (its index vector is allocated)"
			));
		}
	}
	capped(op, this.0.dims(), "each copy")?;
	wrap(op, move || this.0.flip(&ax))
}

fn roll(this: &Tensor, shift: i64, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::roll";
	let a = axis(op, this.0.rank(), dim)?;
	let size = this.0.dims()[a];
	if size == 0 {
		return Err(format!("{op}: axis {a} is empty"));
	}
	// Candle reduces the shift with `rem_euclid(size as i32)` in i32
	if size > i32::MAX as usize {
		return Err(format!(
			"{op}: axis {a} has length {size}, above i32::MAX (Candle's shift arithmetic)"
		));
	}
	// any i64 shift, MIN included, reduced in i64 to 0 <= s < size
	let s = shift.rem_euclid(size as i64) as i32;
	capped(op, this.0.dims(), "the result")?;
	wrap(op, move || this.0.roll(s, a))
}

fn repeat(this: &Tensor, multipliers: Value) -> Result<Tensor, String> {
	let op = "Tensor::repeat";
	let r = this.0.rank();
	let not = || format!("{op}: needs {r} multipliers (one per axis), each 1 to {AXIS}");
	let values = multipliers.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.len() != r {
		return Err(not());
	}
	let mut dims = this.0.dims().to_vec();
	let mut reps = Vec::with_capacity(r);
	for (i, v) in values.iter().enumerate() {
		let m = rune::from_value::<i64>(v.clone()).map_err(|_| not())?;
		// Candle treats 0 as 1, not as an empty axis; its reference list is
		// `vec![&inp; m]`, so m is bounded even for a zero-value tensor
		if m < 1 || m as u64 > AXIS as u64 {
			return Err(format!(
				"{op}: multiplier {m} for axis {i}, want 1 to {AXIS} (0 is refused: Candle treats it as 1)"
			));
		}
		let m = m as usize;
		// each axis's intermediate, in Candle's order
		dims[i] = dims[i]
			.checked_mul(m)
			.ok_or_else(|| format!("{op}: axis {i} overflows"))?;
		capped(op, &dims, "an intermediate")?;
		reps.push(m);
	}
	wrap(op, move || this.0.repeat(reps))
}

fn padded(op: &str, this: &CTensor, a: usize, left: usize, right: usize) -> Result<(), String> {
	let mut d = this.dims().to_vec();
	d[a] = d[a]
		.checked_add(left)
		.and_then(|x| x.checked_add(right))
		.ok_or_else(|| format!("{op}: the padded axis overflows"))?;
	capped(op, &d, "the padded result").map(|_| ())
}

fn pad_with_zeros(this: &Tensor, dim: i64, left: i64, right: i64) -> Result<Tensor, String> {
	let op = "Tensor::pad_with_zeros";
	let a = axis(op, this.0.rank(), dim)?;
	let (l, r) = (count(op, left, "left")?, count(op, right, "right")?);
	if l.checked_add(r).is_none_or(|x| x > AXIS) {
		return Err(format!("{op}: left + right is above {AXIS}"));
	}
	padded(op, &this.0, a, l, r)?;
	wrap(op, move || this.0.pad_with_zeros(a, l, r))
}

fn pad_with_same(this: &Tensor, dim: i64, left: i64, right: i64) -> Result<Tensor, String> {
	let op = "Tensor::pad_with_same";
	let a = axis(op, this.0.rank(), dim)?;
	let (l, r) = (count(op, left, "left")?, count(op, right, "right")?);
	// Candle's reference list is left + right + 1 long
	if l.checked_add(r)
		.and_then(|x| x.checked_add(1))
		.is_none_or(|x| x > AXIS)
	{
		return Err(format!(
			"{op}: left + right + 1 is above {AXIS} (Candle's reference list)"
		));
	}
	if (l > 0 || r > 0) && this.0.elem_count() == 0 {
		return Err(format!(
			"{op}: the tensor is empty; there is no edge to repeat"
		));
	}
	padded(op, &this.0, a, l, r)?;
	wrap(op, move || this.0.pad_with_same(a, l, r))
}

fn unfold(this: &Tensor, dim: i64, size: i64, step: i64) -> Result<Tensor, String> {
	let op = "Tensor::unfold";
	let r = this.0.rank();
	if r == 0 {
		return Err(format!("{op}: needs rank 1 or more"));
	}
	let a = axis(op, r, dim)?;
	let (size, step) = (count(op, size, "size")?, count(op, step, "step")?);
	let len = this.0.dims()[a];
	if size == 0 || step == 0 || size > len {
		return Err(format!(
			"{op}: size {size} and step {step} on an axis of {len}: want size 1 to {len}, step at least 1"
		));
	}
	// Candle counts the windows in f32; the integer count must agree
	let windows = (len - size) / step + 1;
	let candle = ((len as f32 - size as f32) / step as f32 + 1.) as usize;
	if candle != windows {
		return Err(format!(
			"{op}: Candle's f32 window count {candle} differs from the exact {windows} for an axis of {len}"
		));
	}
	// and multiplies the axis's stride by the step, unchecked
	this.0.stride()[a]
		.checked_mul(step)
		.filter(|&x| x <= i64::MAX as usize)
		.ok_or_else(|| format!("{op}: the stride of axis {a} times {step} is above i64::MAX"))?;
	let mut d = this.0.dims().to_vec();
	d[a] = windows;
	d.push(size);
	capped(op, &d, "the windows")?;
	wrap(op, move || this.0.unfold(a, size, step))
}

fn tri(op: &str, n: i64, dtype: &str, upper: bool) -> Result<Tensor, String> {
	let n = count(op, n, "n")?;
	let dt = dtype_from(dtype, op)?;
	capped(op, &[n, n], "the matrix")?;
	wrap(op, move || {
		if upper {
			CTensor::triu2(n, dt, super::CPU)
		} else {
			CTensor::tril2(n, dt, super::CPU)
		}
	})
}

fn tril2(n: i64, dtype: &str) -> Result<Tensor, String> {
	tri("Tensor::tril2", n, dtype, false)
}

fn triu2(n: i64, dtype: &str) -> Result<Tensor, String> {
	tri("Tensor::triu2", n, dtype, true)
}

fn to_vec0(this: &Tensor) -> Result<Value, String> {
	let op = "Tensor::to_vec0";
	if this.0.rank() != 0 {
		return Err(format!("{op}: needs rank 0, found {}", this.0.rank()));
	}
	let v = readback(op, &this.0)?;
	let v = v
		.borrow_ref::<RuneVec>()
		.map_err(|e| format!("{op}: {e}"))?;
	Ok(v[0].clone())
}

fn to_vec3(this: &Tensor) -> Result<Value, String> {
	let op = "Tensor::to_vec3";
	let [d0, d1, d2] = *this.0.dims() else {
		return Err(format!("{op}: needs rank 3, found {}", this.0.rank()));
	};
	// the outer and middle list nodes, independently of the leaves
	d0.checked_mul(d1)
		.filter(|&x| d0 <= AXIS && x <= AXIS)
		.ok_or_else(|| {
			format!("{op}: {d0} x {d1} lists, above {AXIS} (independently of the values)")
		})?;
	let flat = readback(op, &this.0)?;
	let flat = flat
		.borrow_ref::<RuneVec>()
		.map_err(|e| format!("{op}: {e}"))?;
	let mut out = Vec::with_capacity(d0);
	for i in 0..d0 {
		let mut mid = Vec::with_capacity(d1);
		for j in 0..d1 {
			let at = (i * d1 + j) * d2;
			let leaf: Vec<Value> = flat[at..at + d2].to_vec();
			mid.push(rune::to_value(leaf).map_err(|e| format!("{op}: {e}"))?);
		}
		out.push(rune::to_value(mid).map_err(|e| format!("{op}: {e}"))?);
	}
	rune::to_value(out).map_err(|e| format!("{op}: {e}"))
}

fn dim(this: &Tensor, d: i64) -> Result<i64, String> {
	let op = "Tensor::dim";
	let a = axis(op, this.0.rank(), d)?;
	i64::try_from(this.0.dims()[a]).map_err(|_| format!("{op}: the axis is above i64::MAX"))
}

fn stride(this: &Tensor) -> Result<Vec<i64>, String> {
	let op = "Tensor::stride";
	this.0
		.stride()
		.iter()
		.map(|&s| i64::try_from(s).map_err(|_| format!("{op}: a stride is above i64::MAX")))
		.collect()
}

fn force_contiguous(this: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::force_contiguous";
	capped(op, this.0.dims(), "the copy")?;
	wrap(op, || this.0.force_contiguous())
}

fn copy(this: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::copy";
	capped(op, this.0.dims(), "the copy")?;
	wrap(op, || this.0.copy())
}

// ---- H: indexing and grouping ----

/// An index tensor's values, read in one pass and checked: u32 or i64,
/// non-negative, below `bound`. The first bad position is named.
fn checked_ids(op: &str, ids: &CTensor, bound: usize) -> Result<Vec<usize>, String> {
	if !matches!(ids.dtype(), DType::U32 | DType::I64) {
		return Err(format!(
			"{op}: indices must be u32 or i64, found {}",
			dtype_name(ids.dtype())
		));
	}
	capped(op, ids.dims(), "the indices")?;
	let t = ids.clone();
	let values: Vec<i64> = match ids.dtype() {
		DType::U32 => go(op, move || t.flatten_all()?.to_vec1::<u32>())?
			.into_iter()
			.map(i64::from)
			.collect(),
		_ => go(op, move || t.flatten_all()?.to_vec1::<i64>())?,
	};
	let mut out = Vec::with_capacity(values.len());
	for (k, &v) in values.iter().enumerate() {
		if v < 0 || v as u64 >= bound as u64 {
			return Err(format!(
				"{op}: index {v} at position {k} is outside 0..{bound}"
			));
		}
		out.push(v as usize);
	}
	Ok(out)
}

fn index_select(this: &Tensor, ids: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::index_select";
	let a = axis(op, this.0.rank(), dim)?;
	let [n] = *ids.0.dims() else {
		return Err(format!(
			"{op}: indices must be rank 1, found {:?}",
			ids.0.dims()
		));
	};
	if n > AXIS {
		return Err(format!("{op}: {n} indices, at most {AXIS}"));
	}
	checked_ids(op, &ids.0, this.0.dims()[a])?;
	let mut d = this.0.dims().to_vec();
	d[a] = n;
	capped(op, &d, "the result")?;
	wrap(op, move || this.0.index_select(&ids.0, a))
}

fn gather(this: &Tensor, ids: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::gather";
	let a = axis(op, this.0.rank(), dim)?;
	let (d, i) = (this.0.dims(), ids.0.dims());
	if i.len() != d.len()
		|| i.iter()
			.zip(d)
			.enumerate()
			.any(|(k, (x, y))| k != a && x != y)
	{
		return Err(format!(
			"{op}: indices of shape {i:?} must match {d:?} away from axis {a}"
		));
	}
	checked_ids(op, &ids.0, d[a])?;
	capped(op, i, "the result")?;
	wrap(op, move || this.0.gather(&ids.0, a))
}

fn embedding(this: &Tensor, ids: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::embedding";
	let [rows, cols] = *this.0.dims() else {
		return Err(format!(
			"{op}: the table must be rank 2, found {:?}",
			this.0.dims()
		));
	};
	let [n] = *ids.0.dims() else {
		return Err(format!(
			"{op}: ids must be rank 1, found {:?}",
			ids.0.dims()
		));
	};
	if n > AXIS {
		return Err(format!("{op}: {n} ids, at most {AXIS}"));
	}
	checked_ids(op, &ids.0, rows)?;
	capped(op, &[n, cols], "the result")?;
	wrap(op, move || this.0.embedding(&ids.0))
}

fn index_add(this: &Tensor, ids: &Tensor, src: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::index_add";
	let a = axis(op, this.0.rank(), dim)?;
	same_dtype(op, &this.0, &src.0)?;
	let (d, s) = (this.0.dims(), src.0.dims());
	if s.len() != d.len()
		|| s.iter()
			.zip(d)
			.enumerate()
			.any(|(k, (x, y))| k != a && x != y)
	{
		return Err(format!(
			"{op}: the source {s:?} must match {d:?} away from axis {a}"
		));
	}
	let [n] = *ids.0.dims() else {
		return Err(format!(
			"{op}: indices must be rank 1, found {:?}",
			ids.0.dims()
		));
	};
	if n != s[a] {
		return Err(format!("{op}: {n} indices for a source axis of {}", s[a]));
	}
	checked_ids(op, &ids.0, d[a])?;
	capped(op, d, "the result")?;
	wrap(op, move || this.0.index_add(&ids.0, &src.0, a))
}

fn scatter_add(this: &Tensor, ids: &Tensor, src: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::scatter_add";
	let a = axis(op, this.0.rank(), dim)?;
	same_dtype(op, &this.0, &src.0)?;
	let (d, s) = (this.0.dims(), src.0.dims());
	if s.len() != d.len()
		|| s.iter()
			.zip(d)
			.enumerate()
			.any(|(k, (x, y))| k != a && x != y)
	{
		return Err(format!(
			"{op}: the source {s:?} must match {d:?} away from axis {a}"
		));
	}
	if ids.0.dims() != s {
		return Err(format!(
			"{op}: indices of shape {:?} must have the source's shape {s:?}",
			ids.0.dims()
		));
	}
	checked_ids(op, &ids.0, d[a])?;
	capped(op, d, "the result")?;
	wrap(op, move || this.0.scatter_add(&ids.0, &src.0, a))
}

fn get(this: &Tensor, i: i64) -> Result<Tensor, String> {
	let op = "Tensor::get";
	let Some(&n) = this.0.dims().first() else {
		return Err(format!("{op}: a rank-0 tensor has no elements to index"));
	};
	if i < 0 || i as u64 >= n as u64 {
		return Err(format!("{op}: element index {i} is outside 0..{n}"));
	}
	let i = i as usize;
	wrap(op, move || this.0.get(i))
}

fn get_on_dim(this: &Tensor, dim: i64, i: i64) -> Result<Tensor, String> {
	let op = "Tensor::get_on_dim";
	let a = axis(op, this.0.rank(), dim)?;
	let n = this.0.dims()[a];
	if i < 0 || i as u64 >= n as u64 {
		return Err(format!(
			"{op}: element index {i} is outside 0..{n} on axis {a}"
		));
	}
	let i = i as usize;
	wrap(op, move || this.0.get_on_dim(a, i))
}

/// A range's two integers, from a borrowed tuple or vector of exactly two.
fn pair(v: &Value) -> Option<(i64, i64)> {
	let two = |xs: &[Value]| match xs {
		[a, b] => Some((a.as_integer::<i64>().ok()?, b.as_integer::<i64>().ok()?)),
		_ => None,
	};
	if let Ok(t) = v.borrow_tuple_ref() {
		return two(&t);
	}
	let xs = v.borrow_ref::<RuneVec>().ok()?;
	two(&xs)
}

fn slice_assign(this: &Tensor, ranges: Value, src: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::slice_assign";
	let r = this.0.rank();
	let not = || format!("{op}: needs {r} ranges, each [start, end] with start < end");
	let values = ranges.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.len() != r {
		return Err(not());
	}
	same_dtype(op, &this.0, &src.0)?;
	let mut rs = Vec::with_capacity(r);
	let mut extents = Vec::with_capacity(r);
	for (i, v) in values.iter().enumerate() {
		// a tuple (start, end) or a two-element vector [start, end],
		// borrowed (never taken from the script) and checked for exactly two
		// integers before anything else
		let (s, e) = pair(v).ok_or_else(not)?;
		let len = this.0.dims()[i];
		if s < 0 || e <= s || e as u64 > len as u64 {
			return Err(format!(
				"{op}: range [{s}, {e}) on axis {i} of length {len}"
			));
		}
		rs.push(s as usize..e as usize);
		extents.push((e - s) as usize);
	}
	if src.0.dims() != extents.as_slice() {
		return Err(format!(
			"{op}: the source has shape {:?}, the ranges {extents:?}",
			src.0.dims()
		));
	}
	// Candle builds a u8 mask of the source's shape and pads it and the
	// source to the receiver's shape: each intermediate is that size
	capped(op, this.0.dims(), "each intermediate")?;
	wrap(op, move || this.0.slice_assign(&rs, &src.0))
}

// ---- I: composition math ----

fn dot(this: &Tensor, rhs: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::dot";
	float_only(op, &this.0)?;
	same(op, &this.0, &rhs.0)?;
	if this.0.rank() != 1 {
		return Err(format!("{op}: needs rank 1, found {}", this.0.rank()));
	}
	wrap(op, move || this.0.dot(&rhs.0))
}

fn mv(this: &Tensor, rhs: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::mv";
	float_only(op, &this.0)?;
	same_dtype(op, &this.0, &rhs.0)?;
	let ([_, k], [n]) = (this.0.dims(), rhs.0.dims()) else {
		return Err(format!(
			"{op}: needs rank 2 by rank 1, found {:?} and {:?}",
			this.0.dims(),
			rhs.0.dims()
		));
	};
	if k != n {
		return Err(format!("{op}: inner lengths {k} and {n} differ"));
	}
	wrap(op, move || this.0.mv(&rhs.0))
}

fn cumsum(this: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::cumsum";
	float_only(op, &this.0)?;
	let r = this.0.rank();
	if r == 0 {
		return Ok(Tensor(this.0.clone()));
	}
	let a = axis(op, r, dim)?;
	let n = this.0.dims()[a];
	// Candle builds triu2(n) and multiplies by it
	n.checked_mul(n)
		.filter(|&x| x <= MAX_ELEMS)
		.ok_or_else(|| {
			format!("{op}: an axis of {n} needs a {n} x {n} matrix, above {MAX_ELEMS} values")
		})?;
	if r > 1 {
		// Candle transposes the axis last and multiplies by triu2(n) with
		// broadcast_matmul, which concretizes triu2 across the transposed
		// shape's leading (batch) axes: all but its last two, the matrix's
		// own rows and columns
		let mut t = this.0.dims().to_vec();
		t.swap(a, r - 1);
		capped(op, &t, "the transposed copy")?;
		let mut expanded = t[..r - 2].to_vec();
		expanded.extend_from_slice(&[n, n]);
		capped(op, &expanded, "the expanded triu2 operand").map_err(|_| {
			format!(
				"{op}: {:?} batches of a {n} x {n} matrix, above {MAX_ELEMS} values",
				&t[..r - 2]
			)
		})?;
	}
	wrap(op, move || this.0.cumsum(a))
}

fn norm(this: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::norm";
	float_only(op, &this.0)?;
	wrap(op, || this.0.norm())
}

fn log_sum_exp(this: &Tensor, dims: Value) -> Result<Tensor, String> {
	let op = "Tensor::log_sum_exp";
	float_only(op, &this.0)?;
	let ax = axes(op, this.0.rank(), &dims)?;
	capped(op, this.0.dims(), "the intermediates")?;
	wrap(op, move || this.0.log_sum_exp(ax))
}

fn pow(this: &Tensor, e: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::pow";
	float_only(op, &this.0)?;
	same(op, &this.0, &e.0)?;
	wrap(op, move || this.0.pow(&e.0))
}

fn broadcast_pow(this: &Tensor, e: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::broadcast_pow";
	float_only(op, &this.0)?;
	same_dtype(op, &this.0, &e.0)?;
	broadcast_shape(op, this.0.dims(), e.0.dims())?;
	wrap(op, move || this.0.broadcast_pow(&e.0))
}

fn elu(this: &Tensor, alpha: f64) -> Result<Tensor, String> {
	let op = "Tensor::elu";
	float_only(op, &this.0)?;
	if !alpha.is_finite() {
		return Err(format!("{op}: alpha must be finite, found {alpha}"));
	}
	wrap(op, move || this.0.elu(alpha))
}

fn round_to(this: &Tensor, decimals: i64) -> Result<Tensor, String> {
	let op = "Tensor::round_to";
	float_only(op, &this.0)?;
	if !(-30..=30).contains(&decimals) {
		return Err(format!("{op}: decimals {decimals}, want -30 to 30"));
	}
	let d = decimals as i32;
	wrap(op, move || this.0.round_to(d))
}

// ---- the derived helper: the passage-to-document mean ----

/// `candle::segment_mean(values, segments, n)`: row k of the (n × width)
/// result is the mean of the rows of `values` whose segment is k, computed
/// by Candle's own calls (`index_add` of the rows, divided by the counts
/// from `index_add` of ones), so same-call parity is exact. An empty
/// segment is refused by name. Non-finite values follow IEEE.
pub(crate) fn segment_mean(values: &Tensor, segments: &Tensor, n: i64) -> Result<Tensor, String> {
	let op = "candle::segment_mean";
	if !is_float(values.0.dtype()) {
		return Err(format!(
			"{op}: values must be f32 or f64, found {}",
			dtype_name(values.0.dtype())
		));
	}
	let [rows, width] = *values.0.dims() else {
		return Err(format!(
			"{op}: values must be rank 2 (rows x width), found {:?}",
			values.0.dims()
		));
	};
	let [len] = *segments.0.dims() else {
		return Err(format!(
			"{op}: segments must be rank 1, found {:?}",
			segments.0.dims()
		));
	};
	if len != rows {
		return Err(format!("{op}: {len} segment ids for {rows} rows"));
	}
	let n = count(op, n, "n")?;
	if n == 0 || n > AXIS {
		return Err(format!("{op}: n is {n}, want 1 to {AXIS}"));
	}
	// the output, the counts, the ones and the broadcast division's copy
	capped(op, &[n, width], "the result")?;
	capped(op, &[rows], "the ones")?;
	let ids = checked_ids(op, &segments.0, n)?;
	let mut sizes = vec![0usize; n];
	for &k in &ids {
		sizes[k] += 1;
	}
	if let Some(k) = sizes.iter().position(|&c| c == 0) {
		return Err(format!("{op}: segment {k} has no rows"));
	}
	let (v, s, dt) = (values.0.clone(), segments.0.clone(), values.0.dtype());
	wrap(op, move || {
		let sums = CTensor::zeros((n, width), dt, super::CPU)?.index_add(&s, &v, 0)?;
		let ones = CTensor::ones(rows, dt, super::CPU)?;
		let counts = CTensor::zeros(n, dt, super::CPU)?.index_add(&s, &ones, 0)?;
		sums.broadcast_div(&counts.unsqueeze(1)?)
	})
}

pub(super) fn build(m: &mut rnx::rune::Module) -> Result<(), rnx::rune::ContextError> {
	macro_rules! assoc {
		($($name:ident),*) => {$( m.associated_function(stringify!($name), $name)?; )*};
	}
	m.function("cat", cat).build_associated::<Tensor>()?;
	m.function("stack", stack).build_associated::<Tensor>()?;
	m.function("tril2", tril2).build_associated::<Tensor>()?;
	m.function("triu2", triu2).build_associated::<Tensor>()?;
	assoc!(
		chunk,
		permute,
		broadcast_as,
		expand,
		broadcast_left,
		flatten_from,
		flatten_to,
		flip,
		roll,
		repeat,
		pad_with_zeros,
		pad_with_same,
		unfold,
		to_vec0,
		to_vec3,
		dim,
		stride,
		force_contiguous,
		copy,
		index_select,
		gather,
		embedding,
		index_add,
		scatter_add,
		get,
		get_on_dim,
		slice_assign,
		dot,
		mv,
		cumsum,
		norm,
		log_sum_exp,
		pow,
		broadcast_pow,
		elu,
		round_to
	);
	m.function("segment_mean", segment_mean).build()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	//! Record 0137's controls: each identity against the direct Candle call
	//! (bit for bit), each contract's refusals named before Candle, and the
	//! pooling helper's same-call parity and its stated numerics.
	use super::super::from_vec;
	use super::*;
	use candle_core::Device;

	fn tv(values: Vec<f64>, shape: &[i64], dtype: &str) -> Tensor {
		from_vec(
			rune::to_value(values).unwrap(),
			rune::to_value(shape.to_vec()).unwrap(),
			dtype,
		)
		.unwrap()
	}
	fn ids(values: Vec<i64>, dtype: &str) -> Tensor {
		let n = values.len() as i64;
		tv(values.into_iter().map(|v| v as f64).collect(), &[n], dtype)
	}
	fn seq(n: usize, dims: &[i64]) -> Tensor {
		tv((0..n).map(|i| i as f64 * 0.5 - 3.0).collect(), dims, "f32")
	}
	fn bits(t: &CTensor) -> (Vec<usize>, Vec<u32>) {
		let v = t
			.flatten_all()
			.unwrap()
			.to_dtype(DType::F32)
			.unwrap()
			.to_vec1::<f32>()
			.unwrap();
		(t.dims().to_vec(), v.iter().map(|x| x.to_bits()).collect())
	}
	fn eq(ours: Result<Tensor, String>, theirs: candle_core::Result<CTensor>) {
		assert_eq!(bits(&ours.unwrap().0), bits(&theirs.unwrap()));
	}
	fn refuses<T>(r: Result<T, String>, want: &str) {
		match r {
			Ok(_) => panic!("expected a refusal containing {want:?}"),
			Err(e) => assert!(e.contains(want), "{e} (wanted {want})"),
		}
	}
	fn v(x: &[i64]) -> Value {
		rune::to_value(x.to_vec()).unwrap()
	}
	fn list(ts: &[&Tensor]) -> Value {
		rune::to_value(ts.iter().map(|t| (*t).clone()).collect::<Vec<_>>()).unwrap()
	}
	fn zeros(dims: &[usize]) -> Tensor {
		Tensor(CTensor::zeros(dims, DType::F32, &Device::Cpu).unwrap())
	}

	#[test]
	fn shape_and_composition_match_candle() {
		let a = seq(12, &[3, 4]);
		let b = seq(8, &[2, 4]);
		eq(cat(list(&[&a, &b]), 0), CTensor::cat(&[&a.0, &b.0], 0));
		eq(cat(list(&[&a, &a]), -1), CTensor::cat(&[&a.0, &a.0], 1));
		eq(stack(list(&[&a, &a]), 1), CTensor::stack(&[&a.0, &a.0], 1));
		let parts = chunk(&a, 3, -1).unwrap();
		let theirs = a.0.chunk(3, 1).unwrap();
		assert_eq!(parts.len(), theirs.len());
		for (p, q) in parts.iter().zip(&theirs) {
			assert_eq!(bits(&p.0), bits(q));
		}
		// more chunks than elements: one per element, as Candle
		assert_eq!(chunk(&b, 9, 0).unwrap().len(), 2);
		let c = seq(24, &[2, 3, 4]);
		eq(permute(&c, v(&[2, 0, -2])), c.0.permute((2, 0, 1)));
		let row = seq(4, &[1, 4]);
		eq(broadcast_as(&row, v(&[3, 4])), row.0.broadcast_as((3, 4)));
		eq(expand(&row, v(&[2, 3, 4])), row.0.expand((2, 3, 4)));
		eq(broadcast_left(&row, v(&[2])), row.0.broadcast_left(2));
		eq(flatten_from(&c, 1), c.0.flatten_from(1));
		eq(flatten_to(&c, -2), c.0.flatten_to(1));
		eq(flip(&c, v(&[0, 2])), c.0.flip(&[0, 2]));
		for s in [1, -1, 7, i64::MIN, i64::MAX] {
			let want = s.rem_euclid(4) as i32;
			eq(roll(&a, s, 1), a.0.roll(want, 1));
		}
		eq(repeat(&a, v(&[2, 1])), a.0.repeat((2, 1)));
		eq(pad_with_zeros(&a, 0, 1, 2), a.0.pad_with_zeros(0, 1, 2));
		eq(pad_with_same(&a, -1, 2, 1), a.0.pad_with_same(1, 2, 1));
		eq(unfold(&a, 1, 2, 1), a.0.unfold(1, 2, 1));
		eq(unfold(&a, 1, 3, 2), a.0.unfold(1, 3, 2));
		eq(tril2(3, "f32"), CTensor::tril2(3, DType::F32, &Device::Cpu));
		eq(triu2(3, "f64"), CTensor::triu2(3, DType::F64, &Device::Cpu));
		let s = tv(vec![2.5], &[], "f32");
		assert_eq!(rune::from_value::<f64>(to_vec0(&s).unwrap()).unwrap(), 2.5);
		let three = rune::from_value::<Vec<Vec<Vec<f64>>>>(to_vec3(&c).unwrap()).unwrap();
		assert_eq!(three.len(), 2);
		assert_eq!(
			three[1][2][3],
			c.0.to_vec3::<f32>().unwrap()[1][2][3] as f64
		);
		assert_eq!(dim(&c, -1).unwrap(), 4);
		assert_eq!(stride(&c).unwrap(), vec![12, 4, 1]);
		let t = c.0.t().unwrap();
		eq(force_contiguous(&Tensor(t.clone())), t.force_contiguous());
		eq(copy(&Tensor(t.clone())), t.copy());
	}

	#[test]
	fn indexing_and_grouping_match_candle() {
		let a = seq(12, &[3, 4]);
		for dt in ["u32", "i64"] {
			let i = ids(vec![2, 0, 2], dt);
			eq(index_select(&a, &i, 0), a.0.index_select(&i.0, 0));
			eq(embedding(&a, &i), a.0.embedding(&i.0));
		}
		let g = tv(
			vec![0., 3., 1., 1., 2., 0., 3., 3., 0., 1., 2., 3.],
			&[3, 4],
			"i64",
		);
		eq(gather(&a, &g, 1), a.0.gather(&g.0, 1));
		let src = seq(8, &[2, 4]);
		let i = ids(vec![2, 2], "u32");
		eq(index_add(&a, &i, &src, 0), a.0.index_add(&i.0, &src.0, 0));
		let si = tv(vec![0., 1., 2., 0., 2., 2., 1., 0.], &[2, 4], "i64");
		eq(
			scatter_add(&a, &si, &src, 0),
			a.0.scatter_add(&si.0, &src.0, 0),
		);
		eq(get(&a, 1), a.0.get(1));
		eq(get_on_dim(&a, -1, 3), a.0.get_on_dim(1, 3));
		let patch = seq(4, &[2, 2]);
		for ranges in [
			rune::to_value(vec![(1i64, 3i64), (0i64, 2i64)]).unwrap(),
			rune::to_value(vec![vec![1i64, 3], vec![0i64, 2]]).unwrap(),
		] {
			eq(
				slice_assign(&a, ranges, &patch),
				a.0.slice_assign(&[1..3, 0..2], &patch.0),
			);
		}
		// receivers unchanged
		assert_eq!(bits(&a.0), bits(&seq(12, &[3, 4]).0));
	}

	#[test]
	fn composition_math_matches_candle() {
		let x = seq(5, &[5]);
		let y = tv(vec![1., 2., 3., 4., 5.], &[5], "f32");
		eq(dot(&x, &y), x.0.dot(&y.0));
		let m = seq(15, &[3, 5]);
		eq(mv(&m, &y), m.0.mv(&y.0));
		eq(cumsum(&x, 0), x.0.cumsum(0));
		let c = seq(24, &[2, 3, 4]);
		eq(cumsum(&c, 1), c.0.cumsum(1));
		// review round 1: a 2-D matrix has no broadcast batch; [2048, 8]
		// along axis 0 is a 2048 x 2048 triu2, accepted, as direct Candle
		let tall = seq(2048 * 8, &[2048, 8]);
		eq(cumsum(&tall, 0), tall.0.cumsum(0));
		eq(norm(&m), m.0.norm());
		eq(log_sum_exp(&m, v(&[1])), m.0.log_sum_exp(1));
		let p = tv(vec![1., 2., 0.5, 3., 2.], &[5], "f32");
		eq(pow(&y, &p), y.0.pow(&p.0));
		let e = tv(vec![2.], &[1], "f32");
		eq(
			broadcast_pow(&m.0.abs().map(Tensor).unwrap(), &e),
			m.0.abs().unwrap().broadcast_pow(&e.0),
		);
		eq(elu(&x, 0.5), x.0.elu(0.5));
		eq(round_to(&x, 1), x.0.round_to(1));
	}

	#[test]
	fn the_review_cases_are_refused_by_name_before_candle() {
		let a = seq(12, &[3, 4]);
		// repeat: 0 is refused (Candle treats it as 1); a huge multiplier on
		// a zero-value tensor is bounded by AXIS
		refuses(repeat(&a, v(&[0, 1])), "0 is refused");
		refuses(repeat(&zeros(&[0, 1]), v(&[1, 1 << 30])), "want 1 to");
		refuses(repeat(&a, v(&[2])), "needs 2 multipliers");
		// chunk: n above PARTS
		refuses(chunk(&a, (PARTS + 1) as i64, 0), "want 1 to 4096");
		refuses(chunk(&a, 0, 0), "want 1 to 4096");
		// flip: an axis above AXIS, with zero values
		refuses(flip(&zeros(&[0, AXIS + 1]), v(&[1])), "above 16777216");
		// pad_with_same: left + right + 1 above AXIS; an empty tensor
		refuses(pad_with_same(&a, 0, (AXIS - 1) as i64, 1), "reference list");
		refuses(pad_with_same(&zeros(&[0, 3]), 1, 1, 0), "empty");
		refuses(pad_with_zeros(&a, 0, -1, 0), "non-negative");
		// to_vec3: the list nodes, independently of the leaves
		refuses(to_vec3(&zeros(&[1 << 20, 1 << 20, 0])), "lists, above");
		// roll: an axis above i32::MAX with zero values; an empty axis
		refuses(roll(&zeros(&[0, 1 << 32]), 1, 1), "above i32::MAX");
		refuses(roll(&zeros(&[0, 3]), 1, 0), "is empty");
		// unfold: a count Candle's f32 arithmetic gets wrong; a stride that
		// overflows; a zero-value large axis
		let long = zeros(&[0, (1 << 25) + 1]);
		refuses(unfold(&long, 1, 1, 3), "f32 window count");
		refuses(unfold(&a, 1, 5, 1), "want size 1 to 4");
		refuses(unfold(&a, 1, 2, 0), "step at least 1");
		// element indices against axes
		refuses(get(&tv(vec![1.], &[], "f32"), 0), "rank-0");
		refuses(get(&zeros(&[0, 2]), 0), "outside 0..0");
		refuses(get(&a, -1), "element index -1");
		refuses(get_on_dim(&a, -1, 4), "outside 0..4");
		refuses(
			index_select(&a, &ids(vec![0, -1], "i64"), 0),
			"index -1 at position 1",
		);
		refuses(
			index_select(&a, &ids(vec![3], "u32"), 0),
			"index 3 at position 0",
		);
		refuses(index_select(&a, &seq(2, &[2]), 0), "u32 or i64");
		refuses(
			embedding(&seq(24, &[2, 3, 4]), &ids(vec![0], "u32")),
			"rank 2",
		);
		refuses(gather(&a, &ids(vec![0], "i64"), 1), "must match");
		refuses(
			scatter_add(&a, &ids(vec![0], "i64"), &seq(4, &[1, 4]), 0),
			"source's shape",
		);
		let ranges = rune::to_value(vec![(0i64, 2i64), (3i64, 3i64)]).unwrap();
		refuses(slice_assign(&a, ranges, &seq(2, &[2, 1])), "range [3, 3)");
		let ranges = rune::to_value(vec![(0i64, 2i64), (0i64, 2i64)]).unwrap();
		refuses(
			slice_assign(&a, ranges, &seq(3, &[3, 1])),
			"the ranges [2, 2]",
		);
		// cumsum: the n x n matrix
		refuses(cumsum(&zeros(&[5000]), 0), "5000 x 5000");
		// metadata stays an i64: an expansion past i64::MAX is refused even
		// with no values, while a valid large axis reads back exactly
		let big = zeros(&[0, i64::MAX as usize]);
		assert_eq!(dim(&big, 1).unwrap(), i64::MAX);
		assert_eq!(stride(&big).unwrap(), vec![i64::MAX, 1]);
		refuses(repeat(&big, v(&[1, 2])), "above i64::MAX");
		refuses(pad_with_zeros(&big, 1, 0, 1), "above i64::MAX");
		// unfold's stride times step, past i64::MAX on a zero-value tensor
		let wide = zeros(&[0, 2, 1 << 61]);
		refuses(unfold(&wide, 1, 1, 4), "above i64::MAX");
		// a genuine batched refusal: [5, 3, 2048] along the last axis
		// expands triu2 to 5 x 2048 x 2048
		refuses(cumsum(&zeros(&[5, 3, 2048]), -1), "batches");
		// cat and stack
		refuses(cat(list(&[&a, &seq(6, &[2, 3])]), 0), "differs");
		refuses(stack(list(&[&a, &seq(8, &[2, 4])]), 0), "not [3, 4]");
		refuses(
			cat(rune::to_value(Vec::<Tensor>::new()).unwrap(), 0),
			"0 tensors",
		);
		refuses(broadcast_as(&a, v(&[4, 4])), "doesn't broadcast");
		refuses(permute(&a, v(&[0, 0])), "repeated");
		refuses(dot(&seq(3, &[3]), &seq(4, &[4])), "shapes differ");
		refuses(mv(&a, &seq(3, &[3])), "inner lengths");
		refuses(round_to(&a, 31), "want -30 to 30");
		refuses(elu(&a, f64::NAN), "finite");
		// every receiver survives every refusal
		assert_eq!(bits(&a.0), bits(&seq(12, &[3, 4]).0));
	}

	/// The pure-Rust f64 mean, in row order.
	fn f64_mean(rows: &[Vec<f64>], seg: &[usize], n: usize) -> Vec<Vec<f64>> {
		let w = rows[0].len();
		let mut sum = vec![vec![0.0; w]; n];
		let mut cnt = vec![0.0; n];
		for (r, &k) in rows.iter().zip(seg) {
			for j in 0..w {
				sum[k][j] += r[j];
			}
			cnt[k] += 1.0;
		}
		sum.into_iter()
			.zip(cnt)
			.map(|(s, c)| s.into_iter().map(|x| x / c).collect())
			.collect()
	}

	#[test]
	fn segment_mean_is_candles_own_calls_and_its_numerics_are_stated() {
		// unit-norm rows: the stated well-conditioned case
		let rows: Vec<Vec<f64>> = (0..7)
			.map(|i| {
				let r: Vec<f64> = (0..4).map(|j| ((i * 4 + j) as f64).sin()).collect();
				let n = r.iter().map(|x| x * x).sum::<f64>().sqrt();
				r.into_iter().map(|x| x / n).collect()
			})
			.collect();
		let seg = [0usize, 2, 1, 0, 2, 2, 1];
		let values = tv(rows.concat(), &[7, 4], "f32");
		let segs = ids(seg.iter().map(|&k| k as i64).collect(), "u32");
		let ours = segment_mean(&values, &segs, 3).unwrap();
		// same-call parity, authoritative
		let theirs = {
			let dt = DType::F32;
			let sums = CTensor::zeros((3, 4), dt, &Device::Cpu)
				.unwrap()
				.index_add(&segs.0, &values.0, 0)
				.unwrap();
			let ones = CTensor::ones(7, dt, &Device::Cpu).unwrap();
			let counts = CTensor::zeros(3, dt, &Device::Cpu)
				.unwrap()
				.index_add(&segs.0, &ones, 0)
				.unwrap();
			sums.broadcast_div(&counts.unsqueeze(1).unwrap())
		};
		assert_eq!(bits(&ours.0), bits(&theirs.unwrap()));
		// the f64 cross-check, gated here: |rnx - f64| <= 1e-6 + 1e-6|f64|
		let got = ours
			.0
			.to_dtype(DType::F64)
			.unwrap()
			.to_vec2::<f64>()
			.unwrap();
		let want = f64_mean(&rows, &seg, 3);
		for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
			assert!((g - w).abs() <= 1e-6 + 1e-6 * w.abs(), "{g} against {w}");
		}
		// cancellation, reported (not gated): Candle's f32 sum gives 0, the
		// f64 mean 1/3
		let c = tv(vec![1e8, 1.0, -1e8], &[3, 1], "f32");
		let cm = segment_mean(&c, &ids(vec![0, 0, 0], "u32"), 1).unwrap();
		let cv = cm.0.flatten_all().unwrap().to_vec1::<f32>().unwrap()[0];
		assert_eq!(cv, 0.0);
		assert!(
			(f64_mean(&[vec![1e8], vec![1.0], vec![-1e8]], &[0, 0, 0], 1)[0][0] - 1.0 / 3.0).abs()
				< 1e-12
		);
		// non-finite values follow IEEE
		let nan = tv(vec![f64::NAN, 1.0, 2.0, 3.0], &[2, 2], "f32");
		let nm = segment_mean(&nan, &ids(vec![0, 1], "i64"), 2).unwrap();
		let nv = nm.0.flatten_all().unwrap().to_vec1::<f32>().unwrap();
		assert!(nv[0].is_nan() && nv[2] == 2.0);
		// refusals
		refuses(segment_mean(&values, &segs, 4), "segment 3 has no rows");
		refuses(
			segment_mean(&values, &ids(vec![0, 1, 2, 0, 1, 2, 3], "u32"), 3),
			"index 3 at position 6",
		);
		refuses(
			segment_mean(&values, &ids(vec![0, 1, -2, 0, 1, 2, 0], "i64"), 3),
			"index -2",
		);
		refuses(
			segment_mean(&values, &ids(vec![0, 1], "u32"), 2),
			"2 segment ids for 7 rows",
		);
		refuses(segment_mean(&values, &segs, 0), "want 1 to");
		refuses(segment_mean(&seq(8, &[8]), &segs, 3), "rank 2");
		refuses(
			segment_mean(&ids(vec![1, 2], "i64"), &segs, 3),
			"f32 or f64",
		);
	}
}
