//! Record 0149 (review round 1, b): after a failure and a panic injected
//! AFTER decode steps have run (the clones' KV caches exist), with budget
//! waiters, the tracked live bytes return to what they were before the call
//! (within 64 KiB, the allocator's bookkeeping), so no clone or cache
//! outlives its request. One test, so no parallel test moves the figure.
#![cfg(feature = "test-support")]
use rnx::allocation::live;
use rnx_candle::text::{Knobs, generate};

#[test]
fn nothing_outlives_a_failed_or_panicked_request() {
	let g = generate::load_dir(&generate::fixture_dir()).unwrap();
	let (s, u) = (["be brief"; 3], ["a cat sat", "a dog ran", "is it"]);
	let force = Knobs {
		force_length: true,
		..Default::default()
	};
	let one = (0..3)
		.map(|i| {
			generate::estimate_for(&g, generate::prompt_ids(&g, s[i], u[i]).unwrap().len(), 12)
				.unwrap()
		})
		.max()
		.unwrap();
	// warm
	generate::chat_strs(&g, &s, &u, 12, &force).0.unwrap();
	for (name, knobs) in [
		(
			"failure",
			Knobs {
				fail_at_step: Some((1, 3)),
				..force.clone()
			},
		),
		(
			"panic",
			Knobs {
				panic_at_step: Some((1, 3)),
				..force.clone()
			},
		),
	] {
		let knobs = Knobs {
			agg: Some(one),
			workers: Some(3),
			..knobs
		};
		let before = live().unwrap();
		let (r, t) = generate::chat_strs(&g, &s, &u, 12, &knobs);
		assert!(r.is_err());
		drop(r);
		drop(t);
		let after = live().unwrap();
		eprintln!("{name}: live {before} -> {after} bytes");
		assert!(
			after <= before + (64 << 10),
			"{name}: {} bytes outlived the call",
			after - before
		);
	}
}
