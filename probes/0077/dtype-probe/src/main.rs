use polars::prelude::*;
fn main() {
    // the five dtypes whose fixtures fail today: can any route build them under these features?
    let routes: Vec<(&str, Box<dyn Fn() -> PolarsResult<Series>>)> = vec![
        ("Int8 via cast", Box::new(|| Series::new("x".into(), [1i64, 2]).cast(&DataType::Int8))),
        ("Int8 via full_null", Box::new(|| Ok(Series::full_null("x".into(), 2, &DataType::Int8)))),
        ("Int8 via new_empty", Box::new(|| Ok(Series::new_empty("x".into(), &DataType::Int8)))),
        ("Int8 via from_any_values", Box::new(|| Series::from_any_values_and_dtype("x".into(), &[AnyValue::Int8(1), AnyValue::Int8(2)], &DataType::Int8, true))),
        ("UInt16 via from_any_values", Box::new(|| Series::from_any_values_and_dtype("x".into(), &[AnyValue::UInt16(1)], &DataType::UInt16, true))),
        ("BinaryOffset via cast", Box::new(|| Series::new("x".into(), [&b"ab"[..], b"c"]).cast(&DataType::BinaryOffset))),
        ("BinaryOffset via from_any_values", Box::new(|| Series::from_any_values_and_dtype("x".into(), &[AnyValue::Binary(b"ab")], &DataType::BinaryOffset, true))),
        ("BinaryOffset via full_null", Box::new(|| Ok(Series::full_null("x".into(), 2, &DataType::BinaryOffset)))),
    ];
    for (name, f) in routes {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f())) {
            Ok(Ok(s)) => println!("{name}: ok dtype={:?} len={} nulls={}", s.dtype(), s.len(), s.null_count()),
            Ok(Err(e)) => println!("{name}: ERR {e}"),
            Err(e) => println!("{name}: PANIC {}", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()),
        }
    }
}
