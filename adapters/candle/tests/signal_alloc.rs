//! Record 0145: each signal path's temporaries are bounded before Candle
//! allocates (refusals forced by one temporary alone, with small inputs and
//! outputs, allocating under `SMALL`), and the census is an upper bound (a
//! successful call's measured peak within its predicted bytes). One test, so
//! no parallel test moves the peak.
use rnx::allocation::{live, peak, reset_peak};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;

const SMALL: usize = 256 << 10;

fn vm() -> Vm {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	let script = r#"
		pub fn zeros(shape) { candle::Tensor::zeros(shape, "f32") }
		pub fn conv1d(x, k, p) { x.conv1d(k, p, 1, 1, 1) }
		pub fn conv2d(x, k, p) { x.conv2d(k, p, 1, 1, 1) }
		pub fn convtr1d(x, k) { x.conv_transpose1d(k, 0, 0, 1, 1, 1) }
		pub fn scale(x, s) { x.upsample_bilinear2d_with_scale(s, 1.0, false) }
		pub fn conv2d0(x, k) { x.conv2d(k, 0, 1, 1, 1) }
		pub fn convtr1dp(x, k, p) { x.conv_transpose1d(k, p, 0, 1, 1, 1) }
		pub fn convtr2dp(x, k, p) { x.conv_transpose2d(k, p, 0, 1, 1) }
		pub fn bilinear(x, h) { x.upsample_bilinear2d(h, 1, false) }
	"#;
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	Vm::new(runtime, Arc::new(unit))
}

fn result(v: Value) -> Result<Value, String> {
	rune::from_value::<Result<Value, Value>>(v)
		.unwrap()
		.map_err(|e| rune::from_value::<String>(e).unwrap())
}

fn make(vm: &mut Vm, shape: &[i64]) -> Value {
	result(vm.call(["zeros"], (shape.to_vec(),)).unwrap()).unwrap()
}

/// Refused with `want`, allocating under `SMALL` (the inputs exist first).
fn refused(vm: &mut Vm, f: &str, args: impl rune::runtime::GuardedArgs, want: &str) {
	reset_peak();
	let base = peak();
	let out = result(vm.call([f], args).unwrap());
	let used = peak() - base;
	let e = out.unwrap_err();
	assert!(e.contains(want), "{f}: {e} (wanted {want})");
	assert!(used < SMALL, "{f}: the refusal allocated {used} bytes");
}

/// Succeeds, its peak within `predicted` values of f32 (4 bytes each) plus
/// the result's own Rune handle.
fn within(vm: &mut Vm, f: &str, args: impl rune::runtime::GuardedArgs, predicted: usize) -> usize {
	reset_peak();
	let base = peak();
	let out = result(vm.call([f], args).unwrap());
	let used = peak() - base;
	assert!(out.is_ok(), "{f}: {:?}", out.err());
	assert!(
		used <= predicted * 4 + SMALL,
		"{f}: measured {used} bytes above the census's {} predicted",
		predicted * 4
	);
	used
}

#[test]
fn signal_temporaries_are_bounded_and_the_census_is_an_upper_bound() {
	let mut vm = vm();
	// The one-time cost, measured apart: gemm (0.19) keeps a thread-local
	// L2-sized slab per thread (gemm-common gemm.rs `L2_SLAB`), allocated on
	// each pool thread's first multiply and kept for the process. Warming
	// every pool thread through rnx's own call path shows it as persistent
	// growth in live memory that a second warm-up doesn't repeat.
	let (wx, wk) = (
		make(&mut vm, &[1, 64, 256, 256]),
		make(&mut vm, &[64, 64, 3, 3]),
	);
	let before = live().unwrap();
	result(vm.call(["conv2d"], (wx.clone(), wk.clone(), 1i64)).unwrap()).unwrap();
	let slabs = live().unwrap() - before;
	let again = live().unwrap();
	for _ in 0..2 {
		result(vm.call(["conv2d"], (wx.clone(), wk.clone(), 1i64)).unwrap()).unwrap();
	}
	let regrowth = live().unwrap().saturating_sub(again);
	eprintln!(
		"one-time gemm slabs: {slabs} bytes persistent after the first warm-up; {regrowth} more after two further warm-ups"
	);
	assert!(regrowth < SMALL, "the slabs are allocated once");
	drop((wx, wk));

	// conv1d: the im2col column alone over the cap (4,097 × 4,096); the
	// input (8,192) and output (4,097) are small
	let x = make(&mut vm, &[1, 1, 8192]);
	let k = make(&mut vm, &[1, 1, 4096]);
	refused(
		&mut vm,
		"conv1d",
		(x, k, 0i64),
		"the im2col column would hold 16781312 values",
	);
	// conv2d, any non-default 1×1: the full im2col column alone (Codex's
	// case: a 513 × 513 output within the cap, a column of 16,842,816)
	let x = make(&mut vm, &[1, 64, 1, 1]);
	let k = make(&mut vm, &[1, 64, 1, 1]);
	refused(
		&mut vm,
		"conv2d",
		(x, k, 256i64),
		"the im2col column would hold 16842816 values",
	);
	// conv2d, tiled: every tile fits, the sum over all tiles doesn't
	let x = make(&mut vm, &[1, 64, 4, 4]);
	let k = make(&mut vm, &[1, 64, 3, 3]);
	refused(
		&mut vm,
		"conv2d",
		(x, k, 254i64),
		"every tile's column, coordinates and result",
	);
	// conv_transpose1d, the col2im column alone (Codex's case: 64 × 1,024 ×
	// 1,024 = 67,108,864)
	let x = make(&mut vm, &[1, 1, 64]);
	let k = make(&mut vm, &[1, 1024, 1024]);
	refused(
		&mut vm,
		"convtr1d",
		(x, k),
		"the col2im column would hold 67108864 values",
	);
	// an infinite scale, before anything
	let x = make(&mut vm, &[1, 1, 4, 4]);
	refused(&mut vm, "scale", (x, f64::INFINITY), "finite and positive");

	// the census as an upper bound, one successful call per path; each
	// prediction is the census's own formula for these shapes
	// conv1d [1, 4, 2000] * [8, 4, 9], padding 4: copies 8,000 + 288; column
	// 2,000 × 36; result and copy 8 × 2,000 each
	let x = make(&mut vm, &[1, 4, 2000]);
	let k = make(&mut vm, &[8, 4, 9]);
	within(
		&mut vm,
		"conv1d",
		(x, k, 4i64),
		8000 + 288 + 2000 * 36 + 2 * 8 * 2000,
	);
	// conv2d tiled [1, 8, 64, 64] * [4, 8, 3, 3], padding 1: copies 32,768 +
	// 288; NHWC 32,768; flat kernel 288; output 16,384; tiles 8 × (72 + 4) ×
	// 512 + coordinates (8 × 2,048 values), summed
	let x = make(&mut vm, &[1, 8, 64, 64]);
	let k = make(&mut vm, &[4, 8, 3, 3]);
	let tiles = 8 * ((72 + 4) * 512 + 2048);
	within(
		&mut vm,
		"conv2d",
		(x, k, 1i64),
		32768 + 288 + 32768 + 288 + 16384 + (72 + 4) * 512 + 2048 + tiles,
	);
	// conv2d, a non-default 1×1 (full im2col): [1, 8, 32, 32] * [4, 8, 1, 1],
	// padding 2: copies 8,192 + 32; column 36 × 36 × 8; result and copy 4 ×
	// 1,296 each
	let x = make(&mut vm, &[1, 8, 32, 32]);
	let k = make(&mut vm, &[4, 8, 1, 1]);
	within(
		&mut vm,
		"conv2d",
		(x, k, 2i64),
		8192 + 32 + 36 * 36 * 8 + 2 * 4 * 1296,
	);
	// conv_transpose1d, col2im: [1, 4, 500] * [4, 6, 5]: copies 2,000 + 120;
	// column 500 × 6 × 5; output 6 × 504
	let x = make(&mut vm, &[1, 4, 500]);
	let k = make(&mut vm, &[4, 6, 5]);
	within(&mut vm, "convtr1d", (x, k), 2000 + 120 + 500 * 30 + 6 * 504);
	// review round 1: the paths the first submission did not exercise
	// R1, bilinear's tables at 24 bytes an entry: Codex's success case
	// (input 1, height table 2,000,000 × 24 bytes = 12,000,000 f32 values,
	// width table 6, output 2,000,000), and the refusal one table forces
	let x = make(&mut vm, &[1, 1, 1, 1]);
	within(
		&mut vm,
		"bilinear",
		(x.clone(), 2_000_000i64),
		1 + 12_000_000 + 6 + 2_000_000,
	);
	refused(
		&mut vm,
		"bilinear",
		(x, 4_194_303i64),
		"the height table would hold 25165818 values",
	);
	// R2, the fast 1×1 path with many batch items (Codex's case): input copy
	// 8,388,608, kernel 128, per-batch reshaped inputs 8,388,608, output and
	// per-batch results 65,536 each
	let x = make(&mut vm, &[16, 128, 64, 64]);
	let k = make(&mut vm, &[1, 128, 1, 1]);
	within(
		&mut vm,
		"conv2d0",
		(x, k),
		8_388_608 + 128 + 8_388_608 + 65_536 + 65_536,
	);
	// R3, the direct transposed paths (Codex's cases): their own input copy
	// and kernel rows beside the canonical copies
	// 1-D: copies 1,024,000 + 3,072; direct input 1,024,000; rows 1 × 1,024;
	// output 1 × 1 × 4 (999 − 998 + 2 + 1)
	let x = make(&mut vm, &[1, 1024, 1000]);
	let k = make(&mut vm, &[1024, 1, 3]);
	within(
		&mut vm,
		"convtr1dp",
		(x, k, 499i64),
		1_024_000 + 3_072 + 1_024_000 + 1_024 + 4,
	);
	// 2-D: copies 262,144 + 576; output 1 × 1 × 2 × 2; direct input 262,144;
	// rows 1 × 64
	let x = make(&mut vm, &[1, 64, 64, 64]);
	let k = make(&mut vm, &[64, 1, 3, 3]);
	within(
		&mut vm,
		"convtr2dp",
		(x, k, 32i64),
		262_144 + 576 + 4 + 262_144 + 64,
	);
}
