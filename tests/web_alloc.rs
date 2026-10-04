#![cfg(all(
	feature = "allocation-peak",
	feature = "server-runtime",
	feature = "count-allocations"
))]
use rnx::{Extensions, rune, server::Program};
#[test]
fn refusals_preflight_borrowed_inputs_before_payload_copies() {
	let program = Program::compile_source(
		"web-allocation.rn",
		r#"
	pub fn escape(x) { web::escape_html(x) }
	pub fn form(x) { web::parse_form(x) }
	pub fn response(x) { web::response(200,x[0],"text/plain",x[1]) }
	"#,
		Extensions::none(),
	)
	.unwrap();
	let rt = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	let mut hs = rune::runtime::Object::new();
	hs.insert(
		"z".try_into().unwrap(),
		rune::to_value("bad\n".to_owned()).unwrap(),
	)
	.unwrap();
	let mut huge = rune::runtime::Vec::new();
	for _ in 0..131072 {
		huge.push(rune::to_value("x".to_owned()).unwrap()).unwrap();
	}
	let mut hv = rune::runtime::Object::new();
	hv.insert("x".try_into().unwrap(), rune::to_value(huge).unwrap())
		.unwrap();
	let inputs = [
		(
			"escape",
			rune::to_value("&".repeat((1 << 20) / 5 + 1)).unwrap(),
		),
		("form", rune::to_value("&".repeat(131072)).unwrap()),
		(
			"form",
			rune::to_value(format!("a={}&z=%FF", "x".repeat(900000))).unwrap(),
		),
		(
			"response",
			rune::to_value(("x".repeat(1 << 20), hs)).unwrap(),
		),
		(
			"response",
			rune::to_value(("x".repeat(1 << 20), hv)).unwrap(),
		),
	];
	for (handler, input) in inputs {
		let slot = program.slot(Extensions::none()).unwrap();
		let mut call = slot
			.prepare(handler, vec![input.clone()], 100000)
			.map_err(|(e, _)| e)
			.unwrap();
		rnx::allocation::reset_peak();
		let before = rnx::allocation::live().unwrap();
		let output = rt.block_on(call.run());
		let peak = rnx::allocation::peak().saturating_sub(before);
		if handler == "escape" {
			let failure = output.expect_err("escaping overflow must raise a VM error");
			assert!(
				failure.to_string().contains("escape_html output"),
				"{failure}"
			);
		} else {
			let output = output.unwrap();
			assert!(
				output
					.borrow_ref::<Result<rune::Value, rune::Value>>()
					.unwrap()
					.is_err(),
				"{handler}"
			);
		}
		assert!(peak < 256 * 1024, "{handler}: {peak} bytes allocated");
		match call.close() {
			Ok(slot) => drop(slot),
			Err((failure, slot)) => {
				assert!(
					handler == "escape" && failure.category() == "vm",
					"{failure}"
				);
				drop(slot);
			}
		}
		println!("{handler}: refused before payload allocation ({peak} bytes)");
	}
}
