//! Record 0158: the table preview's whole footprint is bounded by its
//! structure, not by the frame, observed through the allocator's peak after
//! each frame already exists (review of 0158). One test, so no parallel test
//! moves the peak.
use polars::prelude::{self as p, NamedFrom};
use rnx::allocation::{peak, reset_peak};

/// The bound computed from the code (plans/0158 evidence): at most 14 rows of
/// 9 cell texts, each at most 80 escaped scalars of at most 8 bytes plus the
/// marker, at up to twice their length in capacity (about 160 KiB); one
/// border or row line at a time, its box-drawing characters three bytes each
/// (under 70 KiB with capacity); the vectors, the widths and the capped
/// output (under 20 KiB). Rounded up to 512 KiB.
const BOUND: usize = 512 << 10;

/// The preview's peak above the live bytes when it started.
fn observe(frame: &p::DataFrame) -> (usize, String) {
	reset_peak();
	let base = peak();
	let text = rnx_polars::preview_text(frame).unwrap();
	(peak() - base, text)
}
/// Record 0159: the same for the HTML form. Its intermediates are the same
/// cells, one row string at a time, and the form itself, at most 16,384
/// bytes (twice that in capacity): inside the same bound.
fn observe_html(frame: &p::DataFrame) -> (usize, String) {
	reset_peak();
	let base = peak();
	let html = rnx_polars::preview_html(frame).unwrap();
	(peak() - base, html)
}

#[test]
fn the_preview_footprint_is_bounded_by_structure() {
	// Hostile text at the scalar bound's worst escape (`\u{202e}`, 8 bytes)
	// in every cell and every name, a million rows by a thousand columns.
	// Scalar columns hold one value each, so the frame itself is small.
	let hostile = "\u{202e}".repeat(10_000);
	let wide = p::DataFrame::new(
		1_000_000,
		(0..1_000)
			.map(|i| {
				p::Column::new_scalar(
					format!("{hostile}{i}").into(),
					p::Scalar::new(
						p::DataType::String,
						p::AnyValue::StringOwned(hostile.clone().into()),
					),
					1_000_000,
				)
			})
			.collect(),
	)
	.unwrap();
	let (bytes, text) = observe(&wide);
	eprintln!(
		"hostile 1,000,000 x 1,000: peak {bytes} bytes above base, {} bytes shown",
		text.len()
	);
	assert!(bytes < BOUND, "{bytes} bytes");
	assert!(text.starts_with("shape: (1_000_000, 1_000)\n") && !text.contains('\u{202e}'));
	let (bytes, html) = observe_html(&wide);
	eprintln!(
		"hostile 1,000,000 x 1,000 as HTML: peak {bytes} bytes above base, {} bytes shown",
		html.len()
	);
	assert!(bytes < BOUND, "{bytes} bytes");
	assert!(html.len() <= rnx::present::HTML_BYTES && rnx::present::html_allowed(&html));
	// A tall numeric frame: the shown cells are short, the footprint small.
	let tall = p::DataFrame::new(
		1_000_000,
		vec![p::Series::new("n".into(), (0..1_000_000i64).collect::<Vec<_>>()).into()],
	)
	.unwrap();
	let (bytes, text) = observe(&tall);
	eprintln!(
		"numeric 1,000,000 x 1: peak {bytes} bytes above base, {} bytes shown",
		text.len()
	);
	assert!(bytes < 64 << 10, "{bytes} bytes");
	assert!(text.contains("│ 999999 │"));
}
