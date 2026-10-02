//! Record 0149, against the pinned instruct model (run explicitly, with
//! `RNX_GEN_MODEL` naming its directory, fetched by probes/0149/fetch.sh).
//!
//! `the_early_gate_chats_are_generated`: the frozen non-D2 gate chats
//! (probes/0149/gate_chats.json): rnx's prompt ids, greedy ids and the
//! complete logits at every step, written to `RNX_GEN_GATE_OUT` (a JSON
//! index and a raw little-endian f32 file beside it) for
//! probes/0149/hf_gate.py; and `chat_strs` on the same chats gives the same
//! ids as the full-logit path.
use rnx_candle::text::generate;

fn gate() -> (Vec<(String, String)>, Vec<usize>) {
	let root = concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../../probes/0149/gate_chats.json"
	);
	let v: serde_json::Value = serde_json::from_slice(&std::fs::read(root).unwrap()).unwrap();
	let chats = v["chats"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| {
			(
				c["system"].as_str().unwrap().to_string(),
				c["user"].as_str().unwrap().to_string(),
			)
		})
		.collect();
	let m = v["max_new_tokens"]
		.as_array()
		.unwrap()
		.iter()
		.map(|x| x.as_u64().unwrap() as usize)
		.collect();
	(chats, m)
}

#[test]
#[ignore = "needs the pinned model in RNX_GEN_MODEL"]
fn the_early_gate_chats_are_generated() {
	let dir = std::env::var("RNX_GEN_MODEL").expect("RNX_GEN_MODEL");
	let out = std::env::var("RNX_GEN_GATE_OUT").expect("RNX_GEN_GATE_OUT");
	let g = generate::load_dir(&dir).unwrap();
	let (chats, max_new) = gate();
	let mut index = Vec::new();
	let mut raw: Vec<u8> = Vec::new();
	for ((s, u), &m) in chats.iter().zip(&max_new) {
		let prompt = generate::prompt_ids(&g, s, u).unwrap();
		let (ids, logits) = generate::full_logits(&g, s, u, m).unwrap();
		let at = raw.len() / 4;
		for v in &logits {
			assert!(v.iter().all(|x| x.is_finite()), "a non-finite logit");
			for x in v {
				raw.extend_from_slice(&x.to_le_bytes());
			}
		}
		// the production path gives the same ids
		let (r, _) = generate::chat_strs(&g, &[s.as_str()], &[u.as_str()], m, &Default::default());
		let r = r.unwrap();
		assert_eq!(r[0].ids, ids, "chat_strs and the full-logit path agree");
		eprintln!(
			"prompt {} tokens, {} generated ({}): {:?}",
			prompt.len(),
			ids.len(),
			r[0].stop,
			r[0].text
		);
		index.push(serde_json::json!({
			"system": s, "user": u, "max_new_tokens": m, "prompt_ids": prompt, "ids": ids,
			"text": r[0].text, "stop": r[0].stop, "logits_at": at, "steps": logits.len(),
			"vocab": logits[0].len(),
		}));
	}
	std::fs::write(format!("{out}.f32"), raw).unwrap();
	std::fs::write(out, serde_json::Value::Array(index).to_string()).unwrap();
}

/// The calibration grid (review round 1, R2), fail-closed, run alone:
/// prompts of exactly 16, 256 and 1,024 tokens (or a named skip) × 1, 64 and
/// 256 new tokens, forced (EOS ignored, so the worst case is measured);
/// each alone and as four concurrent chats, reporting the overlap the budget
/// admitted; peak ≤ the admitted estimate; a lowered limit refused before
/// any model or cache allocation; the minimum budget completes.
#[cfg(feature = "test-support")]
#[test]
#[ignore = "needs the pinned model in RNX_GEN_MODEL; run alone"]
fn the_memory_estimate_holds_across_the_grid() {
	use rnx::allocation::{peak, reset_peak};
	use rnx_candle::text::Knobs;
	let dir = std::env::var("RNX_GEN_MODEL").expect("RNX_GEN_MODEL");
	let g = generate::load_dir(&dir).unwrap();
	eprintln!(
		"resident F32 weights: {} bytes (outside the per-call budget)",
		generate::resident_bytes(&g)
	);
	let sys = "s";
	// a user text whose prompt is exactly p tokens, if one exists
	let user_for = |p: usize| -> Option<String> {
		let base = generate::prompt_ids(&g, sys, "").unwrap().len();
		let mut n = p.checked_sub(base)?;
		loop {
			let u = " the".repeat(n);
			let len = generate::prompt_ids(&g, sys, &u).unwrap().len();
			if len == p {
				return Some(u);
			}
			if len < p || n == 0 {
				return None;
			}
			n -= 1;
		}
	};
	let forced = |w: usize| Knobs {
		force_length: true,
		workers: Some(w),
		..Default::default()
	};
	let mut skips = Vec::new();
	for p in [16usize, 256, 1024] {
		let Some(u) = user_for(p) else {
			skips.push(format!(
				"p={p}: no user text gives exactly {p} prompt tokens"
			));
			continue;
		};
		for m in [1usize, 64, 256] {
			let est = generate::estimate_for(&g, p, m).unwrap();
			// warm, then alone
			generate::chat_strs(&g, &[sys], &[u.as_str()], m, &forced(1))
				.0
				.unwrap();
			reset_peak();
			let base = peak();
			let (r, _) = generate::chat_strs(&g, &[sys], &[u.as_str()], m, &forced(1));
			let used = peak() - base;
			let r = r.unwrap();
			assert_eq!(
				(r[0].prompt_tokens, r[0].ids.len()),
				(p, m),
				"the asked shape"
			);
			eprintln!(
				"p={p} m={m} alone: peak {used} B, estimate {} B, headroom {:.2}x",
				est * 4,
				(est * 4) as f64 / used.max(1) as f64
			);
			assert!(
				used <= est * 4,
				"p={p} m={m}: peak {used} above the estimate {}",
				est * 4
			);
			// four at once
			let (s4, u4) = ([sys; 4], [u.as_str(); 4]);
			reset_peak();
			let base = peak();
			let (r, t) = generate::chat_strs(&g, &s4, &u4, m, &forced(4));
			let used = peak() - base;
			let r = r.unwrap();
			assert!(r.iter().all(|x| (x.prompt_tokens, x.ids.len()) == (p, m)));
			eprintln!(
				"p={p} m={m} x4: max running {} at once (admitted), max in flight {} B, peak {used} B, headroom {:.2}x",
				t.max_running,
				t.max_inflight * 4,
				(t.max_inflight * 4) as f64 / used.max(1) as f64
			);
			assert!(
				used <= t.max_inflight * 4,
				"p={p} m={m} x4: peak {used} above the admitted {}",
				t.max_inflight * 4
			);
			assert_eq!(t.inflight_end, 0);
		}
	}
	for s in &skips {
		eprintln!("named skip: {s}");
	}
	// a lowered limit: refused before any clone or cache
	let u = user_for(1024).unwrap();
	let est = generate::estimate_for(&g, 1024, 256).unwrap();
	reset_peak();
	let base = peak();
	let (e, _) = generate::chat_strs(
		&g,
		&[sys],
		&[u.as_str()],
		256,
		&Knobs {
			agg: Some(est - 1),
			..Default::default()
		},
	);
	let used = peak() - base;
	let e = e.unwrap_err();
	eprintln!(
		"lowered limit: refused ({e}); allocated {used} B against an estimate of {} B",
		est * 4
	);
	assert!(e.contains("exceeds the in-flight budget"), "{e}");
	assert!(used < est * 4 / 100, "the refusal allocated {used} bytes");
	// the minimum admissible budget: four such chats, one at a time
	let (r, t) = generate::chat_strs(
		&g,
		&[sys; 4],
		&[u.as_str(); 4],
		8,
		&Knobs {
			agg: Some(generate::estimate_for(&g, 1024, 8).unwrap()),
			workers: Some(4),
			force_length: true,
			..Default::default()
		},
	);
	assert_eq!(r.unwrap().len(), 4);
	assert_eq!(t.max_running, 1);
	assert_eq!(t.inflight_end, 0);
	eprintln!("minimum budget: 4 chats of 1,024 + 8 tokens completed, one at a time");
}

/// Review round 1, R1: the pinned tokenizer with one ordinary id moved into
/// the padded range (150000 → 151900, the count, specials and every other
/// check preserved) is refused by name, not a panic. No weights are needed:
/// the tokenizer is checked before them.
#[test]
#[ignore = "needs the pinned model in RNX_GEN_MODEL"]
fn a_sparse_tokenizer_is_refused_not_a_panic() {
	let dir = std::env::var("RNX_GEN_MODEL").expect("RNX_GEN_MODEL");
	let d = std::env::temp_dir().join(format!("rnx-0149-sparse-{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	std::fs::copy(format!("{dir}/config.json"), d.join("config.json")).unwrap();
	let mut t: serde_json::Value =
		serde_json::from_slice(&std::fs::read(format!("{dir}/tokenizer.json")).unwrap()).unwrap();
	let vocab = t["model"]["vocab"].as_object_mut().unwrap();
	let key = vocab
		.iter()
		.find(|(_, v)| v.as_u64() == Some(150_000))
		.unwrap()
		.0
		.clone();
	vocab.insert(key, serde_json::json!(151_900));
	std::fs::write(d.join("tokenizer.json"), t.to_string()).unwrap();
	let r = std::panic::catch_unwind(|| generate::load_dir(d.to_str().unwrap()).err());
	let e = r
		.expect("load panicked")
		.expect("accepted a sparse tokenizer");
	eprintln!("refused: {e}");
	assert!(
		e.contains("is not below the tokenizer's size 151665"),
		"{e}"
	);
	std::fs::remove_dir_all(&d).ok();
}

/// Review round 1 (a): the load's own peak, measured, and what stays
/// resident after it (outside the per-call budget): the BF16 file buffer,
/// the BF16 tensors, the F32 conversion and the RoPE tables.
#[cfg(feature = "test-support")]
#[test]
#[ignore = "needs the pinned model in RNX_GEN_MODEL; run alone"]
fn the_load_peak_is_measured() {
	use rnx::allocation::{live, peak, reset_peak};
	let dir = std::env::var("RNX_GEN_MODEL").expect("RNX_GEN_MODEL");
	reset_peak();
	let base = live().unwrap();
	let g = generate::load_dir(&dir).unwrap();
	let top = peak() - base;
	let resident = live().unwrap() - base;
	eprintln!(
		"load: peak {top} bytes, resident after load {resident} bytes (weights F32 {} bytes)",
		generate::resident_bytes(&g)
	);
	assert!(resident >= generate::resident_bytes(&g));
}
