use super::super::tests::{compile, main_of};
use super::*;
use crate::experimental_core::{
	execute::{take_remaining_work_for_test, Executor, ExecutorLimits},
	heap::{Limits, Store},
};
use crate::Hash;

const FIB: &str = "fn fib(n) { if n<2 { n } else { fib(n-1)+fib(n-2) } } pub fn main(n) { fib(n) }";
const TAIL:&str="fn tail(n,total) { if n<=0 { total } else { tail(n-1,total+n) } } pub fn main(n) { tail(n,0) }";
const FACT: &str = "fn fact(n) { if n<=1 { 1 } else { n*fact(n-1) } } pub fn main(n) { fact(n) }";

#[test]
fn n5_admission_and_deferrals() {
	for (src, name) in [
		(FIB, "fib"),
		(TAIL, "tail"),
		(FACT, "fact"),
		(
			"fn rec(n) { if n==0 { true } else { rec(n-1) } } pub fn main(n) { rec(n) }",
			"rec",
		),
		(
			"fn rec(n) { if n==0 { () } else { rec(n-1) } } pub fn main(n) { rec(n) }",
			"rec",
		),
	] {
		let p = compile(src);
		let id = p.function(Hash::type_hash([name])).unwrap();
		let budget = Budget::new(8 << 20);
		let (s, bytes) = analyze(&p.code, id, &budget).expect(name);
		assert_eq!(s.function, id);
		assert!(bytes > 0);
		drop(s);
		budget.release(bytes);
		assert_eq!(budget.used(), 0);
		assert!(analyze(&p.code, main_of(&p), &budget).is_none());
		assert_eq!(budget.used(), 0);
	}
	for src in [
        "fn rec(n) { if n<1 { \"s\" } else { rec(n-1) } } pub fn main(n) { rec(n) }",
        "fn rec(n) { for i in 0..n { rec(n-1); } 1 } pub fn main(n) { rec(n) }",
        "fn other(n) { rec(n) } fn rec(n) { if n<1 { 0 } else { other(n-1) } } pub fn main(n) { rec(n) }",
        "fn helper(n) { n } fn rec(n) { if n<1 { helper(n) } else { rec(n-1) } } pub fn main(n) { rec(n) }",
    ]{let p=compile(src);let id=p.function(Hash::type_hash(["rec"])).unwrap();let b=Budget::new(8<<20);assert!(analyze(&p.code,id,&b).is_none());assert_eq!(b.used(),0);}
}

#[test]
fn n5_safe_transaction_refusal_and_invalid_records() {
	let p = compile(FACT);
	let id = p.function(Hash::type_hash(["fact"])).unwrap();
	let b = Budget::new(8 << 20);
	let (s, bytes) = analyze(&p.code, id, &b).unwrap();
	let store = Store::new(Limits::default()).unwrap();
	let mut w = vec![Value::Unit; s.slots];
	w[0] = Value::I64(7);
	let original = w.clone();
	let room = super::super::commit::Room {
		frames: 8,
		slots: 128,
	};
	for kind in 0..5 {
		let outcome = enter(
			&s,
			&mut w,
			&store,
			kind == 0,
			100,
			room,
			if kind == 1 { 0 } else { 8 << 20 },
			|_, r, private| {
				private[0] = 99;
				if kind == 2 {
					r.status = 2;
					return Some(2);
				}
				if kind == 3 {
					r.status = 1;
					r.fuel = 100;
					return Some(1);
				}
				r.reserved = 1;
				Some(1)
			},
		);
		assert_eq!(w, original);
		assert!(matches!(
			(kind, outcome),
			(0 | 1, Outcome::Refused)
				| (2, Outcome::Replay { work: 0 })
				| (3 | 4, Outcome::Invalid)
		));
	}
	drop(s);
	b.release(bytes);
	assert_eq!(b.used(), 0);
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_executor_every_fuel_and_limits() {
	use super::super::cache;
	for src in [FIB,TAIL,FACT,
        "fn rec(n) { if n==0 { 9223372036854775807 } else { rec(n-1)+1 } } pub fn main(n) { rec(n) }",
        "fn rec(n) { if n==0 { true } else { rec(n-1) } } pub fn main(n) { rec(n) }",
        "fn rec(n) { if n==0 { () } else { rec(n-1) } } pub fn main(n) { rec(n) }",
        "fn rec(flag) { if flag { rec(false) } else { true } } pub fn main(n) { rec(n>0) }"] {
        let p=compile(src);let main=main_of(&p);cache::clear_for_test();cache::enable_for_test(true);cache::prepare(&p.code).unwrap();

        if cache::recursive_counters_for_test(p.code.id()).is_empty(){
            for id in 0..p.code.functions() as u32 {
                let f=p.code.function(id).unwrap();let b=Budget::new(8<<20);
                let analyzed=analyze(&p.code,id,&b);
                if let Some((shape,bytes))=analyzed {
                    let result=crate::experimental_native::lower::recursion::compile(&shape,0);
                    std::println!("N5 diagnostic function{id} compile={:?}",result.as_ref().map(|_|()));
                    drop(result);drop(shape);b.release(bytes);
                }else{std::println!("N5 diagnostic function{id} analysis REFUSED");}
                for pc in 0..f.len(){std::println!("f{id} pc{pc} {:?}",f.operation(pc).unwrap());}
            }
        }
        assert_eq!(cache::recursive_counters_for_test(p.code.id()).len(),1,"{src}: {:?}",cache::summary(p.code.id()));
        for fuel in 0..=48 {
            for limits in [ExecutorLimits::default(),ExecutorLimits{frames:4,slots:128},ExecutorLimits{frames:64,slots:24}] {
                let mut observations=std::vec::Vec::new();
                for native in [false,true]{
                    let mut store=Store::new(Limits::default()).unwrap();let arg=store.root_i64(5);let mut e=Executor::with_limits(limits).unwrap();e.set_native_enabled(native);
                    let recursive=(0..p.code.functions() as u32).find(|&id|id!=main).unwrap();capture_return(recursive,2);
                    let result=e.invoke_code(&p.code,&mut store,main,&[&arg],fuel);
                    let observed=match result {Ok(r)=>{let v=store.root_scalar(&r).unwrap();store.root_release(r).unwrap();Ok(v)},Err(f)=>Err((f.error,f.call_chain,f.cleanup_error,f.remaining_work))};
                    observations.push((observed,take_remaining_work_for_test(),e.capacities_for_test(),take_return()));assert!(e.is_idle());assert_eq!(store.live_entries(),0);store.root_release(arg).unwrap();
                }
                assert_eq!(observations[0],observations[1],"fuel{fuel} limits{limits:?} {src}");
            }
        }
        cache::clear_for_test();cache::enable_for_test(false);
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_cold_warm_and_bounded_replay() {
	use super::super::cache;
	for (src,arg,fuel) in [(FIB,15,10000),(TAIL,200,10000),
        ("fn rec(n) { if n==0 { 9223372036854775807 } else { rec(n-1)+1 } } pub fn main(n) { rec(n) }",200,10000),
        ("fn rec(n) { rec(n) } pub fn main(n) { rec(n) }",1,500)] {
        let p=compile(src);cache::clear_for_test();cache::enable_for_test(true);cache::prepare(&p.code).unwrap();
        let mut store=Store::new(Limits::default()).unwrap();let arg=store.root_i64(arg);let mut e=Executor::new().unwrap();
        for iteration in 0..2 {
            let before=cache::recursive_counters_for_test(p.code.id())[0];
            let r=e.invoke_code(&p.code,&mut store,main_of(&p),&[&arg],fuel);
            if let Ok(r)=r {store.root_release(r).unwrap();}
            let after=cache::recursive_counters_for_test(p.code.id())[0];
            assert_eq!(after.0-before.0,1,"one attempt per dynamic root");
            if src==FIB || src==TAIL {
                assert_eq!(after.1-before.1,u64::from(iteration==1));
                if iteration==1 {assert!(after.3>0 && after.4>1);}
            }else{
                assert_eq!(after.2-before.2,1);
                assert!(after.5-before.5<=fuel as u64*64,"bounded speculative work");
            }
        }
        store.root_release(arg).unwrap();assert!(e.is_idle());cache::clear_for_test();cache::enable_for_test(false);
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_allocation_limits_and_zero_quota_factorial() {
	use super::super::cache;
	for src in [FACT, FIB] {
		let p = compile(src);
		cache::clear_for_test();
		cache::enable_for_test(true);
		cache::prepare(&p.code).unwrap();
		for quota in (0..=2048).step_by(16) {
			let mut observations = std::vec::Vec::new();
			for native in [false, true] {
				let mut store = Store::new(Limits::default()).unwrap();
				let arg = store.root_i64(if src == FACT { 5 } else { 12 });
				let mut e = Executor::new().unwrap();
				e.set_native_enabled(native);
				let r = rune_alloc::limit::with(quota, || {
					(e.invoke_code(&p.code, &mut store, main_of(&p), &[&arg], 10000),)
				})
				.call()
				.0;
				let r = match r {
					Ok(v) => {
						let result = store.root_scalar(&v).unwrap();
						store.root_release(v).unwrap();
						Ok(result)
					}
					Err(f) => Err((f.error, f.call_chain, f.cleanup_error, f.remaining_work)),
				};
				if src == FACT && quota == 0 {
					assert!(r.is_ok());
				}
				observations.push((r, take_remaining_work_for_test(), e.capacities_for_test()));
				assert!(e.is_idle());
				store.root_release(arg).unwrap();
			}
			assert_eq!(observations[0], observations[1], "quota{quota} {src}");
		}
		cache::clear_for_test();
		cache::enable_for_test(false);
	}
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_native_boundary_physical_scratch_checks() {
	let p = compile(FIB);
	let id = p.function(Hash::type_hash(["fib"])).unwrap();
	let b = Budget::new(8 << 20);
	let (shape, bytes) = analyze(&p.code, id, &b).unwrap();
	let compiled = crate::experimental_native::lower::recursion::compile(&shape, 0).unwrap();
	let native = crate::experimental_native::boundary::NativeRecursion::load(&compiled).unwrap();
	assert!(native
		.call(&mut [0], &mut Record::new(100), &mut [])
		.is_none());
	for cells in 1..2 * shape.slots {
		let mut scratch = vec![0i64; cells];
		let mut r = Record::new(100);
		assert_eq!(native.call(&mut [1], &mut r, &mut scratch), Some(2));
		assert_eq!(r.fuel, 100);
	}
	let mut w = vec![Value::Unit; shape.slots];
	w[0] = Value::I64(10);
	let before = w.clone();
	let store = Store::new(Limits::default()).unwrap();
	let r = enter(
		&shape,
		&mut w,
		&store,
		false,
		1000,
		super::super::commit::Room {
			frames: 0,
			slots: 0,
		},
		8 << 20,
		|b, r, s| native.call(b, r, s),
	);
	assert!(matches!(r, Outcome::Replay { .. }));
	assert_eq!(w, before);
	drop(shape);
	b.release(bytes);
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_all_frame_and_slot_limits_and_reuse_after_failure() {
	use super::super::cache;
	let p = compile(FIB);
	cache::clear_for_test();
	cache::enable_for_test(true);
	cache::prepare(&p.code).unwrap();
	for frames in 1..=12 {
		for slots in 1..=96 {
			let mut observations = std::vec::Vec::new();
			for native in [false, true] {
				let mut s = Store::new(Limits::default()).unwrap();
				let arg = s.root_i64(7);
				let mut e = Executor::with_limits(ExecutorLimits { frames, slots }).unwrap();
				e.set_native_enabled(native);
				for _ in 0..2 {
					let r = e.invoke_code(&p.code, &mut s, main_of(&p), &[&arg], 1000);
					let r = match r {
						Ok(v) => {
							let result = s.root_scalar(&v).unwrap();
							s.root_release(v).unwrap();
							Ok(result)
						}
						Err(f) => Err((f.error, f.call_chain, f.cleanup_error, f.remaining_work)),
					};
					observations.push((r, take_remaining_work_for_test(), e.capacities_for_test()));
					assert!(e.is_idle());
				}
				s.root_release(arg).unwrap();
			}
			assert_eq!(
				observations[0], observations[2],
				"frames{frames} slots{slots}"
			);
			assert_eq!(
				observations[1], observations[3],
				"reuse frames{frames} slots{slots}"
			);
		}
	}
	cache::clear_for_test();
	cache::enable_for_test(false);
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_cold_cache_does_not_consume_script_allocation_quota() {
	use super::super::cache;
	for src in [FACT, FIB] {
		let p = compile(src);
		for quota in (0..=4096).step_by(16) {
			let mut observations = std::vec::Vec::new();
			for native in [false, true] {
				cache::clear_for_test();
				cache::enable_for_test(true);
				let mut store = Store::new(Limits::default()).unwrap();
				let arg = store.root_i64(if src == FACT { 5 } else { 12 });
				let mut e = Executor::new().unwrap();
				e.set_native_enabled(native);
				let (r,) = rune_alloc::limit::with(quota, || {
					(e.invoke_code(&p.code, &mut store, main_of(&p), &[&arg], 10000),)
				})
				.call();
				let r = match r {
					Ok(v) => {
						let result = store.root_scalar(&v).unwrap();
						store.root_release(v).unwrap();
						Ok(result)
					}
					Err(f) => Err((f.error, f.call_chain, f.cleanup_error, f.remaining_work)),
				};
				observations.push((r, take_remaining_work_for_test(), e.capacities_for_test()));
				assert!(e.is_idle());
				store.root_release(arg).unwrap();
			}
			assert_eq!(observations[0], observations[1], "cold quota{quota} {src}");
		}
	}
	cache::clear_for_test();
	cache::enable_for_test(false);
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: native machine code")]
#[test]
fn n5_cancel_and_wrong_parameter_types() {
	use super::super::cache;
	use crate::experimental_core::execute::cancel_at_charge_for_test;
	let p = compile(FIB);
	cache::clear_for_test();
	cache::enable_for_test(true);
	cache::prepare(&p.code).unwrap();
	for cut in 0..=32 {
		let mut observations = std::vec::Vec::new();
		for native in [false, true] {
			let mut s = Store::new(Limits::default()).unwrap();
			let arg = s.root_i64(5);
			let mut e = Executor::new().unwrap();
			e.set_native_enabled(native);
			cancel_at_charge_for_test(Some(cut));
			let r = e.invoke_code(&p.code, &mut s, main_of(&p), &[&arg], 1000);
			cancel_at_charge_for_test(None);
			let r = match r {
				Ok(v) => {
					let result = s.root_scalar(&v).unwrap();
					s.root_release(v).unwrap();
					Ok(result)
				}
				Err(f) => Err((f.error, f.call_chain, f.cleanup_error, f.remaining_work)),
			};
			observations.push((r, take_remaining_work_for_test(), e.capacities_for_test()));
			s.root_release(arg).unwrap();
			assert!(e.is_idle());
		}
		assert_eq!(observations[0], observations[1], "cancel cut{cut}");
	}
	let mut observations = std::vec::Vec::new();
	for native in [false, true] {
		let mut s = Store::new(Limits::default()).unwrap();
		let arg = s.root_bool(true);
		let mut e = Executor::new().unwrap();
		e.set_native_enabled(native);
		let f = e
			.invoke_code(&p.code, &mut s, main_of(&p), &[&arg], 1000)
			.unwrap_err();
		observations.push((
			f.error,
			f.call_chain,
			f.remaining_work,
			e.capacities_for_test(),
		));
		s.root_release(arg).unwrap();
		assert!(e.is_idle());
	}
	assert_eq!(observations[0], observations[1]);
	cache::clear_for_test();
	cache::enable_for_test(false);
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[cfg_attr(miri, ignore = "Miri: observes the production native cache")]
#[test]
fn n5_page_refusal_and_recursive_checkout_lifetime() {
	use super::super::cache;
	let p = compile(FACT);
	cache::clear_for_test();
	cache::enable_for_test(true);
	cache::set_page_size_for_test(Some(16384));
	let s = cache::prepare(&p.code).unwrap();
	assert_eq!((s.loaded, s.page_size), (0, 1));
	cache::clear_for_test();
	cache::set_page_size_for_test(None);
	assert!(matches!(
		cache::checkout(&p.code),
		cache::Checkout::Interpret { no_regions: false }
	));
	let guard = cache::checkout_recursion(&p.code).expect("recursive side entry");
	assert!(matches!(
		cache::checkout(&p.code),
		cache::Checkout::Interpret { no_regions: false }
	));
	assert!(!cache::evict(p.code.id()));
	drop(guard);
	assert!(cache::resident_for_test(p.code.id()));
	assert!(cache::evict(p.code.id()));
	// A checked-out handle remains reserved while another recursive entry is
	// evicted/replaced. Clearing drops the side ownership as well as its charges.
	cache::clear_for_test();
	let mut limits = cache::Limits::PRODUCTION;
	limits.cached_programs = 2;
	cache::set_limits_for_test(Some(limits));
	let q = compile(FACT);
	let r = compile(FACT);
	cache::prepare(&p.code).unwrap();
	cache::prepare(&q.code).unwrap();
	let guard = cache::checkout_recursion(&p.code).unwrap();
	cache::prepare(&r.code).unwrap();
	assert!(!cache::resident_for_test(q.code.id()));
	assert!(cache::recursive_counters_for_test(q.code.id()).is_empty());
	assert!(cache::checkout_recursion(&p.code).is_none());
	assert!(cache::checkout_recursion(&r.code).is_some());
	drop(guard);
	assert!(cache::checkout_recursion(&p.code).is_some());
	cache::clear_for_test();
	assert!(cache::recursive_counters_for_test(p.code.id()).is_empty());
	assert!(cache::recursive_counters_for_test(r.code.id()).is_empty());
	assert_eq!(cache::cache_state_for_test(), (0, 0, 0, 0));
	cache::set_limits_for_test(None);
	cache::clear_for_test();
	cache::enable_for_test(false);
}
