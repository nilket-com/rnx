use super::{filled, ColumnRefusal, ColumnType, PreparedColumn, Result};
use crate::experimental_core::{
	arithmetic::{Binary, Unary},
	code::Op,
	frontend::Program,
	value::Value,
};
use std::vec::Vec;

#[derive(Clone, Copy, Debug)]
pub(super) enum Step {
	Input(usize),
	Constant(u64),
	Binary(Binary, usize, usize),
	Not(usize),
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Node {
	pub step: Step,
	pub ty: ColumnType,
	pub pc: usize,
	pub operand: Operand,
}

/// Storage of a node's typed value. Only computed intermediates own slots.
#[derive(Clone, Copy, Debug)]
pub(super) enum Operand {
	Input(usize),
	Constant(u64),
	Slot(usize),
	Output,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Fast {
	I64Add(usize, i64),
	F64Add(usize, f64),
	BoolNot(usize),
	Fused(usize, usize),
	F64PairAdd(usize, usize),
	F64PairSub(usize, usize),
	F64PairMul(usize, usize),
}

fn fast(nodes: &[Node], result: usize) -> Option<Fast> {
	let count = nodes
		.iter()
		.filter(|n| matches!(n.step, Step::Binary(..) | Step::Not(..)))
		.count();
	if count == 1 && nodes[result].ty == ColumnType::F64 {
		if let Step::Binary(op, a, b) = nodes[result].step {
			if let (Step::Input(a), Step::Input(b)) = (nodes[a].step, nodes[b].step) {
				return match op {
					Binary::Add => Some(Fast::F64PairAdd(a, b)),
					Binary::Sub => Some(Fast::F64PairSub(a, b)),
					Binary::Mul => Some(Fast::F64PairMul(a, b)),
					_ => None,
				};
			}
		}
	}
	match nodes[result].step {
		Step::Binary(Binary::Add, a, b) if count == 1 => {
			let (Step::Input(i), Step::Constant(c)) = (nodes[a].step, nodes[b].step) else {
				return None;
			};
			match nodes[result].ty {
				ColumnType::I64 => Some(Fast::I64Add(i, c as i64)),
				ColumnType::F64 => Some(Fast::F64Add(i, f64::from_bits(c))),
				_ => None,
			}
		}
		Step::Not(a) if count == 1 => {
			if let Step::Input(i) = nodes[a].step {
				Some(Fast::BoolNot(i))
			} else {
				None
			}
		}
		Step::Binary(Binary::Add, a, b) if count == 2 && nodes[result].ty == ColumnType::I64 => {
			let (Step::Binary(Binary::Mul, x, c), Step::Input(y)) = (nodes[a].step, nodes[b].step)
			else {
				return None;
			};
			let (Step::Input(x), Step::Constant(2)) = (nodes[x].step, nodes[c].step) else {
				return None;
			};
			Some(Fast::Fused(x, y))
		}
		_ => None,
	}
}

fn binary_type(op: Binary, left: ColumnType, right: ColumnType) -> Result<ColumnType> {
	use Binary::*;
	use ColumnType::*;
	if left != right {
		return Err(ColumnRefusal::Type);
	}
	match (left, op) {
		(I64 | F64, Add | Sub | Mul) => Ok(left),
		(I64 | F64, Eq | Ne | Lt | Le | Gt | Ge) | (Bool, Eq | Ne | BitAnd | BitOr | BitXor) => {
			Ok(Bool)
		}
		_ => Err(ColumnRefusal::Unsupported),
	}
}

pub(super) fn prepare(
	program: &Program,
	function: u32,
	inputs: &[ColumnType],
	output: ColumnType,
	limit: usize,
) -> Result<PreparedColumn> {
	if !program.entry_eligible(function) || program.formatting.guard() != 0 {
		return Err(ColumnRefusal::EntryAuthority);
	}
	if !(1..=3).contains(&inputs.len()) {
		return Err(ColumnRefusal::Bounds);
	}
	let f = program
		.code
		.function(function)
		.ok_or(ColumnRefusal::Bounds)?;
	if f.parameters as usize != inputs.len() {
		return Err(ColumnRefusal::Bounds);
	}
	// Bound temporary verification storage before requesting it. Actual capacities
	// are checked again; the limit includes the plan while temporary slots live.
	let shell = core::mem::size_of::<PreparedColumn>();
	let mut slots = filled(f.slots as usize, None, limit.saturating_sub(shell))?;
	let temporary = slots
		.capacity()
		.checked_mul(core::mem::size_of::<Option<usize>>())
		.ok_or(ColumnRefusal::Memory)?;
	let node_max = f
		.code
		.len()
		.checked_add(inputs.len())
		.ok_or(ColumnRefusal::Memory)?;
	let room = limit
		.checked_sub(shell)
		.and_then(|v| v.checked_sub(temporary))
		.ok_or(ColumnRefusal::Memory)?;
	if node_max
		.checked_mul(core::mem::size_of::<Node>())
		.is_none_or(|v| v > room)
	{
		return Err(ColumnRefusal::Memory);
	}
	let mut nodes = Vec::new();
	nodes
		.try_reserve_exact(node_max)
		.map_err(|_| ColumnRefusal::Memory)?;
	let bytes = nodes
		.capacity()
		.checked_mul(core::mem::size_of::<Node>())
		.and_then(|v| v.checked_add(shell))
		.ok_or(ColumnRefusal::Memory)?;
	if bytes.checked_add(temporary).is_none_or(|v| v > limit) {
		return Err(ColumnRefusal::Memory);
	}
	for (i, &ty) in inputs.iter().enumerate() {
		slots[i] = Some(nodes.len());
		nodes.push(Node {
			step: Step::Input(i),
			ty,
			pc: 0,
			operand: Operand::Input(i),
		});
	}
	let mut row_charge = 0usize;
	let mut result = None;
	for (pc, instruction) in f.code.iter().enumerate() {
		row_charge += usize::from(instruction.is_charged());
		let get = |slot: u32| {
			slots
				.get(slot as usize)
				.copied()
				.flatten()
				.ok_or(ColumnRefusal::Type)
		};
		let (out, step, ty) = match f.operation(pc).map_err(|_| ColumnRefusal::Unsupported)? {
			Op::Literal {
				out,
				value: Value::I64(v),
			} => (out, Step::Constant(v as u64), ColumnType::I64),
			Op::Literal {
				out,
				value: Value::F64(v),
			} => (out, Step::Constant(v.to_bits()), ColumnType::F64),
			Op::Literal {
				out,
				value: Value::Bool(v),
			} => (out, Step::Constant(u64::from(v)), ColumnType::Bool),
			Op::Literal {
				out,
				value: Value::Unit,
			} => {
				slots[out as usize] = None;
				continue;
			}
			Op::Copy { out, value } => {
				slots[out as usize] = Some(get(value)?);
				continue;
			}
			Op::Release { value } => {
				slots[value as usize] = None;
				continue;
			}
			Op::Unary {
				out,
				op: Unary::Not,
				value,
			} => {
				let value = get(value)?;
				if nodes[value].ty != ColumnType::Bool {
					return Err(ColumnRefusal::Type);
				}
				(out, Step::Not(value), ColumnType::Bool)
			}
			Op::Binary { out, op, lhs, rhs } => {
				let lhs = get(lhs)?;
				let rhs = get(rhs)?;
				(
					out,
					Step::Binary(op, lhs, rhs),
					binary_type(op, nodes[lhs].ty, nodes[rhs].ty)?,
				)
			}
			Op::Return { value } => {
				result = Some(get(value)?);
				break;
			}
			_ => return Err(ColumnRefusal::Unsupported),
		};
		// Fold only successful constants. Overflowing constant operations retain
		// their original node/PC and execute normally so replay keeps fault order.
		// row_charge above still counts the original instructions.
		let step = match step {
			Step::Binary(op, a, b) => match (nodes[a].step, nodes[b].step) {
				(Step::Constant(x), Step::Constant(y)) => {
					let (value, failed) = super::run::binary(op, nodes[a].ty, x, y);
					if failed {
						step
					} else {
						Step::Constant(value)
					}
				}
				_ => step,
			},
			Step::Not(a) => match nodes[a].step {
				Step::Constant(value) => Step::Constant(u64::from(value == 0)),
				_ => step,
			},
			_ => step,
		};

		slots[out as usize] = Some(nodes.len());
		nodes.push(Node {
			step,
			ty,
			pc,
			operand: Operand::Output,
		});
	}
	let result = result.ok_or(ColumnRefusal::Unsupported)?;
	if nodes[result].ty != output {
		return Err(ColumnRefusal::Type);
	}
	// Greedy interval allocation. Inputs stay live through their last use; an
	// output never aliases either input of the primitive producing it.
	let extra = nodes
		.len()
		.checked_mul(2 * core::mem::size_of::<usize>())
		.ok_or(ColumnRefusal::Memory)?;
	let occupied = bytes.checked_add(temporary).ok_or(ColumnRefusal::Memory)?;
	if occupied.checked_add(extra).is_none_or(|v| v > limit) {
		return Err(ColumnRefusal::Memory);
	}
	let mut uses = filled(nodes.len(), 0usize, limit - occupied)?;
	for node in &nodes {
		match node.step {
			Step::Binary(_, a, b) => {
				uses[a] += 1;
				uses[b] += 1;
			}
			Step::Not(a) => uses[a] += 1,
			_ => {}
		}
	}
	uses[result] += 1;
	let used_bytes = uses.capacity() * core::mem::size_of::<usize>();
	let mut free = filled(
		nodes.len(),
		0usize,
		limit
			.checked_sub(occupied + used_bytes)
			.ok_or(ColumnRefusal::Memory)?,
	)?;
	free.clear();
	let mut scratch_count = 0;
	for i in 0..nodes.len() {
		nodes[i].operand = match nodes[i].step {
			Step::Input(column) => Operand::Input(column),
			Step::Constant(value) => Operand::Constant(value),
			_ if i == result => Operand::Output,
			_ => Operand::Slot(free.pop().unwrap_or_else(|| {
				let slot = scratch_count;
				scratch_count += 1;
				slot
			})),
		};
		let deps = match nodes[i].step {
			Step::Binary(_, a, b) => [Some(a), Some(b)],
			Step::Not(a) => [Some(a), None],
			_ => [None, None],
		};
		for dependency in deps.into_iter().flatten() {
			uses[dependency] -= 1;
			if uses[dependency] == 0 {
				if let Operand::Slot(slot) = nodes[dependency].operand {
					free.push(slot);
				}
			}
		}
		if uses[i] == 0 {
			if let Operand::Slot(slot) = nodes[i].operand {
				free.push(slot);
			}
		}
	}
	let mut types = [ColumnType::I64; 3];
	types[..inputs.len()].copy_from_slice(inputs);
	let fast = fast(&nodes, result);
	Ok(PreparedColumn {
		code: program.code.id(),
		function,
		inputs: types,
		arity: inputs.len(),
		output,
		nodes,
		result,
		row_charge,
		bytes,
		limit,
		frame_slots: f.slots as usize,
		scratch_count,
		fast,
	})
}
