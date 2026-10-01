//! Record 0132's measurements, on 0131's pinned model and data.
//!
//! - `probe0132 measure MODEL DATA`: throughput (1, 32, 60, 10,000 texts)
//!   with tokenization's share, single-query latency, the worker sweep,
//!   memory for 10,000 texts and for the worst case (512 texts at 256
//!   tokens, which drives the in-flight budget to its limit).
//! - `probe0132 sweep MODEL DATA`: the worker sweep alone (run under
//!   `taskset` for the smaller-machine check).
use rnx_candle::text::{self, CAPS, Knobs, Trace};
use serde_json::Value;
use std::time::Instant;

fn texts(data: &str) -> Value {
	serde_json::from_slice(&std::fs::read(format!("{data}/texts.json")).unwrap()).unwrap()
}
fn strs(v: &Value) -> Vec<&str> {
	v.as_array()
		.unwrap()
		.iter()
		.map(|s| s.as_str().unwrap())
		.collect()
}
fn rss_kib() -> (u64, u64) {
	let s = std::fs::read_to_string("/proc/self/status").unwrap();
	let get = |k: &str| {
		s.lines()
			.find(|l| l.starts_with(k))
			.and_then(|l| l.split_whitespace().nth(1))
			.and_then(|v| v.parse().ok())
			.unwrap_or(0)
	};
	(get("VmRSS:"), get("VmHWM:"))
}
fn mib(b: usize) -> f64 {
	b as f64 / (1 << 20) as f64
}
fn run(enc: &text::TextEncoder, xs: &[&str], knobs: &Knobs) -> (f64, Trace) {
	let t = Instant::now();
	let (r, trace) = text::embed_with(enc, xs, CAPS, knobs);
	r.unwrap();
	(t.elapsed().as_secs_f64(), trace)
}
fn median(enc: &text::TextEncoder, xs: &[&str], knobs: &Knobs, reps: usize) -> (f64, Trace) {
	run(enc, xs, knobs);
	let mut v: Vec<(f64, Trace)> = (0..reps).map(|_| run(enc, xs, knobs)).collect();
	v.sort_by(|a, b| a.0.total_cmp(&b.0));
	v.swap_remove(reps / 2)
}

fn sweep(enc: &text::TextEncoder, corpus: &[&str]) {
	let cpus = std::thread::available_parallelism().map_or(0, |n| n.get());
	println!("sweep (10,000 texts), {cpus} CPUs available:");
	for w in [1, 4, 8, 16, 20, 28] {
		let (s, t) = median(
			enc,
			corpus,
			&Knobs {
				workers: Some(w),
				..Knobs::default()
			},
			1,
		);
		println!(
			"  W {w:>2}: {s:.2} s (plan {:.2} s, exec {:.2} s), max running {}",
			t.plan_s, t.exec_s, t.max_running
		);
	}
	let (s, t) = median(enc, corpus, &Knobs::default(), 1);
	println!("  default (W {}): {s:.2} s", t.workers);
}

fn measure(model: &str, data: &str) {
	let x = texts(data);
	let enc = text::load_dir(model).unwrap();
	let corpus = strs(&x["corpus"]);
	let d = Knobs::default();
	println!(
		"threads: {}",
		std::thread::available_parallelism().map_or(0, |n| n.get())
	);
	for (name, xs, reps) in [
		("one query", strs(&x["queries"])[..1].to_vec(), 41),
		("batch of 32", strs(&x["docs"])[..32].to_vec(), 21),
		("60 docs", strs(&x["docs"]), 21),
		("10,000 corpus", corpus.clone(), 3),
	] {
		let (s, t) = median(&enc, &xs, &d, reps);
		println!(
			"embed {name}: median {s:.4} s, {:.0} texts/s; plan (tokenize) {:.4} s = {:.0}%; W {}, max running {}, max in flight {} values",
			xs.len() as f64 / s,
			t.plan_s,
			100.0 * t.plan_s / s,
			t.workers,
			t.max_running,
			t.max_inflight
		);
	}
	sweep(&enc, &corpus);
	// memory: 10,000 texts, then the worst case
	let long = strs(&x["controls"]["long"])[0];
	let worst: Vec<&str> = vec![long; 512];
	for (name, xs) in [
		("10,000 corpus", corpus.clone()),
		("worst case: 512 texts at 256 tokens", worst),
	] {
		rnx::allocation::reset_peak();
		let before = rnx::allocation::live().unwrap_or(0);
		let (s, t) = run(&enc, &xs, &d);
		let peak = rnx::allocation::peak() - before;
		let (rss, hwm) = rss_kib();
		let planned: usize = t.batches.iter().sum::<usize>();
		println!(
			"memory, {name}: {s:.2} s; output {:.1} MiB; allocator peak during +{:.1} MiB; max running {}, max in flight {} values ({:.0} MiB as f32; budget {} MiB); rss {} MiB, high-water {} MiB; {} batches, {} texts",
			mib(xs.len() * 384 * 4),
			mib(peak),
			t.max_running,
			t.max_inflight,
			mib(t.max_inflight * 4),
			text::AGG * 4 >> 20,
			rss / 1024,
			hwm / 1024,
			t.batches.len(),
			planned
		);
	}
}

fn main() {
	let a: Vec<String> = std::env::args().collect();
	match a[1].as_str() {
		"measure" => measure(&a[2], &a[3]),
		"sweep" => {
			let x = texts(&a[3]);
			let enc = text::load_dir(&a[2]).unwrap();
			sweep(&enc, &strs(&x["corpus"]));
		}
		// one batch alone (W = 1): its real allocator peak against its estimate
		"calibrate" => {
			let x = texts(&a[3]);
			let enc = text::load_dir(&a[2]).unwrap();
			let long = strs(&x["controls"]["long"])[0];
			let short = strs(&x["docs"]);
			for (name, xs) in [
				("32 x 256 tokens", vec![long; 32]),
				("8 x 256 tokens", vec![long; 8]),
				("32 short (~40 tokens)", short[..32].to_vec()),
			] {
				run(
					&enc,
					&xs,
					&Knobs {
						workers: Some(1),
						..Knobs::default()
					},
				);
				rnx::allocation::reset_peak();
				let before = rnx::allocation::live().unwrap_or(0);
				let (_, t) = run(
					&enc,
					&xs,
					&Knobs {
						workers: Some(1),
						..Knobs::default()
					},
				);
				let peak = rnx::allocation::peak() - before;
				let est = t.max_inflight * 4;
				println!(
					"{name}: estimate {:.1} MiB, allocator peak +{:.1} MiB, ratio {:.2}",
					mib(est),
					mib(peak),
					peak as f64 / est as f64
				);
			}
		}
		other => panic!("unknown command {other}"),
	}
}
