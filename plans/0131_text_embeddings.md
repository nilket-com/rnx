# rnx 0131: a pretrained model on the CPU — text embeddings into a ranked table

Status: plan, after 0129 (the bridge) and 0130 (maintenance).

**The question:** 0129 proved the bridge with a fixed MLP fixture. This record asks whether a real pretrained model makes a useful task easy, on the CPU, before any GPU work.

**The task:** semantic search over a document table, in one workflow:
- Polars text column → sentence embeddings → similarity to a query → a ranked Polars table → display.
- **Measured:** correctness, usability, model-loading time and memory, plus throughput as the input for a GPU decision.
- **Scoped to that workflow.**

## The model

`sentence-transformers/all-MiniLM-L6-v2`, pinned at revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41` (Apache-2.0):
- **Architecture:** BERT, 6 layers, hidden size 384, 12 heads, vocabulary 30,522, 22.7 M parameters.
- **Pipeline** (`modules.json`): Transformer, then mean pooling (`1_Pooling/config.json`: `pooling_mode_mean_tokens` only), then L2 normalize.
- **Limits:** `max_seq_length` 256.
- **Files used:** `config.json`, `tokenizer.json`, `model.safetensors` (≈ 91 MB, f32), `modules.json`, `1_Pooling/config.json`, `sentence_bert_config.json`.

**Why this model:** it is the de facto small CPU sentence-embedding model. It is small enough to measure honestly on a laptop-class CPU, and real enough that the result answers the question.

**Acquisition is the user's step, not rnx's.** `rnx` makes no network call while loading.
- The probe fetches the pinned revision's six files with `curl` and checks each one's SHA-256, recorded in the probe.
- A script names a local directory.
- A model registry or download command would be its own record if the workflow earns it.

## The workflow (the script)

```rune
let docs = polars::read_csv(`${dir}/docs.csv`)?;            // id, text
let texts = docs.strings("text")?;                           // a Vec<String>, bounded
let enc = candle::TextEncoder::load(`${dir}/model`)?;        // the pinned files
let d = enc.embed(texts)?;                                    // Dense f32, N x 384, unit rows
let q = enc.embed(["how do I reset my password?"])?;         // Dense f32, 1 x 384
let s = candle::similarity(d, q, ["score"])?;               // Dense f32, N x 1 (cosine)
let ranked = docs.with_dense(s)?.sort(["score"], polars::SortMultipleOptions::default().with_order_descending(true))?.head(Some(5))?;
ranked
```

**Usability is measured on this script:**
- its line count;
- each error a user meets (named in section 3);
- the displays: the ranked frame (0124's preview), a `Dense` summary, and the encoder's own one-line summary.

**Sorting:** this uses the existing generated `DataFrame::sort` and `SortMultipleOptions`, and `head(Some(5))`. `head` takes an `Option<i64>` and returns a `Result`.

**If the sort isn't usable from a script as written, implementation stops at that blocker and reports it.** A fallback top-k would be a new API, and its contract is agreed before it is built; nothing is added silently.

## 1. Text out of Polars: `df.strings(column)` (hand-written)

**The result:** a string column as a Rune `Vec<String>`. Plain Rune values are the right carrier for text: the tokenizer needs `&str`, and a text block in `rnx::interchange` would add a type without removing a copy.

**Checked before copying:**
- the column exists and is `String`;
- it has no nulls (refused by row);
- at most 65,536 rows;
- at most 64 MiB of text in total, summed over the string lengths, with checked addition.

**Frames above `embed`'s cap:** `strings` allows more rows than one `embed` call takes (32,768; section 2). A larger frame is embedded in explicit chunks by the caller:
- `df.slice(offset, n)` for each chunk;
- `strings`, `embed` and `with_dense` per chunk;
- `vstack` to join the chunks back.

The order is the frame's, chunk by chunk.

**The chunking control:** a 10,000-row frame embedded in two 5,000-row chunks, then stacked, gives identical row order to one call, with scores within the reference tolerance.
- **No bit identity is claimed across partitions:** chunk boundaries change batch composition and padding, and so floating-point rounding. The Rust twin stays bit exact for the *same* partition and batch execution. Any cross-partition identity is reported only if measured.
- The chunking's cost is reported against the single call.

**Reuse:** the frame is borrowed, and the frame and the column name stay usable.

## 2. `candle::TextEncoder`

**`TextEncoder::load(dir)`** reads each file bounded: its metadata first, then `take(limit + 1)`. The limits:

| file | limit |
|---|---|
| `config.json` | 64 KiB |
| `modules.json` | 64 KiB |
| `1_Pooling/config.json` | 64 KiB |
| `sentence_bert_config.json` | 64 KiB |
| `tokenizer.json` | 16 MiB |
| `model.safetensors` | 512 MiB |

**The contract, refused by name otherwise:**
- **`config.json`:** `model_type` is `"bert"`, `hidden_act` is `gelu`, and position embeddings are absolute.
  - **Every size is positive and bounded:**

    | field | range |
    |---|---|
    | `num_attention_heads` | 1 to 64 |
    | `hidden_size` | 1 to 1,024 |
    | `num_hidden_layers` | 1 to 24 |
    | `intermediate_size` | 1 to 4,096 |
    | `vocab_size` | 1 to 250,000 |
    | `max_position_embeddings` | 1 to 512 |
    | `type_vocab_size` | 1 to 16 |

  - **Divisibility:** `hidden_size % num_attention_heads == 0`, checked after the head count is known to be positive.
  - **The pad token:** `pad_token_id < vocab_size`.
- **`modules.json`:** exactly Transformer → Pooling → Normalize. The Pooling config has mean-tokens pooling only, with a dimension equal to the hidden size.
- **`max_seq_length`** is between 2 and `max_position_embeddings`: 2 leaves room for the post-processor's [CLS] and [SEP].
- **`tokenizer.json`** loads (`Tokenizer::from_bytes`), and must be compatible with the model:
  - **every id in its vocabulary,** added tokens included, is below the model's `vocab_size`. That checks the actual ids, not the count, since ids may be sparse. Separately, every encoded id is checked below `vocab_size` before the batch's input tensor is built, because a post-processor can emit ids independently. An out-of-range id is a named refusal, with a control using a sparse-id tokenizer;
  - `pad_token_id` maps to a token in it;
  - padding is set by rnx, to the longest text in the batch, with that id;
  - truncation is set to `max_seq_length`.
- **The weights** go through `safetensors::load_buffer`, the buffered path 0129 chose (no memory map, no `unsafe`), into `VarBuilder::from_tensors`, then `candle_transformers::models::bert::BertModel::load`. The weights must be F32; a missing tensor is Candle's error, prefixed with the path.

**`enc.embed(texts)`** takes a vector of strings and returns a new `Dense` (f32, N × hidden, columns `e0`, `e1`, …).

**Text ownership and preflight:**
- **Borrowed, never converted:** the Rune vector and each string are borrowed. There is no typed `Vec<String>` conversion, so nothing is taken or copied before validation.
- **The checks, in order:**
  - the count is 1 to 32,768, checked from the vector's length first;
  - each string is at most 64 KiB, checked borrowed;
  - the total is at most 16 MiB, with checked addition;
  - `N × hidden` is within `Dense`'s 2²⁴ values (32,768 × 384 = 12,582,912 at the pinned model).
- **Tokenizing from the borrows:** the tokenizer encodes from the borrowed `&str`s, with the guards held for the call, so a text is copied only into the tokenizer's own encoding.
- **Reuse:** the list and every text stay usable after success and after an `Err`.

**Per-batch activation caps, with checked arithmetic, before each batch's tensors exist.** `seq` is that batch's padded length, at most `max_seq_length`:

| tensor | values | cap | at the pinned model (32 × 256) |
|---|---|---:|---:|
| hidden states | `batch × seq × hidden` | 2²² (16 MiB of f32) | 3,145,728 |
| feed-forward | `batch × seq × intermediate_size` | 2²⁴ (64 MiB) | 12,582,912 |
| attention scores | `batch × heads × seq²` | 2²⁵ (128 MiB), a deliberate cap of its own, above `Dense`'s | 25,165,824 |

**Batches shrink to fit:** a batch starts at 32 texts and is halved until every cap holds for its own padded length. If one text at `max_seq_length` still exceeds a cap (possible only for configs far larger than the pinned one), `embed` refuses by name. The caps bound each tensor; the evidence measures the real transient multiple (softmax and GELU copies) as peak memory, rather than claiming it.

**The computation:**
- **The attention mask goes into the model:** the tokenizer's mask is passed to `BertModel::forward` as `Some(&mask)` (Candle's `None` means all ones, which would attend to padding), as well as to pooling. Token type ids are zeros.
- **Mean pooling:** the masked sum over the mask count.
  - **The guard, before any division:** every row's mask count is computed, and a row with a count of 0 is a named refusal.
  - **Why it never fires at the pinned model:** the post-processor always adds [CLS] and [SEP], and truncation to `max_seq_length` ≥ 2 keeps them. So the count is at least 2; the guard is the contract, not that expectation.
  - **The padded length:** each batch's padded length is checked to be at most `max_seq_length` before its tensors are built.
- **Normalization:** each row is divided by `max(‖v‖, 1e-12)`, exactly sentence-transformers' `Normalize` (`F.normalize`, `eps = 1e-12`).
- **The worker:** each batch runs on 0129's joined worker, so a panic becomes an `Err`.

**Pipeline controls, each against the independent reference:**
- a text embedded alone, and the same text inside a batch of mixed lengths (padding must not leak);
- an empty string, and a whitespace-only string;
- a text longer than 256 tokens, which must truncate exactly as the reference does;
- 33 texts, so the final batch holds 1;
- **halving:** the pinned model fits every production cap at a batch of 32, so halving is exercised with a test-only lowered cap (`test-support`). The halved batches' embeddings are compared with the independent reference too.

**`candle::similarity(a, b, names)`:** the cosine similarity of every row of `a` with every row of `b`, as an `N × M` `Dense`, with one name per row of `b`.

**Dtypes:** two f32 inputs give an f32 result; if either input is f64, both are computed in f64 and the result is f64. That is the only mixing rule.

**Checked before any conversion or `matmul`:**
- equal widths;
- `names` passed through `names_from`, with exactly M names;
- the full output shape through `Dense::check_shape`: `N × M` within 2²⁴ values with a checked multiply, and `M` at most 4,096 columns.

**Stable normalization:**
- Each row is first scaled by its largest absolute value. The squares are then summed in f64 and the row is divided by the norm.
- So a row holding `f32::MAX`, or only tiny subnormal values, can neither overflow nor underflow to a wrong score or a false zero.
- A genuine zero row (largest absolute value 0) is refused by name, with its row and side.
- The normalized rows are cast to the result dtype, and the product goes through Candle's `matmul`.

**Controls:**
- rows of `f32::MAX`, of subnormals, and mixed magnitudes, against an f64 reference computation;
- a genuine zero row on either side;
- a width mismatch;
- the dtype rule;
- every argument reused afterwards.

**Display:** `TextEncoder[bert; 6 layers, 384 dims, max 256 tokens]`, a bounded one-liner.

**Dependency:** `candle-transformers =0.11.0`, default features off. That means no flash-attention, MKL or Accelerate, and the same CPU kernels as 0129.

## 3. Errors a user meets (each a control)

- **Loading:** a missing directory or file names the file; a wrong `model_type`; an unsupported pooling mode; an oversized file is refused from its metadata; malformed JSON; a truncated safetensors file.
- **`strings`:** a null text, a non-string column, and the row and byte limits.
- **`embed`:** an empty list, more than 32,768 texts, a text over 64 KiB, more than 16 MiB in total, and a non-string element.
- **`similarity`:** a width mismatch, and a zero row.
- **None of these panics,** and every argument is reusable after its error.

## 4. The proofs and the measurements (`probes/0131`)

**The data:**
- `docs.csv` holds 60 hand-written help-desk passages over a few topics, with 8 queries.
- A deterministic 10,000-row corpus, built by recombining sentence fragments, is used for throughput.

**Correctness:**
- **The Rust twin:** the same tokenizer, model and pooling, run directly through `candle-transformers` and `tokenizers`, matches bit for bit (the same CPU kernels).
- **Against an independent reference:** `sentence-transformers` on PyTorch CPU, in a scratch `uv` environment kept to the probe, at the same pinned revision. For every text and query:
  - the cosine between our embedding and the reference embedding is ≥ 0.9999;
  - the score differs by ≤ 1e-4;
  - each query's top-5 is identical.
  - The reference's versions are recorded.

**Model loading:**
- **Warm:** `TextEncoder::load`'s wall time, as the median of 10 loads in one process, with the files in the page cache.
- **An attempted cold-cache load:** the first load after `posix_fadvise(DONTNEED)` on the six files, reported separately. This is an attempt, not proof of a cold cache: the kernel may keep pages, and the evidence says so.
- **Phases:** read, parse and build, timed separately.

**Throughput:**
- texts per second and tokens per second for one query, a batch of 32, the 60-document corpus and the 10,000-row corpus;
- the thread count;
- the same counts through `sentence-transformers` on the CPU, for context.

**Memory:**
- **Two kinds of figure, kept apart:**
  - **Stage live figures:** rnx's tracked live allocation bytes at launch, after the load and after embedding 10,000 texts. These show what each stage keeps.
  - **Cumulative high-water marks:** peak RSS (`getrusage`) and the allocator's peak (`allocation-peak`) across each stage. These expose transient double residency.
- **The reference's peak RSS,** for context.
- **The buffered load's double residency** (the file buffer plus the tensors, transiently) is measured and reported, so a memory-mapped or streaming load can be judged by a number later.

**Build cost:** the cold build time and binary size, with and without `candle-transformers`. It compiles every model in the crate; dead code is dropped at link time, but compile time isn't.
- **If it adds more than 3 minutes** to a cold build, the evidence prices the alternative: a vendored BERT encoder of about 400 lines, checked against the twin.
- The decision to switch would be a follow-up, not this record.

**Launch:** Candle's launch against 0129, which should be unchanged, since the model loads only on `TextEncoder::load`. Polars' launch against 0129.

**For the GPU decision:** the evidence closes with the CPU numbers and what they imply. For example, the time to embed 10,000 and 100,000 documents, and a query's latency. The decision is the user's.

## Gates

- **Composition:** `rnx project add polars candle` runs the script; `:dep polars candle` runs it after push.
- **Shared build:** the gate still holds.
- **Suites:** the core suites, Polars' three suites, Candle's suites and the project tool all pass, and the Polars freeze is unchanged.
- **Platforms:** Linux only.

## Stop rules

- If `candle-transformers`, at 0.11.0 with default features off, pulls in a system library or C++: stop and report.
- If the twin can't match bit for bit: report why before going further.
- If the reference can't meet the tolerance: report why before going further.
- If loading needs `unsafe` (a memory map) to fit the bound: report the measured double residency instead.

## Out of scope

- GPU.
- Other architectures and pooling modes.
- Downloads inside `rnx`.
- Fine-tuning.
- A vector index (search here is exhaustive).
- Batching across calls.
