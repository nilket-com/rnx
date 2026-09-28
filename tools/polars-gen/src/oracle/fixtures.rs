// ---------------------------------------------------------------- oracle tests

/// Core fixtures: canonical type -> (rune fx function, Rust value, how to
/// show it structurally). Frames, series and columns are shown cell by cell
/// through `rnx_polars::oracle`, independent of Polars's `fmt` feature.
/// Record 0076: typed source fixtures. Each is a small deterministic value
/// of a wrapped type, named for its family, that feeds the producer
/// bindings it lists (`series_bool` feeds `Series::bool`); none replaces
/// the type's default fixture.
pub(crate) const TYPED_FIXTURES: &[(&str, &str, &str, &str, &[&str])] = &[
	(
		"polars_core::series::Series",
		"series_bool",
		"p::Series::new(\"x\".into(), [true, false, true])",
		"crate_oracle::series_repr(v)",
		&["bool"],
	),
	(
		"polars_core::series::Series",
		"series_str",
		"p::Series::new(\"x\".into(), [\"a\", \"bb\", \"ccc\"])",
		"crate_oracle::series_repr(v)",
		&["str"],
	),
	(
		"polars_core::series::Series",
		"series_binary",
		"p::Series::new(\"x\".into(), [&b\"ab\"[..], b\"\\x00\\xff\", b\"\"])",
		"crate_oracle::series_repr(v)",
		&["binary"],
	),
	(
		"polars_core::series::Series",
		"series_binary_offset",
		"p::Series::from_any_values_and_dtype(\"x\".into(), &[p::AnyValue::Binary(b\"ab\"), p::AnyValue::Binary(b\"\\x00\\xff\"), p::AnyValue::Binary(b\"\")], &p::DataType::BinaryOffset, true).unwrap()",
		"crate_oracle::series_repr(v)",
		&["binary_offset"],
	),
	(
		"polars_core::series::Series",
		"series_i8",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Int8).unwrap()",
		"crate_oracle::series_repr(v)",
		&["i8"],
	),
	(
		"polars_core::series::Series",
		"series_i16",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Int16).unwrap()",
		"crate_oracle::series_repr(v)",
		&["i16"],
	),
	(
		"polars_core::series::Series",
		"series_i32",
		"p::Series::new(\"x\".into(), [1i32, 2, 3])",
		"crate_oracle::series_repr(v)",
		&["i32"],
	),
	(
		"polars_core::series::Series",
		"series_i64",
		"p::Series::new(\"x\".into(), [1i64, 2, 3])",
		"crate_oracle::series_repr(v)",
		&["i64"],
	),
	(
		"polars_core::series::Series",
		"series_u8",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::UInt8).unwrap()",
		"crate_oracle::series_repr(v)",
		&["u8"],
	),
	(
		"polars_core::series::Series",
		"series_u16",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::UInt16).unwrap()",
		"crate_oracle::series_repr(v)",
		&["u16"],
	),
	(
		"polars_core::series::Series",
		"series_u32",
		"p::Series::new(\"x\".into(), [1u32, 2, 3])",
		"crate_oracle::series_repr(v)",
		&["u32", "idx"],
	),
	(
		"polars_core::series::Series",
		"series_u64",
		"p::Series::new(\"x\".into(), [1u64, 2, 3])",
		"crate_oracle::series_repr(v)",
		&["u64"],
	),
	// record 0091: values beyond u32 and at u64::MAX, which a script integer cannot spell; feeds no producer
	(
		"polars_core::series::Series",
		"series_u64_extremes",
		"p::Series::new(\"x\".into(), [u64::MAX, 4_294_967_297u64, 1])",
		"crate_oracle::series_repr(v)",
		&[],
	),
	// record 0100: two chunks each, feeding no producer: multibyte, empty and null strings;
	// zero, non-UTF-8, empty and null bytes (view and offset binary)
	(
		"polars_core::series::Series",
		"series_str_mixed",
		"{ let mut s = p::Series::new(\"x\".into(), [Some(\"é日本\"), Some(\"\")]); s.append(&p::Series::new(\"x\".into(), [None, Some(\"z\")])).unwrap(); s }",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_binary_mixed",
		"{ let mut s = p::Series::new(\"x\".into(), [Some(&b\"\\xc3\\x28\"[..]), Some(&b\"\"[..])]); s.append(&p::Series::new(\"x\".into(), [None, Some(&b\"\\x00\\x00\\xff\"[..])])).unwrap(); s }",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_binary_offset_mixed",
		"{ let mut s = p::Series::from_any_values_and_dtype(\"x\".into(), &[p::AnyValue::Binary(b\"\\xc3\\x28\"), p::AnyValue::Binary(b\"\")], &p::DataType::BinaryOffset, true).unwrap(); s.append(&p::Series::from_any_values_and_dtype(\"x\".into(), &[p::AnyValue::Null, p::AnyValue::Binary(b\"\\x00\\x00\\xff\")], &p::DataType::BinaryOffset, true).unwrap()).unwrap(); s }",
		"crate_oracle::series_repr(v)",
		&[],
	),
	// record 0098: two chunks of float specials (±0, distinct NaN payloads of both signs, ±inf, nulls); feed no producer
	(
		"polars_core::series::Series",
		"series_f64_specials",
		"{ let mut s = p::Series::new(\"x\".into(), [Some(1.5f64), Some(-0.0), Some(0.0), Some(f64::from_bits(0x7ff8_0000_0000_0001)), None]); s.append(&p::Series::new(\"x\".into(), [Some(f64::NEG_INFINITY), Some(f64::INFINITY), Some(f64::from_bits(0xfff0_0000_0000_0123)), None, Some(-2.5)])).unwrap(); s }",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_f32_specials",
		"{ let mut s = p::Series::new(\"x\".into(), [Some(1.5f32), Some(-0.0), Some(0.0), Some(f32::from_bits(0x7fc0_0001)), None]); s.append(&p::Series::new(\"x\".into(), [Some(f32::NEG_INFINITY), Some(f32::INFINITY), Some(f32::from_bits(0xff80_0123)), None, Some(-2.5)])).unwrap(); s }",
		"crate_oracle::series_repr(v)",
		&[],
	),
	// record 0093: the read-back boundary i64::MAX, i64::MAX + 1, u64::MAX and a null; feeds no producer
	(
		"polars_core::series::Series",
		"series_u64_boundary",
		"p::Series::new(\"x\".into(), [Some(i64::MAX as u64), Some(i64::MAX as u64 + 1), Some(u64::MAX), None])",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_f32",
		"p::Series::new(\"x\".into(), [1.5f32, 2.5, 3.5])",
		"crate_oracle::series_repr(v)",
		&["f32"],
	),
	(
		"polars_core::series::Series",
		"series_f64",
		"p::Series::new(\"x\".into(), [1.5f64, 2.5, 3.5])",
		"crate_oracle::series_repr(v)",
		&["f64"],
	),
	(
		"polars_core::series::Series",
		"series_struct",
		"p::IntoSeries::into_series(df().into_struct(\"x\".into()))",
		"crate_oracle::series_repr(v)",
		&["struct_"],
	),
	(
		"polars_core::series::Series",
		"series_list",
		"p::Series::new(\"x\".into(), [p::Series::new(\"a\".into(), [1i64, 2]), p::Series::new(\"b\".into(), [3i64])])",
		"crate_oracle::series_repr(v)",
		&["list"],
	),
	(
		"polars_core::series::Series",
		"series_date",
		"p::Series::new(\"x\".into(), [1i32, 2, 3]).cast(&p::DataType::Date).unwrap()",
		"crate_oracle::series_repr(v)",
		&["date"],
	),
	(
		"polars_core::series::Series",
		"series_datetime",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Datetime(p::TimeUnit::Milliseconds, None)).unwrap()",
		"crate_oracle::series_repr(v)",
		&["datetime"],
	),
	(
		"polars_core::series::Series",
		"series_duration",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Duration(p::TimeUnit::Milliseconds)).unwrap()",
		"crate_oracle::series_repr(v)",
		&["duration"],
	),
	(
		"polars_core::series::Series",
		"series_time",
		"p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Time).unwrap()",
		"crate_oracle::series_repr(v)",
		&["time"],
	),
	// for the comparator controls: a long array, an array with a null, floats that display alike
	(
		"polars_core::series::Series",
		"series_long",
		"p::Series::new(\"x\".into(), (0..40i64).collect::<Vec<_>>())",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_nulls",
		"p::Series::new(\"x\".into(), [Some(1i64), None, Some(3)])",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_struct_null_field",
		"p::IntoSeries::into_series(p::df!(\"x\" => [None::<i64>], \"y\" => [\"a\"]).unwrap().into_struct(\"s\".into()))",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_list_of_struct",
		"p::Series::new(\"l\".into(), [p::IntoSeries::into_series(p::df!(\"x\" => [None::<i64>], \"y\" => [\"a\"]).unwrap().into_struct(\"s\".into()))])",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_list_long",
		"p::Series::new(\"x\".into(), [p::Series::new(\"i\".into(), (0..40i64).collect::<Vec<_>>())])",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_list_nulls",
		"p::Series::new(\"x\".into(), [p::Series::new(\"i\".into(), [Some(1i64), None, Some(3)])])",
		"crate_oracle::series_repr(v)",
		&[],
	),
	(
		"polars_core::series::Series",
		"series_float_sum",
		"p::Series::new(\"x\".into(), [0.1f64 + 0.2])",
		"crate_oracle::series_repr(v)",
		&[],
	),
];

pub(crate) const FIXTURES: &[(&str, &str, &str, &str)] = &[
	(
		"polars_core::frame::dataframe::DataFrame",
		"df",
		"polars::df!(\"x\" => [1i64, 2, 3], \"y\" => [\"a\", \"b\", \"c\"], \"z\" => [1.5f64, 2.5, 3.5]).unwrap()",
		"crate_oracle::frame_repr(v)",
	),
	(
		"polars_lazy::frame::LazyFrame",
		"lf",
		"df().lazy()",
		"match v.clone().collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"collected \"), Err(e) => crate_oracle::Repr::Text(format!(\"collect error: {}\", crate_oracle::error_kind(&e))) }",
	),
	(
		"polars_plan::dsl::expr::Expr",
		"expr",
		"p::col(\"x\")",
		"match df().lazy().select([v.clone()]).collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"selected \"), Err(e) => crate_oracle::Repr::Text(format!(\"select error: {}\", crate_oracle::error_kind(&e))) }",
	),
	(
		"polars_core::series::Series",
		"series",
		"p::Series::new(\"x\".into(), [1i64, 2, 3])",
		"crate_oracle::series_repr(v)",
	),
	(
		"polars_core::frame::column::Column",
		"column",
		"series().into_column()",
		"crate_oracle::column_repr(v)",
	),
	(
		"polars_core::datatypes::dtype::DataType",
		"dtype",
		"p::DataType::Int64",
		"crate_oracle::Repr::Text(format!(\"{:?}\", v))",
	),
	(
		"polars_core::datatypes::field::Field",
		"field",
		"p::Field::new(\"x\".into(), p::DataType::Int64)",
		"crate_oracle::Repr::Text(format!(\"{:?}\", v))",
	),
	(
		"polars_core::series::implementations::null::NullChunked",
		"null_chunked",
		"p::Series::new_null(\"x\".into(), 2).null().unwrap().clone()",
		"crate_oracle::Repr::Text(format!(\"{}:{:?}:len={}\", p::SeriesTrait::name(v), p::SeriesTrait::dtype(v), v.len()))",
	),
	// record 0094: categorical receivers, built and unwrapped under one lock (support::categorical_fixtures)
	(
		"polars_dtype::categorical::Categories",
		"categories",
		"crate::generated::support::categorical_fixtures::categories()",
		"crate_oracle::Repr::Text(format!(\"{:?}\", v))",
	),
	(
		"polars_dtype::categorical::FrozenCategories",
		"frozen_categories",
		"crate::generated::support::categorical_fixtures::frozen_categories()",
		"crate_oracle::Repr::Text(format!(\"{:?}\", v))",
	),
	(
		"polars_dtype::categorical::mapping::CategoricalMapping",
		"categorical_mapping",
		"crate::generated::support::categorical_fixtures::mapping()",
		"crate_oracle::Repr::Text(format!(\"{:?}\", v))",
	),
	(
		"polars_lazy::frame::LazyGroupBy",
		"group_by",
		"lf().group_by_stable([p::col(\"y\")])",
		"match v.clone().agg([p::col(\"x\").sum()]).collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"agg \"), Err(e) => crate_oracle::Repr::Text(format!(\"agg error: {}\", crate_oracle::error_kind(&e))) }",
	),
];

/// How a fixture value of a wrapped type is built on both sides.
#[derive(Clone)]
pub(crate) struct Recipe {
	/// Rune expression; a fallible constructor is unwrapped with a `fixture:` panic.
	pub(crate) rune: String,
	/// Rust expression of the Polars value; a fallible constructor is
	/// unwrapped with a `fixture:` panic.
	pub(crate) rust: String,
	/// What the recipe is, for `surface.json`.
	pub(crate) kind: String,
}
