//! Safe preparation and publication for N4. No engine address enters generated code.
use super::growth::Growth;
use super::{Payload, Store};
use crate::alloc::Vec;
use crate::experimental_core::region::{
	budget::{BVec, Budget},
	Scalar,
};
use crate::experimental_core::value::{Type, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AppendState {
	pub(crate) value: Value,
	pub(crate) len: usize,
	pub(crate) capacity: usize,
	strong: u32,
}

pub(crate) struct AppendPrepared {
	state: AppendState,
	values: Vec<Value>,
	bytes: usize,
	charged: usize,
	strong: u32,
}

impl Store {
	pub(crate) fn append_state(
		&self,
		value: Value,
		ty: Scalar,
		aliases: usize,
	) -> Option<AppendState> {
		let Value::Heap(h) = value else { return None };
		let e = self.entry(h).ok()?;
		let Payload::List {
			tag: Type::Vec,
			values,
		} = e.payload.as_ref()?
		else {
			return None;
		};
		if e.readers != 0
			|| e.strong
				.checked_add(u32::try_from(aliases).ok()?.checked_add(1)?)
				.is_none()
			|| !self.scalar_release_is_noop()
			|| !values.iter().all(|v| {
				matches!(
					(ty, *v),
					(Scalar::I64, Value::I64(_)) | (Scalar::Bool, Value::Bool(_))
				)
			}) {
			return None;
		}
		Some(AppendState {
			value,
			len: values.len(),
			capacity: values.capacity(),
			strong: e.strong,
		})
	}

	/// Only actual interpreter growth thresholds authorize a native logical length.
	/// `quota` is captured before ANY append preparation allocation.
	pub(crate) fn append_bound(
		&self,
		state: AppendState,
		desired: usize,
		quota: usize,
	) -> Option<usize> {
		let Value::Heap(h) = state.value else {
			return None;
		};
		let mut cap = state.capacity;
		while cap < desired {
			let Ok(g) = Growth::next(cap) else { break };
			if self.replacement_bytes(h, g.bytes).is_err()
				|| g.peak(cap, state.capacity).is_none_or(|peak| peak > quota)
			{
				break;
			}
			cap = g.capacity;
		}
		Some(cap.min(desired))
	}

	pub(crate) fn append_snapshot<'b>(
		&self,
		state: AppendState,
		ty: Scalar,
		bound: usize,
		budget: &'b Budget,
	) -> Option<BVec<'b, i64>> {
		if bound < state.len {
			return None;
		}
		let Value::Heap(h) = state.value else {
			return None;
		};
		let Payload::List { values, .. } = self.payload(h).ok()? else {
			return None;
		};
		let mut out = BVec::with_capacity(budget, bound.max(1)).ok()?;
		out.resize(bound.max(1), 0).ok()?;
		for (dst, v) in out.iter_mut().zip(values.iter()) {
			*dst = match (ty, *v) {
				(Scalar::I64, Value::I64(v)) => v,
				(Scalar::Bool, Value::Bool(v)) => i64::from(v),
				_ => return None,
			};
		}
		Some(out)
	}

	/// Called only after the full record and ownership proof. Failure is an invisible
	/// original-header fallback. Keep scratch alive: it remains actually quota charged.
	pub(crate) fn prepare_append(
		&self,
		state: AppendState,
		ty: Scalar,
		cells: &[i64],
		added_aliases: usize,
		removed_aliases: usize,
	) -> Option<AppendPrepared> {
		if self.append_state(state.value, ty, added_aliases)? != state
			|| cells.len() < state.len
			|| (ty == Scalar::Bool && cells.iter().any(|v| !matches!(v, 0 | 1)))
		{
			return None;
		}
		let mut capacity = state.capacity;
		while capacity < cells.len() {
			capacity = Growth::next(capacity).ok()?.capacity;
		}
		let strong = state
			.strong
			.checked_add(u32::try_from(added_aliases).ok()?)?
			.checked_sub(u32::try_from(removed_aliases).ok()?)?;
		if strong == 0 {
			return None;
		}
		let Value::Heap(h) = state.value else {
			return None;
		};
		let mut values = Vec::try_with_capacity(capacity).ok()?;
		// Exact capacity is part of the semantics. A different allocator policy refuses.
		if values.capacity() != capacity {
			return None;
		}
		for &v in cells {
			values
				.try_push(match ty {
					Scalar::I64 => Value::I64(v),
					Scalar::Bool => Value::Bool(v != 0),
					Scalar::Unit => return None,
				})
				.ok()?;
		}
		let charged = Growth::bytes(capacity).ok()?;
		let bytes = self.replacement_bytes(h, charged).ok()?;
		Some(AppendPrepared {
			state,
			values,
			bytes,
			charged,
			strong,
		})
	}

	/// Exclusive caller validated every iterator/window effect before this first write.
	/// No allocation or fallible ownership operation follows it.
	pub(crate) fn publish_append(&mut self, p: AppendPrepared) {
		let Value::Heap(h) = p.state.value else {
			unreachable!()
		};
		let e = self.slots[h.slot as usize]
			.entry
			.as_mut()
			.expect("validated append handle");
		e.payload = Some(Payload::List {
			tag: Type::Vec,
			values: p.values,
		});
		e.set_charged_bytes(p.charged);
		e.strong = p.strong;
		self.bytes = p.bytes;
	}
}
