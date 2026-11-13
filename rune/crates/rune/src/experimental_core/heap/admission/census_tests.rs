use super::*;
use crate::experimental_core::{error::Refusal, value::Nominal};
use crate::Hash;

fn named(name: &str) -> (Hash, Nominal) {
	let hash = Hash::type_hash([name]);
	(hash, nominal::intern(hash.into_inner(), name).unwrap())
}
fn store() -> Store {
	Store::new(Limits::default()).unwrap()
}
fn authority(denied: Hash) -> crate::sync::Arc<Certificate> {
	let mut state = crate::function::formatting::State::default();
	state
		.begin(
			Hash::associated_function(denied, &crate::runtime::Protocol::DISPLAY_FMT),
			false,
		)
		.unwrap();
	state.snapshot().unwrap()
}
fn refused(store: &Store, root: &Root, certificate: &Certificate) {
	assert_eq!(
		store
			.validate_format_roots(&[root], certificate)
			.unwrap_err()
			.refusal,
		Some(Refusal::Entry)
	);
}

#[test]
fn plain_constructors_allocate_no_census() {
	let mut s = store();
	let before = s.bookkeeping_bytes().unwrap();
	rune_alloc::limit::with(0, || {
		for tag in [Type::Vec, Type::Tuple] {
			let v = s.list(tag, &[]).unwrap();
			s.release(v).unwrap();
		}
		let v = s.record(Type::Object, &[]).unwrap();
		s.release(v).unwrap();
	})
	.call();
	assert!(s.struct_census.is_none());
	assert_eq!(s.bookkeeping_bytes().unwrap(), before);
}

#[test]
fn first_struct_census_allocation_failure_is_visible() {
	let (_, n) = named("Admission0269Alloc");
	let mut s = store();
	let before = (
		s.live_entries(),
		s.payload_bytes(),
		s.bookkeeping_bytes().unwrap(),
	);
	let error = rune_alloc::limit::with(0, || s.record(Type::Struct(n), &[]))
		.call()
		.unwrap_err();
	assert_eq!(error.kind, ErrorKind::Allocation);
	assert!(s.struct_census.is_none());
	assert_eq!(
		(
			s.live_entries(),
			s.payload_bytes(),
			s.bookkeeping_bytes().unwrap()
		),
		before
	);
	let v = s.record(Type::Struct(n), &[]).unwrap();
	assert_eq!(
		s.bookkeeping_bytes().unwrap() - before.2,
		core::mem::size_of::<StructCensus>()
	);
	s.release(v).unwrap();
	assert!(s.struct_census.is_some());
	s.close().unwrap();
	assert!(s.struct_census.is_none());
}

#[test]
fn permitted_nominals_are_checked_once_per_type_not_instance() {
	let (_, a) = named("Admission0269ManyA");
	let (_, b) = named("Admission0269ManyB");
	let certificate = authority(Hash::type_hash(["Admission0269Unrelated"]));
	let mut s = store();
	let v = s.root_list(Type::Vec, &[]).unwrap();
	for i in 0..32 {
		let item = s
			.root_record(Some(if i % 2 == 0 { a } else { b }), &[])
			.unwrap();
		s.root_push(&v, &item).unwrap();
		s.root_release(item).unwrap();
	}
	Store::format_walk_probe();
	s.validate_format_roots(&[&v], &certificate).unwrap();
	assert_eq!(Store::format_walk_probe(), (0, 0, 2));
}

#[test]
fn aliased_push_of_denied_struct_invalidates_no_cached_answer() {
	let (hash, bad) = named("Admission0269Alias");
	let certificate = authority(hash);
	let mut s = store();
	let inner = s.root_list(Type::Vec, &[]).unwrap();
	let outer = s.root_list(Type::Vec, &[&inner]).unwrap();
	s.validate_format_roots(&[&outer], &certificate).unwrap();
	let item = s.root_record(Some(bad), &[]).unwrap();
	s.root_push(&inner, &item).unwrap();
	refused(&s, &outer, &certificate);
	assert_eq!(s.root_len(&inner).unwrap(), 1);
}

#[test]
fn field_replacement_and_object_insert_keep_deep_refusal() {
	let (hash, bad) = named("Admission0269Fields");
	let certificate = authority(hash);
	let mut s = store();
	let scalar = s.root_i64(7);
	let object = s.root_record(None, &[("x", &scalar)]).unwrap();
	s.validate_format_roots(&[&object], &certificate).unwrap();
	let item = s.root_record(Some(bad), &[]).unwrap();
	s.root_set_field(&object, "x", &item).unwrap();
	refused(&s, &object, &certificate);
	s.root_set_field(&object, "x", &scalar).unwrap();
	s.validate_format_roots(&[&object], &certificate).unwrap();
	s.set_key(
		object.value,
		super::super::indexing::RecordKey::Static("new"),
		item.value,
	)
	.unwrap();
	refused(&s, &object, &certificate);
}

#[test]
fn clone_and_capture_transfer_preserve_nominal_census() {
	let (hash, bad) = named("Admission0269Copies");
	let certificate = authority(hash);
	let mut s = store();
	let original = s.root_record(Some(bad), &[]).unwrap();
	let cloned = s.clone_value(original.value).unwrap();
	let cloned = s.export(cloned);
	refused(&s, &cloned, &certificate);
	let moved = s.take_capture(original.value).unwrap();
	let moved = s.export(moved);
	// Both the transferred payload and the old Moved(Struct) tag are checked.
	refused(&s, &original, &certificate);
	refused(&s, &moved, &certificate);
	let inert = s.root_record(Some(bad), &[]).unwrap();
	s.invalidate(inert.value).unwrap();
	refused(&s, &inert, &certificate);
}

#[test]
fn dead_unpermitted_type_causes_walk_not_false_refusal() {
	let (hash, bad) = named("Admission0269Dead");
	let certificate = authority(hash);
	let mut s = store();
	let dead = s.root_record(Some(bad), &[]).unwrap();
	s.root_release(dead).unwrap();
	let scalar = s.root_i64(1);
	let safe = s.root_list(Type::Vec, &[&scalar, &scalar]).unwrap();
	Store::format_walk_probe();
	s.validate_format_roots(&[&safe], &certificate).unwrap();
	let (walks, nodes, _) = Store::format_walk_probe();
	assert_eq!((walks, nodes), (1, 1));
	assert_eq!(s.live_entries(), 1);
}

#[test]
fn changed_certificate_and_other_store_do_not_reuse_permissions() {
	let (hash, bad) = named("Admission0269Certificates");
	let denied = authority(hash);
	let permitted = authority(Hash::type_hash(["Admission0269OtherAuthority"]));
	let mut a = store();
	let item = a.root_record(Some(bad), &[]).unwrap();
	a.validate_format_roots(&[&item], &permitted).unwrap();
	refused(&a, &item, &denied);
	let mut b = store();
	let safe = b.root_list(Type::Vec, &[]).unwrap();
	Store::format_walk_probe();
	b.validate_format_roots(&[&safe], &denied).unwrap();
	assert_eq!(Store::format_walk_probe(), (0, 0, 0));
	assert_eq!(
		b.validate_format_roots(&[&item], &permitted)
			.unwrap_err()
			.kind,
		ErrorKind::InvalidHandle
	);
}

#[test]
fn provisional_or_unknown_nominal_forces_exact_validation() {
	let certificate = authority(Hash::type_hash(["Admission0269Unknown"]));
	let mut s = store();
	let bad = s
		.record(Type::Struct(Nominal(nominal::PROVISIONAL)), &[])
		.unwrap();
	let root = s.export(bad);
	Store::format_walk_probe();
	assert_eq!(
		s.validate_format_roots(&[&root], &certificate)
			.unwrap_err()
			.kind,
		ErrorKind::InvalidCode
	);
	assert_eq!(Store::format_walk_probe().0, 1);
}

#[test]
fn unrecorded_payload_admission_panics_before_publication() {
	let (_, n) = named("Admission0269Bypass");
	for list in [false, true] {
		let mut s = store();
		let payload = if list {
			Payload::List {
				tag: Type::Struct(n),
				values: Vec::new(),
			}
		} else {
			Payload::Record {
				tag: Type::Struct(n),
				fields: Vec::new(),
			}
		};
		let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.admit(payload)))
			.unwrap_err();
		let text = error
			.downcast_ref::<&str>()
			.copied()
			.or_else(|| {
				error
					.downcast_ref::<std::string::String>()
					.map(|s| s.as_str())
			})
			.unwrap();
		assert!(text.contains("Struct payload bypassed the constructor census"));
		assert_eq!((s.live_entries(), s.payload_bytes()), (0, 0));
	}
}

#[test]
fn tuple_struct_first_census_allocation_failure_is_visible() {
	let mut staged = nominal::Staging::default();
	staged
		.stage_shape(
			Hash::type_hash(["Admission0269TupleAlloc"]).into_inner(),
			format_args!("Admission0269TupleAlloc"),
			nominal::Shape::Tuple(0),
		)
		.unwrap();
	let mut remap = staged.prepare_remap().unwrap();
	staged.commit(&mut remap).unwrap();
	let mut s = store();
	let before = (
		s.live_entries(),
		s.payload_bytes(),
		s.bookkeeping_bytes().unwrap(),
	);
	let error = rune_alloc::limit::with(0, || s.list(Type::Struct(remap[0]), &[]))
		.call()
		.unwrap_err();
	assert_eq!(error.kind, ErrorKind::Allocation);
	assert!(s.struct_census.is_none());
	assert_eq!(
		(
			s.live_entries(),
			s.payload_bytes(),
			s.bookkeeping_bytes().unwrap()
		),
		before
	);
	let value = s.list(Type::Struct(remap[0]), &[]).unwrap();
	assert!(s.struct_census.as_ref().unwrap().contains(remap[0]));
	s.release(value).unwrap();
}

// This remaining fallback is an embedding boundary: unlike a session's checked
// compiler, the public Store constructor can import an issued nominal that the
// selected program's certificate denies. Measure the same v.len() before and
// after constructing and dropping such a value in that exact Store.
#[test]
#[ignore = "explicit 0269 large-vector residual admission measurement"]
fn dead_type_fallback_costs() {
	use crate as rune;
	use crate::experimental_core::{execute::Executor, frontend, value::Scalar};
	use std::sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	};
	use std::time::Instant;
	let mut c = crate::Context::with_default_modules().unwrap();
	let effects = Arc::new(AtomicUsize::new(0));
	let calls = effects.clone();
	let mut module = crate::Module::with_crate("fallback269").unwrap();
	module
		.host_function("scalar", move || {
			calls.fetch_add(1, Ordering::SeqCst);
			7i64
		})
		.unwrap();
	c.install(module).unwrap();
	let denied = Hash::associated_function(
		Hash::type_hash(crate::item!(::fallback269::scalar)),
		&crate::runtime::Protocol::DISPLAY_FMT,
	);
	assert!(!c.format_snapshot().unwrap().permits(denied));
	let n = nominal::intern(denied.into_inner(), "Admission269Retired").unwrap();
	let mut sources = crate::Sources::new();
	sources
		.insert(crate::Source::memory("pub fn main(v){v.len()}").unwrap())
		.unwrap();
	let program = frontend::compile(&mut sources, &c, &mut crate::Diagnostics::new(), 0).unwrap();
	let entry = program.function(Hash::type_hash(["main"])).unwrap();
	let mut guarded_sources = crate::Sources::new();
	guarded_sources
		.insert(crate::Source::memory("pub fn main(v){fallback269::scalar();v.len()}").unwrap())
		.unwrap();
	let guarded =
		frontend::compile(&mut guarded_sources, &c, &mut crate::Diagnostics::new(), 0).unwrap();
	let guarded_entry = guarded.function(Hash::type_hash(["main"])).unwrap();
	let mut rows = std::vec::Vec::new();
	let mut deep_controls = std::vec::Vec::new();
	for elements in [16, 5_000_000] {
		let mut s = store();
		let mut executor = Executor::new().unwrap();
		executor
			.set_host_runtime(Some(c.execution_runtime().unwrap()))
			.unwrap();
		let mut values = Vec::try_with_capacity(elements).unwrap();
		for _ in 0..elements {
			values.try_push(Value::I64(1)).unwrap();
		}
		let value = s.list(Type::Vec, &values).unwrap();
		drop(values);
		let v = s.export(value);
		let args = [&v];
		for retired in [false, true] {
			if retired {
				let dead = s.root_record(Some(n), &[]).unwrap();
				s.root_release(dead).unwrap();
			}
			for sample in 0..6 {
				Store::format_walk_probe();
				let start = Instant::now();
				let prepared = executor.prepare(&program, &mut s, entry, &args).unwrap();
				let prepare_ns = start.elapsed().as_nanos();
				let start = Instant::now();
				let result = prepared.invoke(1000).unwrap();
				let execution_ns = start.elapsed().as_nanos();
				assert_eq!(
					s.root_scalar(&result).unwrap(),
					Some(Scalar::U64(elements as u64))
				);
				s.root_release(result).unwrap();
				let (walks, nodes, types) = Store::format_walk_probe();
				assert_eq!(walks, usize::from(retired));
				assert_eq!(nodes, usize::from(retired));
				if sample > 0 {
					rows.push(serde_json::json!({"elements":elements,"retired_denied_type":retired,"sample":sample,"prepare_ns":prepare_ns,"execution_ns":execution_ns,"walks":walks,"nodes":nodes,"types_checked":types}));
				}
			}
		}
		// Put the denied struct behind both a tuple and an object, at the end
		// of this same large vector. The invocation would call the host effect
		// first if admission accidentally skipped the nested struct.
		let bad = s.root_record(Some(n), &[]).unwrap();
		let tuple = s.root_list(Type::Tuple, &[&bad]).unwrap();
		let nested = s.root_record(None, &[("nested", &tuple)]).unwrap();
		s.root_set(&v, elements - 1, &nested).unwrap();
		s.root_release(nested).unwrap();
		s.root_release(tuple).unwrap();
		s.root_release(bad).unwrap();
		let before_effects = effects.load(Ordering::SeqCst);
		Store::format_walk_probe();
		let failure = executor
			.invoke(&guarded, &mut s, guarded_entry, &args, 1000)
			.unwrap_err();
		assert_eq!(failure.error.refusal, Some(Refusal::Entry));
		assert_eq!(effects.load(Ordering::SeqCst), before_effects);
		let (walks, nodes, types) = Store::format_walk_probe();
		assert_eq!((walks, nodes), (1, 4));
		deep_controls.push(serde_json::json!({"elements":elements,"refusal":"Entry","host_effects":effects.load(Ordering::SeqCst)-before_effects,"walks":walks,"nodes":nodes,"types_checked":types}));
	}
	eprintln!(
		"ENTRY-FALLBACK-JSON {}",
		serde_json::json!({"profile":"engine-dev","input":"v.len()","rows":rows,"deep_denied_controls":deep_controls,"scope":"public Executor/Store embedding; session compiler does not admit construction of this denied type"})
	);
}
