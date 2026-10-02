//! Record 0149: `TextGenerator::chat_many` refuses oversized, unequal,
//! over-long, over-total and out-of-range inputs before allocating anything
//! proportional to them, observed through the allocator's peak (the inputs
//! exist first). One test, so no parallel test moves the peak. Uses the unit
//! tests' tiny fixture.
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
		pub fn load(d) { candle::TextGenerator::load(d) }
		pub fn many(g, s, u, m) { g.chat_many(s, u, m) }
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

fn refused(vm: &mut Vm, g: &Value, s: Value, u: Value, m: i64, want: &str) {
	reset_peak();
	let base = peak();
	let out = result(vm.call(["many"], (g.clone(), s, u, m)).unwrap());
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
	let g = result(
		vm.call(["load"], (rnx_candle::text::generate::fixture_dir(),))
			.unwrap(),
	)
	.unwrap();
	let max = rnx_candle::text::generate::MAX_REQUESTS;
	refused(
		&mut vm,
		&g,
		strings(max + 1, "a"),
		strings(max + 1, "b"),
		4,
		"1025 chats, want 1 to 1024",
	);
	refused(
		&mut vm,
		&g,
		strings(max, "a"),
		strings(max - 1, "b"),
		4,
		"1024 systems and 1023 users",
	);
	let long = "a ".repeat(rnx_candle::text::MAX_TEXT / 2 + 1);
	refused(
		&mut vm,
		&g,
		strings(3, "a"),
		strings(3, &long),
		4,
		"user 0 is",
	);
	let half = "a ".repeat(rnx_candle::text::MAX_TEXT / 2 - 1);
	let n = rnx_candle::text::MAX_TOTAL / (2 * half.len()) + 1;
	refused(
		&mut vm,
		&g,
		strings(n, &half),
		strings(n, &half),
		4,
		"bytes of text in all",
	);
	refused(
		&mut vm,
		&g,
		strings(2, "a"),
		strings(2, "b"),
		0,
		"max_new_tokens 0, want 1 to 256",
	);
	refused(
		&mut vm,
		&g,
		strings(2, "a"),
		strings(2, "b"),
		257,
		"max_new_tokens 257, want 1 to 256",
	);
}
