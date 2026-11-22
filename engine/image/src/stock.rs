//! Startup-linked stock CLI entry. No callback or borrowed engine value escapes.
use super::{CALL, FAILED, INVALID, Scope};
use std::{
	ffi::{CStr, c_char},
	mem::{offset_of, size_of},
	panic::{AssertUnwindSafe, catch_unwind},
	ptr, slice,
};

#[repr(C)]
pub struct StockRequest {
	version: u32,
	size: u32,
	flags: u32,
	reserved: u32,
	path: *const c_char,
	argv: *const *const c_char,
	argc: usize,
	fuel: usize,
	exit: *mut i32,
}
const _: () = assert!(size_of::<StockRequest>() == 56 && offset_of!(StockRequest, exit) == 48);
const RNX_NATIVE: u32 = 1;
const RNX_DEBUG: u32 = 2;
const RNX_DEBUG_SOURCE: u32 = 4;
const RNX_REFUSED: i32 = 2;

/// # Safety
/// See the v4 request contract in engine_image.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_engine_stock_file_v4(p: *const StockRequest) -> i32 {
	unsafe { file(p, None) }
}

/// # Safety
/// The v4 request and the v3 provider table and callbacks must remain valid
/// for the invocation as specified in the C headers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_engine_provider_file_v4(
	p: *const StockRequest,
	provider: *const crate::provider_abi::Provider,
) -> i32 {
	if provider.is_null() || !provider.is_aligned() {
		return INVALID;
	}
	unsafe { file(p, Some(provider)) }
}

unsafe fn file(
	p: *const StockRequest,
	provider: Option<*const crate::provider_abi::Provider>,
) -> i32 {
	if p.is_null() || !p.is_aligned() || CALL.get().is_some() {
		return INVALID;
	}
	let header = p.cast::<u32>();
	if unsafe { header.read() } != 4
		|| (unsafe { header.add(1).read() } as usize) < size_of::<StockRequest>()
	{
		return INVALID;
	}
	let r = unsafe { &*p };
	if r.flags & !(RNX_NATIVE | RNX_DEBUG | RNX_DEBUG_SOURCE) != 0
		|| r.reserved != 0
		|| r.path.is_null()
		|| r.exit.is_null()
		|| !r.exit.is_aligned()
		|| !(1..usize::MAX).contains(&r.fuel)
		|| r.argc > isize::MAX as usize / size_of::<*const c_char>()
		|| (r.argc != 0 && (r.argv.is_null() || !r.argv.is_aligned()))
	{
		return INVALID;
	}
	CALL.set(Some((None, ptr::null_mut())));
	let _scope = Scope;
	catch_unwind(AssertUnwindSafe(|| {
		let Ok(path) = unsafe { CStr::from_ptr(r.path) }.to_str() else {
			return INVALID;
		};
		if path.is_empty() {
			return INVALID;
		}
		let mut args = Vec::new();
		if r.argc != 0 {
			if args.try_reserve_exact(r.argc).is_err() {
				return FAILED;
			}
			for &arg in unsafe { slice::from_raw_parts(r.argv, r.argc) } {
				if arg.is_null() {
					return INVALID;
				}
				let Ok(value) = unsafe { CStr::from_ptr(arg) }.to_str() else {
					return INVALID;
				};
				args.push(value.to_owned());
			}
		}
		let run = |modules| {
			rnx::image_file::product_file(
				path,
				args,
				r.flags & RNX_NATIVE != 0,
				r.flags & RNX_DEBUG != 0,
				r.flags & RNX_DEBUG_SOURCE != 0,
				r.fuel,
				modules,
			)
		};
		let result = if let Some(table) = provider {
			let (owner, modules) = match unsafe { crate::adapters::install(table) } {
				Ok(value) => value,
				Err(error) => {
					eprintln!("invalid file provider: {error}");
					return INVALID;
				}
			};
			let result = owner.during(|| run(Some(modules)));
			if let Some(error) = crate::adapters::take_fault() {
				eprintln!("file provider failed: {error}");
				return FAILED;
			}
			result
		} else {
			run(None)
		};
		match result {
			Ok(Some(code)) => {
				unsafe { r.exit.write(code) };
				0
			}
			Ok(None) => RNX_REFUSED,
			Err(error) => {
				eprintln!("error: {error}");
				FAILED
			}
		}
	}))
	.unwrap_or(FAILED)
}

#[cfg(rnx_installed_start)]
mod installed {
	use super::*;
	use rnx::image_file::RouteObserver;
	use std::{ffi::c_int, sync::OnceLock};

	include!(env!("RNX_INSTALLED_RUNTIME_IMPORTS"));

	#[link(name = "installed_start", kind = "static")]
	unsafe extern "C" {
		fn rnx_installed_start(argc: c_int, argv: *mut *mut c_char) -> c_int;
	}

	static OBSERVER: OnceLock<Option<RouteObserver>> = OnceLock::new();

	/// # Safety
	/// Called once from the process main thread with the original C argc/argv,
	/// before engine execution. The observer lives until process exit.
	#[unsafe(no_mangle)]
	pub unsafe extern "C" fn rnx_engine_stock_cli_v5(
		argc: c_int,
		argv: *mut *mut c_char,
		observer: Option<RouteObserver>,
	) -> c_int {
		if OBSERVER.set(observer).is_err() {
			return INVALID;
		}
		// SAFETY: the launcher owns process initialization and supplies argv.
		unsafe { rnx_installed_start(argc, argv) }
	}

	#[unsafe(no_mangle)]
	extern "C" fn rnx_installed_body() -> u8 {
		match catch_unwind(AssertUnwindSafe(|| {
			rnx::image_file::installed_file(OBSERVER.get().copied().flatten())
		})) {
			Ok(Ok(())) => 0,
			Ok(Err(error)) => {
				eprintln!("Error: {error:?}");
				1
			}
			Err(_) => 1,
		}
	}
}
