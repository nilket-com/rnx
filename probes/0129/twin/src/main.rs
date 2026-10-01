//! Record 0129: the workflow's Rust twin, and its deterministic inputs.
//!
//!   twin0129 write <dir>   features.csv (64 rows) and mlp.safetensors (3 -> 8 -> 1, fixed weights)
//!   twin0129 check <dir>   the same steps in Rust; compares out.parquet, written by the script, bit for bit
use candle_core::{Device, Module, Tensor};
use polars::prelude::*;
use std::collections::HashMap;

const ROWS: usize = 64;
const IN: usize = 3;
const HIDDEN: usize = 8;
const OUT: usize = 1;

fn write(dir: &str) {
	let mut csv = String::from("id,a,b,c\n");
	for i in 0..ROWS {
		let (a, b, c) = (
			i as f64 * 0.25,
			(i % 7) as f64 - 3.0,
			((i * 37) % 11) as f64 / 8.0,
		);
		csv.push_str(&format!("{i},{a},{b},{c}\n"));
	}
	std::fs::write(format!("{dir}/features.csv"), csv).unwrap();
	let t = |v: Vec<f32>, s: &[usize]| Tensor::from_vec(v, s, &Device::Cpu).unwrap();
	let w: HashMap<String, Tensor> = [
		(
			"fc1.weight",
			t(
				(0..HIDDEN * IN)
					.map(|i| ((i * 13) % 9) as f32 * 0.125 - 0.5)
					.collect(),
				&[HIDDEN, IN],
			),
		),
		(
			"fc1.bias",
			t(
				(0..HIDDEN).map(|i| i as f32 * 0.0625 - 0.25).collect(),
				&[HIDDEN],
			),
		),
		(
			"fc2.weight",
			t(
				(0..OUT * HIDDEN)
					.map(|i| ((i * 5) % 7) as f32 * 0.25 - 0.75)
					.collect(),
				&[OUT, HIDDEN],
			),
		),
		("fc2.bias", t(vec![0.5], &[OUT])),
	]
	.into_iter()
	.map(|(k, v)| (k.to_string(), v))
	.collect();
	candle_core::safetensors::save(&w, format!("{dir}/mlp.safetensors")).unwrap();
}

/// The twin: Polars reads the same file with its own inference, the three
/// feature columns go row-major as f32, the same MLP runs on the CPU.
fn twin(dir: &str) -> Vec<f32> {
	let df = CsvReadOptions::default()
		.with_has_header(true)
		.try_into_reader_with_file_path(Some(format!("{dir}/features.csv").into()))
		.unwrap()
		.finish()
		.unwrap();
	let mut x = Vec::with_capacity(ROWS * IN);
	let cols: Vec<Vec<f64>> = ["a", "b", "c"]
		.iter()
		.map(|c| {
			df.column(c)
				.unwrap()
				.cast(&DataType::Float64)
				.unwrap()
				.f64()
				.unwrap()
				.into_no_null_iter()
				.collect()
		})
		.collect();
	for r in 0..df.height() {
		for c in &cols {
			x.push(c[r] as f32);
		}
	}
	let x = Tensor::from_vec(x, (df.height(), IN), &Device::Cpu).unwrap();
	let w = candle_core::safetensors::load(format!("{dir}/mlp.safetensors"), &Device::Cpu).unwrap();
	let fc1 = candle_nn::Linear::new(w["fc1.weight"].clone(), Some(w["fc1.bias"].clone()));
	let fc2 = candle_nn::Linear::new(w["fc2.weight"].clone(), Some(w["fc2.bias"].clone()));
	let y = fc2
		.forward(&fc1.forward(&x).unwrap().relu().unwrap())
		.unwrap();
	y.flatten_all().unwrap().to_vec1::<f32>().unwrap()
}

fn check(dir: &str) {
	let want = twin(dir);
	let file = std::fs::File::open(format!("{dir}/out.parquet")).unwrap();
	let out = ParquetReader::new(file).finish().unwrap();
	let names: Vec<String> = out
		.get_column_names()
		.iter()
		.map(|s| s.to_string())
		.collect();
	assert_eq!(
		names,
		["id", "a", "b", "c", "score"],
		"the script's frame: input columns then the prediction"
	);
	let got: Vec<f32> = out
		.column("score")
		.unwrap()
		.f32()
		.unwrap()
		.into_no_null_iter()
		.collect();
	assert_eq!(got.len(), want.len());
	let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
	if bits(&got) != bits(&want) {
		let first = got
			.iter()
			.zip(&want)
			.position(|(a, b)| a.to_bits() != b.to_bits())
			.unwrap();
		panic!(
			"prediction {first} differs: script {} twin {}",
			got[first], want[first]
		);
	}
	println!(
		"twin: {} predictions equal bit for bit (first {:?}, last {:?})",
		got.len(),
		got[0],
		got[got.len() - 1]
	);
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	match (args.get(1).map(|s| s.as_str()), args.get(2)) {
		(Some("write"), Some(d)) => write(d),
		(Some("check"), Some(d)) => check(d),
		_ => {
			eprintln!("usage: twin0129 write|check <dir>");
			std::process::exit(2)
		}
	}
}
