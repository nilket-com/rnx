//! Record 0143: Candle interchange and functional scatter, against Candle's
//! own readers and writers as the parity oracle, NumPy-written fixtures
//! (`tests/data/0143`, from `probes/0143/fixtures.py`), and every refusal.
use candle_core::{Device, Tensor as CTensor};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn data(name: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("tests/data/0143")
		.join(name)
}

fn scratch(tag: &str) -> PathBuf {
	let d = std::env::temp_dir().join(format!("rnx-0143-{tag}-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&d);
	std::fs::create_dir_all(&d).unwrap();
	d
}

/// `pub fn main() { body }`, its Result unwrapped into ours.
fn eval(body: &str) -> Result<Value, String> {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	let script = format!("pub fn main() {{ {body} }}");
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	let unit = unit.unwrap_or_else(|_| panic!("{:?}", diagnostics.diagnostics()));
	let mut vm = Vm::new(runtime, Arc::new(unit));
	let v = vm.call(["main"], ()).map_err(|e| e.to_string())?;
	match rune::from_value::<Result<Value, Value>>(v.clone()) {
		Ok(Ok(v)) => Ok(v),
		Ok(Err(e)) => Err(rune::from_value::<String>(e).unwrap_or_else(|_| "?".into())),
		Err(_) => Ok(v),
	}
}

fn refused(body: &str, want: &str) {
	match eval(body) {
		Ok(_) => panic!("accepted: {body}"),
		Err(e) => assert!(e.contains(want), "{body}\n  got: {e}\n  want: {want}"),
	}
}

/// A tensor as (dims, dtype, little-endian bytes), from a script value.
fn parts(v: &Value) -> (Vec<usize>, String, Vec<u8>) {
	let t = v.borrow_ref::<rnx_candle::Tensor>().unwrap();
	let t = rnx_candle::tensor_ops::inner(&t);
	(t.dims().to_vec(), format!("{:?}", t.dtype()), bytes(t))
}

fn bytes(t: &CTensor) -> Vec<u8> {
	let mut v = Vec::new();
	t.write_bytes(&mut v).unwrap();
	v
}

fn oracle(t: &CTensor) -> (Vec<usize>, String, Vec<u8>) {
	(t.dims().to_vec(), format!("{:?}", t.dtype()), bytes(t))
}

fn s(p: &Path) -> String {
	p.to_str().unwrap().to_string()
}

#[test]
fn numpy_files_read_exactly_as_candle_reads_them() {
	let mut n = 0;
	for e in std::fs::read_dir(data("")).unwrap() {
		let p = e.unwrap().path();
		let name = p.file_name().unwrap().to_str().unwrap().to_string();
		let valid = name.starts_with("f32-")
			|| name.starts_with("f64-")
			|| name.starts_with("i64-")
			|| name.starts_with("u32-")
			|| name.starts_with("u8-");
		if !valid {
			continue;
		}
		let ours = eval(&format!("candle::Tensor::read_npy({:?})", s(&p))).unwrap();
		let theirs = CTensor::read_npy(&p).unwrap();
		assert_eq!(parts(&ours), oracle(&theirs), "{name}");
		n += 1;
	}
	assert_eq!(n, 36, "every valid fixture");
	// np.savez: names and archive order, all of them and by name
	let p = s(&data("several.npz"));
	let ours = eval(&format!("candle::Tensor::read_npz({p:?})")).unwrap();
	let ours: Vec<(String, Value)> = rune::from_value(ours).unwrap();
	let theirs = CTensor::read_npz(&p).unwrap();
	assert_eq!(ours.len(), theirs.len());
	for ((a, at), (b, bt)) in ours.iter().zip(&theirs) {
		assert_eq!(a, b);
		assert_eq!(parts(at), oracle(bt));
	}
	let by = eval(&format!(
		"candle::Tensor::read_npz_by_name({p:?}, [\"mask\", \"weights\"])"
	))
	.unwrap();
	let by: Vec<Value> = rune::from_value(by).unwrap();
	let theirs = CTensor::read_npz_by_name(&p, &["mask", "weights"]).unwrap();
	for (a, b) in by.iter().zip(&theirs) {
		assert_eq!(parts(a), oracle(b));
	}
}

#[test]
fn writers_are_byte_identical_to_candles() {
	let d = scratch("writers");
	let tensors = [
		(
			"a",
			"candle::Tensor::from_vec([1.5, -2.0, 3.25, 0.0, 5.0, 6.0], [2, 3], \"f32\")?",
		),
		("b", "candle::Tensor::from_vec([1, -2, 3], [3], \"i64\")?"),
		("c", "candle::Tensor::from_vec([7], [], \"u32\")?"),
		("d", "candle::Tensor::zeros([2, 0, 3], \"f64\")?"),
		(
			"e",
			"candle::Tensor::from_vec([1, 2, 3, 4], [2, 2], \"u8\")?.t()?",
		),
	];
	for (name, make) in tensors {
		let ours = d.join(format!("{name}.npy"));
		let theirs = d.join(format!("{name}-candle.npy"));
		let st = d.join(format!("{name}.safetensors"));
		let st_theirs = d.join(format!("{name}-candle.safetensors"));
		let v = eval(&format!(
			"let t = {make}; t.write_npy({:?})?; t.save_safetensors(\"x\", {:?})?; Ok((t, t.write_bytes()?))",
			s(&ours),
			s(&st)
		))
		.unwrap();
		let (t, raw): (Value, rune::runtime::Bytes) = rune::from_value(v).unwrap();
		let t =
			rnx_candle::tensor_ops::inner(&t.borrow_ref::<rnx_candle::Tensor>().unwrap()).clone();
		t.write_npy(&theirs).unwrap();
		t.save_safetensors("x", &st_theirs).unwrap();
		assert_eq!(
			std::fs::read(&ours).unwrap(),
			std::fs::read(&theirs).unwrap(),
			"{name} npy"
		);
		assert_eq!(
			std::fs::read(&st).unwrap(),
			std::fs::read(&st_theirs).unwrap(),
			"{name} safetensors"
		);
		assert_eq!(raw.as_slice(), bytes(&t).as_slice(), "{name} bytes");
	}
	// the archive
	let ours = d.join("all.npz");
	let theirs = d.join("all-candle.npz");
	let v = eval(&format!(
		"let a = candle::Tensor::from_vec([1.5, -2.0], [2], \"f32\")?; let b = candle::Tensor::eye(3, \"u32\")?; candle::Tensor::write_npz([(\"first\", a), (\"second\", b)], {:?})?; Ok([a, b])",
		s(&ours)
	))
	.unwrap();
	let ts: Vec<Value> = rune::from_value(v).unwrap();
	let ts: Vec<CTensor> = ts
		.iter()
		.map(|v| {
			rnx_candle::tensor_ops::inner(&v.borrow_ref::<rnx_candle::Tensor>().unwrap()).clone()
		})
		.collect();
	CTensor::write_npz(&[("first", &ts[0]), ("second", &ts[1])], &theirs).unwrap();
	assert_eq!(
		std::fs::read(&ours).unwrap(),
		std::fs::read(&theirs).unwrap(),
		"npz"
	);
	// and our reader reads both back
	let back = eval(&format!("candle::Tensor::read_npz({:?})", s(&ours))).unwrap();
	let back: Vec<(String, Value)> = rune::from_value(back).unwrap();
	assert_eq!(back[0].0, "first");
	assert_eq!(parts(&back[1].1), oracle(&ts[1]));
}

#[test]
fn writers_create_new_files_only_and_check_first() {
	let d = scratch("refuse-write");
	let existing = d.join("there.npy");
	std::fs::write(&existing, b"keep me").unwrap();
	let t = "candle::Tensor::ones([2], \"f32\")?";
	refused(&format!("{t}.write_npy({:?})", s(&existing)), "exists");
	refused(
		&format!(
			"candle::Tensor::write_npz([(\"a\", {t})], {:?})",
			s(&existing)
		),
		"exists",
	);
	refused(
		&format!("{t}.save_safetensors(\"a\", {:?})", s(&existing)),
		"exists",
	);
	assert_eq!(std::fs::read(&existing).unwrap(), b"keep me");
	// over the limit: refused before the file exists (2^24 f64 = 128 MiB)
	let big = d.join("big.npy");
	refused(
		&format!(
			"candle::Tensor::zeros([16777216], \"f64\")?.write_npy({:?})",
			s(&big)
		),
		"at most",
	);
	assert!(!big.exists());
	let npz = d.join("names.npz");
	for (pairs, want) in [
		(format!("[(\"a\", {t}), (\"a\", {t})]"), "appears twice"),
		(format!("[(\"\", {t})]"), "1 to 255 bytes"),
		(format!("[(\"a/b\", {t})]"), "without '/'"),
		(
			format!("[(\"{}\", {t})]", "n".repeat(256)),
			"1 to 255 bytes",
		),
		("[]".to_string(), "want 1 to 256"),
	] {
		refused(
			&format!("candle::Tensor::write_npz({pairs}, {:?})", s(&npz)),
			want,
		);
		assert!(!npz.exists());
	}
}

/// A hand-built `.npy`: version, header text (padded by us) and data.
fn npy(version: (u8, u8), header: &str, data: &[u8]) -> Vec<u8> {
	let mut h = header.to_string();
	h.push('\n');
	let mut v = b"\x93NUMPY".to_vec();
	v.extend_from_slice(&[version.0, version.1]);
	if version.0 == 1 {
		v.extend_from_slice(&(h.len() as u16).to_le_bytes());
	} else {
		v.extend_from_slice(&(h.len() as u32).to_le_bytes());
	}
	v.extend_from_slice(h.as_bytes());
	v.extend_from_slice(data);
	v
}

#[test]
fn malformed_and_unsupported_npy_files_are_refused_by_name() {
	let d = scratch("npy");
	let ok = "{'descr': '<f4', 'fortran_order': False, 'shape': (2,), }";
	let cases: Vec<(&str, Vec<u8>, &str)> = vec![
		(
			"magic",
			b"\x93NUMPX\x01\x00\x00\x00".to_vec(),
			"no NumPy magic",
		),
		("v3", npy((3, 0), ok, &[0; 8]), "version 3.0"),
		(
			"huge-header",
			{
				let mut v = b"\x93NUMPY\x02\x00".to_vec();
				v.extend_from_slice(&u32::MAX.to_le_bytes());
				v.extend_from_slice(&[0; 90]);
				v
			},
			"at most 65536",
		),
		(
			"header-past-end",
			{
				let mut v = b"\x93NUMPY\x01\x00".to_vec();
				v.extend_from_slice(&5000u16.to_le_bytes());
				v.extend_from_slice(&[b' '; 20]);
				v
			},
			"-byte file",
		),
		(
			"overflow",
			npy(
				(1, 0),
				"{'descr': '<f4', 'fortran_order': False, 'shape': (4294967296, 4294967296, 4294967296), }",
				&[],
			),
			"overflows",
		),
		(
			"too-many",
			npy(
				(1, 0),
				"{'descr': '<f4', 'fortran_order': False, 'shape': (16777217,), }",
				&[],
			),
			"exceeds",
		),
		(
			"rank7",
			npy(
				(1, 0),
				"{'descr': '<f4', 'fortran_order': False, 'shape': (1,1,1,1,1,1,1), }",
				&[0; 4],
			),
			"more than 6",
		),
		(
			"dup-key",
			npy(
				(1, 0),
				"{'descr': '<f4', 'descr': '<f4', 'fortran_order': False, 'shape': (2,), }",
				&[0; 8],
			),
			"appears twice",
		),
		(
			"unknown-key",
			npy(
				(1, 0),
				"{'descr': '<f4', 'fortran_order': False, 'shape': (2,), 'x': 1, }",
				&[0; 8],
			),
			"unknown header key",
		),
		(
			"missing-key",
			npy((1, 0), "{'descr': '<f4', 'shape': (2,), }", &[0; 8]),
			"no fortran_order",
		),
		("trailing", npy((1, 0), ok, &[0; 9]), "want exactly 8"),
		("short", npy((1, 0), ok, &[0; 7]), "want exactly 8"),
		(
			"negative",
			npy(
				(1, 0),
				"{'descr': '<f4', 'fortran_order': False, 'shape': (-2,), }",
				&[],
			),
			"not a non-negative integer",
		),
	];
	for (name, bytes, want) in cases {
		let p = d.join(format!("{name}.npy"));
		std::fs::write(&p, &bytes).unwrap();
		refused(&format!("candle::Tensor::read_npy({:?})", s(&p)), want);
	}
	for (fixture, want) in [
		("f16.npy", "descr \"<f2\""),
		("i32.npy", "descr \"<i4\""),
		("bigendian.npy", "big-endian"),
		("fortran.npy", "fortran_order True"),
	] {
		refused(
			&format!("candle::Tensor::read_npy({:?})", s(&data(fixture))),
			want,
		);
	}
}

/// A stored archive built by the zip crate (as Candle writes them).
fn stored(entries: &[(&str, &[u8])]) -> Vec<u8> {
	use std::io::Write;
	let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
	let o: zip::write::FileOptions<()> =
		zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
	for (n, b) in entries {
		z.start_file(*n, o).unwrap();
		z.write_all(b).unwrap();
	}
	z.finish().unwrap().into_inner()
}

#[test]
fn malformed_and_unsupported_archives_are_refused_by_name() {
	let d = scratch("npz");
	let one = npy(
		(1, 0),
		"{'descr': '<f4', 'fortran_order': False, 'shape': (2,), }",
		&[0; 8],
	);
	let read =
		|p: &Path, want: &str| refused(&format!("candle::Tensor::read_npz({:?})", s(p)), want);
	for (fixture, want) in [
		(
			"compressed.npz",
			"compressed .npz is not supported; save with np.savez",
		),
		("duplicate.npz", "appears twice"),
		("comment.npz", "comment"),
		// the forged record is the last 22 bytes, and no directory ends there
		("forged.npz", "does not end at the end record"),
		("notnpy.npz", "is not a .npy"),
	] {
		read(&data(fixture), want);
	}
	let write = |name: &str, b: &[u8]| {
		let p = d.join(name);
		std::fs::write(&p, b).unwrap();
		p
	};
	// 257 entries: the preflight refuses from the end record
	let names: Vec<String> = (0..257).map(|i| format!("a{i}.npy")).collect();
	let many: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), one.as_slice())).collect();
	read(
		&write("many.npz", &stored(&many)),
		"257 entries, at most 256",
	);
	// trailing data after the end record
	let mut t = stored(&[("a.npy", &one)]);
	t.extend_from_slice(b"junk");
	read(&write("trailing.npz", &t), "last 22 bytes");
	// entry counts disagree
	let mut c = stored(&[("a.npy", &one)]);
	let e = c.len() - 22;
	c[e + 8] = 2;
	read(&write("counts.npz", &c), "entry counts disagree");
	// the directory's size points past the end record
	let mut c = stored(&[("a.npy", &one)]);
	let e = c.len() - 22;
	c[e + 12] = c[e + 12].wrapping_add(1);
	read(&write("dirsize.npz", &c), "does not end at the end record");
	// a central record with a bad signature
	let mut c = stored(&[("a.npy", &one)]);
	let e = c.len() - 22;
	let off = u32::from_le_bytes(c[e + 16..e + 20].try_into().unwrap()) as usize;
	c[off] = b'X';
	read(&write("badsig.npz", &c), "bad signature");
	// a ZIP64 end locator before the end record
	let mut z = stored(&[("a.npy", &one)]);
	let e = z.len() - 22;
	z.splice(e..e, b"PK\x06\x07".iter().copied().chain([0u8; 16]));
	read(&write("zip64.npz", &z), "ZIP64 end record");
	// declared sizes past 64 MiB
	let mut c = stored(&[("a.npy", &one)]);
	let e = c.len() - 22;
	let off = u32::from_le_bytes(c[e + 16..e + 20].try_into().unwrap()) as usize;
	c[off + 20..off + 28].copy_from_slice(&[0, 0, 0, 5, 0, 0, 0, 5]);
	read(&write("declared.npz", &c), "declared sizes exceed");
	// the local header's size disagrees with the directory's
	let mut c = stored(&[("a.npy", &one)]);
	c[18] = c[18].wrapping_add(1);
	read(&write("local.npz", &c), "local sizes disagree");
	// a requested name that is missing, or requested twice
	let p = s(&data("several.npz"));
	refused(
		&format!("candle::Tensor::read_npz_by_name({p:?}, [\"nope\"])"),
		"no array \"nope\"",
	);
	refused(
		&format!("candle::Tensor::read_npz_by_name({p:?}, [\"ids\", \"ids\"])"),
		"appears twice",
	);
}

#[cfg(unix)]
#[test]
fn a_fifo_or_directory_is_refused_at_once_by_every_reader() {
	let d = scratch("fifo");
	let fifo = d.join("pipe.npy");
	let c = std::ffi::CString::new(s(&fifo)).unwrap();
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
	let model = d.join("model");
	std::fs::create_dir(&model).unwrap();
	let config = model.join("config.json");
	let c = std::ffi::CString::new(s(&config)).unwrap();
	assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
	let cases = [
		(
			format!("candle::Tensor::read_npy({:?})", s(&fifo)),
			"not a regular file",
		),
		(
			format!("candle::Tensor::read_npz({:?})", s(&fifo)),
			"not a regular file",
		),
		(
			format!("candle::Mlp::load({:?})", s(&fifo)),
			"not a regular file",
		),
		(
			format!("candle::TextEncoder::load({:?})", s(&model)),
			"not a regular file",
		),
		(
			format!("candle::Tensor::read_npy({:?})", s(&d)),
			"a directory",
		),
	];
	for (body, want) in cases {
		let (tx, rx) = std::sync::mpsc::channel();
		let b = body.clone();
		std::thread::spawn(move || {
			let _ = tx.send(eval(&b).map(|_| ()));
		});
		let got = rx
			.recv_timeout(std::time::Duration::from_secs(10))
			.unwrap_or_else(|_| panic!("blocked: {body}"));
		let e = got.err().unwrap_or_else(|| panic!("accepted: {body}"));
		assert!(e.contains(want), "{body}: {e}");
	}
}

#[test]
fn scatter_is_functional_checked_and_last_wins_on_duplicates() {
	let v = eval(
		"let t = candle::Tensor::zeros([2, 4], \"f32\")?; \
		 let ids = candle::Tensor::from_vec([0, 2, 2, 3, 1, 1], [2, 3], \"u32\")?; \
		 let src = candle::Tensor::from_vec([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3], \"f32\")?; \
		 let out = t.scatter(ids, src, 1)?; Ok((out.to_vec2()?, t.to_vec2()?))",
	)
	.unwrap();
	let (out, t): (Vec<Vec<f64>>, Vec<Vec<f64>>) = rune::from_value(v).unwrap();
	// row 0: 0 <- 1, 2 <- 2 then 3 (later wins); row 1: 3 <- 4, 1 <- 5 then 6
	assert_eq!(
		out,
		vec![vec![1.0, 0.0, 3.0, 0.0], vec![0.0, 6.0, 0.0, 4.0]]
	);
	assert_eq!(t, vec![vec![0.0; 4]; 2], "the input is unchanged");
	let base = "let t = candle::Tensor::zeros([2, 4], \"f32\")?; let src = candle::Tensor::ones([2, 1], \"f32\")?;";
	refused(
		&format!("{base} t.scatter(candle::Tensor::from_vec([0, 4], [2, 1], \"u32\")?, src, 1)"),
		"index 4 is outside",
	);
	refused(
		&format!(
			"{base} t.scatter(candle::Tensor::from_vec([0, 4294967295], [2, 1], \"u32\")?, src, 1)"
		),
		"index 4294967295",
	);
	refused(
		&format!("{base} t.scatter(candle::Tensor::from_vec([0, -1], [2, 1], \"i64\")?, src, 1)"),
		"index -1",
	);
	refused(
		&format!("{base} t.scatter(candle::Tensor::from_vec([0, 1], [2, 1], \"f32\")?, src, 1)"),
		"must be u32, i64 or u8",
	);
	refused(
		&format!("{base} t.scatter(candle::Tensor::from_vec([0], [1, 1], \"u32\")?, src, 1)"),
		"must have the source's shape",
	);
	// slice_scatter: in place of a narrow, functional, bounds checked
	let v = eval(
		"let t = candle::Tensor::zeros([3, 4], \"f32\")?; let src = candle::Tensor::ones([3, 2], \"f32\")?; \
		 Ok((t.slice_scatter(src, 1, 2)?.to_vec2()?, t.slice_scatter0(candle::Tensor::ones([1, 4], \"f32\")?, 2)?.to_vec2()?))",
	)
	.unwrap();
	let (a, b): (Vec<Vec<f64>>, Vec<Vec<f64>>) = rune::from_value(v).unwrap();
	assert_eq!(a, vec![vec![0.0, 0.0, 1.0, 1.0]; 3]);
	assert_eq!(b[2], vec![1.0; 4]);
	assert_eq!(b[0], vec![0.0; 4]);
	refused(
		"candle::Tensor::zeros([3, 4], \"f32\")?.slice_scatter(candle::Tensor::ones([3, 2], \"f32\")?, 1, 3)",
		"exceeds axis 1",
	);
	refused(
		"candle::Tensor::zeros([3, 4], \"f32\")?.slice_scatter(candle::Tensor::ones([3, 2], \"f32\")?, 1, 9223372036854775807)",
		"exceeds axis 1",
	);
	refused(
		"candle::Tensor::zeros([3, 4], \"f32\")?.slice_scatter(candle::Tensor::ones([2, 2], \"f32\")?, 1, 0)",
		"every axis but 1",
	);
}

#[test]
fn cmp_meshgrid_and_from_slice_match_candle() {
	for (op, want) in [
		("eq", [0, 1, 0]),
		("ne", [1, 0, 1]),
		("lt", [1, 0, 0]),
		("le", [1, 1, 0]),
		("gt", [0, 0, 1]),
		("ge", [0, 1, 1]),
	] {
		let v = eval(&format!(
			"let t = candle::Tensor::from_vec([1, 2, 3], [3], \"i64\")?; \
			 Ok((t.cmp(2, {op:?})?.to_vec1()?, t.cmp(candle::Tensor::full(2, [3], \"i64\")?, {op:?})?.to_vec1()?))"
		))
		.unwrap();
		let (a, b): (Vec<i64>, Vec<i64>) = rune::from_value(v).unwrap();
		assert_eq!(a, want.to_vec(), "{op} scalar");
		assert_eq!(b, want.to_vec(), "{op} tensor");
	}
	refused(
		"candle::Tensor::ones([2], \"f32\")?.cmp(1.0, \"approx\")",
		"operator \"approx\"",
	);
	// meshgrid against Candle's own, two and three inputs, xy both ways
	for xy in [false, true] {
		for lens in [vec![2usize, 3], vec![2, 3, 4]] {
			let vs: Vec<String> = lens
				.iter()
				.map(|n| format!("candle::Tensor::arange(0, {n}, \"i64\")?"))
				.collect();
			let v = eval(&format!(
				"candle::Tensor::meshgrid([{}], {xy})",
				vs.join(", ")
			))
			.unwrap();
			let ours: Vec<Value> = rune::from_value(v).unwrap();
			let inputs: Vec<CTensor> = lens
				.iter()
				.map(|&n| CTensor::arange(0i64, n as i64, &Device::Cpu).unwrap())
				.collect();
			let theirs = CTensor::meshgrid(&inputs, xy).unwrap();
			assert_eq!(ours.len(), theirs.len());
			for (a, b) in ours.iter().zip(&theirs) {
				assert_eq!(parts(a), oracle(b), "{lens:?} xy={xy}");
			}
			// Candle's xy with three inputs is not NumPy's (whole-order
			// reversal): the first grid's shape shows it
			if xy && lens.len() == 3 {
				assert_eq!(theirs[0].dims(), &[4, 3, 2]);
			}
		}
	}
	refused(
		"candle::Tensor::meshgrid([candle::Tensor::ones([2], \"f32\")?], false)",
		"2 to 6",
	);
	refused(
		"candle::Tensor::meshgrid([candle::Tensor::zeros([0], \"f32\")?, candle::Tensor::zeros([3], \"f32\")?], false)",
		"zero is refused",
	);
	refused(
		"candle::Tensor::meshgrid([candle::Tensor::zeros([5000], \"f32\")?, candle::Tensor::zeros([5000], \"f32\")?], false)",
		"exceeds",
	);
	// from_slice: one hole, inferred
	let v =
		eval("candle::Tensor::from_slice([1, 2, 3, 4, 5, 6], [-1, 3], \"f32\")?.dims()").unwrap();
	assert_eq!(rune::from_value::<Vec<i64>>(v).unwrap(), vec![2, 3]);
	refused(
		"candle::Tensor::from_slice([1, 2, 3, 4], [-1, -1], \"f32\")",
		"at most one -1",
	);
	refused(
		"candle::Tensor::from_slice([], [-1, 0], \"f32\")",
		"zero-sized known shape",
	);
	refused(
		"candle::Tensor::from_slice([1, 2, 3, 4, 5], [-1, 3], \"f32\")",
		"do not divide",
	);
}

#[test]
fn argmax_ties_go_to_the_lowest_index() {
	let v = eval(
		"candle::Tensor::from_vec([0.5, 0.9, 0.9, 0.1, 0.3, 0.3, 0.3, 0.2], [2, 4], \"f32\")?.argmax(1)?.to_vec1()",
	)
	.unwrap();
	assert_eq!(rune::from_value::<Vec<i64>>(v).unwrap(), vec![1, 0]);
}

/// Review round 1, R1: names and pairs are borrowed, never taken. The
/// script reads its strings, vectors, tuples and tensors after a success
/// and after a refusal on a later item.
#[test]
fn names_and_pairs_stay_the_scripts_after_success_and_refusal() {
	let p = s(&data("several.npz"));
	let v = eval(&format!(
		"let n = \"weights\"; let names = [n]; candle::Tensor::read_npz_by_name({p:?}, names)?; \
		 let a = \"ids\"; let b = \"nope\"; let later = [a, b]; let r = candle::Tensor::read_npz_by_name({p:?}, later); \
		 Ok((n.len(), names.len(), names[0].len(), r.is_err(), a.len(), b.len(), later[1].len()))"
	))
	.unwrap();
	let got: (i64, i64, i64, bool, i64, i64, i64) = rune::from_value(v).unwrap();
	assert_eq!(got, (7, 1, 7, true, 3, 4, 4));
	let d = scratch("borrow");
	let (ok, bad) = (s(&d.join("ok.npz")), s(&d.join("bad.npz")));
	let v = eval(&format!(
		"let n = \"x\"; let t = candle::Tensor::ones([2], \"f32\")?; let pairs = [(n, t)]; \
		 candle::Tensor::write_npz(pairs, {ok:?})?; \
		 let m = \"y\"; let later = [(m, t), (m, t)]; let r = candle::Tensor::write_npz(later, {bad:?}); \
		 Ok((n.len(), pairs.len(), pairs[0].0.len(), pairs[0].1.dims(), r.is_err(), m.len(), later[1].0.len(), t.dims()))"
	))
	.unwrap();
	let got: (i64, i64, i64, Vec<i64>, bool, i64, i64, Vec<i64>) = rune::from_value(v).unwrap();
	assert_eq!(got, (1, 1, 1, vec![2], true, 1, 1, vec![2]));
	assert!(!d.join("bad.npz").exists());
}

/// Review round 1, R2: the safetensors bound counts the header's JSON
/// escaping. 255 bytes of U+0001 escape to 1,530; at this size the file
/// would pass 64 MiB, so it is refused before it exists. The same tensor
/// with a plain 255-byte name fits and is written.
#[test]
fn the_safetensors_bound_counts_json_escaping() {
	let d = scratch("escape");
	let (bad, ok) = (d.join("bad.safetensors"), d.join("ok.safetensors"));
	let control = "\\u{1}".repeat(255);
	refused(
		&format!(
			"candle::Tensor::zeros([16776983], \"f32\")?.save_safetensors(\"{control}\", {:?})",
			s(&bad)
		),
		"at most 67108864",
	);
	assert!(!bad.exists());
	eval(&format!(
		"candle::Tensor::zeros([16776983], \"f32\")?.save_safetensors(\"{}\", {:?})",
		"n".repeat(255),
		s(&ok)
	))
	.unwrap();
	let len = std::fs::metadata(&ok).unwrap().len();
	assert!(len <= 64 << 20, "{len}");
}

/// Review round 1, R3: slice_scatter's transposed intermediates are checked.
/// Here the receiver's own stride products fit (a zero axis), but with
/// axis 1 moved to the front they overflow; refused before Candle, in debug
/// and release alike, and the receiver is still the script's.
#[test]
fn slice_scatter_checks_the_transposed_shapes() {
	let v = eval(
		"let t = candle::Tensor::zeros([8589934592, 0, 8589934592], \"f32\")?; \
		 let r = t.slice_scatter(t, 1, 0); let e = match r { Err(e) => e, Ok(_) => \"\" }; Ok((e != \"\", e, t.dims()))",
	)
	.unwrap();
	let (err, text, dims): (bool, String, Vec<i64>) = rune::from_value(v).unwrap();
	assert!(err);
	assert!(text.contains("the transposed receiver"), "{text}");
	assert_eq!(dims, vec![8589934592, 0, 8589934592]);
}
