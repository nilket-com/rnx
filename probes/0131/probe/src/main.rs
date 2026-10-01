//! Record 0131's probe.
//!
//! - `probe0131 twin MODEL DATA OUT.json`: the Rust twin. The same tokenizer,
//!   model and pooling run directly through `candle-transformers` and
//!   `tokenizers`, in batches of 32, written without rnx.
//! - `probe0131 rnx MODEL DATA OUT.json`: the rnx path (`rnx_candle::text`),
//!   asserted bit for bit against the twin in-process, plus the controls
//!   (halved batches through lowered caps, two 5,000-row chunks against one
//!   call); writes every embedding for the reference comparison.
//! - `probe0131 check-script MODEL DATA`: the script's `scores.parquet`
//!   against the twin, bit for bit.
//! - `probe0131 measure MODEL DATA`: load time (warm and an attempted cold
//!   cache), phases, throughput, stage live allocation and high-water marks.
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use rnx::interchange::{Data, Dense};
use rnx_candle::text;
use serde_json::{Value, json};
use std::time::Instant;
use tokenizers::{
	PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer, TruncationDirection,
	TruncationParams, TruncationStrategy,
};

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

// ---- the twin: written directly against the libraries, not rnx ----

struct Twin {
	model: BertModel,
	tok: Tokenizer,
}

fn twin_load(model: &str) -> Twin {
	let config: Config =
		serde_json::from_slice(&std::fs::read(format!("{model}/config.json")).unwrap()).unwrap();
	let mut tok = Tokenizer::from_file(format!("{model}/tokenizer.json")).unwrap();
	let pad_token = tok.id_to_token(config.pad_token_id as u32).unwrap();
	tok.with_padding(Some(PaddingParams {
		strategy: PaddingStrategy::BatchLongest,
		direction: PaddingDirection::Right,
		pad_to_multiple_of: None,
		pad_id: config.pad_token_id as u32,
		pad_type_id: 0,
		pad_token,
	}));
	tok.with_truncation(Some(TruncationParams {
		direction: TruncationDirection::Right,
		max_length: 256,
		strategy: TruncationStrategy::LongestFirst,
		stride: 0,
	}))
	.unwrap();
	let bytes = std::fs::read(format!("{model}/model.safetensors")).unwrap();
	let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu).unwrap();
	let vb = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
	Twin {
		model: BertModel::load(vb, &config).unwrap(),
		tok,
	}
}

fn twin_embed(t: &Twin, xs: &[&str]) -> Vec<f32> {
	let mut out = Vec::new();
	for chunk in xs.chunks(32) {
		let enc = t.tok.encode_batch(chunk.to_vec(), true).unwrap();
		let (b, seq) = (enc.len(), enc[0].len());
		let ids: Vec<u32> = enc.iter().flat_map(|e| e.get_ids().to_vec()).collect();
		let mask: Vec<u32> = enc
			.iter()
			.flat_map(|e| e.get_attention_mask().to_vec())
			.collect();
		let input = Tensor::from_vec(ids, (b, seq), &Device::Cpu).unwrap();
		let mask = Tensor::from_vec(mask, (b, seq), &Device::Cpu).unwrap();
		let h = t
			.model
			.forward(&input, &input.zeros_like().unwrap(), Some(&mask))
			.unwrap();
		let m = mask.to_dtype(DType::F32).unwrap().unsqueeze(2).unwrap();
		let pooled = h
			.broadcast_mul(&m)
			.unwrap()
			.sum(1)
			.unwrap()
			.broadcast_div(&m.sum(1).unwrap())
			.unwrap();
		let norm = pooled
			.sqr()
			.unwrap()
			.sum_keepdim(1)
			.unwrap()
			.sqrt()
			.unwrap()
			.maximum(1e-12)
			.unwrap();
		out.extend(
			pooled
				.broadcast_div(&norm)
				.unwrap()
				.flatten_all()
				.unwrap()
				.to_vec1::<f32>()
				.unwrap(),
		);
	}
	out
}

/// The twin's cosine: the same stable normalization as rnx's, f32 matmul.
fn twin_scores(a: &[f32], b: &[f32], w: usize) -> Vec<f32> {
	let unit = |v: &[f32]| -> Vec<f32> {
		v.chunks(w)
			.flat_map(|r| {
				let r: Vec<f64> = r.iter().map(|&x| x as f64).collect();
				let s = r.iter().fold(0f64, |m, x| m.max(x.abs()));
				let n = r.iter().map(|x| (x / s) * (x / s)).sum::<f64>().sqrt();
				r.into_iter().map(move |x| ((x / s) / n) as f32)
			})
			.collect()
	};
	let (ua, ub) = (unit(a), unit(b));
	let (n, m) = (a.len() / w, b.len() / w);
	let ta = Tensor::from_vec(ua, (n, w), &Device::Cpu).unwrap();
	let tb = Tensor::from_vec(ub, (m, w), &Device::Cpu).unwrap();
	ta.matmul(&tb.t().unwrap())
		.unwrap()
		.flatten_all()
		.unwrap()
		.to_vec1::<f32>()
		.unwrap()
}

fn f32s(d: &Dense) -> Vec<f32> {
	match d.data() {
		Data::F32(v) => v.as_ref().clone(),
		Data::F64(_) => panic!("f64"),
	}
}
fn bits(v: &[f32]) -> Vec<u32> {
	v.iter().map(|x| x.to_bits()).collect()
}
fn rows(v: &[f32], w: usize) -> Value {
	json!(v.chunks(w).map(|r| r.to_vec()).collect::<Vec<_>>())
}

fn twin(model: &str, data: &str, out: &str) {
	let t = twin_load(model);
	let x = texts(data);
	let docs = twin_embed(&t, &strs(&x["docs"]));
	let queries = twin_embed(&t, &strs(&x["queries"]));
	let scores = twin_scores(&docs, &queries, 384);
	std::fs::write(
		out,
		json!({"docs": rows(&docs, 384), "queries": rows(&queries, 384), "scores": rows(&scores, 8)}).to_string(),
	)
	.unwrap();
	println!(
		"twin: {} docs, {} queries",
		docs.len() / 384,
		queries.len() / 384
	);
}

fn rnx_path(model: &str, data: &str, out: &str) {
	let x = texts(data);
	let t = twin_load(model);
	let enc = text::load_dir(model).unwrap();
	println!("{}", text::summary(&enc));
	let mut res = serde_json::Map::new();
	// the documents and queries: bit for bit against the twin
	for key in ["docs", "queries"] {
		let xs = strs(&x[key]);
		let ours = f32s(&text::embed_texts(&enc, &xs).unwrap());
		let theirs = twin_embed(&t, &xs);
		assert_eq!(bits(&ours), bits(&theirs), "{key}: rnx and the twin differ");
		res.insert(key.into(), rows(&ours, 384));
	}
	let d = text::embed_texts(&enc, &strs(&x["docs"])).unwrap();
	let q = text::embed_texts(&enc, &strs(&x["queries"])).unwrap();
	let s = text::similarity_named(&d, &q, (0..8).map(|i| format!("q{i}")).collect()).unwrap();
	assert_eq!(
		bits(&f32s(&s)),
		bits(&twin_scores(&f32s(&d), &f32s(&q), 384)),
		"scores: rnx and the twin differ"
	);
	res.insert("scores".into(), rows(&f32s(&s), 8));
	println!("rnx = twin, bit for bit: 60 docs, 8 queries, 480 scores");
	text::embed_texts(&enc, &strs(&x["controls"]["thirty_three"])).unwrap();
	assert_eq!(
		text::last_batches(),
		[32, 1],
		"production caps: 32 then the final short batch"
	);
	// the pipeline controls, each for the reference comparison
	let mut controls = serde_json::Map::new();
	for (k, v) in x["controls"].as_object().unwrap() {
		let xs = strs(v);
		let ours = f32s(&text::embed_texts(&enc, &xs).unwrap());
		assert_eq!(bits(&ours), bits(&twin_embed(&t, &xs)), "control {k}");
		controls.insert(k.clone(), rows(&ours, 384));
	}
	// halving: caps lowered so the mixed batch and the 33 texts halve
	// (the pinned model fits every production cap at 32)
	let caps = |attention: usize| text::Caps {
		hidden: 1 << 22,
		ffn: 1 << 24,
		attention,
	};
	// mixed: one 256-token text fits (12 x 256^2), the batch of 5 halves to 2;
	// 33 short texts: batches of 32 halve to 8
	for (k, cap) in [
		("mixed", 12 * 256 * 256 * 2),
		("thirty_three", 12 * 40 * 40 * 8),
	] {
		let xs = strs(&x["controls"][k]);
		let halved = f32s(&text::embed_with_caps(&enc, &xs, caps(cap)).unwrap());
		let sizes = text::last_batches();
		println!("{k}: batches {sizes:?} under the lowered cap");
		assert!(
			sizes.iter().all(|&b| b < 32.min(xs.len())) && sizes.iter().sum::<usize>() == xs.len()
		);
		controls.insert(format!("{k}_halved"), rows(&halved, 384));
	}
	let low = caps(12 * 64 * 64 * 4);
	// a single max-length text over the lowered caps is a named refusal
	let e = text::embed_with_caps(&enc, &strs(&x["controls"]["long"]), low).unwrap_err();
	assert!(e.contains("exceeds the activation caps"), "{e}");
	res.insert("controls".into(), Value::Object(controls));
	// chunking: 10,000 texts in one call against two 5,000-text calls
	let corpus = strs(&x["corpus"]);
	let t0 = Instant::now();
	let one = f32s(&text::embed_texts(&enc, &corpus).unwrap());
	let one_s = t0.elapsed().as_secs_f64();
	let t0 = Instant::now();
	let mut two = f32s(&text::embed_texts(&enc, &corpus[..5000]).unwrap());
	two.extend(f32s(&text::embed_texts(&enc, &corpus[5000..]).unwrap()));
	let two_s = t0.elapsed().as_secs_f64();
	let max_diff = one
		.iter()
		.zip(&two)
		.map(|(a, b)| (a - b).abs())
		.fold(0f32, f32::max);
	let identical = bits(&one) == bits(&two);
	println!(
		"chunking: one call {one_s:.2} s, two chunks {two_s:.2} s; max |diff| {max_diff:e}; bit identical: {identical}"
	);
	assert!(max_diff <= 1e-5);
	res.insert(
		"chunking".into(),
		json!({"one_s": one_s, "two_s": two_s, "max_abs_diff": max_diff, "bit_identical": identical}),
	);
	std::fs::write(out, Value::Object(res).to_string()).unwrap();
}

fn check_script(model: &str, data: &str) {
	use polars::prelude::*;
	let x = texts(data);
	let t = twin_load(model);
	let docs = twin_embed(&t, &strs(&x["docs"]));
	let queries = twin_embed(&t, &strs(&x["queries"]));
	let want = twin_scores(&docs, &queries, 384);
	let f = std::fs::File::open(format!("{data}/scores.parquet")).unwrap();
	let df = ParquetReader::new(f).finish().unwrap();
	let names: Vec<String> = df
		.get_column_names()
		.iter()
		.map(|s| s.to_string())
		.collect();
	let mut want_names = vec!["id".to_string(), "text".to_string()];
	want_names.extend((0..8).map(|i| format!("q{i}")));
	assert_eq!(names, want_names);
	for q in 0..8 {
		let col: Vec<f32> = df
			.column(&format!("q{q}"))
			.unwrap()
			.f32()
			.unwrap()
			.into_no_null_iter()
			.collect();
		let w: Vec<f32> = (0..60).map(|r| want[r * 8 + q]).collect();
		assert_eq!(bits(&col), bits(&w), "q{q}");
	}
	println!("script: 480 scores equal the twin's bit for bit");
}

// ---- measurements ----

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
fn live() -> usize {
	rnx::allocation::live().unwrap_or(0)
}
fn mib(b: usize) -> f64 {
	b as f64 / (1 << 20) as f64
}

fn drop_cache(model: &str) {
	// an attempt: posix_fadvise(DONTNEED) on each file; the kernel may keep pages
	for f in [
		"config.json",
		"tokenizer.json",
		"model.safetensors",
		"modules.json",
		"1_Pooling/config.json",
		"sentence_bert_config.json",
	] {
		let status = std::process::Command::new("python3")
			.args([
				"-c",
				"import os,sys; fd=os.open(sys.argv[1], os.O_RDONLY); os.fsync(fd) if False else None; os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED); os.close(fd)",
				&format!("{model}/{f}"),
			])
			.status()
			.unwrap();
		assert!(status.success());
	}
}

fn measure(model: &str, data: &str) {
	let x = texts(data);
	let (rss0, _) = rss_kib();
	let live0 = live();
	println!(
		"launch: rss {} MiB, live {:.1} MiB",
		rss0 / 1024,
		mib(live0)
	);
	// an attempted cold-cache load, first
	drop_cache(model);
	rnx::allocation::reset_peak();
	let t = Instant::now();
	let enc = text::load_dir(model).unwrap();
	let cold = t.elapsed().as_secs_f64();
	let peak_load = rnx::allocation::peak() - live0;
	let (rss1, hwm1) = rss_kib();
	let live1 = live();
	println!(
		"load (attempted cold cache): {:.3} s; live after {:.1} MiB (+{:.1}); allocator peak during load +{:.1} MiB; rss {} MiB, high-water {} MiB",
		cold,
		mib(live1),
		mib(live1 - live0),
		mib(peak_load),
		rss1 / 1024,
		hwm1 / 1024
	);
	// warm: the median of 10 loads, and the phases
	let mut warm: Vec<f64> = (0..10)
		.map(|_| {
			let t = Instant::now();
			drop(text::load_dir(model).unwrap());
			t.elapsed().as_secs_f64()
		})
		.collect();
	warm.sort_by(f64::total_cmp);
	let t = Instant::now();
	let bytes = std::fs::read(format!("{model}/model.safetensors")).unwrap();
	let read_s = t.elapsed().as_secs_f64();
	let t = Instant::now();
	let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu).unwrap();
	let parse_s = t.elapsed().as_secs_f64();
	let config: Config =
		serde_json::from_slice(&std::fs::read(format!("{model}/config.json")).unwrap()).unwrap();
	let t = Instant::now();
	let _m = BertModel::load(
		VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu),
		&config,
	)
	.unwrap();
	let build_s = t.elapsed().as_secs_f64();
	let t = Instant::now();
	let _tok = Tokenizer::from_file(format!("{model}/tokenizer.json")).unwrap();
	let tok_s = t.elapsed().as_secs_f64();
	drop((bytes, _m, _tok));
	println!(
		"load (warm): median {:.3} s (min {:.3}, max {:.3}); phases: read {:.3} s, parse {:.3} s, build {:.3} s, tokenizer {:.3} s",
		warm[5], warm[0], warm[9], read_s, parse_s, build_s, tok_s
	);
	// throughput
	let corpus = strs(&x["corpus"]);
	let sets: [(&str, Vec<&str>); 4] = [
		("one query", strs(&x["queries"])[..1].to_vec()),
		("batch of 32", strs(&x["docs"])[..32].to_vec()),
		("60 docs", strs(&x["docs"])),
		("10,000 corpus", corpus.clone()),
	];
	for (name, xs) in &sets {
		text::embed_texts(&enc, xs).unwrap();
		let reps = if xs.len() > 1000 { 3 } else { 20 };
		let mut ts: Vec<f64> = (0..reps)
			.map(|_| {
				let t = Instant::now();
				drop(text::embed_texts(&enc, xs).unwrap());
				t.elapsed().as_secs_f64()
			})
			.collect();
		ts.sort_by(f64::total_cmp);
		let med = ts[reps / 2];
		println!(
			"embed {name}: median {:.4} s, {:.0} texts/s",
			med,
			xs.len() as f64 / med
		);
	}
	// memory across the 10,000-text embedding
	rnx::allocation::reset_peak();
	let before = live();
	let d = text::embed_texts(&enc, &corpus).unwrap();
	let peak = rnx::allocation::peak() - before;
	let (rss2, hwm2) = rss_kib();
	println!(
		"embed 10,000: result {:.1} MiB kept (live +{:.1}); allocator peak during +{:.1} MiB; rss {} MiB, high-water {} MiB",
		mib(d.rows() * d.columns() * 4),
		mib(live() - before),
		mib(peak),
		rss2 / 1024,
		hwm2 / 1024
	);
	println!(
		"threads: {}",
		std::thread::available_parallelism().map_or(0, |n| n.get())
	);
}

fn main() {
	let a: Vec<String> = std::env::args().collect();
	match a[1].as_str() {
		"twin" => twin(&a[2], &a[3], &a[4]),
		"rnx" => rnx_path(&a[2], &a[3], &a[4]),
		"check-script" => check_script(&a[2], &a[3]),
		"measure" => measure(&a[2], &a[3]),
		// feasibility: N texts split across T caller threads, each embedding
		// its slice concurrently. Each call runs on its own inference worker
		// (worker::run spawns a scoped thread), so a caller-side rayon pool
		// would NOT reach it; the worker's observed thread counts and the
		// environment are printed, so a run states exactly what it measured.
		"bench-par" => {
			let x = texts(&a[3]);
			let enc = text::load_dir(&a[2]).unwrap();
			let n: usize = a[4].parse().unwrap();
			let t: usize = a[5].parse().unwrap();
			let corpus = strs(&x["corpus"]);
			let corpus = &corpus[..n];
			let start = Instant::now();
			let parts: Vec<(Vec<f32>, (usize, usize))> = std::thread::scope(|s| {
				let hs: Vec<_> = corpus
					.chunks(n.div_ceil(t))
					.map(|c| {
						let enc = &enc;
						s.spawn(move || {
							let d = f32s(&text::embed_texts(enc, c).unwrap());
							(d, text::last_threads())
						})
					})
					.collect();
				hs.into_iter().map(|h| h.join().unwrap()).collect()
			});
			let el = start.elapsed().as_secs_f64();
			let joined: Vec<f32> = parts.iter().flat_map(|p| p.0.clone()).collect();
			let one = f32s(&text::embed_texts(&enc, corpus).unwrap());
			let (one_threads, observed) = (text::last_threads(), parts[0].1);
			let diff = one
				.iter()
				.zip(&joined)
				.map(|(a, b)| (a - b).abs())
				.fold(0f32, f32::max);
			println!(
				"{n} texts on {t} concurrent calls in {el:.3} s; max |diff| against one call {diff:e}; RAYON_NUM_THREADS={:?}; worker sees candle threads {} / rayon pool {} (one call: {} / {})",
				std::env::var("RAYON_NUM_THREADS").ok(),
				observed.0,
				observed.1,
				one_threads.0,
				one_threads.1
			);
		}
		// profiling aid: embed the first N corpus texts once
		"bench" => {
			let x = texts(&a[3]);
			let enc = text::load_dir(&a[2]).unwrap();
			let n: usize = a[4].parse().unwrap();
			let t = Instant::now();
			text::embed_texts(&enc, &strs(&x["corpus"])[..n]).unwrap();
			println!("{n} texts in {:.3} s", t.elapsed().as_secs_f64());
		}
		other => panic!("unknown command {other}"),
	}
}
