//! Record 0134: Candle's numeric-analysis core, as family-level bindings.
//!
//! 97 core `Tensor` identities (`probes/0134/manifest.tsv`, counted by
//! `count.py` against the pinned 0.11.0 source), 3 candle-nn functions and
//! one derived helper (`topk`). One contract per family:
//!
//! - **shapes:** rank at most 6; every count and stride product checked
//!   before Candle runs, shapes with zeros included; every output, expanded
//!   operand and copy at most 2²⁴ values, inferred before the call;
//! - **dtypes:** f32, f64, i64, u32, u8; no implicit promotion; strict
//!   constructors; `to_dtype` is Candle's own (Rust `as`) conversion;
//! - **IEEE:** tensors may hold NaN and ±inf; only `to_dense` refuses them;
//! - **reductions:** negative axes count from the end; `var` is unbiased
//!   and needs an axis of at least 2; max/min/argmax/argmin and mean refuse
//!   an empty axis;
//! - **ranking:** every input is copied first into fresh offset-zero
//!   storage (pinned `ArgSort::asort` ignores a view's start offset); NaN is
//!   refused; ties are unspecified, as Candle documents;
//! - **ownership:** tensors are immutable and cheap to clone; every
//!   argument is borrowed and reusable after Ok and Err;
//! - **execution:** every Candle call runs on 0129's joined worker.
use crate::{Tensor, worker};
use candle_core::{DType, Device, Tensor as CTensor};
use rnx::rune::{self, ContextError, Module, Value, runtime::Protocol, runtime::Vec as RuneVec};

/// At most 2²⁴ values in any output, expanded operand or copy.
pub const MAX_ELEMS: usize = rnx::interchange::MAX_VALUES;
/// At most 6 dimensions.
pub const MAX_RANK: usize = 6;

const CPU: &Device = &Device::Cpu;

// ---- the family helpers ----

/// A Candle call on the joined worker, its error named by the operation.
fn go<T: Send>(op: &str, f: impl FnOnce() -> candle_core::Result<T> + Send) -> Result<T, String> {
	worker::run(op, f)?.map_err(|e| format!("{op}: {e}"))
}

fn wrap(
	op: &str,
	f: impl FnOnce() -> candle_core::Result<CTensor> + Send,
) -> Result<Tensor, String> {
	go(op, f).map(Tensor)
}

pub(crate) fn dtype_from(s: &str, op: &str) -> Result<DType, String> {
	match s {
		"f32" => Ok(DType::F32),
		"f64" => Ok(DType::F64),
		"i64" => Ok(DType::I64),
		"u32" => Ok(DType::U32),
		"u8" => Ok(DType::U8),
		other => Err(format!(
			"{op}: dtype {other:?}, want \"f32\", \"f64\", \"i64\", \"u32\" or \"u8\""
		)),
	}
}

pub(crate) fn dtype_name(d: DType) -> String {
	format!("{d:?}").to_lowercase()
}

fn is_float(d: DType) -> bool {
	matches!(d, DType::F32 | DType::F64)
}

fn float_only(op: &str, t: &CTensor) -> Result<(), String> {
	if is_float(t.dtype()) {
		Ok(())
	} else {
		Err(format!(
			"{op}: needs f32 or f64, found {}; convert with to_dtype first",
			dtype_name(t.dtype())
		))
	}
}

fn same_dtype(op: &str, a: &CTensor, b: &CTensor) -> Result<(), String> {
	if a.dtype() != b.dtype() {
		return Err(format!(
			"{op}: dtypes differ ({} and {}); there is no implicit promotion",
			dtype_name(a.dtype()),
			dtype_name(b.dtype())
		));
	}
	Ok(())
}

fn same(op: &str, a: &CTensor, b: &CTensor) -> Result<(), String> {
	same_dtype(op, a, b)?;
	if a.dims() != b.dims() {
		return Err(format!(
			"{op}: shapes differ ({:?} and {:?}); use the broadcast_ form to broadcast",
			a.dims(),
			b.dims()
		));
	}
	Ok(())
}

/// The element count of `dims`, checked, and within the cap; and every
/// step of Candle's own contiguous-stride product (a running product from
/// the last dimension through the first, `shape.rs` `stride_contiguous`),
/// checked too: a zero dimension makes the count 0 but can't hide an
/// overflowing stride.
fn capped(op: &str, dims: &[usize], what: &str) -> Result<usize, String> {
	if dims.len() > MAX_RANK {
		return Err(format!(
			"{op}: {what} has rank {}, at most {MAX_RANK}",
			dims.len()
		));
	}
	let mut stride = 1usize;
	for &d in dims.iter().rev() {
		stride = stride
			.checked_mul(d)
			.ok_or_else(|| format!("{op}: {what} {dims:?} overflows its stride products"))?;
	}
	match dims.iter().try_fold(1usize, |a, &d| a.checked_mul(d)) {
		Some(n) if n <= MAX_ELEMS => Ok(n),
		_ => Err(format!("{op}: {what} {dims:?} exceeds {MAX_ELEMS} values")),
	}
}

/// A shape argument: a vector of non-negative integers, rank at most 6,
/// its count checked.
fn shape_arg(op: &str, v: &Value) -> Result<Vec<usize>, String> {
	let not = || format!("{op}: a shape is a vector of non-negative integers");
	let values = v.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.len() > MAX_RANK {
		return Err(format!("{op}: rank {}, at most {MAX_RANK}", values.len()));
	}
	let mut dims = Vec::with_capacity(values.len());
	for x in values.iter() {
		let d = rune::from_value::<i64>(x.clone()).map_err(|_| not())?;
		dims.push(usize::try_from(d).map_err(|_| not())?);
	}
	capped(op, &dims, "the shape")?;
	Ok(dims)
}

/// An axis for a tensor of `rank`: negative counts from the end.
fn axis(op: &str, rank: usize, d: i64) -> Result<usize, String> {
	let r = rank as i64;
	let a = if d < 0 { d + r } else { d };
	if a < 0 || a >= r {
		return Err(format!("{op}: axis {d} is out of range for rank {rank}"));
	}
	Ok(a as usize)
}

/// Axes: an integer or a vector of distinct integers.
fn axes(op: &str, rank: usize, v: &Value) -> Result<Vec<usize>, String> {
	if let Ok(d) = rune::from_value::<i64>(v.clone()) {
		return Ok(vec![axis(op, rank, d)?]);
	}
	let values = v
		.borrow_ref::<RuneVec>()
		.map_err(|_| format!("{op}: axes are an integer or a vector of integers"))?;
	let mut out = Vec::with_capacity(values.len());
	for x in values.iter() {
		let d = rune::from_value::<i64>(x.clone())
			.map_err(|_| format!("{op}: axes are an integer or a vector of integers"))?;
		let a = axis(op, rank, d)?;
		if out.contains(&a) {
			return Err(format!("{op}: axis {d} is repeated"));
		}
		out.push(a);
	}
	if out.is_empty() {
		return Err(format!("{op}: no axes given"));
	}
	Ok(out)
}

/// A Rune number: an integer, or a float.
#[derive(Clone, Copy)]
enum Num {
	I(i64),
	F(f64),
}

fn num(op: &str, v: &Value) -> Result<Num, String> {
	if let Ok(i) = rune::from_value::<i64>(v.clone()) {
		return Ok(Num::I(i));
	}
	rune::from_value::<f64>(v.clone())
		.map(Num::F)
		.map_err(|_| format!("{op}: expected a number"))
}

/// 2⁶³ as an f64: the first float out of i64's range.
const TWO_63: f64 = 9_223_372_036_854_775_808.0;

fn integral(op: &str, x: Num, lo: f64, hi_exclusive: f64, name: &str) -> Result<i64, String> {
	match x {
		Num::I(i) if (i as f64) >= lo && (i as f64) < hi_exclusive => Ok(i),
		Num::F(f) if f.fract() == 0.0 && f >= lo && f < hi_exclusive => Ok(f as i64),
		_ => Err(format!(
			"{op}: {} is not an integer in the {name} range",
			match x {
				Num::I(i) => i.to_string(),
				Num::F(f) => format!("{f:?}"),
			}
		)),
	}
}

fn as_f64(x: Num) -> f64 {
	match x {
		Num::I(i) => i as f64,
		Num::F(f) => f,
	}
}

fn as_f32(op: &str, x: Num) -> Result<f32, String> {
	let f = as_f64(x);
	if f.is_finite() && f.abs() > f32::MAX as f64 {
		return Err(format!("{op}: {f:?} is outside the f32 range"));
	}
	Ok(f as f32)
}

fn as_i64(op: &str, x: Num) -> Result<i64, String> {
	match x {
		Num::I(i) => Ok(i),
		Num::F(_) => integral(op, x, -TWO_63, TWO_63, "i64"),
	}
}

fn as_u32(op: &str, x: Num) -> Result<u32, String> {
	integral(op, x, 0.0, 4_294_967_296.0, "u32").map(|v| v as u32)
}

fn as_u8(op: &str, x: Num) -> Result<u8, String> {
	integral(op, x, 0.0, 256.0, "u8").map(|v| v as u8)
}

/// A tensor or a number, for comparisons and clamp. A number keeps its
/// Rune representation until it meets the receiver's dtype.
enum Operand {
	T(CTensor),
	N(Num),
}

fn operand(op: &str, v: &Value) -> Result<Operand, String> {
	if let Ok(t) = v.borrow_ref::<Tensor>() {
		return Ok(Operand::T(t.0.clone()));
	}
	num(op, v).map(Operand::N)
}

/// A number as a rank-0 tensor of `dtype`, by the constructors' strict
/// conversion (an i64 above 2⁵³ stays exact; a u8 scalar of 300 is refused),
/// broadcast to `dims` as a view.
fn scalar_like(op: &str, x: Num, dtype: DType, dims: &[usize]) -> Result<CTensor, String> {
	let e = |e: candle_core::Error| format!("{op}: {e}");
	let t = match dtype {
		DType::F32 => CTensor::new(as_f32(op, x)?, CPU),
		DType::F64 => CTensor::new(as_f64(x), CPU),
		DType::I64 => CTensor::new(as_i64(op, x)?, CPU),
		DType::U32 => CTensor::new(as_u32(op, x)?, CPU),
		DType::U8 => CTensor::new(as_u8(op, x)?, CPU),
		other => return Err(format!("{op}: {other:?} doesn't take a number")),
	}
	.map_err(e)?;
	t.broadcast_as(dims).map_err(e)
}

/// A comparison or clamp operand as a tensor of the receiver's shape.
fn operand_like(op: &str, this: &CTensor, v: &Value) -> Result<CTensor, String> {
	match operand(op, v)? {
		Operand::T(r) => {
			same(op, this, &r)?;
			Ok(r)
		}
		Operand::N(x) => scalar_like(op, x, this.dtype(), this.dims()),
	}
}

// ---- A: construction and readback ----

fn from_vec(values: Value, shape: Value, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::from_vec";
	let dims = shape_arg(op, &shape)?;
	let n = capped(op, &dims, "the shape")?;
	let d = dtype_from(dtype, op)?;
	let values = values
		.borrow_ref::<RuneVec>()
		.map_err(|_| format!("{op}: values must be a vector of numbers"))?;
	if values.len() != n {
		return Err(format!(
			"{op}: {} values for shape {dims:?} ({n} values)",
			values.len()
		));
	}
	let nums: Vec<Num> = values
		.iter()
		.map(|v| num(op, v))
		.collect::<Result<_, _>>()?;
	macro_rules! build {
		($conv:expr) => {{
			let data = nums
				.iter()
				.map(|x| $conv(op, *x))
				.collect::<Result<Vec<_>, String>>()?;
			wrap(op, move || CTensor::from_vec(data, dims, CPU))
		}};
	}
	match d {
		DType::F32 => build!(as_f32),
		DType::F64 => build!(|_: &str, x| Ok::<f64, String>(as_f64(x))),
		DType::I64 => build!(as_i64),
		DType::U32 => build!(as_u32),
		DType::U8 => build!(as_u8),
		_ => unreachable!(),
	}
}

fn zeros(shape: Value, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::zeros";
	let dims = shape_arg(op, &shape)?;
	let d = dtype_from(dtype, op)?;
	wrap(op, move || CTensor::zeros(dims, d, CPU))
}

fn ones(shape: Value, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::ones";
	let dims = shape_arg(op, &shape)?;
	let d = dtype_from(dtype, op)?;
	wrap(op, move || CTensor::ones(dims, d, CPU))
}

fn full(value: Value, shape: Value, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::full";
	let dims = shape_arg(op, &shape)?;
	let x = num(op, &value)?;
	match dtype_from(dtype, op)? {
		DType::F32 => {
			let v = as_f32(op, x)?;
			wrap(op, move || CTensor::full(v, dims, CPU))
		}
		DType::F64 => {
			let v = as_f64(x);
			wrap(op, move || CTensor::full(v, dims, CPU))
		}
		DType::I64 => {
			let v = as_i64(op, x)?;
			wrap(op, move || CTensor::full(v, dims, CPU))
		}
		DType::U32 => {
			let v = as_u32(op, x)?;
			wrap(op, move || CTensor::full(v, dims, CPU))
		}
		DType::U8 => {
			let v = as_u8(op, x)?;
			wrap(op, move || CTensor::full(v, dims, CPU))
		}
		_ => unreachable!(),
	}
}

/// Candle's own `arange_step` loop, proven first in the selected dtype:
/// `while current < end { push; current += step }` (or `>` for a negative
/// step), with the terminal increment included. Refused: a zero or
/// non-finite step or endpoint, an increment that overflows or fails to
/// change the value, and more than 2²⁴ steps.
trait Step: Copy + PartialOrd + std::fmt::Debug {
	fn zero() -> Self;
	/// The next value, or `None` if the increment overflows or doesn't move.
	fn next(self, step: Self) -> Option<Self>;
	fn finite(self) -> bool {
		true
	}
}
macro_rules! int_step {
	($($t:ty),*) => {$(
		impl Step for $t {
			fn zero() -> Self { 0 }
			fn next(self, step: Self) -> Option<Self> { self.checked_add(step) }
		}
	)*};
}
int_step!(i64);
impl Step for u32 {
	fn zero() -> Self {
		0
	}
	fn next(self, step: Self) -> Option<Self> {
		self.checked_add(step)
	}
}
impl Step for u8 {
	fn zero() -> Self {
		0
	}
	fn next(self, step: Self) -> Option<Self> {
		self.checked_add(step)
	}
}
macro_rules! float_step {
	($($t:ty),*) => {$(
		impl Step for $t {
			fn zero() -> Self { 0.0 }
			fn next(self, step: Self) -> Option<Self> {
				let n = self + step;
				if n == self || !n.is_finite() { None } else { Some(n) }
			}
			fn finite(self) -> bool { self.is_finite() }
		}
	)*};
}
float_step!(f32, f64);

fn prove_arange<D: Step>(op: &str, start: D, end: D, step: D) -> Result<usize, String> {
	if !(start.finite() && end.finite() && step.finite()) {
		return Err(format!("{op}: start, end and step must be finite"));
	}
	if step == D::zero() {
		return Err(format!("{op}: the step is zero"));
	}
	let up = step > D::zero();
	if (up && start > end) || (!up && start < end) {
		return Err(format!(
			"{op}: a step of {step:?} never reaches {end:?} from {start:?}"
		));
	}
	let mut current = start;
	let mut count = 0usize;
	while if up { current < end } else { current > end } {
		count += 1;
		if count > MAX_ELEMS {
			return Err(format!("{op}: more than {MAX_ELEMS} values"));
		}
		current = current.next(step).ok_or_else(|| {
			format!("{op}: the increment after {current:?} overflows or doesn't change the value")
		})?;
	}
	Ok(count)
}

fn arange_step(start: Value, end: Value, step: Value, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::arange_step";
	let (s, e, k) = (num(op, &start)?, num(op, &end)?, num(op, &step)?);
	macro_rules! run {
		($conv:expr) => {{
			let (s, e, k) = ($conv(op, s)?, $conv(op, e)?, $conv(op, k)?);
			prove_arange(op, s, e, k)?;
			wrap(op, move || CTensor::arange_step(s, e, k, CPU))
		}};
	}
	match dtype_from(dtype, op)? {
		DType::F32 => run!(as_f32),
		DType::F64 => run!(|_: &str, x| Ok::<f64, String>(as_f64(x))),
		DType::I64 => run!(as_i64),
		DType::U32 => run!(as_u32),
		DType::U8 => run!(as_u8),
		_ => unreachable!(),
	}
}

fn arange(start: Value, end: Value, dtype: &str) -> Result<Tensor, String> {
	arange_step(
		start,
		end,
		rune::to_value(1i64).map_err(|e| e.to_string())?,
		dtype,
	)
	.map_err(|e| e.replacen("Tensor::arange_step", "Tensor::arange", 1))
}

fn eye(n: i64, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::eye";
	let n = usize::try_from(n).map_err(|_| format!("{op}: n must be non-negative"))?;
	// n² for the result, and n² again for Candle's u8 mask before to_dtype
	capped(op, &[n, n], "the identity")?;
	let d = dtype_from(dtype, op)?;
	wrap(op, move || CTensor::eye(n, d, CPU))
}

fn zeros_like(this: &Tensor) -> Result<Tensor, String> {
	wrap("Tensor::zeros_like", || this.0.zeros_like())
}

fn ones_like(this: &Tensor) -> Result<Tensor, String> {
	wrap("Tensor::ones_like", || this.0.ones_like())
}

/// Flattened readback into Rune numbers: f64 for floats, i64 for integers.
fn readback(op: &str, t: &CTensor) -> Result<Value, String> {
	capped(op, t.dims(), "the tensor")?;
	let flat = t.clone();
	let v = match t.dtype() {
		DType::F32 => rune::to_value(go(op, move || {
			flat.flatten_all()?.to_dtype(DType::F64)?.to_vec1::<f64>()
		})?),
		DType::F64 => rune::to_value(go(op, move || flat.flatten_all()?.to_vec1::<f64>())?),
		DType::I64 => rune::to_value(go(op, move || flat.flatten_all()?.to_vec1::<i64>())?),
		DType::U32 => rune::to_value(
			go(op, move || flat.flatten_all()?.to_vec1::<u32>())?
				.into_iter()
				.map(i64::from)
				.collect::<Vec<_>>(),
		),
		DType::U8 => rune::to_value(
			go(op, move || flat.flatten_all()?.to_vec1::<u8>())?
				.into_iter()
				.map(i64::from)
				.collect::<Vec<_>>(),
		),
		other => return Err(format!("{op}: {other:?} can't be read back")),
	};
	v.map_err(|e| format!("{op}: {e}"))
}

fn to_vec(this: &Tensor) -> Result<Value, String> {
	readback("Tensor::to_vec", &this.0)
}

fn to_vec1(this: &Tensor) -> Result<Value, String> {
	let op = "Tensor::to_vec1";
	if this.0.rank() != 1 {
		return Err(format!("{op}: needs rank 1, found {}", this.0.rank()));
	}
	readback(op, &this.0)
}

fn to_vec2(this: &Tensor) -> Result<Value, String> {
	let op = "Tensor::to_vec2";
	let [rows, cols] = *this.0.dims() else {
		return Err(format!("{op}: needs rank 2, found {}", this.0.rank()));
	};
	// the outer rows count too: [N, 0] has no elements but N vectors
	capped(op, &[rows], "the rows")?;
	capped(op, &[rows, cols], "the tensor")?;
	let flat = readback(op, &this.0)?;
	let flat = flat
		.borrow_ref::<RuneVec>()
		.map_err(|e| format!("{op}: {e}"))?;
	let mut out = Vec::with_capacity(rows);
	for r in 0..rows {
		let row: Vec<Value> = flat[r * cols..(r + 1) * cols].to_vec();
		out.push(rune::to_value(row).map_err(|e| format!("{op}: {e}"))?);
	}
	rune::to_value(out).map_err(|e| format!("{op}: {e}"))
}

fn to_scalar(this: &Tensor) -> Result<Value, String> {
	let op = "Tensor::to_scalar";
	if this.0.rank() != 0 {
		return Err(format!("{op}: needs rank 0, found {}", this.0.rank()));
	}
	let v = readback(op, &this.0)?;
	let v = v
		.borrow_ref::<RuneVec>()
		.map_err(|e| format!("{op}: {e}"))?;
	Ok(v[0].clone())
}

fn to_dtype(this: &Tensor, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::to_dtype";
	let d = dtype_from(dtype, op)?;
	capped(op, this.0.dims(), "the tensor")?;
	wrap(op, || this.0.to_dtype(d))
}

fn dims(this: &Tensor) -> Vec<i64> {
	this.0.dims().iter().map(|d| *d as i64).collect()
}

fn rank(this: &Tensor) -> i64 {
	this.0.rank() as i64
}

fn elem_count(this: &Tensor) -> i64 {
	this.0.elem_count() as i64
}

fn contiguous(this: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::contiguous";
	capped(op, this.0.dims(), "the copy")?;
	wrap(op, || this.0.contiguous())
}

fn is_contiguous(this: &Tensor) -> bool {
	this.0.is_contiguous()
}

fn flatten_all(this: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::flatten_all";
	capped(op, this.0.dims(), "the copy")?;
	wrap(op, || this.0.flatten_all())
}

// ---- C: arithmetic and broadcasting ----

macro_rules! binary {
	($($name:ident),*) => {$(
		fn $name(this: &Tensor, rhs: &Tensor) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			same(op, &this.0, &rhs.0)?;
			wrap(op, || this.0.$name(&rhs.0))
		}
	)*};
}
binary!(add, sub, mul, div, maximum, minimum);

/// The broadcast shape of two operands, checked and capped.
fn broadcast_shape(op: &str, a: &[usize], b: &[usize]) -> Result<Vec<usize>, String> {
	let rank = a.len().max(b.len());
	if rank > MAX_RANK {
		return Err(format!("{op}: rank {rank}, at most {MAX_RANK}"));
	}
	let mut out = vec![0; rank];
	for i in 0..rank {
		let x = if i + a.len() >= rank {
			a[i + a.len() - rank]
		} else {
			1
		};
		let y = if i + b.len() >= rank {
			b[i + b.len() - rank]
		} else {
			1
		};
		out[i] = match (x, y) {
			(x, y) if x == y => x,
			(1, y) => y,
			(x, 1) => x,
			_ => {
				return Err(format!("{op}: shapes {a:?} and {b:?} don't broadcast"));
			}
		};
	}
	capped(op, &out, "the broadcast result")?;
	Ok(out)
}

macro_rules! broadcast {
	($($name:ident),*) => {$(
		fn $name(this: &Tensor, rhs: &Tensor) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			same_dtype(op, &this.0, &rhs.0)?;
			broadcast_shape(op, this.0.dims(), rhs.0.dims())?;
			wrap(op, || this.0.$name(&rhs.0))
		}
	)*};
}
broadcast!(
	broadcast_add,
	broadcast_sub,
	broadcast_mul,
	broadcast_div,
	broadcast_maximum,
	broadcast_minimum,
	broadcast_eq,
	broadcast_ne,
	broadcast_lt,
	broadcast_le,
	broadcast_gt,
	broadcast_ge
);

macro_rules! compare {
	($($name:ident),*) => {$(
		fn $name(this: &Tensor, rhs: Value) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			let r = operand_like(op, &this.0, &rhs)?;
			wrap(op, move || this.0.$name(&r))
		}
	)*};
}
compare!(eq, ne, lt, le, gt, ge);

fn affine(this: &Tensor, mul: f64, add: f64) -> Result<Tensor, String> {
	let op = "Tensor::affine";
	float_only(op, &this.0)?;
	wrap(op, || this.0.affine(mul, add))
}

fn powf(this: &Tensor, e: f64) -> Result<Tensor, String> {
	let op = "Tensor::powf";
	float_only(op, &this.0)?;
	wrap(op, || this.0.powf(e))
}

fn clamp(this: &Tensor, min: Value, max: Value) -> Result<Tensor, String> {
	let op = "Tensor::clamp";
	let lo = operand_like(op, &this.0, &min)?;
	let hi = operand_like(op, &this.0, &max)?;
	wrap(op, move || this.0.clamp(&lo, &hi))
}

fn where_cond(this: &Tensor, on_true: &Tensor, on_false: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::where_cond";
	if this.0.dtype() != DType::U8 {
		return Err(format!(
			"{op}: the mask must be u8, found {}",
			dtype_name(this.0.dtype())
		));
	}
	same(op, &on_true.0, &on_false.0)?;
	if this.0.dims() != on_true.0.dims() {
		return Err(format!(
			"{op}: the mask's shape {:?} differs from the branches' {:?}",
			this.0.dims(),
			on_true.0.dims()
		));
	}
	wrap(op, || this.0.where_cond(&on_true.0, &on_false.0))
}

macro_rules! unary {
	($($name:ident),*) => {$(
		fn $name(this: &Tensor) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			float_only(op, &this.0)?;
			wrap(op, || this.0.$name())
		}
	)*};
}
unary!(
	exp, log, sqrt, sqr, abs, neg, recip, tanh, relu, gelu, gelu_erf, silu, sin, cos, floor, ceil,
	round, sign, erf
);

/// `t + x`, `t - x`, `t * x`, `t / x`: a tensor with a tensor of the same
/// shape and dtype, or with a number (Candle's operator impls, `affine`).
macro_rules! operator {
	($($name:ident, $method:ident, $sym:tt);*) => {$(
		fn $name(this: &Tensor, rhs: Value) -> Result<Tensor, String> {
			let op = concat!("Tensor ", stringify!($sym));
			if let Ok(r) = rhs.borrow_ref::<Tensor>() {
				same(op, &this.0, &r.0)?;
				let r = r.0.clone();
				return wrap(op, move || this.0.$method(&r));
			}
			let x = as_f64(num(op, &rhs)?);
			float_only(op, &this.0)?;
			wrap(op, move || &this.0 $sym x)
		}
	)*};
}
operator!(op_add, add, +; op_sub, sub, -; op_mul, mul, *; op_div, div, /);

// ---- D: reductions and normalization ----

/// The reduced shape (axes removed, or kept as 1), checked.
fn reduced(op: &str, t: &CTensor, ax: &[usize], keep: bool) -> Result<(), String> {
	let out: Vec<usize> = t
		.dims()
		.iter()
		.enumerate()
		.filter_map(|(i, d)| match (ax.contains(&i), keep) {
			(true, true) => Some(1),
			(true, false) => None,
			(false, _) => Some(*d),
		})
		.collect();
	capped(op, &out, "the result")?;
	Ok(())
}

fn nonempty(op: &str, t: &CTensor, ax: &[usize]) -> Result<(), String> {
	for &a in ax {
		if t.dims()[a] == 0 {
			return Err(format!("{op}: axis {a} is empty"));
		}
	}
	Ok(())
}

macro_rules! sum_like {
	($($name:ident, $keep:expr, $needs_items:expr);*) => {$(
		fn $name(this: &Tensor, dims: Value) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			let ax = axes(op, this.0.rank(), &dims)?;
			if $needs_items { nonempty(op, &this.0, &ax)?; }
			reduced(op, &this.0, &ax, $keep)?;
			wrap(op, move || this.0.$name(ax))
		}
	)*};
}
sum_like!(sum, false, false; sum_keepdim, true, false; mean, false, true; mean_keepdim, true, true);

macro_rules! extreme {
	($($name:ident, $keep:expr);*) => {$(
		fn $name(this: &Tensor, dim: i64) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			let a = axis(op, this.0.rank(), dim)?;
			nonempty(op, &this.0, &[a])?;
			reduced(op, &this.0, &[a], $keep)?;
			wrap(op, move || this.0.$name(a))
		}
	)*};
}
extreme!(
	max, false; max_keepdim, true; min, false; min_keepdim, true; argmax, false;
	argmax_keepdim, true; argmin, false; argmin_keepdim, true
);

macro_rules! whole {
	($($name:ident, $needs_items:expr);*) => {$(
		fn $name(this: &Tensor) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			if $needs_items && this.0.elem_count() == 0 {
				return Err(format!("{op}: the tensor is empty"));
			}
			wrap(op, || this.0.$name())
		}
	)*};
}
whole!(sum_all, false; mean_all, true; max_all, true; min_all, true);

macro_rules! variance {
	($($name:ident, $keep:expr);*) => {$(
		fn $name(this: &Tensor, dim: i64) -> Result<Tensor, String> {
			let op = concat!("Tensor::", stringify!($name));
			float_only(op, &this.0)?;
			let a = axis(op, this.0.rank(), dim)?;
			if this.0.dims()[a] < 2 {
				return Err(format!(
					"{op}: the unbiased variance needs an axis of at least 2, axis {a} has {}",
					this.0.dims()[a]
				));
			}
			// the (x - mean)² temporary is the input's size
			capped(op, this.0.dims(), "the temporary")?;
			reduced(op, &this.0, &[a], $keep)?;
			wrap(op, move || this.0.$name(a))
		}
	)*};
}
variance!(var, false; var_keepdim, true);

macro_rules! nn_softmax {
	($($name:ident, $call:path);*) => {$(
		fn $name(t: &Tensor, dim: i64) -> Result<Tensor, String> {
			let op = concat!("candle::", stringify!($name));
			float_only(op, &t.0)?;
			let a = axis(op, t.0.rank(), dim)?;
			capped(op, t.0.dims(), "the input")?;
			wrap(op, move || $call(&t.0, a))
		}
	)*};
}
nn_softmax!(softmax, candle_nn::ops::softmax; log_softmax, candle_nn::ops::log_softmax);

fn softmax_last_dim(t: &Tensor) -> Result<Tensor, String> {
	let op = "candle::softmax_last_dim";
	float_only(op, &t.0)?;
	if t.0.rank() == 0 {
		return Err(format!("{op}: needs rank 1 or more"));
	}
	wrap(op, || candle_nn::ops::softmax_last_dim(&t.0))
}

// ---- E: matmul ----

fn matmul_check(op: &str, a: &CTensor, b: &CTensor, broadcast: bool) -> Result<(), String> {
	float_only(op, a)?;
	same_dtype(op, a, b)?;
	let (ad, bd) = (a.dims(), b.dims());
	if ad.len() < 2 || bd.len() < 2 {
		return Err(format!(
			"{op}: both operands need rank 2 or more ({ad:?} and {bd:?})"
		));
	}
	let (m, k) = (ad[ad.len() - 2], ad[ad.len() - 1]);
	let (k2, n) = (bd[bd.len() - 2], bd[bd.len() - 1]);
	if k != k2 {
		return Err(format!("{op}: inner dimensions differ ({ad:?} and {bd:?})"));
	}
	let (abatch, bbatch) = (&ad[..ad.len() - 2], &bd[..bd.len() - 2]);
	let batch = if broadcast {
		broadcast_shape(op, abatch, bbatch)?
	} else {
		if abatch != bbatch {
			return Err(format!(
				"{op}: batch dimensions differ ({ad:?} and {bd:?}); use broadcast_matmul"
			));
		}
		abatch.to_vec()
	};
	let with = |x: usize, y: usize| {
		let mut v = batch.clone();
		v.extend([x, y]);
		v
	};
	capped(op, &with(m, n), "the result")?;
	if broadcast {
		// Candle concretizes both broadcasted operands (`contiguous`)
		capped(op, &with(m, k), "the expanded left operand")?;
		capped(op, &with(k, n), "the expanded right operand")?;
	}
	Ok(())
}

fn matmul(this: &Tensor, rhs: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::matmul";
	matmul_check(op, &this.0, &rhs.0, false)?;
	wrap(op, || this.0.matmul(&rhs.0))
}

fn broadcast_matmul(this: &Tensor, rhs: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::broadcast_matmul";
	matmul_check(op, &this.0, &rhs.0, true)?;
	wrap(op, || this.0.broadcast_matmul(&rhs.0))
}

// ---- F: ranking ----

/// A ranking input: rank at least 1, a non-empty last axis, no NaN, and a
/// fresh offset-zero copy (pinned `ArgSort::asort` ignores the layout's
/// start offset, so even a "contiguous" sliced view would sort the wrong
/// values).
fn rankable(op: &str, t: &CTensor) -> Result<CTensor, String> {
	if t.rank() == 0 {
		return Err(format!("{op}: needs rank 1 or more"));
	}
	if t.dims()[t.rank() - 1] == 0 {
		return Err(format!("{op}: the last axis is empty"));
	}
	capped(op, t.dims(), "the copy")?;
	let t = t.clone();
	go(op, move || {
		let fresh = t.force_contiguous()?;
		let nan = if is_float(fresh.dtype()) {
			fresh
				.ne(&fresh)?
				.to_dtype(DType::U32)?
				.sum_all()?
				.to_scalar::<u32>()?
		} else {
			0
		};
		Ok((fresh, nan))
	})
	.and_then(|(fresh, nan)| {
		if nan > 0 {
			Err(format!(
				"{op}: the input holds {nan} NaN values, whose order Candle leaves unspecified"
			))
		} else {
			Ok(fresh)
		}
	})
}

fn arg_sort_last_dim(this: &Tensor, asc: bool) -> Result<Tensor, String> {
	let op = "Tensor::arg_sort_last_dim";
	let fresh = rankable(op, &this.0)?;
	wrap(op, move || fresh.arg_sort_last_dim(asc))
}

fn sort_last_dim(this: &Tensor, asc: bool) -> Result<(Tensor, Tensor), String> {
	let op = "Tensor::sort_last_dim";
	let fresh = rankable(op, &this.0)?;
	go(op, move || fresh.sort_last_dim(asc)).map(|(v, i)| (Tensor(v), Tensor(i)))
}

/// Derived: `sort_last_dim`, then `narrow` to the first k along the last
/// axis; (values, indices).
fn topk(this: &Tensor, k: i64, asc: bool) -> Result<(Tensor, Tensor), String> {
	let op = "Tensor::topk";
	// the rank, the last axis and k first, before any copy or scan
	if this.0.rank() == 0 {
		return Err(format!("{op}: needs rank 1 or more"));
	}
	let last = this.0.rank() - 1;
	let len = this.0.dims()[last];
	if len == 0 {
		return Err(format!("{op}: the last axis is empty"));
	}
	let k = usize::try_from(k)
		.ok()
		.filter(|k| (1..=len).contains(k))
		.ok_or_else(|| format!("{op}: k = {k}, want 1 to {len}"))?;
	let fresh = rankable(op, &this.0)?;
	go(op, move || {
		let (v, i) = fresh.sort_last_dim(asc)?;
		Ok((v.narrow(last, 0, k)?, i.narrow(last, 0, k)?))
	})
	.map(|(v, i)| (Tensor(v), Tensor(i)))
}

// ---- B: minimal shape operations ----

fn reshape(this: &Tensor, shape: Value) -> Result<Tensor, String> {
	let op = "Tensor::reshape";
	let dims = shape_arg(op, &shape)?;
	let n = capped(op, &dims, "the shape")?;
	if n != this.0.elem_count() {
		return Err(format!(
			"{op}: {:?} holds {} values, not {n}",
			this.0.dims(),
			this.0.elem_count()
		));
	}
	wrap(op, move || this.0.reshape(dims))
}

/// A transposed shape's own stride products: a permutation of dims with a
/// zero can overflow where the original didn't.
fn swapped(op: &str, dims: &[usize], a: usize, b: usize) -> Result<(), String> {
	let mut d = dims.to_vec();
	d.swap(a, b);
	capped(op, &d, "the transposed shape").map(|_| ())
}

fn t(this: &Tensor) -> Result<Tensor, String> {
	let op = "Tensor::t";
	let r = this.0.rank();
	if r < 2 {
		return Err(format!("{op}: needs rank 2 or more, found {r}"));
	}
	swapped(op, this.0.dims(), r - 2, r - 1)?;
	wrap(op, || this.0.t())
}

fn transpose(this: &Tensor, d1: i64, d2: i64) -> Result<Tensor, String> {
	let op = "Tensor::transpose";
	let (a, b) = (axis(op, this.0.rank(), d1)?, axis(op, this.0.rank(), d2)?);
	swapped(op, this.0.dims(), a, b)?;
	wrap(op, move || this.0.transpose(a, b))
}

fn squeeze(this: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::squeeze";
	let a = axis(op, this.0.rank(), dim)?;
	wrap(op, move || this.0.squeeze(a))
}

fn unsqueeze(this: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::unsqueeze";
	// a new axis can go at any position 0..=rank; negative counts from the end
	let a = axis(op, this.0.rank() + 1, dim)?;
	let mut d = this.0.dims().to_vec();
	d.insert(a, 1);
	capped(op, &d, "the new shape")?;
	wrap(op, move || this.0.unsqueeze(a))
}

fn narrow(this: &Tensor, dim: i64, start: i64, len: i64) -> Result<Tensor, String> {
	let op = "Tensor::narrow";
	let a = axis(op, this.0.rank(), dim)?;
	let size = this.0.dims()[a];
	let (s, l) = (usize::try_from(start), usize::try_from(len));
	let (Ok(s), Ok(l)) = (s, l) else {
		return Err(format!("{op}: start and len must be non-negative"));
	};
	if s.checked_add(l).is_none_or(|e| e > size) {
		return Err(format!(
			"{op}: {start} + {len} is beyond axis {a} of length {size}"
		));
	}
	wrap(op, move || this.0.narrow(a, s, l))
}

fn flatten(this: &Tensor, start: i64, end: i64) -> Result<Tensor, String> {
	let op = "Tensor::flatten";
	let (a, b) = (
		axis(op, this.0.rank(), start)?,
		axis(op, this.0.rank(), end)?,
	);
	if a > b {
		return Err(format!("{op}: start axis {a} is after end axis {b}"));
	}
	let dims = this.0.dims();
	// the merged segment's own product, checked: zeros at both ends of the
	// shape can make the whole count and every stride valid while the
	// middle product alone overflows (Candle's flatten multiplies unchecked)
	let merged = dims[a..=b]
		.iter()
		.try_fold(1usize, |p, &x| p.checked_mul(x))
		.ok_or_else(|| format!("{op}: axes {a} to {b} of {dims:?} overflow when merged"))?;
	let mut d = dims[..a].to_vec();
	d.push(merged);
	d.extend_from_slice(&dims[b + 1..]);
	capped(op, &d, "the flattened shape")?;
	wrap(op, move || this.0.flatten(a, b))
}

// ---- registration ----

pub(crate) fn build(m: &mut Module) -> Result<Vec<(String, &'static str)>, ContextError> {
	macro_rules! assoc {
		($($name:ident),*) => {$( m.associated_function(stringify!($name), $name)?; )*};
	}
	m.function("from_vec", from_vec)
		.build_associated::<Tensor>()?;
	m.function("zeros", zeros).build_associated::<Tensor>()?;
	m.function("ones", ones).build_associated::<Tensor>()?;
	m.function("full", full).build_associated::<Tensor>()?;
	m.function("arange", arange).build_associated::<Tensor>()?;
	m.function("arange_step", arange_step)
		.build_associated::<Tensor>()?;
	m.function("eye", eye).build_associated::<Tensor>()?;
	assoc!(
		zeros_like,
		ones_like,
		to_vec,
		to_vec1,
		to_vec2,
		to_scalar,
		to_dtype,
		dims,
		rank,
		elem_count,
		contiguous,
		is_contiguous,
		flatten_all,
		add,
		sub,
		mul,
		div,
		maximum,
		minimum,
		broadcast_add,
		broadcast_sub,
		broadcast_mul,
		broadcast_div,
		broadcast_maximum,
		broadcast_minimum,
		broadcast_eq,
		broadcast_ne,
		broadcast_lt,
		broadcast_le,
		broadcast_gt,
		broadcast_ge,
		eq,
		ne,
		lt,
		le,
		gt,
		ge,
		affine,
		powf,
		clamp,
		where_cond,
		exp,
		log,
		sqrt,
		sqr,
		abs,
		neg,
		recip,
		tanh,
		relu,
		gelu,
		gelu_erf,
		silu,
		sin,
		cos,
		floor,
		ceil,
		round,
		sign,
		erf,
		sum,
		sum_keepdim,
		mean,
		mean_keepdim,
		max,
		max_keepdim,
		min,
		min_keepdim,
		argmax,
		argmax_keepdim,
		argmin,
		argmin_keepdim,
		sum_all,
		mean_all,
		max_all,
		min_all,
		var,
		var_keepdim,
		matmul,
		broadcast_matmul,
		arg_sort_last_dim,
		sort_last_dim,
		topk,
		reshape,
		t,
		transpose,
		squeeze,
		unsqueeze,
		narrow,
		flatten
	);
	m.associated_function(&Protocol::ADD, op_add)?;
	m.associated_function(&Protocol::SUB, op_sub)?;
	m.associated_function(&Protocol::MUL, op_mul)?;
	m.associated_function(&Protocol::DIV, op_div)?;
	m.function("softmax", softmax).build()?;
	m.function("log_softmax", log_softmax).build()?;
	m.function("softmax_last_dim", softmax_last_dim).build()?;
	Ok(vec![
		(
			"candle::Tensor::from_vec".into(),
			"from_vec(values, shape, dtype) -> Result<Tensor>: dtype \"f32\" | \"f64\" | \"i64\" | \"u32\" | \"u8\"; strict ranges",
		),
		(
			"candle::Tensor::zeros".into(),
			"zeros(shape, dtype), ones(shape, dtype), full(value, shape, dtype), eye(n, dtype) -> Result<Tensor>",
		),
		(
			"candle::Tensor::arange".into(),
			"arange(start, end, dtype), arange_step(start, end, step, dtype) -> Result<Tensor>: the loop is proven before it runs",
		),
		(
			"candle::Tensor::to_vec".into(),
			"to_vec() / to_vec1() / to_vec2() / to_scalar() -> Result: bounded readback into Rune numbers",
		),
		(
			"candle::Tensor::add".into(),
			"add/sub/mul/div/maximum/minimum(t) and t + - * / (tensor or number) -> Result<Tensor>: equal shapes and dtypes",
		),
		(
			"candle::Tensor::broadcast_add".into(),
			"broadcast_add/sub/mul/div/maximum/minimum/eq/ne/lt/le/gt/ge(t) -> Result<Tensor>: the broadcast shape is checked first",
		),
		(
			"candle::Tensor::eq".into(),
			"eq/ne/lt/le/gt/ge(tensor or number) -> Result<Tensor>: a u8 mask; where_cond(on_true, on_false) on a u8 mask",
		),
		(
			"candle::Tensor::exp".into(),
			"exp, log, sqrt, sqr, abs, neg, recip, tanh, relu, gelu, gelu_erf, silu, sin, cos, floor, ceil, round, sign, erf; affine(mul, add), powf(e), clamp(min, max): f32/f64, IEEE",
		),
		(
			"candle::Tensor::sum".into(),
			"sum/mean(dims), max/min/argmax/argmin(dim) (and _keepdim), sum_all/mean_all/max_all/min_all, var(dim) (unbiased) -> Result<Tensor>",
		),
		(
			"candle::Tensor::matmul".into(),
			"matmul(t), broadcast_matmul(t) -> Result<Tensor>: f32/f64, every operand and result size checked first",
		),
		(
			"candle::Tensor::topk".into(),
			"topk(k, asc), sort_last_dim(asc) -> Result<(values, indices)>; arg_sort_last_dim(asc): NaN refused, ties unspecified",
		),
		(
			"candle::Tensor::reshape".into(),
			"reshape(shape), t(), transpose(a, b), squeeze(d), unsqueeze(d), narrow(d, start, len), flatten(a, b), flatten_all(), contiguous()",
		),
		(
			"candle::softmax".into(),
			"softmax(t, dim), log_softmax(t, dim), softmax_last_dim(t) -> Result<Tensor>",
		),
	])
}

#[cfg(test)]
mod tests {
	//! Record 0134's controls: each family against the direct Candle call
	//! (bit for bit), and every refusal named before any work.
	use super::*;

	fn tv(values: Vec<f64>, shape: &[i64], dtype: &str) -> Result<Tensor, String> {
		from_vec(
			rune::to_value(values).unwrap(),
			rune::to_value(shape.to_vec()).unwrap(),
			dtype,
		)
	}
	fn f32s(t: &Tensor) -> Vec<f32> {
		t.0.flatten_all().unwrap().to_vec1::<f32>().unwrap()
	}
	fn bits32(v: &[f32]) -> Vec<u32> {
		v.iter().map(|x| x.to_bits()).collect()
	}
	fn refuses<T>(r: Result<T, String>, want: &str) {
		match r {
			Ok(_) => panic!("expected a refusal containing {want:?}"),
			Err(e) => assert!(e.contains(want), "{e} (wanted {want})"),
		}
	}
	fn direct(v: Vec<f32>, dims: &[usize]) -> CTensor {
		CTensor::from_vec(v, dims, CPU).unwrap()
	}
	fn shape(v: &[i64]) -> Value {
		rune::to_value(v.to_vec()).unwrap()
	}

	#[test]
	fn construction_is_strict_and_reads_back() {
		let t = tv(
			vec![1.0, 2.5, -3.0, f64::NAN, f64::INFINITY, 0.0],
			&[2, 3],
			"f32",
		)
		.unwrap();
		assert_eq!(dims(&t), [2, 3]);
		assert_eq!((rank(&t), elem_count(&t)), (2, 6));
		let back = rune::from_value::<Vec<f64>>(to_vec(&t).unwrap()).unwrap();
		assert_eq!(back[1], 2.5);
		assert!(back[3].is_nan() && back[4].is_infinite());
		let rows = rune::from_value::<Vec<Vec<f64>>>(to_vec2(&t).unwrap()).unwrap();
		assert_eq!(rows.len(), 2);
		let s = tv(vec![7.0], &[], "i64").unwrap();
		assert_eq!(rune::from_value::<i64>(to_scalar(&s).unwrap()).unwrap(), 7);
		// the ranges
		refuses(tv(vec![3.5e38], &[1], "f32"), "outside the f32 range");
		refuses(tv(vec![256.0], &[1], "u8"), "u8 range");
		refuses(tv(vec![-1.0], &[1], "u32"), "u32 range");
		refuses(tv(vec![2.5], &[1], "i64"), "i64 range");
		// 2^63 is out of i64's range; the f64 just below it is in
		refuses(
			tv(vec![9_223_372_036_854_775_808.0], &[1], "i64"),
			"i64 range",
		);
		let below = tv(vec![9_223_372_036_854_774_784.0], &[1], "i64").unwrap();
		assert_eq!(
			rune::from_value::<Vec<i64>>(to_vec(&below).unwrap()).unwrap(),
			[9_223_372_036_854_774_784]
		);
		let max = from_vec(rune::to_value(vec![i64::MAX]).unwrap(), shape(&[1]), "i64").unwrap();
		assert_eq!(
			rune::from_value::<Vec<i64>>(to_vec(&max).unwrap()).unwrap(),
			[i64::MAX]
		);
		// the shape
		refuses(tv(vec![1.0, 2.0], &[3], "f32"), "2 values for shape [3]");
		refuses(tv(vec![], &[1, 1, 1, 1, 1, 1, 1], "f32"), "rank 7");
		refuses(
			zeros(shape(&[4097, 4097]), "f32"),
			"exceeds 16777216 values",
		);
		refuses(zeros(shape(&[-1]), "f32"), "non-negative");
		refuses(zeros(shape(&[2]), "f16"), "dtype \"f16\"");
		// the constructors against Candle
		let z = zeros(shape(&[2, 0, 3]), "f64").unwrap();
		assert_eq!(z.0.dims(), [2, 0, 3]);
		let e = eye(3, "u8").unwrap();
		assert_eq!(
			e.0.to_vec2::<u8>().unwrap(),
			CTensor::eye(3, DType::U8, CPU)
				.unwrap()
				.to_vec2::<u8>()
				.unwrap()
		);
		refuses(eye(4097, "f32"), "exceeds");
		let f = full(rune::to_value(255i64).unwrap(), shape(&[2]), "u8").unwrap();
		assert_eq!(f.0.to_vec1::<u8>().unwrap(), [255, 255]);
		refuses(
			full(rune::to_value(256i64).unwrap(), shape(&[2]), "u8"),
			"u8 range",
		);
	}

	fn ar(start: f64, end: f64, step: f64, dtype: &str) -> Result<Tensor, String> {
		arange_step(
			rune::to_value(start).unwrap(),
			rune::to_value(end).unwrap(),
			rune::to_value(step).unwrap(),
			dtype,
		)
	}

	#[test]
	fn arange_proves_candles_loop_before_running_it() {
		// equal to Candle's own loop, bit for bit
		let a = ar(0.0, 1.0, 0.1, "f32").unwrap();
		let want = CTensor::arange_step(0f32, 1.0, 0.1, CPU).unwrap();
		assert_eq!(bits32(&f32s(&a)), bits32(&want.to_vec1::<f32>().unwrap()));
		let b = ar(10.0, 0.0, -3.0, "i64").unwrap();
		assert_eq!(b.0.to_vec1::<i64>().unwrap(), [10, 7, 4, 1]);
		let c = arange(
			rune::to_value(0i64).unwrap(),
			rune::to_value(5i64).unwrap(),
			"u8",
		)
		.unwrap();
		assert_eq!(c.0.to_vec1::<u8>().unwrap(), [0, 1, 2, 3, 4]);
		// the three loops that would never end or would overflow
		refuses(
			ar(16_777_216.0, 16_777_220.0, 1.0, "f32"),
			"doesn't change the value",
		);
		let max = i64::MAX;
		refuses(
			arange_step(
				rune::to_value(max - 1).unwrap(),
				rune::to_value(max).unwrap(),
				rune::to_value(2i64).unwrap(),
				"i64",
			),
			"overflows",
		);
		refuses(ar(254.0, 255.0, 2.0, "u8"), "overflows");
		// zero, wrong-signed and non-finite steps, and the count bound
		refuses(ar(0.0, 5.0, 0.0, "f64"), "the step is zero");
		refuses(ar(0.0, 5.0, -1.0, "f64"), "never reaches");
		refuses(ar(0.0, f64::INFINITY, 1.0, "f64"), "must be finite");
		refuses(ar(0.0, 5.0, f64::NAN, "f64"), "must be finite");
		refuses(
			ar(0.0, 16_777_217.0, 1.0, "f64"),
			"more than 16777216 values",
		);
		// an empty range is a valid, empty tensor
		assert_eq!(ar(3.0, 3.0, 1.0, "f64").unwrap().0.dims(), [0]);
	}

	#[test]
	fn arithmetic_matches_candle_and_refuses_mixing() {
		let a = Tensor(direct(vec![1.0, -2.0, 3.0, 0.0], &[2, 2]));
		let b = Tensor(direct(vec![0.5, 4.0, -1.0, 2.0], &[2, 2]));
		for (name, ours, theirs) in [
			("add", add(&a, &b).unwrap(), a.0.add(&b.0).unwrap()),
			("sub", sub(&a, &b).unwrap(), a.0.sub(&b.0).unwrap()),
			("mul", mul(&a, &b).unwrap(), a.0.mul(&b.0).unwrap()),
			("div", div(&a, &b).unwrap(), a.0.div(&b.0).unwrap()),
			(
				"maximum",
				maximum(&a, &b).unwrap(),
				a.0.maximum(&b.0).unwrap(),
			),
		] {
			assert_eq!(
				bits32(&f32s(&ours)),
				bits32(&theirs.flatten_all().unwrap().to_vec1().unwrap()),
				"{name}"
			);
		}
		// operators: tensors, and numbers through Candle's affine impls
		let plus = op_add(&a, rune::to_value(b.clone()).unwrap()).unwrap();
		assert_eq!(f32s(&plus), f32s(&add(&a, &b).unwrap()));
		let scaled = op_mul(&a, rune::to_value(2.0).unwrap()).unwrap();
		assert_eq!(f32s(&scaled), [2.0, -4.0, 6.0, 0.0]);
		let shifted = op_sub(&a, rune::to_value(1i64).unwrap()).unwrap();
		assert_eq!(f32s(&shifted), [0.0, -3.0, 2.0, -1.0]);
		// no implicit promotion, no implicit broadcasting
		let d64 = Tensor(a.0.to_dtype(DType::F64).unwrap());
		refuses(add(&a, &d64), "dtypes differ (f32 and f64)");
		let row = Tensor(direct(vec![10.0, 20.0], &[1, 2]));
		refuses(add(&a, &row), "use the broadcast_ form");
		// broadcasting: the shape first
		let bc = broadcast_add(&a, &row).unwrap();
		assert_eq!(f32s(&bc), [11.0, 18.0, 13.0, 20.0]);
		let three = Tensor(direct(vec![1.0, 2.0, 3.0], &[3]));
		refuses(broadcast_add(&a, &three), "don't broadcast");
		let tall = Tensor(CTensor::zeros((4097, 1), DType::F32, CPU).unwrap());
		let wide = Tensor(CTensor::zeros((1, 4097), DType::F32, CPU).unwrap());
		refuses(broadcast_mul(&tall, &wide), "exceeds 16777216 values");
		// comparisons: masks, against tensors or numbers
		let m = gt(&a, rune::to_value(0.5).unwrap()).unwrap();
		assert_eq!(m.0.to_vec2::<u8>().unwrap(), [[1, 0], [1, 0]]);
		let m2 = le(&a, rune::to_value(b.clone()).unwrap()).unwrap();
		assert_eq!(m2.0.dtype(), DType::U8);
		let w = where_cond(&m, &a, &b).unwrap();
		assert_eq!(f32s(&w), [1.0, 4.0, 3.0, 2.0]);
		refuses(where_cond(&a, &a, &b), "the mask must be u8");
		let cl = clamp(
			&a,
			rune::to_value(-1.0).unwrap(),
			rune::to_value(2.0).unwrap(),
		)
		.unwrap();
		assert_eq!(f32s(&cl), [1.0, -1.0, 2.0, 0.0]);
		// unary math is f32/f64 and IEEE
		let z = Tensor(direct(vec![0.0, -1.0], &[2]));
		let l = log(&z).unwrap();
		assert!(f32s(&l)[0] == f32::NEG_INFINITY);
		assert!(f32s(&sqrt(&z).unwrap())[1].is_nan());
		let ints = Tensor(CTensor::new(&[1i64, 2], CPU).unwrap());
		refuses(exp(&ints), "needs f32 or f64");
		assert_eq!(
			bits32(&f32s(&tanh(&a).unwrap())),
			bits32(
				&a.0.tanh()
					.unwrap()
					.flatten_all()
					.unwrap()
					.to_vec1()
					.unwrap()
			)
		);
	}

	#[test]
	fn reductions_follow_their_stated_conventions() {
		let a = Tensor(direct(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]));
		let ax = |v: i64| rune::to_value(v).unwrap();
		assert_eq!(f32s(&sum(&a, ax(-1)).unwrap()), [6.0, 15.0]);
		assert_eq!(f32s(&sum(&a, ax(0)).unwrap()), [5.0, 7.0, 9.0]);
		assert_eq!(sum_keepdim(&a, ax(1)).unwrap().0.dims(), [2, 1]);
		assert_eq!(
			f32s(&mean(&a, rune::to_value(vec![0i64, 1]).unwrap()).unwrap()),
			[3.5]
		);
		assert_eq!(f32s(&max(&a, 1).unwrap()), [3.0, 6.0]);
		assert_eq!(argmin(&a, 1).unwrap().0.to_vec1::<u32>().unwrap(), [0, 0]);
		assert_eq!(f32s(&sum_all(&a).unwrap()), [21.0]);
		// var is unbiased: [1, 2, 3] has variance 1
		assert_eq!(f32s(&var(&a, 1).unwrap()), [1.0, 1.0]);
		refuses(
			var(&Tensor(direct(vec![1.0, 2.0], &[2, 1])), 1),
			"at least 2",
		);
		refuses(sum(&a, ax(2)), "axis 2 is out of range for rank 2");
		refuses(sum(&a, rune::to_value(vec![1i64, -1]).unwrap()), "repeated");
		// empty axes: sum gives zeros; mean, max, argmax refuse
		let e = Tensor(CTensor::zeros((3, 0), DType::F32, CPU).unwrap());
		assert_eq!(f32s(&sum(&e, ax(1)).unwrap()), [0.0, 0.0, 0.0]);
		refuses(mean(&e, ax(1)), "axis 1 is empty");
		refuses(max(&e, 1), "axis 1 is empty");
		refuses(argmax(&e, -1), "axis 1 is empty");
		refuses(max_all(&e), "the tensor is empty");
		// softmax rows sum to one
		let s = softmax(&a, -1).unwrap();
		let rows = sum(&s, ax(1)).unwrap();
		assert!(f32s(&rows).iter().all(|x| (x - 1.0).abs() < 1e-6));
		let ls = log_softmax(&a, 1).unwrap();
		assert_eq!(
			bits32(&f32s(&ls)),
			bits32(
				&candle_nn::ops::log_softmax(&a.0, 1)
					.unwrap()
					.flatten_all()
					.unwrap()
					.to_vec1()
					.unwrap()
			)
		);
	}

	#[test]
	fn matmul_checks_operands_and_expanded_copies_first() {
		let a = Tensor(direct(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]));
		let b = Tensor(direct(vec![1.0, 0.0, 0.0, 1.0, 1.0, 1.0], &[3, 2]));
		let c = matmul(&a, &b).unwrap();
		assert_eq!(
			bits32(&f32s(&c)),
			bits32(
				&a.0.matmul(&b.0)
					.unwrap()
					.flatten_all()
					.unwrap()
					.to_vec1()
					.unwrap()
			)
		);
		refuses(matmul(&a, &a), "inner dimensions differ");
		let batched = Tensor(CTensor::zeros((4, 3, 2), DType::F32, CPU).unwrap());
		refuses(matmul(&a, &batched), "batch dimensions differ");
		assert_eq!(broadcast_matmul(&a, &batched).unwrap().0.dims(), [4, 2, 2]);
		// the expanded left operand ([4097, 4096, 1]) is refused before Candle
		// copies it, though the result ([4097, 4096, 1]) would also be capped
		let left = Tensor(CTensor::zeros((4096, 1), DType::F32, CPU).unwrap());
		let right = Tensor(CTensor::zeros((4097, 1, 1), DType::F32, CPU).unwrap());
		refuses(broadcast_matmul(&left, &right), "exceeds");
		// a small result, but Candle would copy the left operand twice over:
		// [4096, 4096] against a batch of 2 expands to [2, 4096, 4096] (2^25)
		let square = Tensor(CTensor::zeros((4096, 4096), DType::F32, CPU).unwrap());
		let column = Tensor(CTensor::zeros((2, 4096, 1), DType::F32, CPU).unwrap());
		refuses(
			broadcast_matmul(&square, &column),
			"the expanded left operand [2, 4096, 4096]",
		);
		// and against a batch of 1 the same operands fit
		let one = Tensor(CTensor::zeros((1, 4096, 1), DType::F32, CPU).unwrap());
		assert_eq!(
			broadcast_matmul(&square, &one).unwrap().0.dims(),
			[1, 4096, 1]
		);
		let ints = Tensor(CTensor::new(&[[1i64]], CPU).unwrap());
		refuses(matmul(&ints, &ints), "needs f32 or f64");
	}

	#[test]
	fn ranking_sorts_the_logical_values_of_any_view() {
		// the upstream defect: a contiguous offset view sorts the wrong prefix
		let base = direct(vec![0.0, 1.0, 2.0, 9.0, 3.0, 6.0], &[6]);
		let view = base.narrow(0, 3, 3).unwrap();
		assert!(view.is_contiguous());
		let raw = view
			.sort_last_dim(true)
			.unwrap()
			.0
			.to_vec1::<f32>()
			.unwrap();
		assert_eq!(raw, [9.0, 3.0, 6.0], "pinned Candle's documented defect");
		let (v, i) = sort_last_dim(&Tensor(view.clone()), true).unwrap();
		assert_eq!(f32s(&v), [3.0, 6.0, 9.0]);
		assert_eq!(i.0.to_vec1::<u32>().unwrap(), [1, 2, 0]);
		// a transposed input
		let m = direct(vec![3.0, 1.0, 2.0, 0.0], &[2, 2]);
		let tr = Tensor(m.t().unwrap());
		assert_eq!(
			arg_sort_last_dim(&tr, true)
				.unwrap()
				.0
				.to_vec2::<u32>()
				.unwrap(),
			[[1, 0], [1, 0]]
		);
		// NaN is refused; empty last axis is refused
		refuses(
			sort_last_dim(&Tensor(direct(vec![1.0, f32::NAN], &[2])), true),
			"1 NaN",
		);
		refuses(
			topk(
				&Tensor(CTensor::zeros((2, 0), DType::F32, CPU).unwrap()),
				1,
				false,
			),
			"last axis is empty",
		);
		// top-k with a tie at the cutoff: distinct indices, each holding 5
		let tie = Tensor(direct(vec![5.0, 1.0, 5.0, 3.0, 5.0], &[5]));
		let (vals, idx) = topk(&tie, 2, false).unwrap();
		assert_eq!(f32s(&vals), [5.0, 5.0]);
		let idx = idx.0.to_vec1::<u32>().unwrap();
		assert_ne!(idx[0], idx[1]);
		assert!(idx.iter().all(|&j| [0, 2, 4].contains(&j)));
		refuses(topk(&tie, 6, false), "k = 6, want 1 to 5");
		refuses(topk(&tie, 0, false), "k = 0");
	}

	#[test]
	fn shape_operations_check_their_arguments() {
		let a = Tensor(direct(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]));
		assert_eq!(reshape(&a, shape(&[3, 2])).unwrap().0.dims(), [3, 2]);
		refuses(reshape(&a, shape(&[4, 2])), "holds 6 values, not 8");
		assert_eq!(t(&a).unwrap().0.dims(), [3, 2]);
		refuses(t(&Tensor(direct(vec![1.0], &[1]))), "needs rank 2");
		assert_eq!(transpose(&a, 0, -1).unwrap().0.dims(), [3, 2]);
		assert_eq!(unsqueeze(&a, -1).unwrap().0.dims(), [2, 3, 1]);
		assert_eq!(unsqueeze(&a, 2).unwrap().0.dims(), [2, 3, 1]);
		assert_eq!(
			squeeze(&unsqueeze(&a, 0).unwrap(), 0).unwrap().0.dims(),
			[2, 3]
		);
		assert_eq!(f32s(&narrow(&a, 1, 1, 2).unwrap()), [2.0, 3.0, 5.0, 6.0]);
		refuses(narrow(&a, 1, 2, 2), "beyond axis 1 of length 3");
		refuses(narrow(&a, 1, -1, 2), "non-negative");
		assert_eq!(flatten(&a, 0, 1).unwrap().0.dims(), [6]);
		refuses(flatten(&a, 1, 0), "after end axis");
		assert!(is_contiguous(&a) && !is_contiguous(&t(&a).unwrap()));
		assert!(is_contiguous(&contiguous(&t(&a).unwrap()).unwrap()));
		// to_vec2 counts the rows of an empty matrix
		let rows = Tensor(CTensor::zeros((5, 0), DType::F32, CPU).unwrap());
		assert_eq!(
			rune::from_value::<Vec<Vec<f64>>>(to_vec2(&rows).unwrap())
				.unwrap()
				.len(),
			5
		);
		let too_many = Tensor(CTensor::zeros((MAX_ELEMS + 1, 0), DType::F32, CPU).unwrap());
		refuses(to_vec2(&too_many), "the rows");
	}

	#[test]
	fn the_display_shows_every_result_kind_within_its_bound() {
		let show = |t: &CTensor| crate::display::render(t);
		assert!(show(&CTensor::new(3.5f32, CPU).unwrap()).ends_with("scalar]\n3.5"));
		assert!(show(&CTensor::arange(0u32, 20, CPU).unwrap()).contains("0 | 1 | 2"));
		let u8s = CTensor::new(&[0u8, 1, 255], CPU).unwrap();
		assert!(show(&u8s).contains("0 | 1 | 255"), "{}", show(&u8s));
		let nan = CTensor::new(&[f32::NAN, f32::INFINITY], CPU).unwrap();
		assert!(show(&nan).contains("NaN | inf"));
		let r3 = CTensor::zeros((2, 3, 4), DType::I64, CPU).unwrap();
		assert!(show(&r3).contains("2x3x4] (the [0] slice)"));
		let big = CTensor::zeros((4096, 4096), DType::F64, CPU).unwrap();
		let text = show(&big);
		assert!(text.len() <= 2048 && text.contains("…"));
		let empty = CTensor::zeros((0, 3), DType::F32, CPU).unwrap();
		assert!(show(&empty).contains("(empty)"));
	}

	#[test]
	fn integer_scalars_keep_their_precision_in_comparisons_and_clamp() {
		// 2^53 + 1 is not an f64; compared as an i64 it must match itself
		let big: i64 = 9_007_199_254_740_993;
		let t = Tensor(CTensor::new(&[big - 1, big, big + 1], CPU).unwrap());
		let n = |v: i64| rune::to_value(v).unwrap();
		let direct = |f: &dyn Fn(&CTensor) -> CTensor| f(&t.0).to_vec1::<u8>().unwrap();
		let typed = CTensor::new(big, CPU).unwrap().broadcast_as(3).unwrap();
		assert_eq!(
			eq(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			[0, 1, 0]
		);
		assert_eq!(
			eq(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			direct(&|x| x.eq(&typed).unwrap())
		);
		assert_eq!(
			ne(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			[1, 0, 1]
		);
		assert_eq!(
			lt(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			[1, 0, 0]
		);
		assert_eq!(
			le(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			[1, 1, 0]
		);
		assert_eq!(
			gt(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			[0, 0, 1]
		);
		assert_eq!(
			ge(&t, n(big)).unwrap().0.to_vec1::<u8>().unwrap(),
			[0, 1, 1]
		);
		// the i64 boundaries
		let ends = Tensor(CTensor::new(&[i64::MIN, 0, i64::MAX], CPU).unwrap());
		assert_eq!(
			eq(&ends, n(i64::MAX)).unwrap().0.to_vec1::<u8>().unwrap(),
			[0, 0, 1]
		);
		assert_eq!(
			eq(&ends, n(i64::MIN)).unwrap().0.to_vec1::<u8>().unwrap(),
			[1, 0, 0]
		);
		// clamp with numbers, with a tensor and a number, both exact
		let c = clamp(&t, n(big), n(big)).unwrap();
		assert_eq!(c.0.to_vec1::<i64>().unwrap(), [big, big, big]);
		let lo = Tensor(CTensor::new(&[big, big, big], CPU).unwrap());
		let mixed = clamp(&t, rune::to_value(lo.clone()).unwrap(), n(big + 1)).unwrap();
		assert_eq!(mixed.0.to_vec1::<i64>().unwrap(), [big, big, big + 1]);
		assert_eq!(
			mixed.0.to_vec1::<i64>().unwrap(),
			t.0.clamp(
				&lo.0,
				&CTensor::new(big + 1, CPU).unwrap().broadcast_as(3).unwrap()
			)
			.unwrap()
			.to_vec1::<i64>()
			.unwrap()
		);
		// a number outside the receiver's dtype is refused, not wrapped
		let bytes = Tensor(CTensor::new(&[1u8, 200], CPU).unwrap());
		refuses(lt(&bytes, n(300)), "u8 range");
		refuses(clamp(&bytes, n(-1), n(5)), "u8 range");
		// a float receiver takes the number by the f32 rule
		let f = Tensor(direct_vec(vec![1.0, 2.0]));
		assert_eq!(gt(&f, n(1)).unwrap().0.to_vec1::<u8>().unwrap(), [0, 1]);
	}

	fn direct_vec(v: Vec<f32>) -> CTensor {
		let n = v.len();
		CTensor::from_vec(v, n, CPU).unwrap()
	}

	#[test]
	fn stride_products_are_checked_even_behind_a_zero() {
		let big = i64::MAX;
		// the count is 0, but Candle's reverse stride product overflows
		refuses(
			zeros(shape(&[0, big, big]), "f32"),
			"overflows its stride products",
		);
		refuses(
			zeros(shape(&[0, 1 << 33, 1 << 33]), "f32"),
			"overflows its stride products",
		);
		// a valid zero-sized shape whose transpose moves the zero forward
		let z = zeros(shape(&[1 << 33, 0, 1 << 33]), "f32").unwrap();
		assert_eq!(z.0.elem_count(), 0);
		refuses(transpose(&z, 0, 1), "overflows its stride products");
		let z2 = zeros(shape(&[2, 1 << 33, 0, 1 << 33]), "f32").unwrap();
		refuses(transpose(&z2, 1, 2), "overflows its stride products");
		// the ones that stay valid still work
		assert_eq!(transpose(&z, 0, 2).unwrap().0.dims(), [1 << 33, 0, 1 << 33]);
		assert_eq!(unsqueeze(&z, 0).unwrap().0.dims(), [1, 1 << 33, 0, 1 << 33]);
		assert_eq!(flatten(&z, 0, 1).unwrap().0.dims(), [0, 1 << 33]);
		// zeros at both ends hide a middle segment whose product overflows
		let ends = zeros(shape(&[0, 1 << 33, 1 << 33, 0]), "f32").unwrap();
		refuses(
			flatten(&ends, 1, 2),
			"axes 1 to 2 of [0, 8589934592, 8589934592, 0] overflow when merged",
		);
		// a valid flatten still works, and the receiver is reusable
		assert_eq!(flatten(&ends, 0, 1).unwrap().0.dims(), [0, 1 << 33, 0]);
		let ok = zeros(shape(&[2, 3, 4]), "f32").unwrap();
		assert_eq!(flatten(&ok, 1, 2).unwrap().0.dims(), [2, 12]);
		assert_eq!(ok.0.dims(), [2, 3, 4]);
	}

	#[test]
	fn arange_refuses_a_terminal_increment_that_overflows_to_infinity() {
		refuses(
			ar(3e38, 3.4e38, 3e38, "f32"),
			"overflows or doesn't change the value",
		);
		refuses(
			ar(1e308, 1.5e308, 1e308, "f64"),
			"overflows or doesn't change the value",
		);
		refuses(
			ar(-3e38, -3.4e38, -3e38, "f32"),
			"overflows or doesn't change the value",
		);
		refuses(
			ar(-1e308, -1.5e308, -1e308, "f64"),
			"overflows or doesn't change the value",
		);
		// ordinary large ranges still equal Candle's
		let ok = ar(1e30, 5e30, 1e30, "f32").unwrap();
		assert_eq!(
			bits32(&f32s(&ok)),
			bits32(
				&CTensor::arange_step(1e30f32, 5e30, 1e30, CPU)
					.unwrap()
					.to_vec1::<f32>()
					.unwrap()
			)
		);
	}
}
