//! Record 0148, against the pinned NLI model (run explicitly, with
//! `RNX_NLI_MODEL` naming its directory, fetched by probes/0148/fetch.sh).
//!
//! `the_early_gate_pairs_are_scored`: the frozen non-D2 gate pairs
//! (probes/0148/gate_pairs.json), batched and each alone, written as
//! logits to `RNX_NLI_GATE_OUT` for probes/0148/hf_gate.py.
use rnx_candle::text::{CAPS, nli};

fn gate_pairs() -> (Vec<String>, Vec<String>) {
	let root = concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../../probes/0148/gate_pairs.json"
	);
	let v: serde_json::Value = serde_json::from_slice(&std::fs::read(root).unwrap()).unwrap();
	let long = v["long"].as_str().unwrap().repeat(14);
	let p = v["premises"]
		.as_array()
		.unwrap()
		.iter()
		.map(|x| match x.as_str().unwrap() {
			"LONG" => long.trim_end().to_string(),
			s => s.to_string(),
		})
		.collect();
	let h = v["hypotheses"]
		.as_array()
		.unwrap()
		.iter()
		.map(|x| x.as_str().unwrap().to_string())
		.collect();
	(p, h)
}

#[test]
#[ignore = "needs the pinned NLI model in RNX_NLI_MODEL"]
fn the_early_gate_pairs_are_scored() {
	let dir = std::env::var("RNX_NLI_MODEL").expect("RNX_NLI_MODEL");
	let out = std::env::var("RNX_NLI_GATE_OUT").expect("RNX_NLI_GATE_OUT");
	let m = nli::load_dir(&dir).unwrap();
	let (p, h) = gate_pairs();
	let pr: Vec<&str> = p.iter().map(String::as_str).collect();
	let hr: Vec<&str> = h.iter().map(String::as_str).collect();
	// a batch of all of them (the knob forces one batch, so padding is real)
	let knobs = rnx_candle::text::Knobs {
		concurrency: Some(1),
		..Default::default()
	};
	let (batched, trace) = nli::score_strs(&m, &pr, &hr, CAPS, &knobs);
	let batched = batched.unwrap();
	eprintln!("gate batches: {:?}", trace.batches);
	let mut alone = Vec::new();
	for k in 0..p.len() {
		alone.extend(
			nli::score_strs(&m, &pr[k..k + 1], &hr[k..k + 1], CAPS, &Default::default())
				.0
				.unwrap(),
		);
	}
	let json = serde_json::json!({
		"premises": p, "hypotheses": h, "batches": trace.batches,
		"batched": batched, "alone": alone,
	});
	std::fs::write(out, json.to_string()).unwrap();
}

/// The calibration grid (review round 1, R2), fail-closed. Needs the
/// `test-support` build (for the planner's shapes) and runs alone, so no
/// other test moves the allocator's peak:
///
/// - lengths 16, 128 and 512 tokens; batch sizes 1 and the largest the S1
///   planner admits at that length (a named skip when that is also 1);
/// - each cell runs its exact shape, checked against the planner, warmed,
///   alone and then as `CONCURRENCY` batches in one call, reporting the
///   overlap the budget actually admitted (`max_running`);
/// - peak ≤ the admitted estimate in every cell, with the headroom printed;
/// - a lowered limit refuses before allocating anything proportional;
/// - a single 512-token pair under the minimum admissible budget completes,
///   four times over, with every permit released.
#[cfg(feature = "test-support")]
#[test]
#[ignore = "needs the pinned NLI model in RNX_NLI_MODEL; run alone"]
fn the_memory_estimate_holds_across_the_grid() {
	use rnx::allocation::{peak, reset_peak};
	use rnx_candle::text::{AGG, CONCURRENCY, Knobs};
	let dir = std::env::var("RNX_NLI_MODEL").expect("RNX_NLI_MODEL");
	let m = nli::load_dir(&dir).unwrap();
	// [CLS] the×n [SEP] a cat [SEP] = n + 5 tokens
	let premise = |s: usize| "the ".repeat(s - 5).trim_end().to_string();
	let share = AGG / CONCURRENCY;
	let largest = |s: usize| {
		let mut b = 32;
		while b > 1
			&& !(nli::fits_for(&m, b, s) && nli::estimate_for(&m, b, s).is_some_and(|e| e <= share))
		{
			b /= 2;
		}
		b
	};
	let mut skips = Vec::new();
	for s in [16usize, 128, 512] {
		let p = premise(s);
		let mut cells = vec![1];
		if largest(s) > 1 {
			cells.push(largest(s));
		} else {
			skips.push(format!(
				"s={s} b=max: the planner admits only single pairs at this length"
			));
		}
		for b in cells {
			let ps = vec![p.as_str(); b];
			let hs = vec!["a cat"; b];
			let shapes = nli::planned_shapes(&m, &ps, &hs, &Knobs::default()).unwrap();
			assert_eq!(shapes, vec![(b, s)], "the asked shape s={s} b={b}");
			let est = nli::estimate_for(&m, b, s).unwrap();
			// warm (gemm's per-thread buffers), then measure alone
			nli::score_strs(&m, &ps, &hs, rnx_candle::text::CAPS, &Knobs::default())
				.0
				.unwrap();
			reset_peak();
			let base = peak();
			let (out, _) = nli::score_strs(&m, &ps, &hs, rnx_candle::text::CAPS, &Knobs::default());
			let used = peak() - base;
			assert_eq!(out.unwrap().len(), b * 3);
			eprintln!(
				"s={s} b={b} alone: peak {used} B, estimate {} B, headroom {:.2}x",
				est * 4,
				(est * 4) as f64 / used.max(1) as f64
			);
			assert!(
				used <= est * 4,
				"s={s} b={b}: peak {used} above the estimate {}",
				est * 4
			);
			// CONCURRENCY such batches in one call: a shape the planner forms
			// only at the largest admitted batch (it groups smaller requests)
			if b != largest(s) {
				skips.push(format!(
					"s={s} b={b} x{CONCURRENCY}: the planner groups these pairs into batches of {}, so concurrent b={b} batches are not a shape it forms",
					largest(s)
				));
				continue;
			}
			let n = b * CONCURRENCY;
			let ps = vec![p.as_str(); n];
			let hs = vec!["a cat"; n];
			let knobs = Knobs {
				workers: Some(CONCURRENCY),
				..Default::default()
			};
			let shapes = nli::planned_shapes(&m, &ps, &hs, &knobs).unwrap();
			// every batch at exactly s tokens; the S1 tail (halving from what
			// remains) forms some batches other than b, reported as formed
			assert!(
				shapes.iter().all(|&(_, q)| q == s),
				"s={s} concurrent shapes {shapes:?}"
			);
			let mut sizes = std::collections::BTreeMap::new();
			for &(x, _) in &shapes {
				*sizes.entry(x).or_insert(0) += 1;
			}
			eprintln!(
				"s={s} b={b} x{CONCURRENCY}: the planner formed batch sizes {sizes:?} (size: count)"
			);
			reset_peak();
			let base = peak();
			let (out, trace) = nli::score_strs(&m, &ps, &hs, rnx_candle::text::CAPS, &knobs);
			let used = peak() - base;
			assert_eq!(out.unwrap().len(), n * 3);
			eprintln!(
				"s={s} b={b} x{CONCURRENCY} batches: max running {} at once (admitted by the budget), max in flight {} B, peak {used} B, headroom {:.2}x",
				trace.max_running,
				trace.max_inflight * 4,
				(trace.max_inflight * 4) as f64 / used.max(1) as f64
			);
			assert!(
				used <= trace.max_inflight * 4,
				"s={s} b={b} concurrent: peak {used} above the admitted {}",
				trace.max_inflight * 4
			);
		}
	}
	for s in &skips {
		eprintln!("named skip: {s}");
	}
	// a lowered limit: refused by the planner before anything proportional
	let p = premise(512);
	let est = nli::estimate_for(&m, 1, 512).unwrap();
	let knobs = Knobs {
		agg: Some(est - 1),
		..Default::default()
	};
	reset_peak();
	let base = peak();
	let (e, _) = nli::score_strs(
		&m,
		&[p.as_str()],
		&["a cat"],
		rnx_candle::text::CAPS,
		&knobs,
	);
	let used = peak() - base;
	let e = e.unwrap_err();
	eprintln!(
		"lowered limit: refused ({e}); allocated {used} B against an estimate of {} B",
		est * 4
	);
	assert!(
		e.contains("exceeds the activation caps or the in-flight budget"),
		"{e}"
	);
	assert!(used < est * 4 / 100, "the refusal allocated {used} bytes");
	// the minimum admissible budget: four single 512-token pairs complete
	let knobs = Knobs {
		agg: Some(est),
		workers: Some(4),
		..Default::default()
	};
	let ps = vec![p.as_str(); 4];
	let (out, trace) = nli::score_strs(&m, &ps, &["a cat"; 4], rnx_candle::text::CAPS, &knobs);
	assert_eq!(out.unwrap().len(), 12);
	assert_eq!(
		trace.max_running, 1,
		"one at a time under the minimum budget"
	);
	eprintln!("minimum budget: 4 single 512-token pairs completed, one at a time");
}
