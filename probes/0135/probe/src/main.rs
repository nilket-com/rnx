//! Record 0135's schedule measurements, on 0131's pinned model.
//!
//! - `probe0135 waste MODEL PASSAGES`: S0's padding waste on D1, the most
//!   that sorting by length (S2) could save in tokens.
//! - `probe0135 schedule MODEL PASSAGES LABEL...`: one timed call per label,
//!   in order: `s0`, `s1:C` (S1 at target concurrency C), `default` (the
//!   adapter's own `CONCURRENCY`), `agg:K` (S0 with
//!   K times the budget, for information only; never a candidate).
//! - `probe0135 calibrate MODEL PASSAGES B... [--scale F] [--concurrency C]`:
//!   a gate. One batch of exactly B texts at the model's full length, alone,
//!   its allocator peak against its estimate; exits 1 on any failure.
//! - `probe0135 configs [--scale F] [--bad]`: a gate. The admitted
//!   configurations' extremes (zero weights, the adapter's own batch path),
//!   each expected case's peak against its estimate, alone and four at once;
//!   exits 1 on any failure or missing case. `--scale` and `--bad` are the
//!   gates' failure controls (`probes/0135/calibration_controls.sh`).
//! - `probe0135 dump MODEL PASSAGES LABEL OUT`: the embedding's f32 bits under
//!   one schedule, and its batch sizes in `OUT.batches`.
use rnx_candle::text::{self, AGG, CAPS, Knobs};
use std::time::Instant;

fn passages(path: &str) -> Vec<String> {
	serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn rss_mib() -> (u64, u64) {
	let s = std::fs::read_to_string("/proc/self/status").unwrap();
	let get = |k: &str| {
		s.lines()
			.find(|l| l.starts_with(k))
			.and_then(|l| l.split_whitespace().nth(1))
			.and_then(|v| v.parse::<u64>().ok())
			.unwrap_or(0)
			/ 1024
	};
	(get("VmRSS:"), get("VmHWM:"))
}

fn knobs(label: &str) -> Knobs {
	let (kind, value) = label.split_once(':').unwrap_or((label, ""));
	match kind {
		"s0" => Knobs {
			concurrency: Some(1),
			..Knobs::default()
		},
		"default" => Knobs::default(),
		"s1" => Knobs {
			concurrency: Some(value.parse().unwrap()),
			..Knobs::default()
		},
		"agg" => Knobs {
			agg: Some(AGG * value.parse::<usize>().unwrap()),
			..Knobs::default()
		},
		other => panic!("unknown schedule {other}"),
	}
}

fn schedule(model: &str, path: &str, labels: &[String]) {
	let texts = passages(path);
	let xs: Vec<&str> = texts.iter().map(String::as_str).collect();
	let enc = text::load_dir(model).unwrap();
	println!("{} passages", xs.len());
	for label in labels {
		let k = knobs(label);
		rnx::allocation::reset_peak();
		let before = rnx::allocation::live().unwrap_or(0);
		let t = Instant::now();
		let (r, trace) = text::embed_with(&enc, &xs, CAPS, &k);
		let s = t.elapsed().as_secs_f64();
		r.unwrap();
		let peak = rnx::allocation::peak() - before;
		let (rss, hwm) = rss_mib();
		let mut sizes = trace.batches.clone();
		sizes.sort_unstable();
		sizes.dedup();
		println!(
			"{label}: {s:.1} s (plan {:.2} s, exec {:.1} s); {} batches of sizes {sizes:?}; W {}, max running {}, max in flight {:.0} MiB of {:.0}; allocator peak +{:.0} MiB; rss {rss} MiB, high-water {hwm} MiB",
			trace.plan_s,
			trace.exec_s,
			trace.batches.len(),
			trace.workers,
			trace.max_running,
			(trace.max_inflight * 4) as f64 / (1 << 20) as f64,
			(k.agg.unwrap_or(AGG) * 4) as f64 / (1 << 20) as f64,
			peak as f64 / (1 << 20) as f64,
		);
	}
}

fn waste(model: &str, path: &str) {
	let texts = passages(path);
	let mut tok = tokenizers::Tokenizer::from_file(format!("{model}/tokenizer.json")).unwrap();
	tok.with_truncation(Some(tokenizers::TruncationParams {
		max_length: 256,
		..Default::default()
	}))
	.unwrap();
	tok.with_padding(None);
	let lens: Vec<usize> = texts
		.iter()
		.map(|t| tok.encode(t.as_str(), true).unwrap().len())
		.collect();
	let real: usize = lens.iter().sum();
	let padded = |ls: &[usize]| -> usize {
		ls.chunks(32)
			.map(|c| c.len() * c.iter().max().unwrap())
			.sum()
	};
	let quad = |ls: &[usize]| -> usize {
		ls.chunks(32)
			.map(|c| c.len() * c.iter().max().unwrap().pow(2))
			.sum()
	};
	let mut sorted = lens.clone();
	sorted.sort_unstable();
	let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize];
	println!(
		"{} passages; tokens per passage after truncation: min {}, median {}, p90 {}, max {}; at 256: {}",
		lens.len(),
		sorted[0],
		at(0.5),
		at(0.9),
		sorted[sorted.len() - 1],
		lens.iter().filter(|&&l| l == 256).count()
	);
	println!(
		"input order (S0): {} padded tokens for {real} real, waste {:.1}%; attention (seq²) {}",
		padded(&lens),
		100.0 * (1.0 - real as f64 / padded(&lens) as f64),
		quad(&lens)
	);
	println!(
		"sorted (S2): {} padded tokens, waste {:.1}%; attention {} ({:.1}% below S0); token saving vs S0 {:.1}%",
		padded(&sorted),
		100.0 * (1.0 - real as f64 / padded(&sorted) as f64),
		quad(&sorted),
		100.0 * (1.0 - quad(&sorted) as f64 / quad(&lens) as f64),
		100.0 * (1.0 - padded(&sorted) as f64 / padded(&lens) as f64)
	);
}

/// The gates' controls: `--scale F` compares each peak against F times its
/// estimate (F < 1 must make the gate fail); `--concurrency C` replaces
/// calibration's pinned S0 (C > 1 must split a batch and fail the shape
/// check); `--bad` adds a configuration the loader refuses (an unexpected
/// construction error must fail the grid).
#[derive(Default)]
struct Controls {
	scale: Option<f64>,
	concurrency: Option<usize>,
	bad: bool,
}
fn controls(args: &[String]) -> (Controls, Vec<String>) {
	let mut c = Controls::default();
	let mut rest = Vec::new();
	let mut i = 0;
	while i < args.len() {
		match args[i].as_str() {
			"--scale" => {
				c.scale = Some(args[i + 1].parse().unwrap());
				i += 1;
			}
			"--concurrency" => {
				c.concurrency = Some(args[i + 1].parse().unwrap());
				i += 1;
			}
			"--bad" => c.bad = true,
			other => rest.push(other.to_owned()),
		}
		i += 1;
	}
	(c, rest)
}

/// Exact single-batch calibration, a gate: one batch of exactly `b` texts at
/// the model's full length, alone (W = 1, S0 pinned), its allocator peak
/// against its estimate. A shape the activation caps rule out is skipped by
/// name before running (`text::full_length_fits`); a run whose batches are
/// not exactly `[b]`, or whose peak is above its estimate, fails. The text is
/// D1's passages joined, so every text reaches the model's full length.
fn calibrate(model: &str, path: &str, args: &[String]) -> bool {
	let (ctl, sizes) = controls(args);
	let texts = passages(path);
	// about 4 KiB of real prose: well past 512 tokens, within MAX_TEXT
	let long: String = texts[..40].join(" ").chars().take(4096).collect();
	let enc = text::load_dir(model).unwrap();
	let (mut ran, mut skipped, mut failed) = (0, 0, 0);
	for b in &sizes {
		let b: usize = b.parse().unwrap();
		if !text::full_length_fits(&enc, b) {
			skipped += 1;
			println!(
				"{b} texts: skipped, one batch of {b} at full length is above the activation caps"
			);
			continue;
		}
		let xs = vec![long.as_str(); b];
		let one = Knobs {
			workers: Some(1),
			concurrency: Some(ctl.concurrency.unwrap_or(1)),
			..Knobs::default()
		};
		text::embed_with(&enc, &xs, CAPS, &one).0.unwrap();
		rnx::allocation::reset_peak();
		let before = rnx::allocation::live().unwrap_or(0);
		let (r, t) = text::embed_with(&enc, &xs, CAPS, &one);
		r.unwrap();
		let peak = rnx::allocation::peak() - before;
		let est = t.max_inflight * 4;
		// W = 1 runs one batch at a time, so the in-flight maximum is the one
		// batch's estimate, which rises strictly with the sequence length:
		// recover the actual length from it, and require the full length
		let full = text::max_seq(&enc);
		let seq = (1..=full).find(|&q| text::estimate_for(&enc, b, q) == Some(t.max_inflight));
		let shape_ok = t.batches == [b] && seq == Some(full);
		let ok = shape_ok && peak as f64 <= est as f64 * ctl.scale.unwrap_or(1.0);
		ran += 1;
		failed += usize::from(!ok);
		println!(
			"{b} texts: batches {:?} at {} tokens (full length {full}); estimate {:.1} MiB, allocator peak +{:.1} MiB, ratio {:.2}, {}",
			t.batches,
			seq.map_or("an unrecovered length".to_owned(), |q| q.to_string()),
			est as f64 / (1 << 20) as f64,
			peak as f64 / (1 << 20) as f64,
			peak as f64 / est as f64,
			if !shape_ok {
				"FAIL: not exactly the asked shape (b texts at the full length)"
			} else if !ok {
				"FAIL: peak above the estimate"
			} else {
				"ok"
			}
		);
	}
	println!(
		"# {} asked: {ran} ran, {skipped} skipped (above the caps), {failed} failed",
		sizes.len()
	);
	ran + skipped == sizes.len() && failed == 0
}

/// The admitted configurations' extremes, a gate: zero weights, through the
/// adapter's own `run_batch`, each case warmed once, then its allocator peak
/// against its estimate, alone and with copies running at once. Every row
/// prints its terms (hidden states, feed-forward states, attention scores,
/// in values). The grid's every expected case either runs or is skipped by
/// name because the caps rule its shape out; a case above its estimate, any
/// other construction error, or a missing case fails.
fn configs(args: &[String]) -> bool {
	let (ctl, _) = controls(args);
	// (name, hidden, ffn, heads)
	let mut models = vec![
		("minilm-like", 384, 1536, 12),
		("hidden-dominated", 1024, 1, 1),
		("hidden-dominated-512", 512, 1, 1),
		("ffn-dominated", 8, 4096, 1),
		("attention-dominated", 64, 1, 64),
		("bert-large-like", 1024, 4096, 16),
		("tiny", 1, 1, 1),
		("mid", 256, 1024, 4),
		("all-maximal", 1024, 4096, 64),
		("ffn-mid", 64, 4096, 1),
		("hidden-and-ffn", 1024, 1024, 1),
	];
	if ctl.bad {
		// hidden_size not divisible by num_attention_heads: refused
		models.push(("control-refused", 8, 8, 3));
	}
	let shapes: [(usize, usize); 6] = [(1, 512), (4, 512), (8, 128), (32, 128), (32, 32), (1, 8)];
	// per model and shape: 1 layer with all or half the tokens, alone or
	// four at once (4 cases), then 2 and 12 layers, alone and full (2)
	let mut cases = Vec::new();
	for &(name, h, f, heads) in &models {
		for &(b, seq) in &shapes {
			for real in [seq, seq.div_ceil(2)] {
				for copies in [1, 4] {
					cases.push((name, h, f, heads, 1, b, seq, real, copies));
				}
			}
			for layers in [2, 12] {
				cases.push((name, h, f, heads, layers, b, seq, seq, 1));
			}
		}
	}
	let expected = models.len() * shapes.len() * 6;
	let scale = ctl.scale.unwrap_or(1.0);
	let (mut ran, mut skipped, mut failed) = (0, 0, 0);
	let mut worst = 0f64;
	println!(
		"model\thidden\tffn\theads\tlayers\tb\tseq\treal\tcopies\tH\tF\tA\testimate_mib\tpeak_mib\tratio\tverdict"
	);
	for &(name, h, f, heads, layers, b, seq, real, copies) in &cases {
		let (est, run) = match text::synthetic_batch(h, f, heads, layers, b, seq, real) {
			Ok(x) => x,
			Err(e) if e.ends_with(text::ABOVE_CAPS) => {
				skipped += 1;
				println!(
					"{name}\t{h}\t{f}\t{heads}\t{layers}\t{b}\t{seq}\t{real}\t{copies}\t\t\t\t\t\t\tskipped: above the caps"
				);
				continue;
			}
			Err(e) => {
				failed += 1;
				println!(
					"{name}\t{h}\t{f}\t{heads}\t{layers}\t{b}\t{seq}\t{real}\t{copies}\t\t\t\t\t\t\tFAIL: {e}"
				);
				continue;
			}
		};
		run().unwrap();
		rnx::allocation::reset_peak();
		let before = rnx::allocation::live().unwrap_or(0);
		std::thread::scope(|s| {
			for _ in 0..copies {
				s.spawn(|| run().unwrap());
			}
		});
		let peak = rnx::allocation::peak() - before;
		let est_bytes = est * 4 * copies;
		let ratio = peak as f64 / est_bytes as f64;
		worst = worst.max(ratio);
		let ok = peak as f64 <= est_bytes as f64 * scale;
		ran += 1;
		failed += usize::from(!ok);
		let mib = |v: usize| v as f64 / (1 << 20) as f64;
		println!(
			"{name}\t{h}\t{f}\t{heads}\t{layers}\t{b}\t{seq}\t{real}\t{copies}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{ratio:.3}\t{}",
			b * seq * h,
			b * seq * f,
			b * heads * seq * seq,
			mib(est_bytes),
			mib(peak),
			if ok {
				"ok"
			} else {
				"FAIL: peak above the estimate"
			}
		);
	}
	println!(
		"# {expected} expected cases: {ran} ran, {skipped} skipped (above the caps), {failed} failed; worst ratio {worst:.3}"
	);
	cases.len() == expected && ran + skipped + failed == expected && failed == 0
}

/// One configuration and shape, warmed, then measured.
#[allow(clippy::too_many_arguments)]
fn case(
	h: usize,
	f: usize,
	heads: usize,
	layers: usize,
	b: usize,
	seq: usize,
	real: usize,
	copies: usize,
) {
	let (est, run) = text::synthetic_batch(h, f, heads, layers, b, seq, real).unwrap();
	run().unwrap();
	rnx::allocation::reset_peak();
	let before = rnx::allocation::live().unwrap_or(0);
	std::thread::scope(|s| {
		for _ in 0..copies {
			s.spawn(|| run().unwrap());
		}
	});
	let peak = rnx::allocation::peak() - before;
	let unit = (b * seq * h * 4) as f64;
	println!(
		"h {h} f {f} heads {heads} layers {layers} b {b} seq {seq} real {real} copies {copies}: peak {:.1} MiB = {:.2} hidden-state tensors; estimate {:.1} MiB, ratio {:.3}",
		peak as f64 / (1 << 20) as f64,
		peak as f64 / unit / copies as f64,
		(est * 4 * copies) as f64 / (1 << 20) as f64,
		peak as f64 / (est * 4 * copies) as f64
	);
}

/// The embedding's f32 bits under one schedule, as little-endian bytes.
fn dump(model: &str, path: &str, label: &str, out: &str) {
	let texts = passages(path);
	let xs: Vec<&str> = texts.iter().map(String::as_str).collect();
	let enc = text::load_dir(model).unwrap();
	let (r, trace) = text::embed_with(&enc, &xs, CAPS, &knobs(label));
	let d = r.unwrap();
	let rnx::interchange::Data::F32(values) = d.data() else {
		panic!("embeddings are f32")
	};
	let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
	std::fs::write(out, &bytes).unwrap();
	println!(
		"{label}: {} batches, plan {:.2} s, exec {:.1} s",
		trace.batches.len(),
		trace.plan_s,
		trace.exec_s
	);
	let sizes: Vec<String> = trace.batches.iter().map(usize::to_string).collect();
	std::fs::write(format!("{out}.batches"), sizes.join(",")).unwrap();
}

fn main() {
	let a: Vec<String> = std::env::args().collect();
	match a[1].as_str() {
		"waste" => waste(&a[2], &a[3]),
		"schedule" => schedule(&a[2], &a[3], &a[4..]),
		"dump" => dump(&a[2], &a[3], &a[4], &a[5]),
		"calibrate" => {
			if !calibrate(&a[2], &a[3], &a[4..]) {
				std::process::exit(1);
			}
		}
		"configs" => {
			if !configs(&a[2..]) {
				std::process::exit(1);
			}
		}
		"case" => {
			let n: Vec<usize> = a[2..10].iter().map(|v| v.parse().unwrap()).collect();
			case(n[0], n[1], n[2], n[3], n[4], n[5], n[6], n[7]);
		}
		other => panic!("unknown command {other}"),
	}
}
