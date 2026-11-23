//! Remove the proven dispatch diamond from the private recursive-method program.
use super::*;

struct Slots {
	read: u64,
	write: u64,
}

fn slots(op: Op, args: &[u32]) -> Option<Slots> {
	let bit = |s| 1u64.checked_shl(s);
	let (read, write) = match op {
		Op::Literal { out, .. } => (0, bit(out)?),
		Op::Copy { out, value } | Op::Field { out, value, .. } => (bit(value)?, bit(out)?),
		Op::Release { value } => (0, bit(value)?),
		Op::SetField { value, new, .. } => (bit(value)? | bit(new)?, 0),
		Op::Get { out, value, index } => (bit(value)? | bit(index)?, bit(out)?),
		Op::Set { value, index, new } => (bit(value)? | bit(index)? | bit(new)?, 0),
		Op::Binary { out, lhs, rhs, .. } | Op::AssignBinary { out, lhs, rhs, .. } => {
			(bit(lhs)? | bit(rhs)?, bit(out)?)
		}
		Op::Branch { condition, .. } => (bit(condition)?, 0),
		Op::Jump { .. } => (0, 0),
		Op::Return { value } => (bit(value)?, 0),
		Op::Call {
			out,
			args_start,
			args_len,
			..
		} => {
			let mut read = 0;
			for &slot in
				args.get(args_start as usize..args_start.checked_add(args_len)? as usize)?
			{
				read |= bit(slot)?;
			}
			(read, bit(out)?)
		}
		_ => return None,
	};
	Some(Slots { read, write })
}

pub(super) struct Compact<'a> {
	pub(super) width: usize,
	pub(super) source_pcs: BVec<'a, u32>,
}

// Called after nominal and dataflow validation. A removed scratch definition
// must be dead on every successor, including paths through a loop.
#[allow(clippy::too_many_arguments)]
pub(super) fn project<'a>(
	budget: &'a Budget,
	work: &mut Work<'_>,
	functions: &mut [Function],
	ops: &mut BVec<'_, Op>,
	charged: &mut BVec<'_, bool>,
	reached: &mut BVec<'_, bool>,
	args: &[u32],
	states: &[u128],
	width: usize,
) -> Option<Compact<'a>> {
	if reached.len() != ops.len()
		|| charged.len() != ops.len()
		|| states.len() != ops.len().checked_mul(width)?
	{
		return None;
	}
	let mut map = BVec::with_capacity(budget, ops.len()).ok()?;
	let mut source = BVec::with_capacity(budget, functions.first()?.end).ok()?;
	let mut live = BVec::new(budget);
	live.resize(ops.len(), 0u64).ok()?;
	// Liveness of the nominally reachable path in the original projection.
	loop {
		let mut changed = false;
		for pc in (0..ops.len()).rev() {
			if !*reached.get(pc)? {
				continue;
			}
			work.pay(1)?;
			let op = *ops.get(pc)?;
			let next = match op {
				Op::Return { .. } => 0,
				Op::Branch { yes, no, .. } => *live.get(yes as usize)? | *live.get(no as usize)?,
				Op::Jump { target } => *live.get(target as usize)?,
				_ => *live.get(pc + 1)?,
			};
			let uses = match op {
				Op::Is { out, value, .. } => Slots {
					read: 1u64.checked_shl(value)?,
					write: 1u64.checked_shl(out)?,
				},
				op => slots(op, args)?,
			};
			let entry = uses.read | (next & !uses.write);
			let previous = live.get_mut(pc)?;
			changed |= entry != *previous;
			*previous = entry;
		}
		if !changed {
			break;
		}
	}
	let mut count = 0usize;
	let mut native_width = 0usize;
	for f in functions.iter_mut() {
		let (start, end) = (f.start, f.end);
		if start != map.len() || start >= end || end > ops.len() {
			return None;
		}
		native_width = native_width.max(f.parameters);
		let mut pc = start;
		f.start = count;
		while pc < end {
			work.pay(1)?;
			let first = pc;
			let mut call = pc;
			let mut following = pc + 1;
			if let Op::Is { out: test, .. } = *ops.get(pc)? {
				// guarded_self_edge validated both arms before dataflow.
				call = pc.checked_add(6)?;
				let Op::Literal {
					out: callee,
					value: Value::Function { .. },
				} = *ops.get(pc + 3)?
				else {
					return None;
				};
				if call >= end
					|| !*reached.get(pc)?
					|| !*reached.get(call)?
					|| reached.get(pc + 2..call)?.iter().any(|&r| r)
				{
					return None;
				}
				let after = call + 1;
				let release = matches!(ops.get(after),Some(Op::Release {value}) if *value==callee);
				following = after + usize::from(release);
				let test_mask = 1u64.checked_shl(test)?;
				let callee_mask = 1u64.checked_shl(callee)?;
				if following >= end
					|| (*live.get(after)? & test_mask) != 0
					|| (release && (*live.get(following)? & callee_mask) != 0)
				{
					return None;
				}
				let uses = slots(*ops.get(call)?, args)?;
				let private = test_mask | callee_mask;
				if uses.read & private != 0 || uses.write & private != 0 {
					return None;
				}
				for slot in [test, callee] {
					let row = states.get(pc * width..(pc + 1) * width)?;
					let before = *row.get(slot as usize)?;
					if before & !(SCALAR | HASH | METHOD) != 0 {
						return None;
					}
				}
				// Only the guard's entry may carry a charge. All incoming
				// edges to its interior were excluded by guarded_self_edge.
				if charged.get(pc + 1..following)?.iter().any(|&c| c) {
					return None;
				}
			}
			for _ in first..=call {
				map.push(count as u32).ok()?;
			}
			let op = *ops.get(call)?;
			let uses = slots(op, args)?;
			native_width =
				native_width.max((u64::BITS - (uses.read | uses.write).leading_zeros()) as usize);
			*ops.get_mut(count)? = op;
			*charged.get_mut(count)? = *charged.get(first)?;
			*reached.get_mut(count)? = *reached.get(call)?;
			if start == 0 {
				source.push((call - start) as u32).ok()?;
			}
			count += 1;
			for _ in call + 1..following {
				map.push(count as u32).ok()?;
			}
			pc = following;
		}
		f.end = count;
	}
	while ops.len() > count {
		ops.pop();
		charged.pop();
		reached.pop();
	}
	for op in ops.iter_mut() {
		match op {
			Op::Jump { target }
			| Op::Call {
				function: target, ..
			}
			| Op::Literal {
				value: Value::Function { id: target, .. },
				..
			} => *target = *map.get(*target as usize)?,
			Op::Branch { yes, no, .. } => {
				*yes = *map.get(*yes as usize)?;
				*no = *map.get(*no as usize)?;
			}
			_ => (),
		}
	}
	Some(Compact {
		width: native_width,
		source_pcs: source,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn method_closure_truncated_guard_and_invalid_operand_decline() {
		let budget = Budget::new(1 << 20);
		for receiver in [0, u32::MAX] {
			let mut ops = BVec::new(&budget);
			ops.push(Op::Is {
				out: 1,
				value: receiver,
				tag: Type::I64,
				negate: false,
			})
			.unwrap();
			ops.push(Op::Return { value: 0 }).unwrap();
			let mut charged = BVec::new(&budget);
			let mut reached = BVec::new(&budget);
			charged.resize(2, false).unwrap();
			reached.resize(2, true).unwrap();
			let f = Function {
				id: 0,
				start: 0,
				end: 2,
				slots: 2,
				parameters: 1,
			};
			assert!(project(
				&budget,
				&mut Work(&mut 64),
				&mut [f],
				&mut ops,
				&mut charged,
				&mut reached,
				&[],
				&[0; 4],
				2
			)
			.is_none());
		}
	}
}
