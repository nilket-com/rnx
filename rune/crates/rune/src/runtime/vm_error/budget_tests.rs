use super::*;
use crate::runtime::{budget, Address, Memory, Output};
use crate::{Context, Diagnostics, Module, Source, Sources, Value, Vm};

fn halt(_: &mut dyn Memory, _: Address, _: usize, _: Output) -> Result<(), VmError> {
	Err(VmError::budget_exhausted())
}

fn run(source: &str, work: usize) -> Result<Value, VmError> {
	let mut context = Context::with_default_modules().unwrap();
	let mut module = Module::new();
	module.raw_function("host_halt", halt).build().unwrap();
	module
		.function("ordinary_error", || Err::<i64, i64>(42))
		.build()
		.unwrap();
	context.install(module).unwrap();
	let mut sources = Sources::new();
	sources
		.insert(Source::new("budget-host.rn", source).unwrap())
		.unwrap();
	let mut diagnostics = Diagnostics::new();
	let unit = crate::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	assert!(unit.is_ok(), "{diagnostics:?}");
	let mut vm = Vm::new(
		Arc::try_new(context.runtime().unwrap()).unwrap(),
		Arc::try_new(unit.unwrap()).unwrap(),
	);
	budget::with(work, || vm.call(["main"], ())).call()
}

#[test]
fn host_budget_constructor_matches_real_vm_and_native_halts() {
	let host = VmError::budget_exhausted();
	let actual = run("pub fn main() { loop {} }", 17).unwrap_err();
	let native = budget::with(0, budget::permit).call().unwrap_err();
	for error in [&host, &actual, &native] {
		assert!(error.is_budget_exhausted());
		assert!(matches!(
			error.inner.error.kind,
			VmErrorKind::Halted {
				halt: VmHaltInfo::Limited
			}
		));
		assert_eq!(std::format!("{error}"), std::format!("{host}"));
		assert_eq!(
			std::format!("{:?}", error.inner.error.kind),
			std::format!("{:?}", host.inner.error.kind)
		);
		#[cfg(feature = "execution")]
		assert!(error.error_class().same_class(host.error_class()));
	}
	// Both existing halt paths construct an unlocated error. Host-call
	// propagation adds its own actual trace in the separate control below.
	assert_eq!(std::format!("{host:?}"), std::format!("{native:?}"));
	assert_eq!(std::format!("{host:?}"), std::format!("{actual:?}"));
	assert!(host.stacktrace().is_empty());
}

#[test]
fn budget_predicate_checks_the_root_kind_only() {
	for other in [
		VmError::overflow(),
		VmError::panic("Exhausted the budget it was given"),
		VmError::from(VmErrorKind::Halted {
			halt: VmHaltInfo::Yielded,
		}),
		VmError::from(VmErrorKind::Halted {
			halt: VmHaltInfo::Awaited,
		}),
	] {
		assert!(!other.is_budget_exhausted());
	}
	assert!(VmError::budget_exhausted()
		.with_error(VmErrorKind::BadArgument { arg: 0 })
		.is_budget_exhausted());
	assert!(!VmError::panic("root")
		.with_error(VmErrorKind::Halted {
			halt: VmHaltInfo::Limited
		})
		.is_budget_exhausted());
	budget::with(11, || {
		let error = VmError::budget_exhausted();
		assert!(error.is_budget_exhausted());
		assert_eq!(budget::remaining(), Some(11));
	})
	.call();
}

#[test]
fn host_budget_halt_cannot_be_caught_as_a_script_result() {
	for source in [
		"pub fn main() { host_halt(); 99 }",
		"pub fn main() { host_halt()?; Ok(99) }",
		"fn inner() { host_halt()?; Ok(1) } pub fn main() { match inner() { Ok(_) => 99, Err(_) => 77 } }",
		"pub fn main() { match host_halt() { Ok(_) => 99, Err(_) => 77 } }",
	] {
		let error = run(source, 1000).unwrap_err();
		assert!(error.is_budget_exhausted(), "{error:?}");
		assert!(!error.stacktrace().is_empty());
		assert!(error.logical_frames().next().unwrap().location.is_some());
	}
	let caught = run(
		"pub fn main() { match ordinary_error() { Ok(_) => 99, Err(_) => 77 } }",
		1000,
	)
	.unwrap();
	assert_eq!(crate::from_value::<i64>(caught).unwrap(), 77);
}
