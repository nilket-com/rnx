# rnx 0146 evidence: a cross-encoder re-ranker, with U5 retrieve-then-rerank

**The result:**
- **The new type:** `candle::CrossEncoder` loads the pinned `cross-encoder/ms-marco-MiniLM-L6-v2`, adding the pooler and classifier Candle's BERT lacks, and scores (query, passage) pairs.
- **U5:** retrieves 20 candidates with MiniLM embeddings, re-scores each with the cross-encoder, and shows both top 5s with excerpts.
- **The gates:** the trace is **BIT-EQUAL to an independent Rust twin,** each producer's candidates first recomputed from its retained retrieval evidence over D1; Hugging Face's own `CrossEncoder` agrees within **9.06e-6** with identical orders; re-ranking 100 pairs takes **2.1 s.**
- **On the frozen rubric, re-ranking is mixed:** it moves one supporting record into a top 5 and loses two first places. That's reported as measured, with no claim of improvement.

## 1. The binding (`adapters/candle/src/text/rerank.rs`)

**`CrossEncoder::load(dir)`:**
- **0131's loading contract,** with the shared BERT checks factored into `bert_config` and reused unchanged, and 0143's non-blocking bounded reads;
- **plus:**
  - the architecture must be `BertForSequenceClassification`, with exactly one label;
  - the `sbert_ce_default_activation_function` must be the identity (so the score is the raw logit; sentence-transformers would otherwise apply a sigmoid);
  - the exact pair template (`[CLS] A [SEP] B [SEP]`, types 0 then 1), checked three ways:
    - the serialized post-processor's pair template;
    - **its special-token definitions:** `[CLS]` and `[SEP]` each defined as exactly one token, itself, with its vocabulary id (review round 1);
    - **the complete structure of an encoded probe pair:** exactly `[CLS]`, A as it encodes alone, `[SEP]`, B, `[SEP]`, with types 0 through the first `[SEP]` and 1 after it (review round 1; previously only the first token and the types were checked);
  - the exact weight key set (encoder, pooler, classifier and the `position_ids` buffer), the head's shapes, and F32 throughout except that buffer;
  - **the buffer is exactly I64** (review round 1). The shared `unused_buffer` admits any integer dtype, as `TextEncoder`'s contract does, and that contract is left unchanged. The buffer must hold 0 to 511.

**`ce.score(queries, passages)`:**
- **Checked while borrowed, before anything proportional is allocated:** equal lengths, the count, each text, and **the combined bytes of every query and passage**, one borrowed string at a time.
- **A per-instance tokenizer** with truncation and padding disabled. Each pair is encoded once, and **a pair over 512 tokens is refused by index, never truncated.**
- **Emitted ids and types** are checked against `vocab_size` and `type_vocab_size`.
- **Ids, types and mask** are kept through right padding (the pad id, type 0, mask 0), and all three are passed to `BertModel::forward`.
- **Then the head:** the first token's state, the pooler (dense, then tanh), and the classifier. **The score is the logit.**

**The partition is 0135's S1 applied to pairs, with its single-pair exception:**
- start at 32 pairs and halve while more than one, until the caps hold and the estimate (0135's terms plus the head's) is within the share;
- a single pair needs the caps and the full budget.

**Execution goes through `embed`'s executor,** generalized over the model and batch runner (`execute<M>`; `embed` passes the same arguments as before and its tests pass unchanged). That means concurrent batches within the budget, permits released on every path, results published only on full success, and the lowest-index error otherwise.

## 2. Controls

**Unit tests** (`rerank.rs`, a tiny generated sequence-classification BERT with the pair template; release and debug):
- **Scoring:**
  - three pairs score, and the same pair alone matches its batched score within 1e-5;
  - the vectors are the script's after success and refusals;
  - **refusals:** unequal lengths, empty input, a non-string (named by index), an over-long passage, the combined total;
  - **a pair of exactly 32 tokens** (the fixture's positions) is accepted, and **33 is refused as "pair 1 is 33 tokens, at most 32".**
- **Loading refusals, each by name:**
  - two labels; another architecture; a Sigmoid activation, and none;
  - BERT's own processor in place of the template; a template with its second segment typed 0; a template with `[CLS]` moved;
  - **special-token definitions** (review round 1): a `[CLS]` of ids `[2, 5]` and tokens `["[CLS]", "cat"]`; a `[SEP]` of ids `[3, 3]`; a `[CLS]` mapped to `[SEP]`'s id 3;
  - a missing classifier weight or pooler bias; an extra tensor; a [2, 8] classifier; an F16 bias; zeroed `position_ids`;
  - **`position_ids` as U32 and as U8,** holding the right values (review round 1);
  - an embedding model's weights (no pooler or classifier);
  - a FIFO `config.json` (refused at once).
- **The types reach the model:** the same planned batch with every type zeroed scores differently.

**Allocation** (`tests/cross_encoder_alloc.rs`): 32,769 pairs; 32,768 against 32,767; an over-long passage; and per-text-valid inputs over the combined total. Each is refused under 256 KiB, with the inputs existing first.

**The pinned model** (`tests/cross_encoder_model.rs`, run explicitly with `RNX_CROSS_ENCODER`):
- **The declared policy:** 32 pairs of exactly 512 tokens split into **32 single-pair batches,** as the plan states and the twin derives.
- **The bound:** 513 tokens is refused by index.
- **The estimate holds:** one 512-token pair peaks at **69.3 MB,** within its estimate of **91.5 MB.**

## 3. U5 (`probes/0146/u5_rerank.rn`, a session paste; `out/u5-session.txt`)

**The data:** D1 (321 documents, 3,461 passages) and the frozen rubric (5 queries).
- **Retrieval** is 0136's U1′ at overlap 0, keeping the top 20 documents. Equal scores keep passage order (`with_maintain_order`).
- **The cross-encoder** scores (query, best passage) for each candidate, 100 pairs in all. Every pair ran as a single-pair batch: passages up to 256 tokens put two pairs over the share, so they run concurrently instead.

**Supporting records, retrieval against re-ranking** (`out/u5-summary.md`; a record's rank is its earliest document's):

| query | supporting | retrieval ranks | re-ranked ranks | retrieval top 5 | re-ranked top 5 |
|---|---|---|---|---|---|
| Q1 | 0063, 0067, 0070 | 1, 9, 4 | 2, 4, 3 | 0063 0061 0071 0070 0001 | 0061 0063 0070 0067 0070 |
| Q2 | 0130 | 1 | 1 | 0130 0130 0056 0054 0025 | 0130 0054 0056 0129 0082 |
| Q3 | 0126 | 1 | 2 | 0126 0126 0089 0122 0093 | 0058 0126 0126 0062 0096 |
| Q4 | 0129 | 1 | 1 | 0129 0082 0129 0118 0097 | 0129 0129 0130 0054 0034 |
| Q5 | 0068, 0124 | 1, 4 | 1, 6 | 0068 0058 0068 0124 0131 | 0068 0068 0058 0068 0123 |

| measure | retrieval | re-ranked |
|---|---:|---:|
| hit@1 | 5 / 5 | 3 / 5 |
| hit@5 | 5 / 5 | 5 / 5 |
| recall@5, mean | 0.93 | 0.90 |
| MRR, mean | 1.00 | 0.80 |

**What it says, without promising improvement** (as frozen):
- **The rubric is saturated at hit@1 for retrieval,** so re-ranking could only hold or lose there. It lost Q1 and Q3, each by one place.
- **Re-ranking helped Q1's recall:** 0067 moved from rank 9 into the top 5, so all three supporting records are now in it.
- **It cost Q5:** 0124 fell from 4 to 6.
- **It re-ranked toward passages that answer the query's wording directly.** Q3's new first place is 0058's preview gate rather than the group-by record, and Q5's top 5 now holds three of 0068's documents.
- **A five-query rubric can't say whether this is better in general.** It does show the re-ranker reorders substantively rather than echoing retrieval.

**Latency, reported separately:**

| step | time |
|---|---:|
| retrieval (load, chunk, embed 3,461 passages, similarity) | 69.4 s |
| re-ranking (100 pairs) | **2.0 s** |
| Hugging Face `CrossEncoder.predict`, the same 100 pairs | 1.3 s (median of 1,292, 1,318 and 1,317 ms) |

The round 1 rerun's times; round 0 measured 70.5 s and 2.0 s (the first draft of this table said 70.6 s and 2.1 s). The retained evidence is written after both timings, so neither includes it.

## 4. The gates (`out/u5-gates.txt`)

- **Exact, against the twin** (`twin0133 u5`, `compare_u5.py out/u5-script out/u5-twin RUBRIC D1`): **BIT-EQUAL.**
  - **Each producer retains its retrieval evidence** (review round 1), written by the script and by the twin alike:
    - `u5-passages.json`: every passage's document and text, by pid;
    - `u5-retrieval.tsv`: every passage's score for each query;
    - `u5-pairs.json`: the 100 pairs scored.
    - In `out/`, the first two are gzipped.
  - **Each producer is validated on its own first.** Round 0's checks on the trace stay:
    - shapes and identities;
    - distinct documents and passages;
    - finite scores;
    - retrieval order by score;
    - the re-ranking a permutation ordered by score, ties to the retrieval rank;
    - the metrics recomputed from the ranks and the rubric.
  - **Round 1 binds the trace to its evidence and the corpus:**
    - **The passages:** their documents are exactly D1's `.md` files, and each document's passages occur in its text in order, without overlapping.
    - **The scores:** the retained scores have one finite row per pid.
    - **The candidates are recomputed:** each query's 20 come from the retained scores (a stable descending f32 sort, ties to pid order, each document's first passage, the first 20 documents). They must equal the trace's document, pid and retrieval score exactly.
    - **The pairs:** `u5-pairs.json` must be exactly the rubric's query text with each candidate's retained passage text, in order.
  - **Then everything is compared between the producers:**
    - the passages and retained scores (f32 bits);
    - the pairs;
    - every query's candidates with both scores (f32 bits);
    - the re-ranked orders and the metrics.
    - The twin is independent: its own pair tokenizer, S1 partition, pooler and classifier, retrieval ranking and metrics. It derived the same 100 single-pair batches.
  - **Corruption controls** (`compare_controls.py`, `out/compare-controls.txt`): **26 corruptions, all refused,** and the unmodified pair passes.
    - **Round 0's 13.**
    - **Round 1's five identity corruptions,** each applied to both traces so agreement can't rescue them:
      - a nonexistent document;
      - an out-of-range pid;
      - an existing document with another document's pid (a document outside the query's candidates, so only the binding can catch it);
      - a document swapped for another existing one;
      - candidate 1's retrieval score raised one f32 step, so the order stays monotone.
    - **Seven corruptions of the evidence in both producers:**
      - a passage text not in its document;
      - a passage moved to another document;
      - a D1 document left without passages;
      - a retained score raised past candidate 1;
      - a retained row deleted;
      - a pair's passage and a pair's query changed.
    - **A one-sided flip** of the lowest bit of a non-candidate's retained score.
  - **Codex's reproducer** (`9999_nonexistent.md`, pid 999999999 in both traces) is refused: Q1's candidates are not those recomputed from the retained evidence.
  - **What the binding can't prove:** a consistent forgery of trace and evidence in both producers alike. Agreement with the independent twin, and the twin's own embedding, are what stand against that.
- **The semantic check** (`hf_check.py`, sentence-transformers 6.1.0, torch 2.14.1, CPU):
  - **It validates the directory first,** as above (review round 1), so the pairs it scores are bound to the candidates and the evidence; Codex's reproducer is refused before scoring.
  - **The result:** on the exact 100 pairs (`out/u5-script/u5-pairs.json`), **the largest difference is 9.06e-6** (bound 1e-4), and **the re-ranked orders are identical.**
  - Before U5, three hand-written pairs agreed to about 1e-6 (−11.030222, −11.397905, −11.309924).

## 5. Findings

- **F1, a Rune closure quirk:** a closure that builds a string with `+=` and returns it handed back an empty (moved) value. U5 uses a top-level function. It joins the earlier closure findings (0145: passing a closure moves it).
- **F2, the partition at these lengths is all single pairs:** the share bounds a batch long before the caps do. That's correct by the declared rule; a different concurrency policy would batch more, and is left as it was frozen.

## Gates

- **Replays** (`out/replays.txt`, session pastes): U1′ at overlap 0 identical to 0136's saved output; E4 EQUAL; E6, E7 and U4 BIT-EQUAL.
- **Suites:**

| suite | passed |
|---|---:|
| Candle, default (release) | 85 |
| Candle, `test-support` (release) | 86 |
| Candle `rerank` unit tests, debug | 6 |
| core, with `server-runtime` | 408 |
| Polars | 43 |

  The rerank unit tests now include round 1's loading controls. The pinned-model test passes when run explicitly (rerun after round 1). Clippy is clean on the touched files, and fmt and `git diff --check` are clean.
- **Launch** (`probes/0146/launch.py`, 0129's method; `rnx-candle` at 0145's `f5b4e67`): deltas of −0.28, −0.83 and +0.23 ms. **Within noise.**
- **`:dep candle` and `:dep polars candle`, after push** (a clean worktree binary at the pushed `0ec2b53`, a fresh private cache; `out/dep/`):
  - **`:dep candle`:** U5 needs Polars, so `dep_candle.rn` loads the `CrossEncoder` alone and scores U5's saved 100 pairs. `dep_compare.py` finds every score BIT-EQUAL in f32 bits to the twin-verified trace.
  - **`:dep polars candle`:** the full U5 (the `:dep` took 385.6 s on the fresh cache) validates against its own retained evidence and is BIT-EQUAL to the saved twin. The HF gate passes on its pairs (9.06e-6, identical orders).
