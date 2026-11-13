//! Explicit native measurements; setup and observations are outside each input timer.
use super::*;
use std::time::Instant;

fn context() -> Context {
	let mut context = Context::with_default_modules().unwrap();
	crate::install_core(&mut context).unwrap();
	crate::interchange::install_test_factory(&mut context).unwrap();
	context
}

pub(super) fn measure(observe: impl FnMut(bool) -> serde_json::Value) -> serde_json::Value {
	measure_sizes(5_000_000, 1_000_000, observe)
}

fn measure_sizes(
	integer_count: usize,
	struct_count: usize,
	mut observe: impl FnMut(bool) -> serde_json::Value,
) -> serde_json::Value {
	let mut rows = Vec::new();
	// Even AB/BA size rounds; equal warmups and sample counts. One quiet process.
	for (round, large) in [false, true, true, false].into_iter().enumerate() {
		for layout in ["integers", "structs"] {
			let n = if large {
				if layout == "structs" {
					struct_count
				} else {
					integer_count
				}
			} else {
				16
			};
			let mut s = DomainSession::new(context(), usize::MAX).unwrap();
			let struct_items = layout == "structs";
			let setup_start = Instant::now();
			if struct_items {
				drop(
					s.eval("struct Admission269Point{x} let older=|v|v[0].x+1;0")
						.unwrap(),
				);
			} else {
				drop(s.eval("let older=|v|v[0]+1;0").unwrap());
			}
			let setup = if struct_items {
				format!(
					"let v=Vec::with_capacity({});for i in 0..{n}{{v.push(Admission269Point{{x:i}});}}let dense=rnx_test::dense();0",
					n + 64
				)
			} else {
				format!(
					"let v=Vec::with_capacity({});for i in 0..{n}{{v.push(i);}}let dense=rnx_test::dense();0",
					n + 64
				)
			};
			drop(s.eval(&setup).unwrap());
			let setup_ns = setup_start.elapsed().as_nanos();
			let cases = [
				("len", "v.len()"),
				("index", if struct_items { "v[0].x+1" } else { "v[0]+1" }),
				(
					"push",
					if struct_items {
						"v.push(Admission269Point{x:7});0"
					} else {
						"v.push(7);0"
					},
				),
				("older", "older(v)"),
			];
			for (name, input) in cases {
				drop(s.eval(input).unwrap());
				for sample in 0..5 {
					observe(false);
					let start = Instant::now();
					let result = s.eval(input).unwrap();
					let elapsed_ns = start.elapsed().as_nanos();
					let phases = observe(true);
					let scalar = s.domain.scalar(&result).unwrap();
					let expected = match name {
						"len" => Scalar::U64(n as u64),
						"push" => Scalar::I64(0),
						_ => Scalar::I64(1),
					};
					assert_eq!(scalar, Some(expected), "{layout} {name}");
					drop(result);
					rows.push(serde_json::json!({"round":round,"layout":layout,"elements":n,"case":name,"input":input,"sample":sample,"elapsed_ns":elapsed_ns,"phases":phases,"setup_ns":setup_ns}));
				}
			}
			s.close().unwrap();
		}
	}
	serde_json::json!({"profile":"engine-dev","size_order":["small","large","large","small"],"warmups":1,"samples":5,"rows":rows,"purpose":"native entry-admission phase sizing, not a release-speed claim; push capacity is reserved outside timing"})
}

#[test]
fn all_measurement_fixtures_produce_the_expected_values() {
	let data = measure_sizes(16, 16, |_| serde_json::Value::Null);
	assert_eq!(data["rows"].as_array().unwrap().len(), 160);
}

#[test]
#[ignore = "explicit 0269 large integer/struct admission measurement"]
fn entry_costs() {
	eprintln!("ENTRY-COSTS-JSON {}", measure(|_| serde_json::Value::Null));
}
