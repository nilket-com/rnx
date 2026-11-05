use rune::experimental_core::{
	column::{
		BitSlice, Column, ColumnChunk, ColumnOptions, ColumnOutcome, ColumnSlice, ColumnType,
		ColumnValues, PreparedColumn,
	},
	execute::ExecutorLimits,
	frontend,
	heap::Limits,
};
use rune::{Context, Diagnostics, Hash, Source, Sources};
use serde_json::json;
use std::{
	alloc::{GlobalAlloc, Layout, System},
	hint::black_box,
	sync::atomic::{AtomicBool, AtomicUsize, Ordering},
	time::Instant,
};
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOC: AtomicUsize = AtomicUsize::new(0);
static REALLOC: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct MeasuredAllocator;
unsafe impl GlobalAlloc for MeasuredAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		if TRACK.load(Ordering::Relaxed) {
			ALLOC.fetch_add(1, Ordering::Relaxed);
			BYTES.fetch_add(layout.size(), Ordering::Relaxed);
		}
		System.alloc(layout)
	}
	unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
		System.dealloc(pointer, layout)
	}
	unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
		if TRACK.load(Ordering::Relaxed) {
			REALLOC.fetch_add(1, Ordering::Relaxed);
			BYTES.fetch_add(size, Ordering::Relaxed);
		}
		System.realloc(pointer, layout, size)
	}
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;
fn begin_alloc() {
	ALLOC.store(0, Ordering::Relaxed);
	REALLOC.store(0, Ordering::Relaxed);
	BYTES.store(0, Ordering::Relaxed);
	TRACK.store(true, Ordering::Relaxed);
}
fn end_alloc() -> serde_json::Value {
	TRACK.store(false, Ordering::Relaxed);
	json!({"alloc":ALLOC.load(Ordering::Relaxed), "realloc":REALLOC.load(Ordering::Relaxed), "requested_bytes":BYTES.load(Ordering::Relaxed)})
}

extern "C" {
	fn step_cycles_open() -> i32;
	fn step_begin(fd: i32);
	fn step_end(fd: i32, out: *mut u64);
	fn step_close(fd: i32);
	fn step_thermal_before() -> u64;
	fn step_thermal_after() -> u64;
	fn step_elapsed_ns() -> u64;
}
fn main() {
	let args: Vec<_> = std::env::args().collect();
	assert_eq!(args.len(), 4, "OP ROWS scalar|auto");
	let op = args[1].as_str();
	let n: usize = args[2].parse().unwrap();
	let force_scalar = match args[3].as_str() {
		"scalar" => true,
		"auto" => false,
		_ => panic!("path"),
	};
	let (source, ty, arity) = match op {
		"i64add" => ("pub fn map(x) { x + 1 }", ColumnType::I64, 1),
		"axpy" => ("pub fn map(x, y) { x * 2 + y }", ColumnType::I64, 2),
		"mul3" => ("pub fn map(x, y) { x * 3 + y }", ColumnType::I64, 2),
		"f64pair" => ("pub fn map(x, y) { x + y }", ColumnType::F64, 2),
		"f64add" => ("pub fn map(x) { x + 1.0 }", ColumnType::F64, 1),
		"boolnot" => ("pub fn map(x) { !x }", ColumnType::Bool, 1),
		_ => panic!("operation"),
	};
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let program = frontend::compile(
		&mut sources,
		&Context::with_default_modules().unwrap(),
		&mut Diagnostics::new(),
		1,
	)
	.unwrap();
	let function = program.function(Hash::type_hash(["map"])).unwrap();
	begin_alloc();
	let prepare_started = Instant::now();
	let plan =
		PreparedColumn::prepare(&program, function, &[ty; 3][..arity], ty, Limits::default())
			.unwrap();
	let prepare_ns = prepare_started.elapsed().as_nanos();
	let prepare_alloc = end_alloc();
	let ints: Vec<_> = if ty == ColumnType::I64 {
		(0..n).map(|i| (i % 1009) as i64 - 500).collect()
	} else {
		vec![]
	};
	let y: Vec<_> = if arity == 2 && ty == ColumnType::I64 {
		(0..n).map(|i| (i % 997) as i64 - 300).collect()
	} else {
		vec![]
	};
	let floats: Vec<_> = if ty == ColumnType::F64 {
		(0..n).map(|i| ((i % 1009) as f64 - 500.0) * 0.25).collect()
	} else {
		vec![]
	};
	let float_y: Vec<_> = if arity == 2 && ty == ColumnType::F64 {
		(0..n).map(|i| ((i % 997) as f64 - 300.0) * 0.125).collect()
	} else {
		vec![]
	};
	let bits: Vec<_> = if ty == ColumnType::Bool {
		(0..n.div_ceil(8)).map(|i| (i % 251) as u8).collect()
	} else {
		vec![]
	};
	let values = match ty {
		ColumnType::I64 => ColumnSlice::I64(&ints),
		ColumnType::F64 => ColumnSlice::F64(&floats),
		ColumnType::Bool => ColumnSlice::Bool(BitSlice::new(&bits, 0, n).unwrap()),
	};
	let a = [ColumnChunk::new(values, None).unwrap()];
	let b = [ColumnChunk::new(
		if ty == ColumnType::F64 {
			ColumnSlice::F64(&float_y)
		} else {
			ColumnSlice::I64(&y)
		},
		None,
	)
	.unwrap()];
	let mut columns = vec![Column::new(ty, &a).unwrap()];
	if arity == 2 {
		columns.push(Column::new(ty, &b).unwrap());
	}
	let options = ColumnOptions {
		work: usize::MAX,
		executor_limits: ExecutorLimits::default(),
		force_scalar,
	};
	// Warm dispatch and allocation separately from the measured invoke.
	let warm = plan.execute(&columns, options, || false);
	assert!(matches!(warm, ColumnOutcome::Complete(_)));
	drop(warm);
	let allocation_control = plan.prepare_batch(&columns, options, || false).unwrap();
	begin_alloc();
	let control_result = allocation_control.invoke(|| false);
	let invoke_alloc = end_alloc();
	assert!(matches!(control_result, ColumnOutcome::Complete(_)));
	assert_eq!(invoke_alloc["alloc"], 0);
	assert_eq!(invoke_alloc["realloc"], 0);
	drop(control_result);
	begin_alloc();
	let batch_started = Instant::now();
	let batch = plan.prepare_batch(&columns, options, || false).unwrap();
	let batch_ns = batch_started.elapsed().as_nanos();
	let batch_alloc = end_alloc();
	let owned = batch.owned_bytes();
	let fd = unsafe { step_cycles_open() };
	unsafe { step_begin(fd) };
	let result = black_box(batch).invoke(|| false);
	let mut counters = [0u64; 3];
	unsafe {
		step_end(fd, counters.as_mut_ptr());
		step_close(fd);
	}
	let ColumnOutcome::Complete(result) = result else {
		panic!("kernel refused: {result:?}");
	};
	assert_eq!(result.len, n);
	assert!(result.validity.is_none());
	assert_eq!(result.remaining_work, usize::MAX - n * plan.row_charge());
	let mut checksum = 0u64;
	match result.values {
		ColumnValues::I64(out) => {
			for (i, v) in out.into_iter().enumerate() {
				let expected = if arity == 2 {
					ints[i]
						.checked_mul(if op == "mul3" { 3 } else { 2 })
						.unwrap()
						.checked_add(y[i])
						.unwrap()
				} else {
					ints[i].checked_add(1).unwrap()
				};
				assert_eq!(v, expected);
				checksum = checksum.wrapping_add(v as u64);
			}
		}
		ColumnValues::F64(out) => {
			for (i, v) in out.into_iter().enumerate() {
				assert_eq!(
					v.to_bits(),
					(floats[i] + if arity == 2 { float_y[i] } else { 1.0 }).to_bits()
				);
				checksum = checksum.wrapping_add(v.to_bits());
			}
		}
		ColumnValues::Bool(out) => {
			for i in 0..n {
				let actual = out[i / 8] & (1 << (i % 8)) != 0;
				assert_eq!(actual, bits[i / 8] & (1 << (i % 8)) == 0);
				checksum += u64::from(actual);
			}
		}
	}
	println!(
		"{}",
		json!({"status":"PASS", "op":op, "rows":n, "path":format!("{:?}", result.path), "checksum":checksum, "batch_owned_bytes":owned, "prepare_ns":prepare_ns,"prepare_alloc":prepare_alloc,"batch_prepare_ns":batch_ns,"batch_alloc":batch_alloc,"invoke_alloc":invoke_alloc,"plan_bytes":plan.retained_bytes(), "row_charge":plan.row_charge(), "cycles":counters[0], "enabled":counters[1], "running":counters[2], "thermal_before":unsafe{step_thermal_before()}, "thermal_after":unsafe{step_thermal_after()}, "measured_ns":unsafe{step_elapsed_ns()}})
	);
}
