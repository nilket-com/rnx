//! Record 0134: every expanding operation is refused before Candle
//! allocates, observed through the allocator's peak. One test, so no
//! parallel test moves the peak.
use rnx::allocation::{peak, reset_peak};
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

const SMALL: usize = 1 << 20;

#[test]
fn oversized_results_are_refused_before_any_proportional_allocation() {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let script = r#"
		pub fn broadcast() { let a = candle::Tensor::zeros([4097, 1], "f32")?; let b = candle::Tensor::zeros([1, 4097], "f32")?; a.broadcast_mul(b) }
		pub fn expanded() { let a = candle::Tensor::zeros([2048, 2048], "f32")?; let b = candle::Tensor::zeros([5, 2048, 1], "f32")?; a.broadcast_matmul(b) }
		pub fn eye() { candle::Tensor::eye(5000, "f32") }
		pub fn arange() { candle::Tensor::arange(0, 20000000, "i64") }
		pub fn zeros() { candle::Tensor::zeros([5000, 5000], "f64") }
		pub fn control() { candle::Tensor::zeros([1024, 1024], "f32") }
		pub fn input() { candle::Tensor::ones([1024, 1024], "f32") }
		pub fn topk(t, k) { t.topk(k, false) }
	"#;
	let mut sources = Sources::new();
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	let mut vm = Vm::new(runtime, Arc::new(unit));
	// topk validates k before any copy or scan: the input is built first,
	// then each bad k is measured on its own (review round 1, R4)
	let input = vm.call(["input"], ()).unwrap();
	let input = rune::from_value::<Result<rune::Value, rune::Value>>(input)
		.unwrap()
		.unwrap();
	for k in [-1i64, 0, 1025] {
		reset_peak();
		let base = peak();
		let v = vm.call(["topk"], (input.clone(), k)).unwrap();
		let ok = rune::from_value::<Result<rune::Value, rune::Value>>(v)
			.unwrap()
			.is_ok();
		let grew = peak() - base;
		assert!(!ok, "topk k = {k} was accepted");
		assert!(grew < SMALL, "topk k = {k} allocated {grew}");
	}
	// the receiver is reusable afterwards
	let v = vm.call(["topk"], (input, 3i64)).unwrap();
	assert!(
		rune::from_value::<Result<rune::Value, rune::Value>>(v)
			.unwrap()
			.is_ok()
	);
	let mut observe = |name: &str| {
		reset_peak();
		let base = peak();
		let v = vm.call([name], ()).unwrap();
		let ok = rune::from_value::<Result<rune::Value, rune::Value>>(v)
			.unwrap()
			.is_ok();
		(peak() - base, ok)
	};
	// the positive control: an accepted 4 MiB tensor is visible
	let (grew, ok) = observe("control");
	assert!(ok && grew >= 4 << 20, "{grew}");
	for name in ["broadcast", "expanded", "eye", "arange", "zeros"] {
		let (grew, ok) = observe(name);
		assert!(!ok, "{name} was accepted");
		// the inputs of `expanded` (16 MiB) are built before the refusal,
		// so its bound is the inputs plus a little
		let bound = if name == "expanded" {
			(16 << 20) + SMALL
		} else {
			SMALL
		};
		assert!(grew < bound, "{name} allocated {grew}");
	}
}
