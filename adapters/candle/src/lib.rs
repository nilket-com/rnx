//! Record 0129: Candle for rnx, scoped to one workflow: a tensor from rnx's
//! neutral block (`interchange::Dense`), a small fixed multi-layer
//! perceptron run on the CPU, and the result back as a block. Hand-written
//! and minimal; not a parity effort.
//!
//! - `candle::Tensor::from_dense(block)`, `tensor.to_dense(names)`,
//!   `tensor.shape()`, `tensor.dtype()`, and a bounded display.
//! - `candle::Mlp::load(path)`: exactly `fc1.weight [hidden, in]`,
//!   `fc1.bias [hidden]`, `fc2.weight [out, hidden]`, `fc2.bias [out]`, all
//!   F32, from a local safetensors file of at most 64 MiB, read with a
//!   bounded read and parsed from that buffer (never memory-mapped).
//! - `mlp.forward(tensor)`: `Linear`, ReLU, `Linear` on `Device::Cpu`, with
//!   the activation sizes checked before anything runs.
//!
//! Every call runs on a joined worker thread (`worker::run`).
use candle_core::{DType, Device, Module as _, Tensor as CTensor};
use rnx::interchange::{self, Data, Dense};
use rnx::rune::{self, Any, Module};
use std::io::Read;
use std::sync::Arc;

mod display;
pub mod tensor_ops;
pub mod text;
mod worker;

/// A safetensors file is at most 64 MiB.
pub const MAX_FILE: u64 = 64 << 20;
/// Each of the model's dimensions is 1 to 4,096.
pub const MAX_DIM: usize = 4096;

/// A Candle tensor on the CPU.
#[derive(Any, Clone)]
#[rune(item = ::candle)]
pub struct Tensor(pub(crate) CTensor);

/// The fixed model: `Linear`, ReLU, `Linear`, F32.
#[derive(Any, Clone)]
#[rune(item = ::candle)]
pub struct Mlp {
	fc1: candle_nn::Linear,
	fc2: candle_nn::Linear,
	input: usize,
	hidden: usize,
	output: usize,
}

fn cerr(op: &str) -> impl Fn(candle_core::Error) -> String + '_ {
	move |e| format!("{op}: {e}")
}

/// `Tensor::from_dense(block)`: one copy of the block's values into the
/// tensor's storage; the block keeps its buffer. The block has already
/// proved `rows × columns == len` (Candle's `from_vec` does not check it),
/// and the element count is asserted again.
fn from_dense(block: &Dense) -> Result<Tensor, String> {
	let (rows, columns) = (block.rows(), block.columns());
	let t = match block.data() {
		Data::F32(v) => CTensor::from_vec(v.as_ref().clone(), (rows, columns), &Device::Cpu),
		Data::F64(v) => CTensor::from_vec(v.as_ref().clone(), (rows, columns), &Device::Cpu),
	}
	.map_err(cerr("Tensor::from_dense"))?;
	if t.elem_count() != rows * columns {
		return Err(format!(
			"Tensor::from_dense: {} elements for {rows} x {columns}",
			t.elem_count()
		));
	}
	Ok(Tensor(t))
}

/// `tensor.to_dense(names)`: a 2-D F32/F64 tensor as a new block. The
/// shape, dtype and names are checked before the export allocates; one
/// `Vec` is exported and moved into the block without another copy.
fn to_dense(this: &Tensor, names: rune::Value) -> Result<Dense, String> {
	let op = "Tensor::to_dense";
	let names = interchange::names_from(&names, op, "names")?;
	let (rows, columns) = match this.0.dims() {
		[r, c] => (*r, *c),
		d => {
			return Err(format!(
				"{op}: the tensor must be 2-D, found rank {}",
				d.len()
			));
		}
	};
	interchange::check_shape(rows, columns)?;
	interchange::check_names(&names, columns)?;
	let data = match this.0.dtype() {
		DType::F32 => Data::F32(Arc::new(
			this.0
				.flatten_all()
				.and_then(|t| t.to_vec1::<f32>())
				.map_err(cerr(op))?,
		)),
		DType::F64 => Data::F64(Arc::new(
			this.0
				.flatten_all()
				.and_then(|t| t.to_vec1::<f64>())
				.map_err(cerr(op))?,
		)),
		other => {
			return Err(format!(
				"{op}: the tensor must be F32 or F64, found {other:?}"
			));
		}
	};
	Dense::new(data, rows, columns, names)
}

fn shape(this: &Tensor) -> Vec<i64> {
	this.0.dims().iter().map(|d| *d as i64).collect()
}

fn dtype(this: &Tensor) -> String {
	format!("{:?}", this.0.dtype()).to_lowercase()
}

/// The file, bounded: its size from metadata, then a read of at most
/// `MAX_FILE + 1` bytes, refused if it holds more.
fn read_bounded(path: &str) -> Result<Vec<u8>, String> {
	read_limited(path, MAX_FILE, "Mlp::load")
}

/// A local regular file of at most `limit` bytes. Record 0143: opened
/// non-blocking and checked through the same handle (`rnx::fs::regular`'s
/// behaviour), so a FIFO, directory or device is refused at once rather than
/// blocked on; then refused from its metadata before any read, and read with
/// `take(limit + 1)` in case it grew meanwhile.
pub(crate) fn read_limited(path: &str, limit: u64, op: &str) -> Result<Vec<u8>, String> {
	let mut options = std::fs::OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.custom_flags(libc::O_NONBLOCK);
	}
	let file = options
		.open(path)
		.map_err(|e| format!("{op} {path:?}: {e}"))?;
	let meta = file.metadata().map_err(|e| format!("{op} {path:?}: {e}"))?;
	if !meta.is_file() {
		let kind = if meta.is_dir() {
			"a directory"
		} else {
			"not a regular file"
		};
		return Err(format!("{op} {path:?}: it is {kind}"));
	}
	let len = meta.len();
	if len > limit {
		return Err(format!("{op} {path:?}: {len} bytes, at most {limit}"));
	}
	let mut bytes = Vec::with_capacity(len as usize);
	file.take(limit + 1)
		.read_to_end(&mut bytes)
		.map_err(|e| format!("{op} {path:?}: {e}"))?;
	if bytes.len() as u64 > limit {
		return Err(format!("{op} {path:?}: more than {limit} bytes"));
	}
	Ok(bytes)
}

/// `Mlp::load(path)`: exactly the four F32 tensors, their dimensions in
/// range and consistent, at most 2²⁴ parameters.
fn load(path: &str) -> Result<Mlp, String> {
	let op = "Mlp::load";
	let bytes = read_bounded(path)?;
	let path = path.to_owned();
	worker::run(op, move || {
		let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu)
			.map_err(|e| format!("{op} {path:?}: {e}"))?;
		const KEYS: [&str; 4] = ["fc1.weight", "fc1.bias", "fc2.weight", "fc2.bias"];
		let mut keys: Vec<&str> = tensors.keys().map(|k| k.as_str()).collect();
		keys.sort();
		let mut want = KEYS.to_vec();
		want.sort();
		if keys != want {
			return Err(format!(
				"{op} {path:?}: tensors {keys:?}, want exactly {KEYS:?}"
			));
		}
		let get = |k: &str, rank: usize| -> Result<CTensor, String> {
			let t = tensors[k].clone();
			if t.dtype() != DType::F32 {
				return Err(format!("{op} {path:?}: {k} is {:?}, want F32", t.dtype()));
			}
			if t.rank() != rank {
				return Err(format!(
					"{op} {path:?}: {k} has rank {}, want {rank}",
					t.rank()
				));
			}
			Ok(t)
		};
		let (w1, b1, w2, b2) = (
			get("fc1.weight", 2)?,
			get("fc1.bias", 1)?,
			get("fc2.weight", 2)?,
			get("fc2.bias", 1)?,
		);
		let (hidden, input) = (w1.dims()[0], w1.dims()[1]);
		let (output, hidden2) = (w2.dims()[0], w2.dims()[1]);
		for (name, d) in [("in", input), ("hidden", hidden), ("out", output)] {
			if d == 0 || d > MAX_DIM {
				return Err(format!("{op} {path:?}: {name} = {d}, want 1 to {MAX_DIM}"));
			}
		}
		if b1.dims()[0] != hidden || hidden2 != hidden || b2.dims()[0] != output {
			return Err(format!(
				"{op} {path:?}: mismatched dimensions: fc1.weight {:?}, fc1.bias {:?}, fc2.weight {:?}, fc2.bias {:?}",
				w1.dims(),
				b1.dims(),
				w2.dims(),
				b2.dims()
			));
		}
		let params = hidden * input + hidden + output * hidden + output;
		if params > interchange::MAX_VALUES {
			return Err(format!(
				"{op} {path:?}: {params} parameters, at most {}",
				interchange::MAX_VALUES
			));
		}
		Ok(Mlp {
			fc1: candle_nn::Linear::new(w1, Some(b1)),
			fc2: candle_nn::Linear::new(w2, Some(b2)),
			input,
			hidden,
			output,
		})
	})?
}

/// `mlp.forward(tensor)`: a 2-D F32 input with `in` columns; `batch ×
/// hidden` and `batch × out` checked before anything runs.
fn forward(this: &Mlp, x: &Tensor) -> Result<Tensor, String> {
	let op = "Mlp::forward";
	if x.0.dtype() != DType::F32 {
		return Err(format!("{op}: input must be F32, found {:?}", x.0.dtype()));
	}
	let (batch, columns) = match x.0.dims() {
		[b, c] => (*b, *c),
		d => return Err(format!("{op}: input must be 2-D, found rank {}", d.len())),
	};
	if columns != this.input {
		return Err(format!(
			"{op}: input has {columns} columns, the model takes {}",
			this.input
		));
	}
	for (name, width) in [("hidden", this.hidden), ("out", this.output)] {
		match batch.checked_mul(width) {
			Some(n) if n <= interchange::MAX_VALUES => {}
			_ => {
				return Err(format!(
					"{op}: {batch} x {width} {name} values, at most {}",
					interchange::MAX_VALUES
				));
			}
		}
	}
	let (fc1, fc2, x) = (this.fc1.clone(), this.fc2.clone(), x.0.clone());
	worker::run(op, move || {
		fc1.forward(&x)
			.and_then(|h| h.relu())
			.and_then(|h| fc2.forward(&h))
			.map(Tensor)
			.map_err(cerr(op))
	})?
}

fn mlp_dims(this: &Mlp) -> (i64, i64, i64) {
	(this.input as i64, this.hidden as i64, this.output as i64)
}

fn tensor_display(this: &Tensor, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
	use rune::alloc::fmt::TryWrite;
	let text = display::render(&this.0);
	rune::vm_try!(f.try_write_str(&text));
	rune::runtime::VmResult::Ok(())
}

fn err(e: impl std::fmt::Display) -> String {
	e.to_string()
}

pub fn build(m: &mut Module) -> Result<Vec<(String, &'static str)>, String> {
	m.ty::<Tensor>().map_err(err)?;
	m.ty::<Mlp>().map_err(err)?;
	m.function("from_dense", from_dense)
		.build_associated::<Tensor>()
		.map_err(err)?;
	m.associated_function("to_dense", to_dense).map_err(err)?;
	m.associated_function("shape", shape).map_err(err)?;
	m.associated_function("dtype", dtype).map_err(err)?;
	m.associated_function(&rune::runtime::Protocol::DISPLAY_FMT, tensor_display)
		.map_err(err)?;
	m.function("load", load)
		.build_associated::<Mlp>()
		.map_err(err)?;
	m.associated_function("forward", forward).map_err(err)?;
	m.associated_function("dims", mlp_dims).map_err(err)?;
	let ops = tensor_ops::build(m).map_err(err)?;
	let text = text::build(m).map_err(err)?;
	let rerank = text::rerank::build(m).map_err(err)?;
	Ok(ops.into_iter().chain(text).chain(rerank).chain(vec![
		(
			"candle::Tensor".into(),
			"Tensor: a CPU tensor; its display is bounded (dtype, shape, at most 8x8 values)",
		),
		(
			"candle::Tensor::from_dense".into(),
			"from_dense(block) -> Result<Tensor>: an interchange::Dense as a 2-D tensor (one copy)",
		),
		(
			"candle::Tensor::to_dense".into(),
			"to_dense(names) -> Result<interchange::Dense>: a 2-D F32/F64 tensor as a block",
		),
		("candle::Tensor::shape".into(), "shape() -> Vec<i64>"),
		("candle::Tensor::dtype".into(), "dtype() -> String"),
		(
			"candle::Mlp".into(),
			"Mlp: Linear, ReLU, Linear (F32) on the CPU",
		),
		(
			"candle::Mlp::load".into(),
			"load(path) -> Result<Mlp>: exactly fc1.weight/bias, fc2.weight/bias (F32) from a local safetensors file of at most 64 MiB",
		),
		(
			"candle::Mlp::forward".into(),
			"forward(tensor) -> Result<Tensor>: a 2-D F32 input with the model's input columns",
		),
		("candle::Mlp::dims".into(), "dims() -> (in, hidden, out)"),
	]).collect())
}

/// The session presenter: a tensor's bounded display.
pub fn present(presenters: &mut rnx::Presenters) -> Result<(), String> {
	presenters.register::<Tensor>(|t, out| {
		out.push(&display::render(&t.0));
		Ok(())
	})?;
	text::present(presenters)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;

	fn block(rows: usize, columns: usize, f64_: bool) -> Dense {
		let names = (0..columns).map(|i| format!("c{i}")).collect();
		let n = rows * columns;
		let data = if f64_ {
			Data::F64(Arc::new((0..n).map(|i| i as f64 * 0.5).collect()))
		} else {
			Data::F32(Arc::new((0..n).map(|i| i as f32 * 0.5).collect()))
		};
		Dense::new(data, rows, columns, names).unwrap()
	}

	fn names(v: &[&str]) -> rune::Value {
		rune::to_value(v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
	}

	/// A unique path per call, so parallel tests never share a file.
	fn path(tag: &str) -> std::path::PathBuf {
		static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		let d = std::env::temp_dir().join(format!("rnx-0129-candle-{}", std::process::id()));
		std::fs::create_dir_all(&d).unwrap();
		d.join(format!("{tag}-{n}.safetensors"))
	}

	fn weights(entries: Vec<(&str, CTensor)>) -> String {
		let p = path("w");
		let map: HashMap<String, CTensor> = entries
			.into_iter()
			.map(|(k, t)| (k.to_string(), t))
			.collect();
		candle_core::safetensors::save(&map, &p).unwrap();
		p.to_str().unwrap().to_owned()
	}

	fn t(v: Vec<f32>, shape: &[usize]) -> CTensor {
		CTensor::from_vec(v, shape, &Device::Cpu).unwrap()
	}

	fn good(input: usize, hidden: usize, output: usize) -> Vec<(&'static str, CTensor)> {
		vec![
			(
				"fc1.weight",
				t(
					(0..hidden * input)
						.map(|i| (i % 7) as f32 * 0.1 - 0.3)
						.collect(),
					&[hidden, input],
				),
			),
			(
				"fc1.bias",
				t((0..hidden).map(|i| i as f32 * 0.01).collect(), &[hidden]),
			),
			(
				"fc2.weight",
				t(
					(0..output * hidden)
						.map(|i| (i % 5) as f32 * 0.2 - 0.4)
						.collect(),
					&[output, hidden],
				),
			),
			("fc2.bias", t(vec![0.05; output], &[output])),
		]
	}

	#[test]
	fn a_block_round_trips_through_a_tensor_and_stays_usable() {
		for f64_ in [false, true] {
			let b = block(3, 2, f64_);
			let tensor = from_dense(&b).unwrap();
			assert_eq!(shape(&tensor), vec![3, 2]);
			assert_eq!(dtype(&tensor), if f64_ { "f64" } else { "f32" });
			let back = to_dense(&tensor, names(&["x", "y"])).unwrap();
			assert_eq!(
				(back.rows(), back.columns(), back.names().to_vec()),
				(3, 2, vec!["x".to_string(), "y".to_string()])
			);
			match (b.data(), back.data()) {
				(Data::F32(a), Data::F32(c)) => assert_eq!(a, c),
				(Data::F64(a), Data::F64(c)) => assert_eq!(a, c),
				_ => panic!("dtype changed"),
			}
			// the block kept its buffer
			assert_eq!(b.rows(), 3);
		}
	}

	#[test]
	fn to_dense_refuses_rank_names_and_dtype_before_export() {
		let one_d = Tensor(CTensor::zeros(3, DType::F32, &Device::Cpu).unwrap());
		assert!(
			to_dense(&one_d, names(&["a"]))
				.unwrap_err()
				.contains("must be 2-D, found rank 1")
		);
		let two = from_dense(&block(2, 2, false)).unwrap();
		assert!(
			to_dense(&two, names(&["a"]))
				.unwrap_err()
				.contains("1 names for 2 columns")
		);
		assert!(
			to_dense(&two, names(&["a", "a"]))
				.unwrap_err()
				.contains("duplicate column name")
		);
		let u8_ = Tensor(CTensor::zeros((2, 2), DType::U8, &Device::Cpu).unwrap());
		assert!(
			to_dense(&u8_, names(&["a", "b"]))
				.unwrap_err()
				.contains("must be F32 or F64")
		);
		// the names are preflighted, borrowed, before any is copied
		let many: Vec<String> = (0..4097).map(|i| format!("c{i}")).collect();
		let e = to_dense(&two, rune::to_value(many).unwrap()).unwrap_err();
		assert!(e.contains("4097 names, at most 4096"), "{e}");
		let long = rune::to_value(vec!["x".repeat(257), "b".into()]).unwrap();
		let e = to_dense(&two, long.clone()).unwrap_err();
		assert!(e.contains("1 to 256 bytes, found 257"), "{e}");
		assert!(
			to_dense(&two, names(&[]))
				.unwrap_err()
				.contains("at least one column")
		);
		assert!(
			to_dense(&two, rune::to_value("a".to_string()).unwrap())
				.unwrap_err()
				.contains("names must be a vector of names")
		);
		let ok = rune::to_value(vec!["x".repeat(256), "b".into()]).unwrap();
		assert_eq!(to_dense(&two, ok.clone()).unwrap().names()[0].len(), 256);
		// the script's list is still usable after both
		assert_eq!(rune::from_value::<Vec<String>>(ok).unwrap().len(), 2);
		assert_eq!(rune::from_value::<Vec<String>>(long).unwrap().len(), 2);
		let nan = Tensor(t(vec![1.0, f32::NAN], &[1, 2]));
		assert!(
			to_dense(&nan, names(&["a", "b"]))
				.unwrap_err()
				.contains("non-finite")
		);
	}

	#[test]
	fn load_takes_exactly_the_fixed_model() {
		let m = load(&weights(good(3, 4, 1))).unwrap();
		assert_eq!(mlp_dims(&m), (3, 4, 1));
		let refused = |entries: Vec<(&'static str, CTensor)>, want: &str| {
			let e = load(&weights(entries)).err().unwrap();
			assert!(e.contains(want), "{e} (wanted {want})");
		};
		let mut extra = good(3, 4, 1);
		extra.push(("fc3.weight", t(vec![1.0], &[1, 1])));
		refused(extra, "want exactly");
		let mut missing = good(3, 4, 1);
		missing.pop();
		refused(missing, "want exactly");
		let mut rank = good(3, 4, 1);
		rank[1] = ("fc1.bias", t(vec![0.0; 4], &[4, 1]));
		refused(rank, "fc1.bias has rank 2, want 1");
		let mut wrong_dtype = good(3, 4, 1);
		wrong_dtype[0] = (
			"fc1.weight",
			CTensor::zeros((4, 3), DType::F64, &Device::Cpu).unwrap(),
		);
		refused(wrong_dtype, "fc1.weight is F64, want F32");
		let mut mismatched = good(3, 4, 1);
		mismatched[2] = ("fc2.weight", t(vec![0.0; 5], &[1, 5]));
		refused(mismatched, "mismatched dimensions");
		refused(good(1, 4097, 1), "hidden = 4097");
		// an oversized file is refused from its metadata, before any read
		let big = path("big");
		std::fs::File::create(&big)
			.unwrap()
			.set_len(MAX_FILE + 1)
			.unwrap();
		let e = load(big.to_str().unwrap()).err().unwrap();
		assert!(e.contains("at most 67108864"), "{e}");
		assert!(load("/no/such/file.safetensors").is_err());
	}

	#[test]
	fn malformed_safetensors_bytes_are_refused_without_a_panic() {
		let good = std::fs::read(weights(good(3, 4, 1))).unwrap();
		let header = u64::from_le_bytes(good[..8].try_into().unwrap()) as usize;
		let mut cases: Vec<(&str, Vec<u8>)> = vec![
			("empty", vec![]),
			("short", vec![1, 2, 3]),
			(
				"garbage",
				(0..4096u32)
					.map(|i| i.wrapping_mul(2654435761) as u8)
					.collect(),
			),
			// a header length past the end of the file
			("long header", {
				let mut b = good.clone();
				b[..8].copy_from_slice(&(u64::MAX / 2).to_le_bytes());
				b
			}),
			// a header that isn't JSON
			("not json", {
				let mut b = good.clone();
				b[8] = b'#';
				b
			}),
			// the data cut short of the offsets the header declares
			("truncated", good[..8 + header + 4].to_vec()),
		];
		cases.push(("zero header", vec![0; 8]));
		for (what, bytes) in cases {
			let p = path("bad");
			std::fs::write(&p, bytes).unwrap();
			let e = load(p.to_str().unwrap()).err().unwrap();
			assert!(e.starts_with("Mlp::load"), "{what}: {e}");
			assert!(!e.contains("panicked"), "{what}: {e}");
		}
	}

	#[test]
	fn forward_equals_candle_and_refuses_before_running() {
		let w = weights(good(3, 4, 2));
		let m = load(&w).unwrap();
		let x = from_dense(&block(5, 3, false)).unwrap();
		let y = forward(&m, &x).unwrap();
		// the twin: Candle directly, the same weights
		let ts = candle_core::safetensors::load(&w, &Device::Cpu).unwrap();
		let fc1 = candle_nn::Linear::new(ts["fc1.weight"].clone(), Some(ts["fc1.bias"].clone()));
		let fc2 = candle_nn::Linear::new(ts["fc2.weight"].clone(), Some(ts["fc2.bias"].clone()));
		let want = fc2
			.forward(&fc1.forward(&x.0).unwrap().relu().unwrap())
			.unwrap();
		assert_eq!(
			y.0.to_vec2::<f32>().unwrap(),
			want.to_vec2::<f32>().unwrap()
		);
		// the model is reusable, and so is the input
		assert_eq!(
			forward(&m, &x).unwrap().0.to_vec2::<f32>().unwrap(),
			want.to_vec2::<f32>().unwrap()
		);
		// named refusals, before forward runs
		let f64_ = from_dense(&block(5, 3, true)).unwrap();
		assert!(
			forward(&m, &f64_)
				.err()
				.unwrap()
				.contains("input must be F32, found F64")
		);
		let one_d = Tensor(CTensor::zeros(3, DType::F32, &Device::Cpu).unwrap());
		assert!(
			forward(&m, &one_d)
				.err()
				.unwrap()
				.contains("input must be 2-D, found rank 1")
		);
		let cols = from_dense(&block(5, 2, false)).unwrap();
		assert!(
			forward(&m, &cols)
				.err()
				.unwrap()
				.contains("input has 2 columns, the model takes 3")
		);
		// a small input that would make an oversized hidden activation
		let wide = load(&weights(good(1, 4096, 1))).unwrap();
		let tall = from_dense(&block(4097, 1, false)).unwrap();
		assert!(
			forward(&wide, &tall)
				.err()
				.unwrap()
				.contains("4097 x 4096 hidden values, at most 16777216")
		);
	}

	#[test]
	fn the_display_is_bounded_and_reads_only_its_corner() {
		let big = Tensor(CTensor::zeros((1000, 1000), DType::F32, &Device::Cpu).unwrap());
		let text = display::render(&big.0);
		assert!(text.len() <= display::BYTES, "{}", text.len());
		assert!(text.starts_with("Tensor[f32; 1000x1000]"), "{text}");
		assert_eq!(text.lines().count(), 1 + display::ROWS + 1, "{text}");
		assert!(
			text.lines().nth(1).unwrap().ends_with("| …") && text.ends_with('…'),
			"{text}"
		);
		let small = from_dense(&block(2, 2, false)).unwrap();
		assert_eq!(
			display::render(&small.0),
			"Tensor[f32; 2x2]\n0.0 | 0.5\n1.0 | 1.5"
		);
		let one_d = CTensor::zeros(3, DType::F32, &Device::Cpu).unwrap();
		// record 0134: every rank now shows its values
		assert_eq!(display::render(&one_d), "Tensor[f32; 3]\n0.0 | 0.0 | 0.0");
	}

	#[test]
	fn the_worker_joins_and_turns_a_panic_into_an_error() {
		assert_eq!(worker::run("op", || 7).unwrap(), 7);
		let e = worker::run("Mlp::forward", || -> i32 { panic!("boom") }).unwrap_err();
		assert_eq!(e, "Mlp::forward: Candle panicked");
	}
}
