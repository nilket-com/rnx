use super::*;
use crate::dense_credit::tensor_bytes;
use rune::experimental_core::domain::file::{FileOutcome, FileProgram, Installer};
use std::sync::{Arc, Mutex};

fn input(dtype: DType) -> CTensor {
	CTensor::from_vec(vec![-1.25f64, -0.0, 0.25, 0.75, 1.5, 2.25], (2, 3), CPU)
		.unwrap()
		.to_dtype(dtype)
		.unwrap()
}
fn cells(t: &CTensor) -> Vec<u64> {
	t.to_dtype(DType::F64)
		.unwrap()
		.flatten_all()
		.unwrap()
		.to_vec1::<f64>()
		.unwrap()
		.into_iter()
		.map(|v| {
			if v.is_nan() {
				f64::NAN.to_bits()
			} else {
				v.to_bits()
			}
		})
		.collect()
}

#[test]
fn checked_family_matches_each_backend_operation_at_both_dtypes() {
	#[derive(rune::Any)]
	#[rune(item = ::candle)]
	struct Fixture(String);
	let make = |f: &Fixture| {
		Ok(Tensor(input(if f.0 == "f32" {
			DType::F32
		} else {
			DType::F64
		})))
	};
	let mut context = rune::Context::with_default_modules().unwrap();
	let mut m = Module::with_crate("candle").unwrap();
	crate::build(&mut m).unwrap();
	fixture_member::enroll(&mut m).unwrap(); // the extra copy method
	m.file_path_constructor(
		"load",
		|s: &str| Ok(Fixture(s.to_owned())),
		|f: &Fixture| f.0.capacity(),
	)
	.unwrap();
	m.opaque_shared_constructor("input", make, move |f: &Fixture, _| Ok(make(f)))
		.unwrap();
	let observed = Arc::new(Mutex::new(Vec::new()));
	let sink = observed.clone();
	m.opaque_method::<Tensor, _, _>(
		"observe",
		move |t: &Tensor| {
			sink.lock().unwrap().push((t.0.dtype(), cells(&t.0)));
		},
		None,
	)
	.unwrap();
	let mut installer = Installer::new(&mut context);
	installer.install(m).unwrap();
	let files = installer.finish().unwrap();
	macro_rules! oracles {
		($($op:ident),*) => { [$( (stringify!($op), CTensor::$op as fn(&CTensor)->candle_core::Result<CTensor>) ),*] };
	}
	for (name, reference) in oracles!(
		exp, log, sqrt, sqr, abs, neg, recip, tanh, relu, gelu, gelu_erf, silu, sin, cos, floor,
		ceil, round, sign, erf, copy
	) {
		let name = if name == "copy" { "fixture_copy" } else { name };
		let source = format!(
			"pub fn main(args){{let f=candle::Fixture::load(args[0])?;let x=candle::Tensor::input(f)?;let y=x.{name}()?;drop(x);y.observe();Ok(())}}"
		);
		for (dtype, argument) in [(DType::F32, "f32"), (DType::F64, "f64")] {
			let mut sources = rune::Sources::new();
			sources
				.insert(rune::Source::memory(&source).unwrap())
				.unwrap();
			let FileProgram::Checked(main) = files
				.compile(
					&mut sources,
					&context,
					&mut rune::Diagnostics::new(),
					1,
					&mut rune::compile::NoopSourceLoader::default(),
				)
				.unwrap()
			else {
				panic!("{name} did not enter the checked file")
			};
			let mut file = main
				.prepare(&[argument.into()], Default::default(), false)
				.unwrap();
			let outcome = file.drive(100000, |_| true, || false).unwrap();
			assert!(matches!(outcome, FileOutcome::Complete), "{name} {dtype:?}");
			file.close().unwrap();
			let expected = cells(&reference(&input(dtype)).unwrap());
			assert_eq!(
				*observed.lock().unwrap(),
				[(dtype, expected)],
				"{name} {dtype:?}"
			);
			observed.lock().unwrap().clear();
		}
	}
}

#[test]
fn retained_unary_bytes_drop_independently() {
	use rnx::allocation::live;
	const CHILD: &str = "RNX_TEST_UNARY_DROP";
	if std::env::var_os(CHILD).is_none() {
		let out = std::process::Command::new(std::env::current_exe().unwrap())
			.args([
				"--exact",
				std::thread::current().name().unwrap(),
				"--nocapture",
			])
			.env(CHILD, "1")
			.output()
			.unwrap();
		assert!(
			out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed"),
			"{}",
			String::from_utf8_lossy(&out.stderr)
		);
		return;
	}
	for dtype in [DType::F32, DType::F64] {
		let x = Tensor(input(dtype));
		let y = super::sqr(&x).unwrap();
		let charges = [tensor_bytes(&x), tensor_bytes(&y)];
		let boxed = [
			rune::runtime::Value::new(x).unwrap(),
			rune::runtime::Value::new(y).unwrap(),
		];
		for (v, charge) in boxed.into_iter().zip(charges) {
			let before = live().unwrap();
			drop(v);
			assert_eq!(before - live().unwrap(), charge, "{dtype:?}");
		}
	}
}
