//! Record 0129: a tensor's bounded display. The dtype and shape, then at
//! most 8 rows × 8 columns of a 2-D tensor, read with `narrow` (only those
//! values are copied), each value whole in its round-trip text, with `…`
//! markers for what is omitted. At most 2,048 bytes; the tensor is never
//! formatted whole. Other ranks show the header only.
use candle_core::{DType, Tensor};

pub(crate) const ROWS: usize = 8;
pub(crate) const COLUMNS: usize = 8;
pub(crate) const BYTES: usize = 2048;

pub(crate) fn render(t: &Tensor) -> String {
	let dims: Vec<String> = t.dims().iter().map(|d| d.to_string()).collect();
	let mut text = format!(
		"Tensor[{}; {}]",
		format!("{:?}", t.dtype()).to_lowercase(),
		dims.join("x")
	);
	let (rows, columns) = match t.dims() {
		[r, c] => (*r, *c),
		_ => {
			text.push_str(" (values shown for 2-D tensors only)");
			return text;
		}
	};
	let (r, c) = (rows.min(ROWS), columns.min(COLUMNS));
	let view = t.narrow(0, 0, r).and_then(|v| v.narrow(1, 0, c));
	let values: Result<Vec<Vec<String>>, _> = match t.dtype() {
		DType::F32 => view.and_then(|v| v.to_vec2::<f32>()).map(|vv| {
			vv.into_iter()
				.map(|row| row.into_iter().map(|x| format!("{x:?}")).collect())
				.collect()
		}),
		DType::F64 => view.and_then(|v| v.to_vec2::<f64>()).map(|vv| {
			vv.into_iter()
				.map(|row| row.into_iter().map(|x| format!("{x:?}")).collect())
				.collect()
		}),
		_ => {
			text.push_str(" (values shown for f32 and f64 only)");
			return text;
		}
	};
	let Ok(values) = values else {
		text.push_str(" (values unavailable)");
		return text;
	};
	for row in values {
		let mut line = format!("\n{}", row.join(" | "));
		if columns > c {
			line.push_str(" | …");
		}
		if text.len() + line.len() + 4 > BYTES {
			text.push_str("\n…");
			return text;
		}
		text.push_str(&line);
	}
	if rows > r {
		text.push_str("\n…");
	}
	text
}
