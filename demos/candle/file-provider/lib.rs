//! Single launcher-side owner for checked file adapters. A token belongs to
//! exactly one provider. Numbers inside Dense and Tensor never cross the ABI.
#[path = "../../../engine/image/provider_abi.rs"]
mod abi;
mod inputs;
use abi::{Declaration, Span};
use rnx::{
	interchange::{self, Dense},
	rune::host::OpaqueProducerLimit,
};
use rnx_candle::{Mlp, Tensor, image_provider as candle};
use rnx_polars::{DataFrame, image_provider as polars};
use std::{
	cell::RefCell,
	collections::HashMap,
	ffi::c_void,
	panic::{AssertUnwindSafe, catch_unwind},
	sync::atomic::{AtomicU64, Ordering},
};

enum Value {
	Frame(DataFrame),
	Dense(Dense),
	Tensor(Tensor),
	Model(Mlp),
}
impl Value {
	fn kind(&self) -> u32 {
		match self {
			Self::Frame(_) => abi::FRAME,
			Self::Dense(_) => abi::DENSE,
			Self::Tensor(_) => abi::TENSOR,
			Self::Model(_) => abi::MLP,
		}
	}
	fn backing_bytes(&self) -> usize {
		use rnx::rune::runtime::AnyObj;
		let (stock, wrapper) = match self {
			Self::Frame(x) => (polars::retained(x), size_of::<DataFrame>()),
			Self::Dense(x) => (interchange::logical_bytes(x), size_of::<Dense>()),
			Self::Tensor(x) => (
				candle::tensor_bytes(x),
				AnyObj::owned_allocation_bytes::<Tensor>(),
			),
			Self::Model(x) => (candle::retained(x), AnyObj::owned_allocation_bytes::<Mlp>()),
		};
		// Preserve the stock estimator's overflow sentinel.
		if stock == usize::MAX {
			stock
		} else {
			stock.saturating_sub(wrapper)
		}
	}
}
#[derive(Clone, Copy)]
enum Operation {
	Csv,
	Width,
	Columns,
	Shape,
	Names,
	Dtype,
	Convert,
	Rank,
	Count,
	Load,
	Dims,
	Forward,
	Unary(candle::Unary),
	Display(u32),
}
struct Owner {
	values: RefCell<HashMap<u64, Box<Value>>>,
	operations: Vec<Operation>,
	declarations: Vec<Declaration>,
}
static NEXT: AtomicU64 = AtomicU64::new(1);
impl Owner {
	fn new() -> Self {
		use abi::{DENSE, FRAME, MLP, TENSOR};
		let mut owner = Self {
			values: RefCell::new(HashMap::new()),
			operations: Vec::new(),
			declarations: Vec::new(),
		};
		let mut add = |name, shape, receiver, argument, result, op| {
			owner.operations.push(op);
			owner.declarations.push(Declaration {
				name: Span::text(name),
				id: owner.operations.len() as u32,
				shape,
				receiver,
				argument,
				result,
				reserved: 0,
			});
		};
		let panic =
			std::env::var_os("POLARS_PANIC_ON_ERR").as_deref() == Some(std::ffi::OsStr::new("1"));
		add("read_csv", abi::CSV, 0, 0, FRAME, Operation::Csv);
		add("width", abi::INTEGER, FRAME, 0, 0, Operation::Width);
		add(
			"to_dense",
			abi::COLUMNS,
			FRAME,
			0,
			DENSE,
			Operation::Columns,
		);
		add("shape", abi::PAIR, DENSE, 0, 0, Operation::Shape);
		add("names", abi::NAMES, DENSE, 0, 0, Operation::Names);
		add("dtype", abi::TEXT, DENSE, 0, 0, Operation::Dtype);
		add(
			"from_dense",
			abi::CONVERT,
			0,
			DENSE,
			TENSOR,
			Operation::Convert,
		);
		add("rank", abi::INTEGER, TENSOR, 0, 0, Operation::Rank);
		add("elem_count", abi::INTEGER, TENSOR, 0, 0, Operation::Count);
		add("load", abi::LOAD, 0, 0, MLP, Operation::Load);
		add("dims", abi::TRIPLE, MLP, 0, 0, Operation::Dims);
		add(
			"forward",
			abi::FORWARD,
			MLP,
			TENSOR,
			TENSOR,
			Operation::Forward,
		);
		for kind in [FRAME, DENSE, TENSOR] {
			add(
				"display",
				abi::DISPLAY,
				kind,
				0,
				0,
				Operation::Display(kind),
			);
		}
		for (name, method) in candle::unary() {
			add(
				name,
				abi::UNARY,
				TENSOR,
				0,
				TENSOR,
				Operation::Unary(method),
			);
		}
		if panic {
			for d in &mut owner.declarations {
				if d.shape == abi::CSV {
					d.shape = abi::CSV_PRE_EFFECT_REFUSAL
				}
			}
		}
		owner
	}
}
/// # Safety
/// out points to writable aligned storage for one provider.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_file_provider_open(out: *mut abi::Provider) -> i32 {
	if out.is_null() || !out.is_aligned() {
		return 1;
	}
	let owner = Box::new(Owner::new());
	let declarations = owner.declarations.as_ptr();
	let count = owner.declarations.len();
	unsafe {
		out.write(abi::Provider {
			cookie: Box::into_raw(owner).cast(),
			declarations,
			count,
			invoke: Some(invoke),
			release: Some(release),
		})
	};
	0
}
/// # Safety
/// cookie is a live cookie from open and the engine has released the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rnx_file_provider_close(cookie: *mut c_void) -> i32 {
	if cookie.is_null() {
		return 1;
	}
	let owner = unsafe { &*cookie.cast::<Owner>() };
	if !owner.values.borrow().is_empty() {
		return 1;
	}
	drop(unsafe { Box::from_raw(cookie.cast::<Owner>()) });
	0
}

enum Failure {
	Adapter(String),
	Protocol,
	Limit,
}
impl From<OpaqueProducerLimit> for Failure {
	fn from(_: OpaqueProducerLimit) -> Self {
		Self::Limit
	}
}
impl From<String> for Failure {
	fn from(error: String) -> Self {
		Self::Adapter(error)
	}
}
enum Answer {
	Owned(Value),
	Numbers([i64; 3]),
	Strings(Vec<String>),
}
impl Owner {
	fn perform(&self, id: u32, i: &abi::Input) -> Result<Answer, Failure> {
		use Operation::*;
		let op = self
			.operations
			.get(id.checked_sub(1).ok_or(Failure::Protocol)? as usize)
			.ok_or(Failure::Protocol)?;
		let values = self.values.borrow();
		let get = |token| values.get(&token).map(Box::as_ref).ok_or(Failure::Protocol);
		let frame = || match get(i.receiver)? {
			Value::Frame(v) => Ok(v),
			_ => Err(Failure::Protocol),
		};
		let dense = |token| match get(token)? {
			Value::Dense(v) => Ok(v),
			_ => Err(Failure::Protocol),
		};
		let tensor = |token| match get(token)? {
			Value::Tensor(v) => Ok(v),
			_ => Err(Failure::Protocol),
		};
		let model = || match get(i.receiver)? {
			Value::Model(v) => Ok(v),
			_ => Err(Failure::Protocol),
		};
		let n = |n| Answer::Numbers([n, 0, 0]);
		Ok(match *op {
			Csv => Answer::Owned(Value::Frame(polars::read_csv(
				inputs::text(&i.path)?,
				&inputs::Tree(i),
			)??)),
			Width => n(polars::width(frame()?)),
			Columns => Answer::Owned(Value::Dense(polars::to_dense(
				frame()?,
				inputs::columns(i)?,
				i.credit,
			)??)),
			Shape => {
				let d = dense(i.receiver)?;
				Answer::Numbers([d.rows() as i64, d.columns() as i64, 0])
			}
			Names => Answer::Strings(dense(i.receiver)?.names().to_vec()),
			Dtype => Answer::Strings(vec![dense(i.receiver)?.dtype().into()]),
			Convert => Answer::Owned(Value::Tensor(candle::convert(
				dense(i.argument)?,
				i.credit,
			)??)),
			Rank => n(rnx_candle::tensor_ops::inner(tensor(i.receiver)?).rank() as i64),
			Count => n(rnx_candle::tensor_ops::inner(tensor(i.receiver)?).elem_count() as i64),
			Load => Answer::Owned(Value::Model(candle::load(inputs::text(&i.path)?)?)),
			Dims => {
				let (a, b, c) = candle::dims(model()?);
				Answer::Numbers([a, b, c + i64::from(cfg!(feature = "image-witness"))])
			}
			Forward => Answer::Owned(Value::Tensor(candle::apply(
				model()?,
				tensor(i.argument)?,
				i.credit,
			)??)),
			Unary(op) => Answer::Owned(Value::Tensor(op(tensor(i.receiver)?)?)),
			Display(kind) => Answer::Strings(vec![match kind {
				abi::FRAME => polars::display(frame()?),
				abi::DENSE => interchange::summary(dense(i.receiver)?),
				abi::TENSOR => candle::display(tensor(i.receiver)?),
				_ => return Err(Failure::Protocol),
			}]),
		})
	}
	fn transfer(&self, value: Value) -> Result<abi::Created, Failure> {
		let mut store = self.values.borrow_mut();
		store.try_reserve(1).map_err(|_| Failure::Limit)?;
		let token = NEXT
			.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
			.map_err(|_| Failure::Limit)?;
		let result = abi::Created {
			token,
			kind: value.kind().into(),
			retained: value
				.backing_bytes()
				.saturating_add(size_of::<Value>() + size_of::<u64>() + size_of::<Box<Value>>()),
			temporary: 0,
		};
		store.insert(token, Box::new(value));
		Ok(result)
	}
}
unsafe extern "C" fn invoke(
	cookie: *mut c_void,
	id: u32,
	input: *const abi::Input,
	sink: *mut c_void,
	emit: abi::Emit,
) {
	let mut reply = abi::Output {
		outcome: abi::VALUE,
		..Default::default()
	};
	let result = catch_unwind(AssertUnwindSafe(|| {
		let owner = unsafe { &*cookie.cast::<Owner>() };
		let answer = owner.perform(id, unsafe { &*input })?;
		#[cfg(feature = "image-controls")]
		if let Answer::Owned(Value::Tensor(t)) = &answer
			&& let Some(path) = std::env::var_os("RNX_TEST_ADAPTER_CELLS")
		{
			use std::io::Write;
			let values = rnx_candle::tensor_ops::inner(t)
				.flatten_all()
				.and_then(|t| t.to_vec1::<f32>())
				.map_err(|_| Failure::Protocol)?;
			let mut file = std::fs::OpenOptions::new()
				.create(true)
				.append(true)
				.open(path)
				.map_err(|_| Failure::Protocol)?;
			for n in values {
				write!(file, "{:08x} ", n.to_bits()).map_err(|_| Failure::Protocol)?
			}
			writeln!(file).map_err(|_| Failure::Protocol)?;
		}
		match answer {
			Answer::Owned(v) => reply.payload = owner.transfer(v)?,
			Answer::Numbers(nums) => reply.integers = nums,
			Answer::Strings(text) => return Ok(text),
		}
		Ok(Vec::new())
	}))
	.unwrap_or(Err(Failure::Protocol));
	let text = match result {
		Ok(text) => text,
		Err(Failure::Limit) => {
			reply.outcome = abi::LIMIT;
			vec![]
		}
		Err(Failure::Adapter(error)) => {
			reply.outcome = abi::INNER_ERR;
			vec![error]
		}
		Err(Failure::Protocol) => {
			reply.outcome = abi::EXEC_ERROR;
			vec!["invalid adapter call".into()]
		}
	};
	let spans: Vec<_> = text.iter().map(|s| Span::text(s)).collect();
	reply.strings = spans.as_ptr();
	reply.count = spans.len();
	unsafe { emit(sink, &reply) };
}
unsafe extern "C" fn release(cookie: *mut c_void, token: u64) {
	let owner = unsafe { &*cookie.cast::<Owner>() };
	owner.values.borrow_mut().remove(&token);
}
