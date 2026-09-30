//! Record 0122: the Rust twin of every workflow step: the same operation
//! in Polars' Rust API over the same fixtures, rendered by the oracle's
//! structural text. The probe compares each step's printed result with its
//! twin. Run as an adapter test at v2 (the target pin): `PROBE_DATA` is the
//! fixture directory, `PROBE_OUT` receives `twins.json`.
#![cfg(all(feature = "generated", feature = "test-support"))]
use polars::prelude::*;
use rnx_polars::oracle::{frame_repr, series_repr};
use std::collections::BTreeMap;
use std::sync::Arc;

fn data(name: &str) -> String {
	format!("{}/{name}", std::env::var("PROBE_DATA").unwrap())
}

fn sales_schema() -> Schema {
	Schema::from_iter([
		Field::new("id".into(), DataType::Int64),
		Field::new("date".into(), DataType::String),
		Field::new("region".into(), DataType::String),
		Field::new("product".into(), DataType::String),
		Field::new("qty".into(), DataType::Int64),
		Field::new("price".into(), DataType::Float64),
		Field::new("note".into(), DataType::String),
	])
}

/// The shared `sales` setup: the hand-written reader's options.
fn sales() -> DataFrame {
	CsvReadOptions::default()
		.with_has_header(true)
		.with_schema(Some(Arc::new(sales_schema())))
		.try_into_reader_with_file_path(Some(data("sales.csv").into()))
		.unwrap()
		.finish()
		.unwrap()
}

fn regions() -> DataFrame {
	let schema = Schema::from_iter([
		Field::new("region".into(), DataType::String),
		Field::new("manager".into(), DataType::String),
		Field::new("target".into(), DataType::Int64),
	]);
	CsvReadOptions::default()
		.with_has_header(true)
		.with_schema(Some(Arc::new(schema)))
		.try_into_reader_with_file_path(Some(data("regions.csv").into()))
		.unwrap()
		.finish()
		.unwrap()
}

fn frame(r: PolarsResult<DataFrame>) -> String {
	match r {
		Ok(df) => frame_repr(&df).to_text(),
		Err(e) => format!("<<twin error: {e}>>"),
	}
}

fn with(expr: Expr) -> String {
	frame(sales().lazy().with_column(expr).collect())
}

fn twins() -> BTreeMap<&'static str, String> {
	let mut t: BTreeMap<&'static str, String> = BTreeMap::new();
	// W1 load
	// record 0125: Polars' own inference over the file (it was the
	// explicit-schema `sales()`, which only happened to agree)
	t.insert(
		"w1.csv_file",
		frame(
			CsvReadOptions::default()
				.with_has_header(true)
				.try_into_reader_with_file_path(Some(data("sales.csv").into()))
				.and_then(|r| r.finish()),
		),
	);
	t.insert("w1.csv_file_schema", frame(Ok(sales())));
	t.insert(
		"w1.csv_bytes",
		frame(
			CsvReadOptions::default()
				.with_has_header(true)
				.with_parse_options(CsvParseOptions::default().with_try_parse_dates(true))
				.into_reader_with_file_handle(std::io::Cursor::new(
					std::fs::read(data("sales.csv")).unwrap(),
				))
				.finish(),
		),
	);
	t.insert("w1.parquet_file", frame(Ok(sales())));
	t.insert("w1.parquet_bytes", frame(Ok(sales())));
	t.insert(
		"w1.json_bytes",
		frame(
			JsonReader::new(std::io::Cursor::new(
				std::fs::read(data("regions.json")).unwrap(),
			))
			.finish(),
		),
	);
	// record 0125: Polars' JsonReader over the same file
	t.insert(
		"w1.json_file",
		frame(JsonReader::new(std::fs::File::open(data("regions.json")).unwrap()).finish()),
	);
	t.insert("w1.scan_parquet", frame(Ok(sales())));
	// W2 inspect
	t.insert("w2.schema", format!("{:?}", sales().schema()));
	t.insert("w2.head", frame(Ok(sales().head(Some(3)))));
	t.insert("w2.shape", {
		let (h, w) = sales().shape();
		format!("{h}x{w}")
	});
	t.insert("w2.null_count", frame(Ok(sales().null_count())));
	t.insert(
		"w2.summary",
		frame(
			sales()
				.lazy()
				.select([
					col("qty").mean().alias("mean"),
					col("qty").min().alias("min"),
					col("qty").max().alias("max"),
					col("qty").std(1).alias("std"),
				])
				.collect(),
		),
	);
	// W3 clean
	t.insert("w3.cast", with(col("qty").cast(DataType::Float64)));
	t.insert("w3.rename", {
		let mut d = sales();
		d.rename("qty", "quantity".into()).unwrap();
		frame(Ok(d))
	});
	t.insert("w3.fill_null", with(col("qty").fill_null(lit(0))));
	t.insert(
		"w3.drop_nulls",
		frame(sales().lazy().drop_nulls(Some(cols(["price"]))).collect()),
	);
	t.insert(
		"w3.dedupe",
		frame(sales().unique_stable(None, UniqueKeepStrategy::First, None)),
	);
	t.insert(
		"w3.filter",
		frame(sales().lazy().filter(col("qty").gt(lit(2))).collect()),
	);
	// W4 derive
	t.insert(
		"w4.string",
		with(col("product").str().to_uppercase().alias("product_uc")),
	);
	t.insert(
		"w4.parse_date",
		with(col("date").str().to_date(StrptimeOptions::default())),
	);
	t.insert(
		"w4.temporal",
		frame(
			sales()
				.lazy()
				.with_columns([col("date").str().to_date(StrptimeOptions::default())])
				.with_column(col("date").dt().weekday().alias("weekday"))
				.collect(),
		),
	);
	t.insert(
		"w4.conditional",
		with(
			when(col("qty").gt(lit(3)))
				.then(lit("big"))
				.otherwise(lit("small"))
				.alias("size"),
		),
	);
	t.insert(
		"w4.arithmetic",
		with((col("qty") * col("price")).alias("revenue")),
	);
	// W5 group and aggregate
	t.insert(
		"w5.eager_group_by",
		frame(sales().group_by(["region"]).and_then(|g| g.count())),
	);
	t.insert(
		"w5.lazy_group_by",
		frame(
			sales()
				.lazy()
				.group_by([col("region")])
				.agg([
					col("qty").sum().alias("qty"),
					col("price").mean().alias("avg_price"),
					col("id").count().alias("n"),
				])
				.sort(["region"], Default::default())
				.collect(),
		),
	);
	t.insert(
		"w5.multi_key",
		frame(
			sales()
				.lazy()
				.group_by([col("region"), col("product")])
				.agg([col("qty").sum()])
				.sort(["region", "product"], Default::default())
				.collect(),
		),
	);
	// W6 reshape
	t.insert(
		"w6.join_eager",
		frame(sales().inner_join(&regions(), ["region"], ["region"])),
	);
	t.insert(
		"w6.join_lazy",
		frame(
			sales()
				.lazy()
				.join(
					regions().lazy(),
					[col("region")],
					[col("region")],
					JoinArgs::new(JoinType::Left),
				)
				.and_then(|l| l.collect()),
		),
	);
	t.insert("w6.pivot", frame(pivot(sales().lazy())));
	t.insert(
		"w6.unpivot",
		frame(sales().unpivot(Some(["qty", "price"]), ["id"])),
	);
	t.insert(
		"w6.concat",
		frame(
			concat([sales().lazy(), sales().lazy()], UnionArgs::default()).and_then(|l| l.collect()),
		),
	);
	// W7 windows
	t.insert(
		"w7.over",
		with(col("qty").sum().over([col("region")]).unwrap().alias("region_qty")),
	);
	t.insert(
		"w7.cum_sum",
		with(col("qty").cum_sum(false).alias("running")),
	);
	t.insert(
		"w7.rolling",
		with(
			col("price")
				.rolling_mean(RollingOptionsFixedWindow::default())
				.alias("roll"),
		),
	);
	t.insert(
		"w7.rank",
		with(col("qty").rank(RankOptions::default(), None).alias("rank")),
	);
	// W8 output
	t.insert("w8.csv", {
		let mut b = Vec::new();
		CsvWriter::new(&mut b).finish(&mut sales()).unwrap();
		String::from_utf8(b).unwrap()
	});
	t.insert("w8.parquet", {
		let mut b = Vec::new();
		ParquetWriter::new(&mut b).finish(&mut sales()).unwrap();
		frame(ParquetReader::new(std::io::Cursor::new(b)).finish())
	});
	t.insert("w8.json", {
		let mut b = Vec::new();
		JsonWriter::new(&mut b).finish(&mut sales()).unwrap();
		String::from_utf8(b).unwrap()
	});
	let _ = series_repr;
	// the composed workflows, step for step as their scripts
	t.insert(
		"W1",
		frame((|| {
			let sales = CsvReadOptions::default()
				.with_has_header(true)
				.with_parse_options(CsvParseOptions::default().with_try_parse_dates(true))
				.into_reader_with_file_handle(std::io::Cursor::new(std::fs::read(data("sales.csv")).unwrap()))
				.finish()?;
			let regions = JsonReader::new(std::io::Cursor::new(std::fs::read(data("regions.json")).unwrap())).finish()?;
			sales.inner_join(&regions, ["region"], ["region"])
		})()),
	);
	t.insert(
		"W2",
		frame(
			sales()
				.lazy()
				.select([col("qty").mean().alias("mean"), col("qty").max().alias("max")])
				.collect(),
		),
	);
	t.insert(
		"W3",
		frame((|| {
			let mut d = sales()
				.lazy()
				.with_column(col("qty").cast(DataType::Float64))
				.with_column(col("qty").fill_null(lit(0.0)))
				.drop_nulls(Some(cols(["price"])))
				.filter(col("qty").gt(lit(0.5)))
				.collect()?
				.unique_stable(None, UniqueKeepStrategy::First, None)?;
			d.rename("qty", "quantity".into())?;
			Ok(d)
		})()),
	);
	t.insert(
		"W4",
		frame(
			sales()
				.lazy()
				.with_column(col("date").str().to_date(StrptimeOptions::default()))
				.with_column(col("date").dt().weekday().alias("weekday"))
				.with_column(col("product").str().to_uppercase().alias("product_uc"))
				.with_column(when(col("qty").gt(lit(3))).then(lit("big")).otherwise(lit("small")).alias("size"))
				.with_column((col("qty") * col("price")).alias("revenue"))
				.collect(),
		),
	);
	t.insert(
		"W5",
		frame(
			sales()
				.lazy()
				.group_by([col("region")])
				.agg([col("qty").sum().alias("qty"), col("id").count().alias("n")])
				.sort(["region"], Default::default())
				.collect(),
		),
	);
	t.insert(
		"W6",
		frame((|| {
			let joined = sales().lazy().join(
				regions().lazy(),
				[col("region")],
				[col("region")],
				JoinArgs::new(JoinType::Left),
			)?;
			pivot(joined)?.unpivot(None::<Vec<PlSmallStr>>, ["region"])
		})()),
	);
	t.insert(
		"W7",
		frame(
			sales()
				.lazy()
				.with_column(col("qty").sum().over([col("region")]).unwrap().alias("region_qty"))
				.with_column(col("qty").cum_sum(false).alias("running"))
				.with_column(col("price").rolling_mean(RollingOptionsFixedWindow::default()).alias("roll"))
				.with_column(col("qty").rank(RankOptions::default(), None).alias("rank"))
				.collect(),
		),
	);
	t.insert(
		"W8",
		frame((|| {
			let mut b = Vec::new();
			CsvWriter::new(&mut b).finish(&mut sales())?;
			let mut back = CsvReader::new(std::io::Cursor::new(b)).finish()?;
			let mut p = Vec::new();
			ParquetWriter::new(&mut p).finish(&mut back)?;
			let mut again = ParquetReader::new(std::io::Cursor::new(p)).finish()?;
			let mut j = Vec::new();
			JsonWriter::new(&mut j).finish(&mut again)?;
			Ok(again)
		})()),
	);
	t
}

/// The pinned `LazyFrame::pivot` (eight arguments), as the scripts call it.
fn pivot(lf: LazyFrame) -> PolarsResult<DataFrame> {
	let on_columns = sales()
		.select(["product"])?
		.unique_stable(None, UniqueKeepStrategy::First, None)?;
	lf.pivot(
		cols(["product"]),
		Arc::new(on_columns),
		cols(["region"]),
		cols(["qty"]),
		element().sum(),
		true,
		"_".into(),
		polars_core::frame::PivotColumnNaming::Auto,
	)
	.collect()
}

#[test]
fn twins_json() {
	let t = twins();
	let out = format!("{}/twins.json", std::env::var("PROBE_OUT").unwrap());
	std::fs::write(&out, serde_json::to_string_pretty(&t).unwrap()).unwrap();
	println!("twins: {} -> {out}", t.len());
}
