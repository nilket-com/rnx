use super::*;
use crate as rune;
use crate::experimental_core::error::Refusal;

#[derive(crate::Any)]
#[rune(item = ::fixture)]
struct Item;

#[derive(crate::Any)]
#[rune(item = ::fixture)]
struct Sample(i64);

static OUTPUT_DROPS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
impl Drop for Sample {
	fn drop(&mut self) {
		if self.0 == 42 {
			OUTPUT_DROPS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
		}
	}
}

#[test]
fn file_pair_reads_both_inputs_and_keeps_the_new_result() {
	use core::sync::atomic::{AtomicUsize, Ordering};
	static FAILURE: AtomicUsize = AtomicUsize::new(0);
	static CREDIT: AtomicUsize = AtomicUsize::new(0);
	static CALLS: AtomicUsize = AtomicUsize::new(0);
	let mut context = Context::with_default_modules().unwrap();
	let mut module = Module::with_crate("fixture").unwrap();
	module
		.file_path_constructor("load", |_: &str| Ok(Item), |_: &Item| 0)
		.unwrap();
	module
		.file_path_constructor(
			"load",
			|_: &str| Ok(Sample(21)),
			|s: &Sample| {
				if s.0 == 42 && FAILURE.load(Ordering::SeqCst) == 2 {
					2 << 20
				} else {
					16
				}
			},
		)
		.unwrap();
	module
		.file_pair_producer(
			"apply",
			|_: &Item, _: &Sample| -> core::result::Result<Sample, std::string::String> {
				panic!("legacy handler")
			},
			|_: &Item, x: &Sample, credit| {
				CREDIT.store(credit, Ordering::SeqCst);
				if credit < 1024 {
					return Err(crate::host::OpaqueProducerLimit);
				}
				CALLS.fetch_add(1, Ordering::SeqCst);
				if FAILURE.load(Ordering::SeqCst) == 1 {
					return Ok(Err("worker failed".into()));
				}
				Ok(Ok(Sample(x.0 * 2)))
			},
		)
		.unwrap();
	module
		.opaque_method::<Sample, _, _>("value", |x: &Sample| x.0, None)
		.unwrap();
	let mut installer = Installer::new(&mut context);
	installer.install(module).unwrap();
	let mut files = installer.finish().unwrap();
	let compile = |files: &FileModules, body: &str| {
		let source = std::format!("pub fn main(args) {{ let m=fixture::Item::load(args[0])?;let x=fixture::Sample::load(args[0])?; {body} }}");
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(source.as_str()).unwrap())
			.unwrap();
		files.compile(
			&mut sources,
			&context,
			&mut Diagnostics::new(),
			1,
			&mut crate::compile::NoopSourceLoader::default(),
		)
	};
	assert_eq!(
		compile(&files, "let y=m.apply(m)?; y.value()")
			.err()
			.unwrap()
			.refusal,
		Some(Refusal::OpaqueArgument)
	);
	let twice = r#"let y=m.apply(x)?; let z=m.apply(x)?;
		drop(m); drop(x);
		if y.value()!=42 || z.value()!=42 { panic!("output"); } 42"#;
	let FileProgram::Checked(main) = compile(&files, twice).unwrap() else {
		panic!("ordinary file");
	};
	let mut run = main
		.prepare(&["x".into()], Limits::default(), false)
		.unwrap();
	let outcome = run.drive(10000, |_| true, || false).unwrap();
	if let FileOutcome::Failed { message, .. } = outcome {
		panic!("{message}");
	}
	assert!(matches!(outcome, FileOutcome::Complete));
	run.close().unwrap();
	assert_eq!(CALLS.load(Ordering::SeqCst), 2);
	let run_one = |files: &FileModules, body: &str, bytes| {
		let FileProgram::Checked(main) = compile(files, body).unwrap() else {
			panic!("ordinary file")
		};
		let mut run = main
			.prepare(
				&["x".into()],
				Limits {
					bytes,
					..Limits::default()
				},
				false,
			)
			.unwrap();
		let result = run.drive(10000, |_| true, || false).unwrap();
		run.close().unwrap();
		result
	};
	let body = "let y=m.apply(x)?; y.value()";
	let bytes = 1 << 20;
	assert!(matches!(
		run_one(&files, body, bytes),
		FileOutcome::Complete
	));
	let exact = bytes - CREDIT.load(Ordering::SeqCst) + 1024;
	assert!(matches!(
		run_one(&files, body, exact),
		FileOutcome::Complete
	));
	assert_eq!(CREDIT.load(Ordering::SeqCst), 1024);
	let FileOutcome::Failed { error, .. } = run_one(&files, body, exact - 1) else {
		panic!("short credit accepted")
	};
	assert_eq!(
		error.kind,
		crate::experimental_core::error::ErrorKind::StoreCapacity
	);
	assert_eq!(CREDIT.load(Ordering::SeqCst), 1023);
	for (drops, arg) in [("drop(m);", 0), ("drop(x);", 1), ("drop(m);drop(x);", 0)] {
		let source = std::format!("{drops}{body}");
		let FileOutcome::Failed { error, .. } = run_one(&files, &source, bytes) else {
			panic!("moved input accepted")
		};
		use crate::experimental_core::error::{Detail, MovedAccess};
		assert_eq!(
			error.detail,
			Detail::MovedArgument {
				arg,
				access: MovedAccess::Read
			}
		);
	}
	let method = crate::Hash::associated_function(
		<Item as crate::TypeHash>::HASH,
		crate::Hash::ident("apply"),
	);
	let FileProgram::Checked(mut forged) = compile(&files, body).unwrap() else {
		panic!("ordinary file")
	};
	Arc::get_mut(&mut forged.runtime.inner)
		.unwrap()
		.opaque
		.producer_outputs
		.get_mut(&method)
		.unwrap()
		.id = core::any::TypeId::of::<Item>();
	let mut bad = forged
		.prepare(&["x".into()], Limits::default(), false)
		.unwrap();
	let FileOutcome::Failed { error, .. } = bad.drive(10000, |_| true, || false).unwrap() else {
		panic!("forged descriptor admitted")
	};
	assert_eq!(
		error.kind,
		crate::experimental_core::error::ErrorKind::InvalidCode
	);
	bad.close().unwrap();
	assert_eq!(CALLS.load(Ordering::SeqCst), 4);

	let before = OUTPUT_DROPS.load(Ordering::SeqCst);
	FAILURE.store(1, Ordering::SeqCst);
	let FileProgram::Checked(main) = compile(&files, body).unwrap() else {
		panic!("ordinary file")
	};
	let mut failed = main
		.prepare(&["x".into()], Limits::default(), false)
		.unwrap();
	assert!(matches!(
		failed.drive(10000, |_| true, || false).unwrap(),
		FileOutcome::Complete
	));
	{
		use crate::experimental_core::heap::ViewKind;
		let view = failed.result(Default::default()).unwrap();
		let root = view.root().unwrap();
		assert_eq!(root.kind().unwrap(), ViewKind::Result { ok: false });
		assert_eq!(
			root.child(0).unwrap().kind().unwrap(),
			ViewKind::String("worker failed")
		);
	}
	failed.close().unwrap();
	assert_eq!(OUTPUT_DROPS.load(Ordering::SeqCst), before);
	FAILURE.store(2, Ordering::SeqCst);
	let FileOutcome::Failed { error, .. } = run_one(&files, body, bytes) else {
		panic!("oversized output retained")
	};
	assert_eq!(
		error.kind,
		crate::experimental_core::error::ErrorKind::StoreCapacity
	);
	assert_eq!(OUTPUT_DROPS.load(Ordering::SeqCst), before + 1);
	FAILURE.store(0, Ordering::SeqCst);
	assert!(matches!(
		run_one(&files, body, bytes),
		FileOutcome::Complete
	));
	assert_eq!(OUTPUT_DROPS.load(Ordering::SeqCst), before + 2);

	let binding = files.readers.iter_mut().find(|r| r.hash == method).unwrap();
	binding.original = binding.session.clone();
	assert_eq!(
		compile(&files, body).err().unwrap().refusal,
		Some(Refusal::OpaqueEntry)
	);
	assert_eq!(CALLS.load(Ordering::SeqCst), 7);
}

#[test]
fn file_pair_requires_matching_descriptors_in_either_install_order() {
	#[derive(crate::Any)]
	#[rune(item = ::fixture, name = Sample)]
	struct Impostor;
	let mut producer = Module::with_crate("fixture")
		.unwrap()
		.with_unique("producer");
	producer
		.file_path_constructor("load", |_: &str| Ok(Item), |_: &Item| 0)
		.unwrap();
	producer
		.file_pair_producer(
			"apply",
			|_: &Item, _: &Sample| Ok(Sample(0)),
			|_, _, _| Ok(Ok(Sample(0))),
		)
		.unwrap();
	let mut right = Module::with_crate("fixture")
		.unwrap()
		.with_unique("argument");
	right.opaque_type::<Sample>(|_| 8, |_, _| Ok(())).unwrap();
	let mut wrong = Module::with_crate("fixture")
		.unwrap()
		.with_unique("argument");
	wrong.opaque_type::<Impostor>(|_| 8, |_, _| Ok(())).unwrap();
	for first in [true, false] {
		let mut ctx = Context::new();
		let (a, b) = if first {
			(&producer, &right)
		} else {
			(&right, &producer)
		};
		ctx.install(a).unwrap();
		ctx.install(b).unwrap();
		let mut ctx = Context::new();
		let (a, b) = if first {
			(&producer, &wrong)
		} else {
			(&wrong, &producer)
		};
		ctx.install(a).unwrap();
		assert!(matches!(
			ctx.install(b),
			Err(crate::ContextError::OpaqueType { .. })
		));
	}
}

#[test]
fn file_unary_methods_keep_their_receiver_and_handler_identity() {
	use core::sync::atomic::{AtomicUsize, Ordering};
	static CALLS: AtomicUsize = AtomicUsize::new(0);

	let mut context = Context::with_default_modules().unwrap();
	let mut m = Module::with_crate("fixture").unwrap();
	m.file_path_constructor("load", |_: &str| Ok(Item), |_: &Item| 0)
		.unwrap();
	m.file_path_constructor("load", |_: &str| Ok(Sample(2)), |_: &Sample| 16)
		.unwrap();
	m.file_unary_producer("double", |v: &Sample| {
		CALLS.fetch_add(1, Ordering::SeqCst);
		Ok(Sample(2 * v.0))
	})
	.unwrap();
	m.file_unary_producer("increment", |v: &Sample| Ok(Sample(v.0 + 1)))
		.unwrap();
	m.file_unary_producer("fail", |_: &Sample| Err("worker failed".into()))
		.unwrap();
	m.opaque_method::<Sample, _, _>("value", |v: &Sample| v.0, None)
		.unwrap();
	let mut installer = Installer::new(&mut context);
	installer.install(m).unwrap();
	let mut files = installer.finish().unwrap();
	let compile = |files: &FileModules, body: &str| {
		let src =
			std::format!("pub fn main(args) {{let x=fixture::Sample::load(args[0])?; {body} }}");
		let mut sources = Sources::new();
		sources
			.insert(crate::Source::memory(&src).unwrap())
			.unwrap();
		files.compile(
			&mut sources,
			&context,
			&mut Diagnostics::new(),
			1,
			&mut crate::compile::NoopSourceLoader::default(),
		)
	};
	for (body, reason) in [
		(
			"let other=fixture::Item::load(args[0])?;other.double()?.value()",
			Refusal::OpaqueReceiver,
		),
		(
			"fixture::Sample::double(x)?.value()",
			Refusal::OpaqueReceiver,
		),
	] {
		assert_eq!(compile(&files, body).err().unwrap().refusal, Some(reason));
	}
	let drive = |body: &str| {
		let FileProgram::Checked(main) = compile(&files, body).unwrap() else {
			panic!("not checked")
		};
		let mut run = main
			.prepare(&["x".into()], Limits::default(), false)
			.unwrap();
		let outcome = run.drive(10000, |_| true, || false).unwrap();
		run.close().unwrap();
		outcome
	};
	assert!(matches!(
		drive(
			"let y=x.double()?.increment()?; if y.value()!=5 || x.value()!=2 {panic!(\"value\");} 0"
		),
		FileOutcome::Complete
	));
	let FileOutcome::Failed { error, .. } = drive("drop(x); x.double()?.value()") else {
		panic!("moved")
	};
	use crate::experimental_core::error::{Detail, MovedAccess};
	assert_eq!(
		error.detail,
		Detail::MovedArgument {
			arg: 0,
			access: MovedAccess::Read
		}
	);
	assert_eq!(CALLS.load(Ordering::SeqCst), 1);
	let FileProgram::Checked(main) = compile(&files, "let y=x.fail()?; y.value()").unwrap() else {
		panic!("not checked")
	};
	let mut run = main
		.prepare(&["x".into()], Limits::default(), false)
		.unwrap();
	assert!(matches!(
		run.drive(10000, |_| true, || false).unwrap(),
		FileOutcome::Complete
	));
	{
		use crate::experimental_core::heap::ViewKind;
		let view = run.result(Default::default()).unwrap();
		let root = view.root().unwrap();
		assert_eq!(root.kind().unwrap(), ViewKind::Result { ok: false });
		assert_eq!(
			root.child(0).unwrap().kind().unwrap(),
			ViewKind::String("worker failed")
		);
	}
	run.close().unwrap();
	let key = |name: &str| {
		Hash::associated_function(<Sample as crate::TypeHash>::HASH, Hash::ident(name))
	};
	let mut missing = crate::alloc::clone::TryClone::try_clone(&context.opaque).unwrap();
	missing.file_unary.remove(&key("double"));
	assert!(missing.validate().is_err());
	let mut forged = crate::alloc::clone::TryClone::try_clone(&context.opaque).unwrap();
	forged.producer_outputs.get_mut(&key("double")).unwrap().id = core::any::TypeId::of::<Item>();
	assert!(forged.validate().is_err());
	let wrong = files
		.readers
		.iter()
		.find(|r| r.hash == key("increment"))
		.unwrap()
		.file
		.clone();
	files
		.readers
		.iter_mut()
		.find(|r| r.hash == key("double"))
		.unwrap()
		.file = wrong;
	assert_eq!(
		compile(&files, "x.double()?.value()")
			.err()
			.unwrap()
			.refusal,
		Some(Refusal::OpaqueEntry)
	);
}
