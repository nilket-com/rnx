//! Record 0143: interchange refusals come before any allocation beyond the
//! one bounded file buffer, observed through the allocator's peak. The file
//! buffer is the intentional allocation (at most 64 MiB, the file's own
//! size); everything after it must stay under `SMALL`. One test, so no
//! parallel test moves the peak.
use rnx::allocation::{peak, reset_peak};
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use std::io::Write;
use std::sync::Arc;

/// Everything after the file buffer stays far under a refused allocation.
const SMALL: usize = 256 << 10;

fn vm() -> Vm {
	let mut candle = Module::with_crate("candle").unwrap();
	rnx_candle::build(&mut candle).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(candle).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	let script = r#"
		pub fn npy(p) { candle::Tensor::read_npy(p) }
		pub fn npz(p) { candle::Tensor::read_npz(p) }
		pub fn by_name(p, names) { candle::Tensor::read_npz_by_name(p, names) }
		pub fn write_pairs(pairs, p) { candle::Tensor::write_npz(pairs, p) }
	"#;
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.build()
		.unwrap();
	Vm::new(runtime, Arc::new(unit))
}

fn result(v: Value) -> Result<Value, String> {
	rune::from_value::<Result<Value, Value>>(v)
		.unwrap()
		.map_err(|e| rune::from_value::<String>(e).unwrap())
}

/// The read is refused with `want`; beyond the file buffer (`file` bytes),
/// it allocated under `SMALL`.
fn refused(vm: &mut Vm, f: &str, path: &std::path::Path, want: &str) -> usize {
	let file = std::fs::metadata(path).unwrap().len() as usize;
	let p = path.to_str().unwrap().to_string();
	reset_peak();
	let base = peak();
	let out = result(vm.call([f], (p,)).unwrap());
	let used = peak() - base;
	let e = out.unwrap_err();
	assert!(e.contains(want), "{f}: {e} (wanted {want})");
	assert!(
		used < file + SMALL,
		"{f}: the refusal allocated {used} bytes beyond a {file}-byte file"
	);
	used
}

fn npy_header(version: u8, header: &str) -> Vec<u8> {
	let mut v = b"\x93NUMPY".to_vec();
	v.extend_from_slice(&[version, 0]);
	let h = format!("{header}\n");
	if version == 1 {
		v.extend_from_slice(&(h.len() as u16).to_le_bytes());
	} else {
		v.extend_from_slice(&(h.len() as u32).to_le_bytes());
	}
	v.extend_from_slice(h.as_bytes());
	v
}

#[test]
fn refusals_allocate_nothing_proportional_beyond_the_file_buffer() {
	let d = std::env::temp_dir().join(format!("rnx-0143-alloc-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&d);
	std::fs::create_dir_all(&d).unwrap();
	let mut vm = vm();
	let write = |name: &str, b: &[u8]| {
		let p = d.join(name);
		std::fs::write(&p, b).unwrap();
		p
	};
	// a 4 GiB declared header in a 100-byte file (Candle would allocate it)
	let mut huge = b"\x93NUMPY\x02\x00".to_vec();
	huge.extend_from_slice(&u32::MAX.to_le_bytes());
	huge.extend_from_slice(&[b' '; 88]);
	refused(&mut vm, "npy", &write("header.npy", &huge), "at most 65536");
	// a shape whose product overflows, and one over the cap (Candle would
	// allocate its element count before reading)
	refused(
		&mut vm,
		"npy",
		&write(
			"overflow.npy",
			&npy_header(
				1,
				"{'descr': '<f8', 'fortran_order': False, 'shape': (4294967296, 4294967296, 4294967296), }",
			),
		),
		"overflows",
	);
	refused(
		&mut vm,
		"npy",
		&write(
			"cap.npy",
			&npy_header(
				1,
				"{'descr': '<f8', 'fortran_order': False, 'shape': (1000000000,), }",
			),
		),
		"exceeds",
	);
	// many tiny entries: the preflight refuses from the end record, before
	// the zip crate builds metadata for each of them
	let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
	let o: zip::write::FileOptions<()> =
		zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
	for i in 0..20_000 {
		z.start_file(format!("{i}.npy"), o).unwrap();
		z.write_all(b"x").unwrap();
	}
	let many = z.finish().unwrap().into_inner();
	// the count field wraps at 65,535; 20,000 entries are within it
	refused(
		&mut vm,
		"npz",
		&write("many.npz", &many),
		"20000 entries, at most 256",
	);
	// an entry whose .npy declares a huge array inside a valid archive
	let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
	z.start_file("a.npy", o).unwrap();
	z.write_all(&npy_header(
		1,
		"{'descr': '<f8', 'fortran_order': False, 'shape': (1000000000,), }",
	))
	.unwrap();
	let inner = z.finish().unwrap().into_inner();
	refused(&mut vm, "npz", &write("inner.npz", &inner), "exceeds");
	// review round 1: a 10 MiB name is refused from its length, borrowed,
	// with a bounded diagnostic and nothing copied (the name exists before
	// the measurement starts)
	let big = "n".repeat(10 << 20);
	let names = rune::to_value(vec![big.clone()]).unwrap();
	let several =
		std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/0143/several.npz");
	reset_peak();
	let base = peak();
	let e = result(
		vm.call(["by_name"], (several.to_str().unwrap().to_string(), names))
			.unwrap(),
	)
	.unwrap_err();
	let used = peak() - base;
	assert!(
		e.contains("a name of 10485760 bytes") && e.len() < 200,
		"{e}"
	);
	assert!(used < SMALL, "by_name allocated {used} bytes");
	let one = result(
		vm.call(
			["npy"],
			(write("one.npy", &one_npy()).to_str().unwrap().to_string(),),
		)
		.unwrap(),
	)
	.unwrap();
	let pairs = rune::to_value(vec![(big, one)]).unwrap();
	let out = d.join("big-name.npz").to_str().unwrap().to_string();
	reset_peak();
	let base = peak();
	let e = result(vm.call(["write_pairs"], (pairs, out)).unwrap()).unwrap_err();
	let used = peak() - base;
	assert!(
		e.contains("a name of 10485760 bytes") && e.len() < 200,
		"{e}"
	);
	assert!(used < SMALL, "write_npz allocated {used} bytes");
	let _ = std::fs::remove_dir_all(&d);
}

fn one_npy() -> Vec<u8> {
	let mut v = npy_header(
		1,
		"{'descr': '<f4', 'fortran_order': False, 'shape': (1,), }",
	);
	v.extend_from_slice(&[0; 4]);
	v
}
