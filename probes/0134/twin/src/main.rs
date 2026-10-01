//! Record 0134's direct-Rust twins: the examples' computations written
//! directly against candle-core, candle-transformers and tokenizers.
//!
//! - `twin0134 e2 D1`: the statistics, z-scores, correlation matrix (f64
//!   bits), the strongest pairs and the most unusual documents.
//! - `twin0134 e1 D1 MODEL RUBRIC`: per query, the first five distinct
//!   documents among the top 60 passages, with their f32 score bits.
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::{
	PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer, TruncationDirection,
	TruncationParams, TruncationStrategy,
};

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


// ---- the frozen chunking rule, as the Rune script states it ----

fn words(s: &str) -> Vec<String> {
	s.replace('\n', " ")
		.replace('\t', " ")
		.replace('\r', " ")
		.split(' ')
		.filter(|w| !w.is_empty())
		.map(str::to_owned)
		.collect()
}

fn chunks(body: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut cur: Vec<String> = Vec::new();
	for para in body.split("\n\n") {
		let ws = words(para);
		if ws.is_empty() {
			continue;
		}
		if ws.len() > 180 {
			if !cur.is_empty() {
				out.push(cur.join(" "));
				cur.clear();
			}
			for w in ws.chunks(180) {
				out.push(w.join(" "));
			}
			continue;
		}
		if cur.len() + ws.len() > 180 {
			out.push(cur.join(" "));
			cur.clear();
		}
		cur.extend(ws);
	}
	if !cur.is_empty() {
		out.push(cur.join(" "));
	}
	out
}

struct Doc {
	path: String,
}

fn load(dir: &str) -> (Vec<Doc>, Vec<(usize, String)>) {
	let mut names: Vec<String> = std::fs::read_dir(dir)
		.unwrap()
		.map(|e| e.unwrap().file_name().into_string().unwrap())
		.filter(|n| n.ends_with(".md"))
		.collect();
	names.sort();
	let mut docs = Vec::new();
	let mut passages = Vec::new();
	for n in names {
		let text = std::fs::read_to_string(format!("{dir}/{n}")).unwrap();
		let first = text.split('\n').next().unwrap_or("");
		let title = first.strip_prefix("# ").unwrap_or(first).to_owned();
		let d = docs.len();
		for c in chunks(&text[first.len()..]) {
			passages.push((d, c));
		}
		let _ = title;
		docs.push(Doc { path: n.clone() });
	}
	(docs, passages)
}

fn passages(body: &str) -> usize {
	chunks(body).len()
}

fn e2(dir: &str) {
	let (docs, _) = load(dir);
	let mut values = Vec::new();
	for d in &docs {
		let text = std::fs::read_to_string(format!("{dir}/{}", d.path)).unwrap();
		let first = text.split('\n').next().unwrap_or("");
		let (mut fences, mut rows, mut heads) = (0f64, 0f64, 0f64);
		for line in text.split('\n') {
			if line.starts_with("```") {
				fences += 1.0;
			}
			if line.starts_with('|') {
				rows += 1.0;
			}
			if line.starts_with('#') {
				heads += 1.0;
			}
		}
		values.extend([
			words(&text).len() as f64,
			passages(&text[first.len()..]) as f64,
			fences,
			rows,
			heads,
			first.chars().count() as f64,
		]);
	}
	let (n, k) = (docs.len(), 6);
	let cpu = &Device::Cpu;
	let x = Tensor::from_vec(values, (n, k), cpu).unwrap();
	let mu = x.mean_keepdim(0).unwrap();
	let sd = x.var_keepdim(0).unwrap().sqrt().unwrap();
	let z = x.broadcast_sub(&mu).unwrap().broadcast_div(&sd).unwrap();
	let corr = (z.t().unwrap().matmul(&z).unwrap() * (1.0 / (n - 1) as f64)).unwrap();
	for row in corr.to_vec2::<f64>().unwrap() {
		println!(
			"corr\t{}",
			row.iter()
				.map(|v| format!("{:016x}", v.to_bits()))
				.collect::<Vec<_>>()
				.join("\t")
		);
	}
	let i = Tensor::arange(0u32, k as u32, cpu)
		.unwrap()
		.unsqueeze(1)
		.unwrap();
	let j = Tensor::arange(0u32, k as u32, cpu)
		.unwrap()
		.unsqueeze(0)
		.unwrap();
	let upper = j.broadcast_gt(&i).unwrap();
	let a = corr.abs().unwrap();
	let masked = upper.where_cond(&a, &a.zeros_like().unwrap()).unwrap();
	let (_, at) = masked
		.flatten_all()
		.unwrap()
		.force_contiguous()
		.unwrap()
		.sort_last_dim(false)
		.unwrap();
	for p in at.narrow(0, 0, 5).unwrap().to_vec1::<u32>().unwrap() {
		println!("pair\t{}\t{}", p as usize / k, p as usize % k);
	}
	let (score, who) = z
		.abs()
		.unwrap()
		.max(1)
		.unwrap()
		.force_contiguous()
		.unwrap()
		.sort_last_dim(false)
		.unwrap();
	let s = score.narrow(0, 0, 5).unwrap().to_vec1::<f64>().unwrap();
	for (w, d) in who
		.narrow(0, 0, 5)
		.unwrap()
		.to_vec1::<u32>()
		.unwrap()
		.into_iter()
		.enumerate()
	{
		println!(
			"unusual\t{:016x}\t{}",
			s[w].to_bits(),
			docs[d as usize].path
		);
	}
}

fn e1(dir: &str, model: &str, rubric: &str) {
	let (docs, passages) = load(dir);
	let t = twin_load(model);
	let texts: Vec<&str> = passages.iter().map(|p| p.1.as_str()).collect();
	let qs: Vec<String> = std::fs::read_to_string(rubric)
		.unwrap()
		.lines()
		.skip(1)
		.map(|l| l.split('\t').nth(2).unwrap().to_owned())
		.collect();
	let qrefs: Vec<&str> = qs.iter().map(|s| s.as_str()).collect();
	let cpu = &Device::Cpu;
	let e = Tensor::from_vec(twin_embed(&t, &texts), (texts.len(), 384), cpu).unwrap();
	let q = Tensor::from_vec(twin_embed(&t, &qrefs), (qs.len(), 384), cpu).unwrap();
	let scores = e.matmul(&q.t().unwrap()).unwrap();
	let per = scores.t().unwrap().force_contiguous().unwrap();
	let (v, i) = per.sort_last_dim(false).unwrap();
	let v = v.narrow(1, 0, 60).unwrap().to_vec2::<f32>().unwrap();
	let i = i.narrow(1, 0, 60).unwrap().to_vec2::<u32>().unwrap();
	for k in 0..qs.len() {
		let mut seen = Vec::new();
		for r in 0..60 {
			let d = passages[i[k][r] as usize].0;
			if !seen.contains(&d) {
				seen.push(d);
				println!("{k}\t{}\t{:08x}", docs[d].path, v[k][r].to_bits());
				if seen.len() == 5 {
					break;
				}
			}
		}
	}
}

fn main() {
	let a: Vec<String> = std::env::args().collect();
	match a[1].as_str() {
		"e2" => e2(&a[2]),
		"e1" => e1(&a[2], &a[3], &a[4]),
		other => panic!("unknown command {other}"),
	}
}
