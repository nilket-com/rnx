//! Record 0149: `candle::TextGenerator`, greedy text generation with a small
//! instruct model (Qwen/Qwen2.5-0.5B-Instruct at its pinned revision) on the
//! CPU. Deterministic: each step takes the argmax of the F32 logits, an
//! exact tie going to the lowest id; the model's own sampling defaults are
//! ignored.
//!
//! The configuration and tokenizer are closed (0148's pattern): exactly the
//! production geometry, or the tiny fixture's in a test-support build.
//! Weights are BF16 on disk and converted to F32 once, at load; the file's
//! data section is checked against the geometry before any tensor is read.
//!
//! Generations run through `embed`'s executor, generic over the output
//! record: each request is one planned item with its own checked estimate;
//! its row is a fixed reservation of `max_new_tokens + 1` records (a header
//! and the steps). The stored model carries no cache: a request clones it
//! only after admission, inside the run function, so the clone (and its KV
//! cache) is dropped when the run returns, before the executor drops the
//! permit, on success, error and unwind alike. Outputs are published only
//! when every request succeeds; otherwise the lowest-index error.
use super::{AGG, Knobs, MAX_TEXT, MAX_TOKENIZER, MAX_TOTAL, Planned, Trace, execute, json};
use crate::{read_limited, worker};
use candle_core::{DType, Device, Tensor as CTensor};
use candle_nn::VarBuilder;
use candle_transformers::models::qwen2::{Config, ModelForCausalLM};
use rnx::rune::{self, Any, runtime::Vec as RuneVec};
use std::sync::Arc;
use tokenizers::Tokenizer;

const ARCHITECTURE: &str = "Qwen2ForCausalLM";
/// The weights file bound for this model (the shared `MAX_WEIGHTS` is 512 MiB).
pub const MAX_GEN_WEIGHTS: u64 = 1 << 30;
/// Requests per call, the longest prompt, and the most new tokens.
pub const MAX_REQUESTS: usize = 1024;
pub const MAX_PROMPT: usize = 1024;
pub const MAX_NEW: usize = 256;
/// The planned prompt ids and per-request metadata, in bytes.
pub const GEN_PAYLOAD: usize = MAX_REQUESTS * MAX_PROMPT * 4 + MAX_REQUESTS * 64;
/// The longest token string, in UTF-8 bytes, any admitted tokenizer may hold.
pub const MAX_TOKEN_BYTES: usize = 256;
/// Review round 1 (a): the staged step records AND the results' ids and top
/// 5, which coexist, in bytes, at the largest admitted call; checked before
/// execution.
pub const GEN_OUTPUT: usize = MAX_REQUESTS * (2 * MAX_NEW + 1) * std::mem::size_of::<Step>();
/// Review round 1 (a): the decoded texts, in bytes, summed over a call's
/// requests by their per-request bounds; checked after generation, before
/// any text is allocated.
pub const GEN_TEXT: usize = 64 << 20;

/// One generated step: the chosen id and the top 5 (id, logit) in rank
/// order. A row's first record is its header: `id` the number of generated
/// ids, `top[0].0` the stop (1 EOS, 2 length).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Step {
	pub id: u32,
	pub top: [(u32, f32); 5],
}

/// A closed geometry: the values that vary between production and fixture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
	pub(crate) name: &'static str,
	pub(crate) hidden: usize,
	pub(crate) layers: usize,
	pub(crate) heads: usize,
	pub(crate) kv_heads: usize,
	pub(crate) intermediate: usize,
	pub(crate) vocab: usize,
	/// The tokenizer's exact size, below `vocab`.
	pub(crate) tokens: usize,
	pub(crate) max_positions: usize,
	/// `<|endoftext|>`, `<|im_start|>`, `<|im_end|>`.
	pub(crate) specials: [u32; 3],
}

/// Qwen/Qwen2.5-0.5B-Instruct at 7ae557604adf67be50417f59c2c2f167def9a775.
pub(crate) const PRODUCTION: Geometry = Geometry {
	name: "production",
	hidden: 896,
	layers: 24,
	heads: 14,
	kv_heads: 2,
	intermediate: 4864,
	vocab: 151_936,
	tokens: 151_665,
	max_positions: 32_768,
	specials: [151_643, 151_644, 151_645],
};

#[cfg(any(test, feature = "test-support"))]
pub(crate) const FIXTURE: Geometry = Geometry {
	name: "fixture",
	hidden: 16,
	layers: 2,
	heads: 2,
	kv_heads: 1,
	intermediate: 32,
	vocab: 40,
	tokens: 40,
	max_positions: 32_768,
	specials: [37, 38, 39],
};

fn admitted() -> Vec<Geometry> {
	#[cfg(any(test, feature = "test-support"))]
	return vec![PRODUCTION, FIXTURE];
	#[cfg(not(any(test, feature = "test-support")))]
	vec![PRODUCTION]
}

struct Inner {
	model: ModelForCausalLM,
	tokenizer: Tokenizer,
	geometry: Geometry,
	/// Each id's token string length in UTF-8 bytes (the decoded-output
	/// bound's per-token term).
	token_bytes: Vec<u16>,
}

/// A loaded generator. Cloning shares it; the stored model never holds a
/// cache.
#[derive(Any, Clone)]
#[rune(item = ::candle)]
pub struct TextGenerator(Arc<Inner>);

fn contract(raw: &serde_json::Value, op: &str) -> Result<(Config, Geometry), String> {
	use serde_json::json;
	let o = raw
		.as_object()
		.ok_or_else(|| format!("{op}: config.json is not an object"))?;
	for k in ["rope_scaling", "quantization_config", "quantization"] {
		if o.contains_key(k) {
			return Err(format!(
				"{op}: config.json key {k:?} is not admitted (closed Qwen2 geometry)"
			));
		}
	}
	let fixed: [(&str, serde_json::Value); 12] = [
		("architectures", json!([ARCHITECTURE])),
		("model_type", json!("qwen2")),
		("hidden_act", json!("silu")),
		("rms_norm_eps", json!(1e-6)),
		("rope_theta", json!(1_000_000.0)),
		("tie_word_embeddings", json!(true)),
		// the pinned inactive sliding window, exactly (review round 1)
		("use_sliding_window", json!(false)),
		("sliding_window", json!(32768)),
		("max_window_layers", json!(21)),
		("attention_dropout", json!(0.0)),
		("torch_dtype", json!("bfloat16")),
		("use_cache", json!(true)),
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
	let num = |k: &str| o.get(k).and_then(|v| v.as_u64()).map(|v| v as usize);
	let geometry = admitted()
		.into_iter()
		.find(|g| {
			num("hidden_size") == Some(g.hidden)
				&& num("num_hidden_layers") == Some(g.layers)
				&& num("num_attention_heads") == Some(g.heads)
				&& num("num_key_value_heads") == Some(g.kv_heads)
				&& num("intermediate_size") == Some(g.intermediate)
				&& num("vocab_size") == Some(g.vocab)
				&& num("max_position_embeddings") == Some(g.max_positions)
				&& num("bos_token_id") == Some(g.specials[0] as usize)
				&& num("eos_token_id") == Some(g.specials[2] as usize)
		})
		.ok_or_else(|| {
			format!(
				"{op}: config.json's geometry (hidden_size, num_hidden_layers, num_attention_heads, num_key_value_heads, intermediate_size, vocab_size, max_position_embeddings, bos_token_id, eos_token_id) is not the admitted one: {:?}",
				admitted().iter().map(|g| g.name).collect::<Vec<_>>()
			)
		})?;
	let known: std::collections::BTreeSet<&str> = fixed
		.iter()
		.map(|(k, _)| *k)
		.chain([
			"hidden_size",
			"num_hidden_layers",
			"num_attention_heads",
			"num_key_value_heads",
			"intermediate_size",
			"vocab_size",
			"max_position_embeddings",
			"bos_token_id",
			"eos_token_id",
			"_name_or_path",
			"transformers_version",
			"initializer_range",
		])
		.collect();
	if let Some(k) = o.keys().find(|k| !known.contains(k.as_str())) {
		return Err(format!(
			"{op}: config.json key {k:?} is not admitted (closed Qwen2 geometry)"
		));
	}
	if geometry.hidden % geometry.heads != 0 || geometry.heads % geometry.kv_heads != 0 {
		return Err(format!("{op}: the heads don't divide the geometry"));
	}
	let config: Config =
		serde_json::from_value(raw.clone()).map_err(|e| format!("{op}: config.json: {e}"))?;
	Ok((config, geometry))
}

/// The tokenizer: its exact size, every id below `vocab_size`, the three
/// special tokens defined as exactly themselves at their ids, a ByteLevel
/// decoder and post-processor (which add no tokens), truncation and padding
/// disabled; and each id's token-string length for the output bound.
fn tokenizer(
	dir: &std::path::Path,
	g: &Geometry,
	op: &str,
) -> Result<(Tokenizer, Vec<u16>), String> {
	let path = dir.join("tokenizer.json");
	let path = path
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(path, MAX_TOKENIZER, op)?;
	let raw: serde_json::Value =
		serde_json::from_slice(&bytes).map_err(|e| format!("{op} {path:?}: {e}"))?;
	let byte_level = serde_json::json!({"type": "ByteLevel", "add_prefix_space": false, "trim_offsets": false, "use_regex": false});
	for k in ["decoder", "post_processor"] {
		if raw.get(k) != Some(&byte_level) {
			return Err(format!(
				"{op}: tokenizer.json's {k} must be exactly {byte_level} (it adds no tokens; the chat template supplies them)"
			));
		}
	}
	let mut t = Tokenizer::from_bytes(&bytes).map_err(|e| format!("{op} {path:?}: {e}"))?;
	if t.get_vocab_size(true) != g.tokens {
		return Err(format!(
			"{op}: tokenizer.json has {} tokens, want exactly {} ({} geometry)",
			t.get_vocab_size(true),
			g.tokens,
			g.name
		));
	}
	let vocab = t.get_vocab(true);
	if let Some((token, id)) = vocab.iter().find(|(_, id)| **id as usize >= g.vocab) {
		return Err(format!(
			"{op}: tokenizer id {id} ({token:?}) is not below vocab_size {}",
			g.vocab
		));
	}
	// review round 1, R1: the ids are exactly the dense range 0 .. tokens:
	// none at or past the tokenizer's size, none shared, no hole
	if vocab.len() != g.tokens {
		return Err(format!(
			"{op}: tokenizer.json has {} entries, want exactly {}",
			vocab.len(),
			g.tokens
		));
	}
	if let Some((token, id)) = vocab.iter().find(|(_, id)| **id as usize >= g.tokens) {
		return Err(format!(
			"{op}: tokenizer id {id} ({token:?}) is not below the tokenizer's size {} (the ids must be dense)",
			g.tokens
		));
	}
	let mut seen = vec![false; g.tokens];
	for id in vocab.values() {
		let slot = &mut seen[*id as usize];
		if *slot {
			return Err(format!(
				"{op}: tokenizer id {id} is shared by two tokens (the ids must be dense)"
			));
		}
		*slot = true;
	}
	if let Some(hole) = seen.iter().position(|s| !s) {
		return Err(format!(
			"{op}: tokenizer id {hole} has no token (the ids must be dense)"
		));
	}
	let added = raw
		.get("added_tokens")
		.and_then(|v| v.as_array())
		.cloned()
		.unwrap_or_default();
	for (name, id) in ["<|endoftext|>", "<|im_start|>", "<|im_end|>"]
		.iter()
		.zip(g.specials)
	{
		let def = added
			.iter()
			.find(|a| a.get("content").and_then(|c| c.as_str()) == Some(name));
		let ok = t.token_to_id(name) == Some(id)
			&& def.is_some_and(|d| {
				d.get("id").and_then(|v| v.as_u64()) == Some(id as u64)
					&& d.get("special") == Some(&serde_json::json!(true))
			});
		if !ok {
			return Err(format!(
				"{op}: tokenizer.json's {name} must be the special token {id}"
			));
		}
	}
	t.with_truncation(None)
		.map_err(|e| format!("{op}: truncation: {e}"))?;
	t.with_padding(None);
	// behaviourally: special tokens in text are recognised, and encoding
	// adds nothing
	let probe = t
		.encode("<|im_start|>a<|im_end|>", true)
		.map_err(|e| format!("{op}: {e}"))?;
	let ids = probe.get_ids();
	if ids.first() != Some(&g.specials[1]) || ids.last() != Some(&g.specials[2]) {
		return Err(format!(
			"{op}: tokenizer.json does not recognise <|im_start|> and <|im_end|> in text, or adds tokens"
		));
	}
	let mut token_bytes = vec![0u16; g.tokens];
	for (token, id) in &vocab {
		let n = token.len();
		if n > MAX_TOKEN_BYTES {
			return Err(format!(
				"{op}: token {id} is {n} bytes, at most {MAX_TOKEN_BYTES}"
			));
		}
		*token_bytes
			.get_mut(*id as usize)
			.ok_or_else(|| format!("{op}: tokenizer id {id} is out of range"))? = n as u16;
	}
	Ok((t, token_bytes))
}

/// The exact weights and shapes, all BF16; no `lm_head` (tied).
fn expected(g: &Geometry) -> Option<Vec<(String, Vec<usize>)>> {
	let (h, i) = (g.hidden, g.intermediate);
	let kv = g.kv_heads.checked_mul(h / g.heads)?;
	let mut w = vec![("model.embed_tokens.weight".to_string(), vec![g.vocab, h])];
	for l in 0..g.layers {
		let p = format!("model.layers.{l}");
		for (k, s) in [
			("input_layernorm.weight", vec![h]),
			("post_attention_layernorm.weight", vec![h]),
			("self_attn.q_proj.weight", vec![h, h]),
			("self_attn.q_proj.bias", vec![h]),
			("self_attn.k_proj.weight", vec![kv, h]),
			("self_attn.k_proj.bias", vec![kv]),
			("self_attn.v_proj.weight", vec![kv, h]),
			("self_attn.v_proj.bias", vec![kv]),
			("self_attn.o_proj.weight", vec![h, h]),
			("mlp.gate_proj.weight", vec![i, h]),
			("mlp.up_proj.weight", vec![i, h]),
			("mlp.down_proj.weight", vec![h, i]),
		] {
			w.push((format!("{p}.{k}"), s));
		}
	}
	w.push(("model.norm.weight".to_string(), vec![h]));
	Some(w)
}

fn expected_bytes(w: &[(String, Vec<usize>)]) -> Option<usize> {
	w.iter().try_fold(0usize, |acc, (_, shape)| {
		let n = shape.iter().try_fold(1usize, |a, &d| a.checked_mul(d))?;
		acc.checked_add(n.checked_mul(2)?)
	})
}

fn load(dir: &str) -> Result<TextGenerator, String> {
	let op = "TextGenerator::load";
	let root = std::path::Path::new(dir);
	let raw = json(root, "config.json", op)?;
	let (config, geometry) = contract(&raw, op)?;
	let (tokenizer, token_bytes) = tokenizer(root, &geometry, op)?;
	let want = expected(&geometry)
		.ok_or_else(|| format!("{op}: the {} geometry overflows", geometry.name))?;
	let want_bytes = expected_bytes(&want)
		.ok_or_else(|| format!("{op}: the {} geometry's weights overflow", geometry.name))?;
	let weights = root.join("model.safetensors");
	let weights = weights
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(weights, MAX_GEN_WEIGHTS, op)?;
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
	let model = worker::run(op, move || -> Result<ModelForCausalLM, String> {
		let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu)
			.map_err(|e| format!("{op} {weights:?}: {e}"))?;
		drop(bytes);
		let have: std::collections::BTreeSet<&String> = tensors.keys().collect();
		let names: std::collections::BTreeSet<&String> = want.iter().map(|(k, _)| k).collect();
		if let Some(k) = have.difference(&names).next() {
			return Err(format!("{op}: unexpected tensor {k}"));
		}
		if let Some(k) = names.difference(&have).next() {
			return Err(format!("{op}: missing tensor {k}"));
		}
		for (k, shape) in &want {
			let t = &tensors[k];
			if t.dtype() != DType::BF16 {
				return Err(format!("{op}: {k} is {:?}, want BF16", t.dtype()));
			}
			if t.dims() != shape.as_slice() {
				return Err(format!("{op}: {k} is {:?}, want {shape:?}", t.dims()));
			}
		}
		// converted to F32 once, as each tensor is read
		let vb = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
		ModelForCausalLM::new(&c, vb).map_err(|e| format!("{op} {weights:?}: {e}"))
	})??;
	Ok(TextGenerator(Arc::new(Inner {
		model,
		tokenizer,
		geometry,
		token_bytes,
	})))
}

/// The chat template for one system and one user message, with the
/// generation prompt (checked against `apply_chat_template` by the gates).
pub fn prompt(system: &str, user: &str) -> String {
	format!(
		"<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
	)
}

/// Bytes, as f32 values (rounded up): the executor's budget unit.
fn values(bytes: usize) -> Option<usize> {
	bytes.checked_add(3).map(|v| v / 4)
}

/// A request's in-flight estimate, in f32 values, for a prompt of `p` tokens
/// and `m` new tokens (review round 1, R2). Line numbers are
/// candle-transformers 0.11's `qwen2.rs`.
pub(crate) fn estimate_gen(g: &Geometry, p: usize, m: usize) -> Option<usize> {
	let (h, i, v) = (g.hidden, g.intermediate, g.vocab);
	let d = h / g.heads;
	let kv = g.kv_heads.checked_mul(d)?;
	let t = p.checked_add(m)?;
	let pp = p.checked_mul(p)?;
	let sum = |terms: &[Option<usize>]| terms.iter().try_fold(0usize, |a, x| a.checked_add((*x)?));
	let bytes = sum(&[
		// prefill, one layer all-live (L170–230): [p, H] tensors: the input
		// norm, q, q head-major, RoPE q, the context, its transposed copy,
		// o_proj, the residual, the post norm, down, the residual, and the
		// two repeated key-value heads at prefill ([14, p, 64] each): 13
		p.checked_mul(h)?.checked_mul(13 * 4),
		// [p, kv] tensors: k, v, their head-major copies, RoPE k: 5
		p.checked_mul(kv)?.checked_mul(5 * 4),
		// [14, p, p]: the scores, the masked sum, the softmax (L196–206): 3;
		// the causal mask built on the host, as a tensor, and its cast (L303–323): 3 [p, p]
		g.heads.checked_mul(pp)?.checked_mul(3 * 4),
		pp.checked_mul(3 * 4),
		// the MLP [p, I]: gate, up, the activation, the product (L52–66)
		p.checked_mul(i)?.checked_mul(4 * 4),
		// the embedding lookup and the final norm [p, H]
		p.checked_mul(h)?.checked_mul(2 * 4),
		// the logits [1, 1, V] and the host copy, each step (L391–397)
		v.checked_mul(2 * 4),
		// the KV cache at T, old and new buffers both live during
		// Tensor::cat (L184–185): layers × (k, v) × 2 × [kv, T, d]
		g.layers
			.checked_mul(2 * 2)?
			.checked_mul(t)?
			.checked_mul(kv)?
			.checked_mul(4),
		// the repeated key-value heads' contiguous copies at T (L191–193):
		// one layer's, [14, T, 64] twice
		t.checked_mul(h)?.checked_mul(2 * 4),
		// a decode step at T: the scores, sum and softmax [14, 1, T]
		g.heads.checked_mul(t)?.checked_mul(3 * 4),
		// the clone's handles and bookkeeping
		Some(1 << 16),
	])?;
	values(bytes)
}

/// The per-call parameters the executor's model slot carries.
pub(crate) struct Call<'a> {
	inner: &'a Inner,
	max_new: usize,
	force_length: bool,
	/// The planned requests, so a run finds its own index (for the
	/// mid-decode injections only).
	planned: &'a [Planned],
	fail_at_step: Option<(usize, usize)>,
	panic_at_step: Option<(usize, usize)>,
}

/// The top 5 of `logits` by (logit descending, id ascending).
fn top5(logits: &[f32]) -> [(u32, f32); 5] {
	let mut top = [(u32::MAX, f32::NEG_INFINITY); 5];
	let mut n = 0usize;
	for (id, &x) in logits.iter().enumerate() {
		let id = id as u32;
		let better = |a: (u32, f32)| x > a.1 || (x == a.1 && id < a.0);
		if n < 5 {
			let mut k = n;
			while k > 0 && better(top[k - 1]) {
				top[k] = top[k - 1];
				k -= 1;
			}
			top[k] = (id, x);
			n += 1;
		} else if better(top[4]) {
			let mut k = 4;
			while k > 0 && better(top[k - 1]) {
				top[k] = top[k - 1];
				k -= 1;
			}
			top[k] = (id, x);
		}
	}
	top
}

/// The greedy loop, over a step function (so its policy is tested on exact
/// synthetic logits): `first` is the prefill's logits; `next(token, k)`
/// feeds the token chosen at step `k` and returns the following logits.
/// Records each step's id and top 5; stops after an EOS (included) unless
/// `force_length`, or after `max_new` ids. The row's header holds the count
/// and the stop (1 EOS, 2 length).
pub(crate) fn decode(
	max_new: usize,
	force_length: bool,
	eos: [u32; 2],
	vocab: usize,
	first: Vec<f32>,
	mut next: impl FnMut(u32, usize) -> Result<Vec<f32>, String>,
	op: &str,
) -> Result<Vec<Step>, String> {
	let mut out = vec![Step::default(); max_new + 1];
	let mut logits = first;
	let mut count = 0;
	let mut stop = 2u32;
	for k in 0..max_new {
		if logits.len() != vocab {
			return Err(format!(
				"{op}: {} logits at step {k}, want {vocab}",
				logits.len()
			));
		}
		if let Some(j) = logits.iter().position(|x| !x.is_finite()) {
			return Err(format!("{op}: a non-finite logit at step {k}, id {j}"));
		}
		let top = top5(&logits);
		let tok = top[0].0;
		out[1 + k] = Step { id: tok, top };
		count = k + 1;
		if eos.contains(&tok) && !force_length {
			stop = 1;
			break;
		}
		if k + 1 == max_new {
			break;
		}
		logits = next(tok, k)?;
	}
	out[0] = Step {
		id: count as u32,
		top: [(stop, 0.0); 5],
	};
	Ok(out)
}

/// One request: clone the cache-free model (after admission, here), prefill
/// at offset 0, then greedy decode, advancing by each step's input length.
/// The clone and its cache are dropped when this returns, before the
/// executor drops the permit.
pub(crate) fn run_gen(call: &Call, p: &Planned, op: &str) -> Result<Vec<Step>, String> {
	let cpu = &Device::Cpu;
	let e = |e: candle_core::Error| format!("{op}: {e}");
	let g = &call.inner.geometry;
	let mut model = call.inner.model.clone();
	let flat = |t: CTensor| -> Result<Vec<f32>, String> {
		if t.dims() != [1, 1, g.vocab] {
			return Err(format!(
				"{op}: logits are {:?}, want [1, 1, {}]",
				t.dims(),
				g.vocab
			));
		}
		t.flatten_all().map_err(e)?.to_vec1().map_err(e)
	};
	let input = CTensor::new(p.ids.as_slice(), cpu)
		.map_err(e)?
		.unsqueeze(0)
		.map_err(e)?;
	let first = flat(model.forward(&input, 0).map_err(e)?)?;
	let plen = p.ids.len();
	let me = call.planned.iter().position(|q| std::ptr::eq(q, p));
	decode(
		call.max_new,
		call.force_length,
		[g.specials[2], g.specials[0]],
		g.vocab,
		first,
		|tok, k| {
			let logits = {
				let next = CTensor::new(&[tok], cpu)
					.map_err(e)?
					.unsqueeze(0)
					.map_err(e)?;
				flat(model.forward(&next, plen + k).map_err(e)?)?
			};
			// mid-decode injections (review round 1, b): after step k ran, so
			// the clone's KV cache exists
			if call
				.fail_at_step
				.is_some_and(|(r, s)| Some(r) == me && s == k)
			{
				return Err(format!(
					"{op}: injected failure in request {} after decode step {k}",
					me.unwrap_or(0)
				));
			}
			if call
				.panic_at_step
				.is_some_and(|(r, s)| Some(r) == me && s == k)
			{
				panic!(
					"injected panic in request {} after decode step {k}",
					me.unwrap_or(0)
				);
			}
			Ok(logits)
		},
		op,
	)
}

/// One finished generation, as the script sees it.
#[derive(Clone, Debug)]
pub struct Generation {
	pub text: String,
	/// Including the terminal EOS when it stopped by EOS.
	pub ids: Vec<u32>,
	pub prompt_tokens: usize,
	/// "eos" or "length".
	pub stop: &'static str,
	pub top: Vec<[(u32, f32); 5]>,
}

/// Both stages inside the joined worker. Checked in order (review round 1,
/// R2): the count, each text and the combined bytes; each prompt's ids (one
/// at a time) and the planned id storage; the staged outputs. Then the
/// requests run; each output's decoded-text bound is checked before its
/// text is allocated.
pub fn chat_strs(
	this: &TextGenerator,
	systems: &[&str],
	users: &[&str],
	max_new: usize,
	knobs: &Knobs,
) -> (Result<Vec<Generation>, String>, Trace) {
	let op = "TextGenerator::chat";
	let inner = &*this.0;
	let trace = Trace::default();
	let n = systems.len();
	let check = || -> Result<(), String> {
		if users.len() != n {
			return Err(format!(
				"{op}: {n} systems and {} users; one chat per index",
				users.len()
			));
		}
		if n == 0 || n > MAX_REQUESTS {
			return Err(format!("{op}: {n} chats, want 1 to {MAX_REQUESTS}"));
		}
		if max_new == 0 || max_new > MAX_NEW {
			return Err(format!(
				"{op}: max_new_tokens {max_new}, want 1 to {MAX_NEW}"
			));
		}
		let mut total = 0usize;
		for (side, texts) in [("system", systems), ("user", users)] {
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
	};
	if let Err(e) = check() {
		return (Err(e), trace);
	}
	let agg = knobs.agg.unwrap_or(AGG);
	let workers = knobs
		.workers
		.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |p| p.get()));
	let g = inner.geometry;
	let ran = worker::run(
		op,
		|| -> Result<(Vec<Generation>, Trace), (String, Box<Trace>)> {
			let mut trace = Trace::default();
			let fail = |e: String, t: Trace| Err((e, Box::new(t)));
			// the prompts, one at a time, against the bounds and the payload
			let mut planned = Vec::with_capacity(n);
			let mut payload = 0usize;
			for i in 0..n {
				let text = prompt(systems[i], users[i]);
				let enc = match inner.tokenizer.encode(text, false) {
					Ok(e) => e,
					Err(err) => return fail(format!("{op}: chat {i}: {err}"), trace),
				};
				let ids = enc.get_ids();
				if ids.len() > MAX_PROMPT {
					return fail(
						format!(
							"{op}: chat {i}'s prompt is {} tokens, at most {MAX_PROMPT} (never truncated)",
							ids.len()
						),
						trace,
					);
				}
				if let Some(&id) = ids.iter().find(|&&id| id as usize >= g.vocab) {
					return fail(
						format!(
							"{op}: chat {i}: token id {id} is not below vocab_size {}",
							g.vocab
						),
						trace,
					);
				}
				payload = match payload
					.checked_add(ids.len() * 4 + 64)
					.filter(|v| *v <= GEN_PAYLOAD)
				{
					Some(v) => v,
					None => {
						return fail(
							format!("{op}: the planned prompts exceed {GEN_PAYLOAD} bytes"),
							trace,
						);
					}
				};
				let est = match estimate_gen(&g, ids.len(), max_new) {
					Some(e) if e <= agg => e,
					_ => {
						return fail(
							format!(
								"{op}: chat {i} ({} prompt tokens, {max_new} new) exceeds the in-flight budget",
								ids.len()
							),
							trace,
						);
					}
				};
				planned.push(Planned {
					b: 1,
					seq: ids.len(),
					ids: ids.to_vec(),
					types: None,
					mask: Vec::new(),
					estimate: est,
				});
			}
			// the staged records and the results' ids and top 5 (they coexist),
			// checked before any request runs (review round 1, a)
			let width = max_new + 1;
			let staged = n
				.checked_mul(width + max_new)
				.and_then(|v| v.checked_mul(std::mem::size_of::<Step>()));
			if staged.is_none_or(|v| v > GEN_OUTPUT) {
				return fail(
					format!("{op}: the staged outputs and results exceed {GEN_OUTPUT} bytes"),
					trace,
				);
			}
			let mut out = vec![Step::default(); n * width];
			let call = Call {
				inner,
				max_new,
				force_length: knobs.force_length,
				planned: &planned,
				fail_at_step: knobs.fail_at_step,
				panic_at_step: knobs.panic_at_step,
			};
			trace.batches = vec![1; n];
			let started = std::time::Instant::now();
			execute(
				&call,
				run_gen,
				width,
				&planned,
				&mut out,
				(workers, agg),
				knobs,
				&mut trace,
				op,
			);
			trace.exec_s = started.elapsed().as_secs_f64();
			if let Some((_, e)) = trace.failed.first() {
				return fail(e.clone(), trace);
			}
			// first, every record checked and every text's bound computed, and
			// their sum checked against GEN_TEXT, before any text is allocated
			let mut parts = Vec::with_capacity(n);
			let mut texts = 0usize;
			for i in 0..n {
				let row = &out[i * width..(i + 1) * width];
				let count = row[0].id as usize;
				let stop = row[0].top[0].0;
				if count == 0 || count > max_new || !(stop == 1 || stop == 2) {
					return fail(
						format!("{op}: chat {i}: an inconsistent generation record"),
						trace,
					);
				}
				let ids: Vec<u32> = row[1..=count].iter().map(|s| s.id).collect();
				// the model's vocabulary is padded past the tokenizer's: a padded
				// id has no token, and is refused by name, never dropped
				if let Some(&id) = ids.iter().find(|&&id| id as usize >= g.tokens) {
					return fail(
						format!(
							"{op}: chat {i} generated id {id}, which has no token (the tokenizer has {})",
							g.tokens
						),
						trace,
					);
				}
				let body = if stop == 1 { count - 1 } else { count };
				// the ByteLevel decoder maps each token string's characters to
				// one byte each, then decodes UTF-8 lossily (at most 3 bytes per
				// invalid byte): 3 × the token strings' UTF-8 bytes bounds it
				let bound = ids[..body].iter().try_fold(0usize, |a, &id| {
					a.checked_add(3 * *inner.token_bytes.get(id as usize)? as usize)
				});
				let Some(bound) = bound.filter(|b| *b <= max_new * 3 * MAX_TOKEN_BYTES) else {
					return fail(
						format!("{op}: chat {i}: the decoded text's bound is out of range"),
						trace,
					);
				};
				texts = match texts.checked_add(bound).filter(|t| *t <= GEN_TEXT) {
					Some(t) => t,
					None => {
						return fail(
							format!("{op}: the decoded texts' bound exceeds {GEN_TEXT} bytes"),
							trace,
						);
					}
				};
				let top: Vec<[(u32, f32); 5]> = row[1..=count].iter().map(|s| s.top).collect();
				parts.push((ids, body, bound, stop, top));
			}
			// then each text, checked against its own bound
			let mut results = Vec::with_capacity(n);
			for (i, ((ids, body, bound, stop, top), p)) in
				parts.into_iter().zip(&planned).enumerate()
			{
				let text = match inner.tokenizer.decode(&ids[..body], true) {
					Ok(t) => t,
					Err(err) => return fail(format!("{op}: chat {i}: {err}"), trace),
				};
				if text.len() > bound {
					return fail(
						format!(
							"{op}: chat {i}: decoded {} bytes, above its bound {bound}",
							text.len()
						),
						trace,
					);
				}
				results.push(Generation {
					text,
					ids,
					prompt_tokens: p.ids.len(),
					stop: if stop == 1 { "eos" } else { "length" },
					top,
				});
			}
			Ok((results, trace))
		},
	);
	match ran {
		Ok(Ok((out, t))) => (Ok(out), t),
		Ok(Err((e, t))) => (Err(e), *t),
		Err(e) => (Err(e), trace),
	}
}

/// A generation as a script object: #{ text, ids, prompt_tokens, stop,
/// top_ids, top_logits }.
fn to_object(g: &Generation) -> Result<rune::Value, String> {
	fn e(x: impl std::fmt::Display) -> String {
		x.to_string()
	}
	let mut o = std::collections::HashMap::<String, rune::Value>::new();
	o.insert("text".into(), rune::to_value(g.text.clone()).map_err(e)?);
	o.insert(
		"ids".into(),
		rune::to_value(g.ids.iter().map(|&x| x as i64).collect::<Vec<_>>()).map_err(e)?,
	);
	o.insert(
		"prompt_tokens".into(),
		rune::to_value(g.prompt_tokens as i64).map_err(e)?,
	);
	o.insert(
		"stop".into(),
		rune::to_value(g.stop.to_string()).map_err(e)?,
	);
	o.insert(
		"top_ids".into(),
		rune::to_value(
			g.top
				.iter()
				.map(|t| t.iter().map(|(id, _)| *id as i64).collect::<Vec<_>>())
				.collect::<Vec<_>>(),
		)
		.map_err(e)?,
	);
	o.insert(
		"top_logits".into(),
		rune::to_value(
			g.top
				.iter()
				.map(|t| t.iter().map(|(_, x)| f64::from(*x)).collect::<Vec<_>>())
				.collect::<Vec<_>>(),
		)
		.map_err(e)?,
	);
	rune::to_value(o).map_err(e)
}

fn int(v: &rune::Value, op: &str) -> Result<usize, String> {
	let n: i64 = rune::from_value(v.clone())
		.map_err(|_| format!("{op}: max_new_tokens must be an integer"))?;
	usize::try_from(n).map_err(|_| format!("{op}: max_new_tokens {n}, want 1 to {MAX_NEW}"))
}

fn chat(
	this: &TextGenerator,
	system: rune::Value,
	user: rune::Value,
	max_new: rune::Value,
) -> Result<rune::Value, String> {
	let op = "TextGenerator::chat";
	let s = system
		.borrow_string_ref()
		.map_err(|_| format!("{op}: system must be a string"))?;
	let u = user
		.borrow_string_ref()
		.map_err(|_| format!("{op}: user must be a string"))?;
	let m = int(&max_new, op)?;
	let mut out = chat_strs(this, &[&s], &[&u], m, &Knobs::default()).0?;
	to_object(&out.remove(0))
}

fn chat_many(
	this: &TextGenerator,
	systems: rune::Value,
	users: rune::Value,
	max_new: rune::Value,
) -> Result<Vec<rune::Value>, String> {
	let op = "TextGenerator::chat_many";
	let m = int(&max_new, op)?;
	let sv = systems
		.borrow_ref::<RuneVec>()
		.map_err(|_| format!("{op}: systems must be a vector of strings"))?;
	let uv = users
		.borrow_ref::<RuneVec>()
		.map_err(|_| format!("{op}: users must be a vector of strings"))?;
	if sv.len() != uv.len() {
		return Err(format!(
			"{op}: {} systems and {} users; one chat per index",
			sv.len(),
			uv.len()
		));
	}
	if sv.is_empty() || sv.len() > MAX_REQUESTS {
		return Err(format!(
			"{op}: {} chats, want 1 to {MAX_REQUESTS}",
			sv.len()
		));
	}
	let mut total = 0usize;
	for (side, v) in [("system", &sv), ("user", &uv)] {
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
	let mut sg = Vec::with_capacity(sv.len());
	for v in sv.iter() {
		sg.push(
			v.borrow_string_ref()
				.map_err(|_| format!("{op}: systems must be a vector of strings"))?,
		);
	}
	let mut ug = Vec::with_capacity(uv.len());
	for v in uv.iter() {
		ug.push(
			v.borrow_string_ref()
				.map_err(|_| format!("{op}: users must be a vector of strings"))?,
		);
	}
	let s: Vec<&str> = sg.iter().map(|x| &**x).collect();
	let u: Vec<&str> = ug.iter().map(|x| &**x).collect();
	chat_strs(this, &s, &u, m, &Knobs::default())
		.0?
		.iter()
		.map(to_object)
		.collect()
}

/// Test support.
pub fn load_dir(dir: &str) -> Result<TextGenerator, String> {
	load(dir)
}
pub fn estimate_for(this: &TextGenerator, p: usize, m: usize) -> Option<usize> {
	estimate_gen(&this.0.geometry, p, m)
}
pub fn prompt_ids(this: &TextGenerator, system: &str, user: &str) -> Result<Vec<u32>, String> {
	this.0
		.tokenizer
		.encode(prompt(system, user), false)
		.map(|e| e.get_ids().to_vec())
		.map_err(|e| e.to_string())
}
/// Test support (the calibration): the resident weights, in bytes.
pub fn resident_bytes(this: &TextGenerator) -> usize {
	expected(&this.0.geometry).map_or(0, |w| expected_bytes(&w).unwrap_or(0) * 2)
}
/// Test support (the early gate): the complete logits at every step of a
/// greedy generation, alongside the chosen ids.
pub fn full_logits(
	this: &TextGenerator,
	system: &str,
	user: &str,
	max_new: usize,
) -> Result<(Vec<u32>, Vec<Vec<f32>>), String> {
	let op = "TextGenerator::full_logits";
	let g = &this.0.geometry;
	let ids = prompt_ids(this, system, user)?;
	let e = |e: candle_core::Error| format!("{op}: {e}");
	worker::run(op, || -> Result<(Vec<u32>, Vec<Vec<f32>>), String> {
		let mut model = this.0.model.clone();
		let cpu = &Device::Cpu;
		let input = CTensor::new(ids.as_slice(), cpu)
			.map_err(e)?
			.unsqueeze(0)
			.map_err(e)?;
		let mut logits = model.forward(&input, 0).map_err(e)?;
		let (mut chosen, mut all) = (Vec::new(), Vec::new());
		for k in 0..max_new {
			let v: Vec<f32> = logits.flatten_all().map_err(e)?.to_vec1().map_err(e)?;
			let tok = top5(&v)[0].0;
			all.push(v);
			chosen.push(tok);
			if tok == g.specials[2] || tok == g.specials[0] || k + 1 == max_new {
				break;
			}
			let next = CTensor::new(&[tok], cpu)
				.map_err(e)?
				.unsqueeze(0)
				.map_err(e)?;
			logits = model.forward(&next, ids.len() + k).map_err(e)?;
		}
		Ok((chosen, all))
	})?
}

pub(crate) fn build(
	m: &mut rune::Module,
) -> Result<Vec<(String, &'static str)>, rune::ContextError> {
	m.ty::<TextGenerator>()?;
	m.function("load", load)
		.build_associated::<TextGenerator>()?;
	m.associated_function("chat", chat)?;
	m.associated_function("chat_many", chat_many)?;
	Ok(vec![
		(
			"candle::TextGenerator".into(),
			"TextGenerator: greedy text generation with a small instruct model (Qwen2) on the CPU",
		),
		(
			"candle::TextGenerator::load".into(),
			"load(dir) -> Result<TextGenerator>: config.json (Qwen2ForCausalLM, the closed Qwen2.5-0.5B-Instruct geometry), tokenizer.json and BF16 model.safetensors (computed in F32)",
		),
		(
			"candle::TextGenerator::chat".into(),
			"chat(system, user, max_new_tokens) -> Result<#{text, ids, prompt_tokens, stop, top_ids, top_logits}>: greedy; stops at <|im_end|>/<|endoftext|> or the limit; a prompt over 1024 tokens is refused, never truncated",
		),
		(
			"candle::TextGenerator::chat_many".into(),
			"chat_many(systems, users, max_new_tokens) -> Result<Vec<...>>: independent chats, concurrently within the budget, in input order",
		),
	])
}

#[cfg(feature = "test-support")]
pub fn fixture_dir() -> String {
	tests_impl::good().to_str().unwrap().to_string()
}

#[cfg(test)]
mod tests {
	use super::tests_impl::*;
	use super::*;
	use serde_json::json;

	/// Synthetic logits for `decode`: a vector with `id` highest.
	fn peak(vocab: usize, id: usize) -> Vec<f32> {
		let mut v = vec![0.0f32; vocab];
		v[id] = 1.0;
		v
	}

	#[test]
	fn the_greedy_policy_on_exact_logits() {
		let (v, eos) = (10, [9u32, 8]);
		let seq = |ids: Vec<usize>| {
			let mut it = ids.into_iter().map(move |i| peak(v, i));
			let first = it.next().unwrap();
			(first, it)
		};
		// stops after <|im_end|> (9), included
		let (first, mut rest) = seq(vec![1, 2, 9, 3]);
		let out = decode(
			8,
			false,
			eos,
			v,
			first,
			|_, _| Ok(rest.next().unwrap()),
			"t",
		)
		.unwrap();
		assert_eq!((out[0].id, out[0].top[0].0), (3, 1));
		assert_eq!(
			out[1..4].iter().map(|s| s.id).collect::<Vec<_>>(),
			[1, 2, 9]
		);
		// and after <|endoftext|> (8)
		let (first, mut rest) = seq(vec![8]);
		let out = decode(
			8,
			false,
			eos,
			v,
			first,
			|_, _| Ok(rest.next().unwrap()),
			"t",
		)
		.unwrap();
		assert_eq!((out[0].id, out[0].top[0].0, out[1].id), (1, 1, 8));
		// the length: exactly max_new, no EOS
		let (first, mut rest) = seq(vec![1, 2, 3, 4]);
		let out = decode(
			3,
			false,
			eos,
			v,
			first,
			|_, _| Ok(rest.next().unwrap()),
			"t",
		)
		.unwrap();
		assert_eq!((out[0].id, out[0].top[0].0), (3, 2));
		// forced length ignores an EOS
		let (first, mut rest) = seq(vec![9, 9, 9]);
		let out = decode(3, true, eos, v, first, |_, _| Ok(rest.next().unwrap()), "t").unwrap();
		assert_eq!((out[0].id, out[0].top[0].0), (3, 2));
		// an exact tie goes to the lowest id; the top 5 by logit, then id
		let mut tie = vec![0.0f32; v];
		tie[7] = 2.0;
		tie[4] = 2.0;
		tie[6] = 1.0;
		let out = decode(1, false, eos, v, tie, |_, _| unreachable!(), "t").unwrap();
		assert_eq!(out[1].id, 4);
		assert_eq!(
			out[1].top.map(|(id, _)| id),
			[4, 7, 6, 0, 1],
			"ties among the zeros by id"
		);
		// a non-finite logit is refused by step and id
		let mut bad = vec![0.0f32; v];
		bad[3] = f32::NAN;
		let e = decode(2, false, eos, v, bad, |_, _| unreachable!(), "t").unwrap_err();
		assert!(e.contains("a non-finite logit at step 0, id 3"), "{e}");
	}

	#[test]
	fn the_fixture_generates_and_its_calls_are_independent() {
		let g = load(good().to_str().unwrap()).unwrap();
		let (s, u) = (
			["be brief", "be brief", "say yes"],
			["a cat sat", "a dog ran", "is it"],
		);
		let alone: Vec<Vec<u32>> = (0..3)
			.map(|i| {
				chat_strs(&g, &[s[i]], &[u[i]], 12, &Knobs::default())
					.0
					.unwrap()
					.remove(0)
					.ids
			})
			.collect();
		// chat_many, and interleaved repeats, give the same ids
		let many = chat_strs(&g, &s, &u, 12, &Knobs::default()).0.unwrap();
		assert_eq!(
			many.iter().map(|r| r.ids.clone()).collect::<Vec<_>>(),
			alone
		);
		for i in [2, 0, 1, 0] {
			let r = chat_strs(&g, &[s[i]], &[u[i]], 12, &Knobs::default())
				.0
				.unwrap();
			assert_eq!(r[0].ids, alone[i]);
		}
		// an injected failure and an injected panic, then reuse: identical,
		// the lowest index's error, and nothing left in flight
		for knobs in [
			Knobs {
				fail_at: vec![1, 2],
				..Default::default()
			},
			Knobs {
				panic_at: vec![1],
				..Default::default()
			},
		] {
			let (r, t) = chat_strs(&g, &s, &u, 12, &knobs);
			let e = r.unwrap_err();
			assert!(e.contains("batch 1") || e.contains("panicked"), "{e}");
			assert_eq!(t.inflight_end, 0);
			let again = chat_strs(&g, &s, &u, 12, &Knobs::default()).0.unwrap();
			assert_eq!(
				again.iter().map(|r| r.ids.clone()).collect::<Vec<_>>(),
				alone
			);
		}
		// each result follows the stop policy
		for r in &many {
			assert_eq!(r.top.len(), r.ids.len());
			match r.stop {
				"eos" => assert!([37, 39].contains(r.ids.last().unwrap())),
				_ => assert_eq!(r.ids.len(), 12),
			}
		}
	}

	/// Review round 1, b: a failure and a panic AFTER decode steps have run
	/// (the clone's KV cache exists), with budget waiters behind them: the
	/// lowest-index error, nothing left in flight, and identical reuse (live
	/// memory: tests/generate_cleanup.rs).
	#[test]
	fn a_mid_decode_failure_or_panic_cleans_up() {
		let g = load(good().to_str().unwrap()).unwrap();
		let (s, u) = (["be brief"; 3], ["a cat sat", "a dog ran", "is it"]);
		let force = Knobs {
			force_length: true,
			..Default::default()
		};
		let alone = chat_strs(&g, &s, &u, 12, &force).0.unwrap();
		// a budget for one request at a time: the others wait at admission
		let p = prompt_ids(&g, s[0], u[0]).unwrap().len();
		let one = (0..3)
			.map(|i| {
				estimate_gen(&g.0.geometry, prompt_ids(&g, s[i], u[i]).unwrap().len(), 12).unwrap()
			})
			.max()
			.unwrap();
		assert!(one >= estimate_gen(&g.0.geometry, p, 12).unwrap());
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
			let (r, t) = chat_strs(&g, &s, &u, 12, &knobs);
			let e = r.unwrap_err();
			assert!(
				e.contains("injected failure in request 1 after decode step 3")
					|| e.contains("panicked"),
				"{name}: {e}"
			);
			assert_eq!(t.max_running, 1, "{name}: one at a time, the others waited");
			assert_eq!(t.inflight_end, 0, "{name}: every permit released");
			let again = chat_strs(&g, &s, &u, 12, &force).0.unwrap();
			assert_eq!(
				again.iter().map(|r| &r.ids).collect::<Vec<_>>(),
				alone.iter().map(|r| &r.ids).collect::<Vec<_>>(),
				"{name}: identical reuse"
			);
		}
	}

	#[test]
	fn the_bounds_are_refused_by_name() {
		let g = load(good().to_str().unwrap()).unwrap();
		let ok = Knobs::default();
		let e = chat_strs(&g, &["a"], &["a", "b"], 4, &ok).0.unwrap_err();
		assert!(e.contains("1 systems and 2 users"), "{e}");
		let e = chat_strs(&g, &[], &[], 4, &ok).0.unwrap_err();
		assert!(e.contains("0 chats"), "{e}");
		for m in [0, MAX_NEW + 1] {
			let e = chat_strs(&g, &["a"], &["a"], m, &ok).0.unwrap_err();
			assert!(e.contains(&format!("max_new_tokens {m}")), "{e}");
		}
		let long = "a".repeat(MAX_TEXT + 1);
		let e = chat_strs(&g, &["a"], &[&long], 4, &ok).0.unwrap_err();
		assert!(e.contains("user 0 is"), "{e}");
		// a prompt over 1,024 tokens (one per character here): refused by
		// its count, never truncated
		let over = "a".repeat(MAX_PROMPT);
		let e = chat_strs(&g, &["a"], &[&over], 4, &ok).0.unwrap_err();
		assert!(e.contains("tokens, at most 1024 (never truncated)"), "{e}");
		// a budget below one request's estimate: refused before it runs
		let e = chat_strs(
			&g,
			&["a"],
			&["a"],
			4,
			&Knobs {
				agg: Some(1),
				..Default::default()
			},
		)
		.0
		.unwrap_err();
		assert!(e.contains("exceeds the in-flight budget"), "{e}");
	}

	#[test]
	fn the_closed_contract_refuses_by_name() {
		let c = |k: &'static str, v: serde_json::Value, want: &str| {
			refused(model(move |c| c[k] = v, |_| {}, |_| {}), want)
		};
		c("architectures", json!(["Qwen2Model"]), "architectures");
		c("model_type", json!("llama"), "model_type");
		c("hidden_act", json!("gelu"), "hidden_act");
		c("rms_norm_eps", json!(1e-5), "rms_norm_eps");
		c("rope_theta", json!(10000.0), "rope_theta");
		c("tie_word_embeddings", json!(false), "tie_word_embeddings");
		c("use_sliding_window", json!(true), "use_sliding_window");
		c("sliding_window", json!(4096), "sliding_window");
		c("max_window_layers", json!(28), "max_window_layers");
		c("torch_dtype", json!("float32"), "torch_dtype");
		for k in ["rope_scaling", "quantization_config", "extra_key"] {
			c(k, json!({}), &format!("key \"{k}\" is not admitted"));
		}
		c("num_hidden_layers", json!(3), "is not the admitted one");
		c("eos_token_id", json!(38), "is not the admitted one");
		refused(
			model(
				|c| {
					c.as_object_mut().unwrap().remove("use_sliding_window");
				},
				|_| {},
				|_| {},
			),
			"use_sliding_window is absent",
		);
		// the tokenizer
		refused(
			model(|_| {}, |t| t["model"]["vocab"]["zz"] = json!(36), |_| {}),
			"tokens, want exactly 40",
		);
		// review round 1, R1: dense ids, the count preserved: a shared id
		// (leaving a hole) is refused by name
		refused(
			model(|_| {}, |t| t["model"]["vocab"]["b"] = json!(0), |_| {}),
			"the ids must be dense",
		);
		refused(
			model(|_| {}, |t| t["decoder"] = json!({"type": "Fuse"}), |_| {}),
			"decoder must be exactly",
		);
		refused(
			model(
				|_| {},
				|t| t["post_processor"] = json!({"type": "TemplateProcessing", "single": [], "pair": [], "special_tokens": {}}),
				|_| {},
			),
			"post_processor must be exactly",
		);
		refused(
			model(
				|_| {},
				|t| t["added_tokens"][2]["special"] = json!(false),
				|_| {},
			),
			"<|im_end|> must be the special token 39",
		);
		// the weights
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.remove("model.norm.weight");
				},
			),
			"bytes of tensors",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					let t = m.remove("model.norm.weight").unwrap();
					m.insert("model.other.weight".into(), t);
				},
			),
			"tensor model.",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					let t = m["model.layers.0.self_attn.k_proj.weight"]
						.reshape((16, 8))
						.unwrap();
					m.insert("model.layers.0.self_attn.k_proj.weight".into(), t);
				},
			),
			"k_proj.weight is [16, 8], want [8, 16]",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					// an F16 tensor has BF16's bytes: refused by dtype
					let t = m["model.norm.weight"].to_dtype(DType::F16).unwrap();
					m.insert("model.norm.weight".into(), t);
				},
			),
			"model.norm.weight is F16, want BF16",
		);
		refused(
			model(
				|_| {},
				|_| {},
				|m| {
					m.insert(
						"lm_head.weight".into(),
						CTensor::zeros((0,), DType::BF16, &Device::Cpu).unwrap(),
					);
				},
			),
			"unexpected tensor lm_head.weight",
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
		let w = expected(&PRODUCTION).unwrap();
		assert_eq!(w.len(), 290);
		assert_eq!(expected_bytes(&w), Some(988_065_536));
	}
}

#[cfg(any(test, feature = "test-support"))]
#[allow(dead_code)]
mod tests_impl {
	//! A tiny generated Qwen2 in the fixture geometry, with a ByteLevel BPE
	//! tokenizer of single characters (one token per character) and the
	//! three special tokens at 37, 38 and 39.
	pub(super) use super::*;
	pub(super) use serde_json::json;
	pub(super) use std::collections::HashMap;
	pub(super) use std::path::{Path, PathBuf};

	pub(super) fn fresh(tag: &str) -> PathBuf {
		static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		let d = std::env::temp_dir().join(format!("rnx-0149-gen-{}/{tag}-{n}", std::process::id()));
		std::fs::create_dir_all(&d).unwrap();
		d
	}

	pub(super) fn config() -> serde_json::Value {
		json!({
			"architectures": [ARCHITECTURE], "model_type": "qwen2", "hidden_act": "silu",
			"rms_norm_eps": 1e-6, "rope_theta": 1_000_000.0, "tie_word_embeddings": true,
			"use_sliding_window": false, "sliding_window": 32768, "max_window_layers": 21,
			"attention_dropout": 0.0, "torch_dtype": "bfloat16", "use_cache": true,
			"hidden_size": 16, "num_hidden_layers": 2, "num_attention_heads": 2,
			"num_key_value_heads": 1, "intermediate_size": 32, "vocab_size": 40,
			"max_position_embeddings": 32768, "bos_token_id": 37, "eos_token_id": 39,
			"initializer_range": 0.02, "transformers_version": "4.43.1"
		})
	}

	pub(super) fn tokenizer() -> serde_json::Value {
		let mut v = serde_json::Map::new();
		let chars: Vec<String> = ('a'..='z')
			.map(|c| c.to_string())
			.chain(["Ġ", "Ċ", ".", ",", ":", "?", "!", "-", "0", "1", "<unk>"].map(String::from))
			.collect();
		for (i, c) in chars.iter().enumerate() {
			v.insert(c.clone(), json!(i));
		}
		let bl = json!({"type": "ByteLevel", "add_prefix_space": false, "trim_offsets": false, "use_regex": false});
		let added = |id: u32, c: &str| json!({"id": id, "content": c, "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true});
		json!({
			"version": "1.0", "truncation": null, "padding": null,
			"added_tokens": [added(37, "<|endoftext|>"), added(38, "<|im_start|>"), added(39, "<|im_end|>")],
			"normalizer": {"type": "NFC"}, "pre_tokenizer": bl, "post_processor": bl, "decoder": bl,
			"model": {"type": "BPE", "dropout": null, "unk_token": "<unk>", "continuing_subword_prefix": null,
				"end_of_word_suffix": null, "fuse_unk": false, "byte_fallback": false, "ignore_merges": false,
				"vocab": serde_json::Value::Object(v), "merges": []}
		})
	}

	pub(super) fn write(d: &Path, name: &str, v: &serde_json::Value) {
		std::fs::write(d.join(name), v.to_string()).unwrap();
	}

	/// The weights Candle's own loader reads for the fixture geometry, in
	/// BF16; `edit` changes the tensor map.
	pub(super) fn weights(d: &Path, edit: impl FnOnce(&mut HashMap<String, CTensor>)) {
		let varmap = candle_nn::VarMap::new();
		let vb = VarBuilder::from_varmap(&varmap, DType::F32, &Device::Cpu);
		let typed: Config = serde_json::from_value(config()).unwrap();
		ModelForCausalLM::new(&typed, vb).unwrap();
		let mut map: HashMap<String, CTensor> = varmap
			.data()
			.lock()
			.unwrap()
			.iter()
			.map(|(k, v)| (k.clone(), v.as_tensor().to_dtype(DType::BF16).unwrap()))
			.collect();
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
}
