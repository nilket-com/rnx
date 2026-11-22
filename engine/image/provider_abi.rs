//! Synchronous wire types shared by the engine and the adapter-side provider.
use core::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Span {
	pub data: *const u8,
	pub len: usize,
}

impl Span {
	pub fn text(s: &str) -> Self {
		Self {
			data: s.as_ptr(),
			len: s.len(),
		}
	}
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Declaration {
	pub name: Span,
	pub id: u32,
	pub shape: u32,
	pub receiver: u32,
	pub argument: u32,
	pub result: u32,
	pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Created {
	pub token: u64,
	pub kind: u64,
	pub retained: usize,
	pub temporary: usize,
}

#[repr(C)]
#[derive(Default)]
pub struct Node {
	pub kind: u32,
	pub frontier: u32,
	pub children: usize,
	pub text: Span,
}

#[repr(C)]
#[derive(Default)]
pub struct Schema {
	pub cookie: *const c_void,
	pub node: Option<unsafe extern "C" fn(*const c_void, usize, *mut Node) -> i32>,
	pub child: Option<unsafe extern "C" fn(*const c_void, usize, usize, *mut usize) -> i32>,
}

#[repr(C)]
#[derive(Default)]
pub struct Input {
	pub receiver: u64,
	pub argument: u64,
	pub credit: usize,
	pub path: Span,
	pub schema: Schema,
	pub names: *const Span,
	pub count: usize,
	pub selection: u32,
	pub dtype: u32,
	pub detail: Span,
}

#[repr(C)]
#[derive(Default)]
pub struct Output {
	pub outcome: u32,
	pub reserved: u32,
	pub payload: Created,
	pub integers: [i64; 3],
	pub strings: *const Span,
	pub count: usize,
}

pub type Emit = unsafe extern "C" fn(*mut c_void, *const Output);
pub type Invoke = unsafe extern "C" fn(*mut c_void, u32, *const Input, *mut c_void, Emit);
pub type Release = unsafe extern "C" fn(*mut c_void, u64);

#[repr(C)]
pub struct Provider {
	pub cookie: *mut c_void,
	pub declarations: *const Declaration,
	pub count: usize,
	pub invoke: Option<Invoke>,
	pub release: Option<Release>,
}

pub const FRAME: u32 = 1;
pub const DENSE: u32 = 2;
pub const TENSOR: u32 = 3;
pub const MLP: u32 = 4;

pub const CSV: u32 = 1;
pub const COLUMNS: u32 = 2;
pub const CONVERT: u32 = 3;
pub const LOAD: u32 = 4;
pub const FORWARD: u32 = 5;
pub const UNARY: u32 = 6;
pub const INTEGER: u32 = 7;
pub const PAIR: u32 = 8;
pub const TRIPLE: u32 = 9;
pub const TEXT: u32 = 10;
pub const NAMES: u32 = 11;
pub const DISPLAY: u32 = 12;

pub const VALUE: u32 = 1;
pub const INNER_ERR: u32 = 2;
pub const EXEC_ERROR: u32 = 3;
pub const LIMIT: u32 = 4;

pub const SCHEMA_OTHER: u32 = 0;
pub const SCHEMA_VECTOR: u32 = 1;
pub const SCHEMA_TUPLE: u32 = 2;
pub const SCHEMA_STRING: u32 = 3;
pub const CSV_PRE_EFFECT_REFUSAL: u32 = 13;

/// Caller-owned, thread-affine observation storage for the controls-only ABI.
#[cfg(feature = "image-controls")]
#[repr(C)]
pub struct Effects {
	pub version: u32,
	pub size: u32,
	pub allocations: core::cell::Cell<u64>,
	pub host_bytes: core::cell::Cell<u64>,
}

#[cfg(feature = "image-controls")]
impl Effects {
	/// # Safety
	/// A non-null aligned pointer must have a readable two-word header and,
	/// for a supported version and size, a live writable known prefix.
	pub unsafe fn checked(p: *mut Self) -> Option<core::ptr::NonNull<Self>> {
		let ptr = core::ptr::NonNull::new(p)?;
		if !p.is_aligned() {
			return None;
		}
		let header = p.cast::<u32>();
		(unsafe { header.read() == 1 && header.add(1).read() as usize >= size_of::<Self>() })
			.then_some(ptr)
	}
}
