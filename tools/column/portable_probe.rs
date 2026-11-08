use rnx::column::{
	Chunk, ColumnSlice, ColumnType, ColumnValues, ExecutorLimits, Input, Kernel, Limits, Mode,
	Options, Output, Path,
};

fn never_canceled() -> bool {
	false
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn measured_portable_map(kernel: &mut Kernel, inputs: &[Input<'_>], rows: usize) -> Output {
	kernel
		.map(
			inputs,
			Options {
				work: rows * 100,
				executor_limits: ExecutorLimits::default(),
				mode: Mode::Portable,
				adapter_bytes: 0,
			},
			never_canceled,
		)
		.unwrap()
}

fn main() {
	let args: Vec<_> = std::env::args().collect();
	let rows: usize = args[1].parse().unwrap();
	let warm = args[2] == "warm";
	let op = args[3].as_str();
	assert!(["==", "!=", "<", "<=", ">", ">="].contains(&op));
	let expected = match op {
		"==" | "<" | "<=" => false,
		_ => true,
	};
	let mut kernel = Kernel::prepare(
		&format!("pub fn map(x) {{ x {op} 1 }}"),
		"map",
		&[ColumnType::I64],
		ColumnType::Bool,
		Limits::default(),
	)
	.unwrap();
	let data = vec![2i64; rows];
	let chunks = [Chunk {
		values: ColumnSlice::I64(&data),
		validity: None,
	}];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	if warm {
		let output = kernel
			.map(
				&inputs,
				Options {
					work: rows * 100,
					executor_limits: ExecutorLimits::default(),
					mode: Mode::Portable,
					adapter_bytes: 0,
				},
				never_canceled,
			)
			.unwrap();
		std::hint::black_box(output);
	}
	let output = measured_portable_map(&mut kernel, &inputs, rows);
	assert_eq!(kernel.last_path().path, Some(Path::ScalarPrimitives));
	assert_eq!(kernel.last_path().attempts, 1);
	assert_eq!(kernel.last_path().replays, 0);
	assert_eq!(output.len, rows);
	let ColumnValues::Bool(bits) = output.values else {
		panic!("expected Boolean output")
	};
	for i in 0..rows {
		assert_eq!((bits[i / 8] >> (i % 8)) & 1 != 0, expected);
	}
	println!("PASS {rows} {op} portable");
}
