//! Ownership controls for the directly awaited block boundary, also run by Miri.
use super::*;
use crate::experimental_core::{domain::DomainRoot, error::ErrorKind, execute::ExecutorLimits};

#[test]
fn non_session_blocks_keep_the_existing_refusal_without_a_session_only_remedy() {
	let (c, calls) = fixture();
	for source in ["pub fn main(){async{7}}", "pub fn main(){async move{7}}"] {
		let mut diagnostics = Diagnostics::new();
		let error = crate::experimental_core::frontend::compile(
			&mut sources(source),
			&c,
			&mut diagnostics,
			0,
		)
		.err()
		.unwrap();
		assert_eq!(error.refusal, Some(Refusal::AsyncBlock));
		assert!(!diagnostics.diagnostics().iter().any(|d| {
			let crate::diagnostics::Diagnostic::Fatal(f) = d else {
				return false;
			};
			f.as_compile_error()
				.is_some_and(|e| e.async_block_value_refusal().is_some())
		}));
	}
	assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[cfg(all(debug_assertions, not(miri)))]
#[test]
fn block_nesting_has_the_same_exact_frontier_on_normal_and_headroom_stacks() {
	for bytes in [2_097_152, 1_887_436] {
		let output = std::process::Command::new(std::env::current_exe().unwrap())
			.args([
				"--ignored",
				"--exact",
				"function::host::awaiting::tests::blocks::block_stack_child",
				"--nocapture",
				"--test-threads=1",
			])
			.env("RUNE_ASYNC_BLOCK_STACK_BYTES", bytes.to_string())
			.output()
			.unwrap();
		assert!(
			output.status.success(),
			"stack {bytes}: {}\n{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		assert!(String::from_utf8_lossy(&output.stdout).contains("async-block-frontier 126/127"));
	}
}
#[cfg(all(debug_assertions, not(miri)))]
#[test]
#[ignore = "subprocess for the async-block nesting frontier"]
fn block_stack_child() {
	let bytes = std::env::var("RUNE_ASYNC_BLOCK_STACK_BYTES")
		.unwrap()
		.parse()
		.unwrap();
	std::thread::Builder::new()
		.stack_size(bytes)
		.spawn(|| {
			let (c, calls) = fixture();
			for in_function in [false, true] {
				for depth in [125, 126, 127, 128] {
					let mut expr = "7".to_owned();
					for _ in 0..depth {
						expr = std::format!("async{{{expr}}}.await");
					}
					let source = if in_function {
						std::format!("async fn f(){{{expr}}} pub async fn main(s){{let r=f().await;(#{{}},r)}}")
					} else {
						std::format!("pub async fn main(s){{let r={expr};(#{{}},r)}}")
					};
					let mut d = ProgramDomain::new(
						Default::default(),
						Default::default(),
						c.execution_runtime().unwrap(),
					)
					.unwrap();
					let mut diagnostics = Diagnostics::new();
					let compiled =
						d.compile_session(&c, &mut sources(&source), &mut diagnostics, 1, &[], &[]);
					if depth < 127 {
						let key = compiled.unwrap();
						let arg = d.record_from_fields(&[]).unwrap();
						let output = complete(&mut d, &key, &arg);
						let result = d.get(&output, 1).unwrap();
						assert_eq!(d.scalar(&result).unwrap(), Some(Scalar::I64(7)));
					} else {
						let error = compiled.err().unwrap();
						assert_eq!(error.refusal, Some(Refusal::FrontendDiagnostic));
						assert!(
							std::format!("{diagnostics:?}").contains("MaxItemDepth { max: 128 }"),
							"{diagnostics:?}"
						);
					}

					assert_eq!(calls.load(Ordering::SeqCst), 0);
					d.collect_programs().unwrap();
					d.close().unwrap();
				}
			}
			std::println!("async-block-frontier 126/127");
		})
		.unwrap()
		.join()
		.unwrap();
}

fn complete(
	d: &mut ProgramDomain,
	key: &crate::experimental_core::domain::ProgramKey,
	arg: &DomainRoot,
) -> DomainRoot {
	let entry = d.entry(key, Hash::type_hash(["main"])).unwrap();
	let mut invocation = d.start(key, entry, &[arg], 10_000).unwrap();
	loop {
		match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
			CrossOutcome::Limited => {}
			CrossOutcome::Complete(value) => return value,
			CrossOutcome::Failed(error) => panic!("{}", error.message()),
		}
	}
}
#[test]
fn captured_bindings_and_heap_roots_survive_nested_waits_and_one_step_slices() {
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
			&mut sources("pub async fn main(s){let v=[7];let n=1;let r=async{n=9;v[0]=8;async{wait270::f(v[0]).await}.await}.await;let out=(n,v[0],r);(#{},out)}"),
			&mut Diagnostics::new(),
			1,
			&[],
			&[],
		)
		.unwrap();
	let arg = d.record_from_fields(&[]).unwrap();
	let output = complete(&mut d, &key, &arg);
	let result = d.get(&output, 1).unwrap();
	for (index, value) in [(0, 1), (1, 8), (2, 8)] {
		let value_root = d.get(&result, index).unwrap();
		assert_eq!(d.scalar(&value_root).unwrap(), Some(Scalar::I64(value)));
	}
	assert_eq!(calls.load(Ordering::SeqCst), 1);
	drop(result);
	drop(output);
	drop(arg);
	d.collect_programs().unwrap();
	d.close().unwrap();
}
#[test]
fn second_take_failure_keeps_the_first_take_and_releases_temporaries() {
	let (c, calls) = fixture();
	let mut d = ProgramDomain::new(
		Default::default(),
		Default::default(),
		c.execution_runtime().unwrap(),
	)
	.unwrap();
	let maker = d
		.compile_session(
			&c,
			&mut sources("pub fn main(s){let r=[7];(#{},r)}"),
			&mut Diagnostics::new(),
			0,
			&[],
			&[],
		)
		.unwrap();
	let empty = d.record_from_fields(&[]).unwrap();
	let made = complete(&mut d, &maker, &empty);
	let v = d.get(&made, 1).unwrap();
	drop(made);
	drop(empty);
	let key = d
		.compile_session(
			&c,
			&mut sources("pub async fn main(s){let v=s[\"v\"];let w=v;let r=async move{wait270::f(v[0]+w[0]).await}.await;(#{},r)}"),
			&mut Diagnostics::new(),
			1,
			&[("v", &v)],
			&[],
		)
		.unwrap();
	let arg = d.record_from_fields(&[("v", &v)]).unwrap();
	let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
	{
		let mut invocation = d.start(&key, entry, &[&arg], 1000).unwrap();
		loop {
			match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Failed(e) => {
					assert_eq!(e.failure.error.kind, ErrorKind::AccessConflict);
					break;
				}
				CrossOutcome::Complete(_) => panic!("two aliases cannot both be taken"),
			}
		}
	}
	assert!(
		d.get(&v, 0).is_err(),
		"completed first take is not rolled back"
	);
	assert_eq!(calls.load(Ordering::SeqCst), 0);
	drop(arg);
	drop(v);
	d.collect_programs().unwrap();
	d.close().unwrap();
}
#[test]
fn dropping_a_sliced_move_block_after_a_wait_keeps_effects_and_unwinds_once() {
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
			&mut sources("pub async fn main(s){let v=[7];let r=async move{wait270::f(v[0]).await;loop{}}.await;(#{},r)}"),
			&mut Diagnostics::new(),
			1,
			&[],
			&[],
		)
		.unwrap();
	let arg = d.record_from_fields(&[]).unwrap();
	let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
	{
		let mut invocation = d.start(&key, entry, &[&arg], 10_000).unwrap();
		while calls.load(Ordering::SeqCst) == 0 {
			assert!(matches!(
				invocation.resume(core::num::NonZeroUsize::new(1).unwrap()),
				CrossOutcome::Limited
			));
		}
	}
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
	let output = complete(&mut d, &shallow, &arg);
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
fn nested_blocks_obey_frame_and_fuel_limits_and_recover_after_failure() {
	let (c, _) = fixture();
	for (frames, fuel, expected) in [
		(2, 1000, ErrorKind::FrameCapacity),
		(16, 1, ErrorKind::FuelExhausted),
	] {
		let mut d = ProgramDomain::new(
			Default::default(),
			ExecutorLimits {
				frames,
				slots: 1024,
			},
			c.execution_runtime().unwrap(),
		)
		.unwrap();
		let key = d
			.compile_session(
				&c,
				&mut sources("pub async fn main(s){let r=async{async{7}.await}.await;(#{},r)}"),
				&mut Diagnostics::new(),
				1,
				&[],
				&[],
			)
			.unwrap();
		let arg = d.record_from_fields(&[]).unwrap();
		let entry = d.entry(&key, Hash::type_hash(["main"])).unwrap();
		{
			let mut invocation = d.start(&key, entry, &[&arg], fuel).unwrap();
			loop {
				match invocation.resume(core::num::NonZeroUsize::new(1).unwrap()) {
					CrossOutcome::Limited => {}
					CrossOutcome::Failed(e) => {
						assert_eq!(e.failure.error.kind, expected);
						break;
					}
					CrossOutcome::Complete(_) => panic!("configured bound must fail"),
				}
			}
		}
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
		let output = complete(&mut d, &shallow, &arg);
		drop(output);
		drop(arg);
		d.collect_programs().unwrap();
		d.close().unwrap();
	}
}
