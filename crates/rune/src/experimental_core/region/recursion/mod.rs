//! N5 function-entry admission. Every intraprocedural edge is forward; the only
//! admitted cycle is a direct self call. Analysis and runtime scratch are charged.
use super::budget::{BVec, Budget};
use crate::experimental_core::arithmetic::Binary;
use crate::experimental_core::code::{Code, Op};
use crate::experimental_core::value::Value;
use std::vec::Vec;

pub(crate) const MAX_SLOTS: usize = 64;
pub(crate) const MAX_INSTRUCTIONS: usize = 1024;
/// Bounds type alternatives as well as code/metadata. Not a recursion depth limit.
const MAX_PARAMETERS: usize = 4;

pub(crate) struct Shape {
	pub(crate) function: u32,
	pub(crate) slots: usize,
	pub(crate) parameters: Vec<u32>,
	pub(crate) result: u32,
	pub(crate) ops: Vec<Op>,
	pub(crate) charged: Vec<bool>,
	/// Zero means unreachable or a conflicting join; scalar tags are 1/2/3.
	pub(crate) types: Vec<u32>,
	pub(crate) args: Vec<u32>,
	pub(crate) reached: Vec<bool>,
}

pub(crate) fn tag(v: Value) -> Option<(u32, i64)> {
	match v {
		Value::I64(n) => Some((1, n)),
		Value::Bool(b) => Some((2, i64::from(b))),
		Value::Unit => Some((3, 0)),
		_ => None,
	}
}
pub(crate) fn scalar(t: u32, n: i64) -> Option<Value> {
	match t {
		1 => Some(Value::I64(n)),
		2 if n == 0 || n == 1 => Some(Value::Bool(n != 0)),
		3 if n == 0 => Some(Value::Unit),
		_ => None,
	}
}

impl Shape {
	pub(crate) fn valid_layout(&self) -> bool {
		self.slots > 0
			&& self.slots <= MAX_SLOTS
			&& !self.ops.is_empty()
			&& self.ops.len() <= MAX_INSTRUCTIONS
			&& self.parameters.len() <= self.slots
			&& self.parameters.iter().all(|t| (1..=3).contains(t))
			&& (1..=3).contains(&self.result)
			&& self.charged.len() == self.ops.len()
			&& self.reached.len() == self.ops.len()
			&& self.types.len() == self.slots * self.ops.len()
	}
	pub(crate) fn at(&self, pc: usize) -> &[u32] {
		&self.types[pc * self.slots..(pc + 1) * self.slots]
	}
	pub(crate) fn reached(&self, pc: usize) -> bool {
		self.reached.get(pc).copied().unwrap_or(false)
	}
	pub(crate) fn metadata(&self) -> usize {
		self.parameters.capacity() * 4
			+ self.ops.capacity() * core::mem::size_of::<Op>()
			+ self.charged.capacity()
			+ self.types.capacity() * 4
			+ self.args.capacity() * 4
			+ self.reached.capacity()
	}
}

/// Conservative, bounded monomorphic alternatives. A failed alternative owns no
/// retained buffers. Every read is proven in the final acyclic flow, including
/// recursive arguments and every return; entry checks the parameter tags.
pub(crate) fn analyze(code: &Code, id: u32, budget: &Budget) -> Option<(Shape, usize)> {
	let f = code.function(id)?;
	let n = f.slots as usize;
	let p = f.parameters as usize;
	if n == 0
		|| n > MAX_SLOTS
		|| f.len() == 0
		|| f.len() > MAX_INSTRUCTIONS
		|| p > MAX_PARAMETERS
		|| !f.cached_ref(0)?.charged
	{
		return None;
	}
	// Avoid allocating alternatives for the overwhelmingly common nonrecursive case.
	if !(0..f.len()).any(
		|pc| matches!(f.cached_ref(pc).map(|i|i.op),Some(Op::Call{function,..}) if function==id),
	) {
		return None;
	}
	let alternatives = 3usize.pow((p + 1) as u32);
	for mut bits in 0..alternatives {
		let result = (bits % 3 + 1) as u32;
		bits /= 3;
		let mut parameters = BVec::new(budget);
		for _ in 0..p {
			parameters.push((bits % 3 + 1) as u32).ok()?;
			bits /= 3;
		}
		let mut types = BVec::new(budget);
		types.resize(f.len().checked_mul(n)?, 0u32).ok()?;
		let mut reached = BVec::new(budget);
		reached.resize(f.len(), false).ok()?;
		reached[0] = true;
		for j in 0..n {
			types[j] = if j < p { parameters[j] } else { 3 };
		}
		let mut admitted = true;
		let mut self_call = false;
		let mut returns = 0;
		for pc in 0..f.len() {
			if !reached[pc] {
				continue;
			}
			let mut state = [0u32; MAX_SLOTS];
			state[..n].copy_from_slice(&types[pc * n..(pc + 1) * n]);
			let mut edges = [Some(pc + 1), None];
			let valid = match f.cached_ref(pc)?.op {
				Op::Literal { out, value } => match tag(value) {
					Some((t, _)) => {
						state[out as usize] = t;
						true
					}
					None => false,
				},
				Op::Copy { out, value } => {
					state[out as usize] = state[value as usize];
					state[out as usize] != 0
				}
				Op::Release { value } => {
					state[value as usize] = 3;
					true
				}
				Op::Binary { out, op, lhs, rhs } | Op::AssignBinary { out, op, lhs, rhs } => {
					let ok = state[lhs as usize] == 1 && state[rhs as usize] == 1;
					state[out as usize] = match op {
						Binary::Add | Binary::Sub | Binary::Mul | Binary::Div | Binary::Rem => 1,
						Binary::Eq
						| Binary::Ne
						| Binary::Lt
						| Binary::Le
						| Binary::Gt
						| Binary::Ge => 2,
						_ => 0,
					};
					ok && state[out as usize] != 0
				}
				Op::Branch { condition, yes, no } => {
					edges = [Some(yes as usize), Some(no as usize)];
					state[condition as usize] == 2
				}
				Op::Jump { target } => {
					edges = [Some(target as usize), None];
					true
				}
				Op::Call {
					out,
					function,
					args_start,
					args_len,
				} => {
					self_call = true;
					let a = args_start as usize;
					let ok = function == id
						&& args_len as usize == p
						&& f.args.get(a..a + p).is_some_and(|args| {
							args.iter()
								.enumerate()
								.all(|(j, &slot)| state[slot as usize] == parameters[j])
						});
					state[out as usize] = result;
					ok
				}
				Op::Return { value } => {
					edges = [None, None];
					returns += 1;
					state[value as usize] == result
				}
				_ => false,
			};
			if !valid {
				admitted = false;
				break;
			}
			for next in edges.into_iter().flatten() {
				if next <= pc || next >= f.len() {
					admitted = false;
					break;
				}
				for j in 0..n {
					let cell = &mut types[next * n + j];
					*cell = if !reached[next] || *cell == state[j] {
						state[j]
					} else {
						0
					};
				}
				reached[next] = true;
			}
			if !admitted {
				break;
			}
		}
		if !admitted || !self_call || returns == 0 {
			continue;
		}
		let mut ops = BVec::new(budget);
		let mut charged = BVec::new(budget);
		let mut args = BVec::new(budget);
		for pc in 0..f.len() {
			let i = f.cached_ref(pc)?;
			ops.push(i.op).ok()?;
			charged.push(i.charged).ok()?;
		}
		for &a in &f.args {
			args.push(a).ok()?;
		}
		// Retained buffers are charged to cache metadata by the loading transaction.
		let (parameters, pb) = parameters.into_parts();
		let (ops, ob) = ops.into_parts();
		let (charged, cb) = charged.into_parts();
		let (types, tb) = types.into_parts();
		let (args, ab) = args.into_parts();
		let (reached, rb) = reached.into_parts();
		return Some((
			Shape {
				function: id,
				slots: n,
				// Native metadata belongs to the independently bounded code
				// cache, not the script allocator quota. Conversion is zero-copy
				// and refunds that quota before the interpreter can resume.
				parameters: parameters.into_std(),
				result,
				ops: ops.into_std(),
				charged: charged.into_std(),
				types: types.into_std(),
				args: args.into_std(),
				reached: reached.into_std(),
			},
			pb + ob + cb + tb + ab + rb,
		));
	}
	None
}

/// N5's separate record. The byte layout deliberately uses the existing audited
/// field widths, while the typed boundary never accepts a loop ExitRecord here.
#[derive(Debug)]
#[repr(C)]
pub(crate) struct Record {
	pub(crate) fuel: u64,
	pub(crate) calls: u64,
	pub(crate) work: u64,
	pub(crate) status: u32,
	pub(crate) unused: u32,
	pub(crate) pc: u32,
	pub(crate) reserved: u32,
	pub(crate) peak: u64,
}
impl Record {
	pub(crate) fn new(fuel: usize) -> Self {
		Self {
			fuel: fuel as u64,
			calls: 0,
			work: 0,
			status: 0,
			unused: 0,
			pc: 0,
			reserved: 0,
			peak: 0,
		}
	}
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
	Refused,
	Replay {
		work: u64,
	},
	Complete {
		pc: usize,
		fuel: usize,
		calls: u64,
		peak: u64,
		work: u64,
	},
	Invalid,
}

/// Transactional entry: no engine state or fuel changes until the entire record
/// and every final scalar have passed validation. No fallible operation follows.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enter(
	shape: &Shape,
	window: &mut [Value],
	store: &crate::experimental_core::heap::Store,
	canceled: bool,
	fuel: usize,
	room: super::commit::Room,
	scratch_bytes: usize,
	run: impl FnOnce(&mut [i64], &mut Record, &mut [i64]) -> Option<u32>,
) -> Outcome {
	if !shape.valid_layout() {
		return Outcome::Invalid;
	}
	if canceled
		|| !store.scalar_release_is_noop()
		|| window.len() != shape.slots
		|| (fuel as u64) > u64::MAX / (shape.ops.len() as u64 + 1)
	{
		return Outcome::Refused;
	}
	let mut initial = [0i64; MAX_SLOTS * 2];
	for (j, &v) in window.iter().enumerate() {
		let Some((t, v)) = tag(v) else {
			return Outcome::Refused;
		};
		initial[j] = v;
		initial[shape.slots + j] = t as i64;
	}
	if shape
		.parameters
		.iter()
		.enumerate()
		.any(|(j, &t)| initial[shape.slots + j] != t as i64)
	{
		return Outcome::Refused;
	}
	if window[shape.parameters.len()..]
		.iter()
		.any(|&v| v != Value::Unit)
	{
		return Outcome::Refused;
	}
	let maxdepth = room.frames.min(room.slots / shape.slots);
	let stride = 2 * shape.slots + 1;
	let Some(cells) = maxdepth
		.checked_add(1)
		.and_then(|n| n.checked_mul(stride))
		.filter(|&n| n <= isize::MAX as usize / 8)
	else {
		return Outcome::Refused;
	};
	let budget = Budget::new(scratch_bytes);
	let mut scratch = BVec::new(&budget);
	if scratch.resize(cells, 0i64).is_err() {
		return Outcome::Refused;
	}
	scratch[..2 * shape.slots].copy_from_slice(&initial[..2 * shape.slots]);
	let mut control = [maxdepth as i64];
	let mut record = Record::new(fuel);
	let Some(status) = run(&mut control, &mut record, &mut scratch) else {
		return Outcome::Refused;
	};
	if record.reserved != 0
		|| record.unused != 0
		|| record.work > (fuel as u64 + 1).saturating_mul(shape.ops.len() as u64)
	{
		return Outcome::Invalid;
	}
	if status == 2
		&& record.status == 2
		&& record.fuel == fuel as u64
		&& record.calls == 0
		&& record.peak == 0
		&& record.pc == 0
	{
		return Outcome::Replay { work: record.work };
	}
	let pc = record.pc as usize;
	if status != 1
		|| record.status != 1
		|| record.fuel >= fuel as u64
		|| record.calls >= fuel as u64
		|| record.peak as usize > maxdepth
		|| record.peak > record.calls
		|| !shape.reached(pc)
		|| !matches!(shape.ops.get(pc), Some(Op::Return { .. }))
	{
		return Outcome::Invalid;
	}
	let mut output = [Value::Unit; MAX_SLOTS];
	for j in 0..shape.slots {
		let Ok(t) = u32::try_from(scratch[shape.slots + j]) else {
			return Outcome::Invalid;
		};
		let Some(v) = scalar(t, scratch[j]) else {
			return Outcome::Invalid;
		};
		let expected = shape.at(pc)[j];
		if expected != 0 && t != expected {
			return Outcome::Invalid;
		}
		output[j] = v;
	}
	window.copy_from_slice(&output[..shape.slots]);
	Outcome::Complete {
		pc,
		fuel: record.fuel as usize,
		calls: record.calls,
		peak: record.peak,
		work: record.work,
	}
}

#[cfg(test)]
mod tests;

const _: () = {
	assert!(core::mem::size_of::<Record>() == 48);
	assert!(core::mem::align_of::<Record>() == 8);
	assert!(core::mem::offset_of!(Record, fuel) == 0);
	assert!(core::mem::offset_of!(Record, calls) == 8);
	assert!(core::mem::offset_of!(Record, work) == 16);
	assert!(core::mem::offset_of!(Record, status) == 24);
	assert!(core::mem::offset_of!(Record, unused) == 28);
	assert!(core::mem::offset_of!(Record, pc) == 32);
	assert!(core::mem::offset_of!(Record, reserved) == 36);
	assert!(core::mem::offset_of!(Record, peak) == 40);
};

#[cfg(test)]
std::thread_local! {
	static OBSERVE: core::cell::Cell<Option<(u32,usize)>>=const{core::cell::Cell::new(None)};
	static RETURN_STATE: core::cell::RefCell<Option<(usize,usize,std::vec::Vec<Value>)>>=const{core::cell::RefCell::new(None)};
}
#[cfg(test)]
pub(crate) fn observe_return(id: u32, depth: usize, pc: usize, fuel: usize, window: &[Value]) {
	if OBSERVE.with(|t| t.get() == Some((id, depth))) {
		RETURN_STATE.with(|s| *s.borrow_mut() = Some((pc, fuel, window.to_vec())));
	}
}
#[cfg(test)]
fn capture_return(id: u32, depth: usize) {
	OBSERVE.with(|t| t.set(Some((id, depth))));
	RETURN_STATE.with(|s| s.borrow_mut().take());
}
#[cfg(test)]
fn take_return() -> Option<(usize, usize, std::vec::Vec<Value>)> {
	OBSERVE.with(|t| t.set(None));
	RETURN_STATE.with(|s| s.borrow_mut().take())
}
