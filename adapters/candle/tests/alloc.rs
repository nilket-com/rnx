//! Record 0129: `tensor.to_dense(names)` refuses a bad name list before it
//! copies a name or exports the tensor, observed through the allocator's
//! peak. One test, so no parallel test moves the peak.
use rnx::allocation::{peak, reset_peak};
use rnx::interchange::{Data, Dense};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;

const SIDE: usize = 512;
/// A refusal's whole footprint stays far under the 1 MiB export.
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
		pub fn tensor(block) { candle::Tensor::from_dense(block) }
		pub fn export(t, names) { t.to_dense(names) }
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

fn observe(vm: &mut Vm, t: &Value, names: Value) -> (usize, Result<Value, String>) {
	reset_peak();
	let base = peak();
	let out = result(vm.call(["export"], (t.clone(), names)).unwrap());
	(peak() - base, out)
}

#[test]
fn refusals_come_before_the_export() {
	let mut vm = vm();
	let names: Vec<String> = (0..SIDE).map(|i| format!("c{i}")).collect();
	let block = Dense::new(
		Data::F32(Arc::new(vec![0.5; SIDE * SIDE])),
		SIDE,
		SIDE,
		names.clone(),
	)
	.unwrap();
	let t = result(vm.call(["tensor"], (block,)).unwrap()).unwrap();
	let list = |v: Vec<String>| rune::to_value(v).unwrap();

	// the control: an accepted export is visible (512 x 512 f32, 1 MiB)
	let (grew, out) = observe(&mut vm, &t, list(names.clone()));
	assert!(out.is_ok());
	assert!(
		grew >= SIDE * SIDE * 4,
		"the observation sees the export: {grew}"
	);

	// an 8 MiB name is refused before it is copied or the tensor exported
	let mut long = names.clone();
	long[SIDE - 1] = "n".repeat(8 << 20);
	let (grew, out) = observe(&mut vm, &t, list(long));
	assert!(out.unwrap_err().contains("1 to 256 bytes"));
	assert!(grew < SMALL, "long-name refusal allocated {grew}");

	// a 100,000-entry list is refused from its length
	let (grew, out) = observe(&mut vm, &t, list(vec!["a".into(); 100_000]));
	assert!(out.unwrap_err().contains("100000 names, at most 4096"));
	assert!(grew < SMALL, "long-list refusal allocated {grew}");

	// a duplicate is refused before the export
	let mut dup = names.clone();
	dup[1] = dup[0].clone();
	let (grew, out) = observe(&mut vm, &t, list(dup));
	assert!(out.unwrap_err().contains("duplicate column name"));
	assert!(grew < SMALL, "duplicate refusal allocated {grew}");
}
