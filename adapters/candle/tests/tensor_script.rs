//! Record 0134: the tensor surface from a Rune script, as a user writes it.
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> Result<String, String> {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	if unit.is_err() {
		panic!("{:?}", diagnostics.diagnostics());
	}
	let mut vm = Vm::new(runtime, Arc::new(unit.unwrap()));
	let v = vm.call(["main"], ()).map_err(|e| e.to_string())?;
	match rune::from_value::<Result<String, rune::Value>>(v).unwrap() {
		Ok(s) => Ok(s),
		Err(e) => Err(rune::from_value::<String>(e).unwrap_or_else(|_| "non-string error".into())),
	}
}

#[test]
fn a_script_builds_transforms_reduces_ranks_and_reads_back() {
	let got = run(r#"
		pub fn main() {
			let x = candle::Tensor::from_vec([3.0, 1.0, 4.0, 1.0, 5.0, 9.0], [2, 3], "f32")?;
			let y = (x * 2.0)?;
			let z = (y - x)?;                        // tensor - tensor
			let m = z.mean_keepdim(-1)?;            // [2, 1]
			let centred = z.broadcast_sub(m)?;
			let s = centred.sqr()?.sum_all()?.to_scalar()?;
			let (v, i) = x.flatten_all()?.topk(2, false)?;
			let idx = i.to_vec()?;
			let mask = x.gt(2.0)?.to_vec2()?;
			// every argument still usable
			Ok(format!("{} {:?} {:?} {:?} {:?} {}", s, v.to_vec()?, idx, mask, x.dims(), z.rank()))
		}
	"#);
	assert_eq!(
		got.unwrap(),
		"36.66666793823242 [9.0, 5.0] [5, 4] [[1, 0, 1], [0, 1, 1]] [2, 3] 2"
	);
}

#[test]
fn refusals_are_script_errors_and_arguments_survive_them() {
	let e = run(r#"
		pub fn main() {
			let a = candle::Tensor::ones([2, 2], "f32")?;
			let b = candle::Tensor::ones([3], "f32")?;
			let r = a.broadcast_add(b);
			if r.is_ok() { return Ok(`broadcast should fail`); }
			let a2 = (a + a)?;
			a.matmul(b)?;
			Ok(`unreachable ${a2.dims()}`)
		}
	"#)
	.unwrap_err();
	assert!(
		e.contains("Tensor::matmul: both operands need rank 2"),
		"{e}"
	);
	let e =
		run(r#"pub fn main() { candle::Tensor::arange(0, 5, "u8")?.to_dtype("f16")?; Ok(``) }"#)
			.unwrap_err();
	assert!(e.contains("dtype \"f16\""), "{e}");
	let e = run(r#"pub fn main() { let t = candle::Tensor::from_vec([1.0], [1], "f32")?; let x = (t + "a")?; Ok(``) }"#).unwrap_err();
	assert!(e.contains("expected a number"), "{e}");
}

#[test]
fn softmax_and_correlation_compose() {
	let got = run(r#"
		pub fn main() {
			let x = candle::Tensor::from_vec([1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 0.0, 2.0], [3, 3], "f64")?;
			let p = candle::softmax(x, -1)?.sum(1)?.to_vec()?;
			let z = x.broadcast_sub(x.mean_keepdim(0)?)?.broadcast_div(x.var_keepdim(0)?.sqrt()?)?;
			let corr = (z.t()?.matmul(z)? * 0.5)?;     // 1 / (n - 1)
			let diag = corr.to_vec2()?;
			Ok(format!("{:?} {} {}", p, diag[0][0], diag[1][1]))
		}
	"#);
	let got = got.unwrap();
	assert!(
		got.starts_with("[1.0, 1.0, 1.0]") || got.starts_with("[0.9999999999999999, 1.0"),
		"{got}"
	);
	assert!(
		got.ends_with(" 1.0 1.0") || got.contains(" 0.9999999999999998"),
		"{got}"
	);
}
