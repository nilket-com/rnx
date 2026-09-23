//! Record 0084: function generics chosen from script values. Chained
//! inference feeds `Vec<String>` and `Vec<Expr>` where Polars asks for
//! `IntoIterator<Item = S>` or `AsRef<[IE]>`; iterator inputs feed list
//! builders from script vectors, borrowing strings and bytes for the call.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

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
    let mut vm = Vm::new(runtime, Arc::new(built.unwrap()));
    rune::from_value(vm.call(["main"], ()).unwrap()).unwrap()
}

#[test]
fn frame_chains_take_string_vectors_and_keep_them() {
    let result = run(r#"
        pub fn main() {
            let df = fx::df();
            let names = ["x", "z"];
            let picked = df.select_(names).unwrap();
            let kept = names.len() == 2 && names[1] == "z";
            let none = df.select_([]).unwrap();
            let dropped = df.drop_many(["y"]).unwrap();
            let parts = df.partition_by(["y"], true).unwrap();
            let bad = df.select_(["nope"]);
            let still = names[0] == "x" && df.width() == 3;
            match bad {
                Err(e) => `${picked.width()} ${kept} ${none.width()} ${dropped.width()} ${parts.len()} ${e.kind()} ${still}`,
                Ok(_) => "unexpected".to_string(),
            }
        }
    "#);
    assert_eq!(result, "2 true 0 2 3 ColumnNotFound true");
}

#[test]
fn lazy_and_expression_chains_take_expression_vectors() {
    let result = run(r#"
        pub fn main() {
            let df = fx::df();
            let renamed = df.lazy().rename(["x"], ["xx"], true).unwrap().collect().unwrap();
            let grouped = df.lazy().group_by_stable([polars::col("y")]).unwrap().agg([polars::col("x").sum()]).unwrap().collect().unwrap();
            let by = [polars::col("z")];
            let sorted = df.lazy().select_([polars::col("x").sort_by(by, polars::SortMultipleOptions::default_().with_order_descending(true)).unwrap()]).unwrap().collect().unwrap();
            let over = df.lazy().select_([polars::col("x").sum().over([polars::col("y")]).unwrap()]).unwrap().collect().unwrap();
            // a list column is outside the adapter's collectable dtypes (record 0058), so the plan's schema is checked instead
            let listed = df.lazy().select_([polars::concat_list([polars::col("x"), polars::col("x")]).unwrap().alias("l")]).unwrap().logical_plan().compute_schema();
            let picked = df.lazy().select_([polars::cols(["x", "y"]).unwrap().as_expr()]).unwrap().collect().unwrap();
            `${renamed.column("xx").is_ok()} ${grouped.height()} ${sorted.column("x").unwrap().i64().unwrap().get(0).unwrap() == Some(3)} ${over.height()} ${listed.is_ok()} ${picked.width()} ${by.len()}`
        }
    "#);
    assert_eq!(result, "true 3 true 3 true 2 1");
}

#[test]
fn list_builders_take_script_vectors_borrowed_for_the_call() {
    let result = run(r#"
        pub fn main() {
            let strings = ["a", "", "bc"];
            let sb = polars::ListStringChunkedBuilder::new("s", 2, 8).unwrap();
            sb.append_values_iter(strings).unwrap();
            sb.append_values_iter([]).unwrap();
            sb.append_trusted_len_iter([Some("q"), None]).unwrap();
            let s = sb.finish();
            let bytes = [[0, 255], []];
            let bb = polars::ListBinaryChunkedBuilder::new("b", 1, 4).unwrap();
            bb.append_values_iter(bytes).unwrap();
            bb.append_trusted_len_iter([Some([7]), None]).unwrap();
            let b = bb.finish();
            let ob = polars::ListBooleanChunkedBuilder::new("o", 1, 4).unwrap();
            ob.append_iter([Some(true), None]).unwrap();
            let o = ob.finish();
            let over = polars::ListBinaryChunkedBuilder::new("x", 1, 1).unwrap().append_values_iter([[256]]);
            `${s.len()} ${b.len()} ${o.len()} ${strings[2]} ${bytes[0][1]} ${over.is_err()}`
        }
    "#);
    assert_eq!(result, "3 2 1 bc 255 true");
}
