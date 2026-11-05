//! Receiver binding/publication around the shared native activation ABI.
use super::*;
use crate::experimental_core::{
	heap::{native_methods::Snapshot, Store},
	region::{
		commit::Room,
		recursion::{self, Outcome, Record},
	},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
	pub(crate) helpers: u64,
	pub(crate) recursive: u64,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn enter(
	shape: &Methods,
	code: &Code,
	window: &mut [Value],
	store: &mut Store,
	canceled: bool,
	fuel: usize,
	room: Room,
	scratch_bytes: usize,
	run: impl FnOnce(&mut [i64], &mut Record, &mut [i64]) -> Option<u32>,
) -> (Outcome, Counts) {
	let none = Counts::default();
	if !shape.valid() {
		return (Outcome::Invalid, none);
	}
	let root = &shape.functions[0];
	if canceled
		|| shape.code != code.id()
		|| window.len() != root.slots
		|| (fuel as u64) > u64::MAX / (shape.engine.ops.len() as u64 + 1)
		|| room.slots > (i64::MAX as usize).saturating_sub(MAX_SLOTS)
		|| window[1..root.parameters]
			.iter()
			.any(|&v| super::super::tag(v).is_none())
		|| window[root.parameters..].iter().any(|&v| v != Value::Unit)
	{
		return (Outcome::Refused, none);
	}
	let Some(receiver) = nominal::hash_copy(shape.nominal).map(Hash::new) else {
		return (Outcome::Refused, none);
	};
	for site in &shape.sites {
		if code.region_script_method(Hash::associated_function(receiver, Hash::new(site.name)))
			!= Some(site.target)
		{
			return (Outcome::Refused, none);
		}
	}
	let min_slots = shape.functions.iter().map(|f| f.slots).min().unwrap();
	let maxdepth = room.frames.min(room.slots / min_slots);
	let Some(snapshot) = Snapshot::prepare(store, code, shape, window[0], maxdepth) else {
		return (Outcome::Refused, none);
	};
	let Some(frames) = recursion::activation_cells(shape.engine.slots, maxdepth) else {
		return (Outcome::Refused, none);
	};
	let Some(field_values) = frames.checked_add(HEADER_CELLS) else {
		return (Outcome::Refused, none);
	};
	let Some(field_tags) = field_values.checked_add(shape.fields.len()) else {
		return (Outcome::Refused, none);
	};
	let Some(vector) = field_tags.checked_add(shape.fields.len()) else {
		return (Outcome::Refused, none);
	};
	let Some(cells) = vector
		.checked_add(snapshot.vector_len().max(1))
		.filter(|&n| n <= isize::MAX as usize / 8)
	else {
		return (Outcome::Refused, none);
	};
	let budget = Budget::new(scratch_bytes);
	let mut scratch = BVec::new(&budget);
	if scratch.resize(cells, 0i64).is_err() {
		return (Outcome::Refused, none);
	}
	for i in 0..shape.engine.slots {
		scratch[shape.engine.slots + i] = 3;
	}
	scratch[shape.engine.slots] = 4;
	for (i, &value) in window.iter().enumerate().skip(1) {
		let Some((tag, value)) = recursion::tag(value) else {
			return (Outcome::Refused, none);
		};
		scratch[i] = value;
		scratch[shape.engine.slots + i] = tag as i64;
	}
	scratch[frames + ROOM_SLOTS] = room.slots as i64;
	scratch[frames + VECTOR_LEN] = snapshot.vector_len() as i64;
	scratch[frames + VECTOR_TAG] = snapshot.vector_tag() as i64;
	let (_, tail) = scratch.split_at_mut(field_values);
	let (values, tail) = tail.split_at_mut(shape.fields.len());
	let (tags, vector_data) = tail.split_at_mut(shape.fields.len());
	if snapshot
		.fill(store, shape, values, tags, vector_data)
		.is_none()
	{
		return (Outcome::Refused, none);
	}
	let mut original = [Value::Unit; MAX_SLOTS];
	original[..window.len()].copy_from_slice(window);
	let mut control = [maxdepth as i64];
	let mut record = Record::new(fuel);
	let Some(status) = run(&mut control, &mut record, &mut scratch) else {
		return (Outcome::Refused, none);
	};
	let pc = match recursion::validate_record(&shape.engine, &record, status, fuel, maxdepth) {
		recursion::RecordClass::Complete { pc } => pc,
		recursion::RecordClass::Replay => return (Outcome::Replay { work: record.work }, none),
		recursion::RecordClass::Invalid => return (Outcome::Invalid, none),
	};
	if pc >= root.end
		|| control[0] != maxdepth as i64
		|| scratch[frames + ROOM_SLOTS] != room.slots as i64
		|| scratch[frames + VECTOR_LEN] != snapshot.vector_len() as i64
		|| scratch[frames + VECTOR_TAG] != snapshot.vector_tag() as i64
	{
		return (Outcome::Invalid, none);
	}
	let (Ok(helpers), Ok(recursive)) = (
		u64::try_from(scratch[frames + HELPER_CALLS]),
		u64::try_from(scratch[frames + SELF_CALLS]),
	) else {
		return (Outcome::Invalid, none);
	};
	if helpers.checked_add(recursive) != Some(record.calls) {
		return (Outcome::Invalid, none);
	}
	let mut output = [Value::Unit; MAX_SLOTS];
	for (i, out) in output.iter_mut().enumerate().take(window.len()) {
		let Ok(tag) = u32::try_from(scratch[shape.engine.slots + i]) else {
			return (Outcome::Invalid, none);
		};
		let Some(value) = snapshot.decode(shape, tag, scratch[i]) else {
			return (Outcome::Invalid, none);
		};
		*out = value;
	}
	let Op::Return { value } = shape.engine.ops[pc] else {
		return (Outcome::Invalid, none);
	};
	if value as usize >= root.slots || recursion::tag(output[value as usize]).is_none() {
		return (Outcome::Invalid, none);
	}
	let n = window.len();
	if !snapshot.publish(
		store,
		code,
		shape,
		window,
		&original[..n],
		&output[..n],
		&scratch[field_values..field_tags],
		&scratch[field_tags..vector],
		&scratch[vector..],
	) {
		return (Outcome::Invalid, none);
	}
	(record.completed(pc), Counts { helpers, recursive })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::experimental_core::{heap::Limits, region::tests::compile};

	fn shape(
		source: &str,
		name: &str,
	) -> (
		crate::experimental_core::frontend::Program,
		Methods,
		Budget,
		usize,
	) {
		let p = compile(source);
		let budget = Budget::new(8 << 20);
		let mut work = 1_000_000;
		let (s, bytes) = analyze(
			&p.code,
			p.function(Hash::type_hash([name])).unwrap(),
			&budget,
			&mut work,
		)
		.unwrap();
		(p, s, budget, bytes)
	}

	#[test]
	fn method_closure_atomic_publication_invalid_record_headroom_and_scratch_controls() {
		let(p,s,budget,bytes)=shape("struct Pair{total,enabled} fn run(p){p.total=p.total+1;p.enabled=true;p.total} pub fn main(n){run(Pair{total:3,enabled:false})}","run");
		let mut store = Store::new(Limits::default()).unwrap();
		let receiver = store
			.record(
				Type::Struct(s.nominal),
				&[("total", Value::I64(3)), ("enabled", Value::Bool(false))],
			)
			.unwrap();
		let mut window = [Value::Unit; MAX_SLOTS];
		window[0] = receiver;
		let n = s.functions[0].slots;
		let original = window;
		let before = store.state_for_test();
		let room = Room {
			frames: 4,
			slots: 64,
		};
		let foreign=compile("struct Pair{total,enabled} fn run(p){p.total} pub fn main(n){run(Pair{total:3,enabled:false})}");
		let (outcome, _) = enter(
			&s,
			&foreign.code,
			&mut window[..n],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|_, _, _| panic!("foreign Code must refuse before binding"),
		);
		assert_eq!(outcome, Outcome::Refused);
		assert_eq!(store.state_for_test(), before);
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..n],
			&mut store,
			false,
			100,
			room,
			0,
			|_, _, _| panic!("zero native budget must refuse before execution"),
		);
		assert_eq!(outcome, Outcome::Refused);
		assert_eq!(store.state_for_test(), before);
		assert_eq!(window, original);
		let return_pc = (0..s.functions[0].end)
			.find(|&pc| matches!(s.engine.ops[pc], Op::Return { .. }))
			.unwrap();
		let Op::Return { value } = s.engine.ops[return_pc] else {
			unreachable!()
		};
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..n],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|control, record, scratch| {
				let frame_cells =
					recursion::activation_cells(s.engine.slots, control[0] as usize).unwrap();
				let values = frame_cells + HEADER_CELLS;
				let tags = values + s.fields.len();
				for (i, field) in s.fields.iter().enumerate() {
					let name = p.code.function(field.function).unwrap().strings[field.key as usize]
						.as_str();
					if name == "total" {
						scratch[values + i] = 4;
						scratch[tags + i] = 1;
					} else {
						scratch[values + i] = 2;
						scratch[tags + i] = 2;
					}
				}
				scratch[value as usize] = 4;
				scratch[s.engine.slots + value as usize] = 1;
				record.fuel = 99;
				record.status = 1;
				record.pc = return_pc as u32;
				record.work = 1;
				Some(1)
			},
		);
		assert_eq!(outcome, Outcome::Invalid);
		assert_eq!(store.state_for_test(), before);
		assert_eq!(window, original);
		store.test_set_strong(receiver, u32::MAX - 1);
		let before = store.state_for_test();
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..n],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|_, _, _| panic!("ownership headroom guard must refuse"),
		);
		assert_eq!(outcome, Outcome::Refused);
		assert_eq!(store.state_for_test(), before);
		store.test_set_strong(receiver, 1);
		store.set_readers_for_test(receiver, 1);
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..n],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|_, _, _| panic!("leased receiver must refuse"),
		);
		assert_eq!(outcome, Outcome::Refused);
		store.set_readers_for_test(receiver, 0);
		store.invalidate(receiver).unwrap();
		let before = store.state_for_test();
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..n],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|_, _, _| panic!("moved receiver must refuse before binding"),
		);
		assert_eq!(outcome, Outcome::Refused);
		assert_eq!(store.state_for_test(), before);
		store.release(receiver).unwrap();
		assert_eq!(store.live_entries(), 0);
		drop(s);
		budget.release(bytes);
		assert_eq!(budget.used(), 0);
	}

	#[test]
	fn method_closure_scalar_and_boolean_vector_publish_together_with_root_ownership() {
		let(p,s,budget,bytes)=shape("struct Pair{total,values} fn set(p){p.total+=1;p.values[0]=false;p.total} pub fn main(n){set(Pair{total:3,values:[true]})}","set");
		for invalid in [true, false] {
			let mut store = Store::new(Limits::default()).unwrap();
			let vector = store.list(Type::Vec, &[Value::Bool(true)]).unwrap();
			let receiver = store
				.record(
					Type::Struct(s.nominal),
					&[("total", Value::I64(3)), ("values", vector)],
				)
				.unwrap();
			store.release(vector).unwrap();
			let mut window = [Value::Unit; MAX_SLOTS];
			window[0] = receiver;
			let before = store.state_for_test();
			let original = window;
			let return_pc = (0..s.functions[0].end)
				.find(|&pc| matches!(s.engine.ops[pc], Op::Return { .. }))
				.unwrap();
			let Op::Return { value } = s.engine.ops[return_pc] else {
				unreachable!()
			};
			let (outcome, _) = enter(
				&s,
				&p.code,
				&mut window[..s.functions[0].slots],
				&mut store,
				false,
				100,
				Room {
					frames: 4,
					slots: 64,
				},
				1 << 20,
				|control, record, scratch| {
					let frames =
						recursion::activation_cells(s.engine.slots, control[0] as usize).unwrap();
					let values = frames + HEADER_CELLS;
					let vector_offset = values + 2 * s.fields.len();
					for (i, field) in s.fields.iter().enumerate() {
						if field.kind == 1 {
							scratch[values + i] = 4;
						}
					}
					scratch[vector_offset] = if invalid { 2 } else { 0 };
					scratch[value as usize] = 4;
					scratch[s.engine.slots + value as usize] = 1;
					// A validated private Vec token becomes a rooted final temporary,
					// with its ownership increment published in the same transaction.
					let alias = s.functions[0].slots - 1;
					assert_ne!(alias, value as usize);
					scratch[alias] = 1;
					scratch[s.engine.slots + alias] = 5;
					record.fuel = 99;
					record.status = 1;
					record.pc = return_pc as u32;
					record.work = 1;
					Some(1)
				},
			);
			if invalid {
				assert_eq!(outcome, Outcome::Invalid);
				assert_eq!(store.state_for_test(), before);
				assert_eq!(window, original);
			} else {
				assert!(matches!(outcome, Outcome::Complete { .. }));
				assert_eq!(store.field(receiver, "total").unwrap(), Value::I64(4));
				assert_eq!(store.get(vector, 0).unwrap(), Value::Bool(false));
			}
			for &value in window.iter().take(s.functions[0].slots) {
				store.release(value).unwrap();
			}
			assert_eq!(store.live_entries(), 0);
		}
		drop(s);
		budget.release(bytes);
		assert_eq!(budget.used(), 0);
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "executable code; safe transaction guards have separate Miri coverage"
	)]
	fn method_closure_both_record_orders_execute_native_and_fault_discards_private_mutation() {
		let(p,s,budget,bytes)=shape("struct Pair{left,right} fn read(p){p.left*10+p.right} pub fn main(n){read(Pair{left:1,right:2})}","read");
		let compiled =
			crate::experimental_native::lower::recursion::compile_methods(&s, 0).unwrap();
		let native =
			crate::experimental_native::boundary::NativeRecursion::load(&compiled).unwrap();
		for reversed in [false, true] {
			let mut store = Store::new(Limits::default()).unwrap();
			let fields = if reversed {
				[("right", Value::I64(3)), ("left", Value::I64(4))]
			} else {
				[("left", Value::I64(1)), ("right", Value::I64(2))]
			};
			let receiver = store.record(Type::Struct(s.nominal), &fields).unwrap();
			let mut window = [Value::Unit; MAX_SLOTS];
			window[0] = receiver;
			let (outcome, counts) = enter(
				&s,
				&p.code,
				&mut window[..s.functions[0].slots],
				&mut store,
				false,
				100,
				Room {
					frames: 4,
					slots: 64,
				},
				1 << 20,
				|c, r, b| native.call(c, r, b),
			);
			let Outcome::Complete { pc, .. } = outcome else {
				panic!("{outcome:?}")
			};
			let Op::Return { value } = s.engine.ops[pc] else {
				unreachable!()
			};
			assert_eq!(
				window[value as usize],
				Value::I64(if reversed { 43 } else { 12 })
			);
			assert_eq!(counts, Counts::default());
			for value in window.iter().take(s.functions[0].slots) {
				store.release(*value).unwrap();
			}
			assert_eq!(store.live_entries(), 0);
		}
		drop(s);
		budget.release(bytes);
		let(p,s,budget,bytes)=shape("struct Pair{total,values} fn mutate(p,n){p.total=p.total+1;p.values[0]=p.values[0]+2;p.values[n]} pub fn main(n){mutate(Pair{total:3,values:[1,2]},n)}","mutate");
		let compiled =
			crate::experimental_native::lower::recursion::compile_methods(&s, 0).unwrap();
		let native =
			crate::experimental_native::boundary::NativeRecursion::load(&compiled).unwrap();
		let mut store = Store::new(Limits::default()).unwrap();
		let vector = store
			.list(Type::Vec, &[Value::I64(1), Value::I64(2)])
			.unwrap();
		let receiver = store
			.record(
				Type::Struct(s.nominal),
				&[("total", Value::I64(3)), ("values", vector)],
			)
			.unwrap();
		store.release(vector).unwrap();
		let mut window = [Value::Unit; MAX_SLOTS];
		window[0] = receiver;
		window[1] = Value::I64(100);
		let original = window;
		let before = store.state_for_test();
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..s.functions[0].slots],
			&mut store,
			false,
			100,
			Room {
				frames: 4,
				slots: 64,
			},
			1 << 20,
			|c, r, b| native.call(c, r, b),
		);
		assert!(matches!(outcome, Outcome::Replay { .. }));
		assert_eq!(store.state_for_test(), before);
		assert_eq!(window, original);
		store.release(receiver).unwrap();
		assert_eq!(store.live_entries(), 0);
		drop(s);
		budget.release(bytes);
		assert_eq!(budget.used(), 0);
	}

	#[test]
	#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
	#[cfg_attr(
		miri,
		ignore = "actual native code; boolean vector publication is checked safely as well"
	)]
	fn method_closure_boolean_vector_native_writes_and_lease_guard() {
		let(p,s,budget,bytes)=shape("struct Bag{values} fn exchange(p){let old=p.values[0];p.values[0]=p.values[1];p.values[1]=old;} pub fn main(n){exchange(Bag{values:[true,false]})}","exchange");
		let compiled =
			crate::experimental_native::lower::recursion::compile_methods(&s, 0).unwrap();
		let native =
			crate::experimental_native::boundary::NativeRecursion::load(&compiled).unwrap();
		let mut store = Store::new(Limits::default()).unwrap();
		let vector = store
			.list(Type::Vec, &[Value::Bool(true), Value::Bool(false)])
			.unwrap();
		let receiver = store
			.record(Type::Struct(s.nominal), &[("values", vector)])
			.unwrap();
		store.release(vector).unwrap();
		let mut window = [Value::Unit; MAX_SLOTS];
		window[0] = receiver;
		let room = Room {
			frames: 4,
			slots: 64,
		};
		store.test_set_strong(vector, u32::MAX - 1);
		let before = store.state_for_test();
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..s.functions[0].slots],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|_, _, _| panic!("Vec ownership headroom must refuse before execution"),
		);
		assert_eq!(outcome, Outcome::Refused);
		assert_eq!(store.state_for_test(), before);
		store.test_set_strong(vector, 1);
		store.set_readers_for_test(vector, 1);
		let before = store.state_for_test();
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..s.functions[0].slots],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|_, _, _| panic!("leased Vec must refuse before execution"),
		);
		assert_eq!(outcome, Outcome::Refused);
		assert_eq!(store.state_for_test(), before);
		store.set_readers_for_test(vector, 0);
		let (outcome, _) = enter(
			&s,
			&p.code,
			&mut window[..s.functions[0].slots],
			&mut store,
			false,
			100,
			room,
			1 << 20,
			|c, r, b| native.call(c, r, b),
		);
		assert!(matches!(outcome, Outcome::Complete { .. }));
		assert_eq!(store.get(vector, 0).unwrap(), Value::Bool(false));
		assert_eq!(store.get(vector, 1).unwrap(), Value::Bool(true));
		for &value in window.iter().take(s.functions[0].slots) {
			store.release(value).unwrap();
		}
		assert_eq!(store.live_entries(), 0);
		drop(s);
		budget.release(bytes);
		assert_eq!(budget.used(), 0);
	}
}
