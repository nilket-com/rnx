//! Synchronous ABI for a startup-linked execution engine on x86-64 GNU/Linux.
#![cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
mod adapter_inputs;
mod adapters;
mod handles;
mod provider;
#[path = "../provider_abi.rs"]
mod provider_abi;
use rnx::image_file::{self, Outcome};
use std::{
	cell::Cell,
	ffi::c_void,
	mem::{offset_of, size_of},
	panic::{AssertUnwindSafe, catch_unwind},
	path::Path,
	ptr, slice, str,
};

type Apply = unsafe extern "C" fn(*mut c_void, i64) -> i64;
#[repr(C)]
pub struct Request {
	version: u32,
	size: u32,
	flags: u32,
	reserved: u32,
	path: *const u8,
	path_len: usize,
	fuel: u64,
	apply: Option<Apply>,
	context: *mut c_void,
	buffer: *mut u8,
	capacity: usize,
	written: *mut usize,
	needed: *mut usize,
}
const _: () = assert!(
	size_of::<Request>() == 88
		&& offset_of!(Request, path) == 16
		&& offset_of!(Request, apply) == 40
		&& offset_of!(Request, needed) == 80
);
const INVALID: i32 = 1;
const FAILED: i32 = 3;

thread_local! {static CALL: Cell<Option<(Option<Apply>, *mut c_void)>> = const {Cell::new(None)};}
struct Scope;
impl Drop for Scope {
	fn drop(&mut self) {
		CALL.set(None);
	}
}
fn dispatch(value: i64) -> i64 {
	let (Some(apply), ctx) = CALL.get().expect("invocation scope") else {
		unreachable!("callback enrolled only for a supplied function")
	};
	// SAFETY: synchronous on the calling thread, within the descriptor's lifetime.
	unsafe { apply(ctx, value) }
}
fn context() -> Result<rnx::rune::Context, rnx::rune::ContextError> {
	use rnx::rune::{Context, Module};
	let mut context = Context::with_default_modules()?;
	if CALL.get().is_some_and(|(callback, _)| callback.is_some()) {
		let mut host = Module::with_crate("host")?;
		host.host_function("apply", dispatch)?;
		context.install(host)?;
	}
	rnx::rune::experimental_core::domain::file::initialize_image_thread();
	Ok(context)
}

impl Request {
	fn report(&self, bytes: &[u8], needed: usize) {
		let count = self.capacity.min(bytes.len());
		// SAFETY: the entry verified descriptor consistency; capacities and
		// disjoint, live writable storage are the caller's obligations.
		unsafe {
			if count != 0 {
				ptr::copy_nonoverlapping(bytes.as_ptr(), self.buffer, count);
			}
			self.written.write(count);
			self.needed.write(needed);
		}
	}
}

/// Executes once. See the C header for status, lifetime and threading rules.
///
/// # Safety
/// The caller supplies a readable aligned header and, if accepted, the
/// declared readable prefix and disjoint buffers valid for this call.
/// A supplied callback must return normally without unwinding.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_engine_run_file_v1(p: *const Request) -> i32 {
	if p.is_null() {
		return INVALID;
	}
	// Read only the two guaranteed words, even when adjacent to an unmapped page.
	let header = p.cast::<u32>();
	if unsafe { header.read() } != 1
		|| (unsafe { header.add(1).read() } as usize) < size_of::<Request>()
	{
		return INVALID;
	}
	let r = unsafe { &*p };
	if r.flags & !3 != 0
		|| r.reserved != 0
		|| r.fuel == 0
		|| r.fuel == u64::MAX
		|| r.path.is_null()
		|| r.path_len == 0
		|| r.path_len > isize::MAX as usize
		|| (r.apply.is_none() && !r.context.is_null())
		|| (r.capacity != 0 && r.buffer.is_null())
		|| r.capacity > isize::MAX as usize
		|| r.written.is_null()
		|| r.needed.is_null()
		|| CALL.get().is_some()
	{
		return INVALID;
	}
	let bytes = unsafe { slice::from_raw_parts(r.path, r.path_len) };
	let Ok(path) = str::from_utf8(bytes) else {
		return INVALID;
	};
	if path.as_bytes().contains(&0) {
		return INVALID;
	}
	CALL.set(Some((r.apply, r.context)));
	let _scope = Scope;
	let outcome = catch_unwind(AssertUnwindSafe(|| {
		image_file::run(
			Path::new(path),
			r.flags & 1 != 0,
			r.flags & 2 != 0,
			r.fuel as usize,
			context,
			r.capacity,
		)
	}));
	match outcome {
		Ok(Ok((status, diagnostic))) => {
			r.report(diagnostic.bytes(), diagnostic.needed());
			match status {
				Outcome::Complete => 0,
				Outcome::Refused => 2,
				Outcome::Failed => FAILED,
			}
		}
		Ok(Err(message)) => {
			r.report(message.as_bytes(), message.len());
			FAILED
		}
		Err(_) => {
			let msg = b"engine panicked";
			r.report(msg, msg.len());
			FAILED
		}
	}
}

/// Handle-based checked file entry.
///
/// # Safety
/// All storage and callbacks satisfy the v2 contract in engine_image.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_engine_handles_v2(p: *const handles::Request) -> i32 {
	if CALL.get().is_some() {
		return INVALID;
	}
	CALL.set(Some((None, ptr::null_mut())));
	let _scope = Scope;
	catch_unwind(AssertUnwindSafe(|| unsafe { handles::dispatch(p, 2) })).unwrap_or(FAILED)
}

/// Adapter-backed synchronous checked file invocation.
///
/// # Safety
/// All storage and callbacks satisfy the v3 contract in adapter_provider.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_engine_adapters_v3(p: *const handles::Request) -> i32 {
	if CALL.get().is_some() {
		return INVALID;
	}
	CALL.set(Some((None, ptr::null_mut())));
	let _scope = Scope;
	catch_unwind(AssertUnwindSafe(|| unsafe { handles::dispatch(p, 3) })).unwrap_or(FAILED)
}
