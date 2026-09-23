//! Callback boundary and borrowed-vector integration controls.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run(script: &str) -> String {
    let mut polars = Module::with_crate("polars").unwrap();
    rnx_polars::build(&mut polars).unwrap();
    let mut fixtures = Module::with_crate("fx").unwrap();
    rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
    let mut context = Context::with_default_modules().unwrap();
    context.install(polars).unwrap();
    context.install(fixtures).unwrap();
    let runtime = Arc::new(context.runtime().unwrap());
    let mut sources = Sources::new();
    sources.insert(Source::memory(script).unwrap()).unwrap();
    let mut diagnostics = rune::Diagnostics::new();
    let built = rune::prepare(&mut sources).with_context(&context).with_diagnostics(&mut diagnostics).build();
    if built.is_err() {
        eprintln!("diagnostics: {:?}", diagnostics.diagnostics());
    }
    let unit = built.unwrap();
    let mut vm = Vm::new(runtime, Arc::new(unit));
    rune::from_value(vm.call(["main"], ()).unwrap()).unwrap()
}

#[test]
fn callback_values_and_rollback() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let items = [1, 2, 3];
            let ca = polars::Int64Chunked::from_vec("x", items).unwrap();
            let borrowed = items[0] == 1;
            let increment = 2;
            let applied = ca.apply_mut(|x| x + increment);
            let changed = ca.get(0).unwrap() == Some(3);
            let failed = ca.apply_mut(|x| { if x == 5 { panic("cb marker") } x + 1 });
            let kept = ca.get(0).unwrap() == Some(3);
            match failed {
                Ok(_) => `bad ${borrowed} ${changed} ${kept}`,
                Err(e) => `${borrowed} ${changed} ${kept} ${e.kind()} ${e.message()}`,
            }
        }
    "#);
    assert!(result.starts_with("true true true CallbackError"), "{result}");
    assert!(result.contains("Int64Chunked::apply_mut"), "{result}");
}

#[test]
fn routed_reentry_fails_outer_callback() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let c = fx::column();
            match c.apply_unary_elementwise(|s| { fx::column().reverse(); s }) {
                Ok(_) => "allowed".to_string(),
                Err(e) => `${e.kind()} ${e.message()}`,
            }
        }
    "#);
    assert!(result.contains("CallbackError"), "{result}");
    assert!(result.contains("Column::reverse"), "{result}");
}

#[test]
fn stored_callback_fails_at_later_collect() {
    let _serial = SERIAL.lock().unwrap();
    let mut polars = Module::with_crate("polars").unwrap();
    rnx_polars::build(&mut polars).unwrap();
    let mut fixtures = Module::with_crate("fx").unwrap();
    rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
    let mut context = Context::with_default_modules().unwrap();
    context.install(polars).unwrap();
    context.install(fixtures).unwrap();
    let runtime = Arc::new(context.runtime().unwrap());
    let compile = |script: &str| {
        let mut sources = Sources::new();
        sources.insert(Source::memory(script).unwrap()).unwrap();
        Arc::new(rune::prepare(&mut sources).with_context(&context).build().unwrap())
    };
    let make = compile(r#"
        pub fn main() {
            let e = fx::expr().map(|c| panic("stored marker"), |s, f| f.with_dtype(polars::DataType::Float64())).unwrap();
            fx::df().lazy().select_([e]).unwrap()
        }
    "#);
    let collect = compile(r#"
        pub fn main(plan) {
            match plan.collect() {
                Ok(_) => "allowed".to_string(),
                Err(e) => e,
            }
        }
    "#);
    let mut vm1 = Vm::new(runtime.clone(), make);
    let plan = vm1.call(["main"], ()).unwrap();
    let mut vm2 = Vm::new(runtime, collect);
    let message: String = rune::from_value(vm2.call(["main"], (plan,)).unwrap()).unwrap();
    assert!(message.contains("callback Expr::map"), "{message}");
    assert!(message.contains("stored marker"), "{message}");
}

#[test]
fn callback_refusals_keep_the_receiver() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = polars::Int64Chunked::from_vec("x", [1, 2]).unwrap();
            let mixed = [1, "bad"];
            let vector_refusal = polars::Int64Chunked::from_vec("x", mixed);
            let vector_kept = mixed[1] == "bad";
            let wrong = ca.apply_mut(|x| "wrong");
            let kept = ca.get(0).unwrap() == Some(1);
            let wrapped = fx::series();
            let capture = ca.apply_mut(|x| { let _ = wrapped; x });
            match (wrong, capture) {
                (Err(a), Err(b)) => `${kept} ${vector_kept} ${vector_refusal.is_err()} ${a.kind()} ${a.message()} | ${b.kind()} ${b.message()}`,
                _ => "unexpected success".to_string(),
            }
        }
    "#);
    assert!(result.contains("true true true CallbackError"), "{result}");
    assert!(result.contains("wrong result type"), "{result}");
    assert!(result.contains("CallbackCapture"), "{result}");
}

#[test]
fn callback_budget_reports_exhaustion() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = polars::Int64Chunked::from_vec("x", [1]).unwrap();
            polars::set_callback_budget(1);
            let outcome = ca.apply_mut(|x| x + 1);
            polars::set_callback_budget(0);
            match outcome {
                Ok(_) => "unexpected success".to_string(),
                Err(e) => `${e.kind()} ${e.message()} ${ca.get(0).unwrap() == Some(1)}`,
            }
        }
    "#);
    assert!(result.contains("CallbackError"), "{result}");
    assert!(result.contains("instruction budget 1 exhausted"), "{result}");
    assert!(result.contains("true"), "{result}");
}

#[test]
fn user_panic_mentioning_limited_is_not_budget_exhaustion() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = polars::Int64Chunked::from_vec("x", [1]).unwrap();
            polars::set_callback_budget(1000);
            let outcome = ca.apply_mut(|_| panic("limited by user"));
            polars::set_callback_budget(0);
            match outcome {
                Ok(_) => "unexpected success".to_string(),
                Err(e) => `${e.kind()} ${e.message()}`,
            }
        }
    "#);
    assert!(result.contains("CallbackError"), "{result}");
    assert!(result.contains("limited by user"), "{result}");
    assert!(!result.contains("instruction budget"), "{result}");
}

#[test]
fn failed_apply_keeps_sorted_nullable_receiver() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let ca = fx::series_nulls().i64().unwrap();
            ca.set_sorted_flag(polars::IsSorted::Ascending());
            let outcome = ca.apply_mut(|x| { if x == 3 { panic("late marker") } x + 1 });
            `${outcome.is_err()} ${ca.get(0).unwrap() == Some(1)} ${ca.get(1).unwrap() == None} ${ca.get(2).unwrap() == Some(3)} ${ca.null_count()} ${ca.is_sorted_flag() == polars::IsSorted::Ascending()}`
        }
    "#);
    assert_eq!(result, "true true true true 1 true");
}

#[test]
fn routed_reentry_through_compute_schema_fails_outer_callback() {
    let _serial = SERIAL.lock().unwrap();
    let result = run(r#"
        pub fn main() {
            let c = fx::column();
            let outcome = c.apply_unary_elementwise(|s| {
                match fx::df().lazy().logical_plan().compute_schema() {
                    Ok(_) => panic("re-entry allowed"),
                    Err(e) => panic(`inner ${e.kind()} ${e.message()}`),
                }
            });
            match outcome {
                Ok(_) => "allowed",
                Err(e) => `${e.kind()} ${e.message()} ${c.len()}`,
            }
        }
    "#);
    assert!(result.starts_with("CallbackError"), "{result}");
    assert!(result.contains("inner CallbackError callback polars::DslPlan::compute_schema: routed binding called from a callback"), "{result}");
    assert!(!result.contains("re-entry allowed"), "{result}");
    assert!(result.ends_with(" 3"), "receiver unusable after the refusal: {result}");
}

#[test]
fn stored_callback_fails_at_later_compute_schema() {
    let _serial = SERIAL.lock().unwrap();
    let mut polars = Module::with_crate("polars").unwrap();
    rnx_polars::build(&mut polars).unwrap();
    let mut fixtures = Module::with_crate("fx").unwrap();
    rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
    let mut context = Context::with_default_modules().unwrap();
    context.install(polars).unwrap();
    context.install(fixtures).unwrap();
    let runtime = Arc::new(context.runtime().unwrap());
    let compile = |script: &str| {
        let mut sources = Sources::new();
        sources.insert(Source::memory(script).unwrap()).unwrap();
        Arc::new(rune::prepare(&mut sources).with_context(&context).build().unwrap())
    };
    let make = compile(r#"
        pub fn main() {
            let e = fx::expr().map(|c| c, |s, f| panic("schema marker")).unwrap();
            fx::df().lazy().select_([e]).unwrap()
        }
    "#);
    let schema = compile(r#"
        pub fn main(plan) {
            match plan.logical_plan().compute_schema() {
                Ok(_) => "allowed",
                Err(e) => `${e.kind()} ${e.message()}`,
            }
        }
    "#);
    let mut vm1 = Vm::new(runtime.clone(), make);
    let plan = vm1.call(["main"], ()).unwrap();
    let mut vm2 = Vm::new(runtime, schema);
    let message: String = rune::from_value(vm2.call(["main"], (plan,)).unwrap()).unwrap();
    assert!(message.contains("CallbackError"), "{message}");
    assert!(message.contains("callback Expr::map"), "{message}");
    assert!(message.contains("schema marker"), "{message}");
}
