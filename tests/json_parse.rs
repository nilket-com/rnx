//! Record 0033: the JSON reader's contract, exercised through the script API.
use std::process::Command;

fn eval(source: &str) -> std::process::Output {
	Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["eval", source])
		.output()
		.unwrap()
}

fn result(source: &str) -> String {
	let out = eval(source);
	assert!(
		out.status.success(),
		"{source}: {}",
		String::from_utf8_lossy(&out.stderr)
	);
	String::from_utf8(out.stdout).unwrap().trim_end().to_owned()
}

fn parse_expression(document: &str) -> String {
	format!("json::parse({})", serde_json::to_string(document).unwrap())
}

fn rewritten(document: &str) -> String {
	let printed = result(&format!(
		"json::stringify({}?)?",
		parse_expression(document)
	));
	serde_json::from_str(&printed).unwrap()
}

#[test]
fn integers_keep_their_full_signed_and_unsigned_ranges() {
	for (document, kind) in [
		("0", "i64"),
		("9223372036854775807", "i64"),
		("-9223372036854775808", "i64"),
		("9223372036854775808", "u64"),
		("18446744073709551615", "u64"),
	] {
		assert_eq!(
			result(&format!("{}? is {kind}", parse_expression(document))),
			"true"
		);
	}
	for number in [
		"0",
		"1",
		"-1",
		"9223372036854775807",
		"9223372036854775808",
		"18446744073709551615",
		"-9223372036854775808",
	] {
		assert_eq!(rewritten(number), number);
	}
	for literal in [
		"0",
		"1",
		"-1",
		"9223372036854775807",
		"9223372036854775808u64",
		"18446744073709551615u64",
		"(-9223372036854775807 - 1)",
		"1.5",
		"100.0",
	] {
		assert_eq!(
			result(&format!(
				"let v = {literal}; json::parse(json::stringify(v)?)? == v"
			)),
			"true"
		);
	}
	assert_eq!(
		rewritten("[9223372036854775808,18446744073709551615]"),
		"[9223372036854775808,18446744073709551615]"
	);
}

#[test]
fn floats_and_out_of_range_integers_keep_the_configured_conversion() {
	for (document, expected) in [
		("18446744073709551616", "1.8446744073709552e+19"),
		("-9223372036854775809", "-9.223372036854776e+18"),
		("55527869896048623745833", "5.552786989604862e+22"),
		("1.5", "1.5"),
		("1E2", "100.0"),
		("-0", "-0.0"),
	] {
		assert_eq!(rewritten(document), expected, "{document}");
	}
}

#[test]
fn null_is_unit_and_round_trips_inside_containers() {
	assert_eq!(result("json::parse(\"null\")? == ()"), "true");
	assert_eq!(rewritten("null"), "null");
	assert_eq!(rewritten("[1,null,2]"), "[1,null,2]");
	assert_eq!(rewritten("{\"a\":null}"), "{\"a\":null}");
	assert_eq!(rewritten("[true,false,{},[]]"), "[true,false,{},[]]");
}

#[test]
fn duplicate_keys_take_the_last_value_including_escaped_keys() {
	assert_eq!(rewritten(r#"{"a":1,"a":2}"#), r#"{"a":2}"#);
	assert_eq!(
		rewritten(r#"{"a":1,"\u0061":18446744073709551615}"#),
		r#"{"a":18446744073709551615}"#
	);
}

#[test]
fn unicode_and_escaped_control_characters_survive() {
	let text = "héllo 🦀\n\t\0\"\\";
	let document = serde_json::to_string(text).unwrap();
	let back: String = serde_json::from_str(&rewritten(&document)).unwrap();
	assert_eq!(back, text);
	assert_eq!(rewritten(r#""\ud83e\udd80""#), "\"🦀\"");
}

#[test]
fn refusals_are_catchable_and_name_positions_in_the_document() {
	for (document, reason) in [
		("1e400", "number out of range"),
		("1 2", "trailing characters"),
		("{", "EOF"),
		("nul", "EOF"),
		("[1,]", "trailing comma"),
		(r#""\ud800""#, "hex escape"),
	] {
		let source = format!(
			"match {} {{ Ok(_) => \"unexpected success\", Err(e) => e }}",
			parse_expression(document)
		);
		let message: String = serde_json::from_str(&result(&source)).unwrap();
		assert!(
			message.starts_with("cannot parse JSON document:"),
			"{message}"
		);
		assert!(message.contains(reason), "{message}");
		assert!(message.contains("line 1 column"), "{message}");
	}
	let message = result(&format!(
		"match {} {{ Ok(_) => \"bad\", Err(e) => e }}",
		parse_expression("[\n1,\n]")
	));
	assert!(message.contains("line 3 column 1"), "{message}");
}

#[test]
fn reader_accepts_127_containers_and_refuses_128_without_disabling_its_guard() {
	for objects in [false, true] {
		for depth in [127, 128, 2048] {
			let (open, close) = if objects {
				("{\"a\":", "}")
			} else {
				("[", "]")
			};
			let document = format!("{}0{}", open.repeat(depth), close.repeat(depth));
			let source = format!(
				"match {} {{ Ok(_) => \"accepted\", Err(e) => e }}",
				parse_expression(&document)
			);
			let message: String = serde_json::from_str(&result(&source)).unwrap();
			if depth == 127 {
				assert_eq!(message, "accepted");
			} else {
				assert!(message.contains("recursion limit exceeded"), "{message}");
				assert!(
					message.contains("127 nested arrays or objects"),
					"{message}"
				);
				assert!(message.contains("line 1 column"), "{message}");
			}
		}
	}
}
