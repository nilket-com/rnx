//! Adapter calls are synchronous and thread-affine. The provider owns every
//! native object; the image owns a release obligation for each adopted token.
use crate::adapter_inputs;
use crate::provider_abi as abi;
use rnx::rune::{
	self, Module, Value,
	host::{OpaqueProducerLimit as Limit, OutputCertificate},
	runtime::{Formatter, VmError},
};
use std::{
	cell::Cell,
	collections::{HashMap, HashSet},
	ffi::c_void,
	slice, str,
};

// Refusal-only wire declarations are supplied by the generated C registry.
const SESSION_FACTORY: u32 = 14;
const SESSION_FALLIBLE_READ: u32 = 15;
const RESOLUTION_ONLY: u32 = 16;
const SESSION_ERROR_TEXT: u32 = 17;
const POLARS_ERROR: u32 = 5;

#[derive(Clone, Copy)]
pub(crate) struct Callbacks {
	cookie: *mut c_void,
	invoke: abi::Invoke,
	release: abi::Release,
	display: [u32; 3],
}

thread_local! { static ACTIVE: Cell<Option<Callbacks>> = const { Cell::new(None) }; }

impl Callbacks {
	pub fn during<T>(self, f: impl FnOnce() -> T) -> T {
		struct Reset;
		impl Drop for Reset {
			fn drop(&mut self) {
				ACTIVE.set(None);
			}
		}
		FAULT.take();
		ACTIVE.set(Some(self));
		let _reset = Reset;
		f()
	}
}

struct Payload {
	owner: Callbacks,
	value: abi::Created,
}
impl Drop for Payload {
	fn drop(&mut self) {
		// SAFETY: the owner outlives every checked invocation and token.
		unsafe { (self.owner.release)(self.owner.cookie, self.value.token) }
	}
}

macro_rules! native {
	($name:ident,$module:ident,$kind:ident) => {
		#[derive(rune::Any)]
		#[rune(module = rnx::rune, item = ::$module)]
		pub(super) struct $name(pub(super) Payload);
		impl Native for $name {
			const KIND: u32 = abi::$kind;
			fn new(payload: Payload) -> Self {
				Self(payload)
			}
			fn payload(&self) -> &Payload {
				&self.0
			}
		}
	};
}
trait Native: rune::Any {
	const KIND: u32;
	fn new(p: Payload) -> Self;
	fn payload(&self) -> &Payload;
	fn bytes(&self) -> usize
	where
		Self: Sized,
	{
		self.payload()
			.value
			.retained
			.saturating_add(rune::runtime::AnyObj::owned_allocation_bytes::<Self>())
	}
}
use native_types::{DataFrame, Dense, Mlp, Tensor};
#[allow(
	unsafe_op_in_unsafe_fn,
	reason = "Rune's Any derive generates unsafe reference conversions"
)]
mod native_types {
	use super::*;
	native!(Mlp, candle, MLP);
	native!(Tensor, candle, TENSOR);
	native!(Dense, interchange, DENSE);
	native!(DataFrame, polars, FRAME);
}
struct Answer {
	tag: u32,
	payload: Option<Payload>,
	ints: [i64; 3],
	strings: Vec<String>,
}
struct Sink {
	owner: Callbacks,
	answer: Option<Result<Answer, String>>,
}

// SAFETY: the ABI caller warrants the storage for every nonempty span is live
// and readable for the borrow. Validate lengths and alignment before slicing.
unsafe fn span<'a, T>(ptr: *const T, len: usize) -> Result<&'a [T], String> {
	if len == 0 {
		return Ok(&[]);
	}
	if ptr.is_null() || !ptr.is_aligned() || len > isize::MAX as usize / size_of::<T>() {
		return Err("invalid provider span".into());
	}
	Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

unsafe extern "C" fn receive(cookie: *mut c_void, output: *const abi::Output) {
	// SAFETY: invoke receives this stack-owned sink only during call().
	let sink = unsafe { &mut *cookie.cast::<Sink>() };
	let result = (|| {
		if output.is_null() || !output.is_aligned() {
			return Err("invalid provider output".into());
		}
		let output = unsafe { &*output };
		let payload = (output.outcome == 1 && output.payload.token != 0)
			.then_some(output.payload)
			.map(|value| Payload {
				owner: sink.owner,
				value,
			});
		if (output.outcome != abi::VALUE && (output.count > 1 || output.payload.token != 0))
			|| sink.answer.is_some()
			|| output.reserved != 0
			|| !matches!(
				output.outcome,
				abi::VALUE | abi::INNER_ERR | abi::EXEC_ERROR | abi::LIMIT
			) {
			return Err("invalid or repeated provider output".into());
		}
		if output.count > 4096 {
			return Err("invalid provider string count".into());
		}
		let spans = unsafe { span(output.strings, output.count)? };
		let mut strings = Vec::new();
		strings
			.try_reserve_exact(spans.len())
			.map_err(|_| "provider output allocation")?;
		for s in spans {
			if output.outcome != abi::VALUE && s.len > 262144 {
				return Err("provider diagnostic exceeds byte limit".into());
			}
			let bytes = unsafe { span(s.data, s.len)? };
			let text = str::from_utf8(bytes).map_err(|_| "invalid provider UTF-8")?;
			let mut copy = String::new();
			copy.try_reserve_exact(text.len())
				.map_err(|_| "provider output allocation")?;
			copy.push_str(text);
			strings.push(copy);
		}
		Ok(Answer {
			tag: output.outcome,
			payload,
			ints: output.integers,
			strings,
		})
	})();
	sink.answer = Some(result);
}

fn call(id: u32, input: &abi::Input) -> Result<Answer, String> {
	let owner = ACTIVE.get().ok_or("provider outside checked invocation")?;
	let mut sink = Sink {
		owner,
		answer: None,
	};
	// SAFETY: all spans in input and the sink remain live for this synchronous call.
	unsafe { (owner.invoke)(owner.cookie, id, input, (&raw mut sink).cast(), receive) };
	sink.answer.ok_or("provider did not return an output")?
}

fn invalid(message: impl std::fmt::Display) -> VmError {
	VmError::panic(message.to_string())
}
impl Answer {
	fn text(mut self) -> Result<String, VmError> {
		if self.strings.len() != 1 || self.payload.is_some() {
			return Err(invalid("invalid provider text"));
		}
		Ok(self.strings.remove(0))
	}
	fn success(self) -> Result<Self, VmError> {
		if self.tag == abi::VALUE {
			return Ok(self);
		}
		if self.tag == abi::LIMIT {
			return Err(invalid("provider credit exhausted"));
		}
		Err(invalid(self.text()?))
	}
	fn owned<T: Native>(mut self, credit: usize) -> Result<Result<T, String>, Failure> {
		match self.tag {
			abi::INNER_ERR => return Ok(Err(self.text().map_err(Failure::Terminal)?)),
			abi::LIMIT => return Err(Failure::Limit),
			abi::VALUE => (),
			_ => {
				return Err(Failure::Terminal(invalid(
					self.text().map_err(Failure::Terminal)?,
				)));
			}
		}
		let p = self
			.payload
			.take()
			.ok_or_else(|| Failure::Terminal(invalid("missing native token")))?;
		if p.value.kind != u64::from(T::KIND) || !self.strings.is_empty() {
			return Err(Failure::Terminal(invalid("wrong native result kind")));
		}
		if p.value
			.retained
			.checked_add(p.value.temporary)
			.is_none_or(|sum| sum > credit)
		{
			return Err(Failure::Limit);
		}
		Ok(Ok(T::new(p)))
	}
}
enum Failure {
	Limit,
	Terminal(VmError),
}
type Produced<T> = Result<Result<T, String>, Limit>;
thread_local! {static FAULT:std::cell::RefCell<Option<VmError>>=const{std::cell::RefCell::new(None)};}
pub(crate) fn take_fault() -> Option<VmError> {
	FAULT.take()
}
fn produce<T: Native>(id: u32, input: &abi::Input) -> Produced<T> {
	// The existing producer bridge stops immediately on its terminal limit
	// channel. Preserve a provider protocol error for the image report.
	match call(id, input)
		.map_err(|s| Failure::Terminal(invalid(s)))
		.and_then(|a| a.owned(input.credit))
	{
		Ok(v) => Ok(v),
		Err(Failure::Limit) => Err(Limit),
		Err(Failure::Terminal(e)) => {
			FAULT.replace(Some(e));
			Err(Limit)
		}
	}
}
fn read(id: u32, token: u64) -> Result<Answer, VmError> {
	let a = call(
		id,
		&abi::Input {
			receiver: token,
			..Default::default()
		},
	)
	.map_err(invalid)?
	.success()?;
	if a.payload.is_some() {
		return Err(invalid("extra native payload in reader"));
	}
	Ok(a)
}
fn display<T: Native>(value: &T, f: &mut Formatter) -> Result<(), VmError> {
	use rune::alloc::fmt::TryWrite;
	let p = value.payload();
	f.try_write_str(&read(p.owner.display[(T::KIND - 1) as usize], p.value.token)?.text()?)?;
	Ok(())
}

#[derive(rune::Any)]
#[rune(module = rnx::rune,item=::polars)]
struct Error;
struct Declarations {
	ops: HashMap<(u32, u32), u32>,
	unary: Vec<(rune::alloc::String, u32)>,
	inactive: Vec<(u32, String)>,
	panic_csv: bool,
}
impl Declarations {
	fn id(&self, kind: u32, shape: u32) -> Result<u32, String> {
		self.ops
			.get(&(kind, shape))
			.copied()
			.ok_or_else(|| format!("missing adapter declaration kind {kind}, shape {shape}"))
	}
	fn parse(declarations: &[abi::Declaration]) -> Result<Self, String> {
		use abi::*;
		let mut table = Self {
			ops: HashMap::new(),
			unary: vec![],
			inactive: vec![],
			panic_csv: false,
		};
		let mut ids = HashSet::new();
		let mut names = HashSet::new();
		for d in declarations {
			let inactive = matches!(
				d.shape,
				SESSION_FACTORY | SESSION_FALLIBLE_READ | RESOLUTION_ONLY | SESSION_ERROR_TEXT
			);
			if d.reserved != 0
				|| (inactive && d.id != 0)
				|| (!inactive && (d.id == 0 || !ids.insert(d.id)))
			{
				return Err("invalid or duplicate operation ID".into());
			}
			if d.name.len > 1024 {
				return Err("adapter name too long".into());
			}
			let name = str::from_utf8(unsafe { span(d.name.data, d.name.len)? })
				.map_err(|_| "invalid method name")?;
			let valid = |s: &str| {
				s.as_bytes()
					.first()
					.is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
					&& s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
			};
			let valid_name = if inactive {
				name.contains("::") && name.split("::").all(valid)
			} else {
				valid(name)
			};
			if !valid_name || !names.insert((d.receiver, d.result, name)) {
				return Err("invalid or conflicting adapter name".into());
			}
			let kind = d.receiver.max(d.argument).max(d.result);
			let key = match (name, d.shape, d.receiver, d.argument, d.result) {
				("read_csv", CSV | CSV_PRE_EFFECT_REFUSAL, 0, 0, FRAME) => {
					table.panic_csv = d.shape == CSV_PRE_EFFECT_REFUSAL;
					(FRAME, CSV)
				}
				("width", INTEGER, FRAME, 0, 0) => (FRAME, INTEGER),
				("to_dense", COLUMNS, FRAME, 0, DENSE) => (FRAME, COLUMNS),
				("from_dense", CONVERT, 0, DENSE, TENSOR) => (TENSOR, CONVERT),
				("load", LOAD, 0, 0, MLP) => (MLP, LOAD),
				("forward", FORWARD, MLP, TENSOR, TENSOR) => (MLP, FORWARD),
				("rank", INTEGER, TENSOR, 0, 0) => (TENSOR, INTEGER),
				("elem_count", INTEGER, TENSOR, 0, 0) => (TENSOR, COUNT_KEY),
				("shape", PAIR, DENSE, 0, 0)
				| ("dtype", TEXT, DENSE, 0, 0)
				| ("names", NAMES, DENSE, 0, 0)
				| ("dims", TRIPLE, MLP, 0, 0) => (kind, d.shape),
				("display", DISPLAY, FRAME | DENSE | TENSOR, 0, 0) => (kind, DISPLAY),
				(_, SESSION_FACTORY, 0, 0, FRAME)
				| (_, SESSION_FALLIBLE_READ, FRAME, 0, 0)
				| (_, SESSION_ERROR_TEXT, POLARS_ERROR, 0, 0)
				| (_, RESOLUTION_ONLY, 0, 0, 0) => {
					table.inactive.push((d.shape, name.into()));
					continue;
				}
				(_, UNARY, TENSOR, 0, TENSOR) => {
					table.unary.push((
						rune::alloc::String::try_from(name).map_err(|e| e.to_string())?,
						d.id,
					));
					continue;
				}
				_ => return Err("unsupported adapter declaration".into()),
			};
			if table.ops.insert(key, d.id).is_some() {
				return Err("duplicate adapter declaration".into());
			}
		}
		Ok(table)
	}
}
const COUNT_KEY: u32 = u32::MAX;
fn inactive<T>() -> Result<T, String> {
	Err("inactive checked file handler".into())
}

pub(crate) unsafe fn install(
	table: *const abi::Provider,
) -> Result<(Callbacks, Vec<Module>), String> {
	use abi::*;
	if table.is_null() || !table.is_aligned() {
		return Err("invalid adapter table".into());
	}
	let table = unsafe { &*table };
	let dec = Declarations::parse(unsafe { span(table.declarations, table.count)? })?;
	let owner = Callbacks {
		cookie: table.cookie,
		invoke: table.invoke.ok_or("missing invoke")?,
		release: table.release.ok_or("missing release")?,
		display: [
			dec.id(FRAME, DISPLAY)?,
			dec.id(DENSE, DISPLAY)?,
			dec.id(TENSOR, DISPLAY)?,
		],
	};
	let mut p = Module::with_crate("polars").map_err(|e| e.to_string())?;
	let mut x = Module::with_crate("interchange").map_err(|e| e.to_string())?;
	let mut c = Module::with_crate("candle").map_err(|e| e.to_string())?;
	let error = |e: rune::ContextError| e.to_string();
	p.opaque_type::<DataFrame>(Native::bytes, display)
		.map_err(error)?;
	x.opaque_type::<Dense>(Native::bytes, display)
		.map_err(error)?;
	c.opaque_type::<Tensor>(Native::bytes, display)
		.map_err(error)?;
	let csv = dec.id(FRAME, CSV)?;
	let checked_csv = move |path: &str, tree: rune::host::OwnedTreeInput| {
		adapter_inputs::with_schema(path, &tree, |i| produce::<DataFrame>(csv, i))
	};
	let raw = |_: &mut dyn rune::runtime::Memory,
	           _: rune::runtime::Address,
	           _: usize,
	           _: rune::runtime::Output| Err(invalid("inactive CSV reader"));
	if dec.panic_csv {
		p.opaque_result_function("read_csv", raw, checked_csv)
			.map_err(error)?;
	} else {
		p.file_opaque_result_function("read_csv", raw, checked_csv, checked_csv)
			.map_err(error)?;
	}
	let width = dec.id(FRAME, INTEGER)?;
	p.opaque_method::<DataFrame, _, _>(
		"width",
		move |f: &DataFrame| read(width, f.0.value.token).map(|a| a.ints[0]),
		None,
	)
	.map_err(error)?;
	let columns = dec.id(FRAME, COLUMNS)?;
	p.opaque_columns_producer(
		"to_dense",
		|_: &DataFrame, _: Value, _: &str| inactive::<Dense>(),
		move |f: &DataFrame, sel, credit| {
			adapter_inputs::with_columns(f.0.value.token, &sel, credit, |i| {
				produce::<Dense>(columns, i)
			})
		},
	)
	.map_err(error)?;
	let pair = dec.id(DENSE, PAIR)?;
	x.opaque_method::<Dense, _, _>(
		"shape",
		move |d: &Dense| read(pair, d.0.value.token).map(|a| (a.ints[0], a.ints[1])),
		Some(OutputCertificate::I64_PAIR),
	)
	.map_err(error)?;
	let names = dec.id(DENSE, NAMES)?;
	x.opaque_method::<Dense, _, _>(
		"names",
		move |d: &Dense| read(names, d.0.value.token).map(|a| a.strings),
		Some(OutputCertificate::STRING_LIST),
	)
	.map_err(error)?;
	let dtype = dec.id(DENSE, TEXT)?;
	x.opaque_method::<Dense, _, _>(
		"dtype",
		move |d: &Dense| read(dtype, d.0.value.token)?.text(),
		None,
	)
	.map_err(error)?;
	let convert = dec.id(TENSOR, CONVERT)?;
	c.opaque_shared_constructor(
		"from_dense",
		|_: &Dense| inactive::<Tensor>(),
		move |d: &Dense, credit| {
			produce::<Tensor>(
				convert,
				&Input {
					argument: d.0.value.token,
					credit,
					..Default::default()
				},
			)
		},
	)
	.map_err(error)?;
	for (name, id) in [
		("rank", dec.id(TENSOR, INTEGER)?),
		("elem_count", dec.id(TENSOR, COUNT_KEY)?),
	] {
		c.opaque_method::<Tensor, _, _>(
			name,
			move |t: &Tensor| read(id, t.0.value.token).map(|a| a.ints[0]),
			None,
		)
		.map_err(error)?;
	}
	let load = dec.id(MLP, LOAD)?;
	c.checked_file_path_constructor(
		"load",
		move |path: &str| {
			call(
				load,
				&Input {
					path: Span::text(path),
					credit: isize::MAX as usize,
					..Default::default()
				},
			)
			.map_err(invalid)?
			.owned::<Mlp>(isize::MAX as usize)
			.map_err(|e| match e {
				Failure::Limit => invalid("provider credit exhausted"),
				Failure::Terminal(e) => e,
			})
		},
		Mlp::bytes,
	)
	.map_err(error)?;
	let dims = dec.id(MLP, TRIPLE)?;
	c.opaque_method::<Mlp, _, _>(
		"dims",
		move |m: &Mlp| {
			read(dims, m.0.value.token).map(|a| {
				let [i, h, o] = a.ints;
				(i, h, o)
			})
		},
		Some(OutputCertificate::I64_TRIPLE),
	)
	.map_err(error)?;
	let forward = dec.id(MLP, FORWARD)?;
	c.file_pair_producer(
		"forward",
		|_: &Mlp, _: &Tensor| inactive::<Tensor>(),
		move |m: &Mlp, t: &Tensor, credit| {
			produce::<Tensor>(
				forward,
				&Input {
					receiver: m.0.value.token,
					argument: t.0.value.token,
					credit,
					..Default::default()
				},
			)
		},
	)
	.map_err(error)?;
	for (name, id) in dec.unary {
		c.file_unary_with_credit(
			name,
			|_: &Tensor| inactive::<Tensor>(),
			move |t: &Tensor, credit| {
				produce::<Tensor>(
					id,
					&Input {
						receiver: t.0.value.token,
						credit,
						..Default::default()
					},
				)
			},
		)
		.map_err(error)?;
	}
	p.opaque_type::<Error>(|_| 0, |_, _| Err(invalid("unavailable error payload")))
		.map_err(error)?;
	for (shape, path) in dec.inactive {
		let (namespace, rest) = path.split_once("::").ok_or("missing adapter namespace")?;
		let module = match namespace {
			"polars" => &mut p,
			"interchange" => &mut x,
			"candle" => &mut c,
			_ => return Err("unknown adapter namespace".into()),
		};
		let name = rest.rsplit("::").next().ok_or("missing adapter name")?;
		match shape {
			SESSION_FACTORY
				if rest.strip_prefix("DataFrame::") == Some(name) && namespace == "polars" =>
			{
				module
					.inactive_file_factory::<DataFrame>(name)
					.map_err(error)?
			}
			SESSION_FALLIBLE_READ
				if rest.strip_prefix("DataFrame::") == Some(name) && namespace == "polars" =>
			{
				module
					.inactive_file_i64_result::<DataFrame, Error>(name)
					.map_err(error)?
			}
			SESSION_ERROR_TEXT if namespace == "polars" && rest == "Error::__opaque_display" => {}
			SESSION_ERROR_TEXT
				if namespace == "polars" && rest.strip_prefix("Error::") == Some(name) =>
			{
				module.inactive_file_symbol(rest).map_err(error)?
			}
			RESOLUTION_ONLY => module.inactive_file_symbol(rest).map_err(error)?,
			_ => return Err("invalid refusal declaration".into()),
		}
	}
	Ok((owner, vec![p, x, c]))
}
