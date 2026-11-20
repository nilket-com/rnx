use std::sync::OnceLock;
use std::{
	env,
	ffi::{CString, c_char, c_int, c_void},
	sync::Barrier,
};

#[path = "../../../../rune/crates/rune-alloc/src/limit/initial_exec.rs"]
mod backing;

struct BudgetOps {
	take: unsafe fn(usize) -> bool,
	release: unsafe fn(usize),
}
static ENGINE: OnceLock<BudgetOps> = OnceLock::new();
type Check = extern "C" fn(usize) -> i32;
type Provider = unsafe extern "C" fn(Check, usize) -> i32;
#[repr(C)]
struct Info {
	name: *const c_char,
	base: *mut c_void,
	sym: *const c_char,
	addr: *mut c_void,
}
#[link(name = "dl")]
unsafe extern "C" {
	fn dladdr(addr: *const c_void, info: *mut Info) -> c_int;
	fn dlopen(name: *const c_char, mode: c_int) -> *mut c_void;
	fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}
extern "C" fn check(n: usize) -> i32 {
	use backing::*;
	let engine = ENGINE.get().unwrap();
	unsafe {
		// A genuine late-loaded provider re-enters the host on this thread.
		if (engine.take)(n + 1) || !(engine.take)(n) || (engine.take)(1) {
			return 1;
		}
		(engine.release)(n);
		(engine.release)(usize::MAX);
		if !(engine.take)(usize::MAX) || (engine.take)(1) {
			return 2;
		}
		(engine.release)(usize::MAX);
	}
	if rune_memory_get() != n || rune_memory_take(n + 1) || rune_memory_get() != n {
		return 3;
	}
	let saved = rune_memory_replace(3);
	if saved != n || !rune_memory_take(3) || rune_memory_take(1) {
		return 4;
	}
	let inner = rune_memory_replace(5);
	if inner != 0 || !rune_memory_take(5) || rune_memory_replace(inner) != 0 {
		return 5;
	}
	if rune_memory_get() != 0 || rune_memory_replace(saved) != 0 || rune_memory_get() != n {
		return 6;
	}
	rune_memory_release(usize::MAX);
	if rune_memory_get() != usize::MAX {
		return 7;
	}
	0
}
fn main() {
	let args: Vec<String> = env::args().collect();
	let h = unsafe { dlopen(c"libengine_image.so".as_ptr(), 2 | 4) }; // RTLD_NOW | NOLOAD
	assert!(!h.is_null());
	let api = unsafe { dlsym(h, c"rnx_engine_run_file_v1".as_ptr()) };
	let mut info: Info = unsafe { std::mem::zeroed() };
	assert!(!api.is_null() && unsafe { dladdr(api, &mut info) } != 0);
	let addr = |s: &str| info.base as usize + usize::from_str_radix(s, 16).unwrap();
	ENGINE
		.set(BudgetOps {
			take: unsafe { std::mem::transmute::<usize, unsafe fn(usize) -> bool>(addr(&args[1])) },
			release: unsafe { std::mem::transmute::<usize, unsafe fn(usize)>(addr(&args[2])) },
		})
		.ok()
		.unwrap();
	let barrier = Barrier::new(3);
	let provider: OnceLock<Provider> = OnceLock::new();
	std::thread::scope(|s| {
		let mut workers = Vec::new();
		for n in [31, 127] {
			let b = &barrier;
			let p = &provider;
			workers.push(s.spawn(move || {
				assert_eq!(backing::rune_memory_replace(n), usize::MAX);
				assert!(unsafe { (ENGINE.get().unwrap().take)(usize::MAX - n) });
				b.wait();
				b.wait();
				assert_eq!(unsafe { p.get().unwrap()(check, n) }, 0);
			}));
		}
		barrier.wait();
		let path = CString::new(args[3].as_bytes()).unwrap();
		assert!(unsafe { dlopen(path.as_ptr(), 2 | 4) }.is_null());
		let handle = unsafe { dlopen(path.as_ptr(), 2) };
		assert!(!handle.is_null());
		let f = unsafe { dlsym(handle, c"check_on_caller_thread".as_ptr()) };
		assert!(!f.is_null());
		provider
			.set(unsafe { std::mem::transmute::<*mut c_void, Provider>(f) })
			.unwrap();
		barrier.wait();
		for w in workers {
			w.join().unwrap()
		}
	});
	assert!(unsafe { (ENGINE.get().unwrap().take)(usize::MAX) });
}
