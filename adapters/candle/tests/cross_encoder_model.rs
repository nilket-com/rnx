//! Record 0146, against the pinned cross-encoder (run explicitly, with
//! `RNX_CROSS_ENCODER` naming its directory, fetched by probes/0146/fetch.sh):
//! 32 pairs of exactly 512 tokens follow the declared S1 rule (single pairs,
//! each under the full budget, as the twin derives), and a single 512-token
//! pair's measured peak is within its in-flight estimate.
use rnx::allocation::{peak, reset_peak};
use rnx_candle::text::rerank;

#[test]
#[ignore = "needs the pinned cross-encoder in RNX_CROSS_ENCODER"]
fn long_pairs_follow_the_declared_partition_within_the_estimate() {
	let dir = std::env::var("RNX_CROSS_ENCODER").expect("RNX_CROSS_ENCODER");
	let ce = rerank::load_dir(&dir).unwrap();
	// [CLS] the cat [SEP] + 507 words + [SEP] = 512 tokens
	let q = "the cat";
	let p = "the ".repeat(507);
	let p = p.trim_end();
	let qs = vec![q; 32];
	let ps = vec![p; 32];
	let (scores, trace) =
		rerank::score_strs(&ce, &qs, &ps, rnx_candle::text::CAPS, &Default::default());
	let scores = scores.unwrap();
	assert_eq!(scores.len(), 32);
	assert_eq!(
		trace.batches,
		vec![1; 32],
		"32 x 512 splits to single pairs"
	);
	// 513 tokens: refused, naming the pair
	let over = format!("{p} the");
	let (e, _) = rerank::score_strs(
		&ce,
		&[q],
		&[over.as_str()],
		rnx_candle::text::CAPS,
		&Default::default(),
	);
	assert!(e.unwrap_err().contains("pair 0 is 513 tokens, at most 512"));
	// the estimate as an upper bound: one 512-token pair, measured warmed
	let est = rerank::estimate_for(&ce, 1, 512).unwrap();
	reset_peak();
	let base = peak();
	let one = rerank::score_strs(&ce, &[q], &[p], rnx_candle::text::CAPS, &Default::default())
		.0
		.unwrap();
	let used = peak() - base;
	eprintln!(
		"one 512-token pair: peak {used} bytes, estimate {} bytes ({est} values); score {}",
		est * 4,
		one[0]
	);
	assert!(
		used <= est * 4,
		"peak {used} above the estimate {}",
		est * 4
	);
}
