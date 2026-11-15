//! Independent state, ownership and deterministic work checks for Object indexing.
use super::object_index::{self, Index, INDEX_AT};
use super::*;
use crate::function::object::Method;
use rune_alloc::limit;
use std::borrow::ToOwned;

fn keys(n: usize) -> std::vec::Vec<std::string::String> {
	(0..n).map(|i| std::format!("key_{i:06}")).collect()
}
fn fields(keys: &[std::string::String], value: Value) -> std::vec::Vec<(&str, Value)> {
	keys.iter().map(|key| (key.as_str(), value)).collect()
}
fn object(s: &mut Store, n: usize, value: Value) -> Value {
	let k = keys(n);
	s.record(Type::Object, &fields(&k, value)).unwrap()
}
fn scalar(s: &mut Store, option: Value) -> Value {
	let value = s.tuple_get(option, 0).unwrap();
	s.release(option).unwrap();
	value
}
fn fingerprint(s: &Store) -> (usize, usize, usize, std::string::String) {
	(s.live, s.bytes, s.edges, std::format!("{:?}", s.slots))
}
fn indexed(s: &Store, v: Value) -> bool {
	let Value::Heap(h) = v else {
		panic!("object handle")
	};
	matches!(
		s.payload(h).unwrap(),
		Payload::Object { index: Some(_), .. }
	)
}

#[test]
fn object_index_store_operations_and_order() {
	let sizes: &[usize] = if cfg!(miri) {
		&[0, 1, 31, 32, 33]
	} else {
		&[0, 1, 31, 32, 33, 100000]
	};
	for &n in sizes {
		let mut s = Store::new(Limits::default()).unwrap();
		let v = object(&mut s, n, Value::I64(7));
		assert_eq!(indexed(&s, v), n >= INDEX_AT);
		assert_eq!(s.len(v).unwrap(), n);
		let clone = s.clone_value(v).unwrap();
		assert_eq!(
			s.compare(super::super::arithmetic::Binary::Eq, v, clone)
				.unwrap(),
			Value::Bool(true)
		);
		for key in ["absent", "", "\0", "key_999999"] {
			let k = s.string(key).unwrap();
			let got = s.object_method(Method::ContainsKey, &[v, k]).unwrap();
			assert_eq!(got, Value::Bool(false));
			s.release(k).unwrap();
		}
		if n > 0 {
			let last = std::format!("key_{:06}", n - 1);
			assert_eq!(s.field(v, &last).unwrap(), Value::I64(7));
			s.set_field(v, &last, Value::I64(11)).unwrap();
			assert_eq!(s.field(v, &last).unwrap(), Value::I64(11));
			let key = s.string(&last).unwrap();
			s.dynamic_set(v, key, Value::I64(13)).unwrap();
			assert_eq!(s.dynamic_get(v, key).unwrap(), Value::I64(13));
			let old = s
				.object_method(Method::Insert, &[v, key, Value::I64(17)])
				.unwrap();
			assert_eq!(scalar(&mut s, old), Value::I64(13));
			let value = s.object_method(Method::Get, &[v, key]).unwrap();
			assert_eq!(scalar(&mut s, value), Value::I64(17));
			s.release(key).unwrap();
		}
		// Removal keeps field order; reinsertion appends, including at demotion.
		let mut order = keys(n);
		for choose in [0, 1, 2] {
			if order.is_empty() {
				break;
			}
			let at = match choose {
				0 => 0,
				1 => order.len() / 2,
				_ => order.len() - 1,
			};
			let removed = order.remove(at);
			let key = s.string(&removed).unwrap();
			let old = s.object_method(Method::Remove, &[v, key]).unwrap();
			s.release(old).unwrap();
			assert!(s
				.host_observe_fields(v)
				.unwrap()
				.iter()
				.map(|(k, _)| k.as_str())
				.eq(order.iter().map(|s| s.as_str())));
			let old = s
				.object_method(Method::Insert, &[v, key, Value::I64(19)])
				.unwrap();
			s.release(old).unwrap();
			order.push(removed);
			s.release(key).unwrap();
		}
		for key in &order {
			assert!(s.field(v, key).is_ok());
		}
		assert_eq!(s.len(v).unwrap(), n);
		s.release(clone).unwrap();
		s.release(v).unwrap();
		assert_eq!((s.live, s.bytes, s.edges), (0, 0, 0));
	}
}

fn lookup_bound(probes: [usize; 4], length: usize) -> bool {
	probes[0] <= 9 * (length + 1) + 1 && probes[1] <= 1 && probes[2] <= length
}
#[test]
fn object_index_key_length_bound_and_teeth() {
	let length = 7;
	let bound = 9 * (length + 1) + 1;
	assert!(lookup_bound([bound, 1, length, 0], length));
	assert!(!lookup_bound([bound + 1, 1, length, 0], length));
	let n = if cfg!(miri) { 48 } else { 100000 };
	let k = keys(n);
	let mut index = Index::empty(n).unwrap();
	for entry in 0..n {
		object_index::probes();
		let insert = index.prepare(&k[entry], entry, |i| k[i].as_str()).unwrap();
		let counts = object_index::probes();
		assert!(counts[0] <= 18 * (k[entry].len() + 1) + 2);
		assert!(counts[1] <= 1);
		assert!(counts[2] <= k[entry].len() + 1);
		index.commit(insert).unwrap();
	}
	for (entry, key) in k.iter().enumerate() {
		object_index::probes();
		assert_eq!(index.position(key, |i| k[i].as_str()), Some(entry));
		assert!(lookup_bound(object_index::probes(), key.len()));
	}
	for key in ["", "\0", "k", "key", "key_absent"] {
		object_index::probes();
		assert_eq!(index.position(key, |i| k[i].as_str()), None);
		assert!(lookup_bound(object_index::probes(), key.len()));
	}
}

#[test]
fn object_index_prefix_nul_utf8_and_removal_permutations() {
	let mut k = std::vec![
		"".to_owned(),
		"\0".to_owned(),
		"\0\0".to_owned(),
		"a".to_owned(),
		"ab".to_owned(),
		"é".to_owned(),
		"é\0".to_owned()
	];
	k.extend((0..48).map(|i| std::format!("long-common-prefix-{i:06}")));
	for reverse in [false, true] {
		if reverse {
			k.reverse();
		}
		let mut remaining = k.clone();
		let mut index = Index::build(k.len(), k.len(), |i| k[i].as_str()).unwrap();
		while !remaining.is_empty() {
			for (i, key) in remaining.iter().enumerate() {
				assert_eq!(index.position(key, |i| remaining[i].as_str()), Some(i));
			}
			for key in ["long", "long-common-prefix-", "éx", "\0x"] {
				object_index::probes();
				assert_eq!(index.position(key, |i| remaining[i].as_str()), None);
				assert!(lookup_bound(object_index::probes(), key.len()));
			}
			let at = remaining.len() / 2;
			index.remove(&remaining[at], at);
			remaining.remove(at);
		}
	}
}

#[test]
#[cfg(not(miri))]
fn object_index_fixed_fingerprint_collisions() {
	// Every key collides under the low eight bits of the public fixed FNV-1a-32 hash.
	// No claim of a full 64-bit AHash collision: this index does not hash.
	fn fingerprint(key: &str) -> u8 {
		key.bytes().fold(0xc5u8, |h, b| (h ^ b).wrapping_mul(0x93))
	}
	let mut k = std::vec::Vec::with_capacity(100000);
	let mut trial = 0u64;
	while k.len() < 100000 {
		let key = std::format!("common-prefix-{trial:016x}");
		trial += 1;
		if fingerprint(&key) == 0 {
			k.push(key);
		}
	}
	let index = Index::build(k.len(), k.len(), |i| k[i].as_str()).unwrap();
	for (i, key) in k.iter().enumerate() {
		assert_eq!(fingerprint(key), 0);
		object_index::probes();
		assert_eq!(index.position(key, |i| k[i].as_str()), Some(i));
		assert!(lookup_bound(object_index::probes(), key.len()));
	}
}

#[test]
fn object_index_allocation_and_budget_failure_is_atomic() {
	for n in [31, 32, 64] {
		for budget in [0, 1, 5, 39, 40, 200, 1000, 2000, 4000, 8000] {
			let mut s = Store::new(Limits::default()).unwrap();
			let child = s.string("child").unwrap();
			let v = object(&mut s, n, child);
			let key = s.string("new-key").unwrap();
			let before = fingerprint(&s);
			let result = limit::with(budget, || s.set_field(v, "new-key", child)).call();
			match result {
				Err(e) => {
					assert_eq!(e.kind, ErrorKind::Allocation);
					assert_eq!(fingerprint(&s), before);
				}
				Ok(()) => assert_eq!(s.len(v).unwrap(), n + 1),
			}
			s.release(key).unwrap();
			s.release(v).unwrap();
			s.release(child).unwrap();
			assert_eq!((s.live, s.bytes, s.edges), (0, 0, 0));
		}
		let mut s = Store::new(Limits::default()).unwrap();
		let child = s.string("child").unwrap();
		let v = object(&mut s, n, child);
		let before = fingerprint(&s);
		s.limits.bytes = s.bytes;
		let e = limit::with(0, || s.set_field(v, "new", child).unwrap_err()).call();
		assert_eq!(e.kind, ErrorKind::StoreCapacity);
		assert_eq!(fingerprint(&s), before);
		s.release(v).unwrap();
		s.release(child).unwrap();
	}
}

#[test]
fn object_index_constructor_failure_retains_nothing() {
	let k = keys(32);
	for budget in [0, 1279, 1280, 1500, 2000, 2500, 2600, 3000] {
		let mut s = Store::new(Limits::default()).unwrap();
		let child = s.string("child").unwrap();
		let input = fields(&k, child);
		let before = fingerprint(&s);
		match limit::with(budget, || s.record(Type::Object, &input)).call() {
			Err(e) => {
				assert_eq!(e.kind, ErrorKind::Allocation);
				assert_eq!(fingerprint(&s), before);
			}
			Ok(v) => s.release(v).unwrap(),
		}
		s.release(child).unwrap();
		assert_eq!((s.live, s.bytes, s.edges), (0, 0, 0));
	}
}

#[test]
fn object_index_growth_work_is_linear() {
	let n = if cfg!(miri) { 64 } else { 100000 };
	let mut store = Store::new(Limits::default()).unwrap();
	let object = store.record(Type::Object, &[]).unwrap();
	object_index::probes();
	for i in 0..n {
		let key = std::format!("key_{i:06}");
		store.set_field(object, &key, Value::I64(i as i64)).unwrap();
	}
	let work = object_index::probes();
	assert!(work[0] <= n * 27 * 11 + 4 * n);
	assert!(work[1] <= 2 * n);
	assert!(work[2] <= n * 21);
	assert!(work[3] <= 4 * n + INDEX_AT);
	assert_eq!(store.len(object).unwrap(), n);
	store.release(object).unwrap();
	assert_eq!((store.live, store.bytes, store.edges), (0, 0, 0));
}

#[test]
fn object_index_duplicate_invariant_and_capacity_precedence() {
	let mut k = keys(INDEX_AT);
	k[INDEX_AT - 1] = k[0].clone();
	let mut s = Store::new(Limits::default()).unwrap();
	assert_eq!(
		s.record(Type::Object, &fields(&k, Value::Unit))
			.unwrap_err()
			.kind,
		ErrorKind::InvalidCode
	);
	assert_eq!((s.live, s.bytes, s.edges), (0, 0, 0));
	s.limits.bytes = 1;
	assert_eq!(
		limit::with(0, || s
			.record(Type::Object, &fields(&k, Value::Unit))
			.unwrap_err()
			.kind)
		.call(),
		ErrorKind::StoreCapacity
	);
}

#[test]
fn object_index_full_capacity_failure_and_undo() {
	let k = keys(3);
	let mut index = Index::build(2, 2, |i| k[i].as_str()).unwrap();
	let before = std::format!("{index:?}");
	let insert = index.prepare(&k[2], 2, |i| k[i].as_str()).unwrap();
	assert_eq!(
		index.commit(insert).unwrap_err().kind,
		ErrorKind::InvalidCode
	);
	assert_eq!(before, std::format!("{index:?}"));
	let mut index = index.copy(3).unwrap();
	let before = std::format!("{index:?}");
	let insert = index.prepare(&k[2], 2, |i| k[i].as_str()).unwrap();
	index.commit(insert).unwrap();
	index.undo(insert);
	assert_eq!(before, std::format!("{index:?}"));
}
