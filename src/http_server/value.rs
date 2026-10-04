//! Only owned Rust data crosses the worker boundary. Borrow and bound before copying.
use crate::rune::{self, Value};
use axum::{
	body::Body,
	http::{HeaderName, HeaderValue},
	response::Response,
};
use std::collections::BTreeMap;
pub(super) const BODY: usize = 1 << 20;
pub(super) const HEAD: usize = 16 << 10;
pub(super) const HEADERS: usize = 64;
pub(super) struct Input {
	pub routing: Option<super::routes::Selection>,
	pub method: String,
	pub path: String,
	pub query: Option<String>,
	pub headers: BTreeMap<String, Vec<Vec<u8>>>,
	pub body: Vec<u8>,
}
impl Input {
	pub fn rune(self) -> Result<Value, String> {
		fn put(o: &mut rune::runtime::Object, k: &str, v: Value) -> Result<(), String> {
			o.insert(
				rune::alloc::String::try_from(k).map_err(|e| e.to_string())?,
				v,
			)
			.map_err(|e| e.to_string())?;
			Ok(())
		}
		let err = |e: rune::runtime::RuntimeError| e.to_string();
		let mut o = rune::runtime::Object::new();
		put(&mut o, "method", rune::to_value(self.method).map_err(err)?)?;
		put(&mut o, "path", rune::to_value(self.path).map_err(err)?)?;
		put(
			&mut o,
			"query",
			match self.query {
				Some(q) => rune::to_value(q).map_err(err)?,
				None => rune::to_value(()).map_err(err)?,
			},
		)?;
		if let Some(routing) = self.routing {
			let mut params = rune::runtime::Object::new();
			for (k, v) in routing.params {
				put(&mut params, &k, rune::to_value(v).map_err(err)?)?;
			}
			put(&mut o, "params", rune::to_value(params).map_err(err)?)?;
			put(
				&mut o,
				"allow",
				match routing.allow {
					Some(a) => rune::to_value(a).map_err(err)?,
					None => rune::to_value(()).map_err(err)?,
				},
			)?;
		}
		let mut h = rune::runtime::Object::new();
		for (k, vs) in self.headers {
			let mut values = rune::runtime::Vec::new();
			for v in vs {
				let b = rune::runtime::Bytes::from_slice(v).map_err(|e| e.to_string())?;
				values
					.push(rune::to_value(b).map_err(err)?)
					.map_err(|e| e.to_string())?;
			}
			put(&mut h, &k, rune::to_value(values).map_err(err)?)?;
		}
		put(&mut o, "headers", rune::to_value(h).map_err(err)?)?;
		let b = rune::runtime::Bytes::from_slice(self.body).map_err(|e| e.to_string())?;
		put(&mut o, "body", rune::to_value(b).map_err(err)?)?;
		rune::to_value(o).map_err(err)
	}
}
pub(super) struct Output {
	pub status: u16,
	headers: Vec<(HeaderName, HeaderValue)>,
	body: Vec<u8>,
}
fn bytes(v: &Value, limit: usize) -> Result<Vec<u8>, String> {
	if let Ok(s) = v.borrow_string_ref() {
		if s.len() > limit {
			return Err("response value exceeds its byte limit".into());
		}
		return Ok(s.as_bytes().to_vec());
	}
	let b = v
		.borrow_ref::<rune::runtime::Bytes>()
		.map_err(|_| "response wants a string or bytes".to_owned())?;
	if b.len() > limit {
		return Err("response value exceeds its byte limit".into());
	}
	Ok(b.to_vec())
}
impl Output {
	pub fn error(status: u16) -> Self {
		Self {
			status,
			headers: vec![],
			body: vec![],
		}
	}
	pub fn decode(v: Value) -> Result<Self, String> {
		let o = v
			.borrow_ref::<rune::runtime::Object>()
			.map_err(|_| "response wants an object".to_owned())?;
		if o.len() != 3
			|| !["status", "headers", "body"]
				.iter()
				.all(|k| o.get(*k).is_some())
		{
			return Err("response wants exactly status, headers, body".into());
		}
		let status = o
			.get("status")
			.unwrap()
			.as_integer::<i64>()
			.map_err(|_| "response status wants an integer".to_owned())?;
		if !(200..=599).contains(&status) {
			return Err("response status wants 200 to 599".into());
		}
		let body = bytes(o.get("body").unwrap(), BODY)?;
		if matches!(status, 204 | 304) && !body.is_empty() {
			return Err("response status forbids a body".into());
		}
		let h = o
			.get("headers")
			.unwrap()
			.borrow_ref::<rune::runtime::Object>()
			.map_err(|_| "response headers want an object".to_owned())?;
		if h.len() > HEADERS {
			return Err("too many response header names".into());
		}
		let mut headers = Vec::new();
		let mut total = 64; // Host content-length and framing, including the maximum body length.
		for (key, vs) in h.iter() {
			if key.len() > HEAD - total {
				return Err("response headers exceed 16 KiB".into());
			}
			let name = HeaderName::from_bytes(key.as_bytes())
				.map_err(|_| "invalid response header name".to_owned())?;
			if matches!(
				name.as_str(),
				"content-length"
					| "transfer-encoding"
					| "connection" | "keep-alive"
					| "upgrade" | "trailer"
					| "te" | "proxy-connection"
			) {
				return Err("response header reserved for the transport".into());
			}
			let vs = vs
				.borrow_ref::<rune::runtime::Vec>()
				.map_err(|_| "response header values want a vector".to_owned())?;
			if vs.len() > HEADERS - headers.len() {
				return Err("too many response header values".into());
			}
			for v in vs.iter() {
				let b = bytes(v, HEAD - total)?;
				total += key.len() + b.len() + 4;
				if total > HEAD {
					return Err("response headers exceed 16 KiB".into());
				}
				let value = HeaderValue::from_bytes(&b)
					.map_err(|_| "invalid response header value".to_owned())?;
				headers.push((name.clone(), value));
			}
		}
		Ok(Self {
			status: status as u16,
			headers,
			body,
		})
	}
	pub fn routed(mut self, status: Option<u16>, allow: Option<&str>) -> Result<Self, String> {
		if status.is_some_and(|s| s != self.status) {
			return Err("error hook returned the wrong status".into());
		}
		if let Some(allow) = allow {
			self.headers.retain(|(k, _)| k.as_str() != "allow");
			let total = 64
				+ self
					.headers
					.iter()
					.map(|(k, v)| k.as_str().len() + v.as_bytes().len() + 4)
					.sum::<usize>();
			if self.headers.len() == HEADERS || total + 5 + allow.len() + 4 > HEAD {
				return Err("computed Allow exceeds response header limits".into());
			}
			self.headers.push((
				HeaderName::from_static("allow"),
				HeaderValue::from_bytes(allow.as_bytes())
					.map_err(|_| "invalid computed Allow".to_owned())?,
			));
		}
		Ok(self)
	}
	pub fn response(self, head: bool) -> Response {
		let mut r = Response::builder().status(self.status);
		for (k, v) in self.headers {
			r = r.header(k, v);
		}
		if self.status != 204 && self.status != 304 {
			r = r.header("content-length", self.body.len());
		}
		r.body(if head {
			Body::empty()
		} else {
			Body::from(self.body)
		})
		.expect("prevalidated response")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn result(status: i64, headers: rune::runtime::Object, body: Value) -> Value {
		let mut obj = rune::runtime::Object::new();
		for (k, v) in [
			("status", rune::to_value(status).unwrap()),
			("headers", rune::to_value(headers).unwrap()),
			("body", body),
		] {
			obj.insert(rune::alloc::String::try_from(k).unwrap(), v)
				.unwrap();
		}
		rune::to_value(obj).unwrap()
	}
	fn h(name: &str, values: &[&str]) -> rune::runtime::Object {
		let mut obj = rune::runtime::Object::new();
		let mut v = rune::runtime::Vec::new();
		for s in values {
			v.push(rune::to_value(s.to_string()).unwrap()).unwrap();
		}
		obj.insert(
			rune::alloc::String::try_from(name).unwrap(),
			rune::to_value(v).unwrap(),
		)
		.unwrap();
		obj
	}
	#[test]
	fn routes_fields_and_error_hook_status_allow_are_bounded() {
		let input = Input {
			routing: Some(super::super::routes::Selection {
				handler: Some("hello".into()),
				params: BTreeMap::from([("name".into(), "a/b".into())]),
				status: None,
				allow: None,
			}),
			method: "GET".into(),
			path: "/hello/a%2Fb".into(),
			query: Some("x=1".into()),
			headers: Default::default(),
			body: vec![],
		};
		let v = input.rune().unwrap();
		let o = v.borrow_ref::<rune::runtime::Object>().unwrap();
		assert_eq!(o.len(), 7);
		assert_eq!(
			&*o.get("path").unwrap().borrow_string_ref().unwrap(),
			"/hello/a%2Fb"
		);
		let params = o
			.get("params")
			.unwrap()
			.borrow_ref::<rune::runtime::Object>()
			.unwrap();
		assert_eq!(
			&*params.get("name").unwrap().borrow_string_ref().unwrap(),
			"a/b"
		);
		let out = Output::decode(result(
			405,
			h("allow", &["FORGED"]),
			rune::to_value("method".to_owned()).unwrap(),
		))
		.unwrap()
		.routed(Some(405), Some("GET, HEAD"))
		.unwrap()
		.response(true);
		assert_eq!(out.headers()["allow"], "GET, HEAD");
		assert_eq!(out.headers()["content-length"], "6");
		assert!(hyper::body::Body::is_end_stream(out.body()));
		assert!(Output::error(200).routed(Some(404), None).is_err());
		assert!(
			Output::error(405)
				.routed(Some(405), Some(&"GET".repeat(HEAD)))
				.is_err()
		);
	}

	#[test]
	fn response_is_bounded_and_transport_headers_are_never_accepted() {
		for name in [
			"content-length",
			"CONTENT-LENGTH",
			"transfer-encoding",
			"connection",
			"keep-alive",
			"upgrade",
			"trailer",
			"te",
			"proxy-connection",
			"bad name",
		] {
			assert!(
				Output::decode(result(
					200,
					h(name, &["x"]),
					rune::to_value("".to_owned()).unwrap()
				))
				.is_err()
			);
		}
		for status in [0, 199, 600, 204, 304] {
			assert!(
				Output::decode(result(
					status,
					h("x-a", &["b"]),
					rune::to_value("nonempty".to_owned()).unwrap()
				))
				.is_err()
			);
		}
		assert!(
			Output::decode(result(
				200,
				h("x-a", &["a\r\nb"]),
				rune::to_value("".to_owned()).unwrap()
			))
			.is_err()
		);
		assert!(
			Output::decode(result(
				200,
				h("x-a", &[&"a".repeat(HEAD)]),
				rune::to_value("".to_owned()).unwrap()
			))
			.is_err()
		);
		assert!(
			Output::decode(result(
				200,
				h("x-a", &vec!["x"; 65]),
				rune::to_value("".to_owned()).unwrap()
			))
			.is_err()
		);
		assert!(
			Output::decode(result(
				200,
				h("x-a", &["x"]),
				rune::to_value("x".repeat(BODY + 1)).unwrap()
			))
			.is_err()
		);
		let out = Output::decode(result(
			200,
			h("set-cookie", &["a=1", "b=2"]),
			rune::to_value("hello".to_owned()).unwrap(),
		))
		.unwrap();
		let response = out.response(true);
		assert_eq!(response.headers()["content-length"], "5");
		assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
		assert!(hyper::body::Body::is_end_stream(response.body()));
	}
	#[test]
	fn request_preserves_raw_text_duplicates_bytes_and_unit_query() {
		let input = Input {
			routing: None,
			method: "POST".into(),
			path: "/%E2%9C%93".into(),
			query: None,
			headers: std::collections::BTreeMap::from([(
				"x-a".into(),
				vec![vec![255], b"second".to_vec()],
			)]),
			body: vec![0, 255],
		};
		let v = input.rune().unwrap();
		let o = v.borrow_ref::<rune::runtime::Object>().unwrap();
		assert_eq!(
			&*o.get("path").unwrap().borrow_string_ref().unwrap(),
			"/%E2%9C%93"
		);
		assert_eq!(
			rune::from_value::<()>(o.get("query").unwrap().clone()).unwrap(),
			()
		);
		let h = o
			.get("headers")
			.unwrap()
			.borrow_ref::<rune::runtime::Object>()
			.unwrap();
		let vs = h
			.get("x-a")
			.unwrap()
			.borrow_ref::<rune::runtime::Vec>()
			.unwrap();
		assert_eq!(vs.len(), 2);
		assert_eq!(
			&**vs[0].borrow_ref::<rune::runtime::Bytes>().unwrap(),
			&[255]
		);
		assert_eq!(
			&**o.get("body")
				.unwrap()
				.borrow_ref::<rune::runtime::Bytes>()
				.unwrap(),
			&[0, 255]
		);
	}
}
