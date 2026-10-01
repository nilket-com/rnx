//! Record 0129: the cost of one crossing, through the neutral block and
//! through plain Rune values (the baseline the plan names).
//!
//! - dense: the adapters' own code, called through the VM (VM call overhead
//!   included, which counts against the block);
//! - plain: the same matrix as `Vec<Vec<f64>>` rows through
//!   `rune::to_value` / `rune::from_value`, the conversion an adapter
//!   returning plain values would pay, then the same tensor or column.
//!
//! `cost0129 [reps]`: a table of median microseconds per crossing.
use candle_core::{Device, Tensor};
use polars::prelude::*;
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::sync::Arc;
use std::time::Instant;

const SCRIPT: &str = r#"
pub fn load(path, n) {
    let schema = [];
    for i in 0..n { schema.push((`c${i}`, "f64")); }
    polars::read_csv(path, schema)
}
pub fn names(n) {
    let v = [];
    for i in 0..n { v.push(`c${i}`); }
    v
}
pub fn noop(df, cols) { Ok(()) }
pub fn forward(df, cols) { candle::Tensor::from_dense(df.to_dense(cols, "f32")?) }
pub fn back(df, t) { df.with_dense(t.to_dense(["score"])?) }
pub fn score(t) { t.to_dense(["score"]) }
"#;

fn vm() -> Vm {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(polars).unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	sources.insert(Source::memory(SCRIPT).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	Vm::new(runtime, Arc::new(unit))
}

fn ok(v: Value) -> Value {
	match rune::from_value::<Result<Value, Value>>(v).unwrap() {
		Ok(v) => v,
		Err(e) => panic!("{:?}", rune::from_value::<String>(e)),
	}
}

fn median(mut f: impl FnMut(), reps: usize) -> f64 {
	f();
	let mut t: Vec<f64> = (0..reps)
		.map(|_| {
			let s = Instant::now();
			f();
			s.elapsed().as_secs_f64() * 1e6
		})
		.collect();
	t.sort_by(f64::total_cmp);
	t[reps / 2]
}

fn frame(rows: usize, cols: usize) -> DataFrame {
	let columns: Vec<Column> = (0..cols)
		.map(|c| {
			let v: Vec<f64> = (0..rows)
				.map(|r| ((r * 7 + c * 3) % 97) as f64 / 8.0)
				.collect();
			Series::new(format!("c{c}").into(), v).into()
		})
		.collect();
	DataFrame::new(rows, columns).unwrap()
}

/// The baseline, forward: rows as plain Rune values, then the tensor.
fn plain_forward(df: &DataFrame) -> Tensor {
	let (rows, cols) = df.shape();
	// each column read once by its iterator, then transposed into rows
	let mut table: Vec<Vec<f64>> = (0..rows).map(|_| Vec::with_capacity(cols)).collect();
	for c in df.columns() {
		for (row, v) in table.iter_mut().zip(c.f64().unwrap().into_no_null_iter()) {
			row.push(v);
		}
	}
	let value = rune::to_value(table).unwrap();
	let back: Vec<Vec<f64>> = rune::from_value(value).unwrap();
	let flat: Vec<f32> = back.into_iter().flatten().map(|x| x as f32).collect();
	Tensor::from_vec(flat, (rows, cols), &Device::Cpu).unwrap()
}

/// The baseline, back: one output column as plain Rune values, appended.
fn plain_back(df: &DataFrame, t: &Tensor) -> DataFrame {
	let rows: Vec<Vec<f64>> = t
		.to_vec2::<f32>()
		.unwrap()
		.into_iter()
		.map(|r| r.into_iter().map(f64::from).collect())
		.collect();
	let value = rune::to_value(rows).unwrap();
	let back: Vec<Vec<f64>> = rune::from_value(value).unwrap();
	let col: Vec<f32> = back.into_iter().map(|r| r[0] as f32).collect();
	df.hstack(&[Series::new("score".into(), col).into()])
		.unwrap()
}

fn main() {
	let reps: usize = std::env::args().nth(1).map_or(31, |s| s.parse().unwrap());
	let dir = std::env::temp_dir().join(format!("cost0129-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	println!(
		"| rows x columns | values | VM call | dense forward | plain forward | dense back | plain back |"
	);
	println!("|---|---:|---:|---:|---:|---:|---:|");
	for (rows, cols) in [(64, 3), (4096, 3), (65536, 3), (262144, 3), (16384, 64)] {
		let mut df = frame(rows, cols);
		let path = dir.join(format!("{rows}x{cols}.csv"));
		CsvWriter::new(std::fs::File::create(&path).unwrap())
			.finish(&mut df)
			.unwrap();
		let mut vm = vm();
		let rdf = ok(vm
			.call(["load"], (path.to_str().unwrap(), cols as i64))
			.unwrap());
		let names = vm.call(["names"], (cols as i64,)).unwrap();
		let noop = median(
			|| drop(vm.call(["noop"], (rdf.clone(), names.clone())).unwrap()),
			reps,
		);
		let dense_f = median(
			|| {
				drop(ok(vm
					.call(["forward"], (rdf.clone(), names.clone()))
					.unwrap()))
			},
			reps,
		);
		let plain_f = median(|| drop(plain_forward(&df)), reps);
		// back: a rows x 1 tensor, as the MLP returns
		let t1 = Tensor::from_vec(vec![0.5f32; rows], (rows, 1), &Device::Cpu).unwrap();
		let one = vm.call(["names"], (1i64,)).unwrap();
		let rt1 = ok(vm.call(["forward"], (rdf.clone(), one)).unwrap());
		let dense_b = median(
			|| drop(ok(vm.call(["back"], (rdf.clone(), rt1.clone())).unwrap())),
			reps,
		);
		let plain_b = median(|| drop(plain_back(&df, &t1)), reps);
		println!(
			"| {rows} x {cols} | {} | {noop:.1} | {dense_f:.1} | {plain_f:.1} | {dense_b:.1} | {plain_b:.1} |",
			rows * cols
		);
	}
	let _ = std::fs::remove_dir_all(&dir);
}
