//! Small ownership and frame-frontier controls, also run under Miri.
use super::*;
use crate::experimental_core::{error::ErrorKind, execute::ExecutorLimits};

#[cfg(all(debug_assertions, not(miri)))]
#[test]
fn script_await_nesting_has_the_exact_frontier_on_normal_and_headroom_stacks() {
	for bytes in [2_097_152, 1_887_436] {
		let output = std::process::Command::new(std::env::current_exe().unwrap())
			.args([
				"--ignored",
				"--exact",
				"function::host::awaiting::tests::script::script_await_stack_child",
				"--nocapture",
				"--test-threads=1",
			])
			.env("RUNE_0273_STACK_BYTES", bytes.to_string())
			.output()
			.unwrap();
		assert!(
			output.status.success(),
			"stack {bytes}: {}\n{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		assert!(String::from_utf8_lossy(&output.stdout).contains("script-await-frontier 127/128"));
	}
}
#[cfg(all(debug_assertions, not(miri)))]
#[test]
#[ignore = "subprocess for the script-await stack frontier"]
fn script_await_stack_child() {
	let bytes = std::env::var("RUNE_0273_STACK_BYTES")
		.unwrap()
		.parse()
		.unwrap();
	std::thread::Builder::new()
		.stack_size(bytes)
		.spawn(|| {
			let (c, calls) = fixture();
			for depth in [126, 127, 128] {
				let mut expression = "0".to_owned();
				for _ in 0..depth {
					expression = std::format!("id({expression}).await");
				}
				let source = std::format!(
					"async fn id(x){{x}} pub async fn main(s){{let r={expression};(#{{}},r)}}"
				);
				let mut d = ProgramDomain::new(
					Default::default(),
					Default::default(),
					c.execution_runtime().unwrap(),
				)
				.unwrap();
				let mut diagnostics = Diagnostics::new();
				let compiled =
					d.compile_session(&c, &mut sources(&source), &mut diagnostics, 1, &[], &[]);
				if depth < 128 {
					let key = compiled.unwrap();
					let arg = d.record_from_fields(&[]).unwrap();
					let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
					let output = {
						let mut invocation = d.start(&key, entry, &[&arg], 100_000).unwrap();
						match invocation.resume(core::num::NonZeroUsize::new(100_000).unwrap()) {
							CrossOutcome::Complete(v) => v,
							_ => panic!("admitted nesting must complete"),
						}
					};
					let value = d.get(&output, 1).unwrap();
					assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(0)));
				} else {
					let error = compiled.err().unwrap();
					assert!(
						error.refusal == Some(Refusal::DepthLimit)
							|| (error.refusal == Some(Refusal::FrontendDiagnostic)
								&& std::format!("{diagnostics:?}")
									.contains("MaxItemDepth { max: 128 }")),
						"{error:?}: {diagnostics:?}"
					);
				}
				assert_eq!(calls.load(Ordering::SeqCst), 0);
			}
			std::println!("script-await-frontier 127/128");
		})
		.unwrap()
		.join()
		.unwrap();
}

#[test]
fn direct_script_await_keeps_owned_arguments_and_host_effects_once_across_slices() {
	let (c, calls) = fixture();
	let mut d = ProgramDomain::new(
		Default::default(),
		Default::default(),
		c.execution_runtime().unwrap(),
	)
	.unwrap();
	let key=d.compile_session(&c,&mut sources("async fn at(x){x[0]} async fn twice(x){wait270::f(at(x).await).await} pub async fn main(s){let v=[7];let r=twice(v).await;(#{},r)}"),&mut Diagnostics::new(),1,&[],&[]).unwrap();
	let arg = d.record_from_fields(&[]).unwrap();
	let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
	let output = {
		let mut invocation = d.start(&key, entry, &[&arg], 1000).unwrap();
		loop {
			match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Complete(v) => break v,
				CrossOutcome::Failed(e) => panic!("{}", e.message()),
			}
		}
	};
	let value = d.get(&output, 1).unwrap();
	assert_eq!(d.scalar(&value).unwrap(), Some(Scalar::I64(7)));
	assert_eq!(calls.load(Ordering::SeqCst), 1);
	drop(value);
	drop(output);
	drop(arg);
	d.collect_programs().unwrap();
	d.close().unwrap();
}
#[test]
fn refused_script_future_sites_never_execute_host_effects() {
	let (c, calls) = fixture();
	for (tail, kind) in [
		("f()", Refusal::AwaitRequired),
		("let x=f();0", Refusal::AwaitRequired),
		("let x=f;x", Refusal::FutureValue),
		("id(f)", Refusal::FutureValue),
		("id(f())", Refusal::AwaitRequired),
		("ordinary().await", Refusal::Await),
	] {
		let mut d = ProgramDomain::new(
			Default::default(),
			Default::default(),
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let source=std::format!("async fn f(){{7}} fn id(x){{x}} fn ordinary(){{7}} pub async fn main(s){{wait270::f(9).await;let r={{{tail}}};(#{{}},r)}}");
		let mut diagnostics = Diagnostics::new();
		let error = d
			.compile_session(&c, &mut sources(&source), &mut diagnostics, 1, &[], &[])
			.err()
			.unwrap();
		assert_eq!(error.refusal, Some(kind), "{tail}: {diagnostics:?}");
		assert!(diagnostics.diagnostics().iter().any(|diag|matches!(diag,crate::diagnostics::Diagnostic::Fatal(f) if f.as_compile_error().is_some_and(|e|e.script_await_refusal().is_some()))));
		assert_eq!(calls.load(Ordering::SeqCst), 0);
		d.close().unwrap();
	}
}
#[test]
fn recursive_script_await_has_an_exact_frame_frontier_and_cleans_failed_calls() {
	let (c, _) = fixture();
	for (depth, expected) in [(2, None), (3, Some(ErrorKind::FrameCapacity))] {
		let mut d = ProgramDomain::new(
			Default::default(),
			ExecutorLimits {
				frames: 4,
				slots: 1024,
			},
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let source=std::format!("async fn f(n){{if n==0{{7}}else{{f(n-1).await}}}} pub async fn main(s){{let r=f({depth}).await;(#{{}},r)}}");
		let key = d
			.compile_session(
				&c,
				&mut sources(&source),
				&mut Diagnostics::new(),
				1,
				&[],
				&[],
			)
			.unwrap();
		let arg = d.record_from_fields(&[]).unwrap();
		let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
		{
			let mut invocation = d.start(&key, entry, &[&arg], 1000).unwrap();
			loop {
				match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
					CrossOutcome::Limited => {}
					CrossOutcome::Complete(v) => {
						assert_eq!(expected, None);
						drop(v);
						break;
					}
					CrossOutcome::Failed(e) => {
						assert_eq!(Some(e.failure.error.kind), expected);
						break;
					}
				}
			}
		}
		// Both the failed invocation's slots and diagnostics have been unwound.
		assert!(d.scalar(&arg).unwrap().is_none());
		let shallow = d
			.compile_session(
				&c,
				&mut sources("pub fn main(s){let r=7;(#{},r)}"),
				&mut Diagnostics::new(),
				2,
				&[],
				&[],
			)
			.unwrap();
		let entry = d.entry(&shallow, Hash::type_hash(["main"])).unwrap();
		{
			let mut invocation = d.start(&shallow, entry, &[&arg], 1000).unwrap();
			loop {
				match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
					CrossOutcome::Limited => {}
					CrossOutcome::Complete(v) => {
						drop(v);
						break;
					}
					CrossOutcome::Failed(e) => panic!("{}", e.message()),
				}
			}
		}
		drop(arg);
		d.collect_programs().unwrap();
		d.close().unwrap();
	}
}
