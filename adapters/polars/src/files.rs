use crate::p;
use p::SerReader;
use std::{
	fs::{File, OpenOptions},
	io::{Seek, Write},
	sync::Arc,
};

fn regular(path: &str) -> Result<File, String> {
	let mut options = OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.custom_flags(libc::O_NONBLOCK);
	}
	let file = options
		.open(path)
		.map_err(|e| format!("polars read {path:?}: {e}"))?;
	if !file
		.metadata()
		.map_err(|e| format!("polars read {path:?}: {e}"))?
		.is_file()
	{
		return Err(format!("polars read {path:?}: not a regular file"));
	}
	Ok(file)
}
pub(crate) fn validate(frame: &p::DataFrame) -> Result<(), String> {
	for column in frame.columns() {
		if !matches!(
			column.dtype(),
			p::DataType::String | p::DataType::Int64 | p::DataType::Float64 | p::DataType::Boolean
		) {
			return Err(format!(
				"polars: unsupported column {:?} with dtype {}",
				column.name(),
				column.dtype()
			));
		}
	}
	Ok(())
}
pub(crate) fn csv(path: &str, schema: Arc<p::Schema>) -> Result<p::DataFrame, String> {
	let mut file = regular(path)?;
	csv_handle(&mut file, schema, || Ok(())).map_err(|e| format!("polars read_csv {path:?}: {e}"))
}
fn csv_handle(
	file: &mut File,
	schema: Arc<p::Schema>,
	after_header: impl FnOnce() -> Result<(), String>,
) -> Result<p::DataFrame, String> {
	let header = p::CsvReadOptions::default()
		.with_has_header(false)
		.with_infer_schema_length(Some(0))
		.with_n_rows(Some(1))
		.into_reader_with_file_handle(&mut *file)
		.finish()
		.map_err(|e| e.to_string())?;
	if header.height() != 1 || header.width() != schema.len() {
		return Err("header arity does not match schema".into());
	}
	for (column, name) in header.columns().iter().zip(schema.iter_names()) {
		if column.str().map_err(|e| e.to_string())?.get(0) != Some(name.as_str()) {
			return Err("header names or order do not match schema".into());
		}
	}
	drop(header);
	after_header()?;
	file.rewind().map_err(|e| e.to_string())?;
	let frame = p::CsvReadOptions::default()
		.with_has_header(true)
		.with_schema(Some(schema))
		.into_reader_with_file_handle(file)
		.finish()
		.map_err(|e| e.to_string())?;
	validate(&frame)?;
	Ok(frame)
}
pub(crate) fn parquet(path: &str) -> Result<p::DataFrame, String> {
	let frame = p::ParquetReader::new(regular(path)?)
		.finish()
		.map_err(|e| format!("polars read_parquet {path:?}: {e}"))?;
	validate(&frame)?;
	Ok(frame)
}
pub(crate) fn write(mut frame: p::DataFrame, path: &str) -> Result<(), String> {
	let mut file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)
		.map_err(|e| format!("polars write_parquet_new {path:?}: {e}"))?;
	write_to(&mut frame, &mut file).map_err(|e| format!("polars write_parquet_new {path:?}: {e}"))
}
fn write_to(frame: &mut p::DataFrame, writer: &mut (impl Write + Send)) -> Result<(), String> {
	p::ParquetWriter::new(&mut *writer)
		.with_compression(p::ParquetCompression::Uncompressed)
		.finish(frame)
		.map_err(|e| e.to_string())?;
	writer.flush().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use p::NamedFrom;
	use std::{
		io,
		path::PathBuf,
		sync::atomic::{AtomicU64, Ordering},
	};
	static NEXT: AtomicU64 = AtomicU64::new(0);
	struct Temp(PathBuf);
	impl Temp {
		fn new() -> Self {
			let path = std::env::temp_dir().join(format!(
				"rnx-polars-{}-{}",
				std::process::id(),
				NEXT.fetch_add(1, Ordering::Relaxed)
			));
			std::fs::create_dir(&path).unwrap();
			Self(path)
		}
	}
	impl Drop for Temp {
		fn drop(&mut self) {
			std::fs::remove_dir_all(&self.0).unwrap();
		}
	}
	#[test]
	fn header_and_data_use_same_open_file() {
		crate::engine::run("files::csv_roundtrip", || {
			let temp = Temp::new();
			let path = temp.0.join("input.csv");
			std::fs::write(&path, "k,v\na,7\n").unwrap();
			let mut file = regular(path.to_str().unwrap()).unwrap();
			let schema = Arc::new(p::Schema::from_iter([
				("k".into(), p::DataType::String),
				("v".into(), p::DataType::Int64),
			]));
			let frame = csv_handle(&mut file, schema, || {
				std::fs::rename(&path, temp.0.join("original.csv")).unwrap();
				std::fs::write(&path, "k,v\nb,99\n").unwrap();
				Ok(())
			})
			.unwrap();
			assert_eq!(frame.column("v").unwrap().i64().unwrap().get(0), Some(7));
			assert_eq!(std::fs::read_to_string(&path).unwrap(), "k,v\nb,99\n");
		})
		.unwrap();
	}
	struct FaultWriter {
		file: File,
		left: Option<usize>,
		fail_flush_at: Option<usize>,
		flushes: usize,
	}
	impl Write for FaultWriter {
		fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
			match &mut self.left {
				Some(0) => Err(io::Error::other("injected write failure")),
				Some(left) => {
					let n = self.file.write(&bytes[..bytes.len().min(*left)])?;
					*left -= n;
					Ok(n)
				}
				None => self.file.write(bytes),
			}
		}
		fn flush(&mut self) -> io::Result<()> {
			self.flushes += 1;
			if self.fail_flush_at == Some(self.flushes) {
				Err(io::Error::other("injected flush failure"))
			} else {
				self.file.flush()
			}
		}
	}
	#[test]
	fn failed_write_and_flush_leave_created_file() {
		crate::engine::run("files::csv_partial", || {
			let temp = Temp::new();
			for (name, left, flush) in [
				("partial", Some(16), None),
				("engine-flush", None, Some(1)),
				("final-flush", None, Some(2)),
			] {
				let path = temp.0.join(name);
				let mut frame =
					p::DataFrame::new(2, vec![p::Series::new("v".into(), [1i64, 2]).into()])
						.unwrap();
				let file = OpenOptions::new()
					.write(true)
					.create_new(true)
					.open(&path)
					.unwrap();
				let mut writer = FaultWriter {
					file,
					left,
					fail_flush_at: flush,
					flushes: 0,
				};
				let error = write_to(&mut frame, &mut writer).unwrap_err();
				if flush.is_some() {
					assert!(error.contains("injected flush failure"), "{error}");
				} else {
					assert_eq!(writer.left, Some(0));
					assert!(!error.is_empty());
				}

				drop(writer);
				assert!(path.metadata().unwrap().len() > 0);
				println!(
					"{name}: error={error:?}; retained_bytes={}; retry_refuses_existing=true",
					path.metadata().unwrap().len()
				);
				if flush.is_none() {
					assert_eq!(path.metadata().unwrap().len(), 16);
				}
				assert!(
					write(frame, path.to_str().unwrap())
						.unwrap_err()
						.contains("exist")
				);
			}
		})
		.unwrap();
	}
}
