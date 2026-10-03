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

/// Record 0161: what a tensor's display shows, structurally, for the HTML form. It reads
/// exactly what [`render`] reads (the same view, the same corner, the same dtype text); `render`
/// itself is left as it was, and the tests hold its bytes and these values equal.
enum Shown {
	/// The header (and slice note) alone: a higher rank whose leading axis is zero.
	Header,
	/// A status after the header, on the same line in the text: ` (empty)`, ` (values unavailable)`.
	Status(&'static str),
	/// A line instead of values: an unsupported dtype's `(BF16 values not shown)`.
	Note(String),
	/// The corner's cells, and whether rows or columns were left out.
	Values {
		rank: usize,
		cells: Vec<Vec<String>>,
		more_rows: bool,
		more_columns: bool,
	},
}

fn shown(t: &Tensor) -> (String, Shown) {
	let mut title = header(t);
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
			title.push_str(&format!(" (the [{}] slice)", vec!["0"; r - 2].join(", ")));
			Ok(v)
		}
	};
	let Ok(view) = view else {
		return (title, Shown::Status(" (values unavailable)"));
	};
	let [rows, columns] = view.dims() else {
		return (title, Shown::Header);
	};
	let (rows, columns) = (*rows, *columns);
	let (r, c) = (rows.min(ROWS), columns.min(COLUMNS));
	if r == 0 || c == 0 {
		return (title, Shown::Status(" (empty)"));
	}
	let corner = view.narrow(0, 0, r).and_then(|v| v.narrow(1, 0, c));
	let Ok(values) = corner.and_then(|v| cells(&v)) else {
		return (title, Shown::Status(" (values unavailable)"));
	};
	if !matches!(
		view.dtype(),
		DType::F32 | DType::F64 | DType::I64 | DType::U32 | DType::U8
	) {
		let note = values
			.into_iter()
			.next()
			.and_then(|row| row.into_iter().next())
			.unwrap_or_default();
		return (title, Shown::Note(note));
	}
	let shown = Shown::Values {
		rank: t.rank(),
		cells: values,
		more_rows: rows > r,
		more_columns: columns > c,
	};
	(title, shown)
}

/// The usable bytes of a tensor's HTML form: below rnx's `HTML_BYTES` with room for `Output`'s
/// generic marker, so the writer ends its own fragment and generic truncation never fires.
const HTML_USABLE: usize = rnx::present::HTML_BYTES - 64;
const HTML_OMITTED: &str = "<small>[display byte limit; remainder omitted]</small>";
/// The most any cut needs to close (review of 0161, R1): after the head, an empty body as well
/// as the table, then the marker. Reserved before every unit, whatever the state.
const CLOSING: usize = "<tbody></tbody></table>".len() + HTML_OMITTED.len() + "</div>".len();

/// Text as HTML text: `&`, `<`, `>`, `"` and `'` as entities, a character at a time.
fn html_text(text: &str, out: &mut String) {
	for ch in text.chars() {
		match ch {
			'&' => out.push_str("&amp;"),
			'<' => out.push_str("&lt;"),
			'>' => out.push_str("&gt;"),
			'"' => out.push_str("&quot;"),
			'\'' => out.push_str("&#39;"),
			ch => out.push(ch),
		}
	}
}

fn cell(tag: &str, text: &str, out: &mut String) {
	out.push('<');
	out.push_str(tag);
	out.push('>');
	html_text(text, out);
	out.push_str("</");
	out.push_str(tag);
	out.push('>');
}

/// Record 0161: the tensor's HTML form, from the same reading as the text: the header (and slice
/// note) in `<small>`, then the shown corner as a table. Rank 2 and the shown slice of higher
/// ranks have column indices in the head and a row index per row; rank 1 has column indices and
/// one row; rank 0 is one cell. Omitted rows and columns are a final `…` row and column. A status
/// or a values-not-shown note follows the header, with no table, as the text has no values there.
/// No attribute, class, style or script.
pub(crate) fn render_html(t: &Tensor) -> String {
	render_html_within(t, HTML_USABLE)
}

pub(crate) fn render_html_within(t: &Tensor, usable: usize) -> String {
	let (title, shown) = shown(t);
	let mut html = String::from("<div><small>");
	html_text(&title, &mut html);
	match shown {
		Shown::Header => {}
		Shown::Status(status) => html_text(status, &mut html),
		Shown::Note(note) => {
			html.push_str("</small><div>");
			html_text(&note, &mut html);
			html.push_str("</div></div>");
			return html;
		}
		Shown::Values {
			rank,
			cells,
			more_rows,
			more_columns,
		} => {
			html.push_str("</small><table>");
			let width = cells.first().map_or(0, Vec::len);
			let indexed = rank >= 2;
			let mut units = Vec::new();
			if rank >= 1 {
				let mut head = String::from("<thead><tr>");
				if indexed {
					head.push_str("<th></th>");
				}
				for i in 0..width {
					cell("th", &i.to_string(), &mut head);
				}
				if more_columns {
					cell("th", "…", &mut head);
				}
				head.push_str("</tr></thead>");
				units.push(head);
			}
			units.push("<tbody>".to_owned());
			for (i, row) in cells.iter().enumerate() {
				let mut line = String::from("<tr>");
				if indexed {
					cell("th", &i.to_string(), &mut line);
				}
				for value in row {
					cell("td", value, &mut line);
				}
				if more_columns {
					cell("td", "…", &mut line);
				}
				line.push_str("</tr>");
				units.push(line);
			}
			if more_rows {
				let mut line = String::from("<tr>");
				cell("th", "…", &mut line);
				for _ in 0..width + usize::from(more_columns) {
					cell("td", "…", &mut line);
				}
				line.push_str("</tr>");
				units.push(line);
			}
			// whole units, each checked with room for the closing and the marker
			for unit in units {
				if html.len() + unit.len() + CLOSING > usable {
					if !html.ends_with("<table>") && !html.ends_with("</thead>") {
						html.push_str("</tbody>");
					} else if html.ends_with("</thead>") {
						html.push_str("<tbody></tbody>");
					}
					html.push_str("</table>");
					html.push_str(HTML_OMITTED);
					html.push_str("</div>");
					return html;
				}
				html.push_str(&unit);
			}
			html.push_str("</tbody></table></div>");
			return html;
		}
	}
	html.push_str("</small></div>");
	html
}

#[cfg(test)]
pub(crate) mod cases {
	//! Record 0161: the display's edge cases, shared by the text goldens and the HTML tests.
	use candle_core::{DType, Device, Tensor};

	pub(crate) fn all() -> Vec<(&'static str, Tensor)> {
		let cpu = &Device::Cpu;
		let seq = |n: usize| Tensor::arange(0f32, n as f32, cpu).unwrap();
		let mut out = vec![
			("rank0", Tensor::new(2.5f32, cpu).unwrap()),
			("rank1-short", seq(5)),
			("rank1-long", seq(20)),
			("rank2-fits", seq(12).reshape((3, 4)).unwrap()),
			("rank2-wide", seq(30).reshape((3, 10)).unwrap()),
			("rank2-tall", seq(30).reshape((10, 3)).unwrap()),
			("rank2-both", seq(144).reshape((12, 12)).unwrap()),
			("rank3", seq(60).reshape((3, 4, 5)).unwrap()),
			("rank4", seq(120).reshape((2, 3, 4, 5)).unwrap()),
			("rank5", seq(240).reshape((2, 2, 3, 4, 5)).unwrap()),
			("rank6", seq(480).reshape((2, 2, 2, 3, 4, 5)).unwrap()),
			(
				"offset",
				seq(144)
					.reshape((12, 12))
					.unwrap()
					.narrow(0, 3, 6)
					.unwrap()
					.narrow(1, 2, 9)
					.unwrap(),
			),
			("transposed", seq(30).reshape((3, 10)).unwrap().t().unwrap()),
			(
				"zero-front",
				Tensor::zeros((0, 3, 4), DType::F32, cpu).unwrap(),
			),
			(
				"zero-middle",
				Tensor::zeros((2, 0, 4), DType::F32, cpu).unwrap(),
			),
			(
				"zero-end",
				Tensor::zeros((2, 3, 0), DType::F32, cpu).unwrap(),
			),
			(
				"zero-rank2",
				Tensor::zeros((0, 4), DType::F32, cpu).unwrap(),
			),
			("zero-rank1", Tensor::zeros(0, DType::F32, cpu).unwrap()),
			(
				"unsupported-bf16",
				seq(6)
					.reshape((2, 3))
					.unwrap()
					.to_dtype(DType::BF16)
					.unwrap(),
			),
			(
				"f64",
				Tensor::arange(0f64, 6.0, cpu)
					.unwrap()
					.reshape((2, 3))
					.unwrap(),
			),
			(
				"i64",
				Tensor::arange(-3i64, 3, cpu)
					.unwrap()
					.reshape((2, 3))
					.unwrap(),
			),
			(
				"u32",
				Tensor::arange(0u32, 6, cpu)
					.unwrap()
					.reshape((2, 3))
					.unwrap(),
			),
			("u8", Tensor::new(&[0u8, 1, 255], cpu).unwrap()),
		];
		let f32s = [
			f32::MAX,
			f32::MIN,
			f32::MIN_POSITIVE,
			f32::from_bits(1),
			-0.0,
			0.0,
			f32::NAN,
			f32::INFINITY,
			f32::NEG_INFINITY,
		];
		out.push((
			"f32-extremes",
			Tensor::new(&f32s, cpu).unwrap().reshape((3, 3)).unwrap(),
		));
		let f64s = [
			f64::MAX,
			f64::MIN,
			f64::MIN_POSITIVE,
			f64::from_bits(1),
			-0.0,
			0.0,
			f64::NAN,
			f64::INFINITY,
			f64::NEG_INFINITY,
		];
		out.push((
			"f64-extremes",
			Tensor::new(&f64s, cpu).unwrap().reshape((3, 3)).unwrap(),
		));
		// the longest f64 Debug text in every one of the 8 × 8 shown cells, with both ellipses
		let long = -1.2345678901234567e-300f64;
		out.push((
			"f64-worst",
			Tensor::from_vec(vec![long; 12 * 12], (12, 12), cpu).unwrap(),
		));
		out
	}

	/// Record 0161: the text display stays byte-identical. The goldens were captured from the
	/// renderer before 0161 changed anything (`print_text`), quirks included: a zero leading axis
	/// prints only its header and slice note; an unsupported dtype a values-not-shown line.
	#[test]
	fn text_is_unchanged() {
		let goldens: std::collections::BTreeMap<String, String> =
			serde_json::from_str(include_str!("../tests/data/display_text_0161.json")).unwrap();
		let cases = all();
		assert_eq!(goldens.len(), cases.len());
		for (name, t) in cases {
			assert_eq!(super::render(&t), goldens[name], "{name}");
		}
	}

	/// The data rows of a simple HTML table: each `<tr>`'s cells, `<th>` and `<td>` alike.
	fn html_rows(html: &str) -> Vec<Vec<String>> {
		html.split("<tr>")
			.skip(1)
			.map(|row| {
				let row = row.split("</tr>").next().unwrap();
				row.split(['<', '>'])
					.collect::<Vec<_>>()
					.chunks(4)
					.filter(|c| c.len() >= 3 && (c[1] == "td" || c[1] == "th"))
					.map(|c| c[2].to_owned())
					.collect()
			})
			.collect()
	}

	/// Record 0161: for every case the HTML form passes the allowlist, stays inside the writer's
	/// capacity without generic truncation, and shows exactly what the text shows: the same
	/// first line (header, slice note, status), the same note, or the same corner values, with
	/// indices 0..n and the `…` row and column where the text has them.
	#[test]
	fn html_shows_what_the_text_shows() {
		for (name, t) in all() {
			let text = super::render(&t);
			let html = super::render_html(&t);
			assert!(rnx::present::html_allowed(&html), "{name}: {html}");
			assert!(
				html.len() <= super::HTML_USABLE && !html.contains(super::HTML_OMITTED),
				"{name}"
			);
			let mut lines = text.split('\n');
			let first = lines.next().unwrap();
			let small = html
				.strip_prefix("<div><small>")
				.unwrap()
				.split("</small>")
				.next()
				.unwrap();
			assert_eq!(small, first, "{name}");
			let body: Vec<&str> = lines.collect();
			if body.is_empty() {
				assert!(
					!html.contains("<table>") && html.ends_with("</small></div>"),
					"{name}: {html}"
				);
				continue;
			}
			if body.len() == 1 && body[0].ends_with("values not shown)") {
				assert_eq!(
					html,
					format!("<div><small>{first}</small><div>{}</div></div>", body[0]),
					"{name}"
				);
				continue;
			}
			let more_rows = body.last() == Some(&"…");
			let text_rows: Vec<Vec<String>> = body
				.iter()
				.filter(|l| **l != "…")
				.map(|l| l.split(" | ").map(str::to_owned).collect())
				.collect();
			let more_columns = text_rows[0].last().map(String::as_str) == Some("…");
			let rows = html_rows(&html);
			let (head, data) = if t.rank() >= 1 {
				(Some(&rows[0]), &rows[1..])
			} else {
				(None, &rows[..])
			};
			let indexed = t.rank() >= 2;
			let width = text_rows[0].len() - usize::from(more_columns);
			if let Some(head) = head {
				let mut want: Vec<String> = if indexed { vec![String::new()] } else { vec![] };
				want.extend((0..width).map(|i| i.to_string()));
				if more_columns {
					want.push("…".into());
				}
				assert_eq!(head, &want, "{name}");
			}
			assert_eq!(
				data.len(),
				text_rows.len() + usize::from(more_rows),
				"{name}"
			);
			for (i, (h, tr)) in data.iter().zip(&text_rows).enumerate() {
				let values = if indexed {
					assert_eq!(h[0], i.to_string(), "{name}");
					&h[1..]
				} else {
					&h[..]
				};
				assert_eq!(values, &tr[..], "{name}");
			}
			if more_rows {
				let last = data.last().unwrap();
				assert!(
					last.iter().all(|c| c == "…")
						&& last.len() == width + 1 + usize::from(more_columns),
					"{name}"
				);
			}
		}
	}

	#[test]
	fn html_goldens() {
		let by: std::collections::BTreeMap<&str, Tensor> = all().into_iter().collect();
		let html = |n: &str| super::render_html(&by[n]);
		assert_eq!(
			html("rank0"),
			"<div><small>Tensor[f32; scalar]</small><table><tbody><tr><td>2.5</td></tr></tbody></table></div>"
		);
		assert_eq!(
			html("rank1-long"),
			"<div><small>Tensor[f32; 20]</small><table><thead><tr><th>0</th><th>1</th><th>2</th><th>3</th><th>4</th><th>5</th><th>6</th><th>7</th><th>…</th></tr></thead><tbody><tr><td>0.0</td><td>1.0</td><td>2.0</td><td>3.0</td><td>4.0</td><td>5.0</td><td>6.0</td><td>7.0</td><td>…</td></tr></tbody></table></div>"
		);
		assert_eq!(
			html("rank2-fits"),
			"<div><small>Tensor[f32; 3x4]</small><table><thead><tr><th></th><th>0</th><th>1</th><th>2</th><th>3</th></tr></thead><tbody><tr><th>0</th><td>0.0</td><td>1.0</td><td>2.0</td><td>3.0</td></tr><tr><th>1</th><td>4.0</td><td>5.0</td><td>6.0</td><td>7.0</td></tr><tr><th>2</th><td>8.0</td><td>9.0</td><td>10.0</td><td>11.0</td></tr></tbody></table></div>"
		);
		assert_eq!(
			html("zero-front"),
			"<div><small>Tensor[f32; 0x3x4] (the [0] slice)</small></div>"
		);
		assert_eq!(
			html("zero-middle"),
			"<div><small>Tensor[f32; 2x0x4] (the [0] slice) (empty)</small></div>"
		);
		assert_eq!(
			html("unsupported-bf16"),
			"<div><small>Tensor[bf16; 2x3]</small><div>(BF16 values not shown)</div></div>"
		);
		assert!(html("rank6").starts_with(
			"<div><small>Tensor[f32; 2x2x2x3x4x5] (the [0, 0, 0, 0] slice)</small><table>"
		));
		assert!(
			html("f32-extremes")
				.contains("<td>3.4028235e38</td><td>-3.4028235e38</td><td>1.1754944e-38</td>")
		);
		assert!(html("f32-extremes").contains("<td>1e-45</td><td>-0.0</td><td>0.0</td>"));
		assert!(html("f32-extremes").contains("<td>NaN</td><td>inf</td><td>-inf</td>"));
	}

	/// A calculated worst case (review of 0161): a rank-6 header with six 20-digit dimensions and
	/// the four-index slice note; a head of 8 indices and `…`; 8 rows of a row index, 8 cells of the
	/// longest f64 Debug text (24 bytes, e.g. `-1.2345678901234568e-300`; no f64 Debug text is
	/// longer) and `…`; the `…` row; and the wrapping tags. Nothing in the text needs escaping.
	const WORST: usize = {
		let header = "<div><small>".len()
			+ "Tensor[f64; ".len()
			+ 6 * 20 + 5
			+ "]".len()
			+ " (the [0, 0, 0, 0] slice)".len()
			+ "</small><table>".len();
		let head = "<thead><tr><th></th>".len()
			+ 8 * "<th>7</th>".len()
			+ "<th>…</th>".len()
			+ "</tr></thead>".len();
		let row = "<tr><th>7</th>".len()
			+ 8 * ("<td></td>".len() + 24)
			+ "<td>…</td>".len()
			+ "</tr>".len();
		let more = "<tr><th>…</th>".len() + 9 * "<td>…</td>".len() + "</tr>".len();
		header + head + "<tbody>".len() + 8 * row + more + "</tbody></table></div>".len()
	};

	/// The measured fixtures fit whole under the calculated worst case, far inside the capacity,
	/// with no marker. Cuts: every capacity from the smallest possible fragment up to the whole
	/// form, for several shapes, is honoured exactly, and every cut is whole, closed, marked,
	/// allowed and a prefix of the whole form (review of 0161, R1: the head-to-body transition
	/// overran by seven bytes).
	#[test]
	fn html_bounds_and_cuts() {
		let by: std::collections::BTreeMap<&str, Tensor> = all().into_iter().collect();
		let rank2 = super::render_html(&by["f64-worst"]);
		let rank6 = super::render_html(&by["f64-worst"].reshape((1, 1, 1, 1, 12, 12)).unwrap());
		eprintln!(
			"rank-2 fixture {} bytes, rank-6 fixture {} bytes, calculated worst {WORST}, capacity {}",
			rank2.len(),
			rank6.len(),
			super::HTML_USABLE
		);
		for html in [&rank2, &rank6] {
			assert!(html.len() <= WORST && !html.contains(super::HTML_OMITTED));
		}
		assert!(WORST + super::CLOSING < super::HTML_USABLE / 4);
		let shapes = [
			by["f64-worst"].clone(),
			by["f64-worst"].reshape((1, 1, 1, 1, 12, 12)).unwrap(),
			Tensor::zeros((12, 12), candle_core::DType::F64, &candle_core::Device::Cpu).unwrap(),
			by["rank1-long"].clone(),
			by["rank0"].clone(),
		];
		for t in &shapes {
			let whole = super::render_html(t);
			let opening = whole.find("<table>").unwrap() + "<table>".len();
			let smallest = opening + super::CLOSING;
			for usable in smallest..whole.len() + 8 {
				let html = super::render_html_within(t, usable);
				assert!(
					html.len() <= usable,
					"{usable}: {} bytes: {html}",
					html.len()
				);
				assert!(rnx::present::html_allowed(&html), "{usable}: {html}");
				if html != whole {
					assert!(
						html.ends_with(&format!("</table>{}</div>", super::HTML_OMITTED)),
						"{usable}: {html}"
					);
					let kept = html.split("</table>").next().unwrap();
					let kept = kept
						.trim_end_matches("<tbody></tbody>")
						.trim_end_matches("</tbody>");
					assert!(whole.starts_with(kept), "{usable}: {html}");
				}
			}
		}
	}

	#[test]
	#[ignore]
	fn print_text() {
		for (name, t) in all() {
			println!("CASE {name}\n{:?}", super::render(&t));
		}
	}
}
