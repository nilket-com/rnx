//! The bytes Python's `json` writes, so receipts and their digests agree
//! with the stage programs that still read and bind them.
//!
//! Two forms are in use: `identity()` hashes `json.dumps(v, sort_keys=True,
//! separators=(',', ':'))`, and `atomic_json` writes `json.dump(v,
//! sort_keys=True, indent=2)` and a newline. Both escape every non-ASCII
//! character and write floats as Python's `repr` does.
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

/// Python's float repr: the nearest shortest string that reads back as `x`,
/// ties to even, fixed notation for 1e-4 <= |x| < 1e16.
fn float(out: &mut String, x: f64) -> Result<()> {
	if !x.is_finite() {
		bail!("receipts refuse a non-finite number");
	}
	if x == 0.0 {
		out.push_str(if x.is_sign_negative() { "-0.0" } else { "0.0" });
		return Ok(());
	}
	// Each precision is correctly rounded, so the first that reads back is
	// both the shortest and the nearest.
	let scientific = (0..17)
		.map(|precision| format!("{x:.precision$e}"))
		.find(|text| text.parse::<f64>() == Ok(x))
		.unwrap_or_else(|| format!("{x:e}"));
	let (mantissa, exponent) = scientific
		.split_once('e')
		.expect("LowerExp writes an exponent");
	let exponent: i32 = exponent.parse()?;
	let (sign, mantissa) = mantissa
		.strip_prefix('-')
		.map_or(("", mantissa), |m| ("-", m));
	let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
	out.push_str(sign);
	let point = exponent + 1;
	if -4 < point && point <= 16 {
		if point <= 0 {
			out.push_str("0.");
			out.extend(std::iter::repeat_n('0', (-point) as usize));
			out.push_str(&digits);
		} else if point as usize >= digits.len() {
			out.push_str(&digits);
			out.extend(std::iter::repeat_n('0', point as usize - digits.len()));
			out.push_str(".0");
		} else {
			out.push_str(&digits[..point as usize]);
			out.push('.');
			out.push_str(&digits[point as usize..]);
		}
	} else {
		out.push_str(&digits[..1]);
		if digits.len() > 1 {
			out.push('.');
			out.push_str(&digits[1..]);
		}
		let _ = write!(
			out,
			"e{}{:02}",
			if exponent < 0 { '-' } else { '+' },
			exponent.abs()
		);
	}
	Ok(())
}

fn string(out: &mut String, text: &str) {
	out.push('"');
	for c in text.chars() {
		match c {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			'\u{8}' => out.push_str("\\b"),
			'\u{c}' => out.push_str("\\f"),
			c if !(' '..='~').contains(&c) => {
				let mut units = [0; 2];
				for unit in c.encode_utf16(&mut units) {
					let _ = write!(out, "\\u{unit:04x}");
				}
			}
			c => out.push(c),
		}
	}
	out.push('"');
}

fn encode(out: &mut String, value: &Value, indent: Option<usize>, level: usize) -> Result<()> {
	let newline = |out: &mut String, level: usize| {
		if let Some(width) = indent {
			out.push('\n');
			out.extend(std::iter::repeat_n(' ', width * level));
		}
	};
	let colon = if indent.is_some() { ": " } else { ":" };
	match value {
		Value::Null => out.push_str("null"),
		Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
		Value::Number(n) => {
			if let Some(i) = n.as_i64() {
				let _ = write!(out, "{i}");
			} else if let Some(u) = n.as_u64() {
				let _ = write!(out, "{u}");
			} else {
				float(out, n.as_f64().expect("a JSON number is i64, u64 or f64"))?;
			}
		}
		Value::String(s) => string(out, s),
		Value::Array(items) if items.is_empty() => out.push_str("[]"),
		Value::Array(items) => {
			out.push('[');
			for (i, item) in items.iter().enumerate() {
				if i > 0 {
					out.push(',');
				}
				newline(out, level + 1);
				encode(out, item, indent, level + 1)?;
			}
			newline(out, level);
			out.push(']');
		}
		Value::Object(map) if map.is_empty() => out.push_str("{}"),
		Value::Object(map) => {
			// Python sorts keys by code point, which UTF-8 byte order matches.
			let mut keys: Vec<&String> = map.keys().collect();
			keys.sort();
			out.push('{');
			for (i, key) in keys.into_iter().enumerate() {
				if i > 0 {
					out.push(',');
				}
				newline(out, level + 1);
				string(out, key);
				out.push_str(colon);
				encode(out, &map[key], indent, level + 1)?;
			}
			newline(out, level);
			out.push('}');
		}
	}
	Ok(())
}

/// `json.dumps(value, sort_keys=True, separators=(',', ':'))`.
pub fn compact(value: &Value) -> Result<String> {
	let mut out = String::new();
	encode(&mut out, value, None, 0)?;
	Ok(out)
}

/// `json.dump(value, sort_keys=True, indent=2)` and a newline.
pub fn pretty(value: &Value) -> Result<String> {
	let mut out = String::new();
	encode(&mut out, value, Some(2), 0)?;
	out.push('\n');
	Ok(out)
}

/// The receipts' `identity()`: the SHA-256 of the compact form.
pub fn identity<T: Serialize + ?Sized>(value: &T) -> Result<String> {
	let value = serde_json::to_value(value)?;
	Ok(hex(&Sha256::digest(compact(&value)?.as_bytes())))
}

pub fn hex(bytes: &[u8]) -> String {
	bytes
		.iter()
		.fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
			let _ = write!(out, "{b:02x}");
			out
		})
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn floats_match_python_repr() {
		for (x, text) in [
			(0.0, "0.0"),
			(-0.0, "-0.0"),
			(3.0, "3.0"),
			(0.1, "0.1"),
			(1e16, "1e+16"),
			(1e15, "1000000000000000.0"),
			(1e-4, "0.0001"),
			(1e-5, "1e-05"),
			(1.5e16, "1.5e+16"),
			(1e22, "1e+22"),
			(5e-324, "5e-324"),
			(1.7976931348623157e308, "1.7976931348623157e+308"),
			// An exact tie between two shortest strings goes to the even digit.
			(1_420_895_685_691_392.3, "1420895685691392.2"),
			(-7.25e-7, "-7.25e-07"),
		] {
			let mut out = String::new();
			float(&mut out, x).unwrap();
			assert_eq!(out, text, "{x:e}");
		}
		assert!(compact(&json!({"x": serde_json::Number::from_f64(1.0)})).is_ok());
	}

	#[test]
	fn strings_escape_like_python() {
		assert_eq!(
			compact(&json!("/tmp/é/😀\u{1}\u{7f}\t\"\\")).unwrap(),
			r#""/tmp/\u00e9/\ud83d\ude00\u0001\u007f\t\"\\""#
		);
	}

	#[test]
	fn layout_matches_both_forms() {
		let value = json!({"b": [1, {}], "a": {"z": [], "y": null}, "é": true});
		assert_eq!(
			compact(&value).unwrap(),
			r#"{"a":{"y":null,"z":[]},"b":[1,{}],"\u00e9":true}"#
		);
		assert_eq!(
			pretty(&value).unwrap(),
			"{\n  \"a\": {\n    \"y\": null,\n    \"z\": []\n  },\n  \"b\": [\n    1,\n    {}\n  ],\n  \"\\u00e9\": true\n}\n"
		);
	}

	/// Python itself decides: a corpus encoded by both must agree byte for byte.
	#[test]
	fn a_corpus_matches_python_json() {
		let script = r#"
import json, random, struct, sys
random.seed(11)
vals = []
for _ in range(20000):
    vals.append(struct.unpack('d', struct.pack('Q', random.getrandbits(64)))[0])
    vals.append(random.random() * 10 ** random.randint(-8, 20))
    vals.append(random.getrandbits(52) / 4)
vals = [{'x': v, 'l': [v, str(v) + '\u00e9\U0001f600']} for v in vals if v == v and abs(v) != float('inf')]
vals.append({'big': 2**63 - 1, 'neg': -2**63, 'u': 2**64 - 1, 'e': {}, 'l': [], 'n': None, 'k': {'\uffff': 1, 'Z': 2}})
for v in vals:
    print(json.dumps([v, json.dumps(v, sort_keys=True, separators=(',', ':')), json.dumps(v, sort_keys=True, indent=2)]))
"#;
		let output = std::process::Command::new("python3")
			.args(["-I", "-B", "-c", script])
			.output()
			.expect("python3");
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		let mut count = 0;
		for line in String::from_utf8(output.stdout).unwrap().lines() {
			let row: Vec<Value> = serde_json::from_str(line).unwrap();
			assert_eq!(compact(&row[0]).unwrap(), row[1].as_str().unwrap());
			assert_eq!(
				pretty(&row[0]).unwrap(),
				format!("{}\n", row[2].as_str().unwrap())
			);
			count += 1;
		}
		assert!(count > 50000, "{count}");
	}
}
