//! Record 0145: Candle's signal operations (convolutions, pooling,
//! resampling): 16 core `Tensor` identities (`probes/0145/manifest.tsv`).
//!
//! Every output size is computed by rnx, in upstream's own order of
//! arithmetic, with each step checked: candle-core 0.11.0 computes them in
//! unchecked `usize` arithmetic (an underflow panics in debug and wraps in
//! release), divides by a zero stride, and allocates path-specific
//! temporaries far larger than the input or output (im2col and col2im
//! columns, conv2d's tiles). Each path's temporaries are predicted by formula
//! and bounded before the call ([`Census`]): each at most `MAX_ELEMS`, all of
//! a call together at most `4 × MAX_ELEMS`.
//!
//! Inputs and kernels are canonicalized to fresh, contiguous, offset-zero
//! storage on the worker before the call: candle-core 0.11.0's CPU `conv1d`
//! copies a non-contiguous kernel into `kernel_c` and then multiplies by the
//! original under an imposed contiguous layout (`cpu_backend/mod.rs`, around
//! line 2785), so a strided kernel would give wrong results.
use super::composition::{AXIS, PARTS};
use super::{MAX_ELEMS, capped, float_only, same_dtype, wrap};
use crate::Tensor;
use candle_core::{Tensor as CTensor, conv::CudnnFwdAlgo};
use rnx::rune::{Value, runtime::Vec as RuneVec};

/// A call's temporaries, each and together, predicted before the call.
struct Census<'a> {
	op: &'a str,
	total: usize,
}

impl<'a> Census<'a> {
	fn new(op: &'a str) -> Self {
		Census { op, total: 0 }
	}
	/// `count` values held at once by `what`, one buffer; checked, and added.
	fn add(&mut self, what: &str, count: Option<usize>) -> Result<usize, String> {
		let op = self.op;
		let n = count.ok_or_else(|| format!("{op}: {what} overflows"))?;
		if n > MAX_ELEMS {
			return Err(format!(
				"{op}: {what} would hold {n} values, at most {MAX_ELEMS}"
			));
		}
		self.spread(what, Some(n))
	}
	/// `count` values spread over many buffers (every tile's temporaries):
	/// only the call's cumulative allowance applies.
	fn spread(&mut self, what: &str, count: Option<usize>) -> Result<usize, String> {
		let op = self.op;
		let n = count.ok_or_else(|| format!("{op}: {what} overflows"))?;
		self.total = self
			.total
			.checked_add(n)
			.filter(|&t| t <= 4 * MAX_ELEMS)
			.ok_or_else(|| {
				format!(
					"{op}: the call's buffers would hold more than {} values together ({what} is {n})",
					4 * MAX_ELEMS
				)
			})?;
		Ok(n)
	}
}

/// `bytes` of non-tensor temporaries (usize coordinates, f64 weights),
/// counted as values of the tensor's dtype (`item` bytes each), rounded up.
fn as_values(bytes: Option<usize>, item: usize) -> Option<usize> {
	bytes.map(|b| b.div_ceil(item))
}

fn mul(xs: &[usize]) -> Option<usize> {
	xs.iter().try_fold(1usize, |a, &x| a.checked_mul(x))
}

/// A non-negative integer argument, at least `min`.
fn arg(op: &str, v: i64, what: &str, min: usize) -> Result<usize, String> {
	usize::try_from(v)
		.ok()
		.filter(|&n| n >= min)
		.ok_or_else(|| format!("{op}: {what} must be at least {min}, found {v}"))
}

/// A pair `[a, b]` of integers, each at least `min`.
fn pair(op: &str, v: &Value, what: &str, min: usize) -> Result<(usize, usize), String> {
	let not = || format!("{op}: {what} must be a vector of two integers, each at least {min}");
	let values = v.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.len() != 2 {
		return Err(not());
	}
	let mut out = [0usize; 2];
	for (i, x) in values.iter().enumerate() {
		let n = rnx::rune::from_value::<i64>(x.clone()).map_err(|_| not())?;
		out[i] = arg(op, n, what, min)?;
	}
	Ok((out[0], out[1]))
}

/// Rank, float dtype, and no empty axis.
fn shaped<'t>(op: &str, t: &'t CTensor, rank: usize, what: &str) -> Result<&'t [usize], String> {
	float_only(op, t)?;
	let d = t.dims();
	if d.len() != rank {
		return Err(format!("{op}: {what} must be rank {rank}, found {:?}", d));
	}
	if d.contains(&0) {
		return Err(format!(
			"{op}: {what} {d:?} has an empty axis; empty inputs and kernels are refused"
		));
	}
	Ok(d)
}

/// `size + 2·padding − dilation·(k − 1) − 1`, then `/ stride + 1`, each step
/// checked, in candle's order (`conv.rs` `l_out`, `out_h`, `out_w`).
fn conv_out(
	op: &str,
	size: usize,
	k: usize,
	padding: usize,
	stride: usize,
	dilation: usize,
	axis: &str,
) -> Result<usize, String> {
	let short = || {
		format!(
			"{op}: along {axis}, the dilated kernel ({k} with dilation {dilation}) exceeds the padded input ({size} + 2 × {padding})"
		)
	};
	let wide = || format!("{op}: along {axis}, the sizes overflow");
	let padded = padding
		.checked_mul(2)
		.and_then(|p| size.checked_add(p))
		.ok_or_else(wide)?;
	let extent = dilation.checked_mul(k - 1).ok_or_else(wide)?;
	let n = padded
		.checked_sub(extent)
		.and_then(|n| n.checked_sub(1))
		.ok_or_else(short)?;
	Ok(n / stride + 1)
}

fn algo(op: &str, name: &str) -> Result<Option<CudnnFwdAlgo>, String> {
	Ok(Some(match name {
		"none" => return Ok(None),
		"implicit_gemm" => CudnnFwdAlgo::ImplicitGemm,
		"implicit_precomp_gemm" => CudnnFwdAlgo::ImplicitPrecompGemm,
		"gemm" => CudnnFwdAlgo::Gemm,
		"direct" => CudnnFwdAlgo::Direct,
		"fft" => CudnnFwdAlgo::Fft,
		"fft_tiling" => CudnnFwdAlgo::FftTiling,
		"winograd" => CudnnFwdAlgo::Winograd,
		"winograd_non_fused" => CudnnFwdAlgo::WinogradNonFused,
		other => {
			return Err(format!(
				"{op}: algo {other:?}; want \"none\" or a cuDNN name (\"implicit_gemm\", \"gemm\", \"direct\", \"fft\", ...); on this CPU adapter it has no effect"
			));
		}
	}))
}

/// The groups count: at least 1 and at most `PARTS` (candle chunks the
/// input and kernel into that many tensors), dividing `a` and `b`.
fn groups(op: &str, g: i64, a: usize, b: usize, what: &str) -> Result<usize, String> {
	let g = arg(op, g, "groups", 1)?;
	if g > PARTS {
		return Err(format!("{op}: groups {g}, at most {PARTS}"));
	}
	if !a.is_multiple_of(g) || !b.is_multiple_of(g) {
		return Err(format!("{op}: groups {g} must divide {what} ({a} and {b})"));
	}
	Ok(g)
}

// ---- convolutions ----

struct Conv1 {
	padding: usize,
	stride: usize,
	dilation: usize,
	groups: usize,
}

fn conv1d_checked(
	op: &str,
	x: &CTensor,
	k: &CTensor,
	p: i64,
	s: i64,
	d: i64,
	g: i64,
) -> Result<Conv1, String> {
	same_dtype(op, x, k)?;
	let xd = shaped(op, x, 3, "the input")?.to_vec();
	let kd = shaped(op, k, 3, "the kernel")?.to_vec();
	let (b, c_in, l) = (xd[0], xd[1], xd[2]);
	let (c_out, c_in_k, ks) = (kd[0], kd[1], kd[2]);
	let padding = arg(op, p, "padding", 0)?;
	let stride = arg(op, s, "stride", 1)?;
	let dilation = arg(op, d, "dilation", 1)?;
	let g = groups(op, g, c_in, c_out, "the input and output channels")?;
	if c_in != c_in_k * g {
		return Err(format!(
			"{op}: the input has {c_in} channels; the kernel takes {c_in_k} per group × {g} groups"
		));
	}
	let l_out = conv_out(op, l, ks, padding, stride, dilation, "the length")?;
	let mut c = Census::new(op);
	c.add("the input copy", mul(&[b, c_in, l]))?;
	c.add("the kernel copy", mul(&[c_out, c_in_k, ks]))?;
	// per group: im2col's column, the matmul result and its transposed copy;
	// every group's result is retained until the cat
	for _ in 0..g {
		c.add("the im2col column", mul(&[b, l_out, c_in_k, ks]))?;
		c.add("a group's result", mul(&[b, c_out / g, l_out]))?;
		c.add("a group's result copy", mul(&[b, c_out / g, l_out]))?;
	}
	if g > 1 {
		c.add("the concatenated output", mul(&[b, c_out, l_out]))?;
	}
	capped(op, &[b, c_out, l_out], "the output")?;
	Ok(Conv1 {
		padding,
		stride,
		dilation,
		groups: g,
	})
}

fn conv1d(
	this: &Tensor,
	kernel: &Tensor,
	padding: i64,
	stride: i64,
	dilation: i64,
	groups: i64,
) -> Result<Tensor, String> {
	let op = "Tensor::conv1d";
	let c = conv1d_checked(op, &this.0, &kernel.0, padding, stride, dilation, groups)?;
	let (x, k) = (this.0.clone(), kernel.0.clone());
	wrap(op, move || {
		x.force_contiguous()?.conv1d(
			&k.force_contiguous()?,
			c.padding,
			c.stride,
			c.dilation,
			c.groups,
		)
	})
}

fn conv1d_with_algo(
	this: &Tensor,
	kernel: &Tensor,
	padding: i64,
	stride: i64,
	dilation: i64,
	groups: i64,
	name: &str,
) -> Result<Tensor, String> {
	let op = "Tensor::conv1d_with_algo";
	let a = algo(op, name)?;
	let c = conv1d_checked(op, &this.0, &kernel.0, padding, stride, dilation, groups)?;
	let (x, k) = (this.0.clone(), kernel.0.clone());
	wrap(op, move || {
		x.force_contiguous()?.conv1d_with_algo(
			&k.force_contiguous()?,
			c.padding,
			c.stride,
			c.dilation,
			c.groups,
			a,
		)
	})
}

fn conv2d_checked(
	op: &str,
	x: &CTensor,
	k: &CTensor,
	p: i64,
	s: i64,
	d: i64,
	g: i64,
) -> Result<Conv1, String> {
	same_dtype(op, x, k)?;
	let xd = shaped(op, x, 4, "the input")?.to_vec();
	let kd = shaped(op, k, 4, "the kernel")?.to_vec();
	let (b, c_in, h, w) = (xd[0], xd[1], xd[2], xd[3]);
	let (c_out, c_in_k, kh, kw) = (kd[0], kd[1], kd[2], kd[3]);
	let padding = arg(op, p, "padding", 0)?;
	let stride = arg(op, s, "stride", 1)?;
	let dilation = arg(op, d, "dilation", 1)?;
	let g = groups(op, g, c_in, c_out, "the input and output channels")?;
	if c_in != c_in_k * g {
		return Err(format!(
			"{op}: the input has {c_in} channels; the kernel takes {c_in_k} per group × {g} groups"
		));
	}
	let oh = conv_out(op, h, kh, padding, stride, dilation, "the height")?;
	let ow = conv_out(op, w, kw, padding, stride, dilation, "the width")?;
	let item = x.dtype().size_in_bytes();
	let mut c = Census::new(op);
	c.add("the input copy", mul(&[b, c_in, h, w]))?;
	c.add("the kernel copy", mul(&[c_out, c_in_k, kh, kw]))?;
	let ksz = mul(&[c_in_k, kh, kw]);
	let cog = c_out / g;
	for _ in 0..g {
		if kh == 1 && kw == 1 && stride == 1 && padding == 0 && dilation == 1 {
			// the reshaped 1×1 path: input and kernel reshapes, the output,
			// and a separate matmul result per batch item, summed as if all
			// were live (review)
			// review round 1 (R2): one reshaped input per batch item, built
			// in parallel (conv2d.rs line 92), summed conservatively
			c.add("the per-batch reshaped inputs", mul(&[b, c_in_k, h, w]))?;
			c.add("the reshaped kernel", mul(&[cog, c_in_k]))?;
			c.add("a group's output", mul(&[b, cog, oh, ow]))?;
			c.add("the per-batch matmul results", mul(&[b, cog, oh, ow]))?;
		} else if kh == 1 && kw == 1 {
			// any other 1×1: full im2col (conv2d.rs lines 38-41)
			c.add("the im2col column", ksz.and_then(|k| mul(&[b, oh, ow, k])))?;
			c.add("a group's result", mul(&[b, cog, oh, ow]))?;
			c.add("a group's result copy", mul(&[b, cog, oh, ow]))?;
		} else {
			// tiled im2col: an NHWC input copy, the flat kernel, the output,
			// and per 512-pixel tile a column, coordinates and a matmul
			// result, summed over every tile as if all were live (nested
			// rayon tasks may keep them alive; no tighter bound is proven)
			c.add("the NHWC input copy", mul(&[b, c_in_k, h, w]))?;
			c.add("the flat kernel", ksz.and_then(|k| k.checked_mul(cog)))?;
			c.add("a group's output", mul(&[b, cog, oh, ow]))?;
			let tiles = mul(&[oh, ow]).map(|px| px.div_ceil(512));
			// a tile: its column (k × 512), its matmul result (c_out/g ×
			// 512), and 512 coordinate pairs of two usize each, in bytes
			let coords = as_values(Some(512 * 2 * std::mem::size_of::<usize>()), item);
			let per_tile = ksz
				.and_then(|k| k.checked_add(cog))
				.and_then(|v| v.checked_mul(512))
				.and_then(|v| coords.and_then(|c| v.checked_add(c)));
			c.add("one tile", per_tile)?;
			c.spread(
				"every tile's column, coordinates and result",
				tiles.and_then(|t| per_tile.and_then(|p| mul(&[b, t, p]))),
			)?;
		}
	}
	if g > 1 {
		c.add("the concatenated output", mul(&[b, c_out, oh, ow]))?;
	}
	capped(op, &[b, c_out, oh, ow], "the output")?;
	Ok(Conv1 {
		padding,
		stride,
		dilation,
		groups: g,
	})
}

fn conv2d(
	this: &Tensor,
	kernel: &Tensor,
	padding: i64,
	stride: i64,
	dilation: i64,
	groups: i64,
) -> Result<Tensor, String> {
	let op = "Tensor::conv2d";
	let c = conv2d_checked(op, &this.0, &kernel.0, padding, stride, dilation, groups)?;
	let (x, k) = (this.0.clone(), kernel.0.clone());
	wrap(op, move || {
		x.force_contiguous()?.conv2d(
			&k.force_contiguous()?,
			c.padding,
			c.stride,
			c.dilation,
			c.groups,
		)
	})
}

fn conv2d_with_algo(
	this: &Tensor,
	kernel: &Tensor,
	padding: i64,
	stride: i64,
	dilation: i64,
	groups: i64,
	name: &str,
) -> Result<Tensor, String> {
	let op = "Tensor::conv2d_with_algo";
	let a = algo(op, name)?;
	let c = conv2d_checked(op, &this.0, &kernel.0, padding, stride, dilation, groups)?;
	let (x, k) = (this.0.clone(), kernel.0.clone());
	wrap(op, move || {
		x.force_contiguous()?.conv2d_with_algo(
			&k.force_contiguous()?,
			c.padding,
			c.stride,
			c.dilation,
			c.groups,
			a,
		)
	})
}

/// The transposed length in candle's order (`conv.rs` ParamsConvTranspose1D
/// `l_out`): `(l − 1)·s − 2p` first, so `(l − 1)·s < 2p` underflows
/// upstream even when the final size would be positive; refused by name.
fn transposed1d_out(
	op: &str,
	l: usize,
	k: usize,
	p: usize,
	op_pad: usize,
	s: usize,
	d: usize,
) -> Result<usize, String> {
	let wide = || format!("{op}: the sizes overflow");
	let scaled = (l - 1).checked_mul(s).ok_or_else(wide)?;
	let twice = p.checked_mul(2).ok_or_else(wide)?;
	let base = scaled.checked_sub(twice).ok_or_else(|| {
		format!("{op}: (length − 1) × stride = {scaled} is below 2 × padding = {twice}; candle's 1-D transposed size subtracts the padding before adding the kernel, so this underflows upstream (a pinned restriction)")
	})?;
	d.checked_mul(k - 1)
		.and_then(|e| base.checked_add(e))
		.and_then(|v| v.checked_add(op_pad))
		.and_then(|v| v.checked_add(1))
		.ok_or_else(wide)
}

/// The transposed size in candle's 2-D order (`out_h`, `out_w`): the sum
/// first, `2p` subtracted last.
// the parameters are upstream's own (ParamsConvTranspose2D), one per axis
#[allow(clippy::too_many_arguments)]
fn transposed2d_out(
	op: &str,
	n: usize,
	k: usize,
	p: usize,
	op_pad: usize,
	s: usize,
	d: usize,
	axis: &str,
) -> Result<usize, String> {
	let wide = || format!("{op}: along {axis}, the sizes overflow");
	let sum = (n - 1)
		.checked_mul(s)
		.and_then(|v| d.checked_mul(k - 1).and_then(|e| v.checked_add(e)))
		.and_then(|v| v.checked_add(op_pad))
		.and_then(|v| v.checked_add(1))
		.ok_or_else(wide)?;
	let twice = p.checked_mul(2).ok_or_else(wide)?;
	sum.checked_sub(twice).filter(|&v| v >= 1).ok_or_else(|| {
		format!("{op}: along {axis}, 2 × padding ({twice}) leaves no output ({sum} before it)")
	})
}

/// PyTorch's rule: output_padding below the stride or the dilation.
fn output_padding(op: &str, v: i64, s: usize, d: usize) -> Result<usize, String> {
	let o = arg(op, v, "output_padding", 0)?;
	if o >= s && o >= d {
		return Err(format!(
			"{op}: output_padding {o} must be below the stride ({s}) or the dilation ({d})"
		));
	}
	Ok(o)
}

fn conv_transpose1d(
	this: &Tensor,
	kernel: &Tensor,
	padding: i64,
	out_padding: i64,
	stride: i64,
	dilation: i64,
	groups_: i64,
) -> Result<Tensor, String> {
	let op = "Tensor::conv_transpose1d";
	let (x, k) = (&this.0, &kernel.0);
	same_dtype(op, x, k)?;
	let xd = shaped(op, x, 3, "the input")?.to_vec();
	let kd = shaped(
		op,
		k,
		3,
		"the kernel ([in channels, out channels per group, length])",
	)?
	.to_vec();
	let (b, c_in, l) = (xd[0], xd[1], xd[2]);
	let (c_in_k, cog, ks) = (kd[0], kd[1], kd[2]);
	if c_in != c_in_k {
		return Err(format!(
			"{op}: the input has {c_in} channels, the kernel {c_in_k} (a transposed kernel is [in, out per group, length])"
		));
	}
	let p = arg(op, padding, "padding", 0)?;
	let s = arg(op, stride, "stride", 1)?;
	let d = arg(op, dilation, "dilation", 1)?;
	let o = output_padding(op, out_padding, s, d)?;
	let g = groups(op, groups_, c_in, c_in, "the input channels")?;
	let l_out = transposed1d_out(op, l, ks, p, o, s, d)?;
	let c_out = cog
		.checked_mul(g)
		.ok_or_else(|| format!("{op}: the channels overflow"))?;
	let mut c = Census::new(op);
	c.add("the input copy", mul(&[b, c_in, l]))?;
	c.add("the kernel copy", mul(&[c_in_k, cog, ks]))?;
	for _ in 0..g {
		if d == 1 && p == 0 && o == 0 {
			// the col2im path (the kernel is contiguous after canonicalizing)
			c.add("the col2im column", mul(&[b, l, cog, ks]))?;
		} else {
			// review round 1 (R3): the direct path's own contiguous input
			// copy, and a kernel row (c_in per group) per output channel,
			// built in parallel over the channels at each kernel position
			c.add("the direct path's input copy", mul(&[b, c_in / g, l]))?;
			c.add("the direct path's kernel rows", mul(&[cog, c_in / g]))?;
		}
		c.add("a group's output", mul(&[b, cog, l_out]))?;
	}
	if g > 1 {
		c.add("the concatenated output", mul(&[b, c_out, l_out]))?;
	}
	capped(op, &[b, c_out, l_out], "the output")?;
	let (x, k) = (x.clone(), k.clone());
	wrap(op, move || {
		x.force_contiguous()?
			.conv_transpose1d(&k.force_contiguous()?, p, o, s, d, g)
	})
}

fn conv_transpose2d(
	this: &Tensor,
	kernel: &Tensor,
	padding: i64,
	out_padding: i64,
	stride: i64,
	dilation: i64,
) -> Result<Tensor, String> {
	let op = "Tensor::conv_transpose2d";
	let (x, k) = (&this.0, &kernel.0);
	same_dtype(op, x, k)?;
	let xd = shaped(op, x, 4, "the input")?.to_vec();
	let kd = shaped(op, k, 4, "the kernel ([in channels, out channels, h, w])")?.to_vec();
	let (b, c_in, h, w) = (xd[0], xd[1], xd[2], xd[3]);
	let (c_in_k, c_out, kh, kw) = (kd[0], kd[1], kd[2], kd[3]);
	if c_in != c_in_k {
		return Err(format!(
			"{op}: the input has {c_in} channels, the kernel {c_in_k} (a transposed kernel is [in, out, h, w]; there are no groups)"
		));
	}
	let p = arg(op, padding, "padding", 0)?;
	let s = arg(op, stride, "stride", 1)?;
	let d = arg(op, dilation, "dilation", 1)?;
	let o = output_padding(op, out_padding, s, d)?;
	let oh = transposed2d_out(op, h, kh, p, o, s, d, "the height")?;
	let ow = transposed2d_out(op, w, kw, p, o, s, d, "the width")?;
	let mut c = Census::new(op);
	c.add("the input copy", mul(&[b, c_in, h, w]))?;
	c.add("the kernel copy", mul(&[c_in, c_out, kh, kw]))?;
	c.add("the output", mul(&[b, c_out, oh, ow]))?;
	// review round 1 (R3): the direct path's own contiguous input copy, and
	// a kernel row (c_in) per output channel at each kernel position, built
	// in parallel over the channels
	c.add("the direct path's input copy", mul(&[b, c_in, h, w]))?;
	c.add("the direct path's kernel rows", mul(&[c_out, c_in]))?;
	capped(op, &[b, c_out, oh, ow], "the output")?;
	let (x, k) = (x.clone(), k.clone());
	wrap(op, move || {
		x.force_contiguous()?
			.conv_transpose2d(&k.force_contiguous()?, p, o, s, d)
	})
}

// ---- pooling ----

fn pooled(op: &str, x: &CTensor, k: (usize, usize), s: (usize, usize)) -> Result<(), String> {
	let d = shaped(op, x, 4, "the input")?;
	let (h, w) = (d[2], d[3]);
	if k.0 > h || k.1 > w {
		return Err(format!(
			"{op}: the kernel {:?} is larger than the input ({h}, {w})",
			[k.0, k.1]
		));
	}
	let (oh, ow) = ((h - k.0) / s.0 + 1, (w - k.1) / s.1 + 1);
	let mut c = Census::new(op);
	c.add("the input copy", mul(d))?;
	c.add("the output", mul(&[d[0], d[1], oh, ow]))?;
	capped(op, &[d[0], d[1], oh, ow], "the output").map(|_| ())
}

fn avg_pool2d(this: &Tensor, kernel: Value) -> Result<Tensor, String> {
	let op = "Tensor::avg_pool2d";
	let k = pair(op, &kernel, "the kernel size", 1)?;
	pooled(op, &this.0, k, k)?;
	let x = this.0.clone();
	wrap(op, move || x.force_contiguous()?.avg_pool2d(k))
}

fn avg_pool2d_with_stride(this: &Tensor, kernel: Value, stride: Value) -> Result<Tensor, String> {
	let op = "Tensor::avg_pool2d_with_stride";
	let k = pair(op, &kernel, "the kernel size", 1)?;
	let s = pair(op, &stride, "the stride", 1)?;
	pooled(op, &this.0, k, s)?;
	let x = this.0.clone();
	wrap(op, move || {
		x.force_contiguous()?.avg_pool2d_with_stride(k, s)
	})
}

fn max_pool2d(this: &Tensor, kernel: Value) -> Result<Tensor, String> {
	let op = "Tensor::max_pool2d";
	let k = pair(op, &kernel, "the kernel size", 1)?;
	pooled(op, &this.0, k, k)?;
	let x = this.0.clone();
	wrap(op, move || x.force_contiguous()?.max_pool2d(k))
}

fn max_pool2d_with_stride(this: &Tensor, kernel: Value, stride: Value) -> Result<Tensor, String> {
	let op = "Tensor::max_pool2d_with_stride";
	let k = pair(op, &kernel, "the kernel size", 1)?;
	let s = pair(op, &stride, "the stride", 1)?;
	pooled(op, &this.0, k, s)?;
	let x = this.0.clone();
	wrap(op, move || {
		x.force_contiguous()?.max_pool2d_with_stride(k, s)
	})
}

// ---- resampling ----

fn target(op: &str, v: i64, what: &str) -> Result<usize, String> {
	let n = arg(op, v, what, 1)?;
	if n > AXIS {
		return Err(format!("{op}: {what} {n}, at most {AXIS}"));
	}
	Ok(n)
}

fn resampled(
	op: &str,
	x: &CTensor,
	rank: usize,
	out: &[usize],
	bilinear: bool,
) -> Result<(), String> {
	let d = shaped(op, x, rank, "the input")?;
	let mut dims = d[..2].to_vec();
	dims.extend_from_slice(out);
	let item = x.dtype().size_in_bytes();
	let mut c = Census::new(op);
	c.add("the input copy", mul(d))?;
	// review round 1 (R1): bilinear alone builds per-axis tables, one
	// `(h0, h1, weight)` = (usize, usize, f64) per target entry
	// (cpu_backend/mod.rs lines 532 and 547); each table is one buffer
	if bilinear {
		let per_entry = std::mem::size_of::<(usize, usize, f64)>();
		for (axis, &n) in out.iter().enumerate() {
			let what = if axis == 0 {
				"the height table"
			} else {
				"the width table"
			};
			c.add(what, as_values(n.checked_mul(per_entry), item))?;
		}
	}
	c.add("the output", mul(&dims))?;
	capped(op, &dims, "the output").map(|_| ())
}

fn upsample_nearest1d(this: &Tensor, n: i64) -> Result<Tensor, String> {
	let op = "Tensor::upsample_nearest1d";
	let n = target(op, n, "the target length")?;
	resampled(op, &this.0, 3, &[n], false)?;
	let x = this.0.clone();
	wrap(op, move || x.force_contiguous()?.upsample_nearest1d(n))
}

fn interpolate1d(this: &Tensor, n: i64) -> Result<Tensor, String> {
	let op = "Tensor::interpolate1d";
	let n = target(op, n, "the target length")?;
	resampled(op, &this.0, 3, &[n], false)?;
	let x = this.0.clone();
	wrap(op, move || x.force_contiguous()?.interpolate1d(n))
}

fn upsample_nearest2d(this: &Tensor, h: i64, w: i64) -> Result<Tensor, String> {
	let op = "Tensor::upsample_nearest2d";
	let (h, w) = (
		target(op, h, "the target height")?,
		target(op, w, "the target width")?,
	);
	resampled(op, &this.0, 4, &[h, w], false)?;
	let x = this.0.clone();
	wrap(op, move || x.force_contiguous()?.upsample_nearest2d(h, w))
}

fn interpolate2d(this: &Tensor, h: i64, w: i64) -> Result<Tensor, String> {
	let op = "Tensor::interpolate2d";
	let (h, w) = (
		target(op, h, "the target height")?,
		target(op, w, "the target width")?,
	);
	resampled(op, &this.0, 4, &[h, w], false)?;
	let x = this.0.clone();
	wrap(op, move || x.force_contiguous()?.interpolate2d(h, w))
}

fn upsample_bilinear2d(
	this: &Tensor,
	h: i64,
	w: i64,
	align_corners: bool,
) -> Result<Tensor, String> {
	let op = "Tensor::upsample_bilinear2d";
	let (h, w) = (
		target(op, h, "the target height")?,
		target(op, w, "the target width")?,
	);
	resampled(op, &this.0, 4, &[h, w], true)?;
	let x = this.0.clone();
	wrap(op, move || {
		x.force_contiguous()?
			.upsample_bilinear2d(h, w, align_corners)
	})
}

fn upsample_bilinear2d_with_scale(
	this: &Tensor,
	sh: f64,
	sw: f64,
	align_corners: bool,
) -> Result<Tensor, String> {
	let op = "Tensor::upsample_bilinear2d_with_scale";
	let d = shaped(op, &this.0, 4, "the input")?;
	// candle computes floor(size × scale) as usize, saturating NaN to 0 and
	// infinity to usize::MAX: the same floor, checked here first
	let scaled = |size: usize, scale: f64, axis: &str| -> Result<usize, String> {
		if !(scale.is_finite() && scale > 0.0) {
			return Err(format!(
				"{op}: the {axis} scale must be finite and positive, found {scale}"
			));
		}
		let v = (size as f64 * scale).floor();
		if !(v >= 1.0 && v <= AXIS as f64) {
			return Err(format!(
				"{op}: the {axis} scale {scale} gives {v} from {size}, want 1 to {AXIS}"
			));
		}
		Ok(v as usize)
	};
	let (h, w) = (scaled(d[2], sh, "height")?, scaled(d[3], sw, "width")?);
	resampled(op, &this.0, 4, &[h, w], true)?;
	let x = this.0.clone();
	wrap(op, move || {
		x.force_contiguous()?
			.upsample_bilinear2d_with_scale(sh, sw, align_corners)
	})
}

// ---- raw shims for the wide signatures ----
//
// Rune 0.14.2's typed functions stop at arity 5 (record 0127), and these take
// 6 or 7 arguments. Each shim follows rune's typed `fn_call`, as 0127's do:
// the count first, every slot taken and replaced with an empty value, the
// receiver at index 0, tensors and strings borrowed (never taken), integers
// and flags converted, the guards alive through the call, then
// `ToReturn::to_return` and `out.store`.

use rnx::rune::runtime::{InstAddress, Memory, Output, RuntimeError, ToReturn, VmResult};

fn slots(
	stack: &mut dyn Memory,
	addr: InstAddress,
	len: usize,
	want: usize,
) -> VmResult<Vec<Value>> {
	if len != want {
		return VmResult::err(RuntimeError::bad_argument_count(len, want));
	}
	let slice = rnx::rune::vm_try!(stack.slice_at_mut(addr, len));
	VmResult::Ok(
		slice
			.iter_mut()
			.map(|v| std::mem::replace(v, Value::empty()))
			.collect(),
	)
}

macro_rules! shim {
	($shim:ident, $f:ident, $n:expr, ints: $ints:expr, algo: $algo:expr) => {
		fn $shim(stack: &mut dyn Memory, addr: InstAddress, len: usize, out: Output) -> VmResult<()> {
			let v = rnx::rune::vm_try!(slots(stack, addr, len, $n));
			let r = {
				let t = rnx::rune::vm_try!(v[0].borrow_ref::<Tensor>());
				let k = rnx::rune::vm_try!(v[1].borrow_ref::<Tensor>());
				let mut n = [0i64; 5];
				for i in 0..$ints {
					n[i] = rnx::rune::vm_try!(rnx::rune::from_value::<i64>(v[2 + i].clone()));
				}
				if $algo {
					let a = rnx::rune::vm_try!(v[2 + $ints].borrow_string_ref());
					shim!(@call $f, &t, &k, n, $ints, Some(&*a))
				} else {
					shim!(@call $f, &t, &k, n, $ints, None)
				}
			};
			let value = rnx::rune::vm_try!(ToReturn::to_return(r));
			rnx::rune::vm_try!(out.store(stack, value));
			VmResult::Ok(())
		}
	};
	(@call $f:ident, $t:expr, $k:expr, $n:expr, $ints:expr, $a:expr) => {
		$f($t, $k, &$n[..$ints], $a)
	};
}

// adapters from the shims' uniform shape to each binding's own signature
fn conv1d_any(t: &Tensor, k: &Tensor, n: &[i64], _a: Option<&str>) -> Result<Tensor, String> {
	conv1d(t, k, n[0], n[1], n[2], n[3])
}
fn conv1d_algo_any(t: &Tensor, k: &Tensor, n: &[i64], a: Option<&str>) -> Result<Tensor, String> {
	conv1d_with_algo(t, k, n[0], n[1], n[2], n[3], a.unwrap_or_default())
}
fn conv2d_any(t: &Tensor, k: &Tensor, n: &[i64], _a: Option<&str>) -> Result<Tensor, String> {
	conv2d(t, k, n[0], n[1], n[2], n[3])
}
fn conv2d_algo_any(t: &Tensor, k: &Tensor, n: &[i64], a: Option<&str>) -> Result<Tensor, String> {
	conv2d_with_algo(t, k, n[0], n[1], n[2], n[3], a.unwrap_or_default())
}
fn conv_transpose1d_any(
	t: &Tensor,
	k: &Tensor,
	n: &[i64],
	_a: Option<&str>,
) -> Result<Tensor, String> {
	conv_transpose1d(t, k, n[0], n[1], n[2], n[3], n[4])
}
fn conv_transpose2d_any(
	t: &Tensor,
	k: &Tensor,
	n: &[i64],
	_a: Option<&str>,
) -> Result<Tensor, String> {
	conv_transpose2d(t, k, n[0], n[1], n[2], n[3])
}
shim!(s_conv1d, conv1d_any, 6, ints: 4, algo: false);
shim!(s_conv1d_with_algo, conv1d_algo_any, 7, ints: 4, algo: true);
shim!(s_conv2d, conv2d_any, 6, ints: 4, algo: false);
shim!(s_conv2d_with_algo, conv2d_algo_any, 7, ints: 4, algo: true);
shim!(s_conv_transpose1d, conv_transpose1d_any, 7, ints: 5, algo: false);
shim!(s_conv_transpose2d, conv_transpose2d_any, 6, ints: 4, algo: false);

pub(super) fn build(m: &mut rnx::rune::Module) -> Result<(), rnx::rune::ContextError> {
	macro_rules! assoc {
		($($name:ident),*) => {$( m.associated_function(stringify!($name), $name)?; )*};
	}
	m.raw_function("conv1d", s_conv1d)
		.build_associated::<Tensor>()?;
	m.raw_function("conv1d_with_algo", s_conv1d_with_algo)
		.build_associated::<Tensor>()?;
	m.raw_function("conv2d", s_conv2d)
		.build_associated::<Tensor>()?;
	m.raw_function("conv2d_with_algo", s_conv2d_with_algo)
		.build_associated::<Tensor>()?;
	m.raw_function("conv_transpose1d", s_conv_transpose1d)
		.build_associated::<Tensor>()?;
	m.raw_function("conv_transpose2d", s_conv_transpose2d)
		.build_associated::<Tensor>()?;
	assoc!(
		avg_pool2d,
		avg_pool2d_with_stride,
		max_pool2d,
		max_pool2d_with_stride,
		upsample_nearest1d,
		upsample_nearest2d,
		upsample_bilinear2d,
		upsample_bilinear2d_with_scale,
		interpolate1d,
		interpolate2d
	);
	Ok(())
}

pub(super) fn catalogue() -> Vec<(String, &'static str)> {
	vec![
		(
			"candle::Tensor::conv1d".into(),
			"conv1d(kernel, padding, stride, dilation, groups), conv2d(...), *_with_algo(..., algo) -> Result<Tensor>: f32/f64, kernel [out, in/groups, ...]; every size and buffer checked first; algo has no effect on this CPU adapter",
		),
		(
			"candle::Tensor::conv_transpose1d".into(),
			"conv_transpose1d(kernel, padding, output_padding, stride, dilation, groups), conv_transpose2d(kernel, padding, output_padding, stride, dilation): kernel [in, out per group, ...]",
		),
		(
			"candle::Tensor::avg_pool2d".into(),
			"avg_pool2d([kh, kw]), max_pool2d([kh, kw]), *_with_stride([kh, kw], [sh, sw]) -> Result<Tensor>: rank 4 (batch, channels, h, w)",
		),
		(
			"candle::Tensor::upsample_nearest1d".into(),
			"upsample_nearest1d(n), interpolate1d(n), upsample_nearest2d(h, w), interpolate2d(h, w), upsample_bilinear2d(h, w, align_corners), upsample_bilinear2d_with_scale(sh, sw, align_corners)",
		),
	]
}
