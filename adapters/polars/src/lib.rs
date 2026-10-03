//! Small, synchronous Polars extension. Engine I/O is joined before returning.
//!
//! The value contract (record 0141):
//! - `DataFrame`, `LazyFrame`, `Series` and `Expr` are never consumed: every
//!   method takes them by reference, so one value serves any number of views.
//! - Rune names share one value: after `let b = df;`, an in-place method
//!   (`&mut`: 22 on `DataFrame`, 1 on `LazyFrame`, 8 on `Series`) changes
//!   what every name sees. `clone()` first gives an independent wrapper over
//!   Polars' own clone: columns are shared until either side is written
//!   (copy-on-write), never a deep copy.
//! - Builders, readers, writers and expression namespaces are consumed by
//!   their by-value methods (catalogue text "consumes the receiver"); a
//!   second use is Rune's access error ("Cannot take, value is …").
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
// record 0141: an independent wrapper over Polars' shallow, copy-on-write clone
#[rune::function(instance, path = clone)]
fn clone_frame(frame: &DataFrame) -> DataFrame {
	DataFrame(frame.0.clone())
}
#[rune::function(instance, path = clone)]
fn clone_plan(plan: &LazyFrame) -> LazyFrame {
	LazyFrame(plan.0.clone())
}
#[rune::function(instance, path = clone)]
fn clone_expr(expr: &Expr) -> Expr {
	Expr(expr.0.clone())
}
// `Series` is a generated type, so its clone exists only with `generated`
#[cfg(feature = "generated")]
#[rune::function(instance, path = clone)]
fn clone_series(
	series: &generated::types::W_polars_core__series__Series,
) -> generated::types::W_polars_core__series__Series {
	generated::types::W_polars_core__series__Series(series.0.clone())
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
/// Record 0158: the preview's text for a Rust frame, for the allocation
/// control in `tests/preview_alloc.rs` only; not part of the Rune API.
#[doc(hidden)]
pub fn preview_text(frame: &p::DataFrame) -> Result<String, String> {
	preview::render(frame)
}
/// Record 0159: the HTML form, for the same allocation control only.
#[doc(hidden)]
pub fn preview_html(frame: &p::DataFrame) -> Result<String, String> {
	preview::render_html(frame)
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
	})?;
	// record 0159: the notebook's HTML table beside the text, one whole token
	// within the writer's own bound
	presenters.register_html::<DataFrame>(|frame, out| {
		out.push(&preview::render_html(&frame.0)?);
		Ok(())
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
	// record 0131: a text column as plain strings
	m.function("strings", dense::strings)
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
	m.function_meta(clone_frame).map_err(err)?;
	m.function_meta(clone_plan).map_err(err)?;
	m.function_meta(clone_expr).map_err(err)?;
	#[cfg(feature = "generated")]
	m.function_meta(clone_series).map_err(err)?;
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
			"polars::DataFrame::strings".into(),
			"strings(column) -> Result<Vec<String>>: a str column as strings; nulls refused by row, at most 65,536 rows and 64 MiB",
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
		(
			"polars::DataFrame::clone".into(),
			"clone() -> DataFrame: an independent frame; columns shared copy-on-write, so in-place methods on either side never change the other",
		),
		(
			"polars::LazyFrame::clone".into(),
			"clone() -> LazyFrame: an independent plan (never needed for views: no LazyFrame method consumes it)",
		),
		(
			"polars::Expr::clone".into(),
			"clone() -> Expr: an independent expression (never needed for views: no Expr method consumes it)",
		),
	];
	#[cfg(feature = "generated")]
	catalogue.push((
		"polars::Series::clone".into(),
		"clone() -> Series: an independent series; data shared copy-on-write, so in-place methods on either side never change the other",
	));
	#[cfg(feature = "generated")]
	catalogue.extend(
		generated::catalogue::CATALOGUE
			.iter()
			.map(|(k, v)| (k.to_string(), consuming_text(k).unwrap_or(v))),
	);
	Ok(catalogue)
}

/// Record 0141: a by-value receiver's marked catalogue text (generated,
/// sorted by path); `None` for every other path.
#[cfg(feature = "generated")]
fn consuming_text(path: &str) -> Option<&'static str> {
	let marked = generated::catalogue::CONSUMING;
	marked
		.binary_search_by(|(k, _)| (*k).cmp(path))
		.ok()
		.map(|i| marked[i].1)
}

// Record 0141: `clone()` gives a distinct Rune value, an alias the same
// one. Two exclusive borrows succeed only on distinct values.
#[cfg(test)]
mod clone_identity {
	use super::*;
	use rnx::rune::{Context, Module, Source, Sources, Value, Vm};
	use std::sync::Arc;

	fn pair(value: Value, script: &str) -> (Value, Value) {
		let mut polars = Module::with_crate("polars").unwrap();
		build(&mut polars).unwrap();
		let mut context = Context::with_default_modules().unwrap();
		context.install(polars).unwrap();
		let runtime = Arc::new(context.runtime().unwrap());
		let mut sources = Sources::new();
		sources.insert(Source::memory(script).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources)
			.with_context(&context)
			.build()
			.unwrap();
		let mut vm = Vm::new(runtime, Arc::new(unit));
		let out = vm.call(["main"], (value,)).unwrap();
		rune::from_value::<(Value, Value)>(out).unwrap()
	}

	fn distinct<T: rune::Any + rune::ToValue>(value: T) {
		let v = rune::to_value(value).unwrap();
		let (a, b) = pair(v.clone(), "pub fn main(a) { (a, a.clone()) }");
		let held = a.borrow_mut::<T>().unwrap();
		assert!(b.borrow_mut::<T>().is_ok(), "a clone is a distinct value");
		drop(held);
		let (a, b) = pair(v, "pub fn main(a) { let b = a; (a, b) }");
		let held = a.borrow_mut::<T>().unwrap();
		assert!(b.borrow_mut::<T>().is_err(), "an alias is the same value");
		drop(held);
	}

	fn frame() -> p::DataFrame {
		p::df!("x" => [3i64, 1, 2]).unwrap()
	}

	#[test]
	fn clone_is_distinct_and_alias_is_shared() {
		distinct(DataFrame(frame()));
		distinct(LazyFrame(frame().lazy()));
		distinct(Expr(p::col("x")));
		#[cfg(feature = "generated")]
		distinct(generated::types::W_polars_core__series__Series(
			<p::Series as p::NamedFrom<_, _>>::new("x".into(), [1i64, 2, 3]),
		));
	}

	#[test]
	fn catalogue_names_the_clones() {
		let mut m = Module::with_crate("polars").unwrap();
		let catalogue = build(&mut m).unwrap();
		let clones: Vec<&str> = catalogue
			.iter()
			.map(|(k, _)| k.as_str())
			.filter(|k| k.ends_with("::clone"))
			.collect();
		let mut want = vec![
			"polars::DataFrame::clone",
			"polars::LazyFrame::clone",
			"polars::Expr::clone",
		];
		if cfg!(feature = "generated") {
			want.push("polars::Series::clone");
		}
		assert_eq!(clones, want);
	}
}

// Record 0141's measures, run explicitly in release:
// `cargo test --release --lib clone_cost -- --ignored --nocapture`.
// Each step's time (median) and the live bytes it leaves while its result is
// held; buffer sharing is read from the first column's data pointer.
#[cfg(all(test, feature = "generated"))]
mod clone_cost {
	use super::*;
	use rnx::rune::{Context, Module, Source, Sources, Value, Vm};
	use std::sync::Arc;
	use std::time::Instant;

	const SCRIPT: &str = r#"
		pub fn copy(a) { a.clone() }
		pub fn rename(a) { a.rename("c0", "r0").unwrap() }
		pub fn sort(a) { a.sort_in_place(["r0"], polars::SortMultipleOptions::default_().with_order_descending(true)).unwrap() }
	"#;

	fn vm() -> Vm {
		let mut polars = Module::with_crate("polars").unwrap();
		build(&mut polars).unwrap();
		let mut context = Context::with_default_modules().unwrap();
		context.install(polars).unwrap();
		let runtime = Arc::new(context.runtime().unwrap());
		let mut sources = Sources::new();
		sources.insert(Source::memory(SCRIPT).unwrap()).unwrap();
		let unit = rune::prepare(&mut sources)
			.with_context(&context)
			.build()
			.unwrap();
		Vm::new(runtime, Arc::new(unit))
	}

	fn frame(rows: usize) -> p::DataFrame {
		let columns: Vec<p::Column> = (0..10)
			.map(|c| {
				let v: Vec<i64> = (0..rows as i64)
					.map(|r| (r * 7919 + c) % 1_000_003)
					.collect();
				<p::Series as p::NamedFrom<_, _>>::new(format!("c{c}").into(), v).into()
			})
			.collect();
		p::DataFrame::new(rows, columns).unwrap()
	}

	fn data(df: &p::DataFrame, i: usize) -> *const i64 {
		let s = df.columns()[i].as_materialized_series();
		s.i64()
			.unwrap()
			.downcast_iter()
			.next()
			.unwrap()
			.values()
			.as_ptr()
	}

	fn deep(df: &p::DataFrame) -> p::DataFrame {
		let columns: Vec<p::Column> = df
			.columns()
			.iter()
			.map(|c| {
				let s = c.as_materialized_series();
				let v: Vec<i64> = s.i64().unwrap().cont_slice().unwrap().to_vec();
				<p::Series as p::NamedFrom<_, _>>::new(s.name().clone(), v).into()
			})
			.collect();
		p::DataFrame::new(df.height(), columns).unwrap()
	}

	fn live() -> usize {
		rnx::allocation::live().unwrap()
	}

	fn median(mut v: Vec<f64>) -> f64 {
		v.sort_by(|a, b| a.partial_cmp(b).unwrap());
		v[v.len() / 2]
	}

	#[test]
	#[ignore]
	fn clone_cost() {
		let mut vm = vm();
		println!(
			"rows | clone µs | clone bytes | shared | rename µs | rename bytes | sort µs | sort bytes | original intact | deep copy µs | deep copy bytes"
		);
		for rows in [1usize, 100_000, 1_000_000] {
			let reps = if rows == 1_000_000 { 7 } else { 15 };
			let (mut tc, mut tr, mut ts, mut td) = (vec![], vec![], vec![], vec![]);
			let (mut bc, mut br, mut bs, mut bd) = (0, 0, 0, 0);
			let (mut shared, mut intact) = (true, true);
			for _ in 0..reps {
				let original = frame(rows);
				let expect = original.clone();
				let first = data(&original, 0);
				let a = rune::to_value(DataFrame(original)).unwrap();
				// clone
				let before = live();
				let t = Instant::now();
				let b: Value = vm.call(["copy"], (a.clone(),)).unwrap();
				tc.push(t.elapsed().as_secs_f64() * 1e6);
				bc = live() as isize as i64 - before as i64;
				shared &= data(&b.borrow_ref::<DataFrame>().unwrap().0, 0) == first;
				// metadata mutation of the clone
				let before = live();
				let t = Instant::now();
				vm.call(["rename"], (b.clone(),)).unwrap();
				tr.push(t.elapsed().as_secs_f64() * 1e6);
				br = live() as i64 - before as i64;
				// data-changing mutation of the clone
				let before = live();
				let t = Instant::now();
				vm.call(["sort"], (b.clone(),)).unwrap();
				ts.push(t.elapsed().as_secs_f64() * 1e6);
				bs = live() as i64 - before as i64;
				{
					let a = a.borrow_ref::<DataFrame>().unwrap();
					intact &= data(&a.0, 0) == first && a.0.equals(&expect);
				}
				// the deep-copy baseline, in Rust
				let a = a.borrow_ref::<DataFrame>().unwrap();
				let before = live();
				let t = Instant::now();
				let d = deep(&a.0);
				td.push(t.elapsed().as_secs_f64() * 1e6);
				bd = live() as i64 - before as i64;
				assert!(d.equals(&a.0) && data(&d, 0) != first);
			}
			println!(
				"{rows} | {:.1} | {bc} | {shared} | {:.1} | {br} | {:.1} | {bs} | {intact} | {:.1} | {bd}",
				median(tc),
				median(tr),
				median(ts),
				median(td)
			);
		}
	}
}
