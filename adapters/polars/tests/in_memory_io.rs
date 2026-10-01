//! Record 0118: in-memory I/O. A script writes a frame into a `polars::Sink`
//! and reads it back from `Bytes` through the generated readers and
//! writers; the bytes equal what Rust writes for the same frame and
//! options, the frame read back equals Rust's read of the same bytes, and
//! one option per format is respected. A sink's limit is `MaterializeLimit`
//! with nothing committed, and the sink is reusable afterwards.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;
// Record 0130: a test here lowers the process-wide test limit, so every
// test in this binary holds this lock.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut fixtures = Module::with_crate("fx").unwrap();
	rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(polars).unwrap();
	context.install(fixtures).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let built = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	if built.is_err() {
		panic!("diagnostics: {:?}", diagnostics.diagnostics());
	}
	let mut vm = Vm::new(runtime, Arc::new(built.unwrap()));
	vm.call(["main"], ()).map_err(|e| e.to_string())
}

use polars::prelude::*;
use rnx_polars::generated::fixtures::values;

/// The script's written bytes and the frame it read back from them.
fn script_round_trip(writer: &str, reader: &str) -> (Vec<u8>, String) {
	let v = run(&format!(
		"pub fn main() {{ let s = polars::Sink::new(); let w = {writer}; w.finish(fx::df())?; let b = s.bytes()?; let back = {reader}; Ok((b, back)) }}"
	))
	.unwrap();
	let (bytes, back) = rune::from_value::<Result<(rune::Value, rune::Value), rune::Value>>(v)
		.unwrap()
		.unwrap();
	let bytes = rune::from_value::<rune::runtime::Bytes>(bytes)
		.unwrap()
		.as_slice()
		.to_vec();
	let back = rnx_polars::generated::fixtures::show_dataframe(&back)
		.unwrap()
		.to_text();
	(bytes, back)
}

fn repr(df: &DataFrame) -> String {
	rnx_polars::oracle::frame_repr(df).to_text()
}

#[test]
fn csv_round_trip_with_a_separator() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let (bytes, back) = script_round_trip(
		"polars::CsvWriter::new(s).with_separator(59)?",
		"polars::CsvReader::new(b)?.finish()?",
	);
	let mut want = Vec::new();
	CsvWriter::new(&mut want)
		.with_separator(b';')
		.finish(&mut values::df())
		.unwrap();
	assert_eq!(bytes, want, "the script's CSV bytes are Rust's");
	assert!(
		String::from_utf8_lossy(&bytes).contains(';'),
		"the separator is respected"
	);
	let rust_back = CsvReader::new(std::io::Cursor::new(want)).finish().unwrap();
	assert_eq!(back, repr(&rust_back));
}

#[test]
fn parquet_round_trip_uncompressed() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let (bytes, back) = script_round_trip(
		"polars::ParquetWriter::new(s).with_compression(polars::ParquetCompression::Uncompressed())",
		"polars::ParquetReader::new(b)?.finish()?",
	);
	let mut want = Vec::new();
	ParquetWriter::new(&mut want)
		.with_compression(ParquetCompression::Uncompressed)
		.finish(&mut values::df())
		.unwrap();
	assert_eq!(bytes, want, "the script's Parquet bytes are Rust's");
	let rust_back = ParquetReader::new(std::io::Cursor::new(want))
		.finish()
		.unwrap();
	assert_eq!(back, repr(&rust_back));
	assert_eq!(
		back,
		repr(&values::df()),
		"the frame survives the round trip"
	);
}

#[test]
fn ipc_round_trip_lz4() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let (bytes, back) = script_round_trip(
		"polars::IpcWriter::new(s).with_compression(Some(polars::IpcCompression::LZ4()))?",
		"polars::IpcReader::new(b)?.finish()?",
	);
	let mut want = Vec::new();
	IpcWriter::new(&mut want)
		.with_compression(Some(IpcCompression::LZ4))
		.finish(&mut values::df())
		.unwrap();
	assert_eq!(bytes, want, "the script's IPC bytes are Rust's");
	let rust_back = IpcReader::new(std::io::Cursor::new(want)).finish().unwrap();
	assert_eq!(back, repr(&rust_back));
	assert_eq!(back, repr(&values::df()));
}

/// A writer over a sink too small for its output: `MaterializeLimit`, nothing
/// committed; the same sink then takes a write of exactly its bound (the
/// header-less CSV), byte-equal to Rust's.
#[test]
fn a_sink_over_its_limit_commits_nothing_and_is_reusable() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let mut headerless = Vec::new();
	CsvWriter::new(&mut headerless)
		.include_header(false)
		.finish(&mut values::df())
		.unwrap();
	let v = run(&format!(
		"pub fn main() {{ polars::set_materialize_limit({n}); let s = polars::Sink::new(); polars::set_materialize_limit(0); let e = polars::CsvWriter::new(s).finish(fx::df()); let len_after = s.len(); polars::CsvWriter::new(s).include_header(false).finish(fx::df())?; Ok((e.is_err(), match e {{ Err(e) => e.kind(), Ok(_) => \"\" }}, len_after, s.bytes()?)) }}",
		n = headerless.len()
	))
	.unwrap();
	let (failed, kind, len_after, bytes) =
		rune::from_value::<Result<(bool, String, i64, rune::runtime::Bytes), rune::Value>>(v)
			.unwrap()
			.unwrap();
	assert!(failed, "the full CSV is larger than the sink's bound");
	assert_eq!(kind, "MaterializeLimit");
	assert_eq!(len_after, 0, "a failed write commits nothing");
	assert_eq!(
		bytes.as_slice(),
		headerless.as_slice(),
		"the reused sink takes exactly its bound"
	);
}

/// A reader's source is bounded before its one copy.
#[test]
fn reader_bytes_are_bounded() {
	let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
	let v = run(
		"pub fn main() { let s = polars::Sink::new(); polars::CsvWriter::new(s).finish(fx::df())?; let b = s.bytes()?; polars::set_materialize_limit(4); let r = polars::CsvReader::new(b); polars::set_materialize_limit(0); Ok(match r { Err(e) => e.kind(), Ok(_) => \"accepted\" }) }",
	)
	.unwrap();
	let kind = rune::from_value::<Result<String, rune::Value>>(v)
		.unwrap()
		.unwrap();
	assert_eq!(kind, "MaterializeLimit");
}
