//! Record 0079 gate 2: the contract probe's in-process measurements and
//! controls. Every control asserts; the measurements are written to
//! `out/contract.json` (path from `PROBE_OUT`, default `../out`). The
//! hang-prone controls (pool starvation, Ctrl-C) run as subprocesses from
//! `run.sh` under its watchdog, not here.
use callback_probe::{Script, counters, reset_counters, set_callback_budget};
use polars::prelude as p;
use polars::prelude::NamedFrom;
use std::time::Instant;

fn ok_helpers() -> &'static str {
	r#"
fn ok(r) { match r { Ok(v) => v, Err(e) => panic(`${e.kind()}: ${e.message()}`) } }
fn show(r) { match r { Ok(_) => "ok", Err(e) => `${e.kind()}: ${e.message()}` } }
fn join(v, sep) { let s = ""; let first = true; for p in v { if !first { s += sep; } s += p; first = false; } s }
fn fmt(v) { let parts = []; for x in v { parts.push(match x { Some(y) => `${y}`, None => "null", _ => `${x}` }); } `[${join(parts, ", ")}]` }
"#
}

fn script(body: &str) -> Script {
	Script::compile(&format!("{}\n{}", ok_helpers(), body)).unwrap_or_else(|e| panic!("{e}"))
}

fn run_str(s: &Script, name: &str) -> String {
	let v = s.call(name, ()).unwrap_or_else(|e| panic!("{name}: {e}"));
	rune::from_value::<String>(v).unwrap_or_else(|e| panic!("{name}: not a string: {e}"))
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, f64) {
	let t = Instant::now();
	let r = f();
	(r, t.elapsed().as_secs_f64() * 1000.0)
}

/// Median of `runs` timings of `f`, in milliseconds.
fn median_ms(runs: usize, mut f: impl FnMut()) -> f64 {
	let mut v: Vec<f64> = (0..runs).map(|_| timed(|| f()).1).collect();
	v.sort_by(|a, b| a.partial_cmp(b).unwrap());
	v[v.len() / 2]
}

#[test]
fn contract() {
	let mut report = serde_json::Map::new();
	let mut failures: Vec<String> = Vec::new();
	let mut check = |name: &str, cond: bool, detail: String| {
		if !cond {
			failures.push(format!("{name}: {detail}"));
		}
	};

	// ---- costs: instrumentation off, variants interleaved per iteration, raw samples kept ----
	{
		callback_probe::set_observe(false);
		let s = script(r#"
pub fn scalar_n(n) { let ca = fx::sorted_i64(n); ok(ca.apply_mut_cb(|x| x + 1)); ca.len() }
pub fn scalar_in_place_n(n) { let ca = fx::sorted_i64(n); ok(ca.apply_mut_in_place(|x| x + 1)); ca.len() }
pub fn columns_par_n(n) { let df = fx::wide(n, 1); let cols = ok(df.apply_columns_par_cb(|c| c)); cols.len() }
pub fn series_1m() { let c = fx::wide(1, 1000000); let col = ok(c.column("c0")); ok(col.apply_unary_elementwise_cb(|s| s.plus(1))).len() }
pub fn ident_scalar() { |x| x }
pub fn ident_series() { |s| s }
"#);
		let mut costs = serde_json::Map::new();
		let stats = |v: &[f64]| { let mut w = v.to_vec(); w.sort_by(|a, b| a.partial_cmp(b).unwrap()); serde_json::json!({"median": w[w.len() / 2], "min": w[0], "max": w[w.len() - 1], "n": w.len(), "samples": v}) };
		// end-to-end scalar callbacks: Rune commit-on-success, Rune in place, Rust closure, Rust clone-then-apply; interleaved
		for n in [1i64, 1000, 1_000_000] {
			let iters = if n >= 1_000_000 { 5 } else { 9 };
			let (mut a, mut b, mut c, mut d) = (vec![], vec![], vec![], vec![]);
			for _ in 0..iters {
				a.push(timed(|| { s.call("scalar_n", (n,)).unwrap(); }).1);
				b.push(timed(|| { s.call("scalar_in_place_n", (n,)).unwrap(); }).1);
				c.push(timed(|| { let mut ca = callback_probe::fixtures::sorted_i64(n as usize); ca.apply_mut(|x| x + 1); std::hint::black_box(&ca); }).1);
				d.push(timed(|| { let ca = callback_probe::fixtures::sorted_i64(n as usize); let mut w = ca.clone(); w.apply_mut(|x| x + 1); std::hint::black_box(&w); }).1);
			}
			let diffs: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
			costs.insert(format!("apply_mut_{n}"), serde_json::json!({
				"calls": n, "note": "end to end: the binding, the engine thread, one VM per element; not a separation of VM and conversion costs",
				"rune_commit_on_success_ms": stats(&a), "rune_in_place_ms": stats(&b), "rust_closure_ms": stats(&c), "rust_clone_then_apply_ms": stats(&d),
				"commit_minus_in_place_ms_paired": stats(&diffs),
			}));
		}
		// isolated bridge latency on one thread: the bridge called directly, no Polars, no engine thread
		let ident_scalar = callback_probe::install("probe", rune::from_value::<rune::runtime::Function>(s.call("ident_scalar", ()).unwrap()).unwrap()).unwrap();
		let ident_series = callback_probe::install("probe", rune::from_value::<rune::runtime::Function>(s.call("ident_series", ()).unwrap()).unwrap()).unwrap();
		let series = p::Series::new("x".into(), [1i64, 2, 3]);
		let calls = 100_000i64;
		let (mut sc, mut se) = (vec![], vec![]);
		for _ in 0..7 {
			sc.push(timed(|| for i in 0..calls { let r: i64 = callback_probe::bridge("probe", &ident_scalar, (i,)).unwrap(); std::hint::black_box(r); }).1);
			se.push(timed(|| for _ in 0..calls { let r: callback_probe::Series = callback_probe::bridge("probe", &ident_series, (callback_probe::Series(series.clone()),)).unwrap(); std::hint::black_box(&r); }).1);
		}
		let med = |v: &[f64]| { let mut w = v.to_vec(); w.sort_by(|a, b| a.partial_cmp(b).unwrap()); w[w.len() / 2] };
		costs.insert("bridge_alone_single_thread".into(), serde_json::json!({
			"calls": calls,
			"scalar_i64_in_out_ms": stats(&sc), "series_in_out_ms": stats(&se),
			"per_call_us_scalar": med(&sc) * 1000.0 / calls as f64, "per_call_us_series": med(&se) * 1000.0 / calls as f64,
			"per_call_us_series_conversion_over_scalar": (med(&se) - med(&sc)) * 1000.0 / calls as f64,
			"note": "one VM per call (scalar); a Series wrapped into a Rune value and taken back (series); the identity closure",
		}));
		// parallel throughput on the pool, wall time per item: not a latency
		let n = 1000i64;
		let (mut pr, mut pu) = (vec![], vec![]);
		for _ in 0..7 {
			pr.push(timed(|| { s.call("columns_par_n", (n,)).unwrap(); }).1);
			pu.push(timed(|| { let df = callback_probe::fixtures::wide(n as usize, 1); let out = df.apply_columns_par(|c| c.clone()); std::hint::black_box(&out); }).1);
		}
		costs.insert("apply_columns_par_1000_one_row_columns".into(), serde_json::json!({"columns": n, "rune_ms": stats(&pr), "rust_ms": stats(&pu), "note": "parallel wall time over the pool: throughput, not per-invocation latency"}));
		let (mut es, mut eu) = (vec![], vec![]);
		for _ in 0..5 {
			es.push(timed(|| { s.call("series_1m", ()).unwrap(); }).1);
			eu.push(timed(|| { let c: p::Column = p::Series::new("c0".into(), (0..1_000_000i64).collect::<Vec<_>>()).into(); let out = c.apply_unary_elementwise(|s| s + 1); std::hint::black_box(&out); }).1);
		}
		costs.insert("elementwise_series_1m_rows".into(), serde_json::json!({"rows": 1_000_000, "rune_ms": stats(&es), "rust_ms": stats(&eu), "note": "one bridge call on the whole series plus the fixture, the engine thread and Polars's own work; end to end"}));
		report.insert("costs".into(), serde_json::Value::Object(costs));
	}

	// ---- concurrency under apply_columns_par ----
	{
		let s = script(r#"
pub fn par() { polars::observe(true); polars::reset(); let cols = ok(fx::wide(64, 1000).apply_columns_par_cb(|c| { polars::sleep_ms(5); c.times(3) })); `${cols.len()} ${polars::calls()} ${polars::threads_seen()} ${polars::max_active()} ${polars::rune_threads()}` }
"#);
		let out = run_str(&s, "par");
		callback_probe::set_observe(false);
		let parts: Vec<i64> = out.split(' ').map(|x| x.parse().unwrap()).collect();
		report.insert("concurrency".into(), serde_json::json!({"columns": parts[0], "calls": parts[1], "distinct_threads": parts[2], "max_simultaneous": parts[3], "pool_threads": parts[4]}));
		check("concurrency", parts[1] == 64 && parts[3] > 1 && parts[2] > 1, out.clone());
	}

	// ---- the deferred journey: install in one script, execute in another ----
	{
		let a = script(r#"
pub fn install() {
    let f = |c| c.times(2);
    let g = |s, fld| polars::Field::i64(fld.name());
    let e = ok(polars::col("x").map_cb(f, g)).alias("x2");
    let f = 0; let g = 0; // rebound: the plan holds its own copies
    e
}
pub fn install_failing(which) {
    match which {
        "vm" => ok(polars::col("x").map_cb(|c| panic("later failure"), |s, fld| polars::Field::i64(fld.name()))).alias("x2"),
        "type" => ok(polars::col("x").map_cb(|c| c, |s, fld| 42)).alias("x2"),
        "loop" => ok(polars::col("x").map_cb(|c| { let i = 0; while true { i += 1; } c }, |s, fld| polars::Field::i64(fld.name()))).alias("x2"),
        _ => panic("which"),
    }
}
"#);
		let e = a.call("install", ()).unwrap();
		let e_vm = a.call("install_failing", ("vm",)).unwrap();
		let e_type = a.call("install_failing", ("type",)).unwrap();
		let e_loop = a.call("install_failing", ("loop",)).unwrap();
		drop(a); // the installing script's unit and context are gone from the caller's side
		let b = script(r#"
pub fn later(e) { polars::observe(true); polars::reset(); let df = ok(ok(fx::df().lazy().select_([e])).collect()); `${fmt(ok(ok(df.column("x2")).i64s()))} calls=${polars::calls()}` }
pub fn later_err(e) { polars::observe(true); polars::reset(); `${show(ok(fx::df().lazy().select_([e])).collect())} calls=${polars::calls()}` }
pub fn schema(e) { polars::observe(true); polars::reset(); let s = ok(ok(fx::df().lazy().select_([e])).compute_schema()); `${fmt(s.names())} calls=${polars::calls()}` }
pub fn describe(e) { polars::observe(true); polars::reset(); let _ = ok(ok(fx::df().lazy().select_([e])).describe_plan()); `calls=${polars::calls()}` }
pub fn schema_err(e) { polars::observe(true); polars::reset(); `${show(ok(fx::df().lazy().select_([e])).compute_schema())} calls=${polars::calls()}` }
pub fn internal_panic() { fx::series().panic_inside_engine() }
"#);
		let v = rune::from_value::<String>(b.call("later", (e.clone(),)).unwrap()).unwrap();
		check("deferred value", v.starts_with("[2, 4, 6]"), v.clone());
		let v2 = rune::from_value::<String>(b.call("later", (e.clone(),)).unwrap()).unwrap();
		check("deferred retry", v2 == v, format!("{v} vs {v2}"));
		let sch = rune::from_value::<String>(b.call("schema", (e.clone(),)).unwrap()).unwrap();
		check("deferred schema sink", sch.starts_with("[x2] calls=") && !sch.ends_with("calls=0"), sch.clone());
		let desc = rune::from_value::<String>(b.call("describe", (e.clone(),)).unwrap()).unwrap();
		let vm_err = rune::from_value::<String>(b.call("later_err", (e_vm,)).unwrap()).unwrap();
		// Polars wraps an expression's error in `ExprContext` (the kind a script sees), the ComputeError inside
		check("deferred vm failure", (vm_err.starts_with("ExprContext") || vm_err.starts_with("ComputeError")) && vm_err.contains("callback Expr::map: call failed") && vm_err.contains("later failure"), vm_err.clone());
		let type_err = rune::from_value::<String>(b.call("schema_err", (e_type.clone(),)).unwrap()).unwrap();
		check("deferred wrong type at schema resolution", type_err.contains("callback Expr::map: wrong result type"), type_err.clone());
		let type_err_collect = rune::from_value::<String>(b.call("later_err", (e_type,)).unwrap()).unwrap();
		check("deferred wrong type at collect", type_err_collect.contains("callback Expr::map: wrong result type"), type_err_collect.clone());
		set_callback_budget(500);
		let budget_err = rune::from_value::<String>(b.call("later_err", (e_loop.clone(),)).unwrap()).unwrap();
		set_callback_budget(0);
		check("deferred budget exhaustion", budget_err.contains("callback Expr::map: instruction budget 500 exhausted"), budget_err.clone());
		// a Polars-internal panic inside the engine is still a panic, distinguished from a callback failure
		let internal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.call("internal_panic", ())));
		let internal_ok = match &internal { Err(payload) => payload.downcast_ref::<&str>().is_some_and(|s| s.contains("polars-internal-panic-marker")), Ok(_) => false };
		check("internal panic distinguished", internal_ok, format!("{internal:?}"));
		report.insert("deferred_journey".into(), serde_json::json!({
			"value": v, "retry": v2, "schema_sink": sch, "describe_sink": desc,
			"vm_failure": vm_err, "wrong_type_at_schema": type_err, "wrong_type_at_collect": type_err_collect, "budget_exhaustion": budget_err,
			"entry_points_invoking_callbacks": ["LazyFrame::collect (function and output_type)", "DslPlan::compute_schema (output_type)", "LazyFrame::describe_plan (output_type)"],
		}));
	}

	// ---- re-entry: routed denied at the engine, nested denied at the bridge, guard restored ----
	{
		let s = script(r#"
pub fn routed_from_callback() { show(fx::column().apply_unary_elementwise_cb(|s| { match s.sum_routed() { Ok(_) => panic("routed call was allowed"), Err(e) => panic(`${e.kind()}|${e.message()}`) } })) }
pub fn unrouted_from_callback() { fmt(ok(ok(fx::column().apply_unary_elementwise_cb(|s| s.plus(1).times(2))).i64s())) }
pub fn nested_via_unrouted_path() { show(fx::column().apply_unary_elementwise_cb(|s| { match polars::PolarsError::compute("x").wrap_msg_unrouted(|m| m) { Ok(_) => panic("nested callback was allowed"), Err(e) => panic(`${e.kind()}|${e.message()}`) } })) }
pub fn nested_via_routed_path() { show(fx::column().apply_unary_elementwise_cb(|s| { match polars::PolarsError::compute("x").wrap_msg_cb(|m| m) { Ok(_) => panic("routed wrap_msg was allowed"), Err(e) => panic(`${e.kind()}|${e.message()}`) } })) }
pub fn after_success() { ok(fx::column().apply_unary_elementwise_cb(|s| s)); "done" }
pub fn after_error() { show(fx::column().apply_unary_elementwise_cb(|s| panic("e"))) }
"#);
		let routed = run_str(&s, "routed_from_callback");
		check("routed call from a callback refused at the engine", routed.contains("`Series::sum` is a routed binding and may not be called from a callback"), routed.clone());
		check("guard restored after the refusal", !callback_probe::in_callback(), String::new());
		let unrouted = run_str(&s, "unrouted_from_callback");
		check("unrouted call from a callback allowed", unrouted == "[4, 6, 8]", unrouted.clone());
		let nested = run_str(&s, "nested_via_unrouted_path");
		check("nested callback refused by the bridge before any budget", nested.contains("nested callback: a callback invoked while another is running on this thread"), nested.clone());
		let nested_routed = run_str(&s, "nested_via_routed_path");
		check("callback-bearing routed call from a callback refused at the engine", nested_routed.contains("`PolarsError::wrap_msg` is a routed binding"), nested_routed.clone());
		let _ = run_str(&s, "after_success");
		check("guard restored after success", !callback_probe::in_callback(), String::new());
		let _ = run_str(&s, "after_error");
		check("guard restored after error", !callback_probe::in_callback(), String::new());
		report.insert("reentry".into(), serde_json::json!({"routed_from_callback": routed, "unrouted_from_callback": unrouted, "nested_via_unrouted_path": nested, "nested_via_routed_path": nested_routed}));
	}

	// ---- mutation: commit-on-success, the vector and buffer contracts, a mutated argument ----
	{
		let s = script(r#"
pub fn fail_after_some() {
    let ca = fx::nullable_i64();
    let before = `${fmt(ca.values())} nulls=${ca.null_count()} len=${ca.len()} ${ca.is_sorted_flag()}`;
    let r = ca.apply_mut_cb(|x| if x == 4 { panic("at four") } else { x * 10 });
    let after = `${fmt(ca.values())} nulls=${ca.null_count()} len=${ca.len()} ${ca.is_sorted_flag()}`;
    ok(ca.apply_mut_cb(|x| x + 1));
    `${before}|${show(r)}|${after}|${fmt(ca.values())}`
}
pub fn in_place_partial() {
    let ca = fx::nullable_i64();
    let r = ca.apply_mut_in_place(|x| if x == 4 { panic("at four") } else { x * 10 });
    `${show(r)}|${fmt(ca.values())} ${ca.is_sorted_flag()}`
}
pub fn many() { let e = ok(polars::col("x").map_many_cb(|cols| cols[0].times(10), [polars::col("x")], |s, flds| polars::Field::i64(flds[0].name()))); fmt(ok(ok(ok(ok(fx::df().lazy().select_([e.alias("m")])).collect()).column("m")).i64s())) }
pub fn buffer() { fmt(ok(ok(fx::sorted_i64(3).apply_into_string_amortized_cb(|x| `v${x}`)).strs())) }
pub fn mutated_argument() { let c = fx::column(); let out = ok(c.apply_unary_elementwise_cb(|s| { s.zero_first(); s })); `${fmt(ok(c.i64s()))}|${fmt(ok(out.i64s()))}` }
"#);
		let fas = run_str(&s, "fail_after_some");
		let parts: Vec<&str> = fas.split('|').collect();
		check("apply_mut failure leaves the receiver unchanged", parts[0] == parts[2] && parts[1].contains("at four") && parts[3] == "[2, null, 4, 5, 6]", fas.clone());
		let partial = run_str(&s, "in_place_partial");
		check("the bare in-place form shows the partial write the commit avoids", partial.contains("[10, null, 30, 4, 5]") || partial.contains("[10, null, 30, 40, 5]"), partial.clone());
		let many = run_str(&s, "many");
		check("vector argument", many == "[10, 20, 30]", many.clone());
		let buffer = run_str(&s, "buffer");
		check("result buffer", buffer == "[v0, v1, v2]", buffer.clone());
		let mutated = run_str(&s, "mutated_argument");
		check("a callback that mutates its argument mutates a clone; the source is unchanged", mutated == "[1, 2, 3]|[0, 0, 0]", mutated.clone());
		report.insert("mutation".into(), serde_json::json!({"commit_on_success": fas, "in_place_partial_write": partial, "vector_argument": many, "result_buffer": buffer, "mutated_argument": mutated}));
	}

	// ---- error propagation, fallible and infallible signatures ----
	{
		let s = script(r#"
pub fn fallible_error() { show(ok(fx::df().lazy().select_([ok(polars::col("x").map_cb(|c| panic("vm error"), |s, f| polars::Field::i64(f.name())))])).collect()) }
pub fn fallible_type() { show(ok(fx::df().lazy().select_([ok(polars::col("x").map_cb(|c| "nope", |s, f| polars::Field::i64(f.name())))])).collect()) }
pub fn infallible_error() { let c = fx::column(); let r = c.apply_unary_elementwise_cb(|s| panic("vm error")); `${show(r)}|${fmt(ok(c.i64s()))}|${fmt(ok(ok(c.apply_unary_elementwise_cb(|s| s.plus(1))).i64s()))}` }
pub fn infallible_type() { show(fx::column().apply_unary_elementwise_cb(|s| 7)) }
pub fn wrap_ok() { ok(polars::PolarsError::compute("base").wrap_msg_cb(|m| `[${m}]`)).message() }
pub fn wrap_fail() { show(polars::PolarsError::compute("base").wrap_msg_cb(|m| panic("inside"))) }
"#);
		let fe = run_str(&s, "fallible_error");
		check("fallible: vm error as ComputeError naming the operation", (fe.starts_with("ExprContext") || fe.starts_with("ComputeError")) && fe.contains("callback Expr::map: call failed") && fe.contains("vm error"), fe.clone());
		let ft = run_str(&s, "fallible_type");
		check("fallible: wrong type", ft.contains("callback Expr::map: wrong result type: got ::std::string::String"), ft.clone());
		let ie = run_str(&s, "infallible_error");
		check("infallible: vm error as CallbackError, receiver usable", ie.starts_with("CallbackError: callback Column::apply_unary_elementwise: call failed") && ie.ends_with("|[1, 2, 3]|[2, 3, 4]"), ie.clone());
		let it = run_str(&s, "infallible_type");
		check("infallible: wrong type", it.starts_with("CallbackError: callback Column::apply_unary_elementwise: wrong result type"), it.clone());
		let wo = run_str(&s, "wrap_ok");
		let wf = run_str(&s, "wrap_fail");
		check("wrap_msg routed: value and failure", wo == "[base]" && wf.starts_with("CallbackError: callback PolarsError::wrap_msg: call failed"), format!("{wo} / {wf}"));
		// the control the routing rule exists for: the unrouted build leaks an unwinding panic into the host
		let leak = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let s = script(r#"pub fn go() { show(fx::column().apply_unary_elementwise_unrouted(|s| panic("vm error"))) }"#);
			s.call("go", ())
		}));
		let leaked = match &leak { Err(payload) => payload.downcast_ref::<callback_probe::CallbackFailure>().is_some(), Ok(_) => false };
		check("unrouted build leaks a CallbackFailure panic", leaked, format!("{leak:?}"));
		let leak2 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let s = script(r#"pub fn go() { show(polars::PolarsError::compute("x").wrap_msg_unrouted(|m| panic("vm error"))) }"#);
			s.call("go", ())
		}));
		let leaked2 = match &leak2 { Err(payload) => payload.downcast_ref::<callback_probe::CallbackFailure>().is_some(), Ok(_) => false };
		check("unrouted wrap_msg leaks a CallbackFailure panic", leaked2, format!("{leak2:?}"));
		report.insert("errors".into(), serde_json::json!({"fallible_vm_error": fe, "fallible_wrong_type": ft, "infallible_vm_error": ie, "infallible_wrong_type": it, "wrap_msg_ok": wo, "wrap_msg_fail": wf, "unrouted_leaks_panic": leaked, "unrouted_wrap_msg_leaks_panic": leaked2}));
	}

	// ---- captures ----
	{
		let s = script(r#"
pub fn wrapped() { let w = fx::wrapped(); show(polars::col("x").map_cb(|c| w, |s, f| polars::Field::i64(f.name()))) }
pub fn function() { let g = |x| x; show(fx::column().apply_unary_elementwise_cb(|s| g(s))) }
pub fn nested_object() { let o = #{ s: fx::wrapped() }; show(fx::column().apply_unary_elementwise_cb(|s| o.s)) }
pub fn constant_then_rebound() { let n = fx::one(); let f = |s| s.plus(n); n = 100; fmt(ok(ok(fx::column().apply_unary_elementwise_cb(f)).i64s())) }
"#);
		let w = run_str(&s, "wrapped");
		let f = run_str(&s, "function");
		let o = run_str(&s, "nested_object");
		let c = run_str(&s, "constant_then_rebound");
		check("wrapped capture refused", w.starts_with("CallbackCapture: callback Expr::map: a captured value is not a constant"), w.clone());
		check("function capture refused", f.starts_with("CallbackCapture: callback Column::apply_unary_elementwise"), f.clone());
		check("nested object capture refused", o.starts_with("CallbackCapture"), o.clone());
		check("constant capture carried, later rebinding ignored", c == "[2, 3, 4]", c.clone());
		report.insert("captures".into(), serde_json::json!({"wrapped": w, "function": f, "nested_object": o, "constant_then_rebound": c}));
	}

	// ---- LazyCsvReader::with_schema_modify: the schema the callback returns is the reader's ----
	{
		let s = script(r#"
pub fn csv() {
    polars::observe(true); polars::reset();
    let lf = ok(ok(polars::LazyCsvReader::new("fixtures/small.csv").with_schema_modify_cb(|s| s.with_all_f64())).finish());
    let calls = polars::calls();
    let sch = ok(lf.compute_schema());
    let df = ok(lf.collect());
    let a = ok(df.column("a")); let b = ok(df.column("b"));
    `${fmt(sch.names())} ${match sch.dtype_name("a") { Some(d) => d, None => "none" }} ${match sch.dtype_name("b") { Some(d) => d, None => "none" }} calls=${calls} height=${df.height()} a=${a.len()}`
}
pub fn csv_plain() { let lf = ok(polars::LazyCsvReader::new("fixtures/small.csv").finish()); let sch = ok(lf.compute_schema()); `${match sch.dtype_name("a") { Some(d) => d, None => "none" }}` }
"#);
		let csv = run_str(&s, "csv");
		callback_probe::set_observe(false);
		let plain = run_str(&s, "csv_plain");
		check("with_schema_modify: the callback's schema is applied, invoked once, before finish", csv == "[a, b] f64 f64 calls=1 height=2 a=2" && plain == "i64", format!("{csv} / plain {plain}"));
		report.insert("csv".into(), serde_json::json!({"with_schema_modify": csv, "without": plain}));
	}

	// ---- R1 counterexample: a user panic that mentions the halt word keeps its own text ----
	{
		let s = script(r#"
pub fn immediate() { show(fx::column().apply_unary_elementwise_cb(|s| panic("limited user message"))) }
pub fn install() { ok(polars::col("x").map_cb(|c| panic("limited user message"), |s, f| polars::Field::i64(f.name()))).alias("x2") }
pub fn run(e) { show(ok(fx::df().lazy().select_([e])).collect()) }
pub fn genuine() { show(fx::column().apply_unary_elementwise_cb(|s| { let i = 0; while true { i += 1; } s })) }
"#);
		set_callback_budget(1000);
		let imm = run_str(&s, "immediate");
		let e = s.call("install", ()).unwrap();
		let deferred = rune::from_value::<String>(s.call("run", (e,)).unwrap()).unwrap();
		let genuine = run_str(&s, "genuine");
		set_callback_budget(0);
		check("a user panic mentioning `limited` is not exhaustion (immediate)", imm.contains("call failed: Panicked: limited user message") && !imm.contains("exhausted"), imm.clone());
		check("a user panic mentioning `limited` is not exhaustion (deferred, fallible)", deferred.contains("call failed: Panicked: limited user message") && !deferred.contains("exhausted"), deferred.clone());
		check("genuine exhaustion still reported", genuine.contains("instruction budget 1000 exhausted"), genuine.clone());
		report.insert("budget_discrimination".into(), serde_json::json!({"user_panic_immediate": imm, "user_panic_deferred": deferred, "genuine": genuine}));
	}

	// ---- restoration, on the thread that entered the bridge ----
	{
		let s = script(r#"
// a closure whose body ends with a loop and a tail expression returns unit in Rune 0.14.2; `return` is explicit
pub fn probe() { || { let i = 0; while i < 3000 { i += 1; } return i; } }
pub fn spin() { |x| { let i = 0; while true { i += 1; } return x; } }
pub fn ok_fn() { |x| x + 1 }
pub fn fail_fn() { |x| panic("vm failure") }
pub fn native_fn() { |x| { polars::panic_native(); x } }
pub fn not_native_fn() { |x| x }
pub fn loop_n(n) { let i = 0; while i < n { i += 1; } i }
pub fn worker_reuse() {
    polars::observe(true); polars::reset();
    let failing = fx::wide(64, 100).apply_columns_par_cb(|c| panic("fail on a worker"));
    let failing_calls = polars::calls();
    polars::reset();
    let again = ok(fx::wide(64, 100).apply_columns_par_cb(|c| { if polars::guard_depth() != 1 { panic("guard depth leaked") } let i = 0; while i < 3000 { i += 1; } c.times(2) }));
    let sum = 0; for c in again { sum += ok(c.i64s())[1].unwrap_or(0); }
    `${show(failing)}|failing_calls=${failing_calls}|again=${again.len()} calls=${polars::calls()} sum=${sum}|${polars::restore_failures()}`
}
"#);
		let f = |name: &str| callback_probe::install("probe", rune::from_value::<rune::runtime::Function>(s.call(name, ()).unwrap()).unwrap()).unwrap();
		let (spin, okf, failf, nativef, notnativef) = (f("spin"), f("ok_fn"), f("fail_fn"), f("native_fn"), f("not_native_fn"));
		// the outer-allowance paths run without the probe loop, which would itself spend the finite outer allowance
		callback_probe::set_restore_probe(None).unwrap();
		#[derive(Debug, PartialEq)]
		enum Outcome { Value(i64), Exhausted, VmFailure, NativePanic, TypedUnwind }
		// one path: a fresh outer allowance of 50 000 on this thread, a nonzero inner budget, the
		// call, the outcome asserted (payload included), then the guard and the outer allowance
		// checked on this same thread: a 1 000-iteration loop runs, a 100 000-iteration loop halts
		let path = |name: &str, inner_budget: usize, f: &Arc<SyncFunction>, unwind: bool, want: Outcome| -> (bool, String) {
			rune::runtime::budget::with(50_000, || {
				set_callback_budget(inner_budget);
				let got = if unwind {
					match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { let r: i64 = callback_probe::bridge("probe", f, (1i64,)).unwrap_or_else(callback_probe::unwind); r })) {
						Ok(v) => Outcome::Value(v),
						Err(p) => if p.downcast_ref::<callback_probe::CallbackFailure>().is_some() { Outcome::TypedUnwind } else if p.downcast_ref::<&str>().is_some_and(|m| m.contains("native-panic-marker")) { Outcome::NativePanic } else { Outcome::Value(-1) },
					}
				} else {
					match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback_probe::bridge::<_, i64>("probe", f, (1i64,)))) {
						Ok(Ok(v)) => Outcome::Value(v),
						Ok(Err(e)) => if e.cause.contains("instruction budget") && e.cause.ends_with("exhausted") { Outcome::Exhausted } else if e.cause == "call failed: Panicked: vm failure" { Outcome::VmFailure } else { Outcome::Value(-2) },
						Err(p) => if p.downcast_ref::<&str>().is_some_and(|m| m.contains("native-panic-marker")) { Outcome::NativePanic } else { Outcome::Value(-3) },
					}
				};
				set_callback_budget(0);
				let guard_clear = !callback_probe::in_callback();
				let small = s.call("loop_n", (1000i64,)).is_ok();
				let large = s.call("loop_n", (100_000i64,)).is_err();
				let ok = got == want && guard_clear && small && large;
				(ok, format!("{name}: got {got:?} (wanted {want:?}), guard clear {guard_clear}, outer allowance intact: small runs {small}, large halts {large}"))
			})
			.call()
		};
		use std::sync::Arc;
		use rune::runtime::SyncFunction;
		let paths = vec![
			path("exhaustion under an inner budget of 100", 100, &spin, false, Outcome::Exhausted),
			path("success under an inner budget of 100 000", 100_000, &okf, false, Outcome::Value(2)),
			path("vm failure under an inner budget of 100 000", 100_000, &failf, false, Outcome::VmFailure),
			path("native panic inside the callback under an inner budget of 100 000", 100_000, &nativef, false, Outcome::NativePanic),
			path("typed unwind under an inner budget of 100 000", 100_000, &failf, true, Outcome::TypedUnwind),
		];
		for (ok, note) in &paths {
			check("restoration on the calling thread", *ok, note.clone());
		}
		// the negative control: the native-panic path with a callback that does not panic must fail
		let (neg_ok, neg_note) = path("negative control: no native panic", 100_000, &notnativef, false, Outcome::NativePanic);
		check("omitting the native panic fails the native-panic path", !neg_ok, neg_note.clone());
		// rayon workers reused after callback failures: the invocation counts observed, depth 1 inside,
		// the probe loop after every invocation, the values right
		callback_probe::set_restore_probe(Some(rune::from_value::<rune::runtime::Function>(s.call("probe", ()).unwrap()).unwrap())).unwrap();
		let before = callback_probe::restore_failures();
		let reuse = run_str(&s, "worker_reuse");
		callback_probe::set_observe(false);
		let parts: Vec<&str> = reuse.split('|').collect();
		let failing_calls: i64 = parts[1].trim_start_matches("failing_calls=").parse().unwrap_or(-1);
		check("rayon workers clean after failures", parts[0].starts_with("CallbackError: callback DataFrame::apply_columns_par: call failed: Panicked: fail on a worker") && failing_calls >= 1 && parts[2] == "again=64 calls=64 sum=128" && parts[3] == before.to_string(), format!("{reuse}; last probe failure: {}", callback_probe::restore_last()));
		callback_probe::set_restore_probe(None).unwrap();
		report.insert("restoration".into(), serde_json::json!({"calling_thread": paths.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>(), "negative_control": neg_note, "worker_reuse": reuse, "failing_invocations_observed": failing_calls, "probe_failures": callback_probe::restore_failures() - before}));
	}

	// ---- budget ----
	{
		let s = script(r#"
pub fn spin() { show(fx::column().apply_unary_elementwise_cb(|s| { let i = 0; while true { i += 1; } s })) }
pub fn blocking() { fmt(ok(ok(fx::column().apply_unary_elementwise_cb(|s| { polars::sleep_ms(300); s })).i64s())) }
pub fn install() { ok(polars::col("x").map_cb(|c| { let i = 0; while i < 2000 { i += 1; } c }, |s, f| polars::Field::i64(f.name()))).alias("x2") }
pub fn run(e) { show(ok(fx::df().lazy().select_([e])).collect()) }
pub fn plain_loop() { let i = 0; while i < 20000 { i += 1; } i }
"#);
		set_callback_budget(1000);
		let (spin, spin_ms) = timed(|| run_str(&s, "spin"));
		check("pure nontermination stopped", spin.contains("instruction budget 1000 exhausted"), spin.clone());
		set_callback_budget(10);
		let (blocking, blocking_ms) = timed(|| run_str(&s, "blocking"));
		check("a blocking native call is not stopped by the budget", blocking == "[1, 2, 3]" && blocking_ms >= 300.0, format!("{blocking} in {blocking_ms} ms"));
		set_callback_budget(0);
		let e = s.call("install", ()).unwrap();
		let unbounded = rune::from_value::<String>(s.call("run", (e.clone(),)).unwrap()).unwrap();
		set_callback_budget(100);
		let bounded = rune::from_value::<String>(s.call("run", (e.clone(),)).unwrap()).unwrap();
		set_callback_budget(0);
		let unbounded_again = rune::from_value::<String>(s.call("run", (e,)).unwrap()).unwrap();
		check("a stored callback reads the budget at each invocation", unbounded == "ok" && bounded.contains("instruction budget 100 exhausted") && unbounded_again == "ok", format!("{unbounded} / {bounded} / {unbounded_again}"));
		// restoration is checked on the thread that entered the bridge, above
		report.insert("budget".into(), serde_json::json!({"nontermination": spin, "nontermination_ms": spin_ms, "blocking_native_call": blocking, "blocking_ms": blocking_ms, "stored_callback_reads_setting": [unbounded, bounded, unbounded_again]}));
	}
	// budget cost: the same scalar loop with and without a never-exhausted budget, interleaved, paired differences kept
	{
		let s = script(r#"pub fn scalar_n(n) { let ca = fx::sorted_i64(n); ok(ca.apply_mut_in_place(|x| x + 1)); ca.len() }"#);
		let n = 100_000i64;
		let (mut without, mut with) = (vec![], vec![]);
		for _ in 0..9 {
			set_callback_budget(0);
			without.push(timed(|| { s.call("scalar_n", (n,)).unwrap(); }).1);
			set_callback_budget(1_000_000);
			with.push(timed(|| { s.call("scalar_n", (n,)).unwrap(); }).1);
		}
		set_callback_budget(0);
		let diffs: Vec<f64> = with.iter().zip(&without).map(|(a, b)| (a - b) * 1_000_000.0 / n as f64).collect();
		let mut sd = diffs.clone(); sd.sort_by(|a, b| a.partial_cmp(b).unwrap());
		report.insert("budget_cost".into(), serde_json::json!({"calls": n, "without_budget_ms": without, "with_budget_ms": with, "paired_diff_ns_per_call": diffs, "paired_diff_ns_median": sd[sd.len() / 2], "paired_diff_ns_min": sd[0], "paired_diff_ns_max": sd[sd.len() - 1]}));
	}

	// ---- the runner fails closed: an injected wrong result is caught ----
	{
		let s = script(r#"pub fn wrong() { fmt(ok(ok(fx::column().apply_unary_elementwise_cb(|s| s.plus(1))).i64s())) }"#);
		let v = run_str(&s, "wrong");
		let injected_ok = v == "[3, 4, 5]"; // deliberately wrong expectation
		check("injected wrong expectation is reported (control of the control)", !injected_ok, v.clone());
	}

	report.insert("failures".into(), serde_json::json!(failures));
	let out_dir = std::env::var("PROBE_OUT").unwrap_or_else(|_| format!("{}/../out", env!("CARGO_MANIFEST_DIR")));
	std::fs::create_dir_all(&out_dir).unwrap();
	std::fs::write(format!("{out_dir}/contract.json"), serde_json::to_string_pretty(&serde_json::Value::Object(report)).unwrap()).unwrap();
	let _ = (counters(), reset_counters());
	assert!(failures.is_empty(), "controls that did not hold:\n{}", failures.join("\n"));
}
