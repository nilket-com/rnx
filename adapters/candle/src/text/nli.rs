//! Record 0148: `candle::NliModel`, a DeBERTa-v2 sequence-classification
//! model with three labels (cross-encoder/nli-deberta-v3-xsmall at its
//! pinned revision), scoring (premise, hypothesis) pairs. `score` returns
//! the raw logits as a `Dense` of N × 3 in `id2label` order (contradiction,
//! entailment, neutral); the script turns them into probabilities and
//! decisions.
//!
//! The configuration is closed (review round 1, R1): `config.json` must hold
//! exactly the production geometry below, or, only in a test-support build,
//! the tiny fixture's. Every other key is refused by name, except inert
//! metadata. All size arithmetic is checked before any tensor is read.
//!
//! Pairs share 0146's machinery: the pair-tokenizer contract, the borrowed
//! input checks, 0135's S1 partition with the single-pair exception, and
//! `embed`'s executor. The model takes no token types (`type_vocab_size` 0).
//! The memory estimate is DeBERTa's own (review round 1, R2), each term
//! cited to candle-transformers 0.11's `debertav2.rs`.
use super::rerank::{PairModel, pair_preflight, pair_tokenizer, plan_pairs, with_borrowed_pairs};
use super::{AGG, CAPS, Caps, Knobs, MAX_WEIGHTS, Planned, Trace, execute, json};
use crate::{read_limited, worker};
use candle_core::{DType, Device, Tensor as CTensor};
use candle_nn::VarBuilder;
use candle_transformers::models::debertav2::{Config, DebertaV2SeqClassificationModel};
use rnx::interchange::{Data, Dense};
use rnx::rune::{self, Any};
use std::sync::Arc;
use tokenizers::Tokenizer;

const ARCHITECTURE: &str = "DebertaV2ForSequenceClassification";
const IDENTITY: &str = "torch.nn.modules.linear.Identity";
/// The three labels, in `id2label` order.
pub const LABELS: [&str; 3] = ["contradiction", "entailment", "neutral"];

/// A closed geometry: the values that vary between the production model and
/// the test fixture. Everything else in the contract is the same for both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
	pub(crate) name: &'static str,
	pub(crate) hidden: usize,
	pub(crate) layers: usize,
	pub(crate) heads: usize,
	pub(crate) intermediate: usize,
	pub(crate) vocab: usize,
	pub(crate) max_positions: usize,
	pub(crate) buckets: usize,
	/// The tokenizer's exact size (ids 0 .. tokens − 1), below `vocab`.
	pub(crate) tokens: usize,
}

/// cross-encoder/nli-deberta-v3-xsmall at a150876415327c80daeff35ca6f68f5ed8cf5c24.
pub(crate) const PRODUCTION: Geometry = Geometry {
	name: "production",
	hidden: 384,
	layers: 12,
	heads: 6,
	intermediate: 1536,
	vocab: 128_100,
	max_positions: 512,
	buckets: 256,
	tokens: 128_001,
};

/// The unit tests' tiny generated model; admitted only by test builds.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const FIXTURE: Geometry = Geometry {
	name: "fixture",
	hidden: 16,
	layers: 2,
	heads: 2,
	intermediate: 32,
	vocab: 32,
	max_positions: 32,
	buckets: 8,
	tokens: 20,
};

fn admitted() -> Vec<Geometry> {
	#[cfg(any(test, feature = "test-support"))]
	return vec![PRODUCTION, FIXTURE];
	#[cfg(not(any(test, feature = "test-support")))]
	vec![PRODUCTION]
}

/// The model the executor runs.
pub(crate) struct Head {
	model: DebertaV2SeqClassificationModel,
}

struct Inner {
	head: Head,
	tokenizer: Tokenizer,
	geometry: Geometry,
}

/// A loaded NLI model. Cloning shares it.
#[derive(Any, Clone)]
#[rune(item = ::candle)]
pub struct NliModel(Arc<Inner>);

/// The closed contract. Returns Candle's typed config and the matched
/// geometry; refuses anything else by name.
fn contract(raw: &serde_json::Value, op: &str) -> Result<(Config, Geometry), String> {
	use serde_json::json;
	let o = raw
		.as_object()
		.ok_or_else(|| format!("{op}: config.json is not an object"))?;
	// refused by name: what the geometry doesn't admit, including the
	// convolution Candle leaves as `todo!()`
	for k in o.keys() {
		if k == "attention_head_size"
			|| k == "embedding_size"
			|| k.starts_with("conv_")
			|| k == "cls_dropout"
		{
			return Err(format!(
				"{op}: config.json key {k:?} is not admitted (closed DeBERTa geometry)"
			));
		}
	}
	// the same in every admitted geometry
	let fixed: [(&str, serde_json::Value); 16] = [
		("architectures", json!([ARCHITECTURE])),
		("model_type", json!("deberta-v2")),
		(
			"id2label",
			json!({"0": "contradiction", "1": "entailment", "2": "neutral"}),
		),
		(
			"label2id",
			json!({"contradiction": 0, "entailment": 1, "neutral": 2}),
		),
		("type_vocab_size", json!(0)),
		("relative_attention", json!(true)),
		("position_biased_input", json!(false)),
		("pos_att_type", json!(["p2c", "c2p"])),
		("share_att_key", json!(true)),
		("norm_rel_ebd", json!("layer_norm")),
		("hidden_act", json!("gelu")),
		("pooler_hidden_act", json!("gelu")),
		("pooler_dropout", json!(0)),
		("max_relative_positions", json!(-1)),
		("pad_token_id", json!(0)),
		("layer_norm_eps", json!(1e-7)),
	];
	for (k, want) in &fixed {
		let have = o.get(*k);
		let same = match (have, want) {
			(Some(h), w) if h.is_number() && w.is_number() => h.as_f64() == w.as_f64(),
			(Some(h), w) => h == w,
			(None, _) => false,
		};
		if !same {
			return Err(format!(
				"{op}: config.json {k} is {}, want {want}",
				have.map_or("absent".to_string(), |v| v.to_string())
			));
		}
	}
	// the sentence-transformers activation, if present, is the identity
	if let Some(a) = o.get("sbert_ce_default_activation_function")
		&& a != &json!(IDENTITY)
	{
		return Err(format!(
			"{op}: config.json sbert_ce_default_activation_function is {a}, want {IDENTITY:?} (the outputs are logits)"
		));
	}
	if let Some(t) = o.get("torch_dtype")
		&& t != &json!("float32")
	{
		return Err(format!(
			"{op}: config.json torch_dtype is {t}, want \"float32\""
		));
	}
	// the geometry: one admitted set, exactly
	let num = |k: &str| o.get(k).and_then(|v| v.as_u64()).map(|v| v as usize);
	let geometry = admitted().into_iter().find(|g| {
		num("hidden_size") == Some(g.hidden)
			&& num("num_hidden_layers") == Some(g.layers)
			&& num("num_attention_heads") == Some(g.heads)
			&& num("intermediate_size") == Some(g.intermediate)
			&& num("vocab_size") == Some(g.vocab)
			&& num("max_position_embeddings") == Some(g.max_positions)
			&& num("position_buckets") == Some(g.buckets)
			&& num("pooler_hidden_size") == Some(g.hidden)
	});
	let geometry = geometry.ok_or_else(|| {
		format!(
			"{op}: config.json's geometry (hidden_size, num_hidden_layers, num_attention_heads, intermediate_size, vocab_size, max_position_embeddings, position_buckets, pooler_hidden_size) is not the admitted one: {:?}",
			admitted().iter().map(|g| g.name).collect::<Vec<_>>()
		)
	})?;
	// every key accounted for: the fixed ones, the geometry's, the
	// activation, and inert metadata whose values are ignored
	let known: std::collections::BTreeSet<&str> = fixed
		.iter()
		.map(|(k, _)| *k)
		.chain([
			"hidden_size",
			"num_hidden_layers",
			"num_attention_heads",
			"intermediate_size",
			"vocab_size",
			"max_position_embeddings",
			"position_buckets",
			"pooler_hidden_size",
			"sbert_ce_default_activation_function",
			"torch_dtype",
			"_name_or_path",
			"transformers_version",
			"initializer_range",
			"hidden_dropout_prob",
			"attention_probs_dropout_prob",
		])
		.collect();
	if let Some(k) = o.keys().find(|k| !known.contains(k.as_str())) {
		return Err(format!(
			"{op}: config.json key {k:?} is not admitted (closed DeBERTa geometry)"
		));
	}
	if geometry.hidden % geometry.heads != 0 {
		return Err(format!("{op}: hidden_size is not a multiple of the heads"));
	}
	let config: Config =
		serde_json::from_value(raw.clone()).map_err(|e| format!("{op}: config.json: {e}"))?;
	Ok((config, geometry))
}

/// The exact weights, each with its shape: the encoder under `deberta.`,
/// the pooler, the classifier, and the unused integer position buffer.
fn expected(g: &Geometry) -> Option<Vec<(String, Vec<usize>, DType)>> {
	let (h, i) = (g.hidden, g.intermediate);
	let mut w: Vec<(String, Vec<usize>, DType)> = Vec::new();
	let mut f = |k: String, s: Vec<usize>| w.push((k, s, DType::F32));
	f(
		"deberta.embeddings.word_embeddings.weight".into(),
		vec![g.vocab, h],
	);
	f("deberta.embeddings.LayerNorm.weight".into(), vec![h]);
	f("deberta.embeddings.LayerNorm.bias".into(), vec![h]);
	f(
		"deberta.encoder.rel_embeddings.weight".into(),
		vec![g.buckets.checked_mul(2)?, h],
	);
	f("deberta.encoder.LayerNorm.weight".into(), vec![h]);
	f("deberta.encoder.LayerNorm.bias".into(), vec![h]);
	for l in 0..g.layers {
		let p = format!("deberta.encoder.layer.{l}");
		for k in ["query_proj", "key_proj", "value_proj"] {
			f(format!("{p}.attention.self.{k}.weight"), vec![h, h]);
			f(format!("{p}.attention.self.{k}.bias"), vec![h]);
		}
		f(format!("{p}.attention.output.dense.weight"), vec![h, h]);
		f(format!("{p}.attention.output.dense.bias"), vec![h]);
		f(format!("{p}.attention.output.LayerNorm.weight"), vec![h]);
		f(format!("{p}.attention.output.LayerNorm.bias"), vec![h]);
		f(format!("{p}.intermediate.dense.weight"), vec![i, h]);
		f(format!("{p}.intermediate.dense.bias"), vec![i]);
		f(format!("{p}.output.dense.weight"), vec![h, i]);
		f(format!("{p}.output.dense.bias"), vec![h]);
		f(format!("{p}.output.LayerNorm.weight"), vec![h]);
		f(format!("{p}.output.LayerNorm.bias"), vec![h]);
	}
	f("pooler.dense.weight".into(), vec![h, h]);
	f("pooler.dense.bias".into(), vec![h]);
	f("classifier.weight".into(), vec![3, h]);
	f("classifier.bias".into(), vec![3]);
	w.push((
		"deberta.embeddings.position_ids".into(),
		vec![1, g.max_positions],
		DType::I64,
	));
	Some(w)
}

/// The data section's exact size, from the expected weights, with checked
/// arithmetic; `None` on overflow.
fn expected_bytes(w: &[(String, Vec<usize>, DType)]) -> Option<usize> {
	w.iter().try_fold(0usize, |acc, (_, shape, dt)| {
		let n = shape.iter().try_fold(1usize, |a, &d| a.checked_mul(d))?;
		acc.checked_add(n.checked_mul(dt.size_in_bytes())?)
	})
}

fn load(dir: &str) -> Result<NliModel, String> {
	let op = "NliModel::load";
	let root = std::path::Path::new(dir);
	let raw = json(root, "config.json", op)?;
	let (config, geometry) = contract(&raw, op)?;
	let tokenizer = pair_tokenizer(root, geometry.vocab, 0, op)?;
	if tokenizer.get_vocab_size(true) != geometry.tokens {
		return Err(format!(
			"{op}: tokenizer.json has {} tokens, want exactly {} ({} geometry)",
			tokenizer.get_vocab_size(true),
			geometry.tokens,
			geometry.name
		));
	}
	for (name, id) in [("[PAD]", 0u32), ("[CLS]", 1), ("[SEP]", 2)] {
		if tokenizer.token_to_id(name) != Some(id) {
			return Err(format!(
				"{op}: tokenizer.json's {name} is {:?}, want id {id}",
				tokenizer.token_to_id(name)
			));
		}
	}
	// all arithmetic before any tensor is read: every weight's element and
	// byte count, and their sum against the file's data section
	let want = expected(&geometry)
		.ok_or_else(|| format!("{op}: the {} geometry overflows", geometry.name))?;
	let want_bytes = expected_bytes(&want)
		.ok_or_else(|| format!("{op}: the {} geometry's weights overflow", geometry.name))?;
	let weights = root.join("model.safetensors");
	let weights = weights
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(weights, MAX_WEIGHTS, op)?;
	let header = bytes
		.get(..8)
		.map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])) as usize)
		.ok_or_else(|| format!("{op}: model.safetensors is shorter than its header length"))?;
	let data = bytes
		.len()
		.checked_sub(8)
		.and_then(|v| v.checked_sub(header))
		.ok_or_else(|| format!("{op}: model.safetensors' header length exceeds the file"))?;
	if data != want_bytes {
		return Err(format!(
			"{op}: model.safetensors holds {data} bytes of tensors, want exactly {want_bytes} ({} geometry)",
			geometry.name
		));
	}
	let weights = weights.to_owned();
	let c = config.clone();
	let head = worker::run(op, move || -> Result<Head, String> {
		let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu)
			.map_err(|e| format!("{op} {weights:?}: {e}"))?;
		let have: std::collections::BTreeSet<&String> = tensors.keys().collect();
		let names: std::collections::BTreeSet<&String> = want.iter().map(|(k, _, _)| k).collect();
		if let Some(k) = have.difference(&names).next() {
			return Err(format!("{op}: unexpected tensor {k}"));
		}
		if let Some(k) = names.difference(&have).next() {
			return Err(format!("{op}: missing tensor {k}"));
		}
		for (k, shape, dt) in &want {
			let t = &tensors[k];
			if t.dtype() != *dt {
				return Err(format!("{op}: {k} is {:?}, want {dt:?}", t.dtype()));
			}
			if t.dims() != shape.as_slice() {
				return Err(format!("{op}: {k} is {:?}, want {shape:?}", t.dims()));
			}
		}
		// the unused buffer: exactly 0 .. max_position_embeddings − 1
		let n = c.max_position_embeddings;
		let ok = tensors["deberta.embeddings.position_ids"]
			.flatten_all()
			.and_then(|t| t.to_vec1::<i64>())
			.is_ok_and(|v| v.iter().enumerate().all(|(i, &x)| x == i as i64));
		if !ok {
			return Err(format!(
				"{op}: deberta.embeddings.position_ids must hold 0 to {}",
				n - 1
			));
		}
		let vb = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
		// the encoder under `deberta.`; Candle takes the pooler and the
		// classifier from the root
		let model = DebertaV2SeqClassificationModel::load(vb.pp("deberta"), &c, None)
			.map_err(|e| format!("{op} {weights:?}: {e}"))?;
		Ok(Head { model })
	})??;
	Ok(NliModel(Arc::new(Inner {
		head,
		tokenizer,
		geometry,
	})))
}

/// Bytes, as f32 values (rounded up): the executor's budget unit.
fn values(bytes: usize) -> Option<usize> {
	bytes.checked_add(3).map(|v| v / 4)
}

/// A pair batch's in-flight estimate, in f32 values, for `b` pairs padded to
/// `seq` tokens (review round 1, R2). Every tensor one encoder layer creates
/// is charged as if all were live at once (an upper bound on the layer's
/// live set), plus what persists between layers, the embedding stage and
/// the head. Line numbers are candle-transformers 0.11's `debertav2.rs`.
pub(crate) fn estimate_nli(g: &Geometry, b: usize, seq: usize) -> Option<usize> {
	let (h, i) = (g.hidden, g.intermediate);
	let span = g.buckets.checked_mul(2)?; // P: the relative span, 2 × buckets (L962–966)
	let bs = b.checked_mul(seq)?;
	let ss = seq.checked_mul(seq)?;
	let bh = b.checked_mul(g.heads)?; // B: batch × heads, the attention's leading dim
	let bhss = bh.checked_mul(ss)?;
	let sum = |terms: &[Option<usize>]| terms.iter().try_fold(0usize, |a, t| a.checked_add((*t)?));
	let bytes = sum(&[
		// inputs: ids and mask, u32 [b, s]
		bs.checked_mul(8),
		// persistent: the relative positions, built once through about 20
		// f32, i64 and u8 [s, s] intermediates (L1364–1446), charged at 160
		// bytes each element, and the i64 result
		ss.checked_mul(168),
		// persistent: the extended mask [b, 1, s, s] (L1063–1072), u32, and
		// the input mask per layer path
		b.checked_mul(ss)?.checked_mul(8),
		// persistent: the relative embeddings and their LayerNorm, [P, H]
		span.checked_mul(h)?.checked_mul(8),
		// persistent: the layer input and output [b, s, H]
		bs.checked_mul(h)?.checked_mul(8),
		// embeddings: words, LayerNorm and the masked product [b, s, H], and
		// the f32 mask [b, s, 1] (L197–279)
		bs.checked_mul(h)?.checked_mul(12),
		bs.checked_mul(8),
		// a layer, [b, s, H] tensors: q, k, v and their head-major copies
		// (L425–427, L521–530); the scaled key transpose (L444–448); the
		// context and its permuted copy (L491–507); the attention output's
		// dense, residual and LayerNorm; the output's dense, residual and
		// LayerNorm (L756–760, L817–823): 15, f32
		bs.checked_mul(h)?.checked_mul(60),
		// a layer, [b, s, I]: the intermediate dense and its GELU, f32
		bs.checked_mul(i)?.checked_mul(8),
		// a layer, [B, s, s] (L444–492, L540–697, XSoftmax L284–300): the
		// scores; c2p's gather, scaling and sum; p2c's gather, scaling and
		// sum; scores plus bias; XSoftmax's f32 mask, two where_conds and
		// softmax: 12 f32 = 48 bytes; c2p's i64 gather index 8; p2c's f32
		// index and its u32 copy 8; XSoftmax's two u8 masks 2: 66 bytes
		bhss.checked_mul(66),
		// a layer, [B, s, P]: c2p's and p2c's pre-gather products (L622, L676)
		bh.checked_mul(seq)?.checked_mul(span)?.checked_mul(8),
		// a layer, the relative key and query projections (L576–607): the
		// projected [P, H] and its head-major copy, each twice, and the
		// b-fold repeats [B, P, d] twice
		span.checked_mul(h)?.checked_mul(16),
		b.checked_mul(span)?.checked_mul(h)?.checked_mul(8),
		// a layer, c2p's and p2c's [s, s] position arithmetic (L624–626,
		// L670–673): two i64 and four f32
		ss.checked_mul(32),
		// the head: the first token, the pooler's dense and GELU [b, H], and
		// the logits [b, 3] (L1298–1356)
		b.checked_mul(h)?.checked_mul(12),
		b.checked_mul(12),
	])?;
	values(bytes)
}

/// The activation caps for DeBERTa: `embed`'s three, with the attention cap
/// also bounding the relative pre-gather products `B × s × P`.
pub(crate) fn fits_nli(g: &Geometry, b: usize, seq: usize, caps: Caps) -> bool {
	let within = |a: Option<usize>, cap: usize| a.is_some_and(|v| v <= cap);
	let bs = b.checked_mul(seq);
	let bh = b.checked_mul(g.heads);
	within(bs.and_then(|v| v.checked_mul(g.hidden)), caps.hidden)
		&& within(bs.and_then(|v| v.checked_mul(g.intermediate)), caps.ffn)
		&& within(
			bh.and_then(|v| v.checked_mul(seq))
				.and_then(|v| v.checked_mul(seq)),
			caps.attention,
		) && within(
		bh.and_then(|v| v.checked_mul(seq))
			.and_then(|v| v.checked_mul(g.buckets.checked_mul(2)?)),
		caps.attention,
	)
}

/// One pair batch: ids and mask, no token types; three logits per pair.
pub(crate) fn run_nli(head: &Head, p: &Planned, op: &str) -> Result<Vec<f32>, String> {
	let cpu = &Device::Cpu;
	let e = |e: candle_core::Error| format!("{op}: {e}");
	let input = CTensor::from_slice(&p.ids, (p.b, p.seq), cpu).map_err(e)?;
	let mask = CTensor::from_slice(&p.mask, (p.b, p.seq), cpu).map_err(e)?;
	let logits = head.model.forward(&input, None, Some(mask)).map_err(e)?;
	if logits.dims() != [p.b, 3] {
		return Err(format!(
			"{op}: logits are {:?}, want [{}, 3]",
			logits.dims(),
			p.b
		));
	}
	logits.flatten_all().map_err(e)?.to_vec1::<f32>().map_err(e)
}

/// Both stages inside the joined worker; the error is the lowest failing
/// batch across planning and execution. The logits are row-major N × 3.
pub fn score_strs(
	this: &NliModel,
	premises: &[&str],
	hypotheses: &[&str],
	caps: Caps,
	knobs: &Knobs,
) -> (Result<Vec<f32>, String>, Trace) {
	let op = "NliModel::score";
	let inner = &*this.0;
	let trace = Trace::default();
	if let Err(e) = pair_preflight(
		premises,
		hypotheses,
		[("premise", "premises"), ("hypothesis", "hypotheses")],
		op,
	) {
		return (Err(e), trace);
	}
	let pairs: Vec<(&str, &str)> = premises
		.iter()
		.copied()
		.zip(hypotheses.iter().copied())
		.collect();
	let agg = knobs.agg.unwrap_or(AGG);
	let workers = knobs
		.workers
		.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |p| p.get()));
	let g = inner.geometry;
	let ran = worker::run(op, || -> Result<(Vec<f32>, Trace), (String, Box<Trace>)> {
		let mut trace = Trace::default();
		let model = PairModel {
			tokenizer: &inner.tokenizer,
			max_seq: g.max_positions,
			pad: 0,
			vocab_size: g.vocab,
			type_vocab_size: None,
			estimate: &|b, seq| estimate_nli(&g, b, seq),
			fits: &|b, seq| fits_nli(&g, b, seq, caps),
		};
		let started = std::time::Instant::now();
		let (planned, plan_err) = plan_pairs(&model, &pairs, agg, knobs, op);
		trace.plan_s = started.elapsed().as_secs_f64();
		trace.batches = planned.iter().map(|p| p.b).collect();
		let rows: usize = planned.iter().map(|p| p.b).sum();
		let mut out = vec![0f32; rows * 3];
		let started = std::time::Instant::now();
		execute(
			&inner.head,
			run_nli,
			3,
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

fn score(this: &NliModel, premises: rune::Value, hypotheses: rune::Value) -> Result<Dense, String> {
	let op = "NliModel::score";
	with_borrowed_pairs(
		&premises,
		&hypotheses,
		[("premise", "premises"), ("hypothesis", "hypotheses")],
		op,
		|p, h| {
			let n = p.len();
			let logits = score_strs(this, p, h, CAPS, &Knobs::default()).0?;
			let names = LABELS.iter().map(|s| s.to_string()).collect();
			Dense::new(Data::F32(Arc::new(logits)), n, 3, names).map_err(|e| format!("{op}: {e}"))
		},
	)
}

fn labels(_this: &NliModel) -> Vec<String> {
	LABELS.iter().map(|s| s.to_string()).collect()
}

/// Test support: a loaded model from Rust.
pub fn load_dir(dir: &str) -> Result<NliModel, String> {
	load(dir)
}

/// Test support: the in-flight estimate (f32 values) of `b` pairs at `seq`
/// tokens, and whether the caps admit that batch.
pub fn estimate_for(this: &NliModel, b: usize, seq: usize) -> Option<usize> {
	estimate_nli(&this.0.geometry, b, seq)
}
pub fn fits_for(this: &NliModel, b: usize, seq: usize) -> bool {
	fits_nli(&this.0.geometry, b, seq, CAPS)
}

/// Test support (the calibration grid): the (batch, padded length) shapes
/// the S1 planner forms for these pairs under `knobs`, so every grid cell can
/// prove it ran the exact shape it asked for.
#[cfg(feature = "test-support")]
pub fn planned_shapes(
	this: &NliModel,
	premises: &[&str],
	hypotheses: &[&str],
	knobs: &Knobs,
) -> Result<Vec<(usize, usize)>, String> {
	let inner = &*this.0;
	let g = inner.geometry;
	let pairs: Vec<(&str, &str)> = premises
		.iter()
		.copied()
		.zip(hypotheses.iter().copied())
		.collect();
	let model = PairModel {
		tokenizer: &inner.tokenizer,
		max_seq: g.max_positions,
		pad: 0,
		vocab_size: g.vocab,
		type_vocab_size: None,
		estimate: &|b, seq| estimate_nli(&g, b, seq),
		fits: &|b, seq| fits_nli(&g, b, seq, CAPS),
	};
	let (planned, err) = plan_pairs(
		&model,
		&pairs,
		knobs.agg.unwrap_or(AGG),
		knobs,
		"planned_shapes",
	);
	match err {
		Some(e) => Err(e),
		None => Ok(planned.iter().map(|p| (p.b, p.seq)).collect()),
	}
}

pub(crate) fn build(
	m: &mut rune::Module,
) -> Result<Vec<(String, &'static str)>, rune::ContextError> {
	m.ty::<NliModel>()?;
	m.function("load", load).build_associated::<NliModel>()?;
	m.associated_function("score", score)?;
	m.associated_function("labels", labels)?;
	Ok(vec![
		(
			"candle::NliModel".into(),
			"NliModel: a DeBERTa-v2 NLI cross-encoder (contradiction, entailment, neutral logits) on the CPU",
		),
		(
			"candle::NliModel::load".into(),
			"load(dir) -> Result<NliModel>: config.json (DebertaV2ForSequenceClassification, the closed nli-deberta-v3-xsmall geometry), tokenizer.json ([CLS] A [SEP] B [SEP]) and model.safetensors",
		),
		(
			"candle::NliModel::score".into(),
			"score(premises, hypotheses) -> Result<Dense>: N x 3 logits (contradiction, entailment, neutral), one row per pair in order; a pair over 512 tokens is refused, never truncated",
		),
		(
			"candle::NliModel::labels".into(),
			"labels() -> Vec<String>: the logits' columns, [\"contradiction\", \"entailment\", \"neutral\"]",
		),
	])
}

/// Test support: the tiny generated model directory, for the allocation
/// controls (an integration test can't reach the unit tests).
#[cfg(feature = "test-support")]
pub fn fixture_dir() -> String {
	tests_impl::good().to_str().unwrap().to_string()
}

#[cfg(test)]
mod tests {
	use super::tests_impl::*;
	use super::*;
	use serde_json::json;

	#[test]
	fn the_fixture_scores_pairs_as_n_by_3_logits() {
		let nli = load(good().to_str().unwrap()).unwrap();
		let p = [
			"the cat sat on the mat",
			"a dog ran far away",
			"the cat ran home",
		];
		let h = ["a cat sat", "the dog sat", "a cat ran"];
		let (out, _) = score_strs(&nli, &p, &h, CAPS, &Knobs::default());
		let out = out.unwrap();
		assert_eq!(out.len(), 9);
		assert!(out.iter().all(|x| x.is_finite()));
		// the same pair alone matches its row in the padded batch
		let (alone, _) = score_strs(&nli, &p[..1], &h[..1], CAPS, &Knobs::default());
		let alone = alone.unwrap();
		for k in 0..3 {
			assert!((alone[k] - out[k]).abs() < 1e-5, "{alone:?} {out:?}");
		}
		// the script's view: a Dense N x 3 named by the labels
		let d = score(&nli, list(&p), list(&h)).unwrap();
		assert_eq!((d.rows(), d.columns()), (3, 3));
		assert_eq!(d.names(), LABELS);
		assert_eq!(labels(&nli), LABELS);
	}

	#[test]
	fn scoring_refuses_by_name() {
		let nli = load(good().to_str().unwrap()).unwrap();
		let e = score(&nli, list(&["the"]), list(&["a", "cat"])).unwrap_err();
		assert!(e.contains("1 premises and 2 hypotheses"), "{e}");
		let e = score(&nli, list(&[]), list(&[])).unwrap_err();
		assert!(e.contains("0 pairs"), "{e}");
		let e = score(&nli, rune::to_value(vec![1i64]).unwrap(), list(&["a"])).unwrap_err();
		assert!(e.contains("premise 0 is not a string"), "{e}");
		// 32 tokens (the fixture's positions) accepted; 33 refused by index
		let fits = "the ".repeat(32 - 5);
		assert!(score(&nli, list(&[fits.trim()]), list(&["a cat"])).is_ok());
		let over = "the ".repeat(32 - 4);
		let e = score(&nli, list(&["mat", over.trim()]), list(&["a cat", "a cat"])).unwrap_err();
		assert!(e.contains("pair 1 is 33 tokens, at most 32"), "{e}");
	}

	#[test]
	fn the_closed_contract_refuses_by_name() {
		let c = |k: &'static str, v: serde_json::Value, want: &str| {
			refused(model(move |c| c[k] = v, |_| {}, |_| {}), want)
		};
		c("architectures", json!(["DebertaV2Model"]), "architectures");
		c(
			"id2label",
			json!({"0": "entailment", "1": "contradiction", "2": "neutral"}),
			"id2label",
		);
		c(
			"label2id",
			json!({"contradiction": 1, "entailment": 0, "neutral": 2}),
			"label2id",
		);
		c("type_vocab_size", json!(2), "type_vocab_size");
		c("relative_attention", json!(false), "relative_attention");
		c(
			"position_biased_input",
			json!(true),
			"position_biased_input",
		);
		c("pos_att_type", json!(["c2p"]), "pos_att_type");
		c("share_att_key", json!(false), "share_att_key");
		c("norm_rel_ebd", json!("none"), "norm_rel_ebd");
		c("hidden_act", json!("relu"), "hidden_act");
		c("pooler_hidden_act", json!("tanh"), "pooler_hidden_act");
		c("layer_norm_eps", json!(1e-12), "layer_norm_eps");
		c(
			"max_relative_positions",
			json!(64),
			"max_relative_positions",
		);
		c("pad_token_id", json!(1), "pad_token_id");
		c("pooler_dropout", json!(0.1), "pooler_dropout");
		c("torch_dtype", json!("float16"), "torch_dtype");
		c(
			"sbert_ce_default_activation_function",
			json!("torch.nn.modules.activation.Sigmoid"),
			"want \"torch.nn.modules.linear.Identity\"",
		);
		// keys the closed geometry doesn't admit, by name
		for k in [
			"conv_kernel_size",
			"conv_act",
			"attention_head_size",
			"embedding_size",
			"cls_dropout",
			"z_steps",
		] {
			c(k, json!(3), &format!("key \"{k}\" is not admitted"));
		}
		// geometry outside the admitted sets
		c("num_hidden_layers", json!(3), "is not the admitted one");
		c("position_buckets", json!(16), "is not the admitted one");
		c("pooler_hidden_size", json!(8), "is not the admitted one");
		refused(
			model(
				|c| {
					c.as_object_mut().unwrap().remove("relative_attention");
				},
				|_| {},
				|_| {},
			),
			"relative_attention is absent",
		);
		// the tokenizer
		refused(
			model(|_| {}, |t| t["model"]["vocab"]["extra"] = json!(20), |_| {}),
			"has 21 tokens, want exactly 20",
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
					t["post_processor"]["special_tokens"]["[CLS]"] =
						json!({"id": "[CLS]", "ids": [1, 5], "tokens": ["[CLS]", "cat"]})
				},
				|_| {},
			),
			"[CLS] must be defined as exactly one token",
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
			"bytes of tensors",
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
			"bytes of tensors",
		);
		// the same byte count, a different key or shape
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					let t = m.remove("classifier.bias").unwrap();
					m.insert("classifier.other".into(), t);
				},
			),
			"tensor classifier",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					let t = m["classifier.weight"].reshape((16, 3)).unwrap();
					m.insert("classifier.weight".into(), t);
				},
			),
			"classifier.weight is [16, 3], want [3, 16]",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					// F16 is half the bytes: refused at the size check first
					let t = m["classifier.bias"].to_dtype(DType::F16).unwrap();
					m.insert("classifier.bias".into(), t);
				},
			),
			"bytes of tensors",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					// an I32 buffer of 64 values has the I64 buffer's byte count
					let t = CTensor::arange(0u32, 64, &Device::Cpu)
						.unwrap()
						.reshape((1, 64))
						.unwrap();
					m.insert("deberta.embeddings.position_ids".into(), t);
				},
			),
			"position_ids is",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.insert(
						"deberta.embeddings.position_ids".into(),
						CTensor::zeros((1, 32), DType::I64, &Device::Cpu).unwrap(),
					);
				},
			),
			"position_ids must hold 0 to 31",
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

	#[test]
	fn the_production_geometry_is_the_pinned_files_arithmetic() {
		// the pinned file's data section, measured: 283,328,524 bytes
		let w = expected(&PRODUCTION).unwrap();
		assert_eq!(w.len(), 203);
		assert_eq!(expected_bytes(&w), Some(283_328_524));
	}
}

#[cfg(any(test, feature = "test-support"))]
#[allow(dead_code)]
mod tests_impl {
	//! A tiny generated DeBERTa-v2 sequence classifier in the fixture
	//! geometry, with a word-level tokenizer and the pair template.
	pub(super) use super::*;
	pub(super) use serde_json::json;
	pub(super) use std::collections::HashMap;
	pub(super) use std::path::{Path, PathBuf};

	pub(super) fn fresh(tag: &str) -> PathBuf {
		static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		let d = std::env::temp_dir().join(format!("rnx-0148-nli-{}/{tag}-{n}", std::process::id()));
		std::fs::create_dir_all(&d).unwrap();
		d
	}

	pub(super) fn config() -> serde_json::Value {
		json!({
			"architectures": [ARCHITECTURE], "model_type": "deberta-v2",
			"id2label": {"0": "contradiction", "1": "entailment", "2": "neutral"},
			"label2id": {"contradiction": 0, "entailment": 1, "neutral": 2},
			"hidden_size": 16, "num_hidden_layers": 2, "num_attention_heads": 2,
			"intermediate_size": 32, "vocab_size": 32, "max_position_embeddings": 32,
			"position_buckets": 8, "max_relative_positions": -1, "type_vocab_size": 0,
			"relative_attention": true, "position_biased_input": false,
			"pos_att_type": ["p2c", "c2p"], "share_att_key": true, "norm_rel_ebd": "layer_norm",
			"hidden_act": "gelu", "layer_norm_eps": 1e-7, "pad_token_id": 0,
			"pooler_hidden_size": 16, "pooler_hidden_act": "gelu", "pooler_dropout": 0,
			"hidden_dropout_prob": 0.1, "attention_probs_dropout_prob": 0.1,
			"initializer_range": 0.02, "torch_dtype": "float32", "transformers_version": "4.11.3"
		})
	}

	pub(super) fn tokenizer() -> serde_json::Value {
		let words = [
			"the", "cat", "sat", "on", "mat", "a", "dog", "ran", "far", "away", "home", "now",
			"big", "red", "is", "not",
		];
		let mut v = serde_json::Map::new();
		for (i, t) in ["[PAD]", "[CLS]", "[SEP]", "[UNK]"].iter().enumerate() {
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
				"pair": [{"SpecialToken": {"id": "[CLS]", "type_id": 0}}, {"Sequence": {"id": "A", "type_id": 0}}, {"SpecialToken": {"id": "[SEP]", "type_id": 0}}, {"Sequence": {"id": "B", "type_id": 1}}, {"SpecialToken": {"id": "[SEP]", "type_id": 1}}],
				"special_tokens": {
					"[CLS]": {"id": "[CLS]", "ids": [1], "tokens": ["[CLS]"]},
					"[SEP]": {"id": "[SEP]", "ids": [2], "tokens": ["[SEP]"]}
				}
			},
			"decoder": null,
			"model": {"type": "WordLevel", "vocab": serde_json::Value::Object(v), "unk_token": "[UNK]"}
		})
	}

	pub(super) fn write(d: &Path, name: &str, v: &serde_json::Value) {
		std::fs::write(d.join(name), v.to_string()).unwrap();
	}

	/// The weights Candle's own loader reads for the fixture geometry, plus
	/// the integer position buffer; `edit` changes the tensor map.
	pub(super) fn weights(d: &Path, edit: impl FnOnce(&mut HashMap<String, CTensor>)) {
		let varmap = candle_nn::VarMap::new();
		let vb = VarBuilder::from_varmap(&varmap, DType::F32, &Device::Cpu);
		let typed: Config = serde_json::from_value(config()).unwrap();
		DebertaV2SeqClassificationModel::load(vb.pp("deberta"), &typed, None).unwrap();
		let mut map: HashMap<String, CTensor> = varmap
			.data()
			.lock()
			.unwrap()
			.iter()
			.map(|(k, v)| (k.clone(), v.as_tensor().clone()))
			.collect();
		map.insert(
			"deberta.embeddings.position_ids".into(),
			CTensor::arange(0i64, 32, &Device::Cpu)
				.unwrap()
				.reshape((1, 32))
				.unwrap(),
		);
		edit(&mut map);
		candle_core::safetensors::save(&map, d.join("model.safetensors")).unwrap();
	}

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
