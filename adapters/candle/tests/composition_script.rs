//! Record 0137, review round 1: `slice_assign` borrows the script's range
//! containers; it never takes them. A script reads its ranges after a
//! success and after each kind of refusal, in both accepted forms.
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;

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
		fn t() { candle::Tensor::zeros([3, 4], "f32") }
		fn s(r, c) { candle::Tensor::ones([r, c], "f32") }
		// the ranges are read again after the call, whatever it returned
		fn reread(ranges, src) {
			let out = t()?.slice_assign(ranges, src);
			Ok((out.is_ok(), ranges[0][0], ranges[0][1], ranges[1][0], ranges[1][1], ranges.len()))
		}
		pub fn vector_ok() { reread([[0, 2], [1, 3]], s(2, 2)?) }
		pub fn tuple_ok() { reread([(0, 2), (1, 3)], s(2, 2)?) }
		pub fn vector_later_range_refused() { reread([[0, 2], [3, 9]], s(2, 2)?) }
		pub fn tuple_later_range_refused() { reread([(0, 2), (3, 9)], s(2, 2)?) }
		pub fn vector_source_refused() { reread([[0, 2], [1, 3]], s(3, 3)?) }
		pub fn tuple_source_refused() { reread([(0, 2), (1, 3)], s(3, 3)?) }
	"#;
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	Vm::new(runtime, Arc::new(unit))
}

fn call(vm: &mut Vm, f: &str) -> (bool, i64, i64, i64, i64, i64) {
	let v: Value = vm.call([f], ()).unwrap();
	let ok = rune::from_value::<Result<Value, Value>>(v)
		.unwrap()
		.unwrap();
	rune::from_value(ok).unwrap()
}

#[test]
fn the_ranges_stay_the_scripts_after_success_and_refusal() {
	let mut vm = vm();
	for (f, ok, second) in [
		("vector_ok", true, (1, 3)),
		("tuple_ok", true, (1, 3)),
		("vector_later_range_refused", false, (3, 9)),
		("tuple_later_range_refused", false, (3, 9)),
		("vector_source_refused", false, (1, 3)),
		("tuple_source_refused", false, (1, 3)),
	] {
		let got = call(&mut vm, f);
		assert_eq!(got, (ok, 0, 2, second.0, second.1, 2), "{f}");
	}
}
