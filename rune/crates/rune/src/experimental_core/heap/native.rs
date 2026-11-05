//! Checked native-tier snapshots and commits. B1 snapshots a RangeIterator's next/end;
//! N2 copies a homogeneous scalar Vec into a private budgeted i64 buffer. Generated
//! code never receives any Store, Value or payload pointer. Safe Rust throughout
//! (the execution core is `forbid(unsafe_code)`).
//!
//! Every check of the setter runs before its single write, on the same exclusive borrow
//! of the store: a refusal mutates nothing.

/// The setter refused; nothing was mutated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RangeCommitRefused;

/// `new` is a value of `next` that `iterator_next` could reach from `(from, end)`.
/// Pure comparisons: no arithmetic, so no overflow at any i64 boundary.
pub(crate) fn range_next_reachable((from, end): (i64, i64), new: i64) -> bool {
	if from >= end {
		new == from
	} else {
		from <= new && new <= end
	}
}

use super::{Payload, Store};
use crate::experimental_core::{
	region::{
		budget::{BVec, Budget},
		Scalar,
	},
	value::{Type, Value},
};

/// The live iterator and its already leased source. No extra ownership is acquired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VectorState {
	pub(crate) iterator: Value,
	source: Value,
	pub(crate) index: usize,
	pub(crate) len: usize,
	readers: u32,
}

impl Store {
	pub(crate) fn vector_iterator_for_native(&self, value: Value) -> Option<VectorState> {
		let Value::Heap(handle) = value else {
			return None;
		};
		let entry = self.entry(handle).ok()?;
		if entry.readers != 0 {
			return None;
		}
		let Payload::VectorIterator(iter) = entry.payload.as_ref()? else {
			return None;
		};
		let source = self.entry(iter.handle).ok()?;
		let Payload::List {
			tag: Type::Vec,
			values,
		} = source.payload.as_ref()?
		else {
			return None;
		};
		if source.readers == 0
			|| source.readers == super::sorting::EXCLUSIVE
			|| iter.index > values.len()
			|| i64::try_from(values.len()).is_err()
		{
			return None;
		}
		Some(VectorState {
			iterator: value,
			source: Value::Heap(iter.handle),
			index: iter.index,
			len: values.len(),
			readers: source.readers,
		})
	}

	pub(crate) fn vector_iterator_kind_for_native(&self, value: Value, ty: Scalar) -> bool {
		let Some(state) = self.vector_iterator_for_native(value) else {
			return false;
		};
		let Value::Heap(source) = state.source else {
			return false;
		};
		let Ok(Payload::List { values, .. }) = self.payload(source) else {
			return false;
		};
		values.first().is_none_or(|v| {
			matches!(
				(ty, *v),
				(Scalar::I64, Value::I64(_)) | (Scalar::Bool, Value::Bool(_))
			)
		})
	}

	pub(crate) fn snapshot_iterator_for_native<'b>(
		&self,
		value: Value,
		ty: Scalar,
		budget: &'b Budget,
	) -> Option<(BVec<'b, i64>, VectorState)> {
		let state = self.vector_iterator_for_native(value)?;
		let Value::Heap(source) = state.source else {
			return None;
		};
		let Payload::List { values, .. } = self.payload(source).ok()? else {
			return None;
		};
		if !values.iter().all(|v| {
			matches!(
				(ty, *v),
				(Scalar::I64, Value::I64(_)) | (Scalar::Bool, Value::Bool(_))
			)
		}) {
			return None;
		}
		let mut snapshot = BVec::with_capacity(budget, state.len.max(1)).ok()?;
		snapshot.resize(state.len.max(1), 0).ok()?;
		for (out, value) in snapshot.iter_mut().zip(values.iter()) {
			*out = match (ty, *value) {
				(Scalar::I64, Value::I64(v)) => v,
				(Scalar::Bool, Value::Bool(v)) => i64::from(v),
				_ => return None,
			};
		}
		Some((snapshot, state))
	}

	/// All refusal checks precede the single cursor write. The source lease is retained.
	pub(crate) fn commit_vector_iterator_for_native(
		&mut self,
		expected: VectorState,
		next: i64,
	) -> bool {
		if !self.scalar_release_is_noop()
			|| self.vector_iterator_for_native(expected.iterator) != Some(expected)
		{
			return false;
		}
		let Ok(next) = usize::try_from(next) else {
			return false;
		};
		if next < expected.index || next > expected.len {
			return false;
		}
		let Value::Heap(handle) = expected.iterator else {
			return false;
		};
		let Ok(Payload::VectorIterator(iter)) = self.mutable(handle) else {
			return false;
		};
		iter.index = next;
		true
	}

	/// Available configured heap room also bounds temporary native copies. The native
	/// transient Budget charges the actual scratch capacity independently of heap bytes.
	pub(crate) fn native_snapshot_room(&self) -> usize {
		self.limits.bytes.saturating_sub(self.bytes)
	}

	fn scalar_collection(&self, value: Value, ty: Scalar) -> Option<&[Value]> {
		let Value::Heap(handle) = value else {
			return None;
		};
		let entry = self.entry(handle).ok()?;
		if entry.readers != 0 {
			return None;
		}
		let Payload::List {
			tag: Type::Vec,
			values,
		} = entry.payload.as_ref()?
		else {
			return None;
		};
		if !values.iter().all(|value| {
			matches!(
				(ty, value),
				(Scalar::I64, Value::I64(_)) | (Scalar::Bool, Value::Bool(_))
			)
		}) {
			return None;
		}
		Some(values)
	}

	/// No payload pointer leaves this safe module. One initialized sentinel keeps even
	/// an empty snapshot's clamped address inside a live allocation.
	pub(crate) fn snapshot_for_native<'b>(
		&self,
		value: Value,
		ty: Scalar,
		budget: &'b Budget,
	) -> Option<(BVec<'b, i64>, usize)> {
		let values = self.scalar_collection(value, ty)?;
		let mut snapshot = BVec::with_capacity(budget, values.len().max(1)).ok()?;
		snapshot.resize(values.len().max(1), 0).ok()?;
		for (out, value) in snapshot.iter_mut().zip(values.iter()) {
			*out = match *value {
				Value::I64(v) => v,
				Value::Bool(v) => i64::from(v),
				_ => return None,
			};
		}
		Some((snapshot, values.len()))
	}

	/// Validate BOTH payloads before either write. No allocation, releases, callbacks or
	/// fallible operations follow the first mutation. The two handles cannot alias:
	/// one has a List payload and the other a RangeIterator payload.
	pub(crate) fn commit_snapshot_for_native(
		&mut self,
		value: Value,
		ty: Scalar,
		snapshot: &[i64],
		iter: Option<(Value, (i64, i64), i64)>,
	) -> bool {
		if !self.scalar_release_is_noop()
			|| self
				.scalar_collection(value, ty)
				.is_none_or(|v| v.len() != snapshot.len())
			|| (ty == Scalar::Bool && snapshot.iter().any(|v| !matches!(v, 0 | 1)))
		{
			return false;
		}
		if let Some((value, expect, new)) = iter {
			if self.range_iterator_for_native(value) != Some(expect)
				|| !range_next_reachable(expect, new)
			{
				return false;
			}
		}
		if let Some((Value::Heap(h), _, new)) = iter {
			let Some(Payload::RangeIterator { next, .. }) = self.slots[h.slot as usize]
				.entry
				.as_mut()
				.and_then(|e| e.payload.as_mut())
			else {
				unreachable!()
			};
			*next = new;
		}
		let Value::Heap(h) = value else {
			unreachable!()
		};
		let Some(Payload::List { values, .. }) = self.slots[h.slot as usize]
			.entry
			.as_mut()
			.and_then(|e| e.payload.as_mut())
		else {
			unreachable!()
		};
		for (out, &value) in values.iter_mut().zip(snapshot) {
			*out = match ty {
				Scalar::Bool => Value::Bool(value != 0),
				Scalar::I64 => Value::I64(value),
				Scalar::Unit => unreachable!(),
			};
		}
		true
	}
}

#[cfg(test)]
mod n3_tests {
	use super::*;

	#[test]
	fn n3_snapshot_accepts_source_leases_and_commit_keeps_them() {
		let mut s = Store::new(super::super::Limits::default()).unwrap();
		let values = s
			.list(Type::Vec, &[Value::I64(i64::MIN), Value::I64(i64::MAX)])
			.unwrap();
		let iterator = s.iterator(values).unwrap();
		let before = s.state_for_test();
		assert!(s
			.snapshot_for_native(values, Scalar::I64, &Budget::unlimited())
			.is_none());
		let budget = Budget::new(16);
		let (snapshot, state) = s
			.snapshot_iterator_for_native(iterator, Scalar::I64, &budget)
			.unwrap();
		assert_eq!(&snapshot[..], &[i64::MIN, i64::MAX]);
		assert_eq!(budget.used(), 16);
		assert_eq!(s.state_for_test(), before);
		let other = s.iterator(values).unwrap();
		let shared = s.state_for_test();
		assert!(!s.commit_vector_iterator_for_native(state, 1));
		assert_eq!(s.state_for_test(), shared);
		s.release(other).unwrap();
		assert!(s.commit_vector_iterator_for_native(state, 2));
		assert_eq!(s.vector_iterator_for_native(iterator).unwrap().readers, 1);
		assert_eq!(s.iterator_next(iterator).unwrap(), None);
		drop(snapshot);
		assert_eq!(budget.used(), 0);
		s.release(iterator).unwrap();
		s.release(values).unwrap();
		assert_eq!(s.live_entries(), 0);
	}

	#[test]
	#[cfg_attr(miri, ignore = "Miri: million-element allocation/scan witness")]
	fn n3_snapshot_large_budget_and_resumed_cursor() {
		let mut s = Store::new(super::super::Limits::default()).unwrap();
		let items = vec![Value::I64(i64::MIN); 1_000_001];
		let source = s.list(Type::Vec, &items).unwrap();
		let iterator = s.iterator(source).unwrap();
		assert_eq!(
			s.iterator_next(iterator).unwrap(),
			Some(Value::I64(i64::MIN))
		);
		let before = s.state_for_test();
		let bytes = items.len() * core::mem::size_of::<i64>();
		assert!(s
			.snapshot_iterator_for_native(iterator, Scalar::I64, &Budget::new(bytes - 1))
			.is_none());
		assert_eq!(before, s.state_for_test());
		let budget = Budget::new(bytes);
		let (snapshot, state) = s
			.snapshot_iterator_for_native(iterator, Scalar::I64, &budget)
			.unwrap();
		assert_eq!(
			(state.index, state.len, snapshot.len(), budget.used()),
			(1, items.len(), items.len(), bytes)
		);
		assert!(!s.commit_vector_iterator_for_native(state, 0));
		assert!(!s.commit_vector_iterator_for_native(state, -1));
		assert!(!s.commit_vector_iterator_for_native(state, (items.len() + 1) as i64));
		assert_eq!(before, s.state_for_test());
		assert!(s.commit_vector_iterator_for_native(state, items.len() as i64));
		assert_eq!(s.iterator_next(iterator).unwrap(), None);
		drop(snapshot);
		assert_eq!(budget.used(), 0);
		s.release(iterator).unwrap();
		s.release(source).unwrap();
		assert_eq!(s.live_entries(), 0);
	}

	#[test]
	fn n3_empty_allocation_stale_moved_and_closed_refusals() {
		for mode in 0..4 {
			let mut s = Store::new(super::super::Limits::default()).unwrap();
			let source = s.list(Type::Vec, &[]).unwrap();
			let iterator = s.iterator(source).unwrap();
			let budget = Budget::new(8);
			let (snapshot, state) = s
				.snapshot_iterator_for_native(iterator, Scalar::Bool, &budget)
				.unwrap();
			assert_eq!((state.index, state.len, snapshot.len()), (0, 0, 1));
			drop(snapshot);
			let before = s.state_for_test();
			let refused = rune_alloc::limit::with(0, || {
				s.snapshot_iterator_for_native(iterator, Scalar::Bool, &budget)
			})
			.call();
			assert!(refused.is_none());
			assert_eq!(budget.used(), 0);
			assert_eq!(before, s.state_for_test());
			match mode {
				0 => {
					s.release(iterator).unwrap();
					let _reuse = s.list(Type::Vec, &[]).unwrap();
				}
				1 => s.invalidate(iterator).unwrap(),
				2 => {
					assert!(
						s.invalidate(source).is_err(),
						"live iterator leases its source"
					);
					let Value::Heap(handle) = source else {
						unreachable!()
					};
					s.set_generation_for_test(source, handle.generation + 1);
				}
				_ => {
					assert!(s.close().is_err(), "live iterator prevents store close");
					s.release(iterator).unwrap();
					s.close().unwrap();
				}
			}
			let invalid = s.state_for_test();
			assert!(s
				.snapshot_iterator_for_native(iterator, Scalar::Bool, &budget)
				.is_none());
			assert!(!s.commit_vector_iterator_for_native(state, 0));
			assert_eq!(invalid, s.state_for_test());
		}
	}

	#[test]
	fn n3_snapshot_refusals_do_not_advance_or_change_ownership() {
		let mut s = Store::new(super::super::Limits::default()).unwrap();
		let values = s
			.list(Type::Vec, &[Value::Bool(false), Value::Bool(true)])
			.unwrap();
		let iterator = s.iterator(values).unwrap();
		let before = s.state_for_test();
		assert!(s
			.snapshot_iterator_for_native(iterator, Scalar::I64, &Budget::unlimited())
			.is_none());
		assert!(s
			.snapshot_iterator_for_native(iterator, Scalar::Bool, &Budget::new(15))
			.is_none());
		assert_eq!(s.state_for_test(), before);
		let Value::Heap(handle) = iterator else {
			unreachable!()
		};
		let Some(Payload::VectorIterator(iter)) = s.entry_mut(handle).unwrap().payload.as_mut()
		else {
			unreachable!()
		};
		iter.index = 3;
		let corrupt = s.state_for_test();
		assert!(s.vector_iterator_for_native(iterator).is_none());
		assert_eq!(s.state_for_test(), corrupt);
		let Some(Payload::VectorIterator(iter)) = s.entry_mut(handle).unwrap().payload.as_mut()
		else {
			unreachable!()
		};
		iter.index = 0;
		s.set_readers_for_test(iterator, 1);
		assert!(s.vector_iterator_for_native(iterator).is_none());
		s.set_readers_for_test(iterator, 0);
		s.release(iterator).unwrap();
		assert!(s.vector_iterator_for_native(iterator).is_none());
		s.release(values).unwrap();
		assert_eq!(s.live_entries(), 0);
	}
}

#[cfg(test)]
mod tests;
