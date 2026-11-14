//! Trusted original identity, not a name or a documentation flag, grants await.
use super::*;
use crate::host::{HostType, OutputCertificate, OutputShape};
use crate::Value;
fn certificate() -> OutputCertificate {
	OutputCertificate {
		nodes: &[OutputShape::Scalar(HostType::I64)],
		root: 0,
	}
}
#[test]
fn awaited_alternate_is_bound_to_installed_original_and_keeps_abi() {
	let mut c = Context::new();
	let mut m = Module::with_crate("wait270").unwrap();
	m.host_function_await(
		"f",
		|n: i64| async move { n },
		|n: i64| Ok(Value::from(n)),
		certificate(),
	)
	.unwrap();
	c.install(m).unwrap();
	let key = Hash::type_hash(crate::item!(::wait270::f));
	assert!(c.host_requires_await(key) && c.host_is_wait_adapter(key));
	assert!(!c
		.host(key)
		.unwrap()
		.handler
		.same_allocation(c.functions.get(&key).unwrap()));
	assert!(c
		.execution_runtime()
		.unwrap()
		.inner
		.function(&key)
		.unwrap()
		.same_allocation(&c.host(key).unwrap().handler));
	assert!(c
		.runtime()
		.unwrap()
		.function(&key)
		.unwrap()
		.same_allocation(c.functions.get(&key).unwrap()));
	c.functions
		.try_insert(key, FunctionHandler::new(|_, _, _, _| Ok(())).unwrap())
		.unwrap();
	assert!(!c.host_requires_await(key) && !c.host_is_wait_adapter(key));
	assert!(c.host(key).is_none());
	assert_eq!(core::mem::size_of::<crate::host::HostSignature>(), 7);
	assert_eq!(
		core::mem::size_of::<crate::function::host::HostBinding>(),
		56
	);
}
#[test]
fn a_doc_flag_or_failed_install_cannot_publish_await_permission() {
	let mut c = Context::new();
	let mut m = Module::with_crate("wait270").unwrap();
	m.function("f", |n: i64| n).build().unwrap().is_async(true);
	c.install(m).unwrap();
	let key = Hash::type_hash(crate::item!(::wait270::f));
	assert!(!c.host_requires_await(key));
	let mut m = Module::with_crate("wait270").unwrap();
	m.host_function_await(
		"f",
		|n: i64| async move { n },
		|n: i64| Ok(Value::from(n)),
		certificate(),
	)
	.unwrap();
	assert!(c.install(m).is_err());
	assert!(!c.host_requires_await(key));
	assert!(!c.host_is_wait_adapter(key));
}
#[test]
fn an_invalid_wait_output_certificate_does_not_enroll_the_adapter() {
	let mut c = Context::new();
	let mut m = Module::with_crate("wait270").unwrap();
	m.host_function_await(
		"f",
		|n: i64| async move { n },
		|n: i64| Ok(Value::from(n)),
		OutputCertificate {
			nodes: &[OutputShape::Unsupported(Hash::EMPTY)],
			root: 0,
		},
	)
	.unwrap();
	c.install(m).unwrap();
	let key = Hash::type_hash(crate::item!(::wait270::f));
	assert!(c.host_requires_await(key));
	assert!(!c.host_is_wait_adapter(key));
	assert!(c.host(key).is_none());
}
