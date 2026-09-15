//! Byte barriers before text decoding. Collection ownership changes only at ack.
use crate::transport::Res;
pub const CAP: usize = 2 * 1024 * 1024;
pub const CHUNK: usize = 16 * 1024;
#[derive(Default)]
pub struct Capture {
	marker: Vec<u8>,
	pending: Vec<u8>,
	data: Vec<u8>,
	sent: usize,
	pub discarded: u64,
	pub ended: bool,
}
impl Capture {
	pub fn new(id: u64, name: &str, nonce: &str) -> Self {
		Self {
			marker: format!("\x1eRNX-WORKER-1:{id}:{name}:{nonce}\x1f").into_bytes(),
			..Self::default()
		}
	}
	fn keep(&mut self, bytes: &[u8]) -> Res<()> {
		let n = bytes.len().min(CAP - self.data.len());
		self.data.try_reserve_exact(n)?;
		self.data.extend_from_slice(&bytes[..n]);
		self.discarded = self
			.discarded
			.checked_add((bytes.len() - n) as u64)
			.ok_or("stream discard count overflow")?;
		Ok(())
	}
	/// Returns bytes after the barrier, which belong to the unassociated interval.
	pub fn feed(&mut self, bytes: &[u8]) -> Res<Vec<u8>> {
		if self.ended {
			return Ok(bytes.to_vec());
		}
		self.pending.extend_from_slice(bytes);
		if let Some(i) = self
			.pending
			.windows(self.marker.len())
			.position(|p| p == self.marker)
		{
			let pending = std::mem::take(&mut self.pending);
			self.keep(&pending[..i])?;
			self.ended = true;
			return Ok(pending[i + self.marker.len()..].to_vec());
		}
		// Retain only an actual marker prefix. Keeping an arbitrary marker-sized
		// tail would hide a short newline-terminated progress message until the cell ends.
		let suffix = (1..=self.pending.len().min(self.marker.len() - 1))
			.rev()
			.find(|&n| self.pending.ends_with(&self.marker[..n]))
			.unwrap_or(0);
		let n = self.pending.len() - suffix;
		let pending = std::mem::take(&mut self.pending);
		self.keep(&pending[..n])?;
		self.pending.extend_from_slice(&pending[n..]);
		Ok(vec![])
	}
	pub fn retained_finished(&self) -> bool {
		(self.data.len() == CAP || self.ended) && self.sent == self.data.len()
	}
	pub fn take(&mut self) -> Vec<u8> {
		let end = (self.sent + CHUNK).min(self.data.len());
		let b = self.data[self.sent..end].to_vec();
		self.sent = end;
		b
	}
}
/// At most three valid prefix bytes cross a publication boundary. Replacement
/// is explicit metadata; this decoder does not change worker byte fidelity.
#[derive(Default)]
pub struct Text {
	pending: Vec<u8>,
	pub replaced: bool,
}
impl Text {
	pub fn decode(&mut self, bytes: &[u8], end: bool) -> String {
		self.pending.extend_from_slice(bytes);
		let mut out = String::new();
		let mut used = 0;
		loop {
			match std::str::from_utf8(&self.pending[used..]) {
				Ok(s) => {
					out.push_str(s);
					used = self.pending.len();
					break;
				}
				Err(e) => {
					out.push_str(
						std::str::from_utf8(&self.pending[used..used + e.valid_up_to()]).unwrap(),
					);
					used += e.valid_up_to();
					if let Some(n) = e.error_len() {
						out.push('\u{fffd}');
						self.replaced = true;
						used += n;
					} else if end {
						out.push('\u{fffd}');
						self.replaced = true;
						used = self.pending.len();
						break;
					} else {
						break;
					}
				}
			}
		}
		self.pending.drain(..used);
		out
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn a_short_progress_line_does_not_wait_for_the_barrier() {
		let mut c = Capture::new(1, "stdout", &"a".repeat(64));
		c.feed(b"progress\n").unwrap();
		assert_eq!(c.take(), b"progress\n");
		assert!(!c.ended);
	}

	#[test]
	fn every_marker_split_and_stale_prefix_is_byte_exact() {
		let nonce = "a".repeat(64);
		let marker = format!("\x1eRNX-WORKER-1:1:stdout:{nonce}\x1f");
		for split in 0..=marker.len() {
			let mut c = Capture::new(1, "stdout", &nonce);
			let source = b"no newline\0\x1eRNX-WORKER-1:0:stdout:stale\x1f\x1eRNX-WORKER";
			c.feed(source).unwrap();
			c.feed(&marker.as_bytes()[..split]).unwrap();
			let mut tail = marker.as_bytes()[split..].to_vec();
			tail.extend(b"late");
			assert_eq!(c.feed(&tail).unwrap(), b"late");
			assert!(c.ended);
			assert_eq!(c.take(), source);
		}
	}
	#[test]
	fn cap_continues_draining_and_utf8_waits_for_boundary() {
		let mut c = Capture::new(1, "stderr", &"b".repeat(64));
		for _ in 0..384 {
			c.feed(&[b'x'; 8192]).unwrap();
		}
		let marker = c.marker.clone();
		c.feed(&marker).unwrap();
		assert_eq!(c.data.len(), CAP);
		assert_eq!(c.discarded, 1024 * 1024);
		let mut t = Text::default();
		assert_eq!(t.decode(&[0xf0, 0x9f], false), "");
		assert_eq!(t.decode(&[0x98, 0x80], false), "😀");
		assert!(!t.replaced);
		assert_eq!(t.decode(&[0xff, 0xc3], false), "�");
		assert_eq!(t.decode(&[], true), "�");
		assert!(t.replaced);
	}
}
