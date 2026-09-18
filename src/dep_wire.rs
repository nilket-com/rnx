//! Private dependency transition protocol: network-order bounded TLV, no dependency.
#![allow(dead_code)] // Shared framing operations are split between the two endpoints.
use std::collections::BTreeMap;
use std::io::{Read, Write};
pub const LIMIT: usize = 65536;
pub type Fields = BTreeMap<u8, String>;
pub fn encode(kind: u8, fields: &Fields) -> Result<Vec<u8>, String> {
	let mut body = vec![1, kind];
	for (tag, text) in fields {
		if *tag == 0 || *tag > 12 {
			return Err("unknown field".into());
		}
		if body.len() + 5 + text.len() > LIMIT {
			return Err("frame exceeds 65536 bytes".into());
		}
		body.push(*tag);
		body.extend((text.len() as u32).to_be_bytes());
		body.extend(text.as_bytes());
	}
	let mut out = (body.len() as u32).to_be_bytes().to_vec();
	out.extend(body);
	Ok(out)
}
pub fn decode(body: &[u8]) -> Result<(u8, Fields), String> {
	if body.len() < 2 || body.len() > LIMIT || body[0] != 1 {
		return Err("bad frame/version".into());
	}
	if ![1, 2, 3, 4, 5, 6, 7, 8].contains(&body[1]) {
		return Err("unknown message".into());
	}
	let kind = body[1];
	let mut at = 2;
	let mut fields = Fields::new();
	while at < body.len() {
		if body.len() - at < 5 {
			return Err("truncated field".into());
		}
		let tag = body[at];
		let len = u32::from_be_bytes(body[at + 1..at + 5].try_into().unwrap()) as usize;
		at += 5;
		if tag == 0 || tag > 12 || len > body.len() - at {
			return Err("unknown/truncated field".into());
		}
		let s = std::str::from_utf8(&body[at..at + len]).map_err(|_| "non-UTF-8 field")?;
		if fields.insert(tag, s.to_owned()).is_some() {
			return Err("duplicate field".into());
		}
		at += len;
	}
	Ok((kind, fields))
}
pub fn read(r: &mut impl Read) -> Result<(u8, Fields), String> {
	let mut len = [0; 4];
	r.read_exact(&mut len).map_err(|e| e.to_string())?;
	let len = u32::from_be_bytes(len) as usize;
	if !(2..=LIMIT).contains(&len) {
		return Err("frame length refused".into());
	}
	let mut b = vec![0; len];
	r.read_exact(&mut b).map_err(|e| e.to_string())?;
	decode(&b)
}
pub fn write(w: &mut impl Write, kind: u8, f: &Fields) -> Result<(), String> {
	w.write_all(&encode(kind, f)?).map_err(|e| e.to_string())
}
pub fn exact(f: &Fields, keys: &[u8]) -> Result<(), String> {
	if f.keys().copied().collect::<Vec<_>>() != keys {
		return Err("unexpected message fields".into());
	}
	Ok(())
}
pub fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn unhex(s: &str) -> Result<Vec<u8>, String> {
	if s.len() > 32768 || !s.len().is_multiple_of(2) {
		return Err("association length refused".into());
	}
	s.as_bytes()
		.as_chunks::<2>()
		.0
		.iter()
		.map(|c| {
			let t = std::str::from_utf8(c).map_err(|_| "association encoding")?;
			u8::from_str_radix(t, 16).map_err(|_| "association encoding".into())
		})
		.collect()
}
pub fn capsule(s: &str) -> Result<Fields, String> {
	let raw = unhex(s)?;
	let mut r = std::io::Cursor::new(&raw);
	let (k, f) = read(&mut r)?;
	if k != 7 || r.position() != raw.len() as u64 {
		return Err("bad association/trailing bytes".into());
	}
	exact(&f, &[1, 2, 3, 4, 5, 6])?;
	Ok(f)
}
/// The Unix part of the 0059 metadata policy, used only across private handover.
#[cfg(unix)]
pub fn stamp(path: &std::path::Path) -> Result<String, String> {
	use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
	let f = std::fs::OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
		.open(path)
		.map_err(|e| e.to_string())?;
	let m = f.metadata().map_err(|e| e.to_string())?;
	if !m.is_file() || !(0..1_000_000_000).contains(&m.mtime_nsec()) {
		return Err("invalid handover artifact metadata".into());
	}
	Ok(format!(
		"{}:{}:{}:{}:{}:{}",
		m.len(),
		m.mtime(),
		m.mtime_nsec(),
		m.mode() & 0o111 != 0,
		m.dev(),
		m.ino()
	))
}

/// Fixture-only, comparable monotonic timestamps across the transition processes.
#[cfg(all(unix, feature = "test-support"))]
pub fn trace(name: &str) {
	use std::io::Write;
	let Some(path) = std::env::var_os("RNX_DEP_TIMELINE") else {
		return;
	};
	let mut ts = libc::timespec {
		tv_sec: 0,
		tv_nsec: 0,
	};
	if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
		return;
	}
	if let Ok(mut f) = std::fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
	{
		let _ = writeln!(
			f,
			"{}\t{}\t{name}",
			ts.tv_sec as u128 * 1_000_000_000 + ts.tv_nsec as u128,
			std::process::id()
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn framing() {
		let f = Fields::from([(1, "🦀".into())]);
		let b = encode(2, &f).unwrap();
		assert_eq!(read(&mut &b[..]).unwrap(), (2, f));
		for cut in 0..b.len() {
			assert!(read(&mut &b[..cut]).is_err());
		}
		assert!(read(&mut &[0xff, 0xff, 0xff, 0xff][..]).is_err());
		assert!(decode(&[2, 1]).is_err());
		assert!(decode(&[1, 99]).is_err());
		assert!(decode(&[1, 1, 13, 0, 0, 0, 0]).is_err());
		assert!(decode(&[1, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0]).is_err());
		assert!(encode(2, &Fields::from([(1, "x".repeat(LIMIT))])).is_err());
	}
}
