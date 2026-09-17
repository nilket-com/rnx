//! Small, synchronous Polars extension. Engine I/O is joined before returning.
use p::IntoLazy;
use polars::prelude as p;
use rnx::rune::{self, runtime::Vec as RuneVec};
mod engine;
mod files;
mod preview;
mod values;

#[derive(rune::Any)]
#[rune(item = ::polars)]
struct DataFrame(p::DataFrame);
#[derive(rune::Any)]
#[rune(item = ::polars)]
struct LazyFrame(p::LazyFrame);
#[derive(rune::Any)]
#[rune(item = ::polars)]
struct LazyGroupBy(p::LazyGroupBy);
#[derive(rune::Any)]
#[rune(item = ::polars)]
struct Expr(p::Expr);
fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
}
fn expressions(value: rune::Value, operation: &str) -> Result<Vec<p::Expr>, String> {
	let v = value
		.borrow_ref::<RuneVec>()
		.map_err(|_| format!("polars {operation}: expected a vector of expressions"))?;
	v.iter()
		.map(|v| {
			v.borrow_ref::<Expr>()
				.map(|e| e.0.clone())
				.map_err(|_| format!("polars {operation}: expected an expression"))
		})
		.collect()
}
#[rune::function(instance)]
fn lazy(frame: &DataFrame) -> LazyFrame {
	LazyFrame(frame.0.clone().lazy())
}
#[rune::function(instance)]
fn filter(plan: &LazyFrame, expr: &Expr) -> LazyFrame {
	LazyFrame(plan.0.clone().filter(expr.0.clone()))
}
#[rune::function(instance)]
fn group_by(plan: &LazyFrame, keys: rune::Value) -> Result<LazyGroupBy, String> {
	Ok(LazyGroupBy(
		plan.0.clone().group_by(expressions(keys, "group_by")?),
	))
}
#[rune::function(instance)]
fn agg(group: &LazyGroupBy, values: rune::Value) -> Result<LazyFrame, String> {
	Ok(LazyFrame(group.0.clone().agg(expressions(values, "agg")?)))
}
#[rune::function(instance)]
fn collect(plan: &LazyFrame) -> Result<DataFrame, String> {
	let plan = plan.0.clone();
	engine::run(move || {
		let frame = plan.collect().map_err(|e| format!("polars collect: {e}"))?;
		files::validate(&frame)?;
		Ok(frame)
	})?
	.map(DataFrame)
}
#[rune::function(instance)]
fn gt(expr: &Expr, rhs: &Expr) -> Expr {
	Expr(expr.0.clone().gt(rhs.0.clone()))
}
#[rune::function(instance)]
fn add(expr: &Expr, rhs: &Expr) -> Expr {
	Expr(expr.0.clone() + rhs.0.clone())
}
#[rune::function(instance, protocol = ADD)]
fn add_protocol(expr: &Expr, rhs: &Expr) -> Expr {
	Expr(expr.0.clone() + rhs.0.clone())
}
#[rune::function(instance)]
fn sum(expr: &Expr) -> Expr {
	Expr(expr.0.clone().sum())
}
#[rune::function(instance)]
fn alias(expr: &Expr, name: &str) -> Expr {
	Expr(expr.0.clone().alias(name))
}

#[rune::function(instance)]
fn sort(plan: &LazyFrame, names: rune::Value) -> Result<LazyFrame, String> {
	let values = names
		.borrow_ref::<RuneVec>()
		.map_err(|_| "polars sort: expected a vector of column names")?;
	let names = values
		.iter()
		.map(|v| {
			v.borrow_string_ref()
				.map(|s| p::PlSmallStr::from_str(&s))
				.map_err(|_| "polars sort: expected column names".to_string())
		})
		.collect::<Result<Vec<_>, _>>()?;
	Ok(LazyFrame(
		plan.0
			.clone()
			.sort(names, p::SortMultipleOptions::default()),
	))
}
#[rune::function(instance)]
fn preview(frame: &DataFrame) -> Result<String, String> {
	preview::render(&frame.0)
}
#[rune::function(instance)]
fn write_parquet_new(frame: &DataFrame, path: &str) -> Result<(), String> {
	let frame = frame.0.clone();
	let path = path.to_owned();
	engine::run(move || files::write(frame, &path))?
}
fn read_csv(path: &str, schema: rune::Value) -> Result<DataFrame, String> {
	let schema = values::schema(schema)?;
	let path = path.to_owned();
	engine::run(move || files::csv(&path, schema))?.map(DataFrame)
}
fn read_parquet(path: &str) -> Result<DataFrame, String> {
	let path = path.to_owned();
	engine::run(move || files::parquet(&path))?.map(DataFrame)
}
/// Install into an rnx-created `polars` module. All native values remain opaque.
pub fn build(m: &mut rune::Module) -> Result<Vec<(String, &'static str)>, String> {
	m.ty::<DataFrame>().map_err(err)?;
	m.ty::<LazyFrame>().map_err(err)?;
	m.ty::<LazyGroupBy>().map_err(err)?;
	m.ty::<Expr>().map_err(err)?;
	m.function("read_csv", read_csv).build().map_err(err)?;
	m.function("read_parquet", read_parquet)
		.build()
		.map_err(err)?;
	m.function("col", |name: &str| Expr(p::col(name)))
		.build()
		.map_err(err)?;
	m.function("lit", values::literal).build().map_err(err)?;
	m.function_meta(lazy).map_err(err)?;
	m.function_meta(filter).map_err(err)?;
	m.function_meta(group_by).map_err(err)?;
	m.function_meta(agg).map_err(err)?;
	m.function_meta(sort).map_err(err)?;
	m.function_meta(collect).map_err(err)?;
	m.function_meta(gt).map_err(err)?;
	m.function_meta(add).map_err(err)?;
	m.function_meta(add_protocol).map_err(err)?;
	m.function_meta(sum).map_err(err)?;
	m.function_meta(alias).map_err(err)?;
	m.function_meta(preview).map_err(err)?;
	m.function_meta(write_parquet_new).map_err(err)?;
	#[cfg(feature = "test-support")]
	m.function("engine_counts", engine::counts)
		.build()
		.map_err(err)?;

	Ok(vec![
		(
			"polars::DataFrame::preview".into(),
			"preview() -> Result<String>: dimensions and bounded data; 10 rows, 8 columns, 80 scalars per name/cell, 8192 bytes",
		),
		(
			"polars::read_csv".into(),
			"read_csv(path, schema) -> Result<DataFrame>: strict local CSV; ordered (name, dtype) schema",
		),
		(
			"polars::read_parquet".into(),
			"read_parquet(path) -> Result<DataFrame>: local Parquet, string/i64/f64/bool columns only",
		),
		(
			"polars::col".into(),
			"col(name) -> Expr: deferred column reference",
		),
		(
			"polars::lit".into(),
			"lit(value) -> Result<Expr>: bool, i64, finite f64 or string",
		),
		(
			"polars::DataFrame::write_parquet_new".into(),
			"write_parquet_new(path) -> Result<()>: uncompressed, create-new; failure may leave a partial file",
		),
	])
}
