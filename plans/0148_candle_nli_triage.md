# rnx 0148: NLI classification for ticket triage (U6), against 0139's frozen sample

Status: plan. The user (2026-10-02) set the terms:
- **keep 0139's frozen labels and sample;**
- **define explicitly how NLI scores become a class or an abstention;**
- **compare against 0139's baseline without tuning on that sample;**
- **keep Rust parity and classification quality separate.**

**0139's baseline, on its frozen 40-ticket sample:**
- coverage 32 / 40, and accuracy over assigned 18 / 32 = 0.56, against each of two agent annotators (Cohen's κ 0.89);
- **"question" was never predicted:** zero-shot similarity to one-line descriptions sorts by topic, and asking for help is intent, not topic.

**This record adds a model that reads a (premise, hypothesis) pair and says whether one entails the other.** Whether that helps is reported, not promised.

**Amended after Codex's review (round 1), before any score:**
- **R1:** a closed DeBERTa geometry, with all arithmetic checked before loading.
- **R2:** a source-backed memory formula with a fail-closed calibration grid.
- **R3:**
  - pair identities reconstructed from the verified inputs;
  - an exactly specified recompute;
  - an early semantic stop gate.
- **Wording:** 0.5 is an uncalibrated frozen choice, and the secondary view is "full coverage, no abstention".

## 1. Before any score: what is frozen, and what has been looked at

**Nothing has been scored.** No NLI model has scored any D2 text, sampled or not. The only measurement on D2 so far is token counts:
- 0139's 458 passages × 5 descriptions = 2,290 pairs, tokenized with the pinned tokenizer;
- the longest is **294 tokens**, so none exceeds 512.
- That's needed to know whether the bound refuses any pair. It computes no score.

**Unchanged from 0139,** each checked by SHA-256 at gate 0:

| file | SHA-256 |
|---|---|
| `probes/0139/frozen/labels.tsv` (the five labels and descriptions) | `3ad1535382ab3b584de1ff466ba39a9e2aacce0cb7dba7177f6bcc2d3d910b92` |
| `probes/0139/frozen/rules.md` | `8aa28e2af45288406023237baa3a193e30bf816450e9181c0f124be876a4c928` |
| `probes/0139/frozen/sample.tsv` (40 tickets) | `5ebc6cfa9cd3c2eec357c1c8aa6927321d0b7cfefbaf28b7d7e50e8c753c77c3` |
| `probes/0139/frozen/annotations-claude.tsv` | `7c42f7411899dc0a4de4d59930443c254531a8eaf01fa5d72b50891875fbb0ed` |
| `probes/0139/frozen/annotations-codex.tsv` | `8bca7024e91a8de1e947625f6acb09d202f60a1b722b5728f1de7d328cff3ffd` |
| `probes/0139/out/u4-session.tsv` (0139's per-ticket baseline, accepted at `189ed48`) | `faa81b4683920da59a6da2b8a2cac0ce5683156eb015d810b1180336184d7a5a` |
| `probes/0136/out/ranges-o0.tsv` (the passages' byte ranges) | `a6940aceac5af1547262fa90a3479e4039e7b41d5379fc5643ad6a057b33403a` |

D2 itself is checked by 0133's `manifest.py check` against `d2-manifest.tsv`.

## 2. The model

**`cross-encoder/nli-deberta-v3-xsmall`** (Apache-2.0), pinned to revision `a150876415327c80daeff35ca6f68f5ed8cf5c24`.
- **Architecture:** `DebertaV2ForSequenceClassification`:
  - 12 layers, 384 wide, 6 heads;
  - relative attention (`c2p`, `p2c`, 256 position buckets) and no absolute position input (`position_biased_input: false`);
  - `type_vocab_size: 0`, so token types don't reach the model;
  - a context pooler (dense + GELU on the first token), then a 384 → 3 classifier.
- **Labels:** `id2label` = {0: contradiction, 1: entailment, 2: neutral}.
- **Weights:** 203 tensors, all F32 except the I64 `deberta.embeddings.position_ids` [1, 512].
- **Tokenizer:** Unigram (SentencePiece) with the pair template `[CLS] A [SEP] B [SEP]`; truncation and padding both null.
- **Why this model:**
  - Candle 0.11 already has the architecture (`candle_transformers::models::debertav2::DebertaV2SeqClassificationModel`), with the pooler;
  - it's the smallest of the sentence-transformers NLI cross-encoders.
  - Of the alternatives, the two RoBERTa NLI cross-encoders would go through Candle's XLM-R code instead. No quality comparison was made, and none will be.
- **Fetching:** `probes/0148/fetch.sh` fetches and checks:

| file | SHA-256 |
|---|---|
| `config.json` | `8d9f07bf7ba54a6fc3b1962483056f94c39dcf188db4cf61843e1c88f94b2342` |
| `tokenizer.json` | `5124ef2ead1a10a717703bc436de7f353da76d6340e4587719b42b1693707964` |
| `model.safetensors` | `4e4fc4977f8d29d2a164255c8f69b9d6c158deeb309bb5e70445b94666ccd9e9` |

## 3. The binding: `candle::NliModel`, beside `CrossEncoder`

**`NliModel::load(dir)`** reuses 0146's bounded, non-blocking loading. Its contract:
- **The architecture:** exactly `["DebertaV2ForSequenceClassification"]`, `model_type` `deberta-v2`.
- **The labels:** `id2label` exactly {0: contradiction, 1: entailment, 2: neutral}, with a matching `label2id`.
- **The geometry is closed** (review round 1, R1). `config.json` must hold **exactly** the production values below. Every other key is refused by name, except an allowlist of inert metadata, whose values are ignored:
  - `_name_or_path`, `transformers_version`, `torch_dtype` (must be `float32`), `initializer_range`;
  - the dropout probabilities, which are unused at inference.

| key | production (this model) | test fixture (only under `test-support`) |
|---|---|---|
| `hidden_size` | 384 | 16 |
| `num_hidden_layers` | 12 | 2 |
| `num_attention_heads` (head size = hidden / heads, exact) | 6 (64) | 2 (8) |
| `intermediate_size` | 1536 | 32 |
| `vocab_size` (= the tokenizer's) | 128100 | the fixture tokenizer's, ≤ 1,000 |
| `max_position_embeddings` | 512 | 32 |
| `position_buckets` | 256 | 8 |
| `max_relative_positions` | −1 (so the effective span is 512) | −1 |
| `pooler_hidden_size`, `pooler_hidden_act`, `pooler_dropout` | 384, `gelu`, 0 | 16, `gelu`, 0 |

  - **Same in both:**
    - `type_vocab_size` 0, `relative_attention` true, `position_biased_input` false;
    - `pos_att_type` exactly `["p2c", "c2p"]`, `share_att_key` true, `norm_rel_ebd` `layer_norm`;
    - `hidden_act` `gelu`, `layer_norm_eps` 1e-7, `pad_token_id` 0.
  - **Refused by name:** `attention_head_size`, `embedding_size`, any `conv_*` key and `cls_dropout`. In particular, Candle's conv layer is a `todo!()`.
  - **Arithmetic first:** all of it (heads × head size, 2 × buckets, every weight's expected element count and byte size, and their sum against the file's) is computed with checked operations **before any tensor is read.** Overflow is refused by name.
  - **The fixture geometry** is admitted only by the `test-support` build, so the shipped build loads the production geometry alone. No other geometry is admitted, so no arbitrary-geometry grid is needed.
- **The activation:** any sentence-transformers activation key must be absent or the identity, so the outputs are logits.
- **The tokenizer, as 0146:**
  - the serialized pair template with `[CLS]` and `[SEP]`, each defined as exactly one token (itself) and its id;
  - a complete probe-pair structure check;
  - truncation and padding disabled.
- **The weights:**
  - the exact key set from the architecture;
  - the head's shapes (pooler [384, 384], classifier [3, 384]);
  - F32 except `position_ids`, which must be exactly I64 and hold 0 to 511.

**`nli.score(premises, hypotheses)`** returns **a `Dense` f32 of N × 3 logits,** in `id2label` order. `nli.labels()` returns `["contradiction", "entailment", "neutral"]`.
- **As 0146:**
  - equal lengths, the count, each text, and the combined bytes are checked while borrowed, before anything proportional is allocated;
  - a pair over 512 tokens is refused by index, never truncated;
  - attention masks with right padding;
  - 0135's S1 partition with the single-pair exception, through the shared `execute`.
- **The memory contract** (review round 1, R2). For a batch of b pairs padded to s tokens, the estimate is a checked formula, each term cited to its line in candle-transformers 0.11's `debertav2.rs`:
  - **Embeddings and mask:**
    - ids and mask (I64 [b, s] each), and the f32 embedding [b, s, 384];
    - the 4-D attention mask [b, 1, s, s] (f32), with the float copies `broadcast_lt`/`where` make;
  - **Relative positions:** the bucketed ids built through f32 and I64 [s, s] (and the f32 log/ceil intermediates of `make_log_bucket_position`);
  - **Per layer:**
    - q, k, v [b, s, 384], each also reshaped to [b·6, s, 64];
    - the scores [b·6, s, s];
    - the relative embeddings [2·256, 384], their key and query projections repeated b times ([b·6, 512, 64] each);
    - **c2p:** the pre-gather matrix [b·6, s, 512], the I64 gather index [b·6, s, s] and the gathered [b·6, s, s];
    - **p2c:** the same three;
    - the softmax and context [b·6, s, s] and [b, s, 384];
    - the intermediate [b, s, 1536];
  - **The pooler and head;**
  - **Copies:** `contiguous` and `t()` copies where the code makes them. The estimate charges the largest layer's live set plus what persists between layers.
- **The budget:** the S1 partition and the `AGG / CONCURRENCY` share apply as in 0135 and 0146, and the single-pair exception still needs the caps and the full `AGG`.
- **Calibration, fail-closed:** an `#[ignore]` pinned-model test, run explicitly.
  - **The grid:**
    - lengths s ∈ {16, 128, 512};
    - batch sizes b ∈ {1, and the maximum the partition admits at that length};
    - each alone, and with `CONCURRENCY` such batches running at once.
  - **Exact shapes:** every cell asks for its exact shape; a cell the partition can't form is a named skip, not a silent pass.
  - **Each cell:** the measured peak must be ≤ the estimate, with the headroom reported.
  - **Refusal before allocation:** with the limit lowered below one cell's estimate, that request is refused before anything proportional is allocated.
  - **Single-pair no-deadlock:** a single 512-token pair under the minimum admissible budget completes, and its permit is released.

**Controls** (unit tests on a tiny generated DeBERTa-v2 fixture, plus allocation tests):
- refusals by name for each contract item, including the 0146 R2/R3 cases (a non-I64 buffer; multi-id special tokens);
- a fixture's pairs score, and the same pair alone matches its batched score;
- the bounds: 32 tokens accepted, 33 refused;
- the over-count and over-total inputs refused under 256 KiB.

## 4. How NLI scores become a class or an abstention (frozen)

| step | the rule |
|---|---|
| **Hypothesis** | each label's frozen description from `labels.tsv`, **verbatim**, with no template around it |
| **Premise** | each of the ticket's passages, exactly 0139's (0136's committed byte ranges of title + "\n" + body, overlap 0) |
| **Pair** | (premise, hypothesis), in NLI order: the passage first |
| **Pair score** | `p_entail` = the entailment probability of a 3-way softmax over that pair's logits: the f32 logits cast to f64, then `softmax_last_dim` in f64 |
| **Ticket score per label** | `s_label` = **the maximum** of `p_entail` over the ticket's passages |
| **Class** | the label with the largest `s_label`; an exact tie goes to the first in `labels.tsv` order |
| **Abstention** | **when the largest `s_label` is below 0.5**, the ticket is `review` instead. Exactly 0.5 is assigned |

**Why max and 0.5, fixed before any score:**
- **Max:** a ticket's category is usually stated in one place, often its title, which opens the first passage. Averaging over up to 58 passages of code, logs and discussion would dilute that. Long tickets would drift toward abstention whatever they say. (0139's mean pooling averaged embeddings, a different object.)
- **0.5:** an uncalibrated choice, frozen here, on the model's softmax scores. Abstaining means no passage gives the best label's description an entailment score of at least one half. It isn't a demonstrated calibrated probability, nor the model's native multiclass decision boundary. It's simply not fitted to anything (review round 1).

**A second view, full coverage with no abstention:** classification is the class above with no abstention, compared with 0139's `best` label (its top label before its margin rule). It's defined here, before any score, so a comparison isn't confined to whichever tickets each rule happens to cover.

**No tuning on the sample:**
- **Nothing above changes after any score:** not the hypotheses, the template, the aggregation, the threshold or the tie rule.
- **If the result disappoints, it's reported as is.** A different rule would be a new record, frozen first and evaluated on tickets outside this sample, with fresh annotations.
- **The 40 tickets are the evaluation only.**

## 5. U6, NLI triage (`probes/0148/u6_nli.rn`, a session paste)

All 252 D2 tickets → 458 passages → 2,290 (passage, description) pairs → `nli.score` → the rule in section 4. It shows, as 0139's U4 does:
- tickets per class, `review` included;
- the review queue (the 20 lowest best scores);
- each class's most confident examples.

**Retained, for the gates:**
- `u6-pairs.tsv`: ticket, passage index, label, and the three logits as f32;
- `u6-tickets.tsv`: ticket, the five `s_label` values, class, best, second, and the best score.

## 6. Parity, gated (reported apart from quality)

0. **Provenance, first and fatal** (0147's pattern: `probes/0148/run.sh` runs `preflight.py` before anything):
   - D2 against 0133's manifest;
   - the frozen 0139 files and the byte ranges against the hashes in section 1;
   - MiniLM is not used. The NLI model is checked against section 2's hashes.
   - **Controls** prove that no session, twin or HF step begins after a change.
1. **Each producer validated first** (review round 1, R3):
   - **The pair identities are reconstructed by the validator from the verified inputs:**
     - the ordered Cartesian set of (ticket in D2 manifest order, passage index and byte range from `ranges-o0.tsv`, label in `labels.tsv` order), with the passage text cut from verified D2 and the frozen description;
     - 2,290 identities, every logit triple bound to exactly one of them, in that order.
   - **The ticket table is recomputed from the retained logits, at a stated precision:**
     - the runtime casts each f32 logit triple to f64 and applies `softmax_last_dim` in f64, so the decision arithmetic is f64 in both the script and the twin;
     - the validator recomputes in Python f64;
     - each `s_label` must agree within **1e-12** absolute;
     - classes and abstentions must be identical, except a ticket whose best `s_label` is within 1e-12 of 0.5, or of its second label's. Each such ticket is named.
   - **The boundary is precise:** a best score of exactly 0.5 is assigned; only below 0.5 abstains.
   - **Synthetic controls of the recompute:** an exact tie (resolved by `labels.tsv` order); exactly 0.5 (assigned); just below 0.5 (abstains).
   - **Corruption controls on both producers alike:**
     - a passage swapped between two pairs;
     - a ticket's identity swapped;
     - a label swapped;
     - a missing, an extra and a duplicate identity;
     - a ticket's class changed consistently with forged scores.
   - **One-sided:** a single logit's lowest bit flipped.
2. **Exact parity:** `twin0133 u6` is written directly against candle-transformers' DeBERTa-v2 and tokenizers, with its own pair tokenization, partition, softmax and aggregation. Every logit must be equal in f32 bits, and every ticket row equal.
   - **The limit, stated:** both use the same Candle model code, so the twin checks rnx's binding, partition and rule, not Candle's DeBERTa.
3. **The semantic check against Hugging Face, which does check Candle's DeBERTa:**
   - **First, an early stop gate,** before U6 ever runs:
     - a fixed set of hand-written pairs not drawn from D2: mixed lengths, scored together so that padding is exercised, and each also scored alone;
     - Candle and HF must agree within 1e-4 on every logit.
     - **If they don't, the record stops and reports a Candle-against-HF architecture failure.** It isn't worked around, and the tolerance isn't loosened.
   - **Then the full check:** sentence-transformers' `CrossEncoder` (6.1.0, torch 2.14.1, CPU) scores the 2,290 pairs **reconstructed by the validator** from verified D2, the ranges and the labels (not the producer's own texts);
   - **any non-finite HF output fails;**
   - every logit within **1e-4** absolute;
   - classes and abstentions identical, except where a ticket's best score is within 1e-4 of 0.5 or of its second label's. Each such case is listed.
4. **Launch:** the adapter catalogue grows, so 0129's launch method is used. The delta must be within noise.
5. **After push:**
   - `:dep polars candle` runs U6, BIT-EQUAL to the twin;
   - `:dep candle` runs a Candle-only fixed-pair `NliModel` check, BIT-EQUAL to the saved logits.

### 6a. Amendment after the stop rule (agreed with Codex, 2026-10-02, after the first full run)

**What happened:** the frozen full check (6.3) **failed**, and stays recorded as failed:
- the maximum |rnx − HF| logit was 0.188;
- ticket 975 was `question` for HF but `review` for U6.
- **The cause, measured:** native HF tokenization differs from the pinned `tokenizer.json` on 15 of 2,290 pairs. transformers 5.18's `DebertaV2Tokenizer` rebuilds a normalizer without the file's precompiled charsmap, so "µ" becomes `[UNK]`.
- **The model itself agreed:** fed identical ids, HF agreed with Candle within 1.3e-5.

**What changes in how gate 3 is satisfied:**

1. **The original check is kept and replayed as it was frozen,** with its exit status checked separately. Its failure is recorded, and it's never relabelled as a pass.
2. **A separately named model-level gate is added** (`probes/0148/hf_model.py`):
   - **The input:** PyTorch's DeBERTa (transformers' `AutoModelForSequenceClassification`, the same pinned files) is fed **the exact ids, masks and pair order from the pinned `tokenizer.json`,** for all 2,290 reconstructed inputs.
   - **The checks:**
     - every output has shape 3 and is finite, and is aligned to its identity;
     - every logit is within the unchanged **1e-4**;
     - the decisions are recomputed by section 4's rule and must equal U6's, except ties or threshold cases within 1e-4, each listed.
   - **This checks Candle's DeBERTa semantics.** Native HF tokenization remains a disclosed end-to-end difference.
3. **Reproducible tokenizer diagnostics** (`probes/0148/tokenizer_diag.py`):
   - **What's pinned:** `spm.model` at the same revision, by SHA-256 `c679fbf93643d19aab7ee10c0b99e460bdbc02fedf34b92b05af343b4af586fd`.
   - **What's recorded:**
     - the exact versions of tokenizers, transformers, sentencepiece and torch;
     - the native backend's normalizer configuration;
     - every pair whose native-HF ids differ, with its identity and the differing ids.
   - **What it proves:**
     - every pair with differing native ids is accounted for, as exactly the listed set;
     - on the identical-id subset, native HF `CrossEncoder` logits agree within 1e-4, with shape, finiteness and identity alignment checked;
     - the three tokenizations are compared over every distinct text: the pinned file, native HF and SentencePiece.
4. **What doesn't change:** the hypotheses, aggregation, threshold, sample, tolerance and the quality comparison.
   - **How rnx is described:** it **follows the pinned `tokenizer.json`.** It isn't described as fully matching native HF or SentencePiece.

## 7. Quality, reported and not gated (`probes/0148/evaluate.py`)

**Per annotator (Claude's and Codex's frozen labels), on the 40 sample tickets, for NLI and for 0139's saved table:**
- **coverage** (assigned / 40) and **accuracy over assigned,** each with its numerator and denominator;
- **full-coverage accuracy:** NLI's class with no abstention, against 0139's `best`;
- **recall by label at full coverage,** with `question` reported first because it was 0139's failure;
- **the confusion table** of each;
- **the paired full-coverage comparison:** both right, NLI only, 0139 only, both wrong, with an exact two-sided McNemar p-value on the discordant pairs. Descriptive only: 40 tickets can't establish a general difference.
- **every NLI error,** with the ticket's title and its two best `s_label` values.

**Also reported:**
- the class counts over all 252;
- **time:** the 2,290 pairs, and the session end to end, separate from HF's `predict` on the same pairs.
- **What the comparison means:** both models are agent-labelled against agent annotations, not human or user acceptance. That's said in the evidence, as 0139 said it.

## Out of scope

- **Any tuning:** hypothesis templates, label wording, aggregation, thresholds, a sixth label, calibration.
- **Multi-label output.**
- **The in-place setters and randomness:** still deferred until a workflow needs them.
- **GPU.**
