//! Explicit native cost matrix; phase observers run outside timed input bodies.
use super::*;

pub(super) fn measure(mut phases: impl FnMut(bool) -> serde_json::Value) -> serde_json::Value {
	let mut rows = Vec::new();
	for (round, engine) in ["execution", "legacy", "legacy", "execution"]
		.into_iter()
		.enumerate()
	{
		for layout in [
			"scalar",
			"integers",
			"functions",
			"closure-first",
			"vector-first",
			"after-drop",
		] {
			let setup = match layout {
				"scalar" => "let v=0;0",
				"integers" => "let v=Vec::with_capacity(5000000);for i in 0..5000000{v.push(i);}0",
				"functions" => {
					"let v={let f=||7;let v=Vec::with_capacity(5000000);for i in 0..5000000{v.push(f);}v};0"
				}
				"vector-first" => {
					"let a=Vec::with_capacity(5000000);for i in 0..5000000{a.push(i);}let z={let n=7;||n};0"
				}
				_ => {
					"let a={let n=7;||n};let z=Vec::with_capacity(5000000);for i in 0..5000000{z.push(i);}0"
				}
			};
			let mut context = Context::with_default_modules().unwrap();
			crate::install_core(&mut context).unwrap();
			if engine == "execution" {
				let mut session = DomainSession::new(context, usize::MAX).unwrap();
				let start = std::time::Instant::now();
				drop(session.eval(setup).unwrap());
				let setup_ns = start.elapsed().as_nanos();
				drop(session.eval("7").unwrap());
				let mut drop_input = serde_json::Value::Null;
				if layout == "after-drop" {
					phases(false);
					let start = std::time::Instant::now();
					drop(session.eval("a=0;0").unwrap());
					let elapsed_ns = start.elapsed().as_nanos();
					drop_input = serde_json::json!({"elapsed_ns":elapsed_ns,"phases":phases(true)});
				}
				for sample in 0..3 {
					phases(false);
					let start = std::time::Instant::now();
					let output = session.eval("7").unwrap();
					let elapsed_ns = start.elapsed().as_nanos();
					let observed = phases(true);
					assert_eq!(
						session.domain.scalar(&output).unwrap(),
						Some(Scalar::I64(7))
					);
					drop(output);
					rows.push(serde_json::json!({"round":round,"engine":engine,"layout":layout,"sample":sample,"setup_ns":setup_ns,"elapsed_ns":elapsed_ns,"phases":observed,"drop_input":drop_input,"bindings":session.binding_count(),"programs":session.retained_units()}));
				}
				session.close().unwrap();
			} else {
				let mut session = Session::new(context).unwrap();
				session.ceiling = usize::MAX;
				let start = std::time::Instant::now();
				drop(session.eval(setup).unwrap());
				let setup_ns = start.elapsed().as_nanos();
				drop(session.eval("7").unwrap());
				let mut drop_input = serde_json::Value::Null;
				if layout == "after-drop" {
					phases(false);
					let start = std::time::Instant::now();
					drop(session.eval("a=0;0").unwrap());
					let elapsed_ns = start.elapsed().as_nanos();
					drop_input = serde_json::json!({"elapsed_ns":elapsed_ns,"phases":phases(true)});
				}
				for sample in 0..3 {
					phases(false);
					let start = std::time::Instant::now();
					let output = session.eval("7").unwrap();
					let elapsed_ns = start.elapsed().as_nanos();
					let observed = phases(true);
					assert_eq!(rune::from_value::<i64>(output).unwrap(), 7);
					rows.push(serde_json::json!({"round":round,"engine":engine,"layout":layout,"sample":sample,"setup_ns":setup_ns,"elapsed_ns":elapsed_ns,"phases":observed,"drop_input":drop_input,"bindings":session.names.len(),"programs":session.units.len()}));
				}
				session.close().unwrap();
			}
		}
	}
	serde_json::json!({"rows":rows,"input":"7","elements":5000000,"profile":"engine-dev","engine_order":["execution","legacy","legacy","execution"]})
}

#[test]
#[ignore = "explicit 0268 large held-data input measurement"]
fn input_costs_with_large_held_data() {
	eprintln!(
		"COLLECTION-COSTS-JSON {}",
		measure(|_| serde_json::Value::Null)
	);
}

// Common notebook inputs with the same held integer data in both engines.
// The observer is outside the timed body and, in the phase probe, asserts
// that none of these inputs traverses the graph after the change.
pub(super) fn measure_common(
	mut phases: impl FnMut(bool) -> serde_json::Value,
) -> serde_json::Value {
	let mut rows = Vec::new();
	for (round, engine) in ["execution", "legacy", "legacy", "execution"]
		.into_iter()
		.enumerate()
	{
		let mut context = Context::with_default_modules().unwrap();
		crate::install_core(&mut context).unwrap();
		crate::interchange::install_test_factory(&mut context).unwrap();
		let mut module = rune::Module::with_crate("collection_host").unwrap();
		module.host_function("scalar", || 7i64).unwrap();
		context.install(module).unwrap();
		let setup = "let v=Vec::with_capacity(5000000);for i in 0..5000000{v.push(i);}let dense=rnx_test::dense();0";
		let cases = [
			("scalar", "7"),
			("len", "v.len()"),
			("index", "v[0]+1"),
			("loop", "let n=0;for i in 0..8{n+=i;}n"),
			("println", "println!(\"collection benchmark\");7"),
			("host", "collection_host::scalar()"),
			("dense", "dense.shape()"),
		];
		if engine == "execution" {
			let mut session = DomainSession::new(context, usize::MAX).unwrap();
			drop(session.eval(setup).unwrap());
			for (name, input) in cases {
				drop(session.eval(input).unwrap());
				for sample in 0..3 {
					phases(false);
					let start = std::time::Instant::now();
					let output = session.eval(input).unwrap();
					let elapsed_ns = start.elapsed().as_nanos();
					let observed = phases(true);
					drop(output);
					rows.push(serde_json::json!({"round":round,"engine":engine,"case":name,"input":input,"sample":sample,"elapsed_ns":elapsed_ns,"phases":observed}));
				}
			}
			session.close().unwrap();
		} else {
			let mut session = Session::new(context).unwrap();
			session.ceiling = usize::MAX;
			drop(session.eval(setup).unwrap());
			for (name, input) in cases {
				drop(session.eval(input).unwrap());
				for sample in 0..3 {
					phases(false);
					let start = std::time::Instant::now();
					let output = session.eval(input).unwrap();
					let elapsed_ns = start.elapsed().as_nanos();
					let observed = phases(true);
					drop(output);
					rows.push(serde_json::json!({"round":round,"engine":engine,"case":name,"input":input,"sample":sample,"elapsed_ns":elapsed_ns,"phases":observed}));
				}
			}
			session.close().unwrap();
		}
	}
	serde_json::json!({"rows":rows,"elements":5000000,"profile":"engine-dev"})
}
#[test]
#[ignore = "explicit 0268 common input cost matrix"]
fn common_input_costs_with_large_held_data() {
	eprintln!(
		"COLLECTION-COMMON-JSON {}",
		measure_common(|_| serde_json::Value::Null)
	);
}
