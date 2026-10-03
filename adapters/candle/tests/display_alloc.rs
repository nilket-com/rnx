//! Record 0161: a tensor's text and HTML displays read only the shown corner, whatever the
//! tensor's logical size, observed through the allocator's peak after the input exists. The large
//! inputs are broadcast and strided views of four values: a dense 1,000,000 × 1,000 f32 tensor
//! would be 4 GB, and materializing one would itself prove nothing. One test, so no parallel test
//! moves the peak.
use candle_core::{Device, Tensor};
use rnx::allocation::{peak, reset_peak};

/// The computed bound: the corner copy is at most 64 values; its cell texts at most 64 of the
/// longest Debug text; the text form at most 2,048 bytes and the HTML form at most 16,320; with
/// the view handles and vectors. A full materialization of either input is 4,000,000,000 bytes.
const BOUND: usize = 64 << 10;

fn observe(t: &Tensor) -> (usize, String, String) {
	reset_peak();
	let base = peak();
	let (text, html) = rnx_candle::display_forms(t);
	(peak() - base, text, html)
}

#[test]
fn displays_read_only_the_corner_of_a_huge_view() {
	let small = Tensor::new(&[[1.5f32, -2.0], [0.25, 3.0]], &Device::Cpu).unwrap();
	// a 1,000,000 × 1,000 view: one value of the small storage, broadcast
	let wide = small
		.narrow(0, 0, 1)
		.unwrap()
		.narrow(1, 0, 1)
		.unwrap()
		.broadcast_as((1_000_000, 1_000))
		.unwrap();
	assert!(!wide.is_contiguous() && wide.elem_count() == 1_000_000_000);
	// an offset, transposed view of it: a 1,000 × 999,997 logical tensor
	let offset = wide.narrow(0, 3, 999_997).unwrap().t().unwrap();
	assert_eq!(offset.dims(), &[1_000, 999_997]);
	for (label, t) in [
		("broadcast 1,000,000 x 1,000", &wide),
		("offset transposed 1,000 x 999,997", &offset),
	] {
		let (bytes, text, html) = observe(t);
		eprintln!(
			"{label} (a view of 4 values): peak {bytes} bytes above base; text {} bytes, HTML {} bytes",
			text.len(),
			html.len()
		);
		assert!(bytes < BOUND, "{label}: {bytes} bytes");
		assert!(
			text.lines().count() == 10 && text.ends_with("\n…"),
			"{text}"
		);
		assert!(
			html.matches("<tr>").count() == 1 + 8 + 1 && rnx::present::html_allowed(&html),
			"{html}"
		);
		assert_eq!(html.matches("<td>1.5</td>").count(), 64, "{html}");
	}
}
