//! Record 0131: a pretrained sentence encoder on the CPU, scoped to one
//! workflow (text → embeddings → similarity → a ranked table).
//!
//! - `candle::TextEncoder::load(dir)`: a sentence-transformers BERT model
//!   (Transformer → mean Pooling → Normalize) from a local directory. Every
//!   file is read bounded; the config, module list, pooling mode, sequence
//!   length and tokenizer are checked against a strict contract; weights are
//!   parsed from the buffer (never memory-mapped).
//! - `enc.embed(texts)`: borrowed texts, checked before any copy, encoded in
//!   batches that shrink until every activation cap holds; the attention
//!   mask reaches the model and the pooling; rows are normalized exactly as
//!   sentence-transformers' `Normalize` (`x / max(‖x‖, 1e-12)`).
//! - `candle::similarity(a, b, names)`: cosine of every row pair, with a
//!   stable row normalization (scaled by the row's largest magnitude, the
//!   squares summed in f64).
use crate::{read_limited, worker};
use candle_core::{DType, Device, Tensor as CTensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use rnx::interchange::{self, Data, Dense};
use rnx::rune::{self, Any, runtime::Vec as RuneVec};
use std::sync::Arc;
use tokenizers::{
	PaddingDirection, PaddingParams, PaddingStrategy, PostProcessor, Tokenizer,
	TruncationDirection, TruncationParams, TruncationStrategy,
};

/// A JSON configuration file is at most 64 KiB.
pub const MAX_JSON: u64 = 64 << 10;
/// `tokenizer.json` is at most 16 MiB.
pub const MAX_TOKENIZER: u64 = 16 << 20;
/// `model.safetensors` is at most 512 MiB.
pub const MAX_WEIGHTS: u64 = 512 << 20;
/// One `embed` call: 1 to 32,768 texts, each at most 64 KiB, 16 MiB in all.
pub const MAX_TEXTS: usize = 32_768;
pub const MAX_TEXT: usize = 64 << 10;
pub const MAX_TOTAL: usize = 16 << 20;
/// A batch starts at 32 texts and halves until every cap holds.
pub const BATCH: usize = 32;

/// The per-batch activation caps, in values.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
	/// `batch × seq × hidden`.
	pub hidden: usize,
	/// `batch × seq × intermediate_size`.
	pub ffn: usize,
	/// `batch × heads × seq²`.
	pub attention: usize,
}
pub const CAPS: Caps = Caps {
	hidden: 1 << 22,
	ffn: 1 << 24,
	attention: 1 << 25,
};

const TRANSFORMER: &str = "sentence_transformers.models.Transformer";
const POOLING: &str = "sentence_transformers.models.Pooling";
const NORMALIZE: &str = "sentence_transformers.models.Normalize";

struct Inner {
	model: BertModel,
	tokenizer: Tokenizer,
	config: Config,
	max_seq: usize,
}

/// A loaded sentence encoder. Cloning shares it.
#[derive(Any, Clone)]
#[rune(item = ::candle)]
pub struct TextEncoder(Arc<Inner>);

fn json(dir: &std::path::Path, name: &str, op: &str) -> Result<serde_json::Value, String> {
	let path = dir.join(name);
	let path = path
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(path, MAX_JSON, op)?;
	serde_json::from_slice(&bytes).map_err(|e| format!("{op} {path:?}: {e}"))
}

/// A config size in `1..=max`, named.
fn sized(v: usize, name: &str, max: usize, op: &str) -> Result<usize, String> {
	if v == 0 || v > max {
		return Err(format!("{op}: {name} = {v}, want 1 to {max}"));
	}
	Ok(v)
}

/// The model contract, checked on the parsed files before any weight is read.
fn contract(dir: &std::path::Path, op: &str) -> Result<(Config, usize), String> {
	let raw = json(dir, "config.json", op)?;
	let field = |k: &str| raw.get(k).and_then(|v| v.as_str()).unwrap_or("");
	if field("model_type") != "bert" {
		return Err(format!(
			"{op}: config.json model_type is {:?}, want \"bert\"",
			field("model_type")
		));
	}
	if field("hidden_act") != "gelu" {
		return Err(format!(
			"{op}: config.json hidden_act is {:?}, want \"gelu\"",
			field("hidden_act")
		));
	}
	if let Some(p) = raw.get("position_embedding_type")
		&& p.as_str() != Some("absolute")
	{
		return Err(format!(
			"{op}: config.json position_embedding_type is {p}, want \"absolute\""
		));
	}
	let config: Config =
		serde_json::from_value(raw.clone()).map_err(|e| format!("{op}: config.json: {e}"))?;
	let heads = sized(config.num_attention_heads, "num_attention_heads", 64, op)?;
	let hidden = sized(config.hidden_size, "hidden_size", 1024, op)?;
	sized(config.num_hidden_layers, "num_hidden_layers", 24, op)?;
	sized(config.intermediate_size, "intermediate_size", 4096, op)?;
	let vocab = sized(config.vocab_size, "vocab_size", 250_000, op)?;
	let positions = sized(
		config.max_position_embeddings,
		"max_position_embeddings",
		512,
		op,
	)?;
	sized(config.type_vocab_size, "type_vocab_size", 16, op)?;
	if hidden % heads != 0 {
		return Err(format!(
			"{op}: hidden_size {hidden} is not divisible by num_attention_heads {heads}"
		));
	}
	if config.pad_token_id >= vocab {
		return Err(format!(
			"{op}: pad_token_id {} is not below vocab_size {vocab}",
			config.pad_token_id
		));
	}

	let modules = json(dir, "modules.json", op)?;
	let types: Vec<(&str, &str)> = modules
		.as_array()
		.map(|a| {
			a.iter()
				.map(|m| {
					(
						m.get("type").and_then(|v| v.as_str()).unwrap_or(""),
						m.get("path").and_then(|v| v.as_str()).unwrap_or(""),
					)
				})
				.collect()
		})
		.unwrap_or_default();
	if types.len() != 3
		|| types[0].0 != TRANSFORMER
		|| types[1].0 != POOLING
		|| types[2].0 != NORMALIZE
	{
		return Err(format!(
			"{op}: modules.json must be exactly Transformer, Pooling, Normalize; found {:?}",
			types.iter().map(|t| t.0).collect::<Vec<_>>()
		));
	}
	let pooling_dir = types[1].1;
	if pooling_dir.is_empty() || pooling_dir.contains("..") || pooling_dir.starts_with('/') {
		return Err(format!("{op}: modules.json Pooling path {pooling_dir:?}"));
	}
	let pooling = json(dir, &format!("{pooling_dir}/config.json"), op)?;
	let pooling = pooling
		.as_object()
		.ok_or_else(|| format!("{op}: {pooling_dir}/config.json is not an object"))?;
	if pooling
		.get("word_embedding_dimension")
		.and_then(|v| v.as_u64())
		!= Some(hidden as u64)
	{
		return Err(format!(
			"{op}: the pooling dimension is {:?}, want hidden_size {hidden}",
			pooling.get("word_embedding_dimension")
		));
	}
	for (k, v) in pooling {
		if let Some(mode) = k.strip_prefix("pooling_mode_") {
			let on = v.as_bool().unwrap_or(true);
			if on != (mode == "mean_tokens") {
				return Err(format!(
					"{op}: only mean-token pooling is supported; {k} is {v}"
				));
			}
		}
	}
	if pooling
		.get("pooling_mode_mean_tokens")
		.and_then(|v| v.as_bool())
		!= Some(true)
	{
		return Err(format!("{op}: pooling_mode_mean_tokens must be true"));
	}

	let sbert = json(dir, "sentence_bert_config.json", op)?;
	let max_seq = sbert
		.get("max_seq_length")
		.and_then(|v| v.as_u64())
		.ok_or_else(|| format!("{op}: sentence_bert_config.json has no max_seq_length"))?
		as usize;
	if max_seq < 2 || max_seq > positions {
		return Err(format!(
			"{op}: max_seq_length {max_seq}, want 2 to max_position_embeddings {positions}"
		));
	}
	if sbert.get("do_lower_case").and_then(|v| v.as_bool()) == Some(true) {
		return Err(format!(
			"{op}: do_lower_case = true is not supported (the tokenizer must do any lowercasing)"
		));
	}
	Ok((config, max_seq))
}

/// The tokenizer, checked against the model and configured by rnx.
fn tokenizer(
	dir: &std::path::Path,
	config: &Config,
	max_seq: usize,
	op: &str,
) -> Result<Tokenizer, String> {
	let path = dir.join("tokenizer.json");
	let path = path
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(path, MAX_TOKENIZER, op)?;
	let mut t = Tokenizer::from_bytes(&bytes).map_err(|e| format!("{op} {path:?}: {e}"))?;
	// every id the vocabulary holds, not its count: ids may be sparse
	if let Some((token, id)) = t
		.get_vocab(true)
		.into_iter()
		.find(|(_, id)| *id as usize >= config.vocab_size)
	{
		return Err(format!(
			"{op}: tokenizer id {id} ({token:?}) is not below vocab_size {}",
			config.vocab_size
		));
	}
	let pad_id = config.pad_token_id as u32;
	let pad_token = t
		.id_to_token(pad_id)
		.ok_or_else(|| format!("{op}: pad_token_id {pad_id} is not in the tokenizer"))?;
	// tokenizers computes `max_length - added` unsigned; refuse first
	let added = t.get_post_processor().map_or(0, |p| p.added_tokens(false));
	if max_seq <= added {
		return Err(format!(
			"{op}: max_seq_length {max_seq} leaves no room beside {added} special tokens"
		));
	}
	t.with_padding(Some(PaddingParams {
		strategy: PaddingStrategy::BatchLongest,
		direction: PaddingDirection::Right,
		pad_to_multiple_of: None,
		pad_id,
		pad_type_id: 0,
		pad_token,
	}));
	t.with_truncation(Some(TruncationParams {
		direction: TruncationDirection::Right,
		max_length: max_seq,
		strategy: TruncationStrategy::LongestFirst,
		stride: 0,
	}))
	.map_err(|e| format!("{op}: truncation: {e}"))?;
	Ok(t)
}

/// `TextEncoder::load(dir)`.
fn load(dir: &str) -> Result<TextEncoder, String> {
	let op = "TextEncoder::load";
	let root = std::path::Path::new(dir);
	let (config, max_seq) = contract(root, op)?;
	let tokenizer = tokenizer(root, &config, max_seq, op)?;
	let weights = root.join("model.safetensors");
	let weights = weights
		.to_str()
		.ok_or_else(|| format!("{op}: a non-UTF-8 path"))?;
	let bytes = read_limited(weights, MAX_WEIGHTS, op)?;
	let weights = weights.to_owned();
	let model = worker::run(op, || -> Result<BertModel, String> {
		let tensors = candle_core::safetensors::load_buffer(&bytes, &Device::Cpu)
			.map_err(|e| format!("{op} {weights:?}: {e}"))?;
		// every tensor F32 (the VarBuilder would otherwise convert a weight
		// silently), with one cited exception: transformers' integer
		// `embeddings.position_ids` buffer, which candle's BertModel never reads
		if let Some((k, t)) = tensors
			.iter()
			.find(|(k, t)| t.dtype() != DType::F32 && !unused_buffer(k, t.dtype()))
		{
			return Err(format!("{op}: {k} is {:?}, want F32", t.dtype()));
		}
		let vb = VarBuilder::from_tensors(tensors, DType::F32, &Device::Cpu);
		BertModel::load(vb, &config).map_err(|e| format!("{op} {weights:?}: {e}"))
	})??;
	Ok(TextEncoder(Arc::new(Inner {
		model,
		tokenizer,
		config,
		max_seq,
	})))
}

/// The one known unused buffer: an integer `embeddings.position_ids`, bare
/// or under the model-type prefix (BertModel::load tries both).
fn unused_buffer(key: &str, dtype: DType) -> bool {
	matches!(
		key,
		"embeddings.position_ids" | "bert.embeddings.position_ids"
	) && dtype.is_int()
}

/// The caps for one batch of `batch` texts padded to `seq`, checked.
fn fits(config: &Config, batch: usize, seq: usize, caps: Caps) -> bool {
	let within = |a: Option<usize>, cap: usize| a.is_some_and(|v| v <= cap);
	let bs = batch.checked_mul(seq);
	within(
		bs.and_then(|v| v.checked_mul(config.hidden_size)),
		caps.hidden,
	) && within(
		bs.and_then(|v| v.checked_mul(config.intermediate_size)),
		caps.ffn,
	) && within(
		batch
			.checked_mul(config.num_attention_heads)
			.and_then(|v| v.checked_mul(seq))
			.and_then(|v| v.checked_mul(seq)),
		caps.attention,
	)
}

/// Record 0132: the in-flight budget for one `embed` call, in f32 values
/// (2²⁸, 1 GiB). Batch estimates are calibrated to Candle's measured
/// transients (`TRANSIENT`), so the budget tracks real memory; it is still an
/// estimate for this invocation, not a process-wide or allocator ceiling.
pub const AGG: usize = 1 << 28;
/// A batch's measured allocator peak is up to 3.70 times its activation
/// formula (attention dominated, 32 × 256 tokens; 2.29 for short texts;
/// `probes/0132/out/calibrate.txt`): the estimate is the formula times 4.
pub const TRANSIENT: usize = 4;
/// At most 32 batches run at once.
pub const MAX_WORKERS: usize = 32;
// a lone batch at 0131's caps always fits the budget: no deadlock
const _: () = assert!(TRANSIENT * (CAPS.hidden + CAPS.ffn + CAPS.attention) <= AGG);

/// The planned ids and mask: 2 × 4 bytes × 32,768 texts × 512 tokens, the
/// largest input 0131's contract admits (128 MiB; 64 MiB at the pinned model).
pub const PLAN_PAYLOAD: usize = 2 * 4 * MAX_TEXTS * 512;
/// The batches' own records, derived from their actual size: at most one
/// batch per text.
pub const PLAN_METADATA: usize = std::mem::size_of::<Planned>() * MAX_TEXTS;

/// One planned batch: its texts tokenized, checked and reduced to the ids
/// and mask the model takes; the tokenizer's `Encoding`s are already gone.
struct Planned {
	b: usize,
	seq: usize,
	ids: Vec<u32>,
	mask: Vec<u32>,
	estimate: usize,
}

/// A batch's in-flight estimate: hidden and feed-forward states plus the
/// attention scores, times the measured transient factor, checked.
fn estimate(c: &Config, b: usize, seq: usize) -> Option<usize> {
	let bs = b.checked_mul(seq)?;
	bs.checked_mul(c.hidden_size.checked_add(c.intermediate_size)?)?
		.checked_add(
			b.checked_mul(c.num_attention_heads)?
				.checked_mul(seq)?
				.checked_mul(seq)?,
		)?
		.checked_mul(TRANSIENT)
}

/// Per-call controls. Production uses the defaults; the test-support entry
/// point sets them per call, so no test shares global state with another.
#[derive(Clone, Debug, Default)]
pub struct Knobs {
	pub workers: Option<usize>,
	pub agg: Option<usize>,
	pub plan_fail_at: Option<usize>,
	pub fail_at: Vec<usize>,
	pub panic_at: Vec<usize>,
	/// Hold this batch at admission, as if over budget, until a higher index
	/// has failed (the cutoff control's deterministic barrier).
	pub hold: Option<usize>,
	/// Panic after the model returns, before the rows are published.
	pub panic_after_run: Vec<usize>,
	/// Sleep this long (ms) inside a batch, before publication, so another
	/// batch finishes first.
	pub delay: Vec<(usize, u64)>,
}

/// What one call did, returned with its result.
#[derive(Clone, Debug, Default)]
pub struct Trace {
	/// The planned batches' sizes, in order.
	pub batches: Vec<usize>,
	/// The batches that ran to success, sorted.
	pub executed: Vec<usize>,
	/// The failing batches, sorted, as (index, error).
	pub failed: Vec<(usize, String)>,
	/// The failing indices in the order they failed.
	pub fail_order: Vec<usize>,
	/// The successful indices in the order they completed.
	pub completion_order: Vec<usize>,
	pub workers: usize,
	pub max_inflight: usize,
	pub max_running: usize,
	/// The in-flight sum after every worker joined: always 0.
	pub inflight_end: usize,
	/// Candle's matmul threads and the rayon pool, seen inside a batch.
	pub threads: (usize, usize),
	/// Wall time of the planning (tokenizing) and execution stages.
	pub plan_s: f64,
	pub exec_s: f64,
}

/// Stage 1, sequential: the same batches as 0131 (32, halved until the caps
/// hold), each reduced to compact ids and mask at once. Stops at the first
/// failing batch, returning the batches before it and that error.
fn plan(
	inner: &Inner,
	strs: &[&str],
	caps: Caps,
	agg: usize,
	knobs: &Knobs,
	op: &str,
) -> (Vec<Planned>, Option<String>) {
	let n = strs.len();
	let mut planned: Vec<Planned> = Vec::new();
	let mut payload = 0usize;
	let mut at = 0;
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
		let encodings = loop {
			let encodings = match inner
				.tokenizer
				.encode_batch(strs[at..at + b].to_vec(), true)
			{
				Ok(e) => e,
				Err(e) => return (planned, failed(format!("{op}: {e}"))),
			};
			let seq = encodings[0].len();
			if fits(&inner.config, b, seq, caps) {
				break encodings;
			}
			if b == 1 {
				return (
					planned,
					failed(format!(
						"{op}: text {at} at {seq} tokens exceeds the activation caps"
					)),
				);
			}
			b /= 2;
		};
		let seq = encodings[0].len();
		if seq > inner.max_seq || encodings.iter().any(|e| e.len() != seq) {
			return (
				planned,
				failed(format!(
					"{op}: a padded length of {seq}, at most {}",
					inner.max_seq
				)),
			);
		}
		let Some(est) = estimate(&inner.config, b, seq).filter(|e| *e <= agg) else {
			return (
				planned,
				failed(format!(
					"{op}: batch {index} ({b} texts at {seq} tokens) is above the in-flight budget of {agg} values"
				)),
			);
		};
		// the planned storage, checked before this batch's vectors exist
		let need = b.checked_mul(seq).and_then(|v| v.checked_mul(8));
		match need.and_then(|v| payload.checked_add(v)) {
			Some(total) if total <= PLAN_PAYLOAD => payload = total,
			_ => {
				return (
					planned,
					failed(format!(
						"{op}: the planned input exceeds {PLAN_PAYLOAD} bytes"
					)),
				);
			}
		}
		if (index + 1) * std::mem::size_of::<Planned>() > PLAN_METADATA {
			return (planned, failed(format!("{op}: too many batches")));
		}
		let mut ids = Vec::with_capacity(b * seq);
		let mut mask = Vec::with_capacity(b * seq);
		for e in &encodings {
			for &id in e.get_ids() {
				if id as usize >= inner.config.vocab_size {
					return (
						planned,
						failed(format!(
							"{op}: token id {id} is not below vocab_size {}",
							inner.config.vocab_size
						)),
					);
				}
				ids.push(id);
			}
			mask.extend_from_slice(e.get_attention_mask());
		}
		drop(encodings);
		if let Some(row) = mask.chunks(seq).position(|m| m.iter().all(|&x| x == 0)) {
			return (
				planned,
				failed(format!(
					"{op}: text {row} of its batch has no tokens to pool"
				)),
			);
		}
		planned.push(Planned {
			b,
			seq,
			ids,
			mask,
			estimate: est,
		});
		at += b;
	}
	(planned, None)
}

/// One batch through the model: the mask into the model and the pooling,
/// mean pooled, normalized as `x / max(‖x‖, 1e-12)`.
fn run_batch(inner: &Inner, p: &Planned, op: &str) -> Result<Vec<f32>, String> {
	let cpu = &Device::Cpu;
	let e = |e: candle_core::Error| format!("{op}: {e}");
	let input = CTensor::from_slice(&p.ids, (p.b, p.seq), cpu).map_err(e)?;
	let types = input.zeros_like().map_err(e)?;
	let mask = CTensor::from_slice(&p.mask, (p.b, p.seq), cpu).map_err(e)?;
	let hidden = inner
		.model
		.forward(&input, &types, Some(&mask))
		.map_err(e)?;
	let m = mask
		.to_dtype(DType::F32)
		.map_err(e)?
		.unsqueeze(2)
		.map_err(e)?;
	let summed = hidden.broadcast_mul(&m).map_err(e)?.sum(1).map_err(e)?;
	let count = m.sum(1).map_err(e)?;
	let pooled = summed.broadcast_div(&count).map_err(e)?;
	let norm = pooled
		.sqr()
		.map_err(e)?
		.sum_keepdim(1)
		.map_err(e)?
		.sqrt()
		.map_err(e)?
		.maximum(1e-12)
		.map_err(e)?;
	let rows = pooled.broadcast_div(&norm).map_err(e)?;
	rows.flatten_all().map_err(e)?.to_vec1::<f32>().map_err(e)
}

/// The admission state, guarded by one mutex that the condition variable's
/// predicate is checked under, so no change to `cutoff` or `inflight` can
/// be missed between a check and a wait.
struct Admission {
	inflight: usize,
	running: usize,
	/// The lowest failing batch index so far; indices at or above it are
	/// skipped, while a claimed lower index always still runs.
	cutoff: usize,
	failed: Vec<(usize, String)>,
	executed: Vec<usize>,
	completed: Vec<usize>,
	max_inflight: usize,
	max_running: usize,
	threads: (usize, usize),
}

/// One admitted batch's reservation. Dropping it, on success, error or
/// unwind, releases the in-flight estimate, records the outcome (an unwind
/// with no outcome is that batch's "Candle panicked"), moves the cutoff on
/// failure, and wakes every waiter, all under the admission mutex.
struct Permit<'a> {
	state: &'a std::sync::Mutex<Admission>,
	wake: &'a std::sync::Condvar,
	index: usize,
	estimate: usize,
	outcome: Option<Result<(), String>>,
	op: &'a str,
}

impl Drop for Permit<'_> {
	fn drop(&mut self) {
		let mut st = lock(self.state);
		st.inflight -= self.estimate;
		st.running -= 1;
		let outcome = self
			.outcome
			.take()
			.unwrap_or_else(|| Err(format!("{}: Candle panicked", self.op)));
		match outcome {
			Ok(()) => {
				st.executed.push(self.index);
				st.completed.push(self.index);
				#[cfg(feature = "test-support")]
				{
					st.threads = (
						candle_core::utils::get_num_threads(),
						rayon::current_num_threads(),
					);
				}
			}
			Err(e) => {
				st.failed.push((self.index, e));
				st.cutoff = st.cutoff.min(self.index);
			}
		}
		self.wake.notify_all();
	}
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stage 2, concurrent: W joined workers claim batch indices in order,
/// wait for the budget, run, and write their rows into the batch's own
/// slice of `out`. Returns once every worker has joined.
fn execute(
	inner: &Inner,
	planned: &[Planned],
	out: &mut [f32],
	(workers, agg): (usize, usize),
	knobs: &Knobs,
	trace: &mut Trace,
	op: &str,
) {
	use std::sync::atomic::{AtomicUsize, Ordering};
	let n = planned.len();
	let hidden = inner.config.hidden_size;
	let mut slots = Vec::with_capacity(n);
	let mut rest = out;
	for p in planned {
		let (slot, r) = rest.split_at_mut(p.b * hidden);
		slots.push(std::sync::Mutex::new(slot));
		rest = r;
	}
	let state = std::sync::Mutex::new(Admission {
		inflight: 0,
		running: 0,
		cutoff: n,
		failed: Vec::new(),
		executed: Vec::new(),
		completed: Vec::new(),
		max_inflight: 0,
		max_running: 0,
		threads: (0, 0),
	});
	let wake = std::sync::Condvar::new();
	let next = AtomicUsize::new(0);
	let w = workers.clamp(1, MAX_WORKERS).min(n.max(1));
	let work = || {
		loop {
			let i = next.fetch_add(1, Ordering::SeqCst);
			if i >= n {
				break;
			}
			let est = planned[i].estimate;
			// admission, under the one mutex
			let admitted = {
				let mut st = lock(&state);
				loop {
					if i >= st.cutoff {
						break false;
					}
					let held = knobs.hold == Some(i)
						&& !st.failed.iter().any(|(j, _)| *j > i)
						&& (st.running > 0 || next.load(Ordering::SeqCst) < n);
					if !held && st.inflight + est <= agg {
						st.inflight += est;
						st.running += 1;
						st.max_inflight = st.max_inflight.max(st.inflight);
						st.max_running = st.max_running.max(st.running);
						break true;
					}
					st = wake.wait(st).unwrap_or_else(|e| e.into_inner());
				}
			};
			if !admitted {
				continue;
			}
			// the reservation is a permit: its Drop releases it, records the
			// outcome and wakes every waiter under the admission mutex, on
			// every path, unwinding included
			let mut permit = Permit {
				state: &state,
				wake: &wake,
				index: i,
				estimate: est,
				outcome: None,
				op,
			};
			// the batch and the publication of its rows, inside one indexed
			// panic boundary
			let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
				if knobs.panic_at.contains(&i) {
					panic!("injected panic at batch {i}");
				}
				if knobs.fail_at.contains(&i) {
					return Err(format!("{op}: injected failure at batch {i}"));
				}
				let rows = run_batch(inner, &planned[i], op)?;
				if knobs.panic_after_run.contains(&i) {
					panic!("injected panic after batch {i} ran");
				}
				if let Some((_, ms)) = knobs.delay.iter().find(|(j, _)| *j == i) {
					std::thread::sleep(std::time::Duration::from_millis(*ms));
				}
				lock(&slots[i]).copy_from_slice(&rows);
				Ok(())
			}))
			.unwrap_or_else(|_| Err(format!("{op}: Candle panicked")));
			permit.outcome = Some(r);
			drop(permit);
		}
	};
	if w == 1 {
		work();
	} else {
		std::thread::scope(|s| {
			for _ in 0..w {
				s.spawn(work);
			}
		});
	}
	let st = state.into_inner().unwrap_or_else(|e| e.into_inner());
	let mut executed = st.executed;
	executed.sort_unstable();
	let mut failed = st.failed;
	trace.fail_order = failed.iter().map(|(i, _)| *i).collect();
	failed.sort_by_key(|(i, _)| *i);
	trace.executed = executed;
	trace.completion_order = st.completed;
	trace.failed = failed;
	trace.workers = w;
	trace.max_inflight = st.max_inflight;
	trace.max_running = st.max_running;
	trace.inflight_end = st.inflight;
	trace.threads = st.threads;
}

/// The texts, borrowed and checked before anything is copied or tokenized:
/// the count from the vector's length, each text's bytes, the total.
fn embed_value(this: &TextEncoder, texts: &rune::Value, caps: Caps) -> Result<Dense, String> {
	let op = "TextEncoder::embed";
	let not_texts = || format!("{op}: texts must be a vector of strings");
	let values = texts.borrow_ref::<RuneVec>().map_err(|_| not_texts())?;
	let n = values.len();
	if n == 0 || n > MAX_TEXTS {
		return Err(format!("{op}: {n} texts, want 1 to {MAX_TEXTS}"));
	}
	let mut guards = Vec::with_capacity(n);
	for v in values.iter() {
		guards.push(v.borrow_string_ref().map_err(|_| not_texts())?);
	}
	let strs: Vec<&str> = guards.iter().map(|s| &**s).collect();
	embed_strs(this, &strs, caps, &Knobs::default()).0
}

/// Every entry point's checks, on borrowed text, before any tokenizing or
/// output allocation: the count, each text's bytes, the total, the shape.
fn preflight(strs: &[&str], hidden: usize, op: &str) -> Result<(), String> {
	let n = strs.len();
	if n == 0 || n > MAX_TEXTS {
		return Err(format!("{op}: {n} texts, want 1 to {MAX_TEXTS}"));
	}
	let mut total = 0usize;
	for (i, s) in strs.iter().enumerate() {
		if s.len() > MAX_TEXT {
			return Err(format!(
				"{op}: text {i} is {} bytes, at most {MAX_TEXT}",
				s.len()
			));
		}
		total = total
			.checked_add(s.len())
			.filter(|t| *t <= MAX_TOTAL)
			.ok_or_else(|| format!("{op}: more than {MAX_TOTAL} bytes of text"))?;
	}
	interchange::check_shape(n, hidden).map_err(|e| format!("{op}: {e}"))?;
	Ok(())
}

/// The shared preflight, then both stages inside 0129's joined worker.
/// The error is the one sequential `embed` meets first: the lowest failing
/// batch across both stages (execution failures occur only below a
/// planning failure's index, so they win when present).
fn embed_strs(
	this: &TextEncoder,
	strs: &[&str],
	caps: Caps,
	knobs: &Knobs,
) -> (Result<Dense, String>, Trace) {
	let op = "TextEncoder::embed";
	let inner = &*this.0;
	let n = strs.len();
	let hidden = inner.config.hidden_size;
	let mut trace = Trace::default();
	if let Err(e) = preflight(strs, hidden, op) {
		return (Err(e), trace);
	}
	let agg = knobs.agg.unwrap_or(AGG);
	let workers = knobs
		.workers
		.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |p| p.get()));
	let ran = worker::run(op, || -> Result<(Vec<f32>, Trace), (String, Box<Trace>)> {
		let mut trace = Trace::default();
		let started = std::time::Instant::now();
		let (planned, plan_err) = plan(inner, strs, caps, agg, knobs, op);
		trace.plan_s = started.elapsed().as_secs_f64();
		trace.batches = planned.iter().map(|p| p.b).collect();
		let rows: usize = planned.iter().map(|p| p.b).sum();
		let mut out = vec![0f32; rows * hidden];
		let started = std::time::Instant::now();
		execute(
			inner,
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
	let (out, t) = match ran {
		Ok(Ok(v)) => v,
		Ok(Err((e, t))) => return (Err(e), *t),
		Err(e) => return (Err(e), trace),
	};
	trace = t;
	#[cfg(feature = "test-support")]
	{
		*LAST_BATCHES.lock().unwrap_or_else(|e| e.into_inner()) = trace.batches.clone();
		*LAST_THREADS.lock().unwrap_or_else(|e| e.into_inner()) = trace.threads;
	}
	let names = (0..hidden).map(|i| format!("e{i}")).collect();
	(
		Dense::new(Data::F32(Arc::new(out)), n, hidden, names).map_err(|e| format!("{op}: {e}")),
		trace,
	)
}

fn embed(this: &TextEncoder, texts: rune::Value) -> Result<Dense, String> {
	embed_value(this, &texts, CAPS)
}

#[cfg(feature = "test-support")]
static LAST_BATCHES: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

#[cfg(feature = "test-support")]
static LAST_THREADS: std::sync::Mutex<(usize, usize)> = std::sync::Mutex::new((0, 0));

/// Test support (the probe): (Candle's matmul thread count, the rayon pool)
/// seen inside a batch of the last successful `embed`.
#[cfg(feature = "test-support")]
pub fn last_threads() -> (usize, usize) {
	*LAST_THREADS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Test support (the probe): the batch sizes of the last successful `embed`.
#[cfg(feature = "test-support")]
pub fn last_batches() -> Vec<usize> {
	LAST_BATCHES
		.lock()
		.unwrap_or_else(|e| e.into_inner())
		.clone()
}

/// Test support: `embed` with lowered caps, so a batch halves at the pinned
/// model (whose batches of 32 fit every production cap).
#[cfg(feature = "test-support")]
pub fn embed_with_caps(this: &TextEncoder, texts: &[&str], caps: Caps) -> Result<Dense, String> {
	embed_strs(this, texts, caps, &Knobs::default()).0
}

/// Test support: `embed` with per-call knobs, returning its trace.
#[cfg(feature = "test-support")]
pub fn embed_with(
	this: &TextEncoder,
	texts: &[&str],
	caps: Caps,
	knobs: &Knobs,
) -> (Result<Dense, String>, Trace) {
	embed_strs(this, texts, caps, knobs)
}

/// Rust callers (the probe's twin of the script): the production path.
pub fn load_dir(dir: &str) -> Result<TextEncoder, String> {
	load(dir)
}
pub fn embed_texts(this: &TextEncoder, texts: &[&str]) -> Result<Dense, String> {
	embed_strs(this, texts, CAPS, &Knobs::default()).0
}

/// A row scaled by its largest magnitude, then divided by its norm, summed
/// in f64: no overflow at `f32::MAX`, no false zero for subnormals.
fn unit_rows(block: &Dense, side: &str, op: &str) -> Result<Vec<f64>, String> {
	let width = block.columns();
	let values: Vec<f64> = match block.data() {
		Data::F32(v) => v.iter().map(|&x| x as f64).collect(),
		Data::F64(v) => v.as_ref().clone(),
	};
	let mut out = Vec::with_capacity(values.len());
	for (r, row) in values.chunks(width).enumerate() {
		let scale = row.iter().fold(0f64, |m, x| m.max(x.abs()));
		if scale == 0.0 {
			return Err(format!("{op}: row {r} of {side} is zero"));
		}
		let norm = row
			.iter()
			.map(|x| (x / scale) * (x / scale))
			.sum::<f64>()
			.sqrt();
		out.extend(row.iter().map(|x| (x / scale) / norm));
	}
	Ok(out)
}

/// `candle::similarity(a, b, names)`: cosine of each row of `a` with each
/// row of `b`, `N × M`, one name per row of `b`.
fn similarity(a: &Dense, b: &Dense, names: rune::Value) -> Result<Dense, String> {
	let op = "similarity";
	if a.columns() != b.columns() {
		return Err(format!(
			"{op}: widths differ ({} and {})",
			a.columns(),
			b.columns()
		));
	}
	let names = interchange::names_from(&names, op, "names")?;
	similarity_named(a, b, names)
}

/// The same for Rust callers, with the names already owned and checked by
/// `Dense::new` at the end.
pub fn similarity_named(a: &Dense, b: &Dense, names: Vec<String>) -> Result<Dense, String> {
	let op = "similarity";
	if a.columns() != b.columns() {
		return Err(format!(
			"{op}: widths differ ({} and {})",
			a.columns(),
			b.columns()
		));
	}
	if names.len() != b.rows() {
		return Err(format!(
			"{op}: {} names for {} rows of b",
			names.len(),
			b.rows()
		));
	}
	// the shape (M at most 4,096) and then the names' own limits, before
	// any normalization or product
	interchange::check_shape(a.rows(), b.rows()).map_err(|e| format!("{op}: {e}"))?;
	interchange::check_names(&names, names.len()).map_err(|e| format!("{op}: {e}"))?;
	let f64_ = a.dtype() == "f64" || b.dtype() == "f64";
	let (n, m, w) = (a.rows(), b.rows(), a.columns());
	let ua = unit_rows(a, "a", op)?;
	let ub = unit_rows(b, "b", op)?;
	let data = worker::run(op, || -> Result<Data, String> {
		let e = |e: candle_core::Error| format!("{op}: {e}");
		let cpu = &Device::Cpu;
		if f64_ {
			let ta = CTensor::from_vec(ua, (n, w), cpu).map_err(e)?;
			let tb = CTensor::from_vec(ub, (m, w), cpu).map_err(e)?;
			let r = ta.matmul(&tb.t().map_err(e)?).map_err(e)?;
			Ok(Data::F64(Arc::new(
				r.flatten_all().map_err(e)?.to_vec1::<f64>().map_err(e)?,
			)))
		} else {
			let ta = CTensor::from_vec(
				ua.iter().map(|&x| x as f32).collect::<Vec<_>>(),
				(n, w),
				cpu,
			)
			.map_err(e)?;
			let tb = CTensor::from_vec(
				ub.iter().map(|&x| x as f32).collect::<Vec<_>>(),
				(m, w),
				cpu,
			)
			.map_err(e)?;
			let r = ta.matmul(&tb.t().map_err(e)?).map_err(e)?;
			Ok(Data::F32(Arc::new(
				r.flatten_all().map_err(e)?.to_vec1::<f32>().map_err(e)?,
			)))
		}
	})??;
	Dense::new(data, n, m, names).map_err(|e| format!("{op}: {e}"))
}

/// A bounded one-line summary.
pub fn summary(this: &TextEncoder) -> String {
	let c = &this.0.config;
	format!(
		"TextEncoder[bert; {} layers, {} dims, max {} tokens]",
		c.num_hidden_layers, c.hidden_size, this.0.max_seq
	)
}
fn display(this: &TextEncoder, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
	use rune::alloc::fmt::TryWrite;
	rune::vm_try!(f.try_write_str(&summary(this)));
	rune::runtime::VmResult::Ok(())
}

pub(crate) fn build(
	m: &mut rune::Module,
) -> Result<Vec<(String, &'static str)>, rune::ContextError> {
	m.ty::<TextEncoder>()?;
	m.function("load", load).build_associated::<TextEncoder>()?;
	m.associated_function("embed", embed)?;
	m.associated_function(&rune::runtime::Protocol::DISPLAY_FMT, display)?;
	m.function("similarity", similarity).build()?;
	Ok(vec![
		(
			"candle::TextEncoder".into(),
			"TextEncoder: a sentence-transformers BERT encoder (mean pooling, normalized) on the CPU",
		),
		(
			"candle::TextEncoder::load".into(),
			"load(dir) -> Result<TextEncoder>: config.json, modules.json, the pooling config, sentence_bert_config.json, tokenizer.json and model.safetensors from a local directory",
		),
		(
			"candle::TextEncoder::embed".into(),
			"embed(texts) -> Result<interchange::Dense>: 1 to 32,768 texts (64 KiB each, 16 MiB in all) as unit rows, N x hidden, f32",
		),
		(
			"candle::similarity".into(),
			"similarity(a, b, names) -> Result<interchange::Dense>: the cosine of every row of a with every row of b, N x M, one name per row of b",
		),
	])
}

pub(crate) fn present(presenters: &mut rnx::Presenters) -> Result<(), String> {
	presenters.register::<TextEncoder>(|t, out| {
		out.push(&summary(t));
		Ok(())
	})
}

#[cfg(test)]
mod tests {
	//! A tiny generated BERT model (hidden 8, 2 heads, 1 layer, vocabulary
	//! 16) and a word-level tokenizer with BERT's post-processor, so every
	//! contract refusal is tested without the pinned 90 MB download.
	use super::*;
	use serde_json::json;
	use std::path::{Path, PathBuf};

	fn fresh(tag: &str) -> PathBuf {
		static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		let d =
			std::env::temp_dir().join(format!("rnx-0131-text-{}/{tag}-{n}", std::process::id()));
		std::fs::create_dir_all(d.join("1_Pooling")).unwrap();
		d
	}

	fn config() -> serde_json::Value {
		json!({
			"model_type": "bert", "hidden_act": "gelu", "position_embedding_type": "absolute",
			"vocab_size": 16, "hidden_size": 8, "num_hidden_layers": 1, "num_attention_heads": 2,
			"intermediate_size": 16, "hidden_dropout_prob": 0.0, "max_position_embeddings": 32,
			"type_vocab_size": 2, "initializer_range": 0.02, "layer_norm_eps": 1e-12, "pad_token_id": 0
		})
	}

	fn tokenizer(vocab: serde_json::Value, sep_id: u32) -> serde_json::Value {
		json!({
			"version": "1.0", "truncation": null, "padding": null, "added_tokens": [],
			"normalizer": null, "pre_tokenizer": {"type": "WhitespaceSplit"},
			"post_processor": {"type": "BertProcessing", "sep": ["[SEP]", sep_id], "cls": ["[CLS]", 2]},
			"decoder": null,
			"model": {"type": "WordLevel", "vocab": vocab, "unk_token": "[UNK]"}
		})
	}

	fn vocab() -> serde_json::Value {
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
		serde_json::Value::Object(v)
	}

	fn write(d: &Path, name: &str, v: &serde_json::Value) {
		std::fs::write(d.join(name), v.to_string()).unwrap();
	}

	/// A complete model directory; `edit` changes one file before writing.
	fn model(edit: impl FnOnce(&Path, &mut serde_json::Value)) -> PathBuf {
		let d = fresh("m");
		let mut c = config();
		write(
			&d,
			"modules.json",
			&json!([
				{"idx": 0, "name": "0", "path": "", "type": TRANSFORMER},
				{"idx": 1, "name": "1", "path": "1_Pooling", "type": POOLING},
				{"idx": 2, "name": "2", "path": "2_Normalize", "type": NORMALIZE}
			]),
		);
		write(
			&d,
			"1_Pooling/config.json",
			&json!({"word_embedding_dimension": 8, "pooling_mode_cls_token": false,
				"pooling_mode_mean_tokens": true, "pooling_mode_max_tokens": false}),
		);
		write(
			&d,
			"sentence_bert_config.json",
			&json!({"max_seq_length": 16, "do_lower_case": false}),
		);
		write(&d, "tokenizer.json", &tokenizer(vocab(), 3));
		let varmap = candle_nn::VarMap::new();
		let typed: Config = serde_json::from_value(c.clone()).unwrap();
		BertModel::load(
			VarBuilder::from_varmap(&varmap, DType::F32, &Device::Cpu),
			&typed,
		)
		.unwrap();
		varmap.save(d.join("model.safetensors")).unwrap();
		edit(&d, &mut c);
		write(&d, "config.json", &c);
		d
	}

	fn loads(d: &Path) -> Result<TextEncoder, String> {
		load(d.to_str().unwrap())
	}
	fn refused(d: PathBuf, want: &str) {
		let e = loads(&d).err().unwrap();
		assert!(e.contains(want), "{e} (wanted {want})");
	}
	fn list(v: &[&str]) -> rune::Value {
		rune::to_value(v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
	}

	#[test]
	fn the_fixture_embeds_unit_rows_and_its_inputs_stay_usable() {
		let enc = loads(&model(|_, _| {})).unwrap();
		assert_eq!(
			summary(&enc),
			"TextEncoder[bert; 1 layers, 8 dims, max 16 tokens]"
		);
		let texts = list(&["the cat sat", "a dog ran far away", "", "zebra"]);
		let d = embed(&enc, texts.clone()).unwrap();
		assert_eq!((d.rows(), d.columns(), d.dtype()), (4, 8, "f32"));
		let Data::F32(v) = d.data() else {
			unreachable!()
		};
		for row in v.chunks(8) {
			let n: f32 = row.iter().map(|x| x * x).sum::<f32>().sqrt();
			assert!((n - 1.0).abs() < 1e-5, "{n}");
		}
		// the list after success and after a refusal
		assert!(embed(&enc, list(&[])).is_err());
		assert_eq!(rune::from_value::<Vec<String>>(texts).unwrap().len(), 4);
		// the same text alone and inside a batch
		let alone = embed(&enc, list(&["the cat sat"])).unwrap();
		let Data::F32(a) = alone.data() else {
			unreachable!()
		};
		assert!(a.iter().zip(&v[..8]).all(|(x, y)| (x - y).abs() < 1e-6));
	}

	#[test]
	fn the_config_contract_refuses_by_name() {
		refused(
			model(|_, c| c["model_type"] = json!("roberta")),
			"model_type is \"roberta\"",
		);
		refused(
			model(|_, c| c["hidden_act"] = json!("relu")),
			"hidden_act is \"relu\"",
		);
		refused(
			model(|_, c| c["position_embedding_type"] = json!("relative_key")),
			"position_embedding_type",
		);
		refused(
			model(|_, c| c["num_attention_heads"] = json!(0)),
			"num_attention_heads = 0",
		);
		refused(
			model(|_, c| c["num_attention_heads"] = json!(3)),
			"not divisible",
		);
		refused(
			model(|_, c| c["intermediate_size"] = json!(0)),
			"intermediate_size = 0",
		);
		refused(
			model(|_, c| c["intermediate_size"] = json!(4097)),
			"intermediate_size = 4097",
		);
		refused(
			model(|_, c| c["type_vocab_size"] = json!(0)),
			"type_vocab_size = 0",
		);
		refused(
			model(|_, c| c["type_vocab_size"] = json!(17)),
			"type_vocab_size = 17",
		);
		refused(
			model(|_, c| c["num_hidden_layers"] = json!(25)),
			"num_hidden_layers = 25",
		);
		refused(
			model(|_, c| c["vocab_size"] = json!(250_001)),
			"vocab_size = 250001",
		);
		refused(
			model(|_, c| c["max_position_embeddings"] = json!(513)),
			"max_position_embeddings = 513",
		);
		refused(
			model(|_, c| c["pad_token_id"] = json!(16)),
			"pad_token_id 16 is not below",
		);
		refused(
			model(|_, c| c["hidden_size"] = json!(serde_json::Value::Null)),
			"config.json",
		);
	}

	#[test]
	fn the_module_pooling_and_sequence_contract_refuses_by_name() {
		refused(
			model(|d, _| {
				write(
					d,
					"modules.json",
					&json!([{"type": TRANSFORMER, "path": ""}, {"type": NORMALIZE, "path": "2"}, {"type": POOLING, "path": "1_Pooling"}]),
				)
			}),
			"exactly Transformer, Pooling, Normalize",
		);
		refused(
			model(|d, _| {
				write(
					d,
					"1_Pooling/config.json",
					&json!({"word_embedding_dimension": 8, "pooling_mode_cls_token": true, "pooling_mode_mean_tokens": true}),
				)
			}),
			"pooling_mode_cls_token",
		);
		refused(
			model(|d, _| {
				write(
					d,
					"1_Pooling/config.json",
					&json!({"word_embedding_dimension": 16, "pooling_mode_mean_tokens": true}),
				)
			}),
			"pooling dimension",
		);
		refused(
			model(|d, _| {
				write(
					d,
					"sentence_bert_config.json",
					&json!({"max_seq_length": 1}),
				)
			}),
			"max_seq_length 1",
		);
		refused(
			model(|d, _| {
				write(
					d,
					"sentence_bert_config.json",
					&json!({"max_seq_length": 33}),
				)
			}),
			"max_seq_length 33",
		);
		// 2 fits the contract but leaves no room beside [CLS] and [SEP]
		refused(
			model(|d, _| {
				write(
					d,
					"sentence_bert_config.json",
					&json!({"max_seq_length": 2}),
				)
			}),
			"leaves no room beside 2 special tokens",
		);
		refused(
			model(|d, _| {
				write(
					d,
					"sentence_bert_config.json",
					&json!({"max_seq_length": 16, "do_lower_case": true}),
				)
			}),
			"do_lower_case",
		);
	}

	#[test]
	fn files_are_bounded_and_malformed_ones_refused() {
		refused(
			model(|d, _| std::fs::remove_file(d.join("tokenizer.json")).unwrap()),
			"tokenizer.json",
		);
		assert!(
			load("/no/such/model")
				.err()
				.unwrap()
				.contains("/no/such/model/config.json")
		);
		// an oversized file is refused from its metadata, before any read
		refused(
			model(|d, _| {
				std::fs::File::create(d.join("modules.json"))
					.unwrap()
					.set_len(MAX_JSON + 1)
					.unwrap()
			}),
			"bytes, at most 65536",
		);
		refused(
			model(|d, _| std::fs::write(d.join("1_Pooling/config.json"), "{not json").unwrap()),
			"1_Pooling/config.json",
		);
		refused(
			model(|d, _| {
				let p = d.join("model.safetensors");
				let b = std::fs::read(&p).unwrap();
				std::fs::write(&p, &b[..b.len() / 2]).unwrap()
			}),
			"model.safetensors",
		);
		// a half-precision weight would be converted silently by the VarBuilder
		refused(
			model(|d, _| {
				let p = d.join("model.safetensors");
				let mut t = candle_core::safetensors::load(&p, &Device::Cpu).unwrap();
				let w = t.remove("embeddings.word_embeddings.weight").unwrap();
				t.insert(
					"embeddings.word_embeddings.weight".into(),
					w.to_dtype(DType::F16).unwrap(),
				);
				candle_core::safetensors::save(&t, &p).unwrap();
			}),
			"is F16, want F32",
		);
	}

	fn retype(d: &Path, key: &str, dtype: DType) {
		let p = d.join("model.safetensors");
		let mut t = candle_core::safetensors::load(&p, &Device::Cpu).unwrap();
		let w = t.remove(key).unwrap();
		t.insert(key.into(), w.to_dtype(dtype).unwrap());
		candle_core::safetensors::save(&t, &p).unwrap();
	}

	#[test]
	fn every_weight_is_f32_except_the_cited_position_ids_buffer() {
		// an integer learned weight would be converted to F32 silently
		for dtype in [DType::I32, DType::I64, DType::U8] {
			refused(
				model(|d, _| retype(d, "embeddings.word_embeddings.weight", dtype)),
				&format!("embeddings.word_embeddings.weight is {dtype:?}, want F32"),
			);
		}
		// the pinned file's unused integer buffer loads, bare or prefixed
		for key in ["embeddings.position_ids", "bert.embeddings.position_ids"] {
			let d = model(|d, _| {
				let p = d.join("model.safetensors");
				let mut t = candle_core::safetensors::load(&p, &Device::Cpu).unwrap();
				t.insert(
					key.into(),
					CTensor::arange(0i64, 32, &Device::Cpu)
						.unwrap()
						.unsqueeze(0)
						.unwrap(),
				);
				candle_core::safetensors::save(&t, &p).unwrap();
			});
			assert!(loads(&d).is_ok(), "{key}");
		}
		// but not as a float of the wrong precision, nor under another name
		let d = model(|d, _| {
			let p = d.join("model.safetensors");
			let mut t = candle_core::safetensors::load(&p, &Device::Cpu).unwrap();
			t.insert(
				"embeddings.extra_ids".into(),
				CTensor::arange(0i64, 32, &Device::Cpu).unwrap(),
			);
			candle_core::safetensors::save(&t, &p).unwrap();
		});
		assert!(
			loads(&d)
				.err()
				.unwrap()
				.contains("embeddings.extra_ids is I64, want F32")
		);
	}

	#[test]
	fn the_rust_entry_points_share_the_preflight() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let check = |texts: &[&str], want: &str| {
			// the per-call trace: no batch was planned or run
			let (r, trace) = embed_strs(&enc, texts, CAPS, &Knobs::default());
			let e = r.unwrap_err();
			assert!(e.contains(want), "{e} (wanted {want})");
			assert!(trace.batches.is_empty() && trace.executed.is_empty());
			assert!(embed_texts(&enc, texts).unwrap_err().contains(want));
		};
		let big = "x".repeat(MAX_TEXT + 4);
		check(&[&big], "text 0 is 65540 bytes, at most 65536");
		let full = "x".repeat(MAX_TEXT);
		check(&vec![full.as_str(); 257], "more than 16777216 bytes");
		check(&[], "0 texts");
		check(&vec!["a"; MAX_TEXTS + 1], "32769 texts");
		// the test-support entry point too
		#[cfg(feature = "test-support")]
		assert!(
			embed_with_caps(&enc, &[&big], CAPS)
				.unwrap_err()
				.contains("65540 bytes")
		);
		// similarity_named: the names' limits before any arithmetic
		let b = block(vec![1.0, 2.0, 3.0, 4.0], 2, false);
		for (names, want) in [
			(
				vec!["q".to_string(), "q".to_string()],
				"duplicate column name",
			),
			(
				vec!["q".to_string(), String::new()],
				"1 to 256 bytes, found 0",
			),
			(vec!["q".to_string(), "n".repeat(257)], "found 257"),
		] {
			let e = similarity_named(&b, &b, names).unwrap_err();
			assert!(e.contains(want), "{e} (wanted {want})");
		}
	}

	#[test]
	fn token_ids_are_checked_by_value() {
		// a sparse vocabulary: 16 entries, one of them id 99
		refused(
			model(|d, _| {
				let mut v = vocab();
				v["now"] = json!(99);
				write(d, "tokenizer.json", &tokenizer(v, 3));
			}),
			"tokenizer id 99 (\"now\") is not below vocab_size 16",
		);
		// the pad id must be a token
		refused(
			model(|d, _| {
				let mut v = vocab();
				v.as_object_mut().unwrap().remove("[PAD]");
				write(d, "tokenizer.json", &tokenizer(v, 3));
			}),
			"pad_token_id 0 is not in the tokenizer",
		);
		// the post-processor emits [SEP] as id 40, outside the vocabulary:
		// the load can't see it, the batch's id check refuses it
		let enc = loads(&model(|d, _| {
			write(d, "tokenizer.json", &tokenizer(vocab(), 40))
		}))
		.unwrap();
		let e = embed(&enc, list(&["the cat"])).unwrap_err();
		assert!(e.contains("token id 40 is not below vocab_size 16"), "{e}");
	}

	#[test]
	fn texts_are_checked_borrowed_before_any_copy() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let refuse = |v: rune::Value, want: &str| {
			let e = embed(&enc, v.clone()).unwrap_err();
			assert!(e.contains(want), "{e} (wanted {want})");
			// still usable afterwards
			drop(rune::from_value::<rune::Value>(v).unwrap());
		};
		refuse(list(&[]), "0 texts, want 1 to 32768");
		refuse(
			rune::to_value(vec!["a".to_string(); MAX_TEXTS + 1]).unwrap(),
			"32769 texts",
		);
		refuse(
			rune::to_value(vec!["x".repeat(MAX_TEXT + 1)]).unwrap(),
			"text 0 is 65537 bytes",
		);
		refuse(
			rune::to_value(vec!["x".repeat(MAX_TEXT); 257]).unwrap(),
			"more than 16777216 bytes",
		);
		refuse(
			rune::to_value("the cat".to_string()).unwrap(),
			"must be a vector of strings",
		);
		let mixed = rune::to_value(vec![
			rune::to_value("the").unwrap(),
			rune::to_value(1i64).unwrap(),
		])
		.unwrap();
		refuse(mixed, "must be a vector of strings");
	}

	#[test]
	fn activation_caps_halve_batches_or_refuse() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let inner = &*enc.0;
		assert!(fits(&inner.config, 32, 16, CAPS));
		// attention: 2 heads x 16^2 = 512 per text at the maximum length
		let caps = Caps {
			hidden: 1 << 22,
			ffn: 1 << 24,
			attention: 512 * 3,
		};
		assert!(fits(&inner.config, 3, 16, caps) && !fits(&inner.config, 4, 16, caps));
		let long = vec!["the cat sat on the mat a dog ran far away home now the cat"; 10];
		let halved = embed_strs(&enc, &long, caps, &Knobs::default()).0.unwrap();
		let whole = embed_strs(&enc, &long, CAPS, &Knobs::default()).0.unwrap();
		let (Data::F32(a), Data::F32(b)) = (halved.data(), whole.data()) else {
			unreachable!()
		};
		assert!(a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-6));
		let none = Caps {
			hidden: 1 << 22,
			ffn: 1 << 24,
			attention: 511,
		};
		let e = embed_strs(&enc, &long[..1], none, &Knobs::default())
			.0
			.unwrap_err();
		assert!(e.contains("exceeds the activation caps"), "{e}");
		// overflowing products are refusals, not wraps
		assert!(!fits(&inner.config, usize::MAX, 16, CAPS));
	}

	fn block(v: Vec<f64>, rows: usize, f64_: bool) -> Dense {
		let w = v.len() / rows;
		let names = (0..w).map(|i| format!("c{i}")).collect();
		let data = if f64_ {
			Data::F64(Arc::new(v))
		} else {
			Data::F32(Arc::new(v.into_iter().map(|x| x as f32).collect()))
		};
		Dense::new(data, rows, w, names).unwrap()
	}
	fn names(n: usize) -> rune::Value {
		rune::to_value((0..n).map(|i| format!("q{i}")).collect::<Vec<_>>()).unwrap()
	}
	fn values(d: &Dense) -> Vec<f64> {
		match d.data() {
			Data::F32(v) => v.iter().map(|&x| x as f64).collect(),
			Data::F64(v) => v.as_ref().clone(),
		}
	}

	#[test]
	fn similarity_is_stable_at_the_extremes() {
		let max = f32::MAX as f64;
		let tiny = f32::from_bits(1) as f64; // the smallest subnormal
		let a = block(
			vec![max, max, 0.0, tiny, tiny, 0.0, 3.0, -4.0, 0.0],
			3,
			false,
		);
		let b = block(vec![1.0, 1.0, 0.0, 0.0, 1.0, 0.0], 2, false);
		let s = similarity(&a, &b, names(2)).unwrap();
		assert_eq!((s.rows(), s.columns(), s.dtype()), (3, 2, "f32"));
		let want = [
			1.0,
			0.5f64.sqrt(),
			1.0,
			0.5f64.sqrt(),
			-0.1 * 2f64.sqrt(),
			-0.8,
		];
		for (got, want) in values(&s).iter().zip(want) {
			assert!((got - want).abs() < 1e-6, "{got} vs {want}");
		}
		// f64 in either input computes and returns f64
		let a64 = block(vec![1.0, 2.0], 1, true);
		let b32 = block(vec![2.0, 4.0], 1, false);
		let s = similarity(&a64, &b32, names(1)).unwrap();
		assert_eq!(s.dtype(), "f64");
		assert!((values(&s)[0] - 1.0).abs() < 1e-15);
	}

	#[test]
	fn similarity_refuses_before_computing_and_keeps_its_arguments() {
		let a = block(vec![1.0, 0.0, 0.0, 0.0], 2, false);
		let b = block(vec![1.0, 1.0], 1, false);
		let e = similarity(&a, &b, names(1)).unwrap_err();
		assert!(e.contains("row 1 of a is zero"), "{e}");
		let z = block(vec![0.0, 0.0], 1, false);
		let e = similarity(&b, &z, names(1)).unwrap_err();
		assert!(e.contains("row 0 of b is zero"), "{e}");
		let w3 = block(vec![1.0, 2.0, 3.0], 1, false);
		assert!(
			similarity(&b, &w3, names(1))
				.unwrap_err()
				.contains("widths differ")
		);
		assert!(
			similarity(&b, &b, names(2))
				.unwrap_err()
				.contains("2 names for 1 rows")
		);
		// M above 4,096 columns, through Dense's shape check
		let many = block(vec![1.0; 2 * 4097], 4097, false);
		let e = similarity(&b.clone(), &many, names(4097)).unwrap_err();
		assert!(e.contains("4097 names, at most 4096"), "{e}");
		// the arguments are reusable
		let n = names(1);
		similarity(&b, &b, n.clone()).unwrap();
		assert_eq!(rune::from_value::<Vec<String>>(n).unwrap(), ["q0"]);
		assert_eq!(b.rows(), 1);
	}

	// ---- record 0132: concurrent batches ----

	fn words(n: usize, salt: usize) -> String {
		let w = [
			"the", "cat", "sat", "on", "mat", "a", "dog", "ran", "far", "away", "home", "now",
		];
		(0..n)
			.map(|i| w[(i * 7 + salt) % w.len()])
			.collect::<Vec<_>>()
			.join(" ")
	}
	fn with(enc: &TextEncoder, texts: &[&str], knobs: Knobs) -> (Result<Dense, String>, Trace) {
		embed_strs(enc, texts, CAPS, &knobs)
	}
	fn bits(d: &Dense) -> Vec<u32> {
		match d.data() {
			Data::F32(v) => v.iter().map(|x| x.to_bits()).collect(),
			Data::F64(_) => unreachable!(),
		}
	}
	fn workers(w: usize) -> Knobs {
		Knobs {
			workers: Some(w),
			..Knobs::default()
		}
	}

	#[test]
	fn concurrent_batches_equal_the_sequential_order_bit_for_bit() {
		let enc = loads(&model(|_, _| {})).unwrap();
		// 300 texts of 1 to 14 words: 10 batches, long ones early so later
		// (shorter) batches tend to finish first
		let owned: Vec<String> = (0..300).map(|i| words(14 - (i * 13 / 300), i)).collect();
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		let (one, t1) = with(&enc, &texts, workers(1));
		let one = one.unwrap();
		assert_eq!(t1.workers, 1);
		assert_eq!(t1.batches.len(), 10);
		for w in [2, 8, 32] {
			let (d, t) = with(&enc, &texts, workers(w));
			assert_eq!(bits(&d.unwrap()), bits(&one), "W = {w}");
			assert_eq!((t.batches.clone(), t.inflight_end), (t1.batches.clone(), 0));
			assert_eq!(t.executed, (0..10).collect::<Vec<_>>());
		}
		let (d, t) = with(&enc, &texts, Knobs::default());
		assert_eq!(bits(&d.unwrap()), bits(&one));
		assert!(t.workers >= 1 && t.workers <= MAX_WORKERS);
		// a single batch runs on the outer worker, one worker
		let (_, t) = with(&enc, &texts[..5], Knobs::default());
		assert_eq!((t.workers, t.batches.clone()), (1, vec![5]));
	}

	#[test]
	fn a_512_token_model_plans_within_the_storage_bound_and_matches() {
		let enc = loads(&model(|d, c| {
			c["max_position_embeddings"] = json!(512);
			write(
				d,
				"sentence_bert_config.json",
				&json!({"max_seq_length": 512}),
			);
			// the weights must match the new position count
			let typed: Config = serde_json::from_value(c.clone()).unwrap();
			let varmap = candle_nn::VarMap::new();
			BertModel::load(
				VarBuilder::from_varmap(&varmap, DType::F32, &Device::Cpu),
				&typed,
			)
			.unwrap();
			varmap.save(d.join("model.safetensors")).unwrap();
		}))
		.unwrap();
		assert_eq!(enc.0.max_seq, 512);
		let owned: Vec<String> = (0..40).map(|i| words(600 + i, i)).collect();
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		let (one, t1) = with(&enc, &texts, workers(1));
		let (four, _) = with(&enc, &texts, workers(4));
		assert_eq!(bits(&one.unwrap()), bits(&four.unwrap()));
		// every planned batch is at the full 512 tokens, within the bound
		let planned: usize = t1.batches.iter().map(|b| b * 512 * 8).sum();
		assert!(planned <= PLAN_PAYLOAD, "{planned}");
		assert_eq!(PLAN_PAYLOAD, 128 << 20);
		assert_eq!(PLAN_METADATA, std::mem::size_of::<Planned>() * MAX_TEXTS);
	}

	fn batches_of_32(enc: &TextEncoder, n: usize) -> Vec<String> {
		let _ = enc;
		(0..n).map(|i| words(3 + i % 5, i)).collect()
	}

	#[test]
	fn the_lowest_failing_batch_wins_and_everything_joins() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let owned = batches_of_32(&enc, 320);
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		let fail = |knobs: Knobs, want: &str| {
			let (r, t) = with(
				&enc,
				&texts,
				Knobs {
					workers: Some(8),
					..knobs
				},
			);
			let e = r.unwrap_err();
			assert!(e.contains(want), "{e} (wanted {want})");
			assert_eq!(t.inflight_end, 0, "the budget is fully released");
			t
		};
		fail(
			Knobs {
				fail_at: vec![3],
				..Knobs::default()
			},
			"injected failure at batch 3",
		);
		fail(
			Knobs {
				fail_at: vec![6, 3],
				..Knobs::default()
			},
			"injected failure at batch 3",
		);
		fail(
			Knobs {
				panic_at: vec![2],
				..Knobs::default()
			},
			"Candle panicked",
		);
		// across the two stages: an execution failure below a planning
		// failure wins; a planning failure below an execution failure wins
		fail(
			Knobs {
				plan_fail_at: Some(5),
				fail_at: vec![2],
				..Knobs::default()
			},
			"injected failure at batch 2",
		);
		let t = fail(
			Knobs {
				plan_fail_at: Some(2),
				fail_at: vec![5],
				..Knobs::default()
			},
			"injected planning failure at batch 2",
		);
		assert_eq!(t.batches.len(), 2, "planning stopped at batch 2");
		assert!(t.executed.iter().all(|&i| i < 2), "batch 5 never ran");
		// the planned prefix before a planning failure still executes
		let t = fail(
			Knobs {
				plan_fail_at: Some(4),
				..Knobs::default()
			},
			"batch 4",
		);
		assert_eq!(t.executed, vec![0, 1, 2, 3]);
	}

	#[test]
	fn a_lower_batch_held_for_the_budget_still_runs_after_a_higher_one_fails() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let owned = batches_of_32(&enc, 320);
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		for _ in 0..20 {
			// batch 3 is held at admission until a higher index has failed;
			// batch 6 fails first, then 3 runs and fails: 3's error is returned
			let (r, t) = with(
				&enc,
				&texts,
				Knobs {
					workers: Some(4),
					hold: Some(3),
					fail_at: vec![3, 6],
					..Knobs::default()
				},
			);
			assert!(r.unwrap_err().contains("injected failure at batch 3"));
			assert_eq!(
				t.fail_order,
				vec![6, 3],
				"6 failed while 3 was held, then 3 ran"
			);
			assert_eq!(t.inflight_end, 0);
		}
	}

	#[test]
	fn the_in_flight_budget_bounds_concurrency_and_refuses_what_cannot_fit() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let owned = batches_of_32(&enc, 640);
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		let (_, t) = with(&enc, &texts, workers(8));
		let largest = {
			let inner = &*enc.0;
			let (planned, _) = plan(inner, &texts, CAPS, AGG, &Knobs::default(), "t");
			planned.iter().map(|p| p.estimate).max().unwrap()
		};
		// one batch at a time
		let (r, t1) = with(
			&enc,
			&texts,
			Knobs {
				workers: Some(8),
				agg: Some(largest),
				..Knobs::default()
			},
		);
		assert!(r.is_ok());
		assert_eq!(t1.max_running, 1);
		assert!(t1.max_inflight <= largest && t1.inflight_end == 0);
		// two at a time at most
		let (_, t2) = with(
			&enc,
			&texts,
			Knobs {
				workers: Some(8),
				agg: Some(2 * largest),
				..Knobs::default()
			},
		);
		assert!(t2.max_running <= 2 && t2.max_inflight <= 2 * largest);
		assert!(t.max_inflight <= AGG);
		// a batch that can't fit is refused once its shape is known, during
		// planning; the batches before it still ran, and nothing waits
		let (r, t) = with(
			&enc,
			&texts,
			Knobs {
				workers: Some(8),
				agg: Some(largest - 1),
				..Knobs::default()
			},
		);
		let e = r.unwrap_err();
		assert!(e.contains("above the in-flight budget"), "{e}");
		assert_eq!(t.inflight_end, 0);
	}

	#[test]
	fn an_unwind_after_admission_releases_the_budget_and_terminates() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let owned = batches_of_32(&enc, 320);
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		let largest = {
			let (planned, _) = plan(&enc.0, &texts, CAPS, AGG, &Knobs::default(), "t");
			planned.iter().map(|p| p.estimate).max().unwrap()
		};
		for _ in 0..10 {
			// one batch at a time, so others wait for the budget while batch 2
			// panics after its model step, before publishing its rows
			let (r, t) = with(
				&enc,
				&texts,
				Knobs {
					workers: Some(4),
					agg: Some(largest),
					panic_after_run: vec![2],
					..Knobs::default()
				},
			);
			let e = r.unwrap_err();
			assert!(e.contains("Candle panicked"), "{e}");
			assert_eq!(t.failed.first().map(|f| f.0), Some(2));
			assert_eq!(t.inflight_end, 0, "the permit released the reservation");
			assert_eq!(t.max_running, 1);
		}
	}

	#[test]
	fn later_batches_finishing_first_keep_the_input_order() {
		let enc = loads(&model(|_, _| {})).unwrap();
		let owned = batches_of_32(&enc, 192);
		let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
		let (one, _) = with(&enc, &texts, workers(1));
		// batch 0 sleeps before publishing, so batches 1.. complete first
		let (d, t) = with(
			&enc,
			&texts,
			Knobs {
				workers: Some(4),
				delay: vec![(0, 200)],
				..Knobs::default()
			},
		);
		assert_ne!(
			t.completion_order.first(),
			Some(&0),
			"{:?}",
			t.completion_order
		);
		assert_eq!(
			t.completion_order.last(),
			Some(&0),
			"{:?}",
			t.completion_order
		);
		assert_eq!(bits(&d.unwrap()), bits(&one.unwrap()));
	}
}
