//! Retained credit for checked CPU tensor and model producers.
use super::{CTensor, Data, Dense, Mlp, Tensor, from_dense};
use candle_core::{CpuStorage, Storage};
use rnx::rune::{host::OpaqueProducerLimit as Limit, runtime::AnyObj};
use std::{alloc::Layout, ops::Deref, sync::RwLock};

#[repr(C)]
struct ArcAllocation<T> {
	counts: [usize; 2],
	data: T,
}

const fn arc_bytes<T>() -> usize {
	// std::sync::ArcInner has two atomic usize counters before its data.
	Layout::new::<ArcAllocation<T>>().size()
}

// Candle 0.11.0 builds two Arcs, a two-element shape Vec and a stride
// Vec collected through scan (capacity four for the first two strides).
const TENSOR_ARCS: usize =
	arc_bytes::<<CTensor as Deref>::Target>() + arc_bytes::<RwLock<Storage>>();
const METADATA: usize = TENSOR_ARCS + 6 * size_of::<usize>();

pub(super) fn convert(block: &Dense, credit: usize) -> Result<Result<Tensor, String>, Limit> {
	let width = match block.data() {
		Data::F32(_) => size_of::<f32>(),
		Data::F64(_) => size_of::<f64>(),
	};
	let bytes = block
		.rows()
		.checked_mul(block.columns())
		.and_then(|n| n.checked_mul(width))
		.and_then(|n| n.checked_add(METADATA))
		.ok_or(Limit)?;
	if bytes > credit {
		return Err(Limit);
	}
	Ok(from_dense(block))
}

pub(super) fn tensor_bytes(value: &Tensor) -> usize {
	// The native box stays live after the temporary Result and input packet
	// disappear. Count backing storage, even when looking through a view.
	let (storage, _) = value.0.storage_and_layout();
	let data = match &*storage {
		Storage::Cpu(CpuStorage::F32(v)) => v.capacity().checked_mul(size_of::<f32>()),
		Storage::Cpu(CpuStorage::F64(v)) => v.capacity().checked_mul(size_of::<f64>()),
		_ => None,
	};
	data.and_then(|n| n.checked_add(METADATA + AnyObj::owned_allocation_bytes::<Tensor>()))
		.unwrap_or(usize::MAX)
}

/// Loading constructs four independent contiguous F32 CPU tensors. Their
/// cloned handles in the worker's lookup table are gone at adoption.
pub(super) fn model_bytes(model: &Mlp) -> usize {
	let mut total = AnyObj::owned_allocation_bytes::<Mlp>();
	for layer in [&model.fc1, &model.fc2] {
		let Some(bias) = layer.bias() else {
			return usize::MAX;
		};
		for tensor in [layer.weight(), bias] {
			let (storage, layout) = tensor.storage_and_layout();
			let Storage::Cpu(CpuStorage::F32(values)) = &*storage else {
				return usize::MAX;
			};
			// The loader builds rank-one and rank-two shapes from a slice;
			// the contiguous stride Vec starts with capacity four in both.
			let metadata = TENSOR_ARCS + (layout.dims().len() + 4) * size_of::<usize>();
			total = total
				.saturating_add(values.capacity().saturating_mul(size_of::<f32>()))
				.saturating_add(metadata);
		}
	}
	total
}

#[cfg(test)]
mod tests {
	use super::*;
	use rnx::allocation::{live, peak, reset_peak};
	use std::{process::Command, sync::Arc};

	#[test]
	fn one_buffer_at_exact_credit() {
		// Other lib tests allocate in parallel. Measure in a single-test process.
		const CHILD: &str = "RNX_TEST_CANDLE_EXACT_CREDIT";
		if std::env::var_os(CHILD).is_none() {
			let child = Command::new(std::env::current_exe().unwrap())
				.args([
					"--exact",
					std::thread::current().name().unwrap(),
					"--nocapture",
				])
				.env(CHILD, "1")
				.output()
				.unwrap();
			eprint!("{}", String::from_utf8_lossy(&child.stderr));
			let results = String::from_utf8(child.stdout).unwrap();
			assert!(
				child.status.success() && results.contains("test result: ok. 1 passed;"),
				"{results}"
			);
			return;
		}
		// Measure what is actually freed once the joined loader has released
		// its transient buffers. The Rune box is added separately at adoption.
		let model = crate::load(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/../../demos/data/mlp.safetensors"
		))
		.unwrap();
		let expected = model_bytes(&model) - AnyObj::owned_allocation_bytes::<Mlp>();
		let held = live().unwrap();
		drop(model);
		assert_eq!(held - live().unwrap(), expected, "four model tensors");
		for rows in [1, 257, 65536] {
			for width in [4, 8] {
				let n = rows * 2;
				let data = if width == 4 {
					Data::F32(Arc::new((0..n).map(|x| x as f32 - 0.5).collect()))
				} else {
					Data::F64(Arc::new((0..n).map(|x| x as f64 + 0.25).collect()))
				};
				let block = Dense::new(data, rows, 2, vec!["a".into(), "b".into()]).unwrap();
				let charge = n * width + METADATA;
				reset_peak();
				let initial = peak();
				let short = convert(&block, charge - 1);
				assert!(short.is_err());
				assert_eq!(peak(), initial, "short credit allocated");
				let tensor = convert(&block, charge).unwrap().unwrap();
				let retained = live().unwrap() - initial;
				let high = peak() - initial;
				assert_eq!(retained, charge);
				assert_eq!(high, charge);
				assert_eq!(
					tensor_bytes(&tensor),
					charge + AnyObj::owned_allocation_bytes::<Tensor>()
				);
				assert_eq!(tensor.0.dims(), [rows, 2]);
				let (storage, _) = tensor.0.storage_and_layout();
				match (&*storage, block.data()) {
					(Storage::Cpu(CpuStorage::F32(a)), Data::F32(b)) => {
						assert_eq!(a, b.as_ref());
						assert_ne!(a.as_ptr(), b.as_ptr());
					}
					(Storage::Cpu(CpuStorage::F64(a)), Data::F64(b)) => {
						assert_eq!(a, b.as_ref());
						assert_ne!(a.as_ptr(), b.as_ptr());
					}
					_ => panic!("unexpected tensor storage"),
				}
				drop(storage);
				drop(tensor);
				assert_eq!(live().unwrap(), initial);
				eprintln!(
					"{rows}x2, {width} bytes per cell: payload {}, metadata {METADATA}, peak {high}",
					n * width
				);
			}
		}
	}
}
