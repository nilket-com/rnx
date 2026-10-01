//! Record 0137: oversized shape, composition and pooling calls are refused
//! before Candle allocates, observed through the allocator's peak from
//! after the inputs exist. One test, so no parallel test moves the peak.
use rnx::allocation::{peak, reset_peak};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;

/// A refusal's whole footprint stays far under the refused allocation.
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
		pub fn ids(n) { candle::Tensor::zeros([n], "u32") }
		pub fn repeat(t, m) { t.repeat(m) }
		pub fn cat(a, b) { candle::Tensor::cat([a, b], 0) }
		pub fn unfold(t, size, step) { t.unfold(0, size, step) }
		pub fn cumsum(t) { t.cumsum(0) }
		pub fn segment_mean(v, s, n) { candle::segment_mean(v, s, n) }
		fn list(n) { let xs = []; for _ in 0..n { xs.push(0); } xs }
		pub fn long_list(n) { Ok(list(n)) }
		pub fn ranges_with_long_inner(n) { Ok([list(n), [0, 1]]) }
		pub fn slice_assign(t, ranges, src) { t.slice_assign(ranges, src) }
		pub fn flip(t, axes) { t.flip(axes) }
		pub fn log_sum_exp(t, axes) { t.log_sum_exp(axes) }
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

fn make(vm: &mut Vm, f: &str, arg: impl rune::ToValue) -> Value {
	result(vm.call([f], (arg,)).unwrap()).unwrap()
}

/// The call is refused with `want`, allocating under `SMALL`.
fn refused(vm: &mut Vm, f: &str, args: impl rune::runtime::GuardedArgs, want: &str) {
	reset_peak();
	let base = peak();
	let out = result(vm.call([f], args).unwrap());
	let used = peak() - base;
	let e = out.unwrap_err();
	assert!(e.contains(want), "{f}: {e} (wanted {want})");
	assert!(used < SMALL, "{f}: the refusal allocated {used} bytes");
}

#[test]
fn oversized_outputs_are_refused_before_candle_allocates() {
	let mut vm = vm();
	let one = make(&mut vm, "zeros", vec![1i64, 1]);
	refused(
		&mut vm,
		"repeat",
		(one, vec![4096i64, 4097]),
		"exceeds 16777216 values",
	);
	let half = make(&mut vm, "zeros", vec![(1i64 << 23) + 1]);
	refused(
		&mut vm,
		"cat",
		(half.clone(), half),
		"exceeds 16777216 values",
	);
	let line = make(&mut vm, "zeros", vec![8192i64]);
	refused(
		&mut vm,
		"unfold",
		(line, 4096i64, 1i64),
		"exceeds 16777216 values",
	);
	let long = make(&mut vm, "zeros", vec![5000i64]);
	refused(&mut vm, "cumsum", (long,), "5000 x 5000");
	let wide = make(&mut vm, "zeros", vec![1i64, 4096]);
	let seg = make(&mut vm, "ids", 1i64);
	// review round 1: a malformed container is refused before anything
	// proportional to its length is allocated
	let small = make(&mut vm, "zeros", vec![2i64, 2]);
	let src = make(&mut vm, "zeros", vec![1i64, 1]);
	let ranges = make(&mut vm, "ranges_with_long_inner", 131_072i64);
	refused(
		&mut vm,
		"slice_assign",
		(small.clone(), ranges, src),
		"needs 2 ranges",
	);
	let axes = make(&mut vm, "long_list", 131_072i64);
	refused(
		&mut vm,
		"flip",
		(small.clone(), axes.clone()),
		"131072 axes",
	);
	refused(&mut vm, "log_sum_exp", (small, axes), "131072 axes");
	refused(
		&mut vm,
		"segment_mean",
		(wide, seg, 4097i64),
		"exceeds 16777216 values",
	);
}
