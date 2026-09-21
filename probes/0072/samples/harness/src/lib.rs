//! Record 0072 sample harness. `generated.rs` is written by gen.py; this
//! file is the fixed part: build a Rune context with the generated module,
//! run one script, and give back its result as a string.
#![allow(non_camel_case_types, non_snake_case, unused, clippy::all)]
pub use polars::prelude as p;
pub use polars::prelude::{IntoLazy, IntoColumn};
use rune::{Context, Diagnostics, Source, Sources, Vm};
use std::sync::Arc;

pub mod generated;

pub fn context() -> Result<Context, String> {
    let mut m = rune::Module::with_crate("s").map_err(|e| e.to_string())?;
    generated::install(&mut m).map_err(|e| e.to_string())?;
    let mut context = Context::with_default_modules().map_err(|e| e.to_string())?;
    context.install(m).map_err(|e| e.to_string())?;
    Ok(context)
}

/// Run `pub fn main()` of `src`; the script returns a `Vec<String>`.
pub fn run(src: &str) -> Result<Vec<String>, String> {
    match std::panic::catch_unwind(|| run_inner(src)) {
        Ok(r) => r,
        Err(e) => Err(format!("panic: {}", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())),
    }
}

fn run_inner(src: &str) -> Result<Vec<String>, String> {
    let context = context()?;
    let runtime = Arc::new(context.runtime().map_err(|e| e.to_string())?);
    let mut sources = Sources::new();
    sources.insert(Source::memory(src).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut diagnostics = Diagnostics::new();
    let unit = rune::prepare(&mut sources).with_context(&context).with_diagnostics(&mut diagnostics).build();
    let unit = match unit {
        Ok(u) => u,
        Err(e) => {
            let mut out = String::new();
            for d in diagnostics.diagnostics() {
                out.push_str(&format!("{d:?}\n"));
            }
            return Err(format!("compile: {e}\n{out}"));
        }
    };
    let mut vm = Vm::new(runtime, Arc::new(unit));
    let out = vm.call(["main"], ()).map_err(|e| format!("vm: {e}"))?;
    let out: Vec<String> = rune::from_value(out).map_err(|e| format!("result: {e}"))?;
    Ok(out)
}

/// Deterministic fixtures shared by the Rune side and the Rust oracle.
pub mod fx {
    use super::p;
    use polars::prelude::*;
    pub fn df() -> p::DataFrame {
        polars::df!("x" => [1i64, 2, 3], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap()
    }
    pub fn lf() -> p::LazyFrame { df().lazy() }
    pub fn expr() -> p::Expr { p::col("x") }
    pub fn series() -> p::Series { p::Series::new("x".into(), [1i64, 2, 3]) }
    pub fn column() -> p::Column { series().into_column() }
    pub fn dtype() -> p::DataType { p::DataType::Int64 }
    pub fn schema() -> p::Schema { df().schema().as_ref().clone() }
    pub fn field() -> p::Field { p::Field::new("x".into(), p::DataType::Int64) }
    pub fn group_by() -> p::LazyGroupBy { lf().group_by([p::col("y")]) }
}

/// Oracle formatting: one deterministic string per value type.
pub mod show {
    use super::p;
    pub fn df(v: &p::DataFrame) -> String { format!("{v:?}") }
    pub fn lf(v: &p::LazyFrame) -> String {
        match v.clone().collect() {
            Ok(d) => format!("collected {d:?}"),
            Err(e) => format!("collect error: {e}"),
        }
    }
    /// Expressions are executed, not printed: select against the fixture
    /// frame and collect, so deferred callbacks actually run.
    pub fn expr(v: &p::Expr) -> String {
        use polars::prelude::*;
        match super::fx::df().lazy().select([v.clone()]).collect() {
            Ok(d) => format!("{d:?}"),
            Err(e) => format!("error: {e}"),
        }
    }
    pub fn series(v: &p::Series) -> String { format!("{v:?}") }
    pub fn column(v: &p::Column) -> String { format!("{v:?}") }
    pub fn dtype(v: &p::DataType) -> String { format!("{v:?}") }
    pub fn schema(v: &p::Schema) -> String { format!("{v:?}") }
    pub fn field(v: &p::Field) -> String { format!("{v:?}") }
    pub fn group_by(v: &p::LazyGroupBy) -> String {
        match v.clone().agg([p::col("x").sum()]).collect() {
            Ok(d) => format!("agg {d:?}"),
            Err(e) => format!("agg error: {e}"),
        }
    }
}

/// Test-side result recording.
pub fn record(name: &str, value: serde_json::Value) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&value).unwrap()).unwrap();
}
