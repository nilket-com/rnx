# rnx 0149: deterministic text generation (U7), triage and one-line summaries against 0139's frozen sample

Status: plan. The user (2026-10-02) approved the next cut of the Candle widening: a model family, paired with an analyst workflow. 0146 re-ranked, 0148 classified by entailment. **The family still missing is generation:** a small instruct model writing short answers.

**What it adds for an analyst:**
- a one-line summary of each ticket;
- the same frozen triage task, answered by a generative model: a **third method on 0139's frozen sample,** beside 0139's embeddings and 0148's NLI.

**Decoding is greedy, so the record stays deterministic.** Randomness remains deferred, as agreed, and Rust parity can be exact token for token.

**Amended after Codex's review (round 1), before any generation:**
- **R1:** a 3a failure is diagnosed at its first differing token, not attributed to tokenization; logits must be finite; synthetic divergence controls.
- **R2:** `chat_many`'s resource and lifecycle contract is explicit (section 3a).
- **R3:** the retained generation trace is specified and validated per step (section 4a).
- **Wording:** the pinned *inactive* sliding window is admitted exactly.

**Carried from 0148's review:**
- **both** Hugging Face checks are declared here before anything runs: the native end-to-end one, and the model-level one on rnx's own ids;
- every expected-outcome check writes a completion artifact with a dedicated exit status;
- decisions at a boundary are allowed only where a perturbation within the stated tolerance explains them.

## 1. Before any generation: what is frozen, and what has been looked at

**Nothing has been generated.** No model has generated from any D2 text. The files below were fetched and hashed, and their configurations read.

**Unchanged from 0139 and 0148,** each checked by SHA-256 at gate 0, with the same hashes as plan 0148 section 1:
- 0139's frozen `labels.tsv`, `rules.md`, `sample.tsv` and both annotations;
- 0139's baseline `u4-session.tsv`;
- 0136's `ranges-o0.tsv`;
- **0148's U6 ticket table** (`probes/0148/out/u6-script/u6-tickets.tsv`, at `5af908f`), as the NLI comparison.

D2 itself is checked against 0133's manifest.

## 2. The model

**`Qwen/Qwen2.5-0.5B-Instruct`** (Apache-2.0), pinned to revision `7ae557604adf67be50417f59c2c2f167def9a775`.
- **Architecture:** `Qwen2ForCausalLM`:
  - 24 layers, hidden 896, 14 query heads and 2 key-value heads (head size 64), intermediate 4,864;
  - SiLU, RMSNorm with eps 1e-6, RoPE with θ 10⁶;
  - vocabulary 151,936, with **tied** input and output embeddings;
  - no sliding window (`use_sliding_window: false`).
- **The weights:** 290 tensors, **all BF16**, 988,097,824 bytes. They're computed in **F32**, about 2 GB resident.
- **Candle 0.11 has it:** `candle_transformers::models::qwen2::ModelForCausalLM`, with a per-layer KV cache. The model is `Clone` with shared weights, and each clone carries its own cache.
- **The pinned generation defaults sample** (temperature 0.7, top-p 0.8, top-k 20, repetition penalty 1.1). **This record ignores them and decodes greedily** (section 4).
- **`probes/0149/fetch.sh` fetches and checks:**

| file | SHA-256 |
|---|---|
| `config.json` | `18e18afcaccafade98daf13a54092927904649e1dd4eba8299ab717d5d94ff45` |
| `generation_config.json` | `e558847a8b4402616f1273797b015104dc266fe4b520056fca88823ba8f8ebe6` |
| `tokenizer.json` | `c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539` |
| `tokenizer_config.json` (the chat template, checked against) | `5b5d4f65d0acd3b2d56a35b56d374a36cbc1c8fa5cf3b3febbbfabf22f359583` |
| `model.safetensors` | `fdf756fa7fcbe7404d5c60e26bff1a0c8b8aa1f72ced49e7dd0210fe288fb7fe` |

## 3. The binding: `candle::TextGenerator`

**`TextGenerator::load(dir)`**, with 0148's closed-contract pattern:
- **The configuration:**
  - exactly the production values above, with `hidden_act` `silu` and `tie_word_embeddings` true;
  - a fixture geometry admitted only under `test-support`;
  - unknown keys refused by name, except an allowlist of inert metadata;
  - **the sliding window, admitted exactly as pinned:** `sliding_window` 32768, `max_window_layers` 21 and `use_sliding_window` false. Candle builds its causal mask with `sliding_window`; at 32,768 it's far above this record's 1,280-token bound, so the window is inactive. Any other value, or enabling it, is refused by name (review round 1);
  - `rope_scaling`, quantization and an untied head refused by name.
- **The tokenizer:**
  - the special tokens `<|endoftext|>` 151643, `<|im_start|>` 151644 and `<|im_end|>` 151645, each defined as exactly itself;
  - every id below `vocab_size`;
  - truncation and padding disabled;
  - **no post-processor adding tokens:** the chat template supplies them.
- **The weights:**
  - the exact key set (no `lm_head`, since it's tied) and shapes;
  - all BF16, with the byte total checked against the file's data section before any tensor is read;
  - a model-specific file bound of 1 GiB (the shared `MAX_WEIGHTS` is 512 MiB);
  - converted to F32 once, at load.

**`gen.chat(system, user, max_new_tokens)`** returns `#{ text, ids, prompt_tokens, stop }`. `stop` is `"eos"` or `"length"`.
- **The prompt** is exactly Qwen's template for one system and one user message, with the generation prompt: `<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n`. It's built by rnx and tokenized with special tokens recognized, and a gate checks it against `apply_chat_template` (section 6).
- **Greedy decoding:** each step takes the argmax of the F32 logits, an exact tie going to the lowest id. There's no repetition penalty, temperature or top-k/p.
- **Stopping:** generation stops at `<|im_end|>` or `<|endoftext|>` (neither appears in `text`), or after `max_new_tokens`.
- **The bounds, checked before anything proportional is allocated:**
  - the borrowed strings: each at most `MAX_TEXT`, and the system and user messages combined;
  - the prompt at most **1,024 tokens**, refused by its count, never truncated;
  - `max_new_tokens` from 1 to **256**.
- **`gen.chat_many(systems, users, max_new_tokens)`** runs independent generations concurrently. Each uses its own model clone (shared weights, its own cache), admitted against the in-flight budget with 0135's S1-style accounting. Results are in input order, with the lowest-index error otherwise.
- **The memory estimate:**
  - prompt prefill: the [1, 14, s, s] scores, softmax and mask, q/k/v and the repeated key-value heads, the MLP [s, 4864], residuals, and the logits [1, 151936];
  - each decode step: the same at length 1, against the cache;
  - the KV cache at its final length: 24 layers × 2 × [1, 2, s_total, 64].
  - **Each term is cited to Candle 0.11's `qwen2.rs`.**
  - **Calibrated fail-closed, as 0148:** prompt lengths {16, 256, 1,024} × new tokens {1, 64, 256}, alone and concurrently, with `max_running` reported, peak ≤ estimate, a lowered-limit refusal and a minimum-budget completion.
- **Controls:**
  - unit tests on a tiny generated Qwen2 fixture: contract refusals by name, the bounds, a greedy tie to the lowest id, and stopping at each EOS and at the length;
  - allocation refusals under 256 KiB;
  - the template check.

## 3a. `chat_many`: resources and lifecycle (review round 1, R2)

**Checked before each proportional allocation, in this order:**
1. **The borrowed inputs:**
   - the request count (1 to 1,024) and equal lengths;
   - each system and user string at most `MAX_TEXT`;
   - the combined bytes of all of them at most `MAX_TOTAL`;
   - `max_new_tokens` from 1 to 256.
2. **The tokenized prompts:** each prompt's ids are produced one request at a time and checked at most 1,024. The planned id storage, Σ prompt tokens × 4 bytes, plus per-request metadata, is bounded by a fixed `GEN_PAYLOAD`: 1,024 requests × 1,024 tokens × 4 bytes, plus 64 bytes each.
3. **The staged outputs:** a request's output can't exceed `max_new_tokens`, so storage is reserved for `n × max_new_tokens` ids and `n × max_new_tokens × 5` top-5 records (`(u32, f32)`, 8 bytes each). The text is decoded only after its ids are final, and is bounded by `max_new_tokens × L` bytes per request, where `L` is the tokenizer's longest decoded token (measured: 128 bytes over all 151,665 entries). `L` is computed at load and must be at most 128. The total is checked against `GEN_OUTPUT`, a fixed bound, before any request runs.
   - **This is not `execute`'s fixed-width row accounting:** variable-length outputs have their own staging.

**The per-request estimate,** in f32 values, with checked arithmetic, for a prompt of `p` tokens and `m` new tokens (`T = p + m`). Each term is cited to Candle 0.11's `qwen2.rs`:
- **Prefill** (the largest step), charged all-live:
  - q, k and v, the RoPE outputs and the key-value heads repeated to 14;
  - the [1, 14, p, p] scores, mask, softmax and context;
  - the MLP [p, 4,864] (gate, up, the product);
  - residuals and RMSNorm;
  - the logits [1, 151,936].
- **The KV cache at `T`,** with **old and new buffers both live during `Tensor::cat`:** 24 layers × 2 tensors × 2 (old and new) × [1, 2, T, 64].
- **The repeated KV:** per layer, a `contiguous` copy repeated to 14 heads, [1, 14, T, 64] twice.
- **A decode step at length `T`:** scores [1, 14, 1, T], a mask, and the logits again.
- **One model clone:** handles and its cache only. The weights are shared and charged nowhere per call.
- **Admission:** a request is admitted against `GEN_AGG` (the in-flight budget, 1 GiB); concurrent requests run while their summed estimates fit.
- **Refusal before anything is built:** a single request whose estimate exceeds `GEN_AGG` is refused by name before any clone or cache exists.

**Kept out of the per-call budget, and measured separately:** the resident F32 weights (about 1.98 GB), the RoPE tables, and the load's peak (the 988 MB BF16 file plus the F32 copies), reported at load.

**The lifecycle:**
- **The stored model has no cache.** A chat clones it only **after admission**, starts at offset 0, and advances by each step's actual input length.
- **The clone is dropped before its permit is released,** on success, on error and on unwind (a drop guard that owns both, in that order).
- **Outputs are published only when every request succeeds;** otherwise the lowest-index error is returned.

**Controls:**
- **Independence:** repeated and interleaved `chat` calls, and `chat_many`, give ids identical to isolated calls.
- **Failure and reuse:** an injected failure and an injected panic mid-decode are followed by reuse with identical results.
- **Admission order:** waiters behind the budget, and errors at lower and higher indices, follow 0135's ordering rules, with `inflight` at zero at the end.
- **Calibration** (fail-closed, as 0148):
  - it **forces the requested decode length** with a test-support knob that ignores EOS, so EOS can't shorten the alleged worst case;
  - it reports the actual shapes and the overlap (`max_running`), or named skips;
  - each peak must be ≤ its admitted estimate, apart from the separately reported resident weights;
  - over-budget requests are refused before any model or cache allocation.

## 4. U7, generation over the tickets (`probes/0149/u7_generate.rn`, a session paste), frozen

**The ticket text:** each ticket's **first passage**, exactly 0139's passage 0 (0136's committed byte range of title + "\n" + body, at most 256 MiniLM tokens). That's a frozen input rule. No tokenizer truncation is involved.

**Triage**, for all 252 tickets, with `max_new_tokens` 8:
- **system:** `You are a triage assistant for the issue tracker of Rune, a scripting language. Answer with exactly one word.`
- **user:** `Which one category fits this ticket best?\n` + one line `{label}: {description}` per `labels.tsv` row in order + `\nTicket:\n{passage 0}\n\nAnswer with one word: bug, feature, question, documentation or performance.`
- **The parse:** the generated text is trimmed, lowercased, and stripped of trailing `.`, `,`, `:`, `;` and `!`. It must then equal one of the five labels exactly; anything else is `unparsed`, sent to **review**.
- **No other parsing, retry or fallback.**

**Summaries**, for the 40 sample tickets, with `max_new_tokens` 48:
- **system:** `You are an assistant that summarizes issue tracker tickets.`
- **user:** `Summarize this ticket in one sentence of at most 20 words.\n\nTicket:\n{passage 0}`
- **Displayed, not scored.**

**The display:** class counts with review; the sample's summaries beside their tickets; timings for prefill and decode, and tokens per second.

**Retained:** `u7-triage.tsv` (ticket, prompt tokens, generated ids, text, parsed class, stop), `u7-summaries.tsv` (the same for the 40), and `u7-steps.tsv` (section 4a).

**A prompt over 1,024 tokens** is not dropped or truncated. U7 reports the named refusal and stops, and the record reports it. The measured passage-0 lengths make this unlikely, but the contract is stated.

**No tuning:**
- the prompts, the passage rule, the parse and the token limits are frozen here;
- a disappointing result is reported as is;
- the 40 tickets are the evaluation only.

## 4a. The retained trace, validated per step (review round 1, R3)

**`u7-steps.tsv`** has one row per generated token: task (`triage` or `summary`), ticket, step `k` (0-based), the prompt length `p`, the position `p + k`, the chosen id, and the top 5 as (id, logit) pairs in rank order.

**The policy:**
- `ids` **include** the terminal EOS when generation stopped by EOS;
- EOS appears only last;
- a `length` stop has exactly `max_new_tokens` ids and no EOS;
- `text` is the decode of the ids **without** the terminal EOS, with special tokens skipped (one explicit policy, used by both producers and the validator).

**The validator checks each producer on its own, before any comparison:**
- **Rows:**
  - the exact ticket sets (252 for triage, the 40 sample for summaries) in D2 order;
  - the exact widths.
- **The prompts:** reconstructed from verified D2, the ranges and the frozen prompts, tokenized with the pinned `tokenizer.json`, and equal to the producer's count.
- **The steps:**
  - exactly one step per generated id, `k = 0, 1, …` in order, with the position `p + k`;
  - five distinct in-range ids per step, and finite logits in descending order, an exact tie ordered by id;
  - the chosen id equal to the top-1 id and to the id at `k` in the row.
- **Stopping, by the policy above:** an EOS stop has its EOS only at the end; a length stop has exactly the limit and no EOS.
- **The text and the parse:** the text is recomputed from the ids, and the parse from the text.

**These steps are what the twin comparison and HF's teacher forcing use,** not the ticket rows alone.

**Corruption controls:**
- **on both producers:**
  - a missing, an extra, a duplicate and a reordered step;
  - a forged stop or count;
  - a non-finite logit;
  - a consistent forgery of a token with its re-decoded text and re-derived parse;
- **one-sided:** one logit's lowest bit.

## 5. Quality, reported and not gated

Per annotator, on the 40 sample tickets, for U7, and beside it **0139 and 0148 from their saved tables:**
- **coverage** (parsed / 40) and **accuracy over parsed;**
- **full coverage:** `unparsed` counts as wrong, since U7 has no rule-free fallback label. 0139's and 0148's full-coverage figures are as in 0148.
- **recall by label,** with `question` first;
- **confusion tables;**
- **pairwise exact McNemar** against 0139 and against 0148, at full coverage, descriptive only;
- **every U7 error,** with its raw generated text.

Agent-labelled, not human or user acceptance. The summaries are shown and not judged.

## 6. Parity, gated, reported apart from quality

0. **Provenance, first and fatal** (`run.sh` → `preflight.py`, 0148's pattern): D2, the frozen files in section 1, and the five model files. Sentinel controls prove that no later step begins after a change.
1. **The early stop gate, before any D2 generation** (all vectors finite; a failure stops the record):
   - a fixed set of hand-written non-D2 chats (`gate_chats.json`): mixed lengths, one near 1,024 prompt tokens, and one stopping by length;
   - **the template:** rnx's prompt ids must equal HF's `apply_chat_template(..., add_generation_prompt=True)` ids;
   - **greedy tokens:** identical to transformers' `generate` (F32; `do_sample=False`, `repetition_penalty=1.0`, `temperature`, `top_p` and `top_k` unset; the same EOS ids);
   - **logits:** the **complete** logit vectors at every step (dumped by a Rust test for these few chats) within the frozen tolerance (below).
   - **On failure, the record stops and reports.** There's no workaround and no loosening.
2. **Each producer validated first, then exact parity:**
   - **The validator:**
     - reconstructs each prompt from verified D2, the ranges and the frozen prompt text;
     - re-tokenizes it with the pinned `tokenizer.json`;
     - checks the producer's prompt token count, that its ids are within the vocabulary, its stop reason, its text as the decode of its ids, and its parse by the frozen parse.
   - **Then compared:** `twin0133 u7`, written directly against candle-transformers' Qwen2 and tokenizers, must produce **identical generated ids for every ticket.** Its limit is stated: it uses the same Candle model code.
   - **Corruption controls on both producers alike:** a swapped ticket, a forged id with its text re-decoded, a forged parse, and a missing, extra and duplicate row; plus one on one producer only.
3. **Hugging Face, two checks declared now, with completion artifacts** (exit 0 PASS, 3 a completed mismatch, anything else stops):
   - **3a, native end-to-end:** HF's `apply_chat_template` and tokenizer, then `generate` as in gate 1, for all 252 triage and 40 summary chats. The generated ids must be identical to U7's.
   - **3b, model-level:** HF's model, teacher-forced on **rnx's** prompt and generated ids.
     - **What rnx records:** its **top 5** ids and logits at each step (`u7-steps.tsv`). Full vectors for 292 generations would be about 2 × 10⁹ values; the full-vector comparison is gate 1's.
     - **At every step:**
       - HF's logits at rnx's top-5 ids must be within the frozen tolerance of rnx's;
       - HF's argmax must equal rnx's token, except where HF's logit for rnx's token is within 2 × tolerance of HF's maximum (a perturbation within the tolerance would make rnx's choice). Each such step is listed.
   - **The frozen tolerance is 1e-3 absolute on a logit.** This is an F32 model 24 layers deep, with logits of magnitude about 10 to 30.
     - **It's a proposed gate, frozen, not a demonstrated accuracy.** It's never widened after measurement.
     - All logits, rnx's and HF's, in both the early gate and the full gate, must be finite.
   - **A 3a outcome is kept distinct and diagnosed, never attributed automatically** (review round 1, R1). The diagnosis keeps HF's native prompt ids and finds, per generation, the **first differing token**, then classifies it:
     - **(i) a proven tokenizer or template difference:** the native prompt ids differ from rnx's;
     - **(ii) an explained numeric argmax difference on the same prefix:**
       - the prompt ids are equal and every earlier generated id is equal;
       - at the first divergence, HF's logits (teacher-forced on that shared prefix) have rnx's token within 2 × the tolerance of their maximum.
       - **Every later token is then a different context, and isn't compared.**
     - **(iii) an unexplained decoder or model difference:** anything else.
     - **(iii) stops the record, as an unexplained 3b failure does.** (i) and (ii) are reported with each generation listed.
   - **Controls of the diagnosis,** on synthetic traces:
     - a first divergence after equal prefixes, with a near tie (classified ii);
     - the same with a clear margin (classified iii);
     - a prompt-id difference (classified i);
     - an unexplained choice in 3b (refused).
4. **Launch,** as 0129's method. The delta must be within noise.
5. **After push:**
   - `:dep polars candle` runs U7, with the same ids as the twin;
   - `:dep candle` runs a Candle-only check on the gate chats, with the same ids as saved.

## Out of scope

- **Sampling, temperature and seeds:** still deferred.
- **Batched prefill with padding:** generations are independent and concurrent.
- **Quantized weights, GPU, chat history beyond one turn, tools, streaming.**
- **Any tuning of the prompts, the parse or the limits.**
