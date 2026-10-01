//! Record 0129, widened by 0134: a tensor's bounded display. The dtype and
//! full shape, then the values a result kind needs to be visible:
//!
//! - rank 0: the value;
//! - rank 1: the first 8 values;
//! - rank 2: the 8 × 8 corner;
//! - rank 3 to 6: the corner of the first 2-D slice, labelled as such.
//!
//! Every dtype shows its values faithfully (u8 255 is 255; a mask shows 0
//! and 1 because that is what it holds; NaN and ±inf as such). Only the
//! shown corner is read (`narrow`, then a copy of at most 64 values); the
//! tensor is never formatted whole. At most 2,048 bytes.
use candle_core::{DType, Tensor};

pub(crate) const ROWS: usize = 8;
pub(crate) const COLUMNS: usize = 8;
pub(crate) const BYTES: usize = 2048;

/// A ≤ 8 × 8 2-D view's values as text, in the dtype's own form.
fn cells(view: &Tensor) -> candle_core::Result<Vec<Vec<String>>> {
	fn text<T: std::fmt::Debug>(v: Vec<Vec<T>>) -> Vec<Vec<String>> {
		v.into_iter()
			.map(|row| row.into_iter().map(|x| format!("{x:?}")).collect())
			.collect()
	}
	Ok(match view.dtype() {
		DType::F32 => text(view.to_vec2::<f32>()?),
		DType::F64 => text(view.to_vec2::<f64>()?),
		DType::I64 => text(view.to_vec2::<i64>()?),
		DType::U32 => text(view.to_vec2::<u32>()?),
		DType::U8 => text(view.to_vec2::<u8>()?),
		other => vec![vec![format!("({other:?} values not shown)")]],
	})
}

pub(crate) fn header(t: &Tensor) -> String {
	let dims: Vec<String> = t.dims().iter().map(|d| d.to_string()).collect();
	format!(
		"Tensor[{}; {}]",
		format!("{:?}", t.dtype()).to_lowercase(),
		if dims.is_empty() {
			"scalar".to_string()
		} else {
			dims.join("x")
		}
	)
}

pub(crate) fn render(t: &Tensor) -> String {
	let mut text = header(t);
	// the 2-D view to show: rank 0 and 1 are lifted, rank ≥ 3 is sliced
	let view = match t.rank() {
		0 => t.reshape((1, 1)),
		1 => t.unsqueeze(0),
		2 => Ok(t.clone()),
		r => {
			let mut v = t.clone();
			for _ in 0..r - 2 {
				if v.dims()[0] == 0 {
					break;
				}
				v = match v.narrow(0, 0, 1).and_then(|x| x.squeeze(0)) {
					Ok(x) => x,
					Err(_) => break,
				};
			}
			text.push_str(&format!(" (the [{}] slice)", vec!["0"; r - 2].join(", ")));
			Ok(v)
		}
	};
	let Ok(view) = view else {
		text.push_str(" (values unavailable)");
		return text;
	};
	let [rows, columns] = view.dims() else {
		return text;
	};
	let (rows, columns) = (*rows, *columns);
	let (r, c) = (rows.min(ROWS), columns.min(COLUMNS));
	if r == 0 || c == 0 {
		text.push_str(" (empty)");
		return text;
	}
	let corner = view.narrow(0, 0, r).and_then(|v| v.narrow(1, 0, c));
	let Ok(values) = corner.and_then(|v| cells(&v)) else {
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
