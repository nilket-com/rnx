use super::*;
use rune::experimental_core::domain::file::{FileOutcome, FileProgram, Installer};
use rune::experimental_core::heap::ViewKind;
use std::sync::Mutex;

#[derive(Any)]
#[rune(item = ::candle)]
struct Fixture(String);

const MODEL: &str = concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/../../demos/data/mlp.safetensors"
);
const FEATURES: &str = include_str!("../../../demos/data/features.csv");

fn features() -> Vec<f32> {
	FEATURES
		.lines()
		.skip(1)
		.flat_map(|row| row.split(',').skip(1).map(|v| v.parse::<f32>().unwrap()))
		.collect()
}

fn input(f: &Fixture) -> Result<Tensor, String> {
	let (shape, dtype) = match f.0.as_str() {
		"dtype" => (vec![3], DType::F64),
		"rank" => (vec![3], DType::F32),
		"columns" => (vec![64, 2], DType::F32),
		"columns-before-size" => (vec![65537, 2], DType::F32),
		"hidden" => (vec![65537, 1], DType::F32),
		"out" => (vec![4097, 1], DType::F32),
		"features" => {
			return CTensor::from_vec(features(), (64, 3), &Device::Cpu)
				.map(Tensor)
				.map_err(cerr("fixture"));
		}
		other => panic!("unknown input {other}"),
	};
	CTensor::zeros(shape, dtype, &Device::Cpu)
		.map(Tensor)
		.map_err(cerr("fixture"))
}

#[test]
fn checked_pair_scores_and_ordered_errors() {
	let seen = Arc::new(Mutex::new(Vec::new()));
	let mut context = rune::Context::with_default_modules().unwrap();
	let mut module = Module::with_crate("candle").unwrap();
	build(&mut module).unwrap();
	module
		.file_path_constructor(
			"load",
			|s: &str| Ok(Fixture(s.to_owned())),
			|f: &Fixture| f.0.capacity(),
		)
		.unwrap();
	module
		.opaque_shared_constructor("fixture_input", input, |f: &Fixture, _| Ok(input(f)))
		.unwrap();
	let sink = seen.clone();
	module
		.opaque_method::<Tensor, _, _>(
			"observe",
			move |t: &Tensor| {
				*sink.lock().unwrap() = t.0.flatten_all().unwrap().to_vec1::<f32>().unwrap();
			},
			None,
		)
		.unwrap();
	let mut install = Installer::new(&mut context);
	install.install(module).unwrap();
	let files = install.finish().unwrap();
	let run = |name: &str, path: &str, body: &str, error: Option<&str>| {
		let text = format!(
			"pub fn main(args){{let m=candle::Mlp::load(args[0])?;let f=candle::Fixture::load(args[1])?;let x=candle::Tensor::fixture_input(f)?;{body}}}"
		);
		let mut src = rune::Sources::new();
		src.insert(rune::Source::memory(text.as_str()).unwrap())
			.unwrap();
		let FileProgram::Checked(main) = files
			.compile(
				&mut src,
				&context,
				&mut rune::Diagnostics::new(),
				1,
				&mut rune::compile::NoopSourceLoader::default(),
			)
			.unwrap_or_else(|e| panic!("{name}: {e:?}"))
		else {
			panic!("ordinary file")
		};
		let mut file = main
			.prepare(&[path.into(), name.into()], Default::default(), false)
			.unwrap();
		match file.drive(100000, |_| true, || false).unwrap() {
			FileOutcome::Complete => (),
			FileOutcome::Failed { message, .. } => panic!("{name}: {message}"),
			_ => panic!("unexpected control transfer"),
		}
		if let Some(expected) = error {
			let view = file.result(Default::default()).unwrap();
			let root = view.root().unwrap();
			assert_eq!(
				root.kind().unwrap(),
				ViewKind::Result { ok: false },
				"{name}"
			);
			assert_eq!(
				root.child(0).unwrap().kind().unwrap(),
				ViewKind::String(expected),
				"{name}"
			);
		}
		file.close().unwrap();
	};
	run(
		"features",
		MODEL,
		"let y=m.forward(x)?; drop(m);drop(x);y.observe();42",
		None,
	);
	let weights = candle_core::safetensors::load(MODEL, &Device::Cpu).unwrap();
	let data = |k: &str| weights[k].flatten_all().unwrap().to_vec1::<f32>().unwrap();
	let (w1, b1, w2, b2) = (
		data("fc1.weight"),
		data("fc1.bias"),
		data("fc2.weight"),
		data("fc2.bias"),
	);
	let expected: Vec<f32> = features()
		.as_chunks::<3>()
		.0
		.iter()
		.map(|x| {
			let mut sum = b2[0];
			for (i, w) in w1.as_chunks::<3>().0.iter().enumerate() {
				let h = w.iter().zip(x).fold(b1[i], |s, (a, b)| s + a * b);
				sum += h.max(0.0) * w2[i];
			}
			sum
		})
		.collect();
	assert_eq!(*seen.lock().unwrap(), expected);
	let wide = super::tests::weights(super::tests::good(1, 256, 256));
	let out = super::tests::weights(super::tests::good(1, 1, 4096));
	for (id, path, error) in [
		("dtype", MODEL, "input must be F32, found F64"),
		("rank", MODEL, "input must be 2-D, found rank 1"),
		("columns", MODEL, "input has 2 columns, the model takes 3"),
		(
			"columns-before-size",
			&wide,
			"input has 2 columns, the model takes 1",
		),
		(
			"hidden",
			&wide,
			"65537 x 256 hidden values, at most 16777216",
		),
		("out", &out, "4097 x 4096 out values, at most 16777216"),
	] {
		run(
			id,
			path,
			"let y=m.forward(x)?; y.rank()",
			Some(&format!("Mlp::forward: {error}")),
		);
	}
	std::fs::remove_file(wide).unwrap();
	std::fs::remove_file(out).unwrap();
}
