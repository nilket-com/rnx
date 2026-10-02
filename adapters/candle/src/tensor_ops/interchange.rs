//! Record 0143: Candle interchange (`.npy`, `.npz`, safetensors, raw bytes)
//! and functional scatter, comparison and grids.
//!
//! 13 core `Tensor` identities (`probes/0143/manifest.tsv`), under 0134's
//! contract and 0137's bounds, plus the interchange contract:
//!
//! - **reading:** one bounded, non-blocking read of a regular file
//!   (`read_limited`, at most `MAX_FILE`), then rnx's own strict decoder.
//!   Candle's readers allocate from unchecked headers (`read_header` takes
//!   the declared header length, `from_reader` the declared element count)
//!   and `from_reader` is crate-private, so they are the parity oracle in
//!   the tests, not the path. Every header, count and size is checked
//!   before any allocation beyond the file buffer.
//! - **.npz:** stored entries only (Candle's zip build has no deflate); the
//!   layout is checked over the bytes before the zip crate builds anything,
//!   and the zip crate must then see the same entries at the same offsets.
//! - **writing:** create-new; the whole encoding (data, headers, archive
//!   records) bounded before any buffer is built, encoded in memory, and
//!   byte-identical to Candle's own writers.
use super::composition::{AXIS, PARTS};
use super::{MAX_ELEMS, MAX_RANK, capped, dtype_name, go, operand_like, same_dtype, wrap};
use crate::{MAX_FILE, Tensor, read_limited};
use candle_core::{DType, Device, Tensor as CTensor, op::CmpOp};
use rnx::rune::{
	self, Value,
	runtime::{Bytes, Vec as RuneVec},
};
use std::io::{Cursor, Read, Write};

const CPU: &Device = &Device::Cpu;
/// An `.npy` header (the dict and its padding) is at most 64 KiB.
pub const NPY_HEADER: usize = 64 << 10;
/// An `.npz` holds at most 256 arrays.
pub const ENTRIES: usize = 256;
/// A name (archive entry, requested or written, safetensors key) is at
/// most 255 bytes.
pub const NAME: usize = 255;
/// An archive's central directory is at most 128 KiB.
pub const DIRECTORY: usize = 128 << 10;
/// The values decoded from one archive, all entries together.
pub const ARCHIVE_VALUES: usize = 4 * MAX_ELEMS;

const MAGIC: &[u8] = b"\x93NUMPY";

fn item_size(d: DType) -> usize {
	match d {
		DType::F32 | DType::U32 => 4,
		DType::F64 | DType::I64 => 8,
		DType::U8 => 1,
		_ => unreachable!("only the adapter's dtypes"),
	}
}

// ---- .npy decoding ----

/// A parsed `.npy` header: dtype, dimensions, and where the data starts.
struct Header {
	dtype: DType,
	dims: Vec<usize>,
	count: usize,
	data: usize,
}

/// The header of an `.npy` image, strictly: magic, version 1.0 or 2.0, a
/// header of at most `NPY_HEADER` bytes inside the image, a dict with
/// exactly `descr`, `fortran_order` and `shape`, and a data length equal to
/// the shape's product times the item size.
fn header(op: &str, b: &[u8]) -> Result<Header, String> {
	let bad = |why: String| format!("{op}: not a supported .npy: {why}");
	if b.len() < 10 || &b[..6] != MAGIC {
		return Err(bad("no NumPy magic".into()));
	}
	let (len, start) = match (b[6], b[7]) {
		(1, 0) => (u16::from_le_bytes([b[8], b[9]]) as usize, 10),
		(2, 0) => {
			if b.len() < 12 {
				return Err(bad("truncated header length".into()));
			}
			(u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as usize, 12)
		}
		(major, minor) => {
			return Err(bad(format!(
				"version {major}.{minor}; versions 1.0 and 2.0 are supported"
			)));
		}
	};
	if len > NPY_HEADER {
		return Err(bad(format!("a {len}-byte header, at most {NPY_HEADER}")));
	}
	let end = start + len;
	if end > b.len() {
		return Err(bad(format!(
			"a {len}-byte header in a {}-byte file",
			b.len()
		)));
	}
	let text = std::str::from_utf8(&b[start..end])
		.ok()
		.filter(|t| t.is_ascii())
		.ok_or_else(|| bad("the header is not ASCII".into()))?;
	let (descr, fortran, dims) = dict(text).map_err(&bad)?;
	if fortran {
		return Err(bad("fortran_order True is not supported".into()));
	}
	let dtype = descr_dtype(&descr).map_err(&bad)?;
	let count = capped(op, &dims, "the array")?;
	let size = count * item_size(dtype);
	if b.len() - end != size {
		return Err(bad(format!(
			"{} data bytes, want exactly {size} for shape {dims:?} of {}",
			b.len() - end,
			dtype_name(dtype)
		)));
	}
	Ok(Header {
		dtype,
		dims,
		count,
		data: end,
	})
}

/// The adapter's dtypes, little-endian (or single-byte) only.
fn descr_dtype(d: &str) -> Result<DType, String> {
	let little = cfg!(target_endian = "little");
	if d.starts_with('>') {
		return Err(format!("descr {d:?} is big-endian"));
	}
	let (order, kind) = d.split_at(d.len().min(1));
	let ok_order = order == "<" || (order == "=" && little) || (order == "|" && kind == "u1");
	match (ok_order, kind) {
		(true, "f4") => Ok(DType::F32),
		(true, "f8") => Ok(DType::F64),
		(true, "i8") => Ok(DType::I64),
		(true, "u4") => Ok(DType::U32),
		(true, "u1") => Ok(DType::U8),
		_ => Err(format!(
			"descr {d:?}; supported: <f4, <f8, <i8, <u4, |u1 (f32, f64, i64, u32, u8)"
		)),
	}
}

/// NumPy's header dict, strictly: `{'descr': '<f4', 'fortran_order':
/// False, 'shape': (2, 3), }`, keys in any order, each exactly once, then
/// only spaces and one final newline.
fn dict(text: &str) -> Result<(String, bool, Vec<usize>), String> {
	let body = text.trim_end_matches('\n').trim_end_matches(' ');
	if text.len() - body.len() > 0 && !text.ends_with('\n') {
		return Err("the header does not end with a newline".into());
	}
	let inner = body
		.strip_prefix('{')
		.and_then(|s| s.strip_suffix('}'))
		.ok_or("the header is not a dict")?;
	let mut s = inner.trim_start();
	let (mut descr, mut fortran, mut shape) = (None, None, None);
	while !s.is_empty() {
		let rest = s.strip_prefix('\'').ok_or("a key is not quoted")?;
		let close = rest.find('\'').ok_or("an unterminated key")?;
		let key = &rest[..close];
		s = rest[close + 1..]
			.trim_start()
			.strip_prefix(':')
			.ok_or("a key without ':'")?
			.trim_start();
		let value_end = match key {
			"descr" => {
				let r = s.strip_prefix('\'').ok_or("descr is not a string")?;
				let c = r.find('\'').ok_or("an unterminated descr")?;
				if descr.replace(r[..c].to_string()).is_some() {
					return Err("descr appears twice".into());
				}
				1 + c + 1
			}
			"fortran_order" => {
				let v = if s.starts_with("False") {
					false
				} else if s.starts_with("True") {
					true
				} else {
					return Err("fortran_order is not True or False".into());
				};
				if fortran.replace(v).is_some() {
					return Err("fortran_order appears twice".into());
				}
				if v { 4 } else { 5 }
			}
			"shape" => {
				let r = s.strip_prefix('(').ok_or("shape is not a tuple")?;
				let c = r.find(')').ok_or("an unterminated shape")?;
				let mut dims = Vec::new();
				let items = r[..c].trim();
				let items = items.strip_suffix(',').unwrap_or(items);
				if !items.trim().is_empty() {
					for part in items.split(',') {
						let p = part.trim();
						if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
							return Err(format!("shape item {p:?} is not a non-negative integer"));
						}
						if dims.len() == MAX_RANK {
							return Err(format!("shape has more than {MAX_RANK} dimensions"));
						}
						dims.push(
							p.parse::<usize>()
								.map_err(|_| format!("shape item {p} overflows"))?,
						);
					}
				}
				if shape.replace(dims).is_some() {
					return Err("shape appears twice".into());
				}
				1 + c + 1
			}
			other => return Err(format!("unknown header key {other:?}")),
		};
		s = s[value_end..].trim_start();
		match s.strip_prefix(',') {
			Some(r) => s = r.trim_start(),
			None if s.is_empty() => {}
			None => return Err("header entries are not comma-separated".into()),
		}
	}
	Ok((
		descr.ok_or("no descr")?,
		fortran.ok_or("no fortran_order")?,
		shape.ok_or("no shape")?,
	))
}

/// An `.npy` image decoded into a tensor (on the worker).
fn decode(op: &str, b: &[u8], h: &Header) -> Result<CTensor, String> {
	let data = &b[h.data..];
	let dims = h.dims.clone();
	macro_rules! le {
		($t:ty, $n:expr) => {{
			let v: Vec<$t> = data
				.chunks_exact($n)
				.map(|c| <$t>::from_le_bytes(c.try_into().unwrap()))
				.collect();
			go(op, move || CTensor::from_vec(v, dims, CPU))
		}};
	}
	match h.dtype {
		DType::F32 => le!(f32, 4),
		DType::F64 => le!(f64, 8),
		DType::I64 => le!(i64, 8),
		DType::U32 => le!(u32, 4),
		DType::U8 => {
			let v = data.to_vec();
			go(op, move || CTensor::from_vec(v, dims, CPU))
		}
		_ => unreachable!(),
	}
}

fn read_npy(path: &str) -> Result<Tensor, String> {
	let op = "Tensor::read_npy";
	let b = read_limited(path, MAX_FILE, op)?;
	let op = &format!("{op} {path:?}");
	let h = header(op, &b)?;
	decode(op, &b, &h).map(Tensor)
}

// ---- .npz: the layout preflight, then the zip crate ----

fn u16_at(b: &[u8], i: usize) -> u16 {
	u16::from_le_bytes([b[i], b[i + 1]])
}
fn u32_at(b: &[u8], i: usize) -> u32 {
	u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// One stored entry, as the preflight found it.
struct Entry {
	name: String,
	data: usize,
	size: usize,
}

/// The archive's layout, checked over the bytes before any parser builds
/// metadata. The end record is exactly the last 22 bytes (an archive
/// comment is refused, so no record can hide in one), the central
/// directory ends exactly where it starts, and the local headers and data
/// lie in order before it.
fn preflight(op: &str, b: &[u8]) -> Result<Vec<Entry>, String> {
	let bad = |why: String| format!("{op}: not a supported .npz: {why}");
	if b.len() < 22 {
		return Err(bad("too short for an archive".into()));
	}
	let e = b.len() - 22;
	if u32_at(b, e) != 0x0605_4b50 {
		return Err(bad(
			"no end record in the last 22 bytes (an archive comment or trailing data is not supported)"
				.into(),
		));
	}
	if e >= 20 && u32_at(b, e - 20) == 0x0706_4b50 {
		return Err(bad("a ZIP64 end record is not supported".into()));
	}
	let (disk, cd_disk) = (u16_at(b, e + 4), u16_at(b, e + 6));
	let (here, total) = (u16_at(b, e + 8) as usize, u16_at(b, e + 10) as usize);
	let (cd_size, cd_off) = (u32_at(b, e + 12), u32_at(b, e + 16));
	let comment = u16_at(b, e + 20);
	if disk != 0 || cd_disk != 0 {
		return Err(bad("a multi-disk archive".into()));
	}
	if here == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
		return Err(bad("ZIP64 end values are not supported".into()));
	}
	if comment != 0 {
		return Err(bad("an archive comment is not supported".into()));
	}
	if here != total {
		return Err(bad(format!("entry counts disagree ({here} and {total})")));
	}
	if total > ENTRIES {
		return Err(bad(format!("{total} entries, at most {ENTRIES}")));
	}
	let (cd_size, cd_off) = (cd_size as usize, cd_off as usize);
	if cd_size > DIRECTORY {
		return Err(bad(format!(
			"a {cd_size}-byte central directory, at most {DIRECTORY}"
		)));
	}
	if cd_off.checked_add(cd_size) != Some(e) {
		return Err(bad(
			"the central directory does not end at the end record".into()
		));
	}
	let mut entries: Vec<Entry> = Vec::with_capacity(total);
	let mut seen = std::collections::BTreeSet::new();
	let mut values = 0usize;
	let mut at = cd_off;
	let mut next_local = 0usize;
	for i in 0..total {
		let rec = |why: &str| bad(format!("central record {i}: {why}"));
		if at + 46 > e || u32_at(b, at) != 0x0201_4b50 {
			return Err(rec("missing or bad signature"));
		}
		let flags = u16_at(b, at + 8);
		let method = u16_at(b, at + 10);
		let (csize, usize_) = (u32_at(b, at + 20), u32_at(b, at + 24));
		let (n, x, c) = (
			u16_at(b, at + 28) as usize,
			u16_at(b, at + 30) as usize,
			u16_at(b, at + 32) as usize,
		);
		let local = u32_at(b, at + 42);
		let end = at + 46 + n + x + c;
		if end > e {
			return Err(rec("its lengths overrun the directory"));
		}
		if flags & 1 != 0 {
			return Err(rec("encrypted"));
		}
		if method != 0 {
			return Err(bad(
				"compressed .npz is not supported; save with np.savez".into()
			));
		}
		if csize == 0xFFFF_FFFF || usize_ == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
			return Err(rec("ZIP64 directory values are not supported"));
		}
		if csize != usize_ {
			return Err(rec("a stored entry's sizes disagree"));
		}
		if n == 0 || n > NAME + 4 {
			return Err(rec(&format!("a name of {n} bytes")));
		}
		let raw = &b[at + 46..at + 46 + n];
		let full = std::str::from_utf8(raw).map_err(|_| rec("its name is not UTF-8"))?;
		let name = full
			.strip_suffix(".npy")
			.filter(|s| !s.is_empty() && s.len() <= NAME)
			.ok_or_else(|| {
				rec(&format!(
					"entry {full:?} is not a .npy of at most {NAME} bytes"
				))
			})?;
		if !seen.insert(name.to_string()) {
			return Err(bad(format!("array {name:?} appears twice")));
		}
		let size = usize_ as usize;
		values = values
			.checked_add(size)
			.filter(|&v| v as u64 <= MAX_FILE)
			.ok_or_else(|| bad(format!("declared sizes exceed {MAX_FILE} bytes")))?;
		// the local header: in order, consistent with this record
		let l = local as usize;
		if l != next_local || l + 30 > cd_off || u32_at(b, l) != 0x0403_4b50 {
			return Err(rec("its local header is missing or out of order"));
		}
		let (ln, lx) = (u16_at(b, l + 26) as usize, u16_at(b, l + 28) as usize);
		if u16_at(b, l + 8) != method || ln != n || l + 30 + ln + lx > cd_off {
			return Err(rec("its local header disagrees"));
		}
		if &b[l + 30..l + 30 + n] != raw {
			return Err(rec("its local name disagrees"));
		}
		let (lc, lu) = (u32_at(b, l + 18), u32_at(b, l + 22));
		if (lc, lu) != (csize, usize_) {
			// NumPy writes ZIP64 extras locally (`force_zip64`): the sizes
			// are then in the extra field, and must agree
			if (lc, lu) != (0xFFFF_FFFF, 0xFFFF_FFFF) {
				return Err(rec("its local sizes disagree"));
			}
			let extra = &b[l + 30 + ln..l + 30 + ln + lx];
			let mut zip64 = None;
			let mut k = 0;
			while k + 4 <= extra.len() {
				let (id, len) = (u16_at(extra, k), u16_at(extra, k + 2) as usize);
				if k + 4 + len > extra.len() {
					return Err(rec("a local extra field overruns"));
				}
				if id == 1 && len >= 16 {
					let f = &extra[k + 4..];
					zip64 = Some((
						u64::from_le_bytes(f[..8].try_into().unwrap()),
						u64::from_le_bytes(f[8..16].try_into().unwrap()),
					));
				}
				k += 4 + len;
			}
			if zip64 != Some((usize_ as u64, csize as u64)) {
				return Err(rec("its local ZIP64 sizes disagree"));
			}
		}
		let data = l + 30 + ln + lx;
		let data_end = data
			.checked_add(size)
			.filter(|&d| d <= cd_off)
			.ok_or_else(|| rec("its data overruns the directory"))?;
		next_local = data_end;
		entries.push(Entry {
			name: name.to_string(),
			data,
			size,
		});
		at = end;
	}
	if at != e {
		return Err(bad(
			"the central directory has bytes after its records".into()
		));
	}
	if next_local != cd_off {
		return Err(bad("bytes between the last entry and the directory".into()));
	}
	Ok(entries)
}

/// The archive, preflighted, then opened by the zip crate, which must see
/// the same entries at the same offsets (and checks each entry's CRC on
/// read). Returns the entries' images, in archive order, for `wanted`
/// (`None`: all).
fn archive(
	op: &str,
	b: &[u8],
	wanted: Option<&[String]>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
	let entries = preflight(op, b)?;
	let mut zip = zip::ZipArchive::new(Cursor::new(b)).map_err(|e| format!("{op}: {e}"))?;
	if zip.len() != entries.len() {
		return Err(format!(
			"{op}: the archive parser sees a different directory"
		));
	}
	let pick: Vec<usize> = match wanted {
		None => (0..entries.len()).collect(),
		Some(names) => names
			.iter()
			.map(|n| {
				entries
					.iter()
					.position(|e| &e.name == n)
					.ok_or_else(|| format!("{op}: no array {n:?} in the archive"))
			})
			.collect::<Result<_, _>>()?,
	};
	let mut out = Vec::with_capacity(pick.len());
	for i in pick {
		let e = &entries[i];
		let mut f = zip.by_index(i).map_err(|err| format!("{op}: {err}"))?;
		if f.name() != format!("{}.npy", e.name)
			|| f.data_start() != Some(e.data as u64)
			|| f.size() != e.size as u64
		{
			return Err(format!(
				"{op}: the archive parser sees entry {:?} differently",
				e.name
			));
		}
		let mut image = Vec::with_capacity(e.size);
		(&mut f)
			.take(e.size as u64 + 1)
			.read_to_end(&mut image)
			.map_err(|err| format!("{op}: entry {:?}: {err}", e.name))?;
		if image.len() != e.size {
			return Err(format!("{op}: entry {:?} is not its declared size", e.name));
		}
		out.push((e.name.clone(), image));
	}
	Ok(out)
}

/// The images decoded, the archive's cumulative value count checked
/// before each one's allocation.
fn decode_all(op: &str, images: Vec<(String, Vec<u8>)>) -> Result<Vec<(String, CTensor)>, String> {
	let mut total = 0usize;
	let mut out = Vec::with_capacity(images.len());
	for (name, image) in images {
		let eop = format!("{op}: array {name:?}");
		let h = header(&eop, &image)?;
		total = total
			.checked_add(h.count)
			.filter(|&t| t <= ARCHIVE_VALUES)
			.ok_or_else(|| format!("{op}: the arrays exceed {ARCHIVE_VALUES} values together"))?;
		let t = decode(&eop, &image, &h)?;
		out.push((name, t));
	}
	Ok(out)
}

fn read_npz(path: &str) -> Result<Vec<(String, Tensor)>, String> {
	let op = "Tensor::read_npz";
	let b = read_limited(path, MAX_FILE, op)?;
	let op = &format!("{op} {path:?}");
	let images = archive(op, &b, None)?;
	Ok(decode_all(op, images)?
		.into_iter()
		.map(|(n, t)| (n, Tensor(t)))
		.collect())
}

/// Names from a Rune vector: 1 to `ENTRIES`, each 1 to `NAME` bytes,
/// unique. Each string is borrowed, never taken (the script keeps its
/// values), and checked before it is copied.
fn names(op: &str, v: &Value, what: &str) -> Result<Vec<String>, String> {
	let not = || format!("{op}: {what} must be a vector of strings");
	let values = v.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.is_empty() || values.len() > ENTRIES {
		return Err(format!(
			"{op}: {} {what}, want 1 to {ENTRIES}",
			values.len()
		));
	}
	let mut out: Vec<String> = Vec::with_capacity(values.len());
	for v in values.iter() {
		let s = v.borrow_string_ref().map_err(|_| not())?;
		name_ok(op, &s)?;
		if out.iter().any(|o| *o == *s) {
			return Err(format!("{op}: {:?} appears twice", &*s));
		}
		out.push(s.to_string());
	}
	Ok(out)
}

/// A name's rules, on the borrowed text. The refusal quotes the name only
/// when it is within the length bound, so a diagnostic stays bounded.
fn name_ok(op: &str, s: &str) -> Result<(), String> {
	if s.is_empty() || s.len() > NAME {
		return Err(format!(
			"{op}: a name of {} bytes; names are 1 to {NAME} bytes, without '/' or NUL",
			s.len()
		));
	}
	if s.contains('/') || s.contains('\0') {
		return Err(format!(
			"{op}: name {s:?} must be 1 to {NAME} bytes, without '/' or NUL"
		));
	}
	Ok(())
}

fn read_npz_by_name(path: &str, wanted: Value) -> Result<Vec<Tensor>, String> {
	let op = "Tensor::read_npz_by_name";
	let wanted = names(op, &wanted, "names")?;
	let b = read_limited(path, MAX_FILE, op)?;
	let op = &format!("{op} {path:?}");
	let images = archive(op, &b, Some(&wanted))?;
	Ok(decode_all(op, images)?
		.into_iter()
		.map(|(_, t)| Tensor(t))
		.collect())
}

// ---- writing ----

/// Candle's own `.npy` encoding (`npy.rs` `write`): version 1.0, its dict,
/// padded so the data starts on a 16-byte boundary.
fn npy_image(t: &CTensor) -> Result<Vec<u8>, String> {
	let descr = match t.dtype() {
		DType::F32 => "f4",
		DType::F64 => "f8",
		DType::I64 => "i8",
		DType::U32 => "u4",
		DType::U8 => "u1",
		_ => unreachable!(),
	};
	let mut shape = t
		.dims()
		.iter()
		.map(|d| d.to_string())
		.collect::<Vec<_>>()
		.join(",");
	if !shape.is_empty() {
		shape.push(',');
	}
	let mut h = format!("{{'descr': '<{descr}', 'fortran_order': False, 'shape': ({shape}), }}");
	let pad = 16 - (MAGIC.len() + 5 + h.len()) % 16;
	for _ in 0..pad % 16 {
		h.push(' ');
	}
	h.push('\n');
	let mut out = Vec::with_capacity(10 + h.len() + t.elem_count() * item_size(t.dtype()));
	out.extend_from_slice(MAGIC);
	out.extend_from_slice(&[1, 0, (h.len() % 256) as u8, (h.len() / 256) as u8]);
	out.extend_from_slice(h.as_bytes());
	let t = t.clone();
	let data = go("write", move || {
		let mut v = Vec::new();
		t.write_bytes(&mut v).map(|_| v)
	})?;
	out.extend_from_slice(&data);
	Ok(out)
}

/// The bound on an `.npy` image's size, before encoding: data plus the
/// header (its dict is at most 128 bytes beyond the shape's digits).
fn npy_bound(op: &str, t: &CTensor) -> Result<u64, String> {
	let data = (t.elem_count() as u64) * item_size(t.dtype()) as u64;
	let digits: u64 = t
		.dims()
		.iter()
		.map(|d| d.to_string().len() as u64 + 1)
		.sum();
	let total = data + 10 + 128 + digits + 16;
	if total > MAX_FILE {
		return Err(format!(
			"{op}: about {total} bytes to write, at most {MAX_FILE}"
		));
	}
	Ok(total)
}

/// A new file holding `bytes`: create-new (an existing file is refused and
/// left unchanged); a failure while writing removes the partial file,
/// best-effort, and says so.
fn create(op: &str, path: &str, bytes: &[u8]) -> Result<(), String> {
	let mut f = std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)
		.map_err(|e| format!("{op} {path:?}: {e}"))?;
	if let Err(e) = f.write_all(bytes).and_then(|_| f.flush()) {
		drop(f);
		let removed = std::fs::remove_file(path).is_ok();
		return Err(format!(
			"{op} {path:?}: {e}; the partial file was {}",
			if removed { "removed" } else { "left in place" }
		));
	}
	Ok(())
}

fn write_npy(this: &Tensor, path: &str) -> Result<(), String> {
	let op = "Tensor::write_npy";
	npy_bound(op, &this.0)?;
	let image = npy_image(&this.0)?;
	create(op, path, &image)
}

/// Candle's own `.npz` encoding (`npy.rs` `write_npz`): stored entries
/// named `<name>.npy`, through the same zip crate and options.
fn npz_image(op: &str, pairs: &[(String, CTensor)]) -> Result<Vec<u8>, String> {
	let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
	let options: zip::write::FileOptions<()> =
		zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
	for (name, t) in pairs {
		zip.start_file(format!("{name}.npy"), options)
			.map_err(|e| format!("{op}: {e}"))?;
		let image = npy_image(t)?;
		zip.write_all(&image).map_err(|e| format!("{op}: {e}"))?;
	}
	zip.finish()
		.map(|c| c.into_inner())
		.map_err(|e| format!("{op}: {e}"))
}

fn write_npz(pairs: Value, path: &str) -> Result<(), String> {
	let op = "Tensor::write_npz";
	let not = || format!("{op}: needs a vector of (name, tensor) pairs");
	let values = pairs.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.is_empty() || values.len() > ENTRIES {
		return Err(format!("{op}: {} pairs, want 1 to {ENTRIES}", values.len()));
	}
	let mut out: Vec<(String, CTensor)> = Vec::with_capacity(values.len());
	let mut total = 22u64;
	for v in values.iter() {
		// borrowed: the script's pairs, names and tensors stay its own
		let pair = v.borrow_tuple_ref().map_err(|_| not())?;
		let [name, t] = &pair[..] else {
			return Err(not());
		};
		let name = name.borrow_string_ref().map_err(|_| not())?;
		name_ok(op, &name)?;
		if out.iter().any(|(n, _)| *n == *name) {
			return Err(format!("{op}: {:?} appears twice", &*name));
		}
		let t = t.borrow_ref::<Tensor>().map_err(|_| not())?.0.clone();
		// the image, its local and central records (with ZIP64 extras at
		// most), checked before anything is encoded
		total += npy_bound(op, &t)? + 2 * (46 + 2 * (name.len() as u64 + 4) + 64);
		if total > MAX_FILE {
			return Err(format!(
				"{op}: about {total} bytes to write, at most {MAX_FILE}"
			));
		}
		out.push((name.to_string(), t));
	}
	drop(values);
	let image = npz_image(op, &out)?;
	create(op, path, &image)
}

fn save_safetensors(this: &Tensor, name: &str, path: &str) -> Result<(), String> {
	let op = "Tensor::save_safetensors";
	name_ok(op, name)?;
	// review round 1: the header is JSON, and a name's escaping can grow it
	// six-fold (U+0001 is \u0001); its exact escaped length, plus the fixed
	// keys and punctuation (under 96 bytes), the shape's digits, the two
	// offsets, padding to 8 and the 8-byte length prefix
	let escaped = serde_json::to_string(name)
		.map_err(|e| format!("{op}: {e}"))?
		.len() as u64;
	let data = (this.0.elem_count() as u64) * item_size(this.0.dtype()) as u64;
	let digits: u64 = this
		.0
		.dims()
		.iter()
		.map(|d| d.to_string().len() as u64 + 1)
		.sum();
	let bound = data + 8 + escaped + 96 + digits + 2 * 20 + 8;
	if bound > MAX_FILE {
		return Err(format!(
			"{op}: about {bound} bytes to write, at most {MAX_FILE}"
		));
	}
	let t = this.0.clone();
	let key = name.to_string();
	let image = go(op, move || {
		safetensors::tensor::serialize([(key, &t)], None)
			.map_err(|e| candle_core::Error::Msg(e.to_string()))
	})?;
	// and the encoding itself, before anything is written
	if image.len() as u64 > MAX_FILE {
		return Err(format!(
			"{op}: {} bytes to write, at most {MAX_FILE}",
			image.len()
		));
	}
	create(op, path, &image)
}

fn write_bytes(this: &Tensor) -> Result<Bytes, String> {
	let op = "Tensor::write_bytes";
	npy_bound(op, &this.0)?;
	let t = this.0.clone();
	let v = go(op, move || {
		let mut v = Vec::new();
		t.write_bytes(&mut v).map(|_| v)
	})?;
	Bytes::try_from(v).map_err(|e| format!("{op}: {e}"))
}

// ---- K: functional scatter, comparison and grids ----

/// An index tensor's values, each checked against its axis before Candle
/// sees it: in range, and never the dtype's maximum (Candle's scatter skips
/// that value silently).
fn checked_indexes(op: &str, ids: &CTensor, len: usize) -> Result<(), String> {
	let bad = |v: String| {
		format!(
			"{op}: index {v} is outside the axis of {len} (or is the dtype's maximum, which Candle skips)"
		)
	};
	let flat = ids.clone();
	match ids.dtype() {
		DType::U32 => {
			for v in go(op, move || flat.flatten_all()?.to_vec1::<u32>())? {
				if v == u32::MAX || v as usize >= len {
					return Err(bad(v.to_string()));
				}
			}
		}
		DType::I64 => {
			for v in go(op, move || flat.flatten_all()?.to_vec1::<i64>())? {
				if v < 0 || v == i64::MAX || v as u64 >= len as u64 {
					return Err(bad(v.to_string()));
				}
			}
		}
		DType::U8 => {
			for v in go(op, move || flat.flatten_all()?.to_vec1::<u8>())? {
				if v == u8::MAX || v as usize >= len {
					return Err(bad(v.to_string()));
				}
			}
		}
		other => {
			return Err(format!(
				"{op}: indexes must be u32, i64 or u8, found {}",
				dtype_name(other)
			));
		}
	}
	Ok(())
}

fn scatter(this: &Tensor, indexes: &Tensor, source: &Tensor, dim: i64) -> Result<Tensor, String> {
	let op = "Tensor::scatter";
	let (t, ids, src) = (&this.0, &indexes.0, &source.0);
	let d = super::axis(op, t.rank(), dim)?;
	same_dtype(op, t, src)?;
	let mismatch = t.rank() != src.rank()
		|| t.dims()
			.iter()
			.zip(src.dims())
			.enumerate()
			.any(|(i, (a, b))| i != d && a != b);
	if mismatch {
		return Err(format!(
			"{op}: source {:?} must match {:?} on every axis but {d}",
			src.dims(),
			t.dims()
		));
	}
	if ids.dims() != src.dims() {
		return Err(format!(
			"{op}: indexes {:?} must have the source's shape {:?}",
			ids.dims(),
			src.dims()
		));
	}
	capped(op, t.dims(), "the output")?;
	checked_indexes(op, ids, t.dims()[d])?;
	let (t, ids, src) = (t.clone(), ids.clone(), src.clone());
	wrap(op, move || t.scatter(&ids, &src, d))
}

fn slice_scatter(this: &Tensor, src: &Tensor, dim: i64, start: i64) -> Result<Tensor, String> {
	let op = "Tensor::slice_scatter";
	let d = super::axis(op, this.0.rank(), dim)?;
	sliced(op, &this.0, &src.0, d, start)?;
	let (t, s) = (this.0.clone(), src.0.clone());
	let start = start as usize;
	wrap(op, move || t.slice_scatter(&s, d, start))
}

fn slice_scatter0(this: &Tensor, src: &Tensor, start: i64) -> Result<Tensor, String> {
	let op = "Tensor::slice_scatter0";
	if this.0.rank() == 0 {
		return Err(format!("{op}: needs rank at least 1"));
	}
	sliced(op, &this.0, &src.0, 0, start)?;
	let (t, s) = (this.0.clone(), src.0.clone());
	let start = start as usize;
	wrap(op, move || t.slice_scatter0(&s, start))
}

/// `src` fits in `t` along `d` from `start`; the other axes are equal.
fn sliced(op: &str, t: &CTensor, src: &CTensor, d: usize, start: i64) -> Result<(), String> {
	same_dtype(op, t, src)?;
	if t.rank() != src.rank()
		|| t.dims()
			.iter()
			.zip(src.dims())
			.enumerate()
			.any(|(i, (a, b))| i != d && a != b)
	{
		return Err(format!(
			"{op}: src {:?} must match {:?} on every axis but {d}",
			src.dims(),
			t.dims()
		));
	}
	let start = usize::try_from(start).map_err(|_| format!("{op}: start must be non-negative"))?;
	match start.checked_add(src.dims()[d]) {
		Some(end) if end <= t.dims()[d] => {}
		_ => {
			return Err(format!(
				"{op}: start {start} plus {} exceeds axis {d} of {}",
				src.dims()[d],
				t.dims()[d]
			));
		}
	}
	capped(op, t.dims(), "the output")?;
	// review round 1: for d > 0 Candle transposes both sides so that d is
	// axis 0; each transposed shape has its own stride products, checked
	if d > 0 {
		let mut td = t.dims().to_vec();
		let mut sd = src.dims().to_vec();
		td.swap(0, d);
		sd.swap(0, d);
		capped(op, &td, "the transposed receiver")?;
		capped(op, &sd, "the transposed source")?;
	}
	Ok(())
}

fn cmp(this: &Tensor, rhs: Value, name: &str) -> Result<Tensor, String> {
	let op = "Tensor::cmp";
	let how = match name {
		"eq" => CmpOp::Eq,
		"ne" => CmpOp::Ne,
		"lt" => CmpOp::Lt,
		"le" => CmpOp::Le,
		"gt" => CmpOp::Gt,
		"ge" => CmpOp::Ge,
		other => {
			return Err(format!(
				"{op}: operator {other:?}, want \"eq\", \"ne\", \"lt\", \"le\", \"gt\" or \"ge\""
			));
		}
	};
	let r = operand_like(op, &this.0, &rhs)?;
	let t = this.0.clone();
	wrap(op, move || t.cmp(&r, how))
}

fn meshgrid(vectors: Value, xy: bool) -> Result<Vec<Tensor>, String> {
	let op = "Tensor::meshgrid";
	let not = || format!("{op}: needs a vector of 2 to {MAX_RANK} non-empty 1-D tensors");
	let values = vectors.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if values.len() < 2 || values.len() > MAX_RANK.min(PARTS) {
		return Err(not());
	}
	let mut ts = Vec::with_capacity(values.len());
	for v in values.iter() {
		let t = v.borrow_ref::<Tensor>().map_err(|_| not())?.0.clone();
		if t.rank() != 1 {
			return Err(not());
		}
		let n = t.dims()[0];
		// Candle builds repeat counts from the lengths, and repeat treats 0
		// as 1: a zero-length input would give a wrong shape
		if n == 0 || n > AXIS {
			return Err(format!(
				"{op}: input length {n}, want 1 to {AXIS} (zero is refused: Candle's repeat treats 0 as 1)"
			));
		}
		ts.push(t);
	}
	drop(values);
	for t in &ts[1..] {
		same_dtype(op, &ts[0], t)?;
	}
	// every grid has the full shape (Candle's order: reversed for xy);
	// each reshape and repeat intermediate is at most that size
	let mut lens: Vec<usize> = ts.iter().map(|t| t.dims()[0]).collect();
	if xy {
		lens.reverse();
	}
	let each = capped(op, &lens, "each grid")?;
	if each
		.checked_mul(ts.len())
		.is_none_or(|total| total > 4 * MAX_ELEMS)
	{
		return Err(format!(
			"{op}: the grids exceed {} values together",
			4 * MAX_ELEMS
		));
	}
	let grids = go(op, move || CTensor::meshgrid(&ts, xy))?;
	Ok(grids.into_iter().map(Tensor).collect())
}

fn from_slice(values: Value, shape: Value, dtype: &str) -> Result<Tensor, String> {
	let op = "Tensor::from_slice";
	let not = || format!("{op}: a shape is a vector of non-negative integers and at most one -1");
	let n = values
		.borrow_ref::<RuneVec>()
		.map_err(|_| format!("{op}: values must be a vector of numbers"))?
		.len();
	let items = shape.borrow_ref::<RuneVec>().map_err(|_| not())?;
	if items.len() > MAX_RANK {
		return Err(format!("{op}: rank {}, at most {MAX_RANK}", items.len()));
	}
	let mut dims = Vec::with_capacity(items.len());
	let mut hole = None;
	for (i, v) in items.iter().enumerate() {
		let d = rune::from_value::<i64>(v.clone()).map_err(|_| not())?;
		if d == -1 {
			if hole.replace(i).is_some() {
				return Err(format!("{op}: at most one -1 in the shape"));
			}
			dims.push(1);
		} else {
			dims.push(usize::try_from(d).map_err(|_| not())?);
		}
	}
	drop(items);
	if let Some(h) = hole {
		let known = dims
			.iter()
			.try_fold(1usize, |a, &d| a.checked_mul(d))
			.ok_or_else(|| format!("{op}: the known dimensions overflow"))?;
		// Candle's hole_size: a zero product is refused, then divisibility
		if known == 0 {
			return Err(format!(
				"{op}: a -1 with a zero-sized known shape is ambiguous"
			));
		}
		if n % known != 0 {
			return Err(format!(
				"{op}: {n} values do not divide into the known dimensions ({known})"
			));
		}
		dims[h] = n / known;
	}
	let shape: Vec<Value> = dims
		.iter()
		.map(|&d| rune::to_value(d as i64))
		.collect::<Result<_, _>>()
		.map_err(|e| format!("{op}: {e}"))?;
	let shape = rune::to_value(shape).map_err(|e| format!("{op}: {e}"))?;
	// the values and the inferred shape under from_vec's contract; the
	// tensor is then Candle's from_slice over them
	let built = super::from_vec_named(op, values, shape, dtype)?;
	let (d, dims) = (built.0.dtype(), built.0.dims().to_vec());
	macro_rules! via_slice {
		($t:ty) => {{
			let t = built.0.clone();
			wrap(op, move || {
				let v = t.flatten_all()?.to_vec1::<$t>()?;
				CTensor::from_slice(&v, dims, CPU)
			})
		}};
	}
	match d {
		DType::F32 => via_slice!(f32),
		DType::F64 => via_slice!(f64),
		DType::I64 => via_slice!(i64),
		DType::U32 => via_slice!(u32),
		DType::U8 => via_slice!(u8),
		_ => unreachable!(),
	}
}

pub(super) fn build(m: &mut rnx::rune::Module) -> Result<(), rnx::rune::ContextError> {
	m.function("read_npy", read_npy)
		.build_associated::<Tensor>()?;
	m.function("read_npz", read_npz)
		.build_associated::<Tensor>()?;
	m.function("read_npz_by_name", read_npz_by_name)
		.build_associated::<Tensor>()?;
	m.function("write_npz", write_npz)
		.build_associated::<Tensor>()?;
	m.function("meshgrid", meshgrid)
		.build_associated::<Tensor>()?;
	m.function("from_slice", from_slice)
		.build_associated::<Tensor>()?;
	m.associated_function("write_npy", write_npy)?;
	m.associated_function("save_safetensors", save_safetensors)?;
	m.associated_function("write_bytes", write_bytes)?;
	m.associated_function("scatter", scatter)?;
	m.associated_function("slice_scatter", slice_scatter)?;
	m.associated_function("slice_scatter0", slice_scatter0)?;
	m.associated_function("cmp", cmp)?;
	Ok(())
}

pub(super) fn catalogue() -> Vec<(String, &'static str)> {
	vec![
		(
			"candle::Tensor::read_npy".into(),
			"read_npy(path), read_npz(path) -> [(name, Tensor)], read_npz_by_name(path, names) -> [Tensor]: NumPy files up to 64 MiB; f32/f64/i64/u32/u8, C order, np.savez (not savez_compressed)",
		),
		(
			"candle::Tensor::write_npy".into(),
			"t.write_npy(path), Tensor::write_npz([(name, t)], path), t.save_safetensors(name, path): new files only, byte-identical to Candle's writers; t.write_bytes() -> Bytes (little-endian)",
		),
		(
			"candle::Tensor::scatter".into(),
			"scatter(indexes, source, dim), slice_scatter(src, dim, start), slice_scatter0(src, start) -> Result<Tensor>: new tensors; indexes checked first; duplicate targets: the later source element wins",
		),
		(
			"candle::Tensor::cmp".into(),
			"cmp(rhs, op) -> Result<Tensor> (u8): op \"eq\"/\"ne\"/\"lt\"/\"le\"/\"gt\"/\"ge\", rhs a tensor of the same shape or a scalar",
		),
		(
			"candle::Tensor::meshgrid".into(),
			"meshgrid([v1, v2, ...], xy) -> [Tensor]: 2 to 6 non-empty 1-D inputs; xy is Candle's (whole-order reversal, NumPy's only for two inputs)",
		),
		(
			"candle::Tensor::from_slice".into(),
			"from_slice(values, shape, dtype) -> Result<Tensor>: shape may hold one -1, inferred from the value count",
		),
	]
}
