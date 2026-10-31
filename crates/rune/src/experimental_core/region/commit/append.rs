//! N4's separate entry path. The original scalar, range, N2 and N3 glue is unchanged.
use super::*;
use crate::experimental_core::region::budget::BVec;
use crate::experimental_core::region::budget::Budget;

#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn enter(
	region: &Region,
	window: &mut [Value],
	store: &mut Store,
	cancel_pending: bool,
	fuel: usize,
	room: Room,
	run: impl FnOnce(&Region, &mut [i64], &mut ExitRecord, &mut [i64], usize) -> u32,
) -> Result<core::result::Result<Exited, Refused>> {
	let Some(c) = region.collection else {
		return Ok(Err(Refused::Window));
	};
	let Some(append) = c.append else {
		return Ok(Err(Refused::Window));
	};
	let Some(&value) = window.get(c.slot as usize) else {
		return Ok(Err(Refused::Window));
	};
	// Capture before shadow/snapshot allocations. Native resource overhead may refuse
	// the optimization; it must never enlarge the interpreter's virtual growth room.
	let quota = rune_alloc::limit::get();
	let Some(state) = store.append_state(value, c.ty, append.aliases.count_ones() as usize) else {
		return Ok(Err(Refused::Tag { slot: c.slot }));
	};
	let scratch_limit = c.scratch_bytes.min(store.native_snapshot_room());
	let budget = Budget::new(scratch_limit);
	let Ok(mut shadow) = BVec::with_capacity(&budget, window.len()) else {
		return Ok(Err(Refused::Store));
	};
	for &v in window.iter() {
		if shadow.push(v).is_err() {
			return Ok(Err(Refused::Store));
		}
	}
	for (slot, v) in shadow.iter_mut().enumerate() {
		if slot < 64 && append.aliases & (1u64 << slot) != 0 {
			*v = if *v == value {
				Value::I64(1)
			} else if *v == Value::Unit {
				Value::I64(0)
			} else {
				return Ok(Err(Refused::Tag { slot: slot as u32 }));
			};
		}
	}
	let mut storage = [0i64; MAX_REGION_CELLS];
	let (entry_fuel, iter) = match unbox(
		region,
		&shadow,
		store,
		cancel_pending,
		fuel,
		room,
		&mut storage,
	) {
		Ok(v) => v,
		Err(e) => return Ok(Err(e)),
	};
	let cells = region.cells();
	if cells >= MAX_REGION_CELLS {
		return Ok(Err(Refused::Window));
	}
	let pushes = append.pushes;
	let turns = iter.map_or(fuel, |i| {
		usize::try_from((i.end as i128 - i.next as i128).max(0))
			.unwrap_or(usize::MAX)
			.min(fuel)
	});
	let desired = state.len.saturating_add(turns.saturating_mul(pushes));
	let desired =
		desired.min(scratch_limit.saturating_sub(budget.used()) / core::mem::size_of::<i64>());
	let Some(bound) = store.append_bound(state, desired, quota) else {
		return Ok(Err(Refused::Store));
	};
	let Some(mut snapshot) = store.append_snapshot(state, c.ty, bound, &budget) else {
		return Ok(Err(Refused::Store));
	};
	storage[cells] = state.len as i64;
	let mut record = ExitRecord::new(entry_fuel);
	let returned = run(
		region,
		&mut storage[..cells + 1],
		&mut record,
		&mut snapshot,
		bound,
	);
	let mut plan = match validate(
		region,
		&shadow,
		store,
		entry_fuel,
		iter,
		&storage[..cells],
		&record,
		returned,
	) {
		Ok(p) => p,
		Err(e) => return Ok(Err(e)),
	};
	let Ok(len) = usize::try_from(storage[cells]) else {
		return Ok(Err(Refused::InvalidExit));
	};
	if len < state.len
		|| len > bound
		|| len - state.len > (entry_fuel - record.fuel).saturating_mul(pushes as u64) as usize
	{
		return Ok(Err(Refused::InvalidExit));
	}
	let written = plan.written;
	let mut added = 0;
	let mut removed = 0;
	for (l, &slot) in region.slots.iter().enumerate() {
		if written & (1u64 << l) != 0 && slot < 64 && append.aliases & (1u64 << slot) != 0 {
			removed += usize::from(window[slot as usize] == value);
			match (storage[l], plan.units & (1u64 << l) != 0) {
				(1, false) => added += 1,
				(0, true) => (),
				_ => return Ok(Err(Refused::InvalidExit)),
			}
		}
	}
	let Some(prepared) = store.prepare_append(state, c.ty, &snapshot[..len], added, removed) else {
		return Ok(Err(Refused::Store));
	};
	let progress = plan.iter.take();
	// Apply to private scalars first, preserving its bound-window/store proof. No
	// externally visible state changes until all validation and preparation succeed.
	let exited = match apply(plan, &mut shadow, store)? {
		Ok(v) => v,
		Err(e) => return Ok(Err(e)),
	};
	if let Some(i) = progress {
		if commit_next(store, i).is_err() {
			return Ok(Err(Refused::InvalidExit));
		}
	}
	store.publish_append(prepared);
	for (l, &slot) in region.slots.iter().enumerate() {
		if written & (1u64 << l) == 0 {
			continue;
		}
		window[slot as usize] = if slot < 64
			&& append.aliases & (1u64 << slot) != 0
			&& shadow[slot as usize] == Value::I64(1)
		{
			value
		} else {
			shadow[slot as usize]
		};
	}
	Ok(Ok(exited))
}
