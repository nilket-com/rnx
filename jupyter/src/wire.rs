//! Jupyter 5.4 JSON envelopes. Authentication precedes JSON parsing/dispatch.
//! https://jupyter-client.readthedocs.io/en/stable/messaging.html#the-wire-protocol
use crate::transport::Res;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

const M: usize = 1024 * 1024;
/// Serialize under a byte cap, rather than allocate a whole encoded dictionary
/// and discover its JSON-escaping expansion only at transport admission.
struct JsonBuffer {
	bytes: Vec<u8>,
	cap: usize,
}
impl std::io::Write for JsonBuffer {
	fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
		if bytes.len() > self.cap - self.bytes.len() {
			return Err(std::io::Error::other(
				"encoded Jupyter dictionary exceeds byte budget",
			));
		}
		self.bytes
			.try_reserve_exact(bytes.len())
			.map_err(std::io::Error::other)?;
		self.bytes.extend_from_slice(bytes);
		Ok(bytes.len())
	}
	fn flush(&mut self) -> std::io::Result<()> {
		Ok(())
	}
}
fn dictionary(value: &Value, cap: usize) -> Res<Vec<u8>> {
	let mut output = JsonBuffer { bytes: vec![], cap };
	serde_json::to_writer(&mut output, value)?;
	Ok(output.bytes)
}
const DELIMITER: &[u8] = b"<IDS|MSG>";

/// Never Debug-print this type: it owns the connection-file signing key.
pub struct Codec {
	key: Vec<u8>,
	session: String,
}

/// Borrowed wire header and routing frames retain the request's exact identity.
/// A scheduled operation must acquire application byte credit before copying.
pub struct Request<'a> {
	pub routing: &'a [Vec<u8>],
	pub header_bytes: &'a [u8],
	pub header: Value,
	pub content: Value,
}
impl Request<'_> {
	pub fn kind(&self) -> &str {
		self.header["msg_type"].as_str().expect("validated header")
	}
}
impl Codec {
	pub fn new(key: Vec<u8>) -> Self {
		Self {
			key,
			session: uuid::Uuid::new_v4().to_string(),
		}
	}
	fn signature(&self, frames: &[Vec<u8>]) -> Vec<u8> {
		if self.key.is_empty() {
			return vec![];
		}
		let mut mac =
			Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC accepts any key length");
		for frame in frames {
			mac.update(frame);
		}
		hex::encode(mac.finalize().into_bytes()).into_bytes()
	}
	/// None means unauthenticated: do not send a reply claiming a known sender.
	pub fn decode<'a>(&self, parts: &'a [Vec<u8>]) -> Res<Option<Request<'a>>> {
		if parts.len() > 32
			|| parts
				.iter()
				.try_fold(0usize, |n, p| n.checked_add(p.len()))
				.is_none_or(|n| n > M)
		{
			return Err("Jupyter multipart exceeds 32 parts or 1 MiB".into());
		}
		let d = parts
			.iter()
			.position(|p| p == DELIMITER)
			.ok_or("Jupyter delimiter missing")?;
		if parts.len() < d + 6 {
			return Err("Jupyter envelope is incomplete".into());
		}
		if parts[..d].iter().any(|p| p.len() > 65536)
			|| parts[d + 2..d + 5].iter().any(|p| p.len() > 65536)
		{
			return Err("Jupyter routing/header field exceeds 64 KiB".into());
		}
		let dictionaries = &parts[d + 2..d + 6];
		let signature = &parts[d + 1];
		let valid = if self.key.is_empty() {
			signature.is_empty()
		} else if signature.len() != 64 {
			false
		} else {
			let mut mac =
				Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC accepts any key length");
			for frame in dictionaries {
				mac.update(frame);
			}
			hex::decode(signature)
				.ok()
				.is_some_and(|s| mac.verify_slice(&s).is_ok())
		};
		if !valid {
			return Ok(None);
		}
		if parts.len() != d + 6 {
			return Err("Jupyter binary buffers are unsupported".into());
		}
		let mut values = dictionaries
			.iter()
			.map(|p| serde_json::from_slice::<Value>(p))
			.collect::<Result<Vec<_>, _>>()?;
		if values.iter().any(|v| !v.is_object()) {
			return Err("Jupyter message dictionaries must be objects".into());
		}
		let content = values.pop().unwrap();
		let header = values.remove(0);
		for name in [
			"msg_id", "session", "username", "date", "msg_type", "version",
		] {
			if !header[name].is_string() {
				return Err(format!("Jupyter header {name} must be a string").into());
			}
		}
		Ok(Some(Request {
			routing: &parts[..d],
			header_bytes: &parts[d + 2],
			header,
			content,
		}))
	}
	/// Keep routing and parent bytes supplied by the operation, never a mutable
	/// latest-request header. `date` is supplied by the kernel's clock owner.
	pub fn encode(
		&self,
		routing: &[Vec<u8>],
		parent: &[u8],
		kind: &str,
		date: &str,
		metadata: &Value,
		content: &Value,
	) -> Res<Vec<Vec<u8>>> {
		if !metadata.is_object() || !content.is_object() {
			return Err("Jupyter output requires dictionaries".into());
		}
		if parent.len() > 65536 {
			return Err("parent header exceeds 64 KiB".into());
		}
		if !serde_json::from_slice::<Value>(parent)?.is_object() {
			return Err("Jupyter parent header must be an object".into());
		}
		if parent.len() > 65536 || routing.len() > 26 || routing.iter().any(|p| p.len() > 65536) {
			return Err("outgoing routing/header field exceeds its bound".into());
		}
		// Include worst-case framing headers, delimiter and signature before
		// allocating any dictionary. Encoded output fits transport's 2 MiB cap.
		let fixed = routing.iter().map(Vec::len).sum::<usize>()
			+ parent.len()
			+ DELIMITER.len()
			+ 64 + (routing.len() + 6) * 9;
		let mut available = (2 * M)
			.checked_sub(fixed)
			.ok_or("outgoing Jupyter envelope exceeds byte budget")?;
		let header = dictionary(
			&json!({"msg_id":uuid::Uuid::new_v4().to_string(),"session":self.session,"username":"rnx","date":date,"msg_type":kind,"version":"5.4"}),
			available.min(65536),
		)?;
		available -= header.len();
		let metadata = dictionary(metadata, available.min(65536))?;
		available -= metadata.len();
		let content = dictionary(content, available)?;
		let frames = vec![header, parent.to_vec(), metadata, content];
		let mut out = routing.to_vec();
		out.push(DELIMITER.to_vec());
		out.push(self.signature(&frames));
		out.extend(frames);
		Ok(out)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn frames(codec: &Codec) -> Vec<Vec<u8>> {
		codec
			.encode(
				&[b"route".to_vec()],
				b"{}",
				"execute_request",
				"2026-09-15T00:00:00Z",
				&json!({"extension":true}),
				&json!({"code":"42"}),
			)
			.unwrap()
	}
	#[test]
	fn signs_each_dictionary_and_preserves_parent_bytes() {
		let codec = Codec::new(b"not-a-production-key".to_vec());
		let f = frames(&codec);
		let req = codec.decode(&f).unwrap().unwrap();
		assert_eq!(req.kind(), "execute_request");
		assert_eq!(req.content["code"], "42");
		let reply = codec
			.encode(
				req.routing,
				req.header_bytes,
				"execute_reply",
				"2026-09-15T00:00:01Z",
				&json!({}),
				&json!({"status":"ok"}),
			)
			.unwrap();
		assert_eq!(reply[0], f[0]);
		assert_eq!(reply[4], f[3]);
		for i in 3..7 {
			let mut bad = f.clone();
			bad[i].push(b' ');
			assert!(codec.decode(&bad).unwrap().is_none());
		}
		let other = Codec::new(b"wrong".to_vec());
		assert!(other.decode(&f).unwrap().is_none());
		let mut malformed = f;
		malformed[3] = b"not json".to_vec();
		assert!(codec.decode(&malformed).unwrap().is_none());
	}
	#[test]
	fn empty_key_and_authenticated_shape_refusals() {
		let codec = Codec::new(vec![]);
		let f = frames(&codec);
		assert!(f[2].is_empty());
		assert!(codec.decode(&f).unwrap().is_some());
		let mut bad = f.clone();
		bad[2] = b"signature".to_vec();
		assert!(codec.decode(&bad).unwrap().is_none());
		let mut bad = f.clone();
		bad[6] = b"[]".to_vec();
		assert!(codec.decode(&bad).is_err());
		let mut bad = f.clone();
		bad[3] = b"{}".to_vec();
		assert!(codec.decode(&bad).is_err());
		let mut bad = f.clone();
		bad.push(vec![]);
		assert!(
			codec
				.decode(&bad)
				.err()
				.unwrap()
				.to_string()
				.contains("binary buffers")
		);
		let mut bad = f.clone();
		bad[0] = vec![1; 65537];
		assert!(codec.decode(&bad).is_err());
		let mut bad = f.clone();
		bad[6] = vec![b' '; M];
		assert!(codec.decode(&bad).is_err());
	}
	#[test]
	fn encoding_bounds_json_expansion_before_growth() {
		let codec = Codec::new(vec![]);
		let content = json!({"text":"\u{0}".repeat(M)});
		assert!(
			codec
				.encode(
					&[],
					b"{}",
					"stream",
					"2026-09-15T00:00:00Z",
					&json!({}),
					&content
				)
				.is_err()
		);
		let mut out = JsonBuffer {
			bytes: vec![],
			cap: 17,
		};
		use std::io::Write;
		out.write_all(&[0; 17]).unwrap();
		assert!(out.write_all(&[0]).is_err());
		assert_eq!(out.bytes.len(), 17);
		assert_eq!(out.bytes.capacity(), 17);
	}
}
