//! Record 0133's direct-Rust twin: the same loading, chunking, embedding,
//! similarity and ranking as the workflow scripts, written directly against
//! candle-transformers and tokenizers (0131's twin code), without rnx.
//!
//! - `twin0133 u1 D1 MODEL RUBRIC`: U1's top 5 per query, as
//!   `query\trecord\ttitle\tscore-bits` lines.
//! - `twin0133 u2 D1 D2 MODEL`: U2's pairs in each band, as
//!   `band\ti\tj\tscore` lines (D2 tickets, then D1 documents).
//! - `twin0133 u3 D2 MODEL`: U3's edges and connected components.
//! - `twin0133 e4 D2 MODEL`, `twin0133 e5`: record 0137's examples.
//! - `twin0133 u4 D2 MODEL LABELS`: record 0139's triage table.
//! - `twin0133 e6 D2 MODEL LABELS`, `twin0133 e6-synth SPEC`: record 0143's
//!   topic discovery trace (written independently of the Rune script).
//! - `twin0133 e7 D3`, `twin0133 e7-synth SPEC`: record 0145's traffic
//!   trace, by the same Candle calls, written independently.
//! - `twin0133 passages D1`, `twin0133 embed PASSAGES MODEL BATCHES OUT`:
//!   record 0135's passages, and their embedding on a given partition.
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::{
	PaddingDirection, PaddingParams, PaddingStrategy, PostProcessor, Tokenizer,
	TruncationDirection, TruncationParams, TruncationStrategy,
};

struct Twin {
	model: BertModel,
	tok: Tokenizer,
	config_json: Vec<u8>,
}

fn twin_load(model: &str) -> Twin {
	let config_json = std::fs::read(format!("{model}/config.json")).unwrap();
	let config: Config = serde_json::from_slice(&config_json).unwrap();
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
		config_json,
	}
}

fn twin_embed(t: &Twin, xs: &[&str]) -> Vec<f32> {
	let sizes = match std::env::var("TWIN0135_CONCURRENCY") {
		Ok(c) => s1_sizes(t, xs, c.parse().unwrap()),
		Err(_) => xs.chunks(32).map(<[&str]>::len).collect(),
	};
	twin_embed_sized(t, xs, &sizes)
}

/// Record 0135's S1 partition, derived here independently of rnx: from 32
/// texts at a time, halve until the batch's in-flight estimate (7 × hidden
/// states, 2 × feed-forward states, 6 × attention, plus 2^20) is at most
/// 2^28 / C values, and its activations within the per-batch caps.
fn s1_sizes(t: &Twin, xs: &[&str], c: usize) -> Vec<usize> {
	let config: serde_json::Value = serde_json::from_slice(&t.config_json).unwrap();
	let get = |k: &str| config[k].as_u64().unwrap() as usize;
	let (hidden, ffn, heads) = (
		get("hidden_size"),
		get("intermediate_size"),
		get("num_attention_heads"),
	);
	let lens: Vec<usize> = xs
		.iter()
		.map(|x| t.tok.encode(*x, true).unwrap().get_ids().len())
		.collect();
	let share = (1usize << 28) / c;
	let mut sizes = Vec::new();
	let mut at = 0;
	while at < xs.len() {
		let mut b = 32.min(xs.len() - at);
		loop {
			let seq = *lens[at..at + b].iter().max().unwrap();
			let estimate =
				7 * b * seq * hidden + 2 * b * seq * ffn + 6 * b * heads * seq * seq + (1 << 20);
			let fits = b * seq * hidden <= 1 << 22
				&& b * seq * ffn <= 1 << 24
				&& b * heads * seq * seq <= 1 << 25;
			if fits && (b == 1 || estimate <= share) {
				break;
			}
			b /= 2;
		}
		sizes.push(b);
		at += b;
	}
	sizes
}

/// Record 0135: the same embedding on a given partition, each batch
/// tokenized by the tokenizer itself, with its own padding.
fn twin_embed_sized(t: &Twin, xs: &[&str], sizes: &[usize]) -> Vec<f32> {
	assert_eq!(
		sizes.iter().sum::<usize>(),
		xs.len(),
		"the partition covers every text"
	);
	let mut out = Vec::new();
	let mut at = 0;
	for &b in sizes {
		let chunk = &xs[at..at + b];
		at += b;
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

/// Record 0136's token-aware chunking, written here independently of rnx
/// against the plan's contract: UTF-8 byte offsets grouped into atoms (a
/// token whose range is empty or overlaps the current atom joins it), words
/// by word id, passages of whole words within W content tokens (an unfit
/// word falls back to whole atoms), repair by re-tokenizing the substring
/// with specials and dropping the last word (atom), and the next start from
/// the repaired end. Returns byte ranges into `text`.
fn token_chunks(
	tok: &Tokenizer,
	text: &str,
	max_seq: usize,
	overlap: usize,
) -> Result<Vec<(usize, usize)>, String> {
	let mut t = tok.clone();
	t.with_padding(None);
	t.with_truncation(None).unwrap();
	let w = max_seq - t.get_post_processor().map_or(0, |p| p.added_tokens(false));
	let enc = t.encode(text, false).unwrap();
	// atoms as (start, end, tokens, word key)
	let mut atoms: Vec<(usize, usize, usize, Option<u32>)> = Vec::new();
	let mut lead = 0;
	for (k, &(s, e)) in enc.get_offsets().iter().enumerate() {
		assert!(s <= e && e <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(e));
		let wid = enc.get_word_ids()[k];
		match atoms.last_mut() {
			Some(a) if s == e || s < a.1 => {
				a.0 = a.0.min(s);
				a.1 = a.1.max(e);
				a.2 += 1;
			}
			None if s == e => lead += 1,
			_ => atoms.push((s, e, 1, wid)),
		}
	}
	// tokens with no span cannot be placed in a substring: refused
	match atoms.first_mut() {
		// leading empty-span tokens: the first atom starts at the text's start
		Some(a) => {
			a.2 += lead;
			if lead > 0 {
				a.0 = 0;
			}
		}
		None if lead > 0 => return Err(format!("{lead} tokens with no span")),
		None => {}
	}
	if atoms.iter().map(|a| a.2).sum::<usize>() != enc.len() {
		return Err("the atoms do not hold every token".into());
	}
	let n = atoms.len();
	// a new word starts where the word id changes or is missing
	let starts: Vec<bool> = (0..n)
		.map(|i| i == 0 || atoms[i].3.is_none() || atoms[i].3 != atoms[i - 1].3)
		.collect();
	let word_after = |i: usize| (i + 1..n).find(|&j| starts[j]).unwrap_or(n);
	let count = |a: usize, b: usize| atoms[a..b].iter().map(|x| x.2).sum::<usize>();
	let mut out = Vec::new();
	let mut c = 0;
	while c < n {
		let mut e = c;
		while e < n && count(c, word_after(e)) <= w {
			e = word_after(e);
		}
		let atomwise = e == c;
		if atomwise {
			while e < n && count(c, e + 1) <= w {
				e += 1;
			}
			assert!(e > c, "an unbreakable atom above the window");
		}
		let mut drops = 0;
		while t
			.encode(&text[atoms[c].0..atoms[e - 1].1], true)
			.unwrap()
			.len() > max_seq
		{
			drops += 1;
			assert!(drops <= 16, "repair beyond its bound");
			e = if atomwise {
				e - 1
			} else {
				(c + 1..e).rev().find(|&j| starts[j]).unwrap_or(c)
			};
			assert!(e > c, "nothing fits");
		}
		out.push((atoms[c].0, atoms[e - 1].1));
		if e == n {
			break;
		}
		let mut b = e;
		let mut back = 0;
		while b - 1 > c && back + atoms[b - 1].2 <= overlap {
			b -= 1;
			back += atoms[b].2;
		}
		while b < e && !starts[b] {
			b += 1;
		}
		c = b;
	}
	Ok(out)
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
	load_with(dir, &|body| chunks(body))
}

/// Record 0136: D1 chunked by the twin's token chunker instead, when
/// `TWIN0136_OVERLAP` is set (U1 prime).
fn load_tokens(dir: &str, model: &str) -> (Vec<Doc>, Vec<(usize, String)>) {
	let Ok(overlap) = std::env::var("TWIN0136_OVERLAP") else {
		return load(dir);
	};
	let overlap: usize = overlap.parse().unwrap();
	let tok = twin_load(model).tok;
	load_with(dir, &|body| {
		token_chunks(&tok, body, 256, overlap)
			.unwrap()
			.into_iter()
			.map(|(s, e)| body[s..e].to_owned())
			.collect()
	})
}

fn load_with(dir: &str, split: &dyn Fn(&str) -> Vec<String>) -> (Vec<Doc>, Vec<(usize, String)>) {
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
		for c in split(&text[first.len()..]) {
			passages.push((d, c));
		}
		let _ = title;
		docs.push(Doc { path: n.clone() });
	}
	(docs, passages)
}

fn u1(dir: &str, model: &str, rubric: &str) {
	let (docs, passages) = load_tokens(dir, model);
	let t = twin_load(model);
	let texts: Vec<&str> = passages.iter().map(|p| p.1.as_str()).collect();
	let e = twin_embed(&t, &texts);
	let mut ids = Vec::new();
	let mut qs = Vec::new();
	for line in std::fs::read_to_string(rubric).unwrap().lines().skip(1) {
		let f: Vec<&str> = line.split('\t').collect();
		ids.push(f[0].to_owned());
		qs.push(f[2].to_owned());
	}
	let qrefs: Vec<&str> = qs.iter().map(|s| s.as_str()).collect();
	let q = twin_embed(&t, &qrefs);
	let s = twin_scores(&e, &q, 384);
	let m = ids.len();
	println!("query\trank\tpath\tpid\tscore");
	for (k, id) in ids.iter().enumerate() {
		// the script's ranking: a stable descending sort of passages by score,
		// the first passage per document, the top 5 documents
		let mut order: Vec<usize> = (0..passages.len()).collect();
		order.sort_by(|&a, &b| s[b * m + k].total_cmp(&s[a * m + k]));
		let mut seen = std::collections::HashSet::new();
		let mut n = 0;
		for p in order {
			let d = passages[p].0;
			if seen.insert(d) {
				let sc = s[p * m + k];
				n += 1;
				println!("{id}\t{n}\t{}\t{p}\t{sc}", docs[d].path);
				if n == 5 {
					break;
				}
			}
		}
	}
}

fn ticket_texts(d2: &str) -> (Vec<i64>, Vec<String>, Vec<String>) {
	let v: serde_json::Value = serde_json::from_slice(&std::fs::read(d2).unwrap()).unwrap();
	let mut numbers = Vec::new();
	let mut titles = Vec::new();
	let mut texts = Vec::new();
	for i in v.as_array().unwrap() {
		let ws = words(i["body"].as_str().unwrap());
		let n = ws.len().min(180);
		numbers.push(i["number"].as_i64().unwrap());
		titles.push(i["title"].as_str().unwrap().to_owned());
		texts.push(format!(
			"{}\n{}",
			i["title"].as_str().unwrap(),
			ws[..n].join(" ")
		));
	}
	(numbers, titles, texts)
}

/// Pairs i < j with lo <= score < hi in an n x n score matrix (row-major).
fn band(s: &[f32], n: usize, lo: f32, hi: f32) -> Vec<(usize, usize, f32)> {
	let mut out = Vec::new();
	for i in 0..n {
		for j in i + 1..n {
			let v = s[i * n + j];
			if v >= lo && v < hi {
				out.push((i, j, v));
			}
		}
	}
	out
}

fn u2(d1: &str, d2: &str, model: &str) {
	let t = twin_load(model);
	let (numbers, _, texts) = ticket_texts(d2);
	let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
	let e2 = twin_embed(&t, &refs);
	println!("band\ti\tj\tscore");
	let n2 = texts.len();
	let s2 = twin_scores(&e2, &e2, 384);
	for (name, lo, hi) in [("d2-dup", 0.95f32, 2.0f32), ("d2-rel", 0.85, 0.95)] {
		for (i, j, v) in band(&s2, n2, lo, hi) {
			println!("{name}\t#{}\t#{}\t{v}", numbers[i], numbers[j]);
		}
	}
	let (docs, passages) = load(d1);
	let ptext: Vec<&str> = passages.iter().map(|p| p.1.as_str()).collect();
	let e = twin_embed(&t, &ptext);
	// each document's mean passage embedding, accumulated in f64
	let n1 = docs.len();
	let mut sums = vec![0f64; n1 * 384];
	let mut counts = vec![0usize; n1];
	for (k, (d, _)) in passages.iter().enumerate() {
		counts[*d] += 1;
		for c in 0..384 {
			sums[d * 384 + c] += e[k * 384 + c] as f64;
		}
	}
	let means: Vec<f32> = sums
		.iter()
		.enumerate()
		.map(|(i, s)| (s / counts[i / 384] as f64) as f32)
		.collect();
	let s1 = twin_scores(&means, &means, 384);
	for (name, lo, hi) in [("d1-dup", 0.95f32, 2.0f32), ("d1-rel", 0.85, 0.95)] {
		for (i, j, v) in band(&s1, n1, lo, hi) {
			println!("{name}\t{}\t{}\t{v}", docs[i].path, docs[j].path);
		}
	}
}

fn u3(d2: &str, model: &str) {
	let t = twin_load(model);
	let (numbers, _, texts) = ticket_texts(d2);
	let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
	let e = twin_embed(&t, &refs);
	let n = texts.len();
	let s = twin_scores(&e, &e, 384);
	let edges = band(&s, n, 0.80, 2.0);
	let mut parent: Vec<usize> = (0..n).collect();
	fn find(p: &mut [usize], mut i: usize) -> usize {
		while p[i] != i {
			p[i] = p[p[i]];
			i = p[i];
		}
		i
	}
	for &(i, j, _) in &edges {
		let (a, b) = (find(&mut parent, i), find(&mut parent, j));
		if a != b {
			parent[a] = b;
		}
	}
	let mut groups = std::collections::BTreeMap::<usize, usize>::new();
	for i in 0..n {
		let r = find(&mut parent, i);
		*groups.entry(r).or_default() += 1;
	}
	let mut least = std::collections::BTreeMap::<usize, i64>::new();
	for i in 0..n {
		let r = find(&mut parent, i);
		let e = least.entry(r).or_insert(numbers[i]);
		*e = (*e).min(numbers[i]);
	}
	println!("issue\tcomponent");
	for i in 0..n {
		let r = find(&mut parent, i);
		println!("#{}\t#{}", numbers[i], least[&r]);
	}
	for &(i, j, v) in &edges {
		println!("edge\t#{}\t#{}\t{v}", numbers[i], numbers[j]);
	}
	let _ = groups;
}

fn main() {
	let a: Vec<String> = std::env::args().collect();
	match a[1].as_str() {
		"u1" => u1(&a[2], &a[3], &a[4]),
		"u2" => u2(&a[2], &a[3], &a[4]),
		"u3" => u3(&a[2], &a[3]),
		// record 0135: PASSAGES (a JSON list) embedded on the partition in
		// BATCHES (comma-separated sizes), as little-endian f32 bits
		"embed" => {
			let texts: Vec<String> =
				serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
			let xs: Vec<&str> = texts.iter().map(String::as_str).collect();
			let sizes: Vec<usize> = std::fs::read_to_string(&a[4])
				.unwrap()
				.trim()
				.split(',')
				.map(|v| v.parse().unwrap())
				.collect();
			let v = twin_embed_sized(&twin_load(&a[3]), &xs, &sizes);
			let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
			std::fs::write(&a[5], bytes).unwrap();
		}
		// record 0137, E4: whole tickets chunked (0136, overlap 0), embedded on
		// 0135's S1 partition (C = 32), pooled per ticket by the same Candle
		// calls as candle::segment_mean, renormalized, scored; the pairs at or
		// above 0.80, as the script writes them
		"e4" => {
			let t = twin_load(&a[3]);
			let v: serde_json::Value =
				serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
			let mut numbers = Vec::new();
			let mut passages = Vec::new();
			let mut owner: Vec<u32> = Vec::new();
			for (k, i) in v.as_array().unwrap().iter().enumerate() {
				numbers.push(i["number"].as_i64().unwrap());
				let text = format!(
					"{}\n{}",
					i["title"].as_str().unwrap(),
					i["body"].as_str().unwrap()
				);
				for (s, e) in token_chunks(&t.tok, &text, 256, 0).unwrap() {
					passages.push(text[s..e].to_owned());
					owner.push(k as u32);
				}
			}
			let n = numbers.len();
			let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
			let sizes = s1_sizes(&t, &refs, 32);
			let flat = twin_embed_sized(&t, &refs, &sizes);
			let cpu = &Device::Cpu;
			let rows = refs.len();
			let emb = Tensor::from_vec(flat, (rows, 384), cpu).unwrap();
			let seg = Tensor::from_vec(owner, rows, cpu).unwrap();
			let sums = Tensor::zeros((n, 384), DType::F32, cpu)
				.unwrap()
				.index_add(&seg, &emb, 0)
				.unwrap();
			let ones = Tensor::ones(rows, DType::F32, cpu).unwrap();
			let counts = Tensor::zeros(n, DType::F32, cpu)
				.unwrap()
				.index_add(&seg, &ones, 0)
				.unwrap();
			let pooled = sums.broadcast_div(&counts.unsqueeze(1).unwrap()).unwrap();
			let norms = pooled
				.sqr()
				.unwrap()
				.sum_keepdim(1)
				.unwrap()
				.sqrt()
				.unwrap();
			let unit = pooled.broadcast_div(&norms).unwrap();
			let sims = unit
				.matmul(&unit.t().unwrap())
				.unwrap()
				.to_vec2::<f32>()
				.unwrap();
			println!("issue_a\tissue_b\tscore");
			for x in 0..n {
				for y in x + 1..n {
					if sims[x][y] as f64 >= 0.80 {
						println!("#{}\t#{}\t{}", numbers[x], numbers[y], sims[x][y]);
					}
				}
			}
		}
		// record 0139, U4: whole tickets chunked and pooled (as E4), scored
		// against the frozen label descriptions; the best (ties to the earlier
		// label), the second, the f64 margin, review below 0.02; the table as
		// the script writes it
		"u4" => {
			let t = twin_load(&a[3]);
			let v: serde_json::Value =
				serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
			let mut names = Vec::new();
			let mut descriptions = Vec::new();
			for line in std::fs::read_to_string(&a[4]).unwrap().lines().skip(1) {
				if line.is_empty() {
					continue;
				}
				let f: Vec<&str> = line.split('\t').collect();
				names.push(f[0].to_owned());
				descriptions.push(f[1].to_owned());
			}
			let k = names.len();
			let mut numbers = Vec::new();
			let mut passages = Vec::new();
			let mut owner: Vec<u32> = Vec::new();
			for (i, issue) in v.as_array().unwrap().iter().enumerate() {
				numbers.push(issue["number"].as_i64().unwrap());
				let text = format!(
					"{}\n{}",
					issue["title"].as_str().unwrap(),
					issue["body"].as_str().unwrap()
				);
				for (s, e) in token_chunks(&t.tok, &text, 256, 0).unwrap() {
					passages.push(text[s..e].to_owned());
					owner.push(i as u32);
				}
			}
			let n = numbers.len();
			let cpu = &Device::Cpu;
			let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
			let flat = twin_embed_sized(&t, &refs, &s1_sizes(&t, &refs, 32));
			let rows = refs.len();
			let emb = Tensor::from_vec(flat, (rows, 384), cpu).unwrap();
			let seg = Tensor::from_vec(owner, rows, cpu).unwrap();
			let sums = Tensor::zeros((n, 384), DType::F32, cpu)
				.unwrap()
				.index_add(&seg, &emb, 0)
				.unwrap();
			let counts = Tensor::zeros(n, DType::F32, cpu)
				.unwrap()
				.index_add(&seg, &Tensor::ones(rows, DType::F32, cpu).unwrap(), 0)
				.unwrap();
			let pooled = sums.broadcast_div(&counts.unsqueeze(1).unwrap()).unwrap();
			let norms = pooled
				.sqr()
				.unwrap()
				.sum_keepdim(1)
				.unwrap()
				.sqrt()
				.unwrap();
			let tickets = pooled.broadcast_div(&norms).unwrap();
			let drefs: Vec<&str> = descriptions.iter().map(String::as_str).collect();
			let lflat = twin_embed_sized(&t, &drefs, &s1_sizes(&t, &drefs, 32));
			let labels = Tensor::from_vec(lflat, (k, 384), cpu).unwrap();
			let scores = tickets
				.matmul(&labels.t().unwrap())
				.unwrap()
				.to_vec2::<f32>()
				.unwrap();
			let mut header = "ticket\ttriage\tbest\tsecond\tmargin".to_owned();
			for name in &names {
				header.push_str(&format!("\t{name}"));
			}
			println!("{header}");
			for (i, row) in scores.iter().enumerate() {
				let row: Vec<f64> = row.iter().map(|&x| x as f64).collect();
				let mut best = 0;
				for j in 1..k {
					if row[j] > row[best] {
						best = j;
					}
				}
				let mut next = if best == 0 { 1 } else { 0 };
				for j in 0..k {
					if j != best && row[j] > row[next] {
						next = j;
					}
				}
				let m = row[best] - row[next];
				let triage = if m < 0.02 {
					"review"
				} else {
					names[best].as_str()
				};
				let mut line = format!(
					"{}\t{triage}\t{}\t{}\t{m}",
					numbers[i], names[best], names[next]
				);
				for x in &row {
					line.push_str(&format!("\t{x}"));
				}
				println!("{line}");
			}
		}
		// record 0137, E5: the tour's results, by the same Candle calls
		"e5" => {
			let cpu = &Device::Cpu;
			let row = |name: &str, t: &Tensor| {
				let dims: String = t.dims().iter().map(|d| format!("{d},")).collect();
				let vals: String = t
					.flatten_all()
					.unwrap()
					.to_vec1::<f32>()
					.unwrap()
					.iter()
					.map(|x| format!("{x},"))
					.collect();
				println!("{name}\t{dims}\t{vals}");
			};
			let a = Tensor::arange(0f32, 12., cpu)
				.unwrap()
				.reshape((3, 4))
				.unwrap();
			let b = Tensor::arange(100f32, 108., cpu)
				.unwrap()
				.reshape((2, 4))
				.unwrap();
			let joined = Tensor::cat(&[&a, &b], 0).unwrap();
			row("cat", &joined);
			let stacked = Tensor::stack(&[&a, &a], 1).unwrap();
			row("stack", &stacked);
			row("permute", &stacked.permute((2, 0, 1)).unwrap());
			let pick = Tensor::from_vec(vec![4u32, 0, 2], 3, cpu).unwrap();
			row("index_select", &joined.index_select(&pick, 0).unwrap());
			let g =
				Tensor::from_vec(vec![3i64, 2, 1, 0, 0, 1, 2, 3, 1, 1, 1, 1], (3, 4), cpu).unwrap();
			row("gather", &a.gather(&g, 1).unwrap());
			row("cumsum", &a.cumsum(1).unwrap());
			row(
				"unfold",
				&Tensor::arange(0f32, 6., cpu)
					.unwrap()
					.unfold(0, 3, 1)
					.unwrap(),
			);
			let patch = Tensor::full(-1f32, (2, 2), cpu).unwrap();
			row(
				"slice_assign",
				&a.slice_assign(&[0..2, 1..3], &patch).unwrap(),
			);
			row("a_unchanged", &a);
			let gi = Tensor::from_vec(vec![1u32, 0, 1], 3, cpu).unwrap();
			row(
				"index_add",
				&Tensor::zeros((2, 4), DType::F32, cpu)
					.unwrap()
					.index_add(&gi, &a, 0)
					.unwrap(),
			);
			let sums = Tensor::zeros((2, 4), DType::F32, cpu)
				.unwrap()
				.index_add(&gi, &a, 0)
				.unwrap();
			let counts = Tensor::zeros(2, DType::F32, cpu)
				.unwrap()
				.index_add(&gi, &Tensor::ones(3, DType::F32, cpu).unwrap(), 0)
				.unwrap();
			row(
				"segment_mean",
				&sums.broadcast_div(&counts.unsqueeze(1).unwrap()).unwrap(),
			);
		}
		// record 0136: every passage's byte range, D1 bodies (as U1 splits a
		// document) then D2 tickets (title, newline, body), at an overlap
		"chunks" => {
			let tok = twin_load(&a[4]).tok;
			let overlap: usize = a[5].parse().unwrap();
			println!("source\tindex\tstart\tend");
			let mut names: Vec<String> = std::fs::read_dir(&a[2])
				.unwrap()
				.map(|e| e.unwrap().file_name().into_string().unwrap())
				.filter(|n| n.ends_with(".md"))
				.collect();
			names.sort();
			for n in names {
				let text = std::fs::read_to_string(format!("{}/{n}", a[2])).unwrap();
				let first = text.split('\n').next().unwrap_or("");
				match token_chunks(&tok, &text[first.len()..], 256, overlap) {
					Ok(r) => {
						for (k, (s, e)) in r.into_iter().enumerate() {
							println!("d1:{n}\t{k}\t{s}\t{e}");
						}
					}
					Err(e) => println!("d1:{n}\tREFUSED\t{e}\t"),
				}
			}
			let v: serde_json::Value =
				serde_json::from_slice(&std::fs::read(&a[3]).unwrap()).unwrap();
			for i in v.as_array().unwrap() {
				let text = format!(
					"{}\n{}",
					i["title"].as_str().unwrap(),
					i["body"].as_str().unwrap()
				);
				match token_chunks(&tok, &text, 256, overlap) {
					Ok(r) => {
						for (k, (s, e)) in r.into_iter().enumerate() {
							println!("d2:#{}\t{k}\t{s}\t{e}", i["number"]);
						}
					}
					Err(e) => println!("d2:#{}\tREFUSED\t{e}\t", i["number"]),
				}
			}
		}
		// record 0143, E6: tickets chunked, embedded and pooled as E4/U4, then
		// the frozen spherical k-means; the trace as the script writes it
		"e6" => {
			let t = twin_load(&a[3]);
			let v: serde_json::Value =
				serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
			let mut passages = Vec::new();
			let mut owner: Vec<u32> = Vec::new();
			for (i, issue) in v.as_array().unwrap().iter().enumerate() {
				let text = format!(
					"{}\n{}",
					issue["title"].as_str().unwrap(),
					issue["body"].as_str().unwrap()
				);
				for (s, e) in token_chunks(&t.tok, &text, 256, 0).unwrap() {
					passages.push(text[s..e].to_owned());
					owner.push(i as u32);
				}
			}
			let n = v.as_array().unwrap().len();
			let cpu = &Device::Cpu;
			let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
			let flat = twin_embed_sized(&t, &refs, &s1_sizes(&t, &refs, 32));
			let rows = refs.len();
			let emb = Tensor::from_vec(flat, (rows, 384), cpu).unwrap();
			let seg = Tensor::from_vec(owner, rows, cpu).unwrap();
			let u = e6_unit(&e6_segment_mean(&emb, &seg, n)).unwrap();
			print!("{}", e6_trace(&e6_kmeans(&u, 6).unwrap()));
		}
		"e6-synth" => {
			let spec: serde_json::Value =
				serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
			let k = spec["k"].as_u64().unwrap() as usize;
			let rows = spec["rows"].as_array().unwrap();
			let w = rows[0].as_array().unwrap().len();
			let flat: Vec<f32> = rows
				.iter()
				.flat_map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32))
				.collect();
			let x = Tensor::from_vec(flat, (rows.len(), w), &Device::Cpu).unwrap();
			match e6_unit(&x).and_then(|u| e6_kmeans(&u, k)) {
				Ok(r) => print!("{}", e6_trace(&r)),
				Err(e) => println!("refused\t{e}"),
			}
		}
		// record 0145, E7: D3's views (articles in columns), the frozen method
		"e7" => {
			let text = std::fs::read_to_string(&a[2]).unwrap();
			let mut lines = text.lines();
			let c = lines.next().unwrap().split('\t').count() - 1;
			let mut cols: Vec<Vec<f64>> = vec![Vec::new(); c];
			for line in lines.filter(|l| !l.is_empty()) {
				for (ch, f) in line.split('\t').skip(1).enumerate() {
					cols[ch].push(f.parse::<i64>().unwrap() as f64);
				}
			}
			print!("{}", e7(&cols).unwrap_or_else(|e| format!("refused\t{e}\n")));
		}
		"e7-synth" => {
			let spec: serde_json::Value =
				serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
			let s: Vec<f64> = spec["series"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
			print!("{}", e7(&[s]).unwrap_or_else(|e| format!("refused\t{e}\n")));
		}
		// record 0135: D1's passages, as U1 chunks them, for timing
		"passages" => {
			let texts: Vec<String> = load(&a[2]).1.into_iter().map(|p| p.1).collect();
			println!("{}", serde_json::to_string(&texts).unwrap());
		}
		other => panic!("unknown command {other}"),
	}
}

// ---- record 0143, E6: the frozen k-means, independently of the script ----

/// Candle's segment mean by the calls `candle::segment_mean` documents.
fn e6_segment_mean(v: &Tensor, seg: &Tensor, n: usize) -> Tensor {
	let cpu = &Device::Cpu;
	let (rows, w) = v.dims2().unwrap();
	let sums = Tensor::zeros((n, w), DType::F32, cpu).unwrap().index_add(seg, v, 0).unwrap();
	let ones = Tensor::ones(rows, DType::F32, cpu).unwrap();
	let counts = Tensor::zeros(n, DType::F32, cpu).unwrap().index_add(seg, &ones, 0).unwrap();
	sums.broadcast_div(&counts.unsqueeze(1).unwrap()).unwrap()
}

/// Rows to unit length; a zero or non-finite norm is refused.
fn e6_unit(x: &Tensor) -> Result<Tensor, String> {
	let norms = x.sqr().unwrap().sum_keepdim(1).unwrap().sqrt().unwrap();
	for z in norms.flatten_all().unwrap().to_vec1::<f32>().unwrap() {
		if !(z > 0.0 && z.is_finite()) {
			return Err(format!("norm {z}"));
		}
	}
	Ok(x.broadcast_div(&norms).unwrap())
}

struct E6 {
	chosen: Vec<u32>,
	moves: Vec<(usize, usize, usize)>,
	history: Vec<Vec<u32>>,
	it: usize,
	converged: bool,
	a: Vec<u32>,
	centroids: Tensor,
}

fn e6_kmeans(u: &Tensor, k: usize) -> Result<E6, String> {
	let cpu = &Device::Cpu;
	let n = u.dims()[0];
	let ids = |v: &[u32]| Tensor::from_vec(v.to_vec(), v.len(), cpu).unwrap();
	let g = e6_unit(&u.mean_keepdim(0).unwrap())?;
	let first = u.matmul(&g.t().unwrap()).unwrap().argmax(0).unwrap().to_vec1::<u32>().unwrap()[0];
	let mut chosen = vec![first];
	while chosen.len() < k {
		let c = u.index_select(&ids(&chosen), 0).unwrap();
		let m = u.matmul(&c.t().unwrap()).unwrap().max(1).unwrap();
		chosen.push(m.argmin(0).unwrap().to_scalar::<u32>().unwrap());
	}
	let mut centroids = u.index_select(&ids(&chosen), 0).unwrap();
	let mut prev: Option<Vec<u32>> = None;
	let mut moves = Vec::new();
	let mut history = Vec::new();
	let (mut it, mut converged) = (0, false);
	let mut a: Vec<u32> = Vec::new();
	while it < 50 {
		it += 1;
		let st = u.matmul(&centroids.t().unwrap()).unwrap();
		let s = st.to_vec2::<f32>().unwrap();
		a = st.argmax(1).unwrap().to_vec1::<u32>().unwrap();
		let mut sizes = vec![0usize; k];
		for &x in &a {
			sizes[x as usize] += 1;
		}
		for c in 0..k {
			if sizes[c] != 0 {
				continue;
			}
			// the eligible ticket (its cluster has two or more members) least
			// similar to its own centroid; the first such on ties
			let mut donor: Option<usize> = None;
			for i in 0..n {
				let own = a[i] as usize;
				if sizes[own] < 2 {
					continue;
				}
				if donor.is_none_or(|d| s[i][own] < s[d][a[d] as usize]) {
					donor = Some(i);
				}
			}
			let d = donor.expect("n > k");
			sizes[a[d] as usize] -= 1;
			a[d] = c as u32;
			sizes[c] = 1;
			moves.push((it, c, d));
		}
		history.push(a.clone());
		if prev.as_ref() == Some(&a) {
			converged = true;
			break;
		}
		centroids = e6_unit(&e6_segment_mean(u, &ids(&a), k))?;
		prev = Some(a.clone());
	}
	Ok(E6 { chosen, moves, history, it, converged, a, centroids })
}

/// The trace, formatted as Rune formats it (f64 as Rust's `{:?}`).
fn e6_trace(r: &E6) -> String {
	let mut out = format!(
		"init\t{}\n",
		r.chosen.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(",")
	);
	for (it, c, d) in &r.moves {
		out += &format!("move\t{it}\t{c}\t{d}\n");
	}
	for (k, h) in r.history.iter().enumerate() {
		let v: Vec<String> = h.iter().map(|x| x.to_string()).collect();
		out += &format!("iter\t{}\t{}\n", k + 1, v.join(","));
	}
	out += &format!("iterations\t{}\t{}\n", r.it, r.converged);
	for (i, x) in r.a.iter().enumerate() {
		out += &format!("assign\t{i}\t{x}\n");
	}
	for (c, row) in r.centroids.to_vec2::<f32>().unwrap().iter().enumerate() {
		let vals: Vec<String> = row.iter().map(|&x| format!("{:?}", x as f64)).collect();
		out += &format!("centroid\t{c}\t{}\n", vals.join(","));
	}
	out
}

// ---- record 0145, E7: the frozen traffic method, independently ----

fn e7_median(t: &Tensor, n: usize) -> Tensor {
	let (s, _) = t.sort_last_dim(true).unwrap();
	if n % 2 == 1 {
		return s.narrow(2, n / 2, 1).unwrap();
	}
	s.narrow(2, n / 2 - 1, 1)
		.unwrap()
		.add(&s.narrow(2, n / 2, 1).unwrap())
		.unwrap()
		.affine(0.5, 0.0)
		.unwrap()
}

fn e7(cols: &[Vec<f64>]) -> Result<String, String> {
	let cpu = &Device::Cpu;
	let c = cols.len();
	let days = cols[0].len();
	if cols.iter().flatten().any(|&v| !(v > 0.0)) {
		return Err("a non-positive count".into());
	}
	let flat: Vec<f64> = cols.iter().flatten().copied().collect();
	let views = Tensor::from_vec(flat, (1, c, days), cpu).unwrap();
	let x = views.log().unwrap();
	let k = Tensor::full(1.0f64 / 29.0, (c, 1, 29), cpu).unwrap();
	let sums = x.conv1d(&k, 14, 1, 1, c).unwrap();
	let norm = Tensor::ones((1, c, days), DType::F64, cpu).unwrap().conv1d(&k, 14, 1, 1, c).unwrap();
	let trend = sums.div(&norm).unwrap();
	let r = x.sub(&trend).unwrap();
	let med = e7_median(&r, days);
	let dev = r.broadcast_sub(&med).unwrap().abs().unwrap();
	let mad = e7_median(&dev, days).affine(1.4826, 0.0).unwrap();
	if mad.flatten_all().unwrap().to_vec1::<f64>().unwrap().iter().any(|&v| !(v > 0.0)) {
		return Err("zero MAD".into());
	}
	let z = r.broadcast_sub(&med).unwrap().broadcast_div(&mad).unwrap();
	let weeks = days / 7;
	let whole = weeks * 7;
	let xw = x.narrow(2, 0, whole).unwrap();
	let wmean = xw.reshape((1, c, 1, whole)).unwrap().avg_pool2d_with_stride((1, 7), (1, 7)).unwrap();
	let up = wmean.reshape((1, c, weeks)).unwrap().upsample_nearest1d(whole).unwrap();
	let profile = xw
		.sub(&up)
		.unwrap()
		.reshape((c, weeks, 7))
		.unwrap()
		.mean(1)
		.unwrap()
		.exp()
		.unwrap()
		.affine(1.0, -1.0)
		.unwrap()
		.to_vec2::<f64>()
		.unwrap();
	let raw = views
		.narrow(2, 0, whole)
		.unwrap()
		.reshape((1, c, 1, whole))
		.unwrap()
		.avg_pool2d_with_stride((1, 7), (1, 7))
		.unwrap();
	let top = raw
		.max_pool2d_with_stride((1, weeks), (1, weeks))
		.unwrap()
		.reshape(c)
		.unwrap()
		.to_vec1::<f64>()
		.unwrap();
	let rawv = raw.reshape((c, weeks)).unwrap().to_vec2::<f64>().unwrap();
	let trend = trend.reshape((c, days)).unwrap().to_vec2::<f64>().unwrap();
	let z = z.reshape((c, days)).unwrap().to_vec2::<f64>().unwrap();
	let join = |xs: &[f64]| xs.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(",");
	let mut out = String::new();
	for ch in 0..c {
		out += &format!("trend\t{ch}\t{}\n", join(&trend[ch]));
		out += &format!("z\t{ch}\t{}\n", join(&z[ch]));
		// runs of z >= 4: peak = highest z (ties to the earliest); then the
		// five largest peaks (ties to the earliest peak)
		let row = &z[ch];
		let mut found: Vec<(f64, usize, usize)> = Vec::new();
		let mut t = 0;
		while t < row.len() {
			if row[t] >= 4.0 {
				let (s, mut p) = (t, t);
				while t < row.len() && row[t] >= 4.0 {
					if row[t] > row[p] {
						p = t;
					}
					t += 1;
				}
				found.push((row[p], p, t - s));
			} else {
				t += 1;
			}
		}
		found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
		for (zv, p, len) in found.iter().take(5) {
			out += &format!("event\t{ch}\t{p}\t{len}\t{zv:?}\n");
		}
		out += &format!("weekday\t{ch}\t{}\n", join(&profile[ch]));
		let w = rawv[ch].iter().position(|&v| v == top[ch]).unwrap();
		out += &format!("busiest\t{ch}\t{w}\t{:?}\n", top[ch]);
	}
	Ok(out)
}
