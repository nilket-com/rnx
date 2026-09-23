//! Record 0083 gate 3: concrete monomorphizations compiled and executed
//! against the pinned Polars build. Each check names the generic it
//! instantiates and how a script-owned vector feeds it.
use polars::prelude::*;
use polars_core::utils::Container;

fn df() -> DataFrame { df!("x" => [1i64, 2, 3], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap() }

/// A. Chained inference: `I: IntoIterator<Item = S>, S: AsRef<str> | Into<PlSmallStr>`
/// with `I = Vec<String>`, `S = String`; and `E: AsRef<[IE]>, IE: Into<Expr> + Clone`
/// with `E = Vec<Expr>`, `IE = Expr`. The script vector is owned and consumed.
fn chained() {
    let frame = df();
    let names: Vec<String> = vec!["x".into(), "z".into()];
    let selected = frame.select(names.clone()).unwrap();
    assert_eq!(selected.get_column_names().iter().map(|n| n.as_str()).collect::<Vec<_>>(), ["x", "z"]);
    assert_eq!(names.len(), 2, "the caller's vector is cloned in, not moved");
    let dropped = frame.drop_many(vec![String::from("y")]);
    assert_eq!(dropped.width(), 2);
    let empty: Vec<String> = vec![];
    assert_eq!(frame.select(empty).unwrap().width(), 0, "an empty selection is a zero-column frame");
    let parts = frame.partition_by(vec![String::from("y")], true).unwrap();
    assert_eq!(parts.len(), 3);
    let grouped = frame.group_by(vec![String::from("y")]).unwrap().count().unwrap();
    assert_eq!(grouped.height(), 3);
    let lazy = frame.clone().lazy().group_by(vec![col("y")]).agg([col("x").sum()]).collect().unwrap();
    assert_eq!(lazy.height(), 3);
    let renamed = frame.clone().lazy().rename(vec![String::from("x")], vec![String::from("xx")], true).collect().unwrap();
    assert!(renamed.column("xx").is_ok());
    let sorted = frame.clone().lazy().select([col("x").sort_by(vec![col("z")], SortMultipleOptions::default().with_order_descending(true))]).collect().unwrap();
    assert_eq!(sorted.column("x").unwrap().i64().unwrap().get(0), Some(3));
    let over = frame.clone().lazy().select([col("x").sum().over(vec![col("y")]).unwrap()]).collect().unwrap();
    assert_eq!(over.height(), 3);
    let listed = frame.clone().lazy().select([concat_list(vec![col("x"), col("x")]).unwrap().alias("l")]).collect().unwrap();
    assert_eq!(listed.column("l").unwrap().dtype(), &DataType::List(Box::new(DataType::Int64)));
    let picked = frame.clone().lazy().select([cols(vec![String::from("x"), String::from("y")]).as_expr()]).collect().unwrap();
    assert_eq!(picked.width(), 2);
    println!("A chained inference: ok");
}

/// B. Iterator input on a family pair: `match_chunks<I: Iterator<Item = usize>>`
/// on `Int64Chunked`, fed by `Vec<usize>::into_iter()`. Polars documents
/// "it is the caller's responsibility" and enforces it with `debug_assert!`
/// only (`chunked_array/mod.rs:851-866`): a single-chunk receiver and ids
/// that are chunk lengths summing to the array length; the slicing itself
/// is `sliced_unchecked`. In this debug probe the violations panic; in a
/// release binding they would be undefined behaviour, so a binding must
/// validate both preconditions itself or the operation stays refused.
fn iterator_family() {
    let ca = df().column("x").unwrap().i64().unwrap().clone();
    assert_eq!(ca.n_chunks(), 1);
    let lengths: Vec<usize> = vec![1, 2];
    let matched = ca.match_chunks(lengths.into_iter());
    assert_eq!((matched.n_chunks(), matched.len()), (2, 3), "valid lengths re-chunk the single chunk");
    assert_eq!((ca.n_chunks(), ca.len()), (1, 3), "the receiver is untouched");
    let short_sum = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ca.match_chunks(vec![1usize].into_iter()))).is_err();
    let empty = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ca.match_chunks(Vec::<usize>::new().into_iter()))).is_err();
    let two = { let mut c = ca.clone(); c.append(&ca).unwrap(); c };
    let multi = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| two.match_chunks(vec![6usize].into_iter()))).is_err();
    assert!(short_sum && empty && multi, "lengths that do not sum to the array length, an empty list and a multi-chunk receiver all trip the debug assertions");
    println!("B iterator input on a family pair: ok (debug-only preconditions recorded: unsafe to bind without adapter-side validation)");
}

/// C. Iterator input whose items are borrowed from the script's vector:
/// `append_values_iter<I: Iterator<Item = &str>>` on `ListStringChunkedBuilder`
/// and `append_values_iter<I: Iterator<Item = &[u8]>>` on `ListBinaryChunkedBuilder`,
/// fed by `Vec<String>` and `Vec<Vec<u8>>` borrowed for the call; the vector
/// is readable afterwards. Also a `TrustedLen` bound met by `Vec::into_iter`.
fn borrowed_items() {
    let strings: Vec<String> = vec!["a".into(), "".into(), "bc".into()];
    let mut b = ListStringChunkedBuilder::new("s".into(), 2, 8);
    b.append_values_iter(strings.iter().map(String::as_str));
    let empty: Vec<String> = vec![];
    b.append_values_iter(empty.iter().map(String::as_str));
    let list = b.finish();
    assert_eq!(list.len(), 2);
    assert_eq!(strings[2], "bc", "the script's strings are intact after the borrow");
    let bytes: Vec<Vec<u8>> = vec![vec![0, 255], vec![]];
    let mut bb = ListBinaryChunkedBuilder::new("b".into(), 1, 4);
    bb.append_values_iter(bytes.iter().map(Vec::as_slice));
    let blist = bb.finish();
    assert_eq!(blist.len(), 1);
    assert_eq!(bytes[0], [0, 255]);
    let mut bo = ListBooleanChunkedBuilder::new("o".into(), 1, 4);
    let flags: Vec<Option<bool>> = vec![Some(true), None];
    bo.append_iter(flags.into_iter());
    assert_eq!(bo.finish().len(), 1);
    println!("C borrowed iterator items: ok");
}

/// D. What stays refused: a return-only generic cannot be chosen by a script
/// (`Series::sum::<T>` needs the annotation); shown here only as the Rust
/// form the script cannot express.
fn return_only() {
    let s = df().column("x").unwrap().as_materialized_series().clone();
    let total: i64 = s.sum::<i64>().unwrap();
    assert_eq!(total, 6);
    println!("D return-only generic needs a Rust annotation: noted");
}

fn main() {
    chained();
    iterator_family();
    borrowed_items();
    return_only();
}
