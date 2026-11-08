use super::*;
use rune::experimental_core::error::ErrorKind;

fn kernel(source: &str, inputs: &[ColumnType], output: ColumnType) -> Kernel {
	Kernel::prepare(source, "map", inputs, output, Limits::default()).unwrap()
}
fn options(work: usize, mode: Mode) -> Options {
	Options {
		work,
		mode,
		executor_limits: ExecutorLimits::default(),
		adapter_bytes: 0,
	}
}
fn one(values: &[i64]) -> Chunk<'_> {
	Chunk {
		values: ColumnSlice::I64(values),
		validity: None,
	}
}
fn ints(output: &Output) -> &[i64] {
	let ColumnValues::I64(values) = &output.values else {
		panic!("expected i64")
	};
	values
}

#[test]
fn successful_primitives_and_reference_have_identical_work() {
	let mut k = kernel(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let data: Vec<_> = (0..4099).collect();
	let chunks = [one(&data)];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	let work = k.row_charge().unwrap() * data.len();
	for mode in [Mode::Auto, Mode::Portable, Mode::Reference] {
		let out = k.map(&inputs, options(work, mode), || false).unwrap();
		assert_eq!(ints(&out), &(1..4100).collect::<Vec<_>>());
		assert_eq!(out.remaining_work, 0);
		assert_eq!(k.last_path().valid_rows, data.len());
		assert!(k.last_path().completed);
	}
}

#[test]
fn each_fuel_stop_replays_once_with_original_failure() {
	let mut k = kernel(
		"pub fn map(x) { x * 2 + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let chunks = [one(&[1, 2, 3, 4])];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	let total = k.row_charge().unwrap() * 4;
	for fuel in 0..total {
		let fast = k
			.map(&inputs, options(fuel, Mode::Auto), || false)
			.unwrap_err();
		assert_eq!(k.last_path().attempts, 1);
		assert_eq!(k.last_path().replays, 1);
		let reference = k
			.map(&inputs, options(fuel, Mode::Reference), || false)
			.unwrap_err();
		assert_eq!(fast.row, reference.row);
		assert_eq!(fast.remaining_work, reference.remaining_work);
		let (Cause::Core(a), Cause::Core(b)) = (fast.cause, reference.cause) else {
			panic!("core failures required")
		};
		assert_eq!(a.error.kind, ErrorKind::FuelExhausted);
		assert_eq!(a.error.kind, b.error.kind);
		assert_eq!(a.error.location, b.error.location);
		assert_eq!(a.call_chain, b.call_chain);
	}
}

#[test]
fn late_overflow_preserves_first_error_and_trace() {
	let mut k = kernel(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let mut data = vec![1; 4097];
	data[4095] = i64::MAX;
	let chunks = [one(&data)];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	let error = k
		.map(&inputs, options(usize::MAX, Mode::Auto), || false)
		.unwrap_err();
	assert_eq!(error.row, Some(4095));
	assert_eq!(k.last_path().valid_rows, 4095);
	assert_eq!((k.last_path().attempts, k.last_path().replays), (1, 1));
	let text = k.format_error(&error);
	let reference = k
		.map(&inputs, options(usize::MAX, Mode::Reference), || false)
		.unwrap_err();
	assert_eq!(text, k.format_error(&reference));
	assert_eq!(error.remaining_work, reference.remaining_work);
}

#[test]
fn independent_boundaries_and_null_payloads() {
	let mut k = kernel(
		"pub fn map(x, y) { x * 2 + y }",
		&[ColumnType::I64; 2],
		ColumnType::I64,
	);
	let null = [0u8];
	let valid = [0b1010_1100u8];
	let a = [
		one(&[]),
		Chunk {
			values: ColumnSlice::I64(&[i64::MAX]),
			validity: Some(BitSlice::new(&null, 0, 1).unwrap()),
		},
		one(&[2, 3, 4]),
		one(&[]),
	];
	let b = [
		Chunk {
			values: ColumnSlice::I64(&[1, 10]),
			validity: Some(BitSlice::new(&valid, 2, 2).unwrap()),
		},
		one(&[]),
		one(&[20, 30]),
	];
	let inputs = [
		Input {
			ty: ColumnType::I64,
			chunks: &a,
		},
		Input {
			ty: ColumnType::I64,
			chunks: &b,
		},
	];
	let charge = k.row_charge().unwrap();
	for mode in [Mode::Auto, Mode::Portable, Mode::Reference] {
		let out = k.map(&inputs, options(3 * charge, mode), || false).unwrap();
		assert_eq!(ints(&out), [0, 14, 26, 38]);
		assert_eq!(out.validity.as_deref(), Some([14u8].as_slice()));
		assert_eq!(out.remaining_work, 0);
	}
}

#[test]
fn all_bit_offsets_are_independent() {
	let mut k = kernel(
		"pub fn map(x) { !x }",
		&[ColumnType::Bool],
		ColumnType::Bool,
	);
	let values = [0b1010_0101u8; 8];
	let valid = [0b1101_1010u8; 8];
	for offset in 0..8 {
		for validity_offset in 0..8 {
			let chunks = [Chunk {
				values: ColumnSlice::Bool(BitSlice::new(&values, offset, 43).unwrap()),
				validity: Some(BitSlice::new(&valid, validity_offset, 43).unwrap()),
			}];
			let inputs = [Input {
				ty: ColumnType::Bool,
				chunks: &chunks,
			}];
			let a = k
				.map(&inputs, options(usize::MAX, Mode::Auto), || false)
				.unwrap();
			let b = k
				.map(&inputs, options(usize::MAX, Mode::Reference), || false)
				.unwrap();
			let (ColumnValues::Bool(a), ColumnValues::Bool(b)) = (a.values, b.values) else {
				unreachable!()
			};
			assert_eq!(a, b);
			assert_eq!(a.last().unwrap() & 0b1111_1000, 0);
		}
	}
}

#[test]
fn f64_special_values_preserve_bits() {
	let mut k = kernel(
		"pub fn map(x) { x + 1.0 }",
		&[ColumnType::F64],
		ColumnType::F64,
	);
	let values = [
		f64::NAN,
		f64::INFINITY,
		f64::NEG_INFINITY,
		-0.,
		f64::MIN_POSITIVE,
	];
	let chunks = [Chunk {
		values: ColumnSlice::F64(&values),
		validity: None,
	}];
	let inputs = [Input {
		ty: ColumnType::F64,
		chunks: &chunks,
	}];
	for mode in [Mode::Auto, Mode::Portable, Mode::Reference] {
		let out = k.map(&inputs, options(1000, mode), || false).unwrap();
		let ColumnValues::F64(out) = out.values else {
			unreachable!()
		};
		for (x, y) in values.iter().zip(out) {
			assert_eq!((x + 1.).to_bits(), y.to_bits());
		}
	}
}

#[test]
fn refused_branch_uses_stock_without_attempts() {
	let mut k = kernel(
		"pub fn map(x) { if x < 0 { -x } else { x } }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	assert!(k.row_charge().is_none());
	let chunks = [one(&[-3, 0, 4])];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	let a = k.map(&inputs, options(1000, Mode::Auto), || false).unwrap();
	assert_eq!(ints(&a), [3, 0, 4]);
	assert_eq!((k.last_path().attempts, k.last_path().replays), (0, 0));
	let b = k
		.map(&inputs, options(1000, Mode::Reference), || false)
		.unwrap();
	assert_eq!(a.remaining_work, b.remaining_work);
	assert!(a.remaining_work < 1000);
}

#[test]
fn stock_unsigned_result_is_not_coerced_to_declared_i64() {
	let mut k = kernel(
		"pub fn map(x) { if x > 0 { 1u64 } else { 2u64 } }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	assert!(matches!(k.scalar, Scalar::Stock { .. }));
	let chunks = [one(&[1, 2])];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	for mode in [Mode::Auto, Mode::Reference] {
		let error = k.map(&inputs, options(1000, mode), || false).unwrap_err();
		assert!(matches!(error.cause, Cause::Invalid("non-scalar result")));
		assert_eq!(error.row, Some(0));
		assert!(error.remaining_work < 1000);
		assert_eq!((k.last_path().attempts, k.last_path().replays), (0, 0));
		assert!(!k.last_path().completed);
	}
}

#[test]
fn cancellation_and_validation_clear_observation() {
	let mut k = kernel(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let data = vec![1; 4096];
	let chunks = [one(&data)];
	let inputs = [Input {
		ty: ColumnType::I64,
		chunks: &chunks,
	}];
	k.map(&inputs, options(100000, Mode::Auto), || false)
		.unwrap();
	assert!(k.map(&[], options(100000, Mode::Auto), || false).is_err());
	assert!(k.last_path().path.is_none());
	assert!(!k.last_path().completed);
	let mut checks = 0;
	let err = k
		.map(&inputs, options(100000, Mode::Auto), || {
			checks += 1;
			checks == 4
		})
		.unwrap_err();
	assert!(matches!(err.cause, Cause::Canceled));
	assert_eq!(err.remaining_work, 100000);
	assert_eq!(k.last_path().path, Some(Path::Canceled));
	assert_eq!(k.last_path().replays, 0);
}

#[test]
fn empty_and_all_null_batches_charge_nothing() {
	let mut k = kernel(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let null = [0u8];
	let chunks = [Chunk {
		values: ColumnSlice::I64(&[i64::MAX; 8]),
		validity: Some(BitSlice::new(&null, 0, 8).unwrap()),
	}];
	for chunks in [&[][..], &chunks[..]] {
		for mode in [Mode::Auto, Mode::Reference] {
			let out = k
				.map(
					&[Input {
						ty: ColumnType::I64,
						chunks,
					}],
					options(0, mode),
					|| false,
				)
				.unwrap();
			assert_eq!(out.remaining_work, 0);
			assert_eq!(k.last_path().valid_rows, 0);
		}
	}
}

#[test]
fn preparation_validates_arity_without_executing_body() {
	assert!(
		Kernel::prepare(
			"pub fn map(x,y) { panic(\"must not execute\") }",
			"map",
			&[ColumnType::I64],
			ColumnType::I64,
			Limits::default()
		)
		.is_err()
	);
	assert!(
		Kernel::prepare(
			"pub fn map(x) { x }",
			"map;bad",
			&[ColumnType::I64],
			ColumnType::I64,
			Limits::default()
		)
		.is_err()
	);
	assert!(
		Kernel::prepare(
			"pub fn map(x) { x }",
			"map",
			&[ColumnType::I64],
			ColumnType::I64,
			Limits {
				bytes: 0,
				..Limits::default()
			}
		)
		.is_err()
	);
}

#[test]
fn adapter_capacity_cannot_disappear_from_limits() {
	let mut k = kernel(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let chunks = [one(&[1])];
	let mut opts = options(1000, Mode::Auto);
	opts.adapter_bytes = usize::MAX;
	let err = k
		.map(
			&[Input {
				ty: ColumnType::I64,
				chunks: &chunks,
			}],
			opts,
			|| false,
		)
		.unwrap_err();
	assert!(matches!(err.cause, Cause::Memory));
	assert_eq!(err.remaining_work, 1000);
}

#[test]
fn scalar_cancellation_keeps_completed_row_debits() {
	let mut k = kernel(
		"pub fn map(x) { x + 1 }",
		&[ColumnType::I64],
		ColumnType::I64,
	);
	let chunks = [one(&[1, 2, 3, 4])];
	let charge = k.row_charge().unwrap();
	let mut checks = 0;
	let error = k
		.map(
			&[Input {
				ty: ColumnType::I64,
				chunks: &chunks,
			}],
			options(1000, Mode::Reference),
			|| {
				checks += 1;
				checks == 4
			},
		)
		.unwrap_err();
	assert!(matches!(error.cause, Cause::Canceled));
	assert_eq!(error.remaining_work, 1000 - charge * 2);
	assert_eq!(k.last_path().valid_rows, 2);
	assert_eq!(k.last_path().replays, 0);
}

#[test]
fn owned_handles_and_results_can_cross_a_joined_worker() {
	fn send<T: Send>() {}
	send::<Kernel>();
	send::<Error>();
	send::<Output>();
}

#[test]
fn validity_word_count_matches_independent_bit_reference() {
	for len in 0usize..=1031 {
		for pattern in [0u8, 255, 170, 49] {
			let mut bits = vec![pattern; len.div_ceil(8)];
			if !len.is_multiple_of(8) {
				*bits.last_mut().unwrap() &= (1 << (len % 8)) - 1;
			}
			let expected = (0..len)
				.filter(|&i| bits[i / 8] & (1 << (i % 8)) != 0)
				.count();
			assert_eq!(
				validity_count(&bits, len),
				expected,
				"len {len}, pattern {pattern}"
			);
			if !len.is_multiple_of(8) {
				*bits.last_mut().unwrap() |= !((1 << (len % 8)) - 1);
				assert_eq!(
					validity_count(&bits, len),
					expected,
					"dirty padding, len {len}"
				);
			}
		}
	}
}

#[test]
fn core_valid_rows_matches_reference_for_sliced_dirty_masks() {
	for offset in 0..8 {
		for len in [0usize, 1, 63, 64, 65, 129] {
			let data: Vec<_> = (0..len).map(|i| i as i64).collect();
			let bits = vec![0xa5; (offset + len).div_ceil(8)];
			let mask = BitSlice::new(&bits, offset, len).unwrap();
			let chunks = [Chunk {
				values: ColumnSlice::I64(&data),
				validity: Some(mask),
			}];
			let inputs = [Input {
				ty: ColumnType::I64,
				chunks: &chunks,
			}];
			let mut k = kernel(
				"pub fn map(x) { x * 3 + x }",
				&[ColumnType::I64],
				ColumnType::I64,
			);
			let expected = (0..len).filter(|&i| mask.get(i).unwrap()).count();
			for mode in [Mode::Auto, Mode::Portable, Mode::Reference] {
				let out = k.map(&inputs, options(10000, mode), || false).unwrap();
				assert_eq!(k.last_path().valid_rows, expected);
				for (i, &v) in ints(&out).iter().enumerate() {
					assert_eq!(v, if mask.get(i).unwrap() { data[i] * 4 } else { 0 });
				}
			}
		}
	}
}

#[test]
fn explicit_reuse_refusal_sequence_matches_plain_map() {
	let x = [1.0; 65];
	let bits = [0x55; 9];
	let chunks = [Chunk {
		values: ColumnSlice::F64(&x),
		validity: Some(BitSlice::new(&bits, 0, x.len()).unwrap()),
	}];
	let short = [Chunk {
		values: ColumnSlice::F64(&x[..64]),
		validity: None,
	}];
	for source in [
		"pub fn map(x, y) { (x + y) * 2.0 }",
		"pub fn map(x, y) { x + y }",
		"pub fn map(x, y) { x - y }",
		"pub fn map(x, y) { x * y }",
	] {
		for case in [
			"type",
			"bounds",
			"frames",
			"slots",
			"cancel",
			"fuel",
			"validity-memory",
			"scratch-memory",
			"success",
		] {
			let mut k = kernel(source, &[ColumnType::F64; 2], ColumnType::F64);
			let inputs = [
				Input {
					ty: if case == "type" {
						ColumnType::I64
					} else {
						ColumnType::F64
					},
					chunks: &chunks,
				},
				Input {
					ty: ColumnType::F64,
					chunks: if case == "bounds" { &short } else { &chunks },
				},
			];
			let mut opts = options(100000, Mode::Auto);
			match case {
				"frames" => opts.executor_limits.frames = 0,
				"slots" => opts.executor_limits.slots = 0,
				"fuel" => opts.work = 1,
				"validity-memory" => k.limits.bytes = k.retained_bytes + 1,
				"scratch-memory" => k.limits.bytes = k.retained_bytes + 4096,
				_ => {}
			}
			let fresh = k.map(&inputs, opts, || case == "cancel");
			let observation = format!("{:?}", k.last_path());
			let reused = k.map_reusing(&inputs, opts, Some(vec![42.0; 65]), || case == "cancel");
			assert_eq!(observation, format!("{:?}", k.last_path()), "{case}");
			match (fresh, reused) {
				(Ok(a), Ok(b)) => {
					assert_eq!(a.remaining_work, b.remaining_work, "{case}");
					assert_eq!(a.validity, b.validity);
				}
				(Err(a), Err(b)) => {
					assert_eq!(a.remaining_work, b.remaining_work, "{case}");
					assert_eq!(k.format_error(&a), k.format_error(&b), "{case}");
				}
				_ => panic!("different refusal: {case}"),
			}
		}
	}
}
