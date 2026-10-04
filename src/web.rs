//! Pure, bounded web helpers. Inputs are borrowed; results own their payload.
use crate::host::HostFunction;
use reqwest::header::HeaderName;
use rune::{
	Context, Module, Value,
	runtime::{Bytes, Object, Vec as RuneVec},
};

pub(crate) const BODY: usize = 1 << 20;
pub(crate) const HEAD: usize = 16 << 10;
pub(crate) const HEADERS: usize = 64;
const PAIRS: usize = 1024;

fn fail(e: impl std::fmt::Display) -> String {
	e.to_string()
}
fn bounded(n: usize, max: usize, field: &str) -> Result<(), String> {
	if n > max {
		Err(format!("web: {field} is {n} bytes/items, limit {max}"))
	} else {
		Ok(())
	}
}
fn with_bytes<T>(v: &Value, f: impl FnOnce(&[u8]) -> Result<T, String>) -> Result<T, String> {
	if let Ok(s) = v.borrow_string_ref() {
		return f(s.as_bytes());
	}
	let b = v
		.borrow_ref::<Bytes>()
		.map_err(|_| "web: expected String or Bytes".to_owned())?;
	f(&b)
}
fn escaped(c: char) -> Option<&'static str> {
	match c {
		'&' => Some("&amp;"),
		'<' => Some("&lt;"),
		'>' => Some("&gt;"),
		'"' => Some("&quot;"),
		'\'' => Some("&#39;"),
		_ => None,
	}
}
fn escape_html_checked(s: &str) -> Result<String, String> {
	bounded(s.len(), BODY, "escape_html input")?;
	let mut len = 0usize;
	for c in s.chars() {
		len = len
			.checked_add(escaped(c).map_or(c.len_utf8(), str::len))
			.ok_or("web: escape_html length overflow")?;
		bounded(len, BODY, "escape_html output")?;
	}
	let mut out = String::with_capacity(len);
	for c in s.chars() {
		if let Some(t) = escaped(c) {
			out.push_str(t);
		} else {
			out.push(c);
		}
	}
	Ok(out)
}
// Escaping is an inline template operation. Its data-dependent failure is a cap;
// abort the VM rather than requiring Result plumbing in every interpolation.
fn escape_html(s: &str) -> rune::runtime::VmResult<String> {
	match escape_html_checked(s) {
		Ok(value) => rune::runtime::VmResult::Ok(value),
		Err(error) => rune::runtime::VmResult::panic(error),
	}
}
fn hex(b: u8) -> Option<u8> {
	match b {
		b'0'..=b'9' => Some(b - b'0'),
		b'a'..=b'f' => Some(b - b'a' + 10),
		b'A'..=b'F' => Some(b - b'A' + 10),
		_ => None,
	}
}
struct Decoded<'a> {
	bytes: &'a [u8],
	plus: bool,
}
impl Iterator for Decoded<'_> {
	type Item = u8;
	fn next(&mut self) -> Option<u8> {
		let (&b, rest) = self.bytes.split_first()?;
		if b == b'%'
			&& rest.len() >= 2
			&& let (Some(h), Some(l)) = (hex(rest[0]), hex(rest[1]))
		{
			self.bytes = &rest[2..];
			return Some(h * 16 + l);
		}
		self.bytes = rest;
		Some(if self.plus && b == b'+' { b' ' } else { b })
	}
}
// Validate decoded UTF-8 with a single scalar-sized stack buffer, without copying a component.
fn decoded_len(s: &str, plus: bool) -> Result<usize, String> {
	let mut bytes = Decoded {
		bytes: s.as_bytes(),
		plus,
	};
	let mut len = 0;
	while let Some(b) = bytes.next() {
		let n = match b {
			0..=127 => 1,
			0xc2..=0xdf => 2,
			0xe0..=0xef => 3,
			0xf0..=0xf4 => 4,
			_ => return Err("web: decoded component is not UTF-8".into()),
		};
		let mut scalar = [0; 4];
		scalar[0] = b;
		for slot in &mut scalar[1..n] {
			*slot = bytes.next().ok_or("web: decoded component is not UTF-8")?;
		}
		std::str::from_utf8(&scalar[..n]).map_err(|_| "web: decoded component is not UTF-8")?;
		len += n;
	}
	Ok(len)
}
fn decode(s: &str, plus: bool) -> Result<String, String> {
	let len = decoded_len(s, plus)?;
	let mut out = Vec::with_capacity(len);
	out.extend(Decoded {
		bytes: s.as_bytes(),
		plus,
	});
	String::from_utf8(out).map_err(fail)
}
fn decode_component(s: &str) -> Result<String, String> {
	bounded(s.len(), BODY, "decode_component input")?;
	decode(s, false)
}
fn put(o: &mut Object, key: &str, value: Value) -> Result<(), String> {
	o.insert(rune::alloc::String::try_from(key).map_err(fail)?, value)
		.map_err(fail)?;
	Ok(())
}
fn form(body: Value) -> Result<Value, String> {
	with_bytes(&body, |bytes| {
		bounded(bytes.len(), BODY, "parse_form input")?;
		let s = std::str::from_utf8(bytes).map_err(|_| "web: parse_form body is not UTF-8")?;
		let count = if s.is_empty() {
			0
		} else {
			s.bytes().filter(|b| *b == b'&').count() + 1
		};
		bounded(count, PAIRS, "parse_form pairs")?;
		let mut total = 0usize;
		if count > 0 {
			for pair in s.split('&') {
				let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
				for part in [k, v] {
					total = total
						.checked_add(decoded_len(part, true)?)
						.ok_or("web: parse_form length overflow")?;
					bounded(total, BODY, "parse_form decoded bytes")?;
				}
			}
		}
		let mut out = Object::new();
		if count > 0 {
			for pair in s.split('&') {
				let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
				let k = decode(k, true)?;
				if out.get(k.as_str()).is_none() {
					put(
						&mut out,
						&k,
						rune::to_value(decode(v, true)?).map_err(fail)?,
					)?;
				}
			}
		}
		rune::to_value(out).map_err(fail)
	})
}

/// Shared host/constructor response rules. Grammar checks scan borrowed bytes.
pub(crate) fn status(status: i64, body_len: usize) -> Result<(), String> {
	if !(200..=599).contains(&status) {
		return Err("response status wants 200 to 599".into());
	}
	bounded(body_len, BODY, "response body")?;
	if matches!(status, 204 | 304) && body_len != 0 {
		return Err("response status forbids a body".into());
	}
	Ok(())
}
pub(crate) fn header_name(key: &str) -> Result<HeaderName, String> {
	let name =
		HeaderName::from_bytes(key.as_bytes()).map_err(|_| "invalid response header name")?;
	if matches!(
		name.as_str(),
		"content-length"
			| "transfer-encoding"
			| "connection"
			| "keep-alive"
			| "upgrade"
			| "trailer"
			| "te" | "proxy-connection"
	) {
		return Err("response header reserved for the transport".into());
	}
	Ok(name)
}
pub(crate) fn header_value(bytes: &[u8]) -> Result<(), String> {
	if bytes.iter().all(|b| *b == b'\t' || *b >= 32 && *b != 127) {
		Ok(())
	} else {
		Err("invalid response header value".into())
	}
}
fn copy_payload(v: &Value) -> Result<Value, String> {
	if let Ok(s) = v.borrow_string_ref() {
		return rune::to_value(s.to_owned()).map_err(fail);
	}
	let b = v
		.borrow_ref::<Bytes>()
		.map_err(|_| "web: expected String or Bytes")?;
	rune::to_value(Bytes::from_slice(&*b).map_err(fail)?).map_err(fail)
}
fn header_error(key: &str, index: Option<usize>, error: &str) -> String {
	let mut shown = String::new();
	if crate::format::terminal_safe_into(key, 64, &mut shown) < key.len() {
		shown.push('…');
	}
	let field = index.map_or(String::new(), |i| format!(" value {i}"));
	format!("web::response header {shown:?}{field}: {error}")
}
fn values(v: &Value, mut visit: impl FnMut(&[u8]) -> Result<(), String>) -> Result<(), String> {
	if v.borrow_string_ref().is_ok() || v.borrow_ref::<Bytes>().is_ok() {
		return with_bytes(v, visit);
	}
	let list = v
		.borrow_ref::<RuneVec>()
		.map_err(|_| "web: header values want String, Bytes or a vector")?;
	bounded(list.len(), HEADERS, "response header values")?;
	for v in list.iter() {
		with_bytes(v, &mut visit)?;
	}
	Ok(())
}
fn response(
	status_code: i64,
	body: Value,
	content_type: &str,
	extra: Value,
) -> Result<Value, String> {
	with_bytes(&body, |b| status(status_code, b.len()))?;
	bounded(content_type.len(), HEAD - 64, "response content type")?;
	header_value(content_type.as_bytes())
		.map_err(|e| format!("web::response content_type: {e}"))?;
	let headers = extra
		.borrow_ref::<Object>()
		.map_err(|_| "web: response headers want an object")?;
	bounded(headers.len() + 1, HEADERS, "response header names")?;
	let mut names = Vec::with_capacity(headers.len());
	let mut count = 1usize;
	let mut total = 64 + "content-type".len() + content_type.len() + 4;
	bounded(total, HEAD, "response headers")?;
	for (key, v) in headers.iter() {
		bounded(key.len(), HEAD - total, "response header name")
			.map_err(|e| header_error(key, None, &e))?;
		let name = header_name(key).map_err(|e| header_error(key, None, &e))?;
		if name == "content-type" || names.contains(&name) {
			return Err(header_error(
				key,
				None,
				"duplicate name (including content-type)",
			));
		}
		let mut index = 0;
		values(v, |b| {
			count += 1;
			bounded(count, HEADERS, "response header values")?;
			total = total
				.checked_add(key.len())
				.and_then(|n| n.checked_add(b.len()))
				.and_then(|n| n.checked_add(4))
				.ok_or("web: response header length overflow")?;
			bounded(total, HEAD, "response headers")?;
			header_value(b)?;
			index += 1;
			Ok(())
		})
		.map_err(|e| header_error(key, Some(index), &e))?;
		names.push(name);
	}
	let mut out_headers = Object::new();
	let mut ct = RuneVec::new();
	ct.push(rune::to_value(content_type.to_owned()).map_err(fail)?)
		.map_err(fail)?;
	put(
		&mut out_headers,
		"content-type",
		rune::to_value(ct).map_err(fail)?,
	)?;
	for (key, v) in headers.iter() {
		let mut list = RuneVec::new();
		if v.borrow_string_ref().is_ok() || v.borrow_ref::<Bytes>().is_ok() {
			list.push(copy_payload(v)?).map_err(fail)?;
		} else {
			let inputs = v.borrow_ref::<RuneVec>().map_err(fail)?;
			for input in inputs.iter() {
				list.push(copy_payload(input)?).map_err(fail)?;
			}
		}
		put(
			&mut out_headers,
			&key.to_ascii_lowercase(),
			rune::to_value(list).map_err(fail)?,
		)?;
	}
	let mut out = Object::new();
	put(
		&mut out,
		"status",
		rune::to_value(status_code).map_err(fail)?,
	)?;
	put(
		&mut out,
		"headers",
		rune::to_value(out_headers).map_err(fail)?,
	)?;
	put(&mut out, "body", copy_payload(&body)?)?;
	rune::to_value(out).map_err(fail)
}
fn html(status: i64, body: Value) -> Result<Value, String> {
	response(
		status,
		body,
		"text/html; charset=utf-8",
		rune::to_value(Object::new()).map_err(fail)?,
	)
}
fn text(status: i64, body: Value) -> Result<Value, String> {
	response(
		status,
		body,
		"text/plain; charset=utf-8",
		rune::to_value(Object::new()).map_err(fail)?,
	)
}

pub fn install(context: &mut Context) -> crate::Result<Vec<HostFunction>> {
	let mut module = Module::with_crate("web")?;
	let mut registered = Vec::new();
	macro_rules! register {
		($name:literal,$function:expr,$doc:literal) => {
			module.function($name, $function).build()?;
			registered.push(HostFunction {
				path: concat!("web::", $name).into(),
				doc: $doc,
			});
		};
	}
	register!(
		"escape_html",
		escape_html,
		"escape_html(text) -> String: HTML text/quoted-attribute escaping; &, <, >, quotes escaped again; not JS/CSS/URL validation; input/output <=1 MiB; overflow raises a VM error; borrowed"
	);
	register!(
		"decode_component",
		decode_component,
		"decode_component(text) -> Result<String>: one percent decode, + literal, malformed % literal, invalid UTF-8 refused; input <=1 MiB; do not decode route params twice; borrowed"
	);
	register!(
		"parse_form",
		form,
		"parse_form(String|Bytes) -> Result<Object>: URL-encoded form, + is space, first duplicate wins; empty pairs/names retained; invalid UTF-8 refused including duplicates; <=1 MiB, <=1024 raw pairs; borrowed"
	);
	register!(
		"response",
		response,
		"response(status, String|Bytes body, content_type, headers) -> Result<Object>: canonical response; scalar/vector String|Bytes headers; empty vectors accepted; lowercase names, duplicates/reserved names refused; 200..599; 204/304 empty; body <=1 MiB; <=64 names/values and 16 KiB headers; all inputs borrowed, output independent"
	);
	register!(
		"html",
		html,
		"html(status, String|Bytes body) -> Result<Object>: response with text/html; charset=utf-8, no extra headers; body not escaped; response limits; borrowed"
	);
	register!(
		"text",
		text,
		"text(status, String|Bytes body) -> Result<Object>: response with text/plain; charset=utf-8, no extra headers; response limits; borrowed"
	);
	context.install(module)?;
	Ok(registered)
}

#[cfg(test)]
mod tests {
	use super::*;
	fn run(source: &str) -> Value {
		let mut context = Context::with_default_modules().unwrap();
		install(&mut context).unwrap();
		crate::call(&context, source, Value::empty()).unwrap()
	}
	fn check(body: &str) {
		let v = run(&format!("pub fn main(_) {{ {body} }}"));
		assert!(rune::from_value::<bool>(v).unwrap());
	}
	#[test]
	fn escape_and_decode_exact() {
		assert_eq!(
			escape_html_checked("&<>\"'é\n\0").unwrap(),
			"&amp;&lt;&gt;&quot;&#39;é\n\0"
		);
		assert_eq!(escape_html_checked("&amp;").unwrap(), "&amp;amp;");
		for (s, want) in [
			("%e2%9c%93+", "✓+"),
			("%2f%252f", "/%2f"),
			("%z1%1%", "%z1%1%"),
			("", ""),
			("%00", "\0"),
		] {
			assert_eq!(decode_component(s).unwrap(), want);
		}
		for s in ["%ff", "%c0%af", "%e2%82", "%ed%a0%80", "%f4%90%80%80"] {
			assert!(decode_component(s).is_err());
		}
	}
	#[test]
	fn form_semantics_and_borrowing() {
		check(
			r#"
		let s = "a=first&a=second&plus=a+b&encoded=a%2Bb&equals=a=b&bare&&=last";
		let f = web::parse_form(s).unwrap();
		let b = s.as_bytes(); let fb = web::parse_form(b).unwrap();
		let bad = "a=valid&a=%FF";
		web::parse_form(s).is_ok() && web::parse_form(b).is_ok() &&
		f.a == "first" && fb.a == f.a && f.plus == "a b" && f.encoded == "a+b" &&
		f.equals == "a=b" && f.bare == "" && f[""] == "" &&
		web::parse_form("").unwrap().len() == 0 && web::parse_form(bad).is_err() && bad.len() == 13 && s.len() == b.len()
		"#,
		);
		check(r#"web::parse_form(1).is_err() && web::parse_form("%FF=x").is_err()"#);
	}
	#[test]
	fn responses_canonical_independent_and_borrowed() {
		check(
			r#"
		let body = "hello".as_bytes(); let hs = #{"Cache-Control": "public", "Set-Cookie": ["a=1", "b=2"], "empty": []};
		let r = web::response(200,body,"text/plain",hs).unwrap();
		let scalar = web::response(200,"x","text/plain",#{"x": "v"}).unwrap();
		let vector = web::response(200,"x","text/plain",#{"x": ["v"]}).unwrap();
		let first = scalar.headers.x[0];
		let second = vector.headers.x[0];
		r.body.push(33); r.headers["set-cookie"].push("c=3");
		let bad = #{"a":"valid", "z":"invalid\r\n"};
		let failed = web::response(200,body,"text/plain",bad).is_err();
		body.len() == 5 && hs["Cache-Control"] == "public" && hs["Set-Cookie"].len() == 2 &&
		r.body.len() == 6 && r.headers.empty.len() == 0 && first == second && failed && bad.a == "valid" &&
		web::html(200,"hi").unwrap().headers["content-type"][0] == "text/html; charset=utf-8" &&
		web::text(200,"hi").unwrap().headers["content-type"][0] == "text/plain; charset=utf-8"
		"#,
		);
	}
	#[test]
	fn response_refusals_and_shared_grammar() {
		for body in [
			"web::response(199,\"x\",\"text/plain\",#{})",
			"web::response(600,\"x\",\"text/plain\",#{})",
			"web::html(204,\"x\")",
			"web::text(304,\"x\")",
			"web::response(200,\"x\",\"text/plain\",#{\"Content-Type\":\"x\"})",
			"web::response(200,\"x\",\"text/plain\",#{\"X\":\"a\",\"x\":\"b\"})",
			"web::response(200,\"x\",\"text/plain\",#{\"x\":1})",
			"web::response(200,1,\"text/plain\",#{})",
			"web::response(200,\"x\",\"bad\\n\",#{})",
		] {
			check(&format!("{body}.is_err()"));
		}
		check("web::html(204,\"\").is_ok() && web::text(304,\"\").is_ok()");
		for key in [
			"content-length",
			"Transfer-Encoding",
			"connection",
			"keep-alive",
			"upgrade",
			"trailer",
			"te",
			"proxy-connection",
			"bad name",
			"",
		] {
			assert!(header_name(key).is_err());
		}
		for b in 0..=255u8 {
			assert_eq!(
				header_value(&[b]).is_ok(),
				reqwest::header::HeaderValue::from_bytes(&[b]).is_ok(),
				"{b}"
			);
		}
	}
	#[test]
	fn bounded_before_payload_allocation() {
		assert!(escape_html_checked(&"&".repeat(BODY / 5 + 1)).is_err());
		assert_eq!(escape_html_checked(&"a".repeat(BODY)).unwrap().len(), BODY);
		assert!(escape_html_checked(&"a".repeat(BODY + 1)).is_err());
		let v = rune::to_value("&".repeat(PAIRS)).unwrap();
		assert!(form(v).is_err());
		assert!(decode_component(&"a".repeat(BODY + 1)).is_err());
		assert!(status(200, BODY + 1).is_err());
	}
	#[test]
	fn inline_escape_and_readable_typed_output() {
		check(r#"let s="<&"; web::escape_html(s) == "&lt;&amp;" && s == "<&""#);
		let mut context = Context::with_default_modules().unwrap();
		install(&mut context).unwrap();
		let error = crate::call(
			&context,
			"pub fn main(_) { web::escape_html(1) }",
			Value::empty(),
		)
		.err()
		.unwrap();
		assert!(
			error.to_string().contains("::std::string::String"),
			"{error}"
		);
		check(
			r#"
  let body="hi"; let value="v"; let bytes="b".as_bytes();
  let headers=#{"x-a": value, "x-b": bytes};
  let r=web::response(200,body,"text/plain",headers).unwrap();
  r.body.push_str("!"); r.headers["x-a"][0].push_str("!"); r.headers["x-b"][0].push(33);
  body == "hi" && value == "v" && bytes.len() == 1 &&
  r.body == "hi!" && r.headers["x-a"][0] == "v!" && r.headers["x-b"][0].len() == 2
  "#,
		);
		check(
			r#"match web::response(200,"hi","text/plain",#{"x-a":["v","PRIVATE\r\n"]}) {
    Err(e) => e.contains("web::response") && e.contains("x-a") && e.contains("value 1") && !e.contains("PRIVATE"), _ => false
  }"#,
		);
		let error = header_error(
			&"\u{1b}".repeat(10000),
			Some(2),
			"invalid response header name",
		);
		assert!(error.len() < 200 && !error.contains('\u{1b}'));
	}

	#[test]
	fn cap_boundaries_include_generated_headers() {
		let empty = || rune::to_value(Object::new()).unwrap();
		let body = || rune::to_value("x".to_owned()).unwrap();
		assert!(response(200, body(), &"x".repeat(HEAD - 64 - 12 - 4), empty()).is_ok());
		assert!(response(200, body(), &"x".repeat(HEAD - 64 - 12 - 3), empty()).is_err());
		let headers = |names: usize| {
			let mut o = Object::new();
			for i in 0..names {
				put(
					&mut o,
					&format!("x-{i}"),
					rune::to_value("v".to_owned()).unwrap(),
				)
				.unwrap();
			}
			rune::to_value(o).unwrap()
		};
		assert!(response(200, body(), "x", headers(63)).is_ok());
		assert!(response(200, body(), "x", headers(64)).is_err());
		let headers = |n: usize| {
			let mut o = Object::new();
			let mut vs = RuneVec::new();
			for _ in 0..n {
				vs.push(rune::to_value("v".to_owned()).unwrap()).unwrap();
			}
			put(&mut o, "x", rune::to_value(vs).unwrap()).unwrap();
			rune::to_value(o).unwrap()
		};
		assert!(response(200, body(), "x", headers(63)).is_ok());
		assert!(response(200, body(), "x", headers(64)).is_err());
		assert!(form(rune::to_value("a&".repeat(PAIRS - 1) + "a").unwrap()).is_ok());
		assert!(form(rune::to_value("a&".repeat(PAIRS) + "a").unwrap()).is_err());
		check(
			r#"let body="hi"; let mime="text/plain"; web::response(200,body,mime,#{}).unwrap(); web::response(200,body,mime,#{"x":"bad\n"}).is_err() && body == "hi" && mime == "text/plain""#,
		);
	}

	#[test]
	fn registrations_are_documented() {
		let mut c = Context::with_default_modules().unwrap();
		let functions = install(&mut c).unwrap();
		assert_eq!(functions.len(), 6);
		for f in functions {
			assert!(f.doc.contains(if f.path == "web::escape_html" {
				"-> String"
			} else {
				"Result<"
			}));
			assert!(f.doc.contains("borrowed"));
		}
	}
}
