//! Session-only waiting adapters. No future is admitted as an execution Value.
use super::{HostOutputKind, HostSignature, HostType, OwnedTreeInput};
use crate::function::{Function, Plain, Ref};
use crate::runtime::{Value, VmError};

mod sealed {
	pub trait Callable<A> {}
}

/// Closed waiting signatures with owned input and a terminal error channel.
/// The successful Value must satisfy the enrollment's output certificate.
#[doc(hidden)]
pub trait AwaitHostFunction<A>:
	Function<A, Plain, Return = Result<Value, VmError>> + sealed::Callable<A>
{
	/// Immutable signature used by the unchanged synchronous host bridge.
	fn await_signature() -> HostSignature;
}

macro_rules! signature {
	(($($arg:ty),*), ($($rust:ty),*), [$($kind:expr),*], $n:expr) => {
		impl<F> sealed::Callable<($($arg,)*)> for F where
			F: Fn($($rust),*) -> Result<Value, VmError>
				+ Function<($($arg,)*), Plain, Return = Result<Value, VmError>> {}
		impl<F> AwaitHostFunction<($($arg,)*)> for F where
			F: Fn($($rust),*) -> Result<Value, VmError>
				+ Function<($($arg,)*), Plain, Return = Result<Value, VmError>>
		{
			fn await_signature() -> HostSignature {
				HostSignature { arity: $n, arguments: [$($kind),*], output: HostOutputKind::Dynamic }
			}
		}
	};
}
signature!(
	(i64),
	(i64),
	[
		HostType::I64,
		HostType::Unit,
		HostType::Unit,
		HostType::Unit
	],
	1
);
signature!(
	(Ref<str>),
	(&str),
	[
		HostType::String,
		HostType::Unit,
		HostType::Unit,
		HostType::Unit
	],
	1
);
signature!(
	(Ref<str>, Ref<str>, OwnedTreeInput),
	(&str, &str, OwnedTreeInput),
	[
		HostType::String,
		HostType::String,
		HostType::OwnedTreeInput,
		HostType::Unit
	],
	3
);

#[cfg(test)]
mod tests {
	mod script;
	use super::*;
	use crate::experimental_core::{
		domain::{CrossOutcome, ProgramDomain},
		error::Refusal,
		value::Scalar,
	};
	use crate::{Context, Diagnostics, Hash, Module, Source, Sources};
	use std::sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	};
	#[cfg(all(debug_assertions, not(miri)))]
	use std::{
		borrow::ToOwned,
		string::{String, ToString},
	};
	fn fixture() -> (Context, Arc<AtomicUsize>) {
		let mut c = Context::new();
		let mut m = Module::with_crate("wait270").unwrap();
		let calls = Arc::new(AtomicUsize::new(0));
		let copy = calls.clone();
		m.host_function_await(
			"f",
			|n: i64| async move { n },
			move |n: i64| {
				copy.fetch_add(1, Ordering::SeqCst);
				Ok(Value::from(n))
			},
			crate::host::OutputCertificate {
				nodes: &[crate::host::OutputShape::Scalar(HostType::I64)],
				root: 0,
			},
		)
		.unwrap();
		m.function("unenrolled", |n: i64| async move { n })
			.build()
			.unwrap();
		c.install(m).unwrap();
		(c, calls)
	}
	fn sources(s: &str) -> Sources {
		let mut src = Sources::new();
		src.insert(Source::memory(s).unwrap()).unwrap();
		src
	}
	#[test]
	fn only_a_direct_host_wait_in_a_checked_entry_executes_once_across_slices() {
		let (c, calls) = fixture();
		let mut d = ProgramDomain::new(
			Default::default(),
			Default::default(),
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let key = d
			.compile_session(
				&c,
				&mut sources("pub async fn main(s){let r=wait270::f(7).await;(#{},r)}"),
				&mut Diagnostics::new(),
				0,
				&[],
				&[],
			)
			.unwrap();
		let arg = d.record_from_fields(&[]).unwrap();
		let id = d.entry(&key, Hash::type_hash(["main"])).unwrap();
		let output = {
			let mut call = d.start(&key, id, &[&arg], 1000).unwrap();
			loop {
				match call.resume(core::num::NonZeroUsize::new(1).unwrap()) {
					CrossOutcome::Limited => {}
					CrossOutcome::Complete(v) => break v,
					CrossOutcome::Failed(e) => panic!("{}", e.message()),
				}
			}
		};
		{
			let value = d.get(&output, 1).unwrap();
			assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(7)));
		}
		assert_eq!(calls.load(Ordering::SeqCst), 1);
		drop(output);
		drop(arg);
		d.collect_programs().unwrap();
		d.close().unwrap();
	}
	#[test]
	fn bare_future_value_and_non_session_await_refuse_without_effects() {
		let (c, calls) = fixture();
		for (s, kind) in [
			("pub fn main(){wait270::f(7)}", Refusal::AwaitRequired),
			("pub fn main(){wait270::f}", Refusal::FutureValue),
			("pub fn main(){wait270::unenrolled(7)}", Refusal::Native),
			("pub fn main(){wait270::unenrolled}", Refusal::Native),
			(
				"pub async fn main(){wait270::f(7).await}",
				Refusal::AsyncFunction,
			),
		] {
			let e = crate::experimental_core::frontend::compile(
				&mut sources(s),
				&c,
				&mut Diagnostics::new(),
				0,
			)
			.err()
			.unwrap();
			assert_eq!(e.refusal, Some(kind), "{s}: {e:?}");
		}
		let mut domain = ProgramDomain::new(
			Default::default(),
			Default::default(),
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let error = domain
			.compile_session(
				&c,
				&mut sources("pub async fn main(s){let r=wait270::unenrolled(7).await;(#{},r)}"),
				&mut Diagnostics::new(),
				0,
				&[],
				&[],
			)
			.err()
			.unwrap();
		assert_eq!(error.refusal, Some(Refusal::Native));
		assert_eq!(calls.load(Ordering::SeqCst), 0);
	}
	#[cfg(all(debug_assertions, not(miri)))]
	#[test]
	fn nested_await_arguments_keep_the_normal_stack_and_headroom() {
		const CHILD: &str = "function::host::awaiting::tests::nested_await_stack_child";
		for bytes in [2_usize * 1024 * 1024, 2_usize * 1024 * 1024 * 9 / 10] {
			let result = std::process::Command::new(std::env::current_exe().unwrap())
				.args([
					"--ignored",
					"--exact",
					CHILD,
					"--nocapture",
					"--test-threads=1",
				])
				.env("RUNE_0270_STACK_BYTES", bytes.to_string())
				.output()
				.unwrap();
			assert!(
				result.status.success(),
				"stack {bytes}: {}\n{}",
				String::from_utf8_lossy(&result.stdout),
				String::from_utf8_lossy(&result.stderr)
			);
			assert!(String::from_utf8_lossy(&result.stdout).contains("nested-await-boundary-pass"));
		}
	}

	#[cfg(all(debug_assertions, not(miri)))]
	#[test]
	#[ignore = "subprocess for the nested-await stack-boundary control"]
	fn nested_await_stack_child() {
		let bytes: usize = std::env::var("RUNE_0270_STACK_BYTES")
			.unwrap()
			.parse()
			.unwrap();
		std::thread::Builder::new()
			.stack_size(bytes)
			.spawn(|| {
				let (context, calls) = fixture();
				for shape in ["direct", "block"] {
					let mut expression = "wait270::f(0).await".to_owned();
					let mut last_admitted = None;
					let mut boundary = None;
					for depth in 1..=129 {
						let mut domain = ProgramDomain::new(
							Default::default(),
							Default::default(),
							context.execution_runtime().unwrap(),
						)
						.unwrap();
						let input = format!("pub async fn main(s){{let r={expression};(#{{}},r)}}");
						let mut diagnostics = Diagnostics::new();
						match domain.compile_session(
							&context,
							&mut sources(&input),
							&mut diagnostics,
							0,
							&[],
							&[],
						) {
							Ok(key) => {
								assert_eq!(calls.load(Ordering::SeqCst), 0);
								let arg = domain.record_from_fields(&[]).unwrap();
								let id = domain.entry(&key, Hash::type_hash(["main"])).unwrap();
								let result = {
									let mut invocation =
										domain.start(&key, id, &[&arg], 100_000).unwrap();
									match invocation
										.resume(core::num::NonZeroUsize::new(100_000).unwrap())
									{
										CrossOutcome::Complete(v) => v,
										CrossOutcome::Limited => {
											panic!("nested await exceeded its work budget")
										}
										CrossOutcome::Failed(error) => {
											panic!("nested await failed: {}", error.message())
										}
									}
								};
								let value = domain.get(&result, 1).unwrap();
								assert_eq!(domain.scalar(&value).unwrap(), Some(Scalar::I64(0)));
								assert_eq!(calls.swap(0, Ordering::SeqCst), depth);
								last_admitted = Some(depth);
							}
							Err(error) => {
								assert!(
									error.refusal == Some(Refusal::DepthLimit)
										|| (error.refusal == Some(Refusal::FrontendDiagnostic)
											&& format!("{diagnostics:?}")
												.contains("MaxItemDepth { max: 128 }")),
									"depth {depth}: {error:?}: {diagnostics:?}"
								);
								assert_eq!(calls.load(Ordering::SeqCst), 0);
								assert_eq!(last_admitted.map(|v| v + 1), Some(depth));
								boundary = Some(depth);
								break;
							}
						}
						expression = if shape == "direct" {
							format!("wait270::f({expression}).await")
						} else {
							format!("wait270::f({{{expression};0}}).await")
						};
					}
					assert!(last_admitted.is_some_and(|v| v > 0));
					println!("nested-await-boundary-pass {shape}: last={last_admitted:?} refused={boundary:?}");
					let expected = if shape == "direct" {
						(Some(127), Some(128))
					} else {
						(Some(64), Some(65))
					};
					assert_eq!(
						(last_admitted, boundary),
						expected,
						"nesting boundary changed for {shape}"
					);
				}
			})
			.unwrap()
			.join()
			.unwrap();
	}
	#[test]
	fn terminal_adapter_error_releases_call_owned_input_once() {
		let mut c = Context::new();
		let mut m = Module::with_crate("wait270").unwrap();
		m.host_function_await(
			"f",
			|_s: &str| async { 0i64 },
			|_s: &str| Err(VmError::panic("terminal wait control")),
			crate::host::OutputCertificate {
				nodes: &[crate::host::OutputShape::Scalar(HostType::I64)],
				root: 0,
			},
		)
		.unwrap();
		c.install(m).unwrap();
		let mut d = ProgramDomain::new(
			Default::default(),
			Default::default(),
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let empty = d.live_counts();
		let key = d
			.compile_session(
				&c,
				&mut sources(
					"pub async fn main(s){let r=wait270::f(\"owned snapshot\").await;(#{},r)}",
				),
				&mut Diagnostics::new(),
				0,
				&[],
				&[],
			)
			.unwrap();
		let arg = d.record_from_fields(&[]).unwrap();
		let id = d.entry(&key, Hash::type_hash(["main"])).unwrap();
		{
			let mut call = d.start(&key, id, &[&arg], 1000).unwrap();
			assert!(matches!(
				call.resume(core::num::NonZeroUsize::new(1000).unwrap()),
				CrossOutcome::Failed(_)
			));
		}
		drop(arg);
		d.collect_programs().unwrap();
		assert_eq!(d.live_counts(), empty);
		d.close().unwrap();
	}
	#[test]
	fn prefixed_owned_options_are_snapshotted_and_released_through_the_existing_abi() {
		let mut c = Context::new();
		let mut m = Module::with_crate("wait270").unwrap();
		let calls = Arc::new(AtomicUsize::new(0));
		let copy = calls.clone();
		m.host_function_await(
			"request",
			|_method: &str, _url: &str, _options: Value| async { 0i64 },
			move |method: &str, url: &str, options: OwnedTreeInput| {
				assert_eq!((method, url), ("GET", "local"));
				let node = options.node(0).unwrap();
				assert!(matches!(node.kind(), crate::host::TreeKind::Object));
				assert_eq!(node.len(), 1);
				copy.fetch_add(1, Ordering::SeqCst);
				Ok(Value::from(7i64))
			},
			crate::host::OutputCertificate {
				nodes: &[crate::host::OutputShape::Scalar(HostType::I64)],
				root: 0,
			},
		)
		.unwrap();
		c.install(m).unwrap();
		let mut d = ProgramDomain::new(
			Default::default(),
			Default::default(),
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let before = d.live_counts();
		let key=d.compile_session(&c,&mut sources("pub async fn main(s){let r=wait270::request(\"GET\",\"local\",#{timeout_ms:7}).await;(#{},r)}"),&mut Diagnostics::new(),0,&[],&[]).unwrap();
		let arg = d.record_from_fields(&[]).unwrap();
		let id = d.entry(&key, Hash::type_hash(["main"])).unwrap();
		let out = {
			let mut call = d.start(&key, id, &[&arg], 1000).unwrap();
			match call.resume(core::num::NonZeroUsize::new(1000).unwrap()) {
				CrossOutcome::Complete(out) => out,
				_ => panic!("owned input did not complete"),
			}
		};
		{
			let value = d.get(&out, 1).unwrap();
			assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(7)));
		}
		assert_eq!(calls.load(Ordering::SeqCst), 1);
		drop(out);
		drop(arg);
		d.collect_programs().unwrap();
		assert_eq!(d.live_counts(), before);
		d.close().unwrap();
	}
}
