//! Record 0146: `CrossEncoder::score` refuses oversized, unequal, over-long
//! and over-total inputs before allocating anything proportional to them,
//! observed through the allocator's peak (the inputs exist first). One test,
//! so no parallel test moves the peak. Uses the unit tests' tiny fixture.
#![cfg(feature = "test-support")]
use rnx::allocation::{peak, reset_peak};
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
		pub fn load(d) { candle::CrossEncoder::load(d) }
		pub fn score(ce, q, p) { ce.score(q, p) }
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

fn refused(vm: &mut Vm, ce: &Value, q: Value, p: Value, want: &str) {
	reset_peak();
	let base = peak();
	let out = result(vm.call(["score"], (ce.clone(), q, p)).unwrap());
	let used = peak() - base;
	let e = out.unwrap_err();
	assert!(e.contains(want), "{e} (wanted {want})");
	assert!(used < SMALL, "the refusal ({want}) allocated {used} bytes");
}

fn strings(n: usize, s: &str) -> Value {
	rune::to_value(vec![s.to_string(); n]).unwrap()
}

#[test]
fn refusals_allocate_nothing_proportional() {
	let mut vm = vm();
	let ce = result(
		vm.call(["load"], (rnx_candle::text::rerank::fixture_dir(),))
			.unwrap(),
	)
	.unwrap();
	let max = rnx_candle::text::MAX_TEXTS;
	refused(
		&mut vm,
		&ce,
		strings(max + 1, "the"),
		strings(max + 1, "cat"),
		"32769 pairs, want 1 to 32768",
	);
	refused(
		&mut vm,
		&ce,
		strings(max, "the"),
		strings(max - 1, "cat"),
		"32768 queries and 32767 passages",
	);
	let long = "a ".repeat(rnx_candle::text::MAX_TEXT / 2 + 1);
	refused(
		&mut vm,
		&ce,
		strings(3, "the"),
		strings(3, &long),
		"passage 0 is",
	);
	// within every per-text bound, over the combined one
	let half = "a ".repeat(rnx_candle::text::MAX_TEXT / 2 - 1);
	let n = rnx_candle::text::MAX_TOTAL / (2 * half.len()) + 1;
	refused(
		&mut vm,
		&ce,
		strings(n, &half),
		strings(n, &half),
		"bytes of text in all",
	);
}
