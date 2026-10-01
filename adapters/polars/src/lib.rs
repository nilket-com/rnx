//! Small, synchronous Polars extension. Engine I/O is joined before returning.
use p::IntoLazy;
use polars::prelude as p;
use rnx::rune::{self, runtime::Vec as RuneVec};
mod dense;
mod engine;
mod files;
#[cfg(feature = "generated")]
#[doc(hidden)]
pub mod generated;
#[cfg(all(feature = "generated", feature = "test-support"))]
#[doc(hidden)]
pub mod oracle;
mod preview;
mod values;

#[derive(rune::Any, Clone)]
#[rune(item = ::polars)]
struct DataFrame(pub(crate) p::DataFrame);
#[derive(rune::Any, Clone)]
#[rune(item = ::polars)]
struct LazyFrame(pub(crate) p::LazyFrame);
#[derive(rune::Any, Clone)]
#[rune(item = ::polars)]
struct LazyGroupBy(pub(crate) p::LazyGroupBy);
#[derive(rune::Any, Clone)]
#[rune(item = ::polars)]
struct Expr(pub(crate) p::Expr);
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
	engine::run("LazyFrame::collect", move || {
		// record 0122: a collected frame keeps whatever dtypes the query
		// produced (a count is UInt32, a parsed date Date), as the generated
		// eager operations' frames always have; record 0058 refused every
		// dtype outside its four, which blocked every workflow that counts,
		// ranks or parses dates. The hand-written readers and writers keep
		// their four-dtype contract; `preview` refuses what it cannot show.
		plan.collect().map_err(|e| format!("polars collect: {e}"))
	})
	.map_err(err)?
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
/// Explicit `format!("{frame}")` shows the same bounded preview.
#[rune::function(instance, protocol = DISPLAY_FMT)]
fn display_fmt(frame: &DataFrame, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
	use rune::alloc::fmt::TryWrite;
	match preview::render(&frame.0) {
		Ok(text) => rune::vm_write!(f, "{text}"),
		Err(e) => rune::vm_write!(f, "<polars::DataFrame: preview unavailable: {e}>"),
	}
}
/// Record 0068: register the session presenter for `DataFrame`. Lazy plans,
/// group-bys and expressions stay opaque; presenting one must never run it.
pub fn present(presenters: &mut rnx::Presenters) -> Result<(), String> {
	presenters.register::<DataFrame>(|frame, out| {
		preview::render_into(&frame.0, &mut |token| out.push(token)).map(|_| ())
	})
}
#[rune::function(instance)]
fn write_parquet_new(frame: &DataFrame, path: &str) -> Result<(), String> {
	let frame = frame.0.clone();
	let path = path.to_owned();
	engine::run("DataFrame::write_parquet_new", move || {
		files::write(frame, &path)
	})
	.map_err(err)?
}
fn read_csv(path: &str, schema: rune::Value) -> Result<DataFrame, String> {
	let schema = values::schema(schema)?;
	let path = path.to_owned();
	engine::run("read_csv", move || files::csv(&path, schema))
		.map_err(err)?
		.map(DataFrame)
}
/// Record 0125: `read_csv(path)` without a schema reads with Polars' own
/// schema inference, date parsing off (the 0.55.2 date-parsing crash).
fn read_csv_inferred(path: &str) -> Result<DataFrame, String> {
	let path = path.to_owned();
	engine::run("read_csv", move || files::csv_inferred(&path))
		.map_err(err)?
		.map(DataFrame)
}
/// Record 0125: one `read_csv` for both forms. Rune has no optional
/// arguments, so it is a raw function that dispatches on the count: one
/// argument infers the schema, two are the strict `(name, dtype)` schema.
fn read_csv_either(
	stack: &mut dyn rune::runtime::Memory,
	addr: rune::runtime::InstAddress,
	len: usize,
	out: rune::runtime::Output,
) -> rune::runtime::VmResult<()> {
	use rune::runtime::VmResult;
	let args = rune::vm_try!(stack.slice_at(addr, len));
	let result = match args {
		[path] => {
			// borrowed, never taken: the script keeps its path (review of 0125)
			let path = rune::vm_try!(path.borrow_string_ref()).to_string();
			read_csv_inferred(&path)
		}
		[path, schema] => {
			// borrowed, never taken: the script keeps its path (review of 0125)
			let path = rune::vm_try!(path.borrow_string_ref()).to_string();
			read_csv(&path, schema.clone())
		}
		_ => {
			return VmResult::panic(format!(
				"read_csv takes (path) or (path, schema), found {len} arguments"
			));
		}
	};
	rune::vm_try!(out.store(stack, || rune::to_value(result)));
	VmResult::Ok(())
}
/// Record 0125: a local JSON file, beside `read_csv` and `read_parquet`.
fn read_json(path: &str) -> Result<DataFrame, String> {
	let path = path.to_owned();
	engine::run("read_json", move || files::json(&path))
		.map_err(err)?
		.map(DataFrame)
}
fn read_parquet(path: &str) -> Result<DataFrame, String> {
	let path = path.to_owned();
	engine::run("read_parquet", move || files::parquet(&path))
		.map_err(err)?
		.map(DataFrame)
}
/// Record 0122 (test support): the oracle's structural text of a script
/// value, so a workflow probe compares a script's result with its Rust twin.
/// A lazy frame is collected; other values show their scalar text.
#[cfg(all(feature = "generated", feature = "test-support"))]
fn oracle_repr(v: rune::Value) -> Result<String, String> {
	use generated::types::{W_polars_core__frame__column__Column, W_polars_core__series__Series};
	if let Ok(df) = v.borrow_ref::<DataFrame>() {
		return Ok(oracle::frame_repr(&df.0).to_text());
	}
	if let Ok(lf) = v.borrow_ref::<LazyFrame>() {
		let df = lf.0.clone().collect().map_err(err)?;
		return Ok(oracle::frame_repr(&df).to_text());
	}
	if let Ok(s) = v.borrow_ref::<W_polars_core__series__Series>() {
		return Ok(oracle::series_repr(&s.0).to_text());
	}
	if let Ok(c) = v.borrow_ref::<W_polars_core__frame__column__Column>() {
		return Ok(oracle::column_repr(&c.0).to_text());
	}
	for f in [
		|v: &rune::Value| rune::from_value::<String>(v.clone()).ok(),
		|v: &rune::Value| {
			rune::from_value::<i64>(v.clone())
				.ok()
				.map(|x| x.to_string())
		},
		|v: &rune::Value| {
			rune::from_value::<f64>(v.clone())
				.ok()
				.map(|x| format!("{x:?}"))
		},
		|v: &rune::Value| {
			rune::from_value::<bool>(v.clone())
				.ok()
				.map(|x| x.to_string())
		},
	] {
		if let Some(s) = f(&v) {
			return Ok(s);
		}
	}
	Err(format!(
		"oracle_repr: no structural text for {}",
		v.type_info()
	))
}

/// Install into an rnx-created `polars` module. All native values remain opaque.
pub fn build(m: &mut rune::Module) -> Result<Vec<(String, &'static str)>, String> {
	#[cfg(feature = "test-support")]
	if let Some(path) = std::env::var_os("RNX_POLARS_BUILD_MARKER") {
		use std::io::Write;
		std::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(path)
			.and_then(|mut file| file.write_all(b"polars builder\n"))
			.map_err(|e| format!("Polars test builder marker: {e}"))?;
	}

	m.ty::<DataFrame>().map_err(err)?;
	m.ty::<LazyFrame>().map_err(err)?;
	m.ty::<LazyGroupBy>().map_err(err)?;
	m.ty::<Expr>().map_err(err)?;
	m.raw_function("read_csv", read_csv_either)
		.build()
		.map_err(err)?;
	m.function("read_json", read_json).build().map_err(err)?;
	// record 0129: numeric columns to and from rnx's neutral block
	m.function("to_dense", dense::to_dense)
		.build_associated::<DataFrame>()
		.map_err(err)?;
	m.function("from_dense", dense::from_dense)
		.build_associated::<DataFrame>()
		.map_err(err)?;
	m.function("with_dense", dense::with_dense)
		.build_associated::<DataFrame>()
		.map_err(err)?;
	m.function("read_parquet", read_parquet)
		.build()
		.map_err(err)?;
	m.function("col", |name: &str| Expr(p::col(name)))
		.build()
		.map_err(err)?;
	m.function("lit", values::literal).build().map_err(err)?;
	m.function("version", || String::from(polars::VERSION))
		.build()
		.map_err(err)?;
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
	m.function_meta(display_fmt).map_err(err)?;
	m.function_meta(write_parquet_new).map_err(err)?;
	#[cfg(all(feature = "generated", feature = "test-support"))]
	m.function("oracle_repr", oracle_repr)
		.build()
		.map_err(err)?;
	// record 0127: the wide-binding control, registered typed here and raw
	// by the generated install (the same function, two conventions)
	#[cfg(all(feature = "generated", feature = "test-support"))]
	{
		m.function("wide_control_typed", generated::support::wide_control)
			.build_associated::<DataFrame>()
			.map_err(err)?;
		m.function("wide_control_vm_typed", generated::support::wide_control_vm)
			.build_associated::<DataFrame>()
			.map_err(err)?;
	}
	#[cfg(feature = "test-support")]
	m.function("engine_counts", engine::counts)
		.build()
		.map_err(err)?;
	// Record 0073: generated bindings, registered after the hand-written
	// ones so a hand-written name always wins.
	#[cfg(feature = "generated")]
	{
		generated::support::install(m).map_err(err)?;
		generated::types::install(m).map_err(err)?;
		generated::functions::install(m).map_err(err)?;
	}

	let mut catalogue = vec![
		(
			"polars::DataFrame::preview".into(),
			"preview() -> Result<String>: dimensions and bounded data; 10 rows, 8 columns, 80 scalars per name/cell, 8192 bytes",
		),
		(
			"polars::read_csv".into(),
			"read_csv(path[, schema]) -> Result<DataFrame>: local CSV; the schema inferred by Polars (dates stay strings), or a strict ordered (name, dtype) schema",
		),
		(
			"polars::DataFrame::to_dense".into(),
			"to_dense(columns, dtype) -> Result<interchange::Dense>: listed numeric columns as an \"f32\"/\"f64\" row-major block; nulls, non-numeric, non-finite and inexact values refused",
		),
		(
			"polars::DataFrame::from_dense".into(),
			"from_dense(block) -> Result<DataFrame>: a new frame of the block's columns",
		),
		(
			"polars::DataFrame::with_dense".into(),
			"with_dense(block) -> Result<DataFrame>: this frame with the block's columns appended (same height)",
		),
		(
			"polars::read_json".into(),
			"read_json(path) -> Result<DataFrame>: local JSON array of objects, schema inferred by Polars; string/i64/f64/bool columns only",
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
			"polars::version".into(),
			"version() -> String: the Rust polars crate version this executable was built with",
		),
		(
			"polars::DataFrame::write_parquet_new".into(),
			"write_parquet_new(path) -> Result<()>: uncompressed, create-new; failure may leave a partial file",
		),
	];
	#[cfg(feature = "generated")]
	catalogue.extend(
		generated::catalogue::CATALOGUE
			.iter()
			.map(|(k, v)| (k.to_string(), *v)),
	);
	Ok(catalogue)
}
