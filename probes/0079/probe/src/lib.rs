//! Record 0079 gate 2: a scratch adapter with the callback bridge the
//! generator would emit. Nothing here is production; `adapters/polars`
//! is untouched. The plan's decisions, as code:
//!
//! - decision 1: a Rune `Function` becomes a `SyncFunction` before any
//!   Polars work; a non-constant capture is refused as `CallbackCapture`;
//! - decision 2: every callback call goes through `bridge`; a failure is a
//!   `CallbackFailure`; a fallible Polars signature carries it as
//!   `ComputeError`, an infallible one unwinds it, and `engine::run` (the
//!   one boundary) turns the payload into `EngineFailure::Callback`;
//! - decision 3: a callback may not call a routed binding (`engine::run`
//!   refuses under the thread-local guard) and may not start under another
//!   (the bridge refuses `nested callback` before any budget is set);
//!   `apply_mut` commits on success only;
//! - decision 4: an opt-in per-invocation instruction budget, read at each
//!   invocation, restored on every exit.

use polars::prelude as p;
use polars::prelude::{IntoLazy, IntoSeries, LazyFileListReader, NamedFrom};
use rune::runtime::{Function, SyncFunction};
use rune::{Any, Module};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};

// ---- wrappers (the adapter's shape, reduced to what the probe exercises) ----

#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct Series(pub p::Series);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct Column(pub p::Column);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct DataFrame(pub p::DataFrame);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct LazyFrame(pub p::LazyFrame);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct Expr(pub p::Expr);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct Schema(pub p::Schema);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct Field(pub p::Field);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct Int64Chunked(pub p::Int64Chunked);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct LazyCsvReader(pub p::LazyCsvReader);
#[derive(Any, Clone)]
#[rune(item = ::polars)]
pub struct PolarsError(pub Arc<p::PolarsError>);
/// The script-level error: `kind()` is what Rust matches on.
#[derive(Any, Clone, Debug)]
#[rune(item = ::polars)]
pub struct Error(pub String, pub String);

impl From<p::PolarsError> for Error {
	fn from(e: p::PolarsError) -> Self {
		let kind = format!("{e:?}");
		let kind: String = kind.chars().take_while(|c| c.is_alphanumeric()).collect();
		Error(if kind.is_empty() { "PolarsError".into() } else { kind }, e.to_string())
	}
}

#[rune::function(instance)]
fn kind(e: &Error) -> String {
	e.0.clone()
}
#[rune::function(instance)]
fn message(e: &Error) -> String {
	e.1.clone()
}

// ---- the bridge (decisions 1 to 4) ----

/// What a callback invocation reported: the installing operation and the cause.
#[derive(Debug, Clone)]
pub struct CallbackFailure {
	pub op: String,
	pub cause: String,
}

impl CallbackFailure {
	pub fn text(&self) -> String {
		format!("callback {}: {}", self.op, self.cause)
	}
}

thread_local! {
	/// Depth of callback invocations on this thread: the re-entry guard.
	static IN_CALLBACK: Cell<u32> = const { Cell::new(0) };
}

struct Guard;
impl Guard {
	fn enter() -> Guard {
		IN_CALLBACK.with(|c| c.set(c.get() + 1));
		Guard
	}
}
impl Drop for Guard {
	fn drop(&mut self) {
		IN_CALLBACK.with(|c| c.set(c.get() - 1));
	}
}
pub fn in_callback() -> bool {
	IN_CALLBACK.with(|c| c.get() > 0)
}

/// Decision 4: the process-wide instruction budget per invocation; 0 is none.
static CALLBACK_BUDGET: AtomicUsize = AtomicUsize::new(0);
/// Observation instrumentation (counters, thread set) is on only for the
/// controls that read it; the timed measurements run without it, so the
/// bridge measured is the bridge proposed.
static OBSERVE: AtomicBool = AtomicBool::new(false);
/// Restoration checks, on the thread that entered the bridge: after every
/// invocation the guard must be clear and the thread's instruction
/// allowance unlimited again (a 3000-instruction probe loop must run under
/// no budget); a failure of either is counted, never silently passed.
static RESTORE_PROBE: Mutex<Option<Arc<SyncFunction>>> = Mutex::new(None);
/// Whether a restoration probe is installed: read before any lock, so a
/// timed bridge call touches no observer synchronization at all.
static RESTORE_ON: AtomicBool = AtomicBool::new(false);
static RESTORE_FAILURES: AtomicUsize = AtomicUsize::new(0);
static RESTORE_LAST: Mutex<String> = Mutex::new(String::new());
/// Rune's own text for a halted execution, matched exactly; a user panic
/// whose message contains the word is `Panicked: ...` and stays itself.
const HALT_LIMITED: &str = "Halted for unexpected reason `limited`";
/// Evidence switch (decision 3's experiments only): lets a routed binding run from a callback.
static ALLOW_REENTRY: AtomicBool = AtomicBool::new(false);
/// Observation counters for the controls.
static CALLS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static MAX_ACTIVE: AtomicUsize = AtomicUsize::new(0);
static THREADS: Mutex<Vec<std::thread::ThreadId>> = Mutex::new(Vec::new());
static BARRIER: Mutex<Option<Arc<Barrier>>> = Mutex::new(None);

/// Decision 1: the conversion, before any Polars work.
pub fn install(op: &str, f: Function) -> Result<Arc<SyncFunction>, Error> {
	f.into_sync().map(Arc::new).map_err(|e| Error("CallbackCapture".into(), format!("callback {op}: a captured value is not a constant: {e}")))
}

/// Decision 2: one bridge for every invocation. The guard is checked before
/// the budget is set, so an invocation under another callback (only a path
/// the routing rule missed could produce one) fails before replacing any
/// allowance.
pub fn bridge<A, R>(op: &str, f: &SyncFunction, args: A) -> Result<R, CallbackFailure>
where
	A: rune::runtime::GuardedArgs,
	R: rune::runtime::FromValue,
{
	if in_callback() {
		return Err(CallbackFailure { op: op.into(), cause: "nested callback: a callback invoked while another is running on this thread".into() });
	}
	let r = {
		let _guard = Guard::enter();
		let observe = OBSERVE.load(Ordering::Relaxed);
		struct Active(bool);
		impl Drop for Active {
			fn drop(&mut self) {
				if self.0 {
					ACTIVE.fetch_sub(1, Ordering::SeqCst);
				}
			}
		}
		let _active = Active(observe);
		if observe {
			CALLS.fetch_add(1, Ordering::SeqCst);
			let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
			MAX_ACTIVE.fetch_max(active, Ordering::SeqCst);
			let id = std::thread::current().id();
			let mut t = THREADS.lock().unwrap();
			if !t.contains(&id) {
				t.push(id);
			}
		}
		let budget = CALLBACK_BUDGET.load(Ordering::SeqCst);
		// exhaustion is Rune's halt, matched exactly, with the allowance
		// confirmed spent inside the budget scope; anything else keeps its text
		let (r, exhausted) = if budget > 0 {
			rune::runtime::budget::with(budget, || {
				let r = f.call::<rune::Value>(args);
				let spent = r.is_err() && { let mut g = rune::runtime::budget::acquire(); !g.take() };
				(r, spent)
			})
			.call()
		} else {
			(f.call::<rune::Value>(args), false)
		};
		match r {
			rune::runtime::VmResult::Ok(v) => {
				let type_name = v.type_info().to_string();
				rune::from_value::<R>(v).map_err(|e| CallbackFailure { op: op.into(), cause: format!("wrong result type: got {type_name} ({e})") })
			}
			rune::runtime::VmResult::Err(e) => {
				let text = e.to_string();
				let cause = if exhausted && text == HALT_LIMITED { format!("instruction budget {budget} exhausted") } else { format!("call failed: {text}") };
				Err(CallbackFailure { op: op.into(), cause })
			}
		}
	};
	restoration_check();
	r
}

/// On the thread that ran the invocation, after its guard and budget scope
/// ended: the guard is clear and the allowance is unlimited again.
fn restoration_check() {
	if !RESTORE_ON.load(Ordering::Relaxed) {
		return;
	}
	let probe = RESTORE_PROBE.lock().unwrap().clone();
	let Some(probe) = probe else { return };
	if in_callback() {
		RESTORE_FAILURES.fetch_add(1, Ordering::SeqCst);
		*RESTORE_LAST.lock().unwrap() = "guard still set after the invocation".into();
	}
	if let Err(e) = probe.call::<i64>(()).into_result() {
		RESTORE_FAILURES.fetch_add(1, Ordering::SeqCst);
		*RESTORE_LAST.lock().unwrap() = format!("probe loop after the invocation: {e}");
	}
}
pub fn restore_last() -> String {
	RESTORE_LAST.lock().unwrap().clone()
}
pub fn set_restore_probe(f: Option<Function>) -> Result<(), Error> {
	let installed = match f { Some(f) => Some(install("restore-probe", f)?), None => None };
	RESTORE_ON.store(installed.is_some(), Ordering::SeqCst);
	*RESTORE_PROBE.lock().unwrap() = installed;
	Ok(())
}
pub fn restore_failures() -> usize {
	RESTORE_FAILURES.load(Ordering::SeqCst)
}
pub fn set_observe(on: bool) {
	OBSERVE.store(on, Ordering::SeqCst);
}
#[rune::function(path = observe)]
fn observe_rune(on: bool) {
	set_observe(on);
}
#[rune::function(path = set_restore_probe)]
fn set_restore_probe_rune(f: Function) -> Result<(), Error> {
	set_restore_probe(Some(f))
}
#[rune::function(path = restore_failures)]
fn restore_failures_rune() -> i64 {
	restore_failures() as i64
}
#[rune::function(path = guard_depth)]
fn guard_depth() -> i64 {
	IN_CALLBACK.with(|c| c.get()) as i64
}
/// A native function that panics (not a callback failure), for the restoration controls.
#[rune::function(path = panic_native)]
fn panic_native() {
	panic!("native-panic-marker")
}

/// An infallible Polars signature has no error channel: the failure unwinds
/// as its own payload, for `engine::run` to translate.
pub fn unwind<T>(e: CallbackFailure) -> T {
	std::panic::resume_unwind(Box::new(e))
}

fn compute_error(e: CallbackFailure) -> p::PolarsError {
	p::PolarsError::ComputeError(e.text().into())
}

// ---- the engine boundary ----

#[derive(Debug)]
pub enum EngineFailure {
	NoThread(String),
	Callback(String),
	Reentry(String),
}

pub mod engine {
	use super::*;
	/// The adapter's `engine::run` with decision 2's translation and decision 3's check.
	pub fn run<T: Send>(binding: &str, call: impl FnOnce() -> T + Send) -> Result<T, EngineFailure> {
		if in_callback() && !ALLOW_REENTRY.load(Ordering::SeqCst) {
			return Err(EngineFailure::Reentry(format!("callback: `{binding}` is a routed binding and may not be called from a callback")));
		}
		std::thread::scope(|scope| {
			let worker = std::thread::Builder::new().name("rnx-polars-engine".into()).spawn_scoped(scope, call).map_err(|e| EngineFailure::NoThread(format!("cannot start Polars engine thread: {e}")))?;
			match worker.join() {
				Ok(v) => Ok(v),
				Err(payload) => match payload.downcast::<CallbackFailure>() {
					Ok(f) => Err(EngineFailure::Callback(f.text())),
					Err(other) => std::panic::resume_unwind(other),
				},
			}
		})
	}
}

fn routed<T: Send>(binding: &str, call: impl FnOnce() -> Result<T, p::PolarsError> + Send) -> Result<T, Error> {
	match engine::run(binding, call) {
		Ok(Ok(v)) => Ok(v),
		Ok(Err(e)) => Err(Error::from(e)),
		Err(EngineFailure::Callback(t)) => Err(Error("CallbackError".into(), t)),
		Err(EngineFailure::Reentry(t)) => Err(Error("CallbackError".into(), t)),
		Err(EngineFailure::NoThread(t)) => panic!("{t}"),
	}
}

// ---- the six operations (plus the two the plan added) ----

/// `Expr::map(function, output_type)`: both closures stored in the plan.
#[rune::function(instance, path = map_cb)]
fn expr_map(this: &Expr, function: Function, output_type: Function) -> Result<Expr, Error> {
	let f = install("Expr::map", function)?;
	let g = install("Expr::map", output_type)?;
	let expr = this.0.clone();
	routed("Expr::map", move || {
		Ok::<_, p::PolarsError>(expr.map(
			move |c: p::Column| -> p::PolarsResult<p::Column> { bridge::<_, Column>("Expr::map", &f, (Column(c),)).map(|c| c.0).map_err(compute_error) },
			move |s: &p::Schema, fld: &p::Field| -> p::PolarsResult<p::Field> { bridge::<_, Field>("Expr::map", &g, (Schema(s.clone()), Field(fld.clone()))).map(|f| f.0).map_err(compute_error) },
		))
	})
	.map(Expr)
}

/// `Expr::map_many(function, arguments, output_type)`: the `&mut [Column]`
/// arrives as a vector of clones and is never read back.
#[rune::function(instance, path = map_many_cb)]
fn expr_map_many(this: &Expr, function: Function, arguments: Vec<rune::Value>, output_type: Function) -> Result<Expr, Error> {
	let f = install("Expr::map_many", function)?;
	let g = install("Expr::map_many", output_type)?;
	let expr = this.0.clone();
	let args: Vec<p::Expr> = exprs(arguments)?;
	routed("Expr::map_many", move || {
		Ok::<_, p::PolarsError>(expr.map_many(
			move |cols: &mut [p::Column]| -> p::PolarsResult<p::Column> {
				let v: Vec<Column> = cols.iter().cloned().map(Column).collect();
				bridge::<_, Column>("Expr::map_many", &f, (v,)).map(|c| c.0).map_err(compute_error)
			},
			&args,
			move |s: &p::Schema, flds: &[p::Field]| -> p::PolarsResult<p::Field> {
				let v: Vec<Field> = flds.iter().cloned().map(Field).collect();
				bridge::<_, Field>("Expr::map_many", &g, (Schema(s.clone()), v)).map(|f| f.0).map_err(compute_error)
			},
		))
	})
	.map(Expr)
}

/// `Column::apply_unary_elementwise(f)`: an infallible signature, invoked immediately.
#[rune::function(instance, path = apply_unary_elementwise_cb)]
fn column_apply_unary(this: &Column, f: Function) -> Result<Column, Error> {
	let f = install("Column::apply_unary_elementwise", f)?;
	let col = this.0.clone();
	routed("Column::apply_unary_elementwise", move || {
		Ok::<_, p::PolarsError>(col.apply_unary_elementwise(|s: &p::Series| -> p::Series { bridge::<_, Series>("Column::apply_unary_elementwise", &f, (Series(s.clone()),)).map(|s| s.0).unwrap_or_else(unwind) }))
	})
	.map(Column)
}

/// The same operation built without the routing rule: the bridge runs on
/// the calling thread and an infallible failure unwinds into the script.
/// The control the rule exists for; never emitted.
#[rune::function(instance, path = apply_unary_elementwise_unrouted)]
fn column_apply_unary_unrouted(this: &Column, f: Function) -> Result<Column, Error> {
	let f = install("Column::apply_unary_elementwise", f)?;
	Ok(Column(this.0.apply_unary_elementwise(|s: &p::Series| -> p::Series { bridge::<_, Series>("Column::apply_unary_elementwise", &f, (Series(s.clone()),)).map(|s| s.0).unwrap_or_else(unwind) })))
}

/// `DataFrame::apply_columns_par(f)`: invoked from the rayon pool, in parallel.
#[rune::function(instance, path = apply_columns_par_cb)]
fn df_apply_columns_par(this: &DataFrame, f: Function) -> Result<Vec<Column>, Error> {
	let f = install("DataFrame::apply_columns_par", f)?;
	let df = this.0.clone();
	routed("DataFrame::apply_columns_par", move || {
		Ok::<_, p::PolarsError>(df.apply_columns_par(|c: &p::Column| -> p::Column { bridge::<_, Column>("DataFrame::apply_columns_par", &f, (Column(c.clone()),)).map(|c| c.0).unwrap_or_else(unwind) }))
	})
	.map(|v| v.into_iter().map(Column).collect())
}

/// `Int64Chunked::apply_mut(f)`: in place in Polars; here on a clone,
/// committed only when every element succeeded (decision 3).
#[rune::function(instance, path = apply_mut_cb)]
fn i64_apply_mut(this: &mut Int64Chunked, f: Function) -> Result<(), Error> {
	let f = install("Int64Chunked::apply_mut", f)?;
	let mut work = this.0.clone();
	let done = routed("Int64Chunked::apply_mut", move || {
		work.apply_mut(|x: i64| -> i64 { bridge::<_, i64>("Int64Chunked::apply_mut", &f, (x,)).unwrap_or_else(unwind) });
		Ok::<_, p::PolarsError>(work)
	})?;
	this.0 = done;
	Ok(())
}

/// The bare in-place form, for the cost of the commit clone: same bridge,
/// no clone, the receiver mutated as Polars mutates it. A control, never emitted.
#[rune::function(instance, path = apply_mut_in_place)]
fn i64_apply_mut_in_place(this: &mut Int64Chunked, f: Function) -> Result<(), Error> {
	let f = install("Int64Chunked::apply_mut", f)?;
	let mut work = std::mem::take(&mut this.0);
	let r = engine::run("Int64Chunked::apply_mut", || {
		let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			work.apply_mut(|x: i64| -> i64 { bridge::<_, i64>("Int64Chunked::apply_mut", &f, (x,)).unwrap_or_else(unwind) });
		}));
		(work, r)
	});
	match r {
		Ok((work, inner)) => {
			this.0 = work;
			match inner {
				Ok(()) => Ok(()),
				Err(payload) => match payload.downcast::<CallbackFailure>() {
					Ok(f) => Err(Error("CallbackError".into(), f.text())),
					Err(other) => std::panic::resume_unwind(other),
				},
			}
		}
		Err(EngineFailure::Reentry(t)) | Err(EngineFailure::Callback(t)) => Err(Error("CallbackError".into(), t)),
		Err(EngineFailure::NoThread(t)) => panic!("{t}"),
	}
}

/// `ChunkedArray::apply_into_string_amortized(f)`: the `&mut String` buffer
/// is the result; the Rune callback returns the string the bridge writes.
#[rune::function(instance, path = apply_into_string_amortized_cb)]
fn i64_apply_into_string(this: &Int64Chunked, f: Function) -> Result<Series, Error> {
	let f = install("ChunkedArray::apply_into_string_amortized", f)?;
	let ca = this.0.clone();
	routed("ChunkedArray::apply_into_string_amortized", move || {
		let out = ca.apply_into_string_amortized(|x: i64, buf: &mut String| {
			let s = bridge::<_, String>("ChunkedArray::apply_into_string_amortized", &f, (x,)).unwrap_or_else(unwind);
			buf.push_str(&s);
		});
		Ok::<_, p::PolarsError>(out.into_series())
	})
	.map(Series)
}

/// `LazyCsvReader::with_schema_modify(f)`: invoked immediately, the schema inferred first.
#[rune::function(instance, path = with_schema_modify_cb)]
fn csv_with_schema_modify(this: &LazyCsvReader, f: Function) -> Result<LazyCsvReader, Error> {
	let f = install("LazyCsvReader::with_schema_modify", f)?;
	let r = this.0.clone();
	routed("LazyCsvReader::with_schema_modify", move || {
		r.with_schema_modify(|s: p::Schema| -> p::PolarsResult<p::Schema> { bridge::<_, Schema>("LazyCsvReader::with_schema_modify", &f, (Schema(s),)).map(|s| s.0).map_err(compute_error) })
	})
	.map(LazyCsvReader)
}

/// `PolarsError::wrap_msg(f)`: the immediate non-data-owner case; routed by the rule.
#[rune::function(instance, path = wrap_msg_cb)]
fn error_wrap_msg(this: &PolarsError, f: Function) -> Result<PolarsError, Error> {
	let f = install("PolarsError::wrap_msg", f)?;
	let e = this.0.clone();
	routed("PolarsError::wrap_msg", move || Ok::<_, p::PolarsError>(e.wrap_msg(|m: &str| -> String { bridge::<_, String>("PolarsError::wrap_msg", &f, (m.to_string(),)).unwrap_or_else(unwind) }))).map(|e| PolarsError(Arc::new(e)))
}

/// `wrap_msg` without the rule: unrouted, the failure unwinds into the script. A control.
#[rune::function(instance, path = wrap_msg_unrouted)]
fn error_wrap_msg_unrouted(this: &PolarsError, f: Function) -> Result<PolarsError, Error> {
	let f = install("PolarsError::wrap_msg", f)?;
	Ok(PolarsError(Arc::new(this.0.wrap_msg(|m: &str| -> String { bridge::<_, String>("PolarsError::wrap_msg", &f, (m.to_string(),)).unwrap_or_else(unwind) }))))
}

// ---- sinks, plan construction, fixtures and observation ----

#[rune::function(instance)]
fn lazy(df: &DataFrame) -> LazyFrame {
	LazyFrame(df.0.clone().lazy())
}
/// Wrapped values in a script vector are borrowed and cloned out, never
/// taken: the script keeps its expressions (a `Vec<Expr>` parameter would
/// move each element out of the script's values).
fn exprs(v: Vec<rune::Value>) -> Result<Vec<p::Expr>, Error> {
	v.iter().map(|x| x.borrow_ref::<Expr>().map(|e| e.0.clone()).map_err(|e| Error("TypeError".into(), format!("expected an Expr: {e}")))).collect()
}
#[rune::function(instance)]
fn select_(lf: &LazyFrame, exprs_: Vec<rune::Value>) -> Result<LazyFrame, Error> {
	Ok(LazyFrame(lf.0.clone().select(exprs(exprs_)?)))
}
#[rune::function(instance)]
fn with_columns(lf: &LazyFrame, exprs_: Vec<rune::Value>) -> Result<LazyFrame, Error> {
	Ok(LazyFrame(lf.0.clone().with_columns(exprs(exprs_)?)))
}
/// A sink: plan execution.
#[rune::function(instance)]
fn collect(lf: &LazyFrame) -> Result<DataFrame, Error> {
	let plan = lf.0.clone();
	routed("LazyFrame::collect", move || plan.collect()).map(DataFrame)
}
/// A sink: schema resolution through `DslPlan::compute_schema` (the path Codex reproduced).
#[rune::function(instance)]
fn compute_schema(lf: &LazyFrame) -> Result<Schema, Error> {
	let plan = lf.0.logical_plan.clone();
	routed("DslPlan::compute_schema", move || plan.compute_schema().map(|s| (*s).clone())).map(Schema)
}
#[rune::function(instance)]
fn describe_plan(lf: &LazyFrame) -> Result<String, Error> {
	let plan = lf.0.clone();
	routed("LazyFrame::describe_plan", move || plan.describe_plan())
}
#[rune::function(path = col)]
fn col(name: &str) -> Expr {
	Expr(p::col(name))
}
#[rune::function(path = lit)]
fn lit(v: i64) -> Expr {
	Expr(p::lit(v))
}
#[rune::function(instance)]
fn alias(e: &Expr, name: &str) -> Expr {
	Expr(e.0.clone().alias(name))
}
#[rune::function(instance, path = add)]
fn expr_add(e: &Expr, rhs: &Expr) -> Expr {
	Expr(e.0.clone() + rhs.0.clone())
}
#[rune::function(instance, path = mul_lit)]
fn expr_mul(e: &Expr, k: i64) -> Expr {
	Expr(e.0.clone() * p::lit(k))
}
/// A routed data binding, for the re-entry controls.
#[rune::function(instance, path = sum_routed)]
fn series_sum(s: &Series) -> Result<i64, Error> {
	let s = s.0.clone();
	routed("Series::sum", move || s.sum::<i64>())
}
#[rune::function(instance, path = sum_routed)]
fn column_sum(c: &Column) -> Result<i64, Error> {
	let s = c.0.as_materialized_series().clone();
	routed("Column::sum", move || s.sum::<i64>())
}
/// A Polars-internal panic inside the engine (not a callback failure): stays a panic.
#[rune::function(instance, path = panic_inside_engine)]
fn series_panic_inside(s: &Series) -> Result<i64, Error> {
	let _ = s.0.clone();
	routed("Series::panic_inside_engine", move || -> Result<i64, p::PolarsError> { panic!("polars-internal-panic-marker") })
}
/// Mutates the received value in place (a callback that mutates its argument).
#[rune::function(instance, path = zero_first)]
fn series_zero_first(s: &mut Series) {
	let mut ca = s.0.i64().unwrap().clone();
	ca.apply_mut(|_| 0);
	s.0 = ca.into_series();
}
/// An unrouted binding (plan construction), allowed inside a callback.
#[rune::function(instance, path = times)]
fn series_times(s: &Series, k: i64) -> Series {
	Series(&s.0 * k)
}
#[rune::function(instance, path = plus)]
fn series_plus(s: &Series, k: i64) -> Series {
	Series(&s.0 + k)
}
#[rune::function(instance, path = len)]
fn series_len(s: &Series) -> i64 {
	s.0.len() as i64
}
#[rune::function(instance, path = name)]
fn series_name(s: &Series) -> String {
	s.0.name().to_string()
}
#[rune::function(instance, path = i64s)]
fn series_i64s(s: &Series) -> Result<Vec<Option<i64>>, Error> {
	Ok(s.0.iter().map(|av| av.extract::<i64>()).collect())
}
#[rune::function(instance, path = strs)]
fn series_strs(s: &Series) -> Result<Vec<Option<String>>, Error> {
	Ok(s.0.iter().map(|av| av.get_str().map(String::from)).collect())
}
#[rune::function(instance, path = series)]
fn column_series(c: &Column) -> Series {
	Series(c.0.as_materialized_series().clone())
}
#[rune::function(instance, path = times)]
fn column_times(c: &Column, k: i64) -> Column {
	Column((c.0.as_materialized_series() * k).into())
}
#[rune::function(instance, path = name)]
fn column_name(c: &Column) -> String {
	c.0.name().to_string()
}
#[rune::function(instance, path = len)]
fn column_len(c: &Column) -> i64 {
	c.0.len() as i64
}
#[rune::function(instance, path = i64s)]
fn column_i64s(c: &Column) -> Result<Vec<Option<i64>>, Error> {
	Ok(c.0.as_materialized_series().iter().map(|av| av.extract::<i64>()).collect())
}
#[rune::function(instance, path = cast_f64)]
fn column_cast_f64(c: &Column) -> Result<Column, Error> {
	c.0.cast(&p::DataType::Float64).map(Column).map_err(Error::from)
}
#[rune::function(instance, path = column)]
fn df_column(df: &DataFrame, name: &str) -> Result<Column, Error> {
	df.0.column(name).map(|c| Column(c.clone())).map_err(Error::from)
}
#[rune::function(instance, path = width)]
fn df_width(df: &DataFrame) -> i64 {
	df.0.width() as i64
}
#[rune::function(instance, path = height)]
fn df_height(df: &DataFrame) -> i64 {
	df.0.height() as i64
}
#[rune::function(instance, path = show)]
fn df_show(df: &DataFrame) -> String {
	format!("{:?}", df.0)
}
#[rune::function(instance, path = name)]
fn field_name(f: &Field) -> String {
	f.0.name().to_string()
}
#[rune::function(instance, path = dtype_name)]
fn field_dtype(f: &Field) -> String {
	format!("{}", f.0.dtype())
}
#[rune::function(free, path = Field::f64)]
fn field_f64(name: &str) -> Field {
	Field(p::Field::new(name.into(), p::DataType::Float64))
}
#[rune::function(free, path = Field::i64)]
fn field_i64(name: &str) -> Field {
	Field(p::Field::new(name.into(), p::DataType::Int64))
}
#[rune::function(free, path = Field::string)]
fn field_string(name: &str) -> Field {
	Field(p::Field::new(name.into(), p::DataType::String))
}
#[rune::function(instance, path = names)]
fn schema_names(s: &Schema) -> Vec<String> {
	s.0.iter_names().map(|n| n.to_string()).collect()
}
#[rune::function(instance, path = dtype_name)]
fn schema_dtype(s: &Schema, name: &str) -> Option<String> {
	s.0.get(name).map(|d| format!("{d}"))
}
#[rune::function(instance, path = with_all_f64)]
fn schema_all_f64(s: &Schema) -> Schema {
	Schema(s.0.iter_names().map(|n| p::Field::new(n.clone(), p::DataType::Float64)).collect())
}
#[rune::function(instance, path = series)]
fn i64_series(ca: &Int64Chunked) -> Series {
	Series(ca.0.clone().into_series())
}
#[rune::function(instance, path = is_sorted_flag)]
fn i64_sorted(ca: &Int64Chunked) -> String {
	format!("{:?}", ca.0.is_sorted_flag())
}
#[rune::function(instance, path = values)]
fn i64_values(ca: &Int64Chunked) -> Vec<Option<i64>> {
	ca.0.to_vec()
}
#[rune::function(instance, path = len)]
fn i64_len(ca: &Int64Chunked) -> i64 {
	ca.0.len() as i64
}
#[rune::function(instance, path = null_count)]
fn i64_nulls(ca: &Int64Chunked) -> i64 {
	ca.0.null_count() as i64
}
#[rune::function(free, path = LazyCsvReader::new)]
fn csv_new(path: &str) -> LazyCsvReader {
	LazyCsvReader(p::LazyCsvReader::new(p::PlRefPath::from(path)))
}
#[rune::function(instance, path = finish)]
fn csv_finish(r: &LazyCsvReader) -> Result<LazyFrame, Error> {
	let r = r.0.clone();
	routed("LazyCsvReader::finish", move || r.finish()).map(LazyFrame)
}
#[rune::function(instance, path = message)]
fn error_message(e: &PolarsError) -> String {
	e.0.to_string()
}
#[rune::function(free, path = PolarsError::compute)]
fn error_compute(msg: &str) -> PolarsError {
	PolarsError(Arc::new(p::PolarsError::ComputeError(msg.to_string().into())))
}

pub mod fixtures {
	use super::*;
	pub fn df() -> p::DataFrame {
		p::df!("x" => [1i64, 2, 3], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap()
	}
	pub fn wide(cols: usize, rows: usize) -> p::DataFrame {
		let columns: Vec<p::Column> = (0..cols).map(|i| p::Series::new(format!("c{i}").into(), (0..rows as i64).collect::<Vec<_>>()).into()).collect();
		p::DataFrame::new(rows, columns).unwrap()
	}
	pub fn sorted_i64(n: usize) -> p::Int64Chunked {
		let mut ca = p::Int64Chunked::new("x".into(), (0..n as i64).collect::<Vec<_>>());
		ca.set_sorted_flag(polars_core::series::IsSorted::Ascending);
		ca
	}
	pub fn nullable_i64() -> p::Int64Chunked {
		let mut ca = p::Int64Chunked::new("x".into(), [Some(1i64), None, Some(3), Some(4), Some(5)]);
		ca.set_sorted_flag(polars_core::series::IsSorted::Ascending);
		ca
	}
}

#[rune::function(path = df)]
fn fx_df() -> DataFrame {
	DataFrame(fixtures::df())
}
#[rune::function(path = wide)]
fn fx_wide(cols: i64, rows: i64) -> DataFrame {
	DataFrame(fixtures::wide(cols as usize, rows as usize))
}
#[rune::function(path = series)]
fn fx_series() -> Series {
	Series(p::Series::new("x".into(), [1i64, 2, 3]))
}
#[rune::function(path = column)]
fn fx_column() -> Column {
	Column(p::Series::new("x".into(), [1i64, 2, 3]).into())
}
#[rune::function(path = sorted_i64)]
fn fx_sorted(n: i64) -> Int64Chunked {
	Int64Chunked(fixtures::sorted_i64(n as usize))
}
#[rune::function(path = nullable_i64)]
fn fx_nullable() -> Int64Chunked {
	Int64Chunked(fixtures::nullable_i64())
}
#[rune::function(path = wrapped)]
fn fx_wrapped() -> Series {
	Series(p::Series::new("w".into(), [7i64]))
}
#[rune::function(path = one)]
fn fx_one() -> i64 {
	// a runtime value the compiler cannot fold (0072's capture control)
	std::hint::black_box(1)
}

// ---- controls' knobs ----

#[rune::function(path = set_callback_budget)]
fn set_budget(n: i64) {
	CALLBACK_BUDGET.store(n.max(0) as usize, Ordering::SeqCst);
}
pub fn set_callback_budget(n: usize) {
	CALLBACK_BUDGET.store(n, Ordering::SeqCst);
}
pub fn allow_reentry(on: bool) {
	ALLOW_REENTRY.store(on, Ordering::SeqCst);
}
#[rune::function(path = allow_reentry_for_the_experiment)]
fn allow_reentry_rune(on: bool) {
	allow_reentry(on);
}
pub fn reset_counters() {
	CALLS.store(0, Ordering::SeqCst);
	ACTIVE.store(0, Ordering::SeqCst);
	MAX_ACTIVE.store(0, Ordering::SeqCst);
	THREADS.lock().unwrap().clear();
}
pub fn counters() -> (usize, usize, usize) {
	(CALLS.load(Ordering::SeqCst), MAX_ACTIVE.load(Ordering::SeqCst), THREADS.lock().unwrap().len())
}
#[rune::function(path = calls)]
fn calls_rune() -> i64 {
	CALLS.load(Ordering::SeqCst) as i64
}
#[rune::function(path = max_active)]
fn max_active_rune() -> i64 {
	MAX_ACTIVE.load(Ordering::SeqCst) as i64
}
#[rune::function(path = threads_seen)]
fn threads_rune() -> i64 {
	THREADS.lock().unwrap().len() as i64
}
#[rune::function(path = reset)]
fn reset_rune() {
	reset_counters();
}
/// A finite blocking native call inside a callback (decision 4's control).
#[rune::function(path = sleep_ms)]
fn sleep_ms(ms: i64) {
	std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}
/// Synchronizes `n` callback invocations before any proceeds (decision 3's experiments).
#[rune::function(path = barrier_init)]
fn barrier_init(n: i64) {
	*BARRIER.lock().unwrap() = Some(Arc::new(Barrier::new(n.max(1) as usize)));
}
#[rune::function(path = barrier_wait)]
fn barrier_wait() {
	let b = BARRIER.lock().unwrap().clone();
	if let Some(b) = b {
		b.wait();
	}
}
#[rune::function(path = rune_threads)]
fn rune_threads() -> i64 {
	polars_core::runtime::RAYON.current_num_threads() as i64
}
#[rune::function(path = thread_name)]
fn thread_name() -> String {
	std::thread::current().name().unwrap_or("?").to_string()
}

pub fn module() -> Result<Module, rune::ContextError> {
	let mut m = Module::with_crate("polars")?;
	m.ty::<Series>()?;
	m.ty::<Column>()?;
	m.ty::<DataFrame>()?;
	m.ty::<LazyFrame>()?;
	m.ty::<Expr>()?;
	m.ty::<Schema>()?;
	m.ty::<Field>()?;
	m.ty::<Int64Chunked>()?;
	m.ty::<LazyCsvReader>()?;
	m.ty::<PolarsError>()?;
	m.ty::<Error>()?;
	m.function_meta(kind)?;
	m.function_meta(message)?;
	m.function_meta(expr_map)?;
	m.function_meta(expr_map_many)?;
	m.function_meta(column_apply_unary)?;
	m.function_meta(column_apply_unary_unrouted)?;
	m.function_meta(df_apply_columns_par)?;
	m.function_meta(i64_apply_mut)?;
	m.function_meta(i64_apply_mut_in_place)?;
	m.function_meta(i64_apply_into_string)?;
	m.function_meta(csv_with_schema_modify)?;
	m.function_meta(error_wrap_msg)?;
	m.function_meta(error_wrap_msg_unrouted)?;
	m.function_meta(lazy)?;
	m.function_meta(select_)?;
	m.function_meta(with_columns)?;
	m.function_meta(collect)?;
	m.function_meta(compute_schema)?;
	m.function_meta(describe_plan)?;
	m.function_meta(col)?;
	m.function_meta(lit)?;
	m.function_meta(alias)?;
	m.function_meta(expr_add)?;
	m.function_meta(expr_mul)?;
	m.function_meta(series_sum)?;
	m.function_meta(series_panic_inside)?;
	m.function_meta(series_zero_first)?;
	m.function_meta(column_sum)?;
	m.function_meta(series_times)?;
	m.function_meta(series_plus)?;
	m.function_meta(series_len)?;
	m.function_meta(series_name)?;
	m.function_meta(series_i64s)?;
	m.function_meta(series_strs)?;
	m.function_meta(column_series)?;
	m.function_meta(column_times)?;
	m.function_meta(column_name)?;
	m.function_meta(column_len)?;
	m.function_meta(column_i64s)?;
	m.function_meta(column_cast_f64)?;
	m.function_meta(df_column)?;
	m.function_meta(df_width)?;
	m.function_meta(df_height)?;
	m.function_meta(df_show)?;
	m.function_meta(field_name)?;
	m.function_meta(field_dtype)?;
	m.function_meta(field_f64)?;
	m.function_meta(field_i64)?;
	m.function_meta(field_string)?;
	m.function_meta(schema_names)?;
	m.function_meta(schema_dtype)?;
	m.function_meta(schema_all_f64)?;
	m.function_meta(i64_series)?;
	m.function_meta(i64_sorted)?;
	m.function_meta(i64_values)?;
	m.function_meta(i64_len)?;
	m.function_meta(i64_nulls)?;
	m.function_meta(csv_new)?;
	m.function_meta(csv_finish)?;
	m.function_meta(error_message)?;
	m.function_meta(error_compute)?;
	m.function_meta(set_budget)?;
	m.function_meta(observe_rune)?;
	m.function_meta(set_restore_probe_rune)?;
	m.function_meta(restore_failures_rune)?;
	m.function_meta(guard_depth)?;
	m.function_meta(panic_native)?;
	m.function_meta(allow_reentry_rune)?;
	m.function_meta(calls_rune)?;
	m.function_meta(max_active_rune)?;
	m.function_meta(threads_rune)?;
	m.function_meta(reset_rune)?;
	m.function_meta(sleep_ms)?;
	m.function_meta(barrier_init)?;
	m.function_meta(barrier_wait)?;
	m.function_meta(rune_threads)?;
	m.function_meta(thread_name)?;
	Ok(m)
}

pub fn fixtures_module() -> Result<Module, rune::ContextError> {
	let mut m = Module::with_crate("fx")?;
	m.function_meta(fx_df)?;
	m.function_meta(fx_wide)?;
	m.function_meta(fx_series)?;
	m.function_meta(fx_column)?;
	m.function_meta(fx_sorted)?;
	m.function_meta(fx_nullable)?;
	m.function_meta(fx_wrapped)?;
	m.function_meta(fx_one)?;
	Ok(m)
}

/// A compiled script with the probe's modules: `call(name, args)` runs one
/// of its functions; values cross between scripts as `rune::Value`.
pub struct Script {
	pub context: Arc<rune::runtime::RuntimeContext>,
	pub unit: Arc<rune::Unit>,
}

impl Script {
	pub fn compile(source: &str) -> Result<Script, String> {
		let mut context = rune::Context::with_default_modules().map_err(|e| e.to_string())?;
		context.install(module().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
		context.install(fixtures_module().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
		let mut sources = rune::Sources::new();
		sources.insert(rune::Source::memory(source).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
		let mut diagnostics = rune::Diagnostics::new();
		let unit = rune::prepare(&mut sources).with_context(&context).with_diagnostics(&mut diagnostics).build();
		let unit = match unit {
			Ok(u) => u,
			Err(e) => return Err(format!("compile: {e}; diagnostics: {:?}", diagnostics.diagnostics().iter().map(|d| format!("{d:?}")).collect::<Vec<_>>())),
		};
		Ok(Script { context: Arc::new(context.runtime().map_err(|e| e.to_string())?), unit: Arc::new(unit) })
	}
	pub fn call(&self, name: &str, args: impl rune::runtime::GuardedArgs) -> Result<rune::Value, String> {
		let mut vm = rune::Vm::new(self.context.clone(), self.unit.clone());
		vm.call([name], args).map_err(|e| format!("vm: {e}"))
	}
}
