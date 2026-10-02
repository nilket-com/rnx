//! Record 0146: `candle::CrossEncoder`, a BERT sequence-classification
//! model with one label (cross-encoder/ms-marco-MiniLM-L6-v2 at its pinned
//! revision), scoring (query, passage) pairs. Candle's `BertModel` has no
//! pooler or classifier; they are added here from the same weights: the
//! pooler is a dense layer and tanh over the first (`[CLS]`) token, the
//! classifier a 384 → 1 layer, and the score is its raw logit (the model's
//! sentence-transformers activation is the identity).
//!
//! The pair tokenizer's contract is explicit (not `embed`'s, whose types are
//! all zero): the `[CLS] A [SEP] B [SEP]` template with types 0 and 1, a
//! per-instance tokenizer with truncation and padding disabled, and every
//! pair's ids, token types and mask kept through right padding. A pair over
//! the model's length is refused by index, never truncated.
//!
//! Batches follow record 0135's S1 rule, applied to pairs, and run through
//! `embed`'s executor (concurrent within the budget, results published only
//! when every batch succeeds, the lowest-index error otherwise).
use super::{
	AGG, BATCH, CAPS, CONCURRENCY, Caps, Knobs, MAX_TEXT, MAX_TEXTS, MAX_TOKENIZER, MAX_TOTAL,
	MAX_WEIGHTS, Planned, Trace, bert_config, estimate, execute, fits, json, unused_buffer,
};
use crate::{read_limited, worker};
use candle_core::{DType, Device, Module, Tensor as CTensor};
use candle_nn::{Linear, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config};
use rnx::rune::{self, Any, runtime::Vec as RuneVec};
use std::sync::Arc;
use tokenizers::Tokenizer;

const ARCHITECTURE: &str = "BertForSequenceClassification";
const IDENTITY: &str = "torch.nn.modules.linear.Identity";

/// The model the executor runs: the encoder, the pooler and the classifier.
pub(crate) struct Head {
	bert: BertModel,
	pooler: Linear,
	classifier: Linear,
}

struct Inner {
	head: Head,
	tokenizer: Tokenizer,
	config: Config,
	/// The longest pair, in tokens: `max_position_embeddings`.
	max_seq: usize,
}

/// A loaded cross-encoder. Cloning shares it.
#[derive(Any, Clone)]
#[rune(item = ::candle)]
pub struct CrossEncoder(Arc<Inner>);

/// The config's sequence-classification fields: the architecture, exactly
/// one label, and the identity activation (sentence-transformers would
/// otherwise apply a sigmoid to a one-label model's logit).
fn contract(dir: &std::path::Path, op: &str) -> Result<Config, String> {
	let raw = json(dir, "config.json", op)?;
	let config = bert_config(&raw, op)?;
	let archs: Vec<&str> = raw
		.get("architectures")
		.and_then(|v| v.as_array())
		.map(|a| a.iter().filter_map(|v| v.as_str()).collect())
		.unwrap_or_default();
	if archs != [ARCHITECTURE] {
		return Err(format!(
			"{op}: config.json architectures is {archs:?}, want [{ARCHITECTURE:?}]"
		));
	}
	let labels = raw
		.get("id2label")
		.and_then(|v| v.as_object())
		.map(|o| o.len());
	if labels != Some(1) {
		return Err(format!(
			"{op}: config.json has {labels:?} labels, want exactly 1 (a relevance score)"
		));
	}
	let activation = raw
		.get("sbert_ce_default_activation_function")
		.and_then(|v| v.as_str());
	if activation != Some(IDENTITY) {
		return Err(format!(
			"{op}: config.json sbert_ce_default_activation_function is {activation:?}, want {IDENTITY:?} (the score is the raw logit)"
		));
	}
	Ok(config)
}

/// The exact pair template, as tokenizers serializes it.
fn pair_template() -> serde_json::Value {
	serde_json::json!([
		{"SpecialToken": {"id": "[CLS]", "type_id": 0}},
		{"Sequence": {"id": "A", "type_id": 0}},
		{"SpecialToken": {"id": "[SEP]", "type_id": 0}},
		{"Sequence": {"id": "B", "type_id": 1}},
		{"SpecialToken": {"id": "[SEP]", "type_id": 1}}
	])
}

/// The tokenizer: its ids checked against the model, the pair template
/// checked exactly, and its own truncation and padding disabled, so a
/// pair's full length is measured.
fn tokenizer(dir: &std::path::Path, config: &Config, op: &str) -> Result<Tokenizer, String> {
	pair_tokenizer(dir, config.vocab_size, config.pad_token_id, op)
}

/// Record 0148: the pair-tokenizer contract, shared with `NliModel`: every
/// id below `vocab_size`, the pad id present, the exact pair template and
/// special-token definitions, a complete probe pair, and truncation and
/// padding disabled.
pub(crate) fn pair_tokenizer(
	dir: &std::path::Path,
	vocab_size: usize,
	pad_token_id: usize,
	op: &str,
) -> Result<Tokenizer, String> {
	let path = dir.join("tokenizer.json");
	let path = path
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(path, MAX_TOKENIZER, op)?;
	let mut t = Tokenizer::from_bytes(&bytes).map_err(|e| format!("{op} {path:?}: {e}"))?;
	if let Some((token, id)) = t
		.get_vocab(true)
		.into_iter()
		.find(|(_, id)| *id as usize >= vocab_size)
	{
		return Err(format!(
			"{op}: tokenizer id {id} ({token:?}) is not below vocab_size {vocab_size}"
		));
	}
	t.id_to_token(pad_token_id as u32)
		.ok_or_else(|| format!("{op}: pad_token_id {pad_token_id} is not in the tokenizer"))?;
	let post = t
		.get_post_processor()
		.and_then(|p| serde_json::to_value(p).ok())
		.unwrap_or(serde_json::Value::Null);
	if post.get("type").and_then(|v| v.as_str()) != Some("TemplateProcessing")
		|| post.get("pair") != Some(&pair_template())
	{
		return Err(format!(
			"{op}: tokenizer.json's pair template must be [CLS] A [SEP] B [SEP] with token types 0, 0, 0, 1, 1"
		));
	}
	// review round 1: each special token is defined as exactly itself, one
	// vocabulary id mapping to that token
	for name in ["[CLS]", "[SEP]"] {
		let id = t.token_to_id(name);
		let def = post.get("special_tokens").and_then(|s| s.get(name));
		let ids = def.and_then(|d| d.get("ids"));
		let tokens = def.and_then(|d| d.get("tokens"));
		let ok = id.is_some()
			&& def.and_then(|d| d.get("id")).and_then(|v| v.as_str()) == Some(name)
			&& ids == Some(&serde_json::json!([id]))
			&& tokens == Some(&serde_json::json!([name]));
		if !ok {
			return Err(format!(
				"{op}: tokenizer.json's {name} must be defined as exactly one token, itself (id {id:?})"
			));
		}
	}
	t.with_truncation(None)
		.map_err(|e| format!("{op}: truncation: {e}"))?;
	t.with_padding(None);
	// behaviourally too, the complete structure: a probe pair encodes as
	// exactly [CLS] A [SEP] B [SEP], A and B as they encode alone, with
	// types 0 through the first [SEP] and 1 after it
	let (cls, sep) = (t.token_to_id("[CLS]"), t.token_to_id("[SEP]"));
	let alone = |s: &str| t.encode(s, false).map(|e| e.get_ids().to_vec());
	let probe = t.encode(("a", "b"), true);
	let ok = match (cls, sep, alone("a"), alone("b"), probe) {
		(Some(c), Some(s), Ok(a), Ok(b), Ok(p)) => {
			let mut want = vec![c];
			want.extend(&a);
			want.push(s);
			let first = want.len();
			want.extend(&b);
			want.push(s);
			let types: Vec<u32> = (0..want.len()).map(|i| u32::from(i >= first)).collect();
			p.get_ids() == want.as_slice() && p.get_type_ids() == types.as_slice()
		}
		_ => false,
	};
	if !ok {
		return Err(format!(
			"{op}: tokenizer.json does not encode a pair as [CLS] A [SEP] B [SEP] with types 0 and 1"
		));
	}
	Ok(t)
}

/// The exact weight keys: BertModel's encoder, the pooler, the classifier,
/// and the one unused integer buffer.
fn expected_keys(c: &Config) -> std::collections::BTreeSet<String> {
	let mut keys = std::collections::BTreeSet::new();
	for k in [
		"word_embeddings.weight",
		"position_embeddings.weight",
		"token_type_embeddings.weight",
		"LayerNorm.weight",
		"LayerNorm.bias",
	] {
		keys.insert(format!("bert.embeddings.{k}"));
	}
	for l in 0..c.num_hidden_layers {
		for k in [
			"attention.self.query",
			"attention.self.key",
			"attention.self.value",
			"attention.output.dense",
			"attention.output.LayerNorm",
			"intermediate.dense",
			"output.dense",
			"output.LayerNorm",
		] {
			for p in ["weight", "bias"] {
				keys.insert(format!("bert.encoder.layer.{l}.{k}.{p}"));
			}
		}
	}
	for k in [
		"bert.pooler.dense.weight",
		"bert.pooler.dense.bias",
		"classifier.weight",
		"classifier.bias",
		"bert.embeddings.position_ids",
	] {
		keys.insert(k.to_string());
	}
	keys
}

fn load(dir: &str) -> Result<CrossEncoder, String> {
	let op = "CrossEncoder::load";
	let root = std::path::Path::new(dir);
	let config = contract(root, op)?;
	let tokenizer = tokenizer(root, &config, op)?;
	let weights = root.join("model.safetensors");
	let weights = weights
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(weights, MAX_WEIGHTS, op)?;
	let weights = weights.to_owned();
	let c = config.clone();
	let head = worker::run(op, move || -> Result<Head, String> {
		let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu)
			.map_err(|e| format!("{op} {weights:?}: {e}"))?;
		let have: std::collections::BTreeSet<String> = tensors.keys().cloned().collect();
		let want = expected_keys(&c);
		if let Some(k) = have.difference(&want).next() {
			return Err(format!("{op}: unexpected tensor {k}"));
		}
		if let Some(k) = want.difference(&have).next() {
			return Err(format!("{op}: missing tensor {k}"));
		}
		if let Some((k, t)) = tensors
			.iter()
			.find(|(k, t)| t.dtype() != DType::F32 && !unused_buffer(k, t.dtype()))
		{
			return Err(format!("{op}: {k} is {:?}, want F32", t.dtype()));
		}
		// the unused buffer: exactly I64 (review round 1: `unused_buffer`
		// admits any integer dtype for TextEncoder; this contract is I64),
		// holding 0 .. max_position_embeddings - 1
		let positions = &tensors["bert.embeddings.position_ids"];
		if positions.dtype() != DType::I64 {
			return Err(format!(
				"{op}: bert.embeddings.position_ids is {:?}, want I64",
				positions.dtype()
			));
		}
		let n = c.max_position_embeddings;
		let ok = positions.dims() == [1, n]
			&& positions
				.flatten_all()
				.and_then(|t| t.to_dtype(DType::I64))
				.and_then(|t| t.to_vec1::<i64>())
				.is_ok_and(|v| v.iter().enumerate().all(|(i, &x)| x == i as i64));
		if !ok {
			return Err(format!(
				"{op}: bert.embeddings.position_ids must be [1, {n}] holding 0 to {}",
				n - 1
			));
		}
		let h = c.hidden_size;
		for (k, shape) in [
			("bert.pooler.dense.weight", vec![h, h]),
			("bert.pooler.dense.bias", vec![h]),
			("classifier.weight", vec![1, h]),
			("classifier.bias", vec![1]),
		] {
			if tensors[k].dims() != shape.as_slice() {
				return Err(format!(
					"{op}: {k} is {:?}, want {shape:?}",
					tensors[k].dims()
				));
			}
		}
		let vb = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
		let e = |e: candle_core::Error| format!("{op} {weights:?}: {e}");
		let bert = BertModel::load(vb.clone(), &c).map_err(e)?;
		let pooler = candle_nn::linear(h, h, vb.pp("bert.pooler.dense")).map_err(e)?;
		let classifier = candle_nn::linear(h, 1, vb.pp("classifier")).map_err(e)?;
		Ok(Head {
			bert,
			pooler,
			classifier,
		})
	})??;
	let max_seq = config.max_position_embeddings;
	Ok(CrossEncoder(Arc::new(Inner {
		head,
		tokenizer,
		config,
		max_seq,
	})))
}

/// A pair batch's in-flight estimate: 0135's encoder terms, plus the head
/// (the first token's state, the pooler's dense and tanh outputs, and the
/// classifier's scores).
pub(crate) fn estimate_pairs(c: &Config, b: usize, seq: usize) -> Option<usize> {
	estimate(c, b, seq)?
		.checked_add(b.checked_mul(c.hidden_size)?.checked_mul(3)?)?
		.checked_add(b)
}

/// The planned storage: ids, types and mask, 4 bytes each, at the largest
/// input the contract admits.
pub const PAIR_PAYLOAD: usize = 3 * 4 * MAX_TEXTS * 512;

/// Stage 1, sequential: pairs in input order, encoded once each, then 0135's
/// S1 rule: a batch starts at 32 pairs and, while it holds more than one,
/// halves until the caps hold and its estimate is within its share; a
/// single pair needs the caps and the full budget. Stops at the first
/// failing batch.
fn plan(
	inner: &Inner,
	pairs: &[(&str, &str)],
	caps: Caps,
	agg: usize,
	knobs: &Knobs,
	op: &str,
) -> (Vec<Planned>, Option<String>) {
	let c = &inner.config;
	let model = PairModel {
		tokenizer: &inner.tokenizer,
		max_seq: inner.max_seq,
		pad: c.pad_token_id as u32,
		vocab_size: c.vocab_size,
		type_vocab_size: Some(c.type_vocab_size),
		estimate: &|b, seq| estimate_pairs(c, b, seq),
		fits: &|b, seq| fits(c, b, seq, caps),
	};
	plan_pairs(&model, pairs, agg, knobs, op)
}

/// Record 0148: what the S1 pair planner needs from a model, so
/// `CrossEncoder` and `NliModel` share one planner.
pub(crate) struct PairModel<'a> {
	pub(crate) tokenizer: &'a Tokenizer,
	pub(crate) max_seq: usize,
	pub(crate) pad: u32,
	pub(crate) vocab_size: usize,
	/// `Some(n)`: each token type is checked below `n` and passed to the
	/// model; `None`: the model takes no token types.
	pub(crate) type_vocab_size: Option<usize>,
	pub(crate) estimate: &'a dyn Fn(usize, usize) -> Option<usize>,
	pub(crate) fits: &'a dyn Fn(usize, usize) -> bool,
}

pub(crate) fn plan_pairs(
	m: &PairModel,
	pairs: &[(&str, &str)],
	agg: usize,
	knobs: &Knobs,
	op: &str,
) -> (Vec<Planned>, Option<String>) {
	let n = pairs.len();
	let mut planned: Vec<Planned> = Vec::new();
	let mut payload = 0usize;
	let mut at = 0;
	let pad = m.pad;
	let mut cache: std::collections::VecDeque<(Vec<u32>, Vec<u32>)> = Default::default();
	let mut cache_at = 0;
	while at < n {
		let index = planned.len();
		let failed = |e: String| Some(e);
		if knobs.plan_fail_at == Some(index) {
			return (
				planned,
				failed(format!("{op}: injected planning failure at batch {index}")),
			);
		}
		let mut b = BATCH.min(n - at);
		while cache_at < at {
			cache.pop_front();
			cache_at += 1;
		}
		let have = cache_at + cache.len();
		if have < at + b {
			let encodings = match m.tokenizer.encode_batch(pairs[have..at + b].to_vec(), true) {
				Ok(e) => e,
				Err(e) => return (planned, failed(format!("{op}: {e}"))),
			};
			for (k, e) in encodings.into_iter().enumerate() {
				let len = e.get_ids().len();
				if len > m.max_seq {
					return (
						planned,
						failed(format!(
							"{op}: pair {} is {len} tokens, at most {} (chunk the passage; pairs are never truncated)",
							have + k,
							m.max_seq
						)),
					);
				}
				cache.push_back((e.get_ids().to_vec(), e.get_type_ids().to_vec()));
			}
		}
		let longest = |b: usize| {
			cache
				.iter()
				.take(b)
				.map(|(i, _)| i.len())
				.max()
				.unwrap_or(0)
		};
		let share = match knobs.concurrency.unwrap_or(CONCURRENCY) {
			0 | 1 => usize::MAX,
			k => agg / k,
		};
		loop {
			let seq = longest(b);
			let est = (m.estimate)(b, seq);
			if b == 1 {
				// the single-pair exception: the caps and the full budget
				if (m.fits)(1, seq) && est.is_some_and(|e| e <= agg) {
					break;
				}
				return (
					planned,
					failed(format!(
						"{op}: pair {at} at {seq} tokens exceeds the activation caps or the in-flight budget"
					)),
				);
			}
			if (m.fits)(b, seq) && est.is_some_and(|e| e <= share) {
				break;
			}
			b /= 2;
		}
		let seq = longest(b);
		let est = (m.estimate)(b, seq).unwrap_or(usize::MAX);
		let need = b.checked_mul(seq).and_then(|v| v.checked_mul(12));
		match need.and_then(|v| payload.checked_add(v)) {
			Some(total) if total <= PAIR_PAYLOAD => payload = total,
			_ => {
				return (
					planned,
					failed(format!(
						"{op}: the planned input exceeds {PAIR_PAYLOAD} bytes"
					)),
				);
			}
		}
		let mut ids = Vec::with_capacity(b * seq);
		let mut types = Vec::with_capacity(b * seq);
		let mut mask = Vec::with_capacity(b * seq);
		for (k, (pid, ptype)) in cache.iter().take(b).enumerate() {
			if let Some(&id) = pid.iter().find(|&&id| id as usize >= m.vocab_size) {
				return (
					planned,
					failed(format!(
						"{op}: pair {}: token id {id} is not below vocab_size {}",
						at + k,
						m.vocab_size
					)),
				);
			}
			if let Some(tv) = m.type_vocab_size
				&& let Some(&ty) = ptype.iter().find(|&&t| t as usize >= tv)
			{
				return (
					planned,
					failed(format!(
						"{op}: pair {}: token type {ty} is not below type_vocab_size {tv}",
						at + k
					)),
				);
			}
			ids.extend_from_slice(pid);
			ids.resize(ids.len() + seq - pid.len(), pad);
			types.extend_from_slice(ptype);
			types.resize(types.len() + seq - ptype.len(), 0);
			mask.resize(mask.len() + pid.len(), 1);
			mask.resize(mask.len() + seq - pid.len(), 0);
		}
		planned.push(Planned {
			b,
			seq,
			ids,
			types: m.type_vocab_size.map(|_| types),
			mask,
			estimate: est,
		});
		at += b;
	}
	(planned, None)
}

/// One pair batch: the encoder with the pairs' ids, token types and mask,
/// then the pooler (dense and tanh over the first token) and the
/// classifier; one logit per pair.
pub(crate) fn run_pairs(head: &Head, p: &Planned, op: &str) -> Result<Vec<f32>, String> {
	let cpu = &Device::Cpu;
	let e = |e: candle_core::Error| format!("{op}: {e}");
	let input = CTensor::from_slice(&p.ids, (p.b, p.seq), cpu).map_err(e)?;
	let types = p
		.types
		.as_ref()
		.ok_or_else(|| format!("{op}: a pair batch without token types"))?;
	let types = CTensor::from_slice(types, (p.b, p.seq), cpu).map_err(e)?;
	let mask = CTensor::from_slice(&p.mask, (p.b, p.seq), cpu).map_err(e)?;
	let hidden = head.bert.forward(&input, &types, Some(&mask)).map_err(e)?;
	let first = hidden.narrow(1, 0, 1).map_err(e)?.squeeze(1).map_err(e)?;
	let pooled = head.pooler.forward(&first).map_err(e)?.tanh().map_err(e)?;
	let logits = head.classifier.forward(&pooled).map_err(e)?;
	logits.flatten_all().map_err(e)?.to_vec1::<f32>().map_err(e)
}

/// The pairs, borrowed and checked before anything is copied or
/// tokenized: equal lengths, the count, each text, the combined bytes.
fn preflight(queries: &[&str], passages: &[&str], op: &str) -> Result<(), String> {
	pair_preflight(
		queries,
		passages,
		[("query", "queries"), ("passage", "passages")],
		op,
	)
}

/// Record 0148, shared with `NliModel`: the Rust-side checks of
/// `with_borrowed_pairs`, for callers that already hold `&str`s.
pub(crate) fn pair_preflight(
	queries: &[&str],
	passages: &[&str],
	[(a, aa), (b, bb)]: [(&str, &str); 2],
	op: &str,
) -> Result<(), String> {
	if queries.len() != passages.len() {
		return Err(format!(
			"{op}: {} {aa} and {} {bb}; one pair per index",
			queries.len(),
			passages.len()
		));
	}
	let n = queries.len();
	if n == 0 || n > MAX_TEXTS {
		return Err(format!("{op}: {n} pairs, want 1 to {MAX_TEXTS}"));
	}
	let mut total = 0usize;
	for (side, texts) in [(a, queries), (b, passages)] {
		for (i, s) in texts.iter().enumerate() {
			if s.len() > MAX_TEXT {
				return Err(format!(
					"{op}: {side} {i} is {} bytes, at most {MAX_TEXT}",
					s.len()
				));
			}
			total = total
				.checked_add(s.len())
				.filter(|t| *t <= MAX_TOTAL)
				.ok_or_else(|| format!("{op}: more than {MAX_TOTAL} bytes of text in all"))?;
		}
	}
	Ok(())
}

/// Both stages inside the joined worker; the error is the lowest failing
/// batch across planning and execution.
pub fn score_strs(
	this: &CrossEncoder,
	queries: &[&str],
	passages: &[&str],
	caps: Caps,
	knobs: &Knobs,
) -> (Result<Vec<f32>, String>, Trace) {
	let op = "CrossEncoder::score";
	let inner = &*this.0;
	let trace = Trace::default();
	if let Err(e) = preflight(queries, passages, op) {
		return (Err(e), trace);
	}
	let pairs: Vec<(&str, &str)> = queries
		.iter()
		.copied()
		.zip(passages.iter().copied())
		.collect();
	let agg = knobs.agg.unwrap_or(AGG);
	let workers = knobs
		.workers
		.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |p| p.get()));
	let ran = worker::run(op, || -> Result<(Vec<f32>, Trace), (String, Box<Trace>)> {
		let mut trace = Trace::default();
		let started = std::time::Instant::now();
		let (planned, plan_err) = plan(inner, &pairs, caps, agg, knobs, op);
		trace.plan_s = started.elapsed().as_secs_f64();
		trace.batches = planned.iter().map(|p| p.b).collect();
		let rows: usize = planned.iter().map(|p| p.b).sum();
		let mut out = vec![0f32; rows];
		let started = std::time::Instant::now();
		execute(
			&inner.head,
			run_pairs,
			1,
			&planned,
			&mut out,
			(workers, agg),
			knobs,
			&mut trace,
			op,
		);
		trace.exec_s = started.elapsed().as_secs_f64();
		if let Some((_, e)) = trace.failed.first() {
			return Err((e.clone(), Box::new(trace)));
		}
		if let Some(e) = plan_err {
			return Err((e, Box::new(trace)));
		}
		Ok((out, trace))
	});
	match ran {
		Ok(Ok((out, t))) => (Ok(out), t),
		Ok(Err((e, t))) => (Err(e), *t),
		Err(e) => (Err(e), trace),
	}
}

/// Two vectors of strings, borrowed (never taken).
fn borrowed<'a>(
	v: &'a rune::Value,
	what: &str,
	op: &str,
) -> Result<rune::runtime::BorrowRef<'a, RuneVec>, String> {
	v.borrow_ref::<RuneVec>()
		.map_err(|_| format!("{op}: {what} must be a vector of strings"))
}

fn score(
	this: &CrossEncoder,
	queries: rune::Value,
	passages: rune::Value,
) -> Result<Vec<f64>, String> {
	with_borrowed_pairs(
		&queries,
		&passages,
		[("query", "queries"), ("passage", "passages")],
		"CrossEncoder::score",
		|q, p| {
			score_strs(this, q, p, CAPS, &Knobs::default())
				.0
				.map(|v| v.into_iter().map(f64::from).collect())
		},
	)
}

/// Record 0148, shared with `NliModel`: two script vectors of strings,
/// borrowed (never taken) and checked before anything proportional to them
/// is allocated: equal lengths, the count, then every size and the combined
/// total one borrowed string at a time. `f` gets the borrowed strings.
pub(crate) fn with_borrowed_pairs<R>(
	first: &rune::Value,
	second: &rune::Value,
	[(a, aa), (b, bb)]: [(&str, &str); 2],
	op: &str,
	f: impl FnOnce(&[&str], &[&str]) -> Result<R, String>,
) -> Result<R, String> {
	let qv = borrowed(first, aa, op)?;
	let pv = borrowed(second, bb, op)?;
	// the counts first, from the vectors' lengths, before anything is copied
	if qv.len() != pv.len() {
		return Err(format!(
			"{op}: {} {aa} and {} {bb}; one pair per index",
			qv.len(),
			pv.len()
		));
	}
	if qv.is_empty() || qv.len() > MAX_TEXTS {
		return Err(format!("{op}: {} pairs, want 1 to {MAX_TEXTS}", qv.len()));
	}
	// every size and the combined total, one borrowed string at a time,
	// before anything proportional to the input is allocated
	let mut total = 0usize;
	for (side, v) in [(a, &qv), (b, &pv)] {
		for (i, x) in v.iter().enumerate() {
			let t = x
				.borrow_string_ref()
				.map_err(|_| format!("{op}: {side} {i} is not a string"))?;
			if t.len() > MAX_TEXT {
				return Err(format!(
					"{op}: {side} {i} is {} bytes, at most {MAX_TEXT}",
					t.len()
				));
			}
			total = total
				.checked_add(t.len())
				.filter(|t| *t <= MAX_TOTAL)
				.ok_or_else(|| format!("{op}: more than {MAX_TOTAL} bytes of text in all"))?;
		}
	}
	let mut qg = Vec::with_capacity(qv.len());
	for v in qv.iter() {
		qg.push(
			v.borrow_string_ref()
				.map_err(|_| format!("{op}: {aa} must be a vector of strings"))?,
		);
	}
	let mut pg = Vec::with_capacity(pv.len());
	for v in pv.iter() {
		pg.push(
			v.borrow_string_ref()
				.map_err(|_| format!("{op}: {bb} must be a vector of strings"))?,
		);
	}
	let q: Vec<&str> = qg.iter().map(|s| &**s).collect();
	let p: Vec<&str> = pg.iter().map(|s| &**s).collect();
	f(&q, &p)
}

/// Test support: a loaded cross-encoder from Rust.
pub fn load_dir(dir: &str) -> Result<CrossEncoder, String> {
	load(dir)
}

/// Test support: the batch partition for the given pair lengths would be
/// derived from (the estimate the twin reproduces independently).
pub fn estimate_for(this: &CrossEncoder, b: usize, seq: usize) -> Option<usize> {
	estimate_pairs(&this.0.config, b, seq)
}

pub(crate) fn build(
	m: &mut rune::Module,
) -> Result<Vec<(String, &'static str)>, rune::ContextError> {
	m.ty::<CrossEncoder>()?;
	m.function("load", load)
		.build_associated::<CrossEncoder>()?;
	m.associated_function("score", score)?;
	Ok(vec![
		(
			"candle::CrossEncoder".into(),
			"CrossEncoder: a BERT sequence-classification re-ranker (one label, raw logit) on the CPU",
		),
		(
			"candle::CrossEncoder::load".into(),
			"load(dir) -> Result<CrossEncoder>: config.json (BertForSequenceClassification, 1 label, identity activation), tokenizer.json ([CLS] A [SEP] B [SEP]) and model.safetensors",
		),
		(
			"candle::CrossEncoder::score".into(),
			"score(queries, passages) -> Result<Vec<f64>>: one relevance logit per (query, passage) pair, in order; a pair over 512 tokens is refused, never truncated",
		),
	])
}

/// Test support: the tiny generated model directory the unit tests use, for
/// the allocation controls (an integration test can't reach the unit tests).
#[cfg(feature = "test-support")]
pub fn fixture_dir() -> String {
	tests_fixture::good().to_str().unwrap().to_string()
}

#[cfg(any(test, feature = "test-support"))]
mod tests_fixture {
	pub(super) use super::tests_impl::*;
}

#[cfg(test)]
mod tests {
	use super::tests_impl::*;
	use super::*;
	use serde_json::json;

	#[test]
	fn the_fixture_scores_pairs_and_its_inputs_stay_usable() {
		let ce = load(good().to_str().unwrap()).unwrap();
		let q = list(&["the cat", "the cat", "a dog"]);
		let p = list(&["sat on the mat", "ran far away", "ran home now"]);
		let s = score(&ce, q.clone(), p.clone()).unwrap();
		assert_eq!(s.len(), 3);
		assert!(s.iter().all(|x| x.is_finite()));
		// the same pair alone gives the same score as inside the batch, within
		// the batch's padding effects
		let alone = score(&ce, list(&["the cat"]), list(&["sat on the mat"])).unwrap();
		assert!((alone[0] - s[0]).abs() < 1e-5, "{} {}", alone[0], s[0]);
		// the vectors after success and after refusals
		assert!(score(&ce, list(&["the cat"]), list(&[])).is_err());
		assert!(score(&ce, list(&[]), list(&[])).is_err());
		assert_eq!(rune::from_value::<Vec<String>>(q).unwrap().len(), 3);
		assert_eq!(rune::from_value::<Vec<String>>(p).unwrap().len(), 3);
	}

	#[test]
	fn scoring_refuses_by_name() {
		let ce = load(good().to_str().unwrap()).unwrap();
		let e = score(&ce, list(&["the"]), list(&["a", "b"])).unwrap_err();
		assert!(e.contains("1 queries and 2 passages"), "{e}");
		let e = score(&ce, list(&[]), list(&[])).unwrap_err();
		assert!(e.contains("0 pairs"), "{e}");
		let e = score(&ce, rune::to_value(vec![1i64]).unwrap(), list(&["a"])).unwrap_err();
		assert!(e.contains("query 0 is not a string"), "{e}");
		let long = "the ".repeat(MAX_TEXT / 4 + 1);
		let e = score(&ce, list(&["the"]), list(&[&long])).unwrap_err();
		assert!(e.contains("passage 0 is"), "{e}");
		// a pair of 32 tokens (the fixture's positions) is accepted; 33 is
		// refused, naming the pair, and never truncated
		let fits = "the ".repeat(32 - 5); // [CLS] q [SEP] ... [SEP] with a 2-word query
		assert!(score(&ce, list(&["the cat"]), list(&[fits.trim()])).is_ok());
		let over = "the ".repeat(32 - 4);
		let e = score(
			&ce,
			list(&["the cat", "the cat"]),
			list(&["mat", over.trim()]),
		)
		.unwrap_err();
		assert!(e.contains("pair 1 is 33 tokens, at most 32"), "{e}");
	}

	#[test]
	fn the_combined_bytes_are_bounded_before_anything_is_copied() {
		let ce = load(good().to_str().unwrap()).unwrap();
		let half = "a ".repeat(MAX_TEXT / 2 - 1);
		let n = MAX_TOTAL / (2 * half.len()) + 1;
		let q: Vec<&str> = vec![&half; n];
		let e = score_strs(&ce, &q, &q, CAPS, &Knobs::default())
			.0
			.unwrap_err();
		assert!(e.contains("bytes of text in all"), "{e}");
	}

	#[test]
	fn the_loading_contract_refuses_by_name() {
		refused(
			model(
				|c| c["id2label"] = json!({"0": "a", "1": "b"}),
				|_| {},
				|_| {},
			),
			"want exactly 1",
		);
		refused(
			model(
				|c| c["architectures"] = json!(["BertModel"]),
				|_| {},
				|_| {},
			),
			"architectures",
		);
		refused(
			model(
				|c| {
					c["sbert_ce_default_activation_function"] =
						json!("torch.nn.modules.activation.Sigmoid")
				},
				|_| {},
				|_| {},
			),
			"want \"torch.nn.modules.linear.Identity\"",
		);
		refused(
			model(
				|c| {
					c.as_object_mut()
						.unwrap()
						.remove("sbert_ce_default_activation_function");
				},
				|_| {},
				|_| {},
			),
			"sbert_ce_default_activation_function is None",
		);
		// the pair template: missing (BERT's processor), or altered
		refused(
			model(
				|_| {},
				|t| {
					t["post_processor"] =
						json!({"type": "BertProcessing", "sep": ["[SEP]", 3], "cls": ["[CLS]", 2]})
				},
				|_| {},
			),
			"pair template",
		);
		refused(
			model(
				|_| {},
				|t| t["post_processor"]["pair"][3]["Sequence"]["type_id"] = json!(0),
				|_| {},
			),
			"pair template",
		);
		refused(
			model(
				|_| {},
				|t| {
					let p = t["post_processor"]["pair"].as_array_mut().unwrap();
					p.swap(0, 1);
				},
				|_| {},
			),
			"pair template",
		);
		// the weights
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.remove("classifier.weight");
				},
			),
			"missing tensor classifier.weight",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.remove("bert.pooler.dense.bias");
				},
			),
			"missing tensor bert.pooler.dense.bias",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.insert(
						"extra.weight".into(),
						CTensor::zeros(2, DType::F32, &Device::Cpu).unwrap(),
					);
				},
			),
			"unexpected tensor extra.weight",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.insert(
						"classifier.weight".into(),
						CTensor::zeros((2, 8), DType::F32, &Device::Cpu).unwrap(),
					);
				},
			),
			"classifier.weight is [2, 8], want [1, 8]",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					let t = m["classifier.bias"].to_dtype(DType::F16).unwrap();
					m.insert("classifier.bias".into(), t);
				},
			),
			"classifier.bias is F16, want F32",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.insert(
						"bert.embeddings.position_ids".into(),
						CTensor::zeros((1, 32), DType::I64, &Device::Cpu).unwrap(),
					);
				},
			),
			"position_ids must be [1, 32] holding 0 to 31",
		);
		// review round 1: exactly I64, whatever integer width holds the values
		for dt in [DType::U32, DType::U8] {
			refused(
				model(
					|_| {},
					|_| {},
					|m| {
						let t = CTensor::arange(0u32, 32, &Device::Cpu)
							.unwrap()
							.reshape((1, 32))
							.unwrap()
							.to_dtype(dt)
							.unwrap();
						m.insert("bert.embeddings.position_ids".into(), t);
					},
				),
				"position_ids is",
			);
		}
		// review round 1: the special tokens are defined as exactly themselves
		refused(
			model(
				|_| {},
				|t| {
					t["post_processor"]["special_tokens"]["[CLS]"] =
						json!({"id": "[CLS]", "ids": [2, 5], "tokens": ["[CLS]", "cat"]});
				},
				|_| {},
			),
			"[CLS] must be defined as exactly one token",
		);
		refused(
			model(
				|_| {},
				|t| {
					t["post_processor"]["special_tokens"]["[SEP]"] =
						json!({"id": "[SEP]", "ids": [3, 3], "tokens": ["[SEP]", "[SEP]"]});
				},
				|_| {},
			),
			"[SEP] must be defined as exactly one token",
		);
		refused(
			model(
				|_| {},
				|t| {
					t["post_processor"]["special_tokens"]["[CLS]"] =
						json!({"id": "[CLS]", "ids": [3], "tokens": ["[CLS]"]});
				},
				|_| {},
			),
			"[CLS] must be defined as exactly one token",
		);
		// an embedding model's weights: no pooler, no classifier
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.retain(|k, _| !k.contains("pooler") && !k.starts_with("classifier"));
				},
			),
			"missing tensor",
		);
	}

	/// The segment types reach the model: the same pairs scored with all
	/// types zeroed give different scores.
	#[test]
	fn the_pairs_token_types_reach_the_model() {
		let ce = load(good().to_str().unwrap()).unwrap();
		let inner = &*ce.0;
		let pairs = [("the cat", "sat on the mat"), ("a dog", "ran far away")];
		let (planned, err) = plan(inner, &pairs, CAPS, AGG, &Knobs::default(), "test");
		assert!(err.is_none());
		let p = &planned[0];
		let types = p.types.as_ref().unwrap();
		assert!(types.contains(&1), "the second segment has type 1");
		let real = run_pairs(&inner.head, p, "test").unwrap();
		let zeroed = Planned {
			b: p.b,
			seq: p.seq,
			ids: p.ids.clone(),
			types: Some(vec![0; types.len()]),
			mask: p.mask.clone(),
			estimate: p.estimate,
		};
		let flat = run_pairs(&inner.head, &zeroed, "test").unwrap();
		assert!(
			real.iter().zip(&flat).any(|(a, b)| a != b),
			"{real:?} {flat:?}"
		);
	}

	#[cfg(unix)]
	#[test]
	fn a_fifo_config_is_refused_at_once() {
		let d = fresh("fifo");
		let c = std::ffi::CString::new(d.join("config.json").to_str().unwrap()).unwrap();
		assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
		let (tx, rx) = std::sync::mpsc::channel();
		let p = d.to_str().unwrap().to_string();
		std::thread::spawn(move || {
			let _ = tx.send(load(&p).map(|_| ()));
		});
		let e = rx
			.recv_timeout(std::time::Duration::from_secs(10))
			.expect("blocked")
			.unwrap_err();
		assert!(e.contains("not a regular file"), "{e}");
	}
}

#[cfg(any(test, feature = "test-support"))]
#[allow(dead_code)]
mod tests_impl {
	//! A tiny generated sequence-classification BERT (hidden 8, 2 heads, 1
	//! layer, vocabulary 16, 32 positions) with a word-level tokenizer and
	//! the pair template, so every contract refusal is tested without the
	//! pinned 91 MB download.
	pub(super) use super::*;
	pub(super) use serde_json::json;
	pub(super) use std::collections::HashMap;
	pub(super) use std::path::{Path, PathBuf};

	pub(super) fn fresh(tag: &str) -> PathBuf {
		static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		let d = std::env::temp_dir().join(format!("rnx-0146-ce-{}/{tag}-{n}", std::process::id()));
		std::fs::create_dir_all(&d).unwrap();
		d
	}

	pub(super) fn config() -> serde_json::Value {
		json!({
			"architectures": ["BertForSequenceClassification"], "id2label": {"0": "LABEL_0"},
			"sbert_ce_default_activation_function": IDENTITY,
			"model_type": "bert", "hidden_act": "gelu", "position_embedding_type": "absolute",
			"vocab_size": 16, "hidden_size": 8, "num_hidden_layers": 1, "num_attention_heads": 2,
			"intermediate_size": 16, "hidden_dropout_prob": 0.0, "max_position_embeddings": 32,
			"type_vocab_size": 2, "initializer_range": 0.02, "layer_norm_eps": 1e-12, "pad_token_id": 0
		})
	}

	pub(super) fn tokenizer() -> serde_json::Value {
		let words = [
			"the", "cat", "sat", "on", "mat", "a", "dog", "ran", "far", "away", "home", "now",
		];
		let mut v = serde_json::Map::new();
		for (i, t) in ["[PAD]", "[UNK]", "[CLS]", "[SEP]"].iter().enumerate() {
			v.insert(t.to_string(), json!(i));
		}
		for (i, w) in words.iter().enumerate() {
			v.insert(w.to_string(), json!(i + 4));
		}
		json!({
			"version": "1.0", "truncation": null, "padding": null, "added_tokens": [],
			"normalizer": null, "pre_tokenizer": {"type": "WhitespaceSplit"},
			"post_processor": {
				"type": "TemplateProcessing",
				"single": [{"SpecialToken": {"id": "[CLS]", "type_id": 0}}, {"Sequence": {"id": "A", "type_id": 0}}, {"SpecialToken": {"id": "[SEP]", "type_id": 0}}],
				"pair": pair_template(),
				"special_tokens": {
					"[CLS]": {"id": "[CLS]", "ids": [2], "tokens": ["[CLS]"]},
					"[SEP]": {"id": "[SEP]", "ids": [3], "tokens": ["[SEP]"]}
				}
			},
			"decoder": null,
			"model": {"type": "WordLevel", "vocab": serde_json::Value::Object(v), "unk_token": "[UNK]"}
		})
	}

	pub(super) fn write(d: &Path, name: &str, v: &serde_json::Value) {
		std::fs::write(d.join(name), v.to_string()).unwrap();
	}

	/// The weights: a BERT encoder under `bert.`, the pooler, the classifier
	/// and the integer position buffer; `edit` changes the tensor map.
	pub(super) fn weights(d: &Path, edit: impl FnOnce(&mut HashMap<String, CTensor>)) {
		let varmap = candle_nn::VarMap::new();
		let vb = VarBuilder::from_varmap(&varmap, DType::F32, &Device::Cpu);
		let typed: Config = serde_json::from_value(config()).unwrap();
		BertModel::load(vb.pp("bert"), &typed).unwrap();
		candle_nn::linear(8, 8, vb.pp("bert.pooler.dense")).unwrap();
		candle_nn::linear(8, 1, vb.pp("classifier")).unwrap();
		let mut map: HashMap<String, CTensor> = varmap
			.data()
			.lock()
			.unwrap()
			.iter()
			.map(|(k, v)| (k.clone(), v.as_tensor().clone()))
			.collect();
		map.insert(
			"bert.embeddings.position_ids".into(),
			CTensor::arange(0i64, 32, &Device::Cpu)
				.unwrap()
				.reshape((1, 32))
				.unwrap(),
		);
		edit(&mut map);
		candle_core::safetensors::save(&map, d.join("model.safetensors")).unwrap();
	}

	/// A complete model directory; the edits change one file each.
	pub(super) fn model(
		edit_config: impl FnOnce(&mut serde_json::Value),
		edit_tokenizer: impl FnOnce(&mut serde_json::Value),
		edit_weights: impl FnOnce(&mut HashMap<String, CTensor>),
	) -> PathBuf {
		let d = fresh("m");
		let mut c = config();
		edit_config(&mut c);
		write(&d, "config.json", &c);
		let mut t = tokenizer();
		edit_tokenizer(&mut t);
		write(&d, "tokenizer.json", &t);
		weights(&d, edit_weights);
		d
	}

	pub(super) fn good() -> PathBuf {
		model(|_| {}, |_| {}, |_| {})
	}

	pub(super) fn refused(d: PathBuf, want: &str) {
		let e = load(d.to_str().unwrap())
			.err()
			.unwrap_or_else(|| panic!("accepted (wanted {want})"));
		assert!(e.contains(want), "{e} (wanted {want})");
	}

	pub(super) fn list(v: &[&str]) -> rune::Value {
		rune::to_value(v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
	}
}
