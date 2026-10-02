# rnx 0146: a cross-encoder re-ranker, with U5 retrieve-then-rerank over rnx's own records

Status: plan. The user (2026-10-02) chose a new model capability, starting with a small cross-encoder re-ranker:
- **The workflow:** retrieve 20 candidates with embeddings, score each query–document pair with the re-ranker, and show the best five with excerpts.
- **The comparison:** retrieval alone against re-ranking, on the frozen search rubric.
- **The gate:** Rust parity. Relevance changes and added latency are reported separately, **without promising improvement.**
- **Deferred,** until a workflow needs them: the in-place setters and randomness.

**Amended after Codex's review:**
- **R1:** an explicit pair-tokenizer contract.
- **R2:** the partition reconciled with the activation caps and admission.
- **The metric:** identities for documents against records.

## 1. The model

**`cross-encoder/ms-marco-MiniLM-L6-v2`** (Apache-2.0), pinned to revision `233902d25c440f23af6f7d6e94d2946bac0bee0a`.
- **Architecture:** `BertForSequenceClassification` with one label: a 6-layer, 384-wide BERT, then the pooler (dense + tanh on the first token) and a 384 → 1 classifier.
- **The score:** its sentence-transformers default activation is the identity, so the raw logit is the relevance score.
- **Fetching:** `probes/0146/fetch.sh` fetches `config.json`, `tokenizer.json` and `model.safetensors` and checks each SHA-256:

| file | SHA-256 |
|---|---|
| `config.json` | `380e02c9…` |
| `tokenizer.json` | `d241a60d…` |
| `model.safetensors` | `821d1aa6…` |

  The script is the user's acquisition step; rnx makes no network call (0131's precedent).

**Measured before planning:**
- the tokenizer's pair template is `[CLS] A [SEP] B [SEP]`, with token types 0 and 1;
- no truncation or padding is configured;
- the maximum length is 512;
- the weights are 106 tensors, including `bert.pooler.dense.{weight,bias}` and `classifier.{weight,bias}`, all F32 except one I64 `bert.embeddings.position_ids` buffer.

**Candle's `BertModel`** loads the encoder (it accepts the `bert.` prefix) but has no pooler or classifier. The adapter adds both from the same file, with `candle_nn::linear`.

## 2. The binding (`adapters/candle/src/text.rs`, beside `TextEncoder`)

- **`candle::CrossEncoder::load(dir)`:**
  - **0131's whole loading contract:** bounded, non-blocking reads (0143's `read_limited`); the config's exact fields; the tokenizer contract; the weights read through `load_buffer`.
  - **Plus:**
    - the architecture must be `BertForSequenceClassification` with exactly one label;
    - the weight keys must be exactly the encoder's, plus the pooler's and the classifier's, each with its expected shape;
    - every weight is F32 **except `bert.embeddings.position_ids`,** which must be I64 [1, 512] holding 0 to 511, and is otherwise unused (Candle computes positions itself);
    - any other key, dtype or shape is refused by name.
- **`ce.score(queries, passages)`:** two vectors of strings of equal length, one pair per index. Returns a vector of f32 scores in input order.
  - **Inputs are checked first, while borrowed:** both outer vectors, before anything is copied. The vectors have equal length; there are 1 to `MAX_TEXTS` pairs; each string is at most `MAX_TEXT`; and **`MAX_TOTAL` bounds the combined bytes of every query and passage.** Nothing proportional to the input is allocated before these pass.
  - **R1, the pair tokenizer's contract,** explicit rather than inherited from `embed` (whose input deliberately has all-zero token types, and whose planned storage counts only ids and mask):
    - **At load:** the tokenizer's post-processor must be exactly the `[CLS] A [SEP] B [SEP]` pair template with token types 0 for `[CLS] A [SEP]` and 1 for `B [SEP]`, `[CLS]` at position 0 (the pooler's token). Anything else is refused by name.
    - **A per-instance tokenizer** with any inherited truncation and padding **disabled**, so a pair's untruncated length is measured.
    - **At scoring:** each pair is encoded once. A pair over 512 tokens is **refused, naming its index**, never truncated (0136's principle: the caller chunks). Every emitted token id is checked below `vocab_size`, and every type id below `type_vocab_size`.
    - **Each encoding's ids, token types and attention mask are kept** through right padding (the pad id, type 0, mask 0), and all three are passed to `BertModel::forward`.
    - **The planned storage** (ids, types and mask, three arrays, plus each batch's record) is counted before any of it exists.
  - **R2, the partition,** with 0135's S1 rule applied to pairs. It's deterministic and derived independently by the twin:
    - a batch starts at 32 pairs in input order and, while it holds more than one pair, halves until the per-batch activation caps hold (hidden `b × seq × 384`, feed-forward `b × seq × 1,536`, attention `b × 12 × seq²`) **and** the batch's in-flight estimate is within its share (`AGG / CONCURRENCY`);
    - **S1's single-pair exception** (Codex's correction): a batch of one pair needs the caps and the **full** budget (`AGG`), not the share;
    - each batch is padded to its own longest pair, with actual token types kept in re-padding.
    - **At the pinned model and 512 tokens,** the estimate is 21,823,488·b + 1,048,576 + 1,153·b values against a share of 8,388,608 (with the default `CONCURRENCY` of 32). So **32 pairs of 512 tokens split into single pairs:** 8 would fit the activation caps alone, but not the share.
    - **A single full-length pair** is admitted under the full budget, despite exceeding the share. A single pair that can't fit even that is refused by name.
    - **The twin derives this same rule.**
    - **The concurrency policy is the default, stated now;** it won't be changed after timings.
    - **The in-flight estimate** is 0135's (hidden, feed-forward and attention terms plus a fixed allowance) **plus the head:** the first token's state, the pooler's dense and tanh outputs (`b × 384` each) and the classifier's `b` scores.
    - **Verified, not assumed:** the measured allocator peak at long-pair shapes (32 pairs of 512 tokens, halved, and mixed lengths) is within the estimate.
  - **Execution:** as `embed` does since 0135, through the same executor:
    - batches run concurrently on the joined workers within the budget;
    - each batch's permit is released on every path, unwinding included, and a panic is that batch's error;
    - results are published **only when every batch succeeds,** in input order;
    - on failure, the lowest-index error across planning and execution is reported.
  - **The scores are each pair's logit,** unnormalized.
- **Contract errors are named:** unequal lengths, empty input, a non-string, an over-long text or pair.

## 3. U5, retrieve then re-rank (frozen before any run)

**D1 and the rubric are unchanged** from 0133 and 0136: rnx's records, and `probes/0133/rubric.tsv` with its 5 queries and their supporting records.

**Retrieval is U1′ at overlap 0** (0136's script), except that it keeps the top **20** documents per query instead of 5. That's each document's best passage by MiniLM cosine, ties to the earlier passage as in 0136.

**Re-ranking:**
- for each query, the cross-encoder scores (query, best passage) for each of its 20 candidates: 100 pairs in all, in query order then retrieval order;
- the candidates are reordered by score, descending, ties to the better retrieval rank.

**Shown** (Polars tables, displayed): for each query, the top 5 after re-ranking, with record, title, cross-encoder score, retrieval rank, and a 300-character excerpt of the scored passage. Retrieval's own top 5 are shown beside them.

**The relevance comparison, frozen.** Candidates are documents (a record's plan and its evidence are separate documents). Each document maps to its record by its four-digit name prefix. **A supporting record's rank is the earliest rank among its documents,** and recall@5 counts each record once.
- **for every query and every supporting record:** its rank among the 20, under retrieval and under re-ranking (or "> 20" when retrieval didn't return it);
- **hit@1 and hit@5** (any supporting record at rank 1, or within 5);
- **recall@5** (the share of supporting records within the top 5);
- **MRR** (the reciprocal rank of the first supporting record, 0 beyond 20).

**Stated before running:**
- **The rubric is small,** with 5 queries.
- **Retrieval alone already has hit@1 5/5** at overlap 0 (0136).
- So **re-ranking can't improve hit@1 here,** and can only keep it or lose it. The finer measures (supporting-record ranks, recall@5, MRR) are what can move. Every change is reported, in both directions, with no claim of improvement.

**Latency, reported separately:** the session's retrieval (embedding and similarity) and re-ranking (100 pairs) times. PyTorch's re-ranking time is given beside them.

## 4. Gates

- **Each trace is complete:** for every query, the 20 candidates' document paths, best passage ids, retrieval scores and retrieval order, and the cross-encoder score and re-ranked order of each. Each trace is validated against its own retained retrieval inputs:
  - the retrieval order is by retrieval score, descending, ties as frozen;
  - the re-ranked order is a permutation of the same 20, by score, ties to the retrieval rank;
  - the metrics are recomputed from those ranks.
  This is checked before any comparison between producers.
- **Exact parity with an independent Rust twin** (`twin0133 u5`, written directly against candle-transformers and tokenizers):
  - its own pooler and classifier;
  - the same pair tokenization, batch partition and padding;
  - **every one of the 100 scores equal in f32 bits;**
  - the same candidates, the same re-ranked order, and the same metrics.
  - **Validation first:** a comparer that checks each trace on its own (shapes, counts, queries, finite values, identities, and the metrics recomputed from the ranks) before comparing them, with corruption controls (0143 and 0145's lessons).
- **A semantic check against Hugging Face** (independent of my reading of the architecture):
  - sentence-transformers 6.1.0 `CrossEncoder` (torch 2.14.1, CPU) scores the same 100 pairs;
  - the scores agree within **1e-4 absolute;**
  - the re-ranked orders are identical, except between candidates whose scores differ by less than 1e-4, each listed.
  - This checks the pooler, tanh and classifier are what the model means, which a twin written from the same understanding couldn't.
- **Before any real run, controls on fixed pairs** (unit tests, and the twin):
  - an obviously relevant passage scores above an unrelated one for a fixed query (a sanity check, not a gate);
  - the same pair alone, in a batch of 32, and in a batch of mixed lengths gives the stated result: equal in bits within one partition, and across partitions the difference measured and reported;
  - **32 pairs of 512 tokens** exercise the declared policy: split into 32 single pairs, each admitted under the full budget, with the partition equal to the twin's and the peak within the estimate.
- **Replays:** E4, E6, E7 and U4 unchanged. 0136's U1′ (the top 5) still equals its saved output.
- **Suites:** Candle (with and without `test-support`), core, Polars; clippy and fmt.
- **Launch:** within noise.
- **`:dep polars candle`** after push, with U5. A Candle-only check of `CrossEncoder` on fixed pairs runs under `:dep candle`.

## 5. Controls (`adapters/candle/tests/cross_encoder.rs`)

**Loading** (each refused by name, before the weights are decoded where the config decides):
- a sequence-classification config with 2 labels;
- an embedding model's directory (no pooler or classifier);
- a missing, extra or misshapen classifier tensor;
- an F16 weight;
- `position_ids` with the wrong values or dtype;
- a FIFO or directory in place of a file;
- an over-limit file.

**Scoring:**
- unequal lengths, empty vectors, a non-string;
- an over-long text;
- a pair of 513 tokens (refused, naming its index) and one of exactly 512 (accepted);
- the counts and total bound;
- the receiver and arguments reused after success and after refusal.

**R1 mutations** (each refused by name at load, or shown at scoring):
- a missing pair template;
- an altered template (types swapped, or `[CLS]` moved);
- **segment types replaced by zeros:** a tokenizer that loads but emits all-zero types is caught by the template check, and the controls show the score changes when types are zeroed, so the types matter;
- inherited truncation re-enabled: the per-instance tokenizer still measures the full pair.

**Allocation:** oversized outer lists, over-long texts and an over-limit combined total are refused before any proportional allocation, under 256 KiB (0136's `count_alloc` pattern).

## Out of scope

- Fine-tuning, other cross-encoders, and GPU.
- Scoring every passage of each candidate (only its best passage is scored, as frozen above).
- Changing the rubric.
- The in-place setters and randomness.
