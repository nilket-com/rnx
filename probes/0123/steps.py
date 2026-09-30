"""Record 0122: the workflow steps, written as a notebook user would write
them against the rnx Polars adapter. Each step is a standalone script: it
builds its own input from the fixtures (`{DATA}` is the data directory,
`{OUT}` a scratch directory) and prints `polars::oracle_repr` of its
result, which the probe compares with the step's Rust twin.

A step's `code` is the body of `main`; `load` names a shared setup (below)
the step starts from, so an early missing API cannot hide a later blocker.
"""

# shared setups: each yields `df` (the sales frame) through the most basic
# path that works today (the hand-written reader), so a step exercises only
# its own operation
SETUPS = {
    "sales": 'let df = polars::read_csv("{DATA}/sales.csv", [("id", "i64"), ("date", "string"), ("region", "string"), ("product", "string"), ("qty", "i64"), ("price", "f64"), ("note", "string")])?;',
    "sales_dates": 'let df = polars::read_csv("{DATA}/sales.csv", [("id", "i64"), ("date", "string"), ("region", "string"), ("product", "string"), ("qty", "i64"), ("price", "f64"), ("note", "string")])?.lazy().with_columns([polars::col("date").str().to_date(polars::StrptimeOptions::default_())])?.collect()?;',
    "regions": 'let regions = polars::read_csv("{DATA}/regions.csv", [("region", "string"), ("manager", "string"), ("target", "i64")])?;',
}

STEPS = [
    # W1 load
    ("w1.csv_file", "load a CSV file, as a user first tries it", None, 'polars::read_csv("{DATA}/sales.csv")?'),
    ("w1.csv_file_schema", "load a CSV file with its schema spelled out", None, 'polars::read_csv("{DATA}/sales.csv", [("id", "i64"), ("date", "string"), ("region", "string"), ("product", "string"), ("qty", "i64"), ("price", "f64"), ("note", "string")])?'),
    ("w1.csv_bytes", "load CSV from bytes, parsing dates", None,
     'let bytes = fs::read_bytes("{DATA}/sales.csv")?;\n'
     'let opts = polars::CsvReadOptions::default_().with_has_header(true).with_parse_options(polars::CsvParseOptions::default_().with_try_parse_dates(true));\n'
     'polars::CsvReader::new(bytes)?.with_options(opts).finish()?'),
    ("w1.parquet_file", "load a Parquet file", None, 'polars::read_parquet("{OUT}/sales.parquet")?'),
    ("w1.parquet_bytes", "load Parquet from bytes", None,
     'let bytes = fs::read_bytes("{OUT}/sales.parquet")?;\npolars::ParquetReader::new(bytes)?.finish()?'),
    ("w1.json_bytes", "load JSON from bytes", None,
     'let bytes = fs::read_bytes("{DATA}/regions.json")?;\npolars::JsonReader::new(bytes)?.finish()?'),
    ("w1.scan_parquet", "scan Parquet lazily", None,
     'polars::LazyFrame::scan_parquet("{OUT}/sales.parquet", polars::ScanArgsParquet::default_())?.collect()?'),
    # W2 inspect
    ("w2.schema", "the schema", "sales", '`${df.schema()}`'),
    ("w2.head", "the first rows", "sales", 'df.head(Some(3))?'),
    ("w2.shape", "rows and columns", "sales", 'let s = df.shape()?; `${s.0}x${s.1}`'),
    ("w2.null_count", "nulls per column", "sales", 'df.null_count()'),
    ("w2.summary", "summary statistics", "sales",
     'df.lazy().select_([polars::col("qty").mean().alias("mean"), polars::col("qty").min().alias("min"), polars::col("qty").max().alias("max"), polars::col("qty").std(1)?.alias("std")])?.collect()?'),
    # W3 clean
    ("w3.cast", "cast a column", "sales",
     'df.lazy().with_column(polars::col("qty").cast(polars::DataType::Float64())).collect()?'),
    ("w3.rename", "rename a column", "sales", 'let d = df; d.rename("qty", "quantity")?; d'),
    ("w3.fill_null", "fill missing values", "sales",
     'df.lazy().with_column(polars::col("qty").fill_null(polars::lit(0)?)).collect()?'),
    ("w3.drop_nulls", "drop rows with a missing price", "sales",
     'df.lazy().drop_nulls(Some(polars::cols(["price"])?))?.collect()?'),
    ("w3.dedupe", "remove duplicate rows", "sales",
     'df.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?'),
    ("w3.filter", "keep rows by a condition", "sales",
     'df.lazy().filter(polars::col("qty").gt(polars::lit(2)?)).collect()?'),
    # W4 derive
    ("w4.string", "a string transformation", "sales",
     'df.lazy().with_column(polars::col("product").str().to_uppercase().alias("product_uc")).collect()?'),
    ("w4.parse_date", "parse a date column", "sales",
     'df.lazy().with_column(polars::col("date").str().to_date(polars::StrptimeOptions::default_())).collect()?'),
    ("w4.temporal", "a temporal part", "sales_dates",
     'df.lazy().with_column(polars::col("date").dt().weekday().alias("weekday")).collect()?'),
    ("w4.conditional", "a conditional column", "sales",
     'df.lazy().with_column(polars::when(polars::col("qty").gt(polars::lit(3)?)).then(polars::lit("big")?).otherwise(polars::lit("small")?).alias("size")).collect()?'),
    ("w4.arithmetic", "arithmetic between columns", "sales",
     'df.lazy().with_column((polars::col("qty") * polars::col("price"))?.alias("revenue")).collect()?'),
    # W5 group and aggregate
    ("w5.eager_group_by", "group and aggregate, eagerly", "sales",
     'df.group_by(["region"])?.count()?'),
    ("w5.lazy_group_by", "group and aggregate, lazily", "sales",
     'df.lazy().group_by([polars::col("region")])?.agg([polars::col("qty").sum().alias("qty"), polars::col("price").mean().alias("avg_price"), polars::col("id").count().alias("n")])?.sort(["region"])?.collect()?'),
    ("w5.multi_key", "group by two keys", "sales",
     'df.lazy().group_by([polars::col("region"), polars::col("product")])?.agg([polars::col("qty").sum()])?.sort(["region", "product"])?.collect()?'),
    # W6 reshape
    ("w6.join_eager", "join two frames, eagerly", "sales",
     '{regions}\ndf.inner_join(regions, ["region"], ["region"])?'),
    ("w6.join_lazy", "join two frames, lazily", "sales",
     '{regions}\ndf.lazy().join(regions.lazy(), [polars::col("region")], [polars::col("region")], polars::JoinArgs::new(polars::JoinType::Left()))?.collect()?'),
    ("w6.pivot", "pivot wide", "sales",
     'let on_columns = df.select_(["product"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?;\n'
     'df.lazy().pivot(polars::cols(["product"])?, on_columns, polars::cols(["region"])?, polars::cols(["qty"])?, polars::element().sum(), true, "_", polars::PivotColumnNaming::Auto())?.collect()?'),
    ("w6.unpivot", "unpivot long", "sales",
     'df.unpivot(Some(["qty", "price"]), ["id"])?'),
    ("w6.concat", "stack two frames", "sales",
     'polars::concat([df.lazy(), df.lazy()], polars::UnionArgs::default_())?.collect()?'),
    # W7 windows
    ("w7.over", "a windowed aggregate", "sales",
     'df.lazy().with_column(polars::col("qty").sum().over([polars::col("region")])?.alias("region_qty")).collect()?'),
    ("w7.cum_sum", "a cumulative sum", "sales",
     'df.lazy().with_column(polars::col("qty").cum_sum(false).alias("running")).collect()?'),
    ("w7.rolling", "a rolling mean", "sales",
     'df.lazy().with_column(polars::col("price").rolling_mean(polars::RollingOptionsFixedWindow::default_()).alias("roll")).collect()?'),
    ("w7.rank", "a rank", "sales",
     'df.lazy().with_column(polars::col("qty").rank(polars::RankOptions::default_(), None)?.alias("rank")).collect()?'),
    # W8 output
    ("w8.csv", "write CSV", "sales",
     'let sink = polars::Sink::new(); polars::CsvWriter::new(sink).finish(df)?; String::from_utf8(sink.bytes()?)?'),
    ("w8.parquet", "write Parquet and read it back", "sales",
     'let sink = polars::Sink::new(); polars::ParquetWriter::new(sink).finish(df)?; polars::ParquetReader::new(sink.bytes()?)?.finish()?'),
    ("w8.json", "write JSON", "sales",
     'let sink = polars::Sink::new(); polars::JsonWriter::new(sink).finish(df)?; String::from_utf8(sink.bytes()?)?'),
    ("w8.present", "present the result", "sales", 'df.preview()?'),
]


# a presentation step has no Rust analogue: it is checked by the markers its
# output must contain, and counted apart from works/differs/blocked
PRESENTATION = {"w8.present": ["region", "north", "apple"]}

# the composed workflows: each runs its steps in sequence on one frame, as a
# notebook would, and prints the final result (compared with its twin)
WORKFLOWS = [
    ("W1", "load each format and combine", None,
     'let bytes = fs::read_bytes("{DATA}/sales.csv")?;\n'
     'let opts = polars::CsvReadOptions::default_().with_has_header(true).with_parse_options(polars::CsvParseOptions::default_().with_try_parse_dates(true));\n'
     'let sales = polars::CsvReader::new(bytes)?.with_options(opts).finish()?;\n'
     'let archived = polars::LazyFrame::scan_parquet("{OUT}/sales.parquet", polars::ScanArgsParquet::default_())?.collect()?;\n'
     'let regions = polars::JsonReader::new(fs::read_bytes("{DATA}/regions.json")?)?.finish()?;\n'
     'let plain = polars::read_csv("{DATA}/sales.csv")?;\n'
     'sales.inner_join(regions, ["region"], ["region"])?'),
    ("W2", "inspect a frame", "sales",
     'println!("{}", df.schema());\n'
     'let s = df.shape()?;\n'
     'df.lazy().select_([polars::col("qty").mean().alias("mean"), polars::col("qty").max().alias("max")])?.collect()?'),
    ("W3", "clean a frame", "sales",
     'let lf = df.lazy().with_column(polars::col("qty").cast(polars::DataType::Float64())).with_column(polars::col("qty").fill_null(polars::lit(0.0)?));\n'
     'let d = lf.drop_nulls(Some(polars::cols(["price"])?))?.filter(polars::col("qty").gt(polars::lit(0.5)?)).collect()?;\n'
     'let d = d.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?;\n'
     'd.rename("qty", "quantity")?;\n'
     'd'),
    ("W4", "derive columns", "sales",
     'df.lazy()\n'
     '  .with_column(polars::col("date").str().to_date(polars::StrptimeOptions::default_()))\n'
     '  .with_column(polars::col("date").dt().weekday().alias("weekday"))\n'
     '  .with_column(polars::col("product").str().to_uppercase().alias("product_uc"))\n'
     '  .with_column(polars::when(polars::col("qty").gt(polars::lit(3)?)).then(polars::lit("big")?).otherwise(polars::lit("small")?).alias("size"))\n'
     '  .with_column((polars::col("qty") * polars::col("price"))?.alias("revenue"))\n'
     '  .collect()?'),
    ("W5", "group and aggregate", "sales",
     'let counts = df.group_by(["region"])?.count()?;\n'
     'df.lazy().group_by([polars::col("region")])?.agg([polars::col("qty").sum().alias("qty"), polars::col("id").count().alias("n")])?.sort(["region"])?.collect()?'),
    ("W6", "reshape", "sales",
     '{regions}\n'
     'let joined = df.lazy().join(regions.lazy(), [polars::col("region")], [polars::col("region")], polars::JoinArgs::new(polars::JoinType::Left()))?;\n'
     'let on_columns = df.select_(["product"])?.unique_stable(None, polars::UniqueKeepStrategy::First(), None)?;\n'
     'let wide = joined.pivot(polars::cols(["product"])?, on_columns, polars::cols(["region"])?, polars::cols(["qty"])?, polars::element().sum(), true, "_", polars::PivotColumnNaming::Auto())?.collect()?;\n'
     'wide.unpivot(None, ["region"])?'),
    ("W7", "windows", "sales",
     'df.lazy()\n'
     '  .with_column(polars::col("qty").sum().over([polars::col("region")])?.alias("region_qty"))\n'
     '  .with_column(polars::col("qty").cum_sum(false).alias("running"))\n'
     '  .with_column(polars::col("price").rolling_mean(polars::RollingOptionsFixedWindow::default_()).alias("roll"))\n'
     '  .with_column(polars::col("qty").rank(polars::RankOptions::default_(), None)?.alias("rank"))\n'
     '  .collect()?'),
    ("W8", "write out and read back", "sales",
     'let csv = polars::Sink::new(); polars::CsvWriter::new(csv).finish(df)?;\n'
     'let back = polars::CsvReader::new(csv.bytes()?)?.finish()?;\n'
     'let pq = polars::Sink::new(); polars::ParquetWriter::new(pq).finish(back)?;\n'
     'let again = polars::ParquetReader::new(pq.bytes()?)?.finish()?;\n'
     'let js = polars::Sink::new(); polars::JsonWriter::new(js).finish(again)?;\n'
     'println!("{}", again.preview()?);\n'
     'again'),
]


def script(step):
    sid, _title, setup, code = step
    body = code.replace("{regions}", SETUPS["regions"])
    pre = SETUPS[setup] if setup else ""
    return (
        "pub fn main(args) {\n"
        f"    {pre}\n"
        "    let result = {\n"
        + "".join(f"        {line}\n" for line in body.split("\n"))
        + "    };\n"
        "    println!(\"{}\", polars::oracle_repr(result)?);\n"
        "    Ok(())\n"
        "}\n"
    )
