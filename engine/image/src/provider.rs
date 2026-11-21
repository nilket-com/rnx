//! The payload belongs to the C provider. This guard owns one successful create,
//! including while the result is provisional and the Store is admitting it.
use rnx::rune;
use rune::host::{OpaqueProducerLimit, OwnedTreeInput};
use rune::runtime::VmError;
use std::cell::Cell;
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Provider {
	pub cookie: *mut c_void,
	pub create: Option<unsafe extern "C" fn(*mut c_void, usize, *mut Created) -> i32>,
	pub read: Option<unsafe extern "C" fn(*mut c_void, u64) -> i64>,
	pub release: Option<unsafe extern "C" fn(*mut c_void, u64)>,
}
#[repr(C)]
#[derive(Default)]
pub struct Created {
	pub token: u64,
	pub kind: u64,
	pub retained: usize,
	pub temporary: usize,
}

thread_local! {
	static ACTIVE: Cell<Option<Provider>> = const { Cell::new(None) };
}
pub fn during<T>(provider: Provider, f: impl FnOnce() -> T) -> T {
	struct Reset;
	impl Drop for Reset {
		fn drop(&mut self) {
			ACTIVE.set(None);
		}
	}
	ACTIVE.set(Some(provider));
	let _reset = Reset;
	f()
}

#[derive(rune::Any)]
#[rune(module = rnx::rune, item = ::probe)]
struct Item {
	provider: Provider,
	value: Created,
}
impl Drop for Item {
	fn drop(&mut self) {
		// SAFETY: the table was validated at registration. The parent context
		// keeps its cookie live until every invocation and payload is released.
		unsafe { (self.provider.release.unwrap())(self.provider.cookie, self.value.token) }
	}
}

fn create(_: &str, packet: OwnedTreeInput) -> Result<Result<Item, String>, OpaqueProducerLimit> {
	let provider = ACTIVE.get().ok_or(OpaqueProducerLimit)?;
	let credit = packet
		.remaining_bytes()
		.and_then(|n| n.checked_sub(size_of::<Item>()))
		.ok_or(OpaqueProducerLimit)?;
	let mut value = Created::default();
	// SAFETY: synchronous callback with an initialized, writable output slot.
	let status = unsafe { (provider.create.unwrap())(provider.cookie, credit, &mut value) };
	if status != 0 {
		return Ok(Err("provider create failed".into()));
	}
	let item = Item { provider, value };
	if item.value.kind != 1 || item.value.token == 0 {
		return Ok(Err("invalid provider payload".into()));
	}
	let peak = item
		.value
		.retained
		.checked_add(item.value.temporary)
		.ok_or(OpaqueProducerLimit)?;
	if peak > credit {
		return Err(OpaqueProducerLimit);
	}
	Ok(Ok(item))
}

pub fn module() -> Result<rune::Module, rune::ContextError> {
	let mut module = rune::Module::with_crate("probe")?;
	module.opaque_type::<Item>(
		|item| size_of::<Item>() + item.value.retained,
		|_, _| Err(VmError::panic("opaque provider item")),
	)?;
	module.file_opaque_result_function(
		"make",
		|_, _, _, _| Err(VmError::panic("requires checked file")),
		|_, _| Err(OpaqueProducerLimit),
		create,
	)?;
	module.opaque_method::<Item, _, _>(
		"read",
		|item: &Item| {
			// SAFETY: the Store holds a shared guard on the live native value
			// for this synchronous call. The callback cannot release its owner.
			unsafe { (item.provider.read.unwrap())(item.provider.cookie, item.value.token) }
		},
		None,
	)?;
	Ok(module)
}
