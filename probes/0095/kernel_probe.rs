// record 0095 gate 1 probe (scratch, not committed): lhs_div / lhs_rem on every numeric type
use polars::prelude as p;
use polars::prelude::*;
fn show<T: PolarsNumericType>(ca: &ChunkedArray<T>) -> String where T::Native: std::fmt::Debug {
    ca.iter().map(|v| v.map_or("n".to_string(), |v| format!("{v:?}"))).collect::<Vec<_>>().join(",")
}
fn row<T: PolarsNumericType>(name: &str, vals: Vec<Option<T::Native>>, lhs: Vec<T::Native>) where T::Native: std::fmt::Debug {
    let ca: ChunkedArray<T> = ChunkedArray::from_iter_options("x".into(), vals.into_iter());
    for l in lhs {
        let d = ca.lhs_div::<T::Native>(l);
        let r = ca.lhs_rem::<T::Native>(l);
        println!("{name} lhs={l:?} rhs=[{}] div=[{}] rem=[{}]", show(&ca), show(&d), show(&r));
    }
}
macro_rules! sint { ($T:ty, $n:ty) => { row::<$T>(stringify!($T), vec![Some(0), Some(1), Some(-1), Some(2), Some(-3), Some(<$n>::MIN), Some(<$n>::MAX), None], vec![0, 7, -7, <$n>::MIN, <$n>::MAX]) } }
macro_rules! uint { ($T:ty, $n:ty) => { row::<$T>(stringify!($T), vec![Some(0), Some(1), Some(2), Some(3), Some(<$n>::MAX), None], vec![0, 7, <$n>::MAX]) } }
macro_rules! flt { ($T:ty, $n:ty) => { row::<$T>(stringify!($T), vec![Some(0.0), Some(-0.0), Some(2.0), Some(-3.0), Some(<$n>::NAN), Some(<$n>::INFINITY), Some(<$n>::NEG_INFINITY), None], vec![0.0, -0.0, 7.0, -7.0, <$n>::NAN, <$n>::INFINITY]) } }
fn main() {
    sint!(p::Int8Type, i8); sint!(p::Int16Type, i16); sint!(p::Int32Type, i32); sint!(p::Int64Type, i64);
    uint!(p::UInt8Type, u8); uint!(p::UInt16Type, u16); uint!(p::UInt32Type, u32); uint!(p::UInt64Type, u64);
    flt!(p::Float32Type, f32); flt!(p::Float64Type, f64);
    // empty, multi-chunk
    let e: p::Int64Chunked = p::Int64Chunked::from_vec("x".into(), vec![]);
    println!("empty div=[{}] rem=[{}]", show(&e.lhs_div::<i64>(5)), show(&e.lhs_rem::<i64>(5)));
    let mut m = p::Int64Chunked::from_vec("x".into(), vec![2, 0]);
    m.append(&p::Int64Chunked::from_iter_options("x".into(), [Some(-3), None].into_iter())).unwrap();
    println!("multi chunks={} div=[{}] rem=[{}]", m.chunks().len(), show(&m.lhs_div::<i64>(7)), show(&m.lhs_rem::<i64>(7)));
}
