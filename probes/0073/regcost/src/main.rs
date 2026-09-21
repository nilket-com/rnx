//! Record 0073 gate 1: what registering N functions costs at context build.
//! Prints, per N: module build, context install, runtime build, and a
//! one-line script compile+run, in microseconds, median of 15.
use rune::{Any, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
use std::time::Instant;

#[derive(Any, Clone)]
#[rune(item = ::polars)]
struct Frame(i64);

fn stub(f: &Frame, x: i64) -> i64 { f.0 + x }

macro_rules! types { ($($t:ident),*) => { $( #[derive(Any, Clone)] #[rune(item = ::polars)] struct $t(i64); )* fn install_types(m: &mut Module, n: usize) { let fs: &[fn(&mut Module)] = &[ $( |m| { m.ty::<$t>().unwrap(); } ),* ]; for f in fs.iter().take(n) { f(m); } } } }
types!(T0,T1,T2,T3,T4,T5,T6,T7,T8,T9,T10,T11,T12,T13,T14,T15,T16,T17,T18,T19,T20,T21,T22,T23,T24,T25,T26,T27,T28,T29,T30,T31,T32,T33,T34,T35,T36,T37,T38,T39,T40,T41,T42,T43,T44,T45,T46,T47,T48,T49,T50,T51,T52,T53,T54,T55,T56,T57,T58,T59,T60,T61,T62,T63,T64,T65,T66,T67,T68,T69,T70,T71,T72,T73,T74,T75,T76,T77,T78,T79,T80,T81,T82,T83,T84,T85,T86,T87,T88,T89,T90,T91,T92,T93,T94,T95,T96,T97,T98,T99);

fn module(n: usize) -> Module {
    module_with(n, TYPES.with(|t| t.get()))
}
thread_local! { static TYPES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

fn module_with(n: usize, types: usize) -> Module {
    let mut m = Module::with_crate("polars").unwrap();
    m.ty::<Frame>().unwrap();
    install_types(&mut m, types);
    m.function("frame", || Frame(1)).build().unwrap();
    // Names must be 'static for rune: leak them once per run.
    for i in 0..n {
        let name: &'static str = Box::leak(format!("m{i}").into_boxed_str());
        m.function(name, stub).build_associated::<Frame>().unwrap();
    }
    m
}

fn median(mut v: Vec<u128>) -> u128 { v.sort(); v[v.len() / 2] }

fn main() {
    println!("| shape | module build µs | context install µs | runtime µs | compile+run µs | total µs |");
    println!("|---:|---:|---:|---:|---:|---:|");
    for &(types, n) in &[(0usize, 0usize), (100, 0), (0, 1000), (100, 1000), (0, 3000), (100, 3000)] {
        TYPES.with(|t| t.set(types));
        let (mut a, mut b, mut c, mut d) = (vec![], vec![], vec![], vec![]);
        for _ in 0..15 {
            let t0 = Instant::now();
            let m = module(n);
            let t1 = Instant::now();
            let mut ctx = Context::with_default_modules().unwrap();
            ctx.install(m).unwrap();
            let t2 = Instant::now();
            let rt = Arc::new(ctx.runtime().unwrap());
            let t3 = Instant::now();
            let mut sources = Sources::new();
            sources.insert(Source::memory("pub fn main() { let f = polars::frame(); f.m0(1) }").unwrap()).unwrap();
            let unit = rune::prepare(&mut sources).with_context(&ctx).build();
            let unit = match unit { Ok(u) => u, Err(_) => { rune::prepare(&mut sources).with_context(&ctx).build().unwrap() } };
            let mut vm = Vm::new(rt, Arc::new(unit));
            let _ = vm.call(["main"], ()).ok();
            let t4 = Instant::now();
            a.push((t1 - t0).as_micros()); b.push((t2 - t1).as_micros()); c.push((t3 - t2).as_micros()); d.push((t4 - t3).as_micros());
        }
        let (a, b, c, d) = (median(a), median(b), median(c), median(d));
        println!("| {types} types, {n} functions | {a} | {b} | {c} | {d} | {} |", a + b + c + d);
    }
}
