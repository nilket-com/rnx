# rnx 0131 evidence: a pretrained model on the CPU (text embeddings into a ranked table)

**The measure is met.** A real pretrained model makes semantic search a short script.

**The model:** `sentence-transformers/all-MiniLM-L6-v2`, pinned at `1110a243`.

**The workflow:** Polars text column → `TextEncoder` embeddings → cosine similarity → a ranked Polars table → display.

**Correct twice over:**
- **The Rust twin:** bit for bit.
- **The independent reference** (sentence-transformers on PyTorch): every embedding's cosine is ≥ 0.999999999998; scores agree within 6.7 × 10⁻⁷; all 8 queries have an identical top 5.

**Loading and memory:** the model loads in 0.10 s, as fast as PyTorch, at a third of PyTorch's peak memory.

**The finding for the GPU question:**
- **As shipped:** inference is 5–7× slower than PyTorch on this CPU.
- **The cause:** Candle's poor multi-threaded scaling on these small matrix products, not its arithmetic.
- **The fix, measured:** concurrent `embed` calls over slices of the input match PyTorch, bit identical. 10,000 texts take 10.0 s with Candle's default threading, or 9.6 s with `RAYON_NUM_THREADS=1`, against 10.5 s. That points to a CPU record before any GPU work.

## 1. The workflow (`probes/0131/workflow.rn`, run in a project)

```rune
let docs = polars::read_csv(`${dir}/docs.csv`)?;
let texts = docs.strings("text")?;
let enc = candle::TextEncoder::load(args[1])?;
let d = enc.embed(texts)?;
let s = candle::similarity(d, enc.embed(["how do I reset my password?"])?, ["score"])?;
let ranked = docs.with_dense(s)?.sort(["score"], polars::SortMultipleOptions::new().with_order_descending(true))?.head(Some(5))?;
```

**Usability:**
- **The search is these six lines.** The probe's script adds the 8-query score table and its Parquet write.
- **`SortMultipleOptions::new()` replaces the plan's `default()`.** `default` is a Rune keyword, so `::default()` can't be written in a script; Polars' `new()` is literally `Self::default()`. The generated sort and `head(Some(5))?` then work as written. There was no sort blocker and no new API.
- **The displays:**
  - `TextEncoder[bert; 6 layers, 384 dims, max 256 tokens]`;
  - `Dense[f32; 60 x 384](e0, …, e7, …)`;
  - the ranked frame through 0124's preview, with long texts cut and marked `…[truncated]`.
- **The result is sensible:** for "how do I reset my password?", the top 5 are the reset passage (0.734), the reset email (0.535), lockout (0.386), recovery codes (0.367) and deletion (0.325).
- **The bindings stay usable:** the texts, the queries and the frame are all reused afterwards.

**The project run** (`rnx project add polars candle`; a cold build of 6 min 13 s):
- 0.80 s wall and 221 MiB peak RSS for the whole script: the load, 69 embeddings, the similarity, the sort and the Parquet write.
- `probe0131 check-script`: **480 scores equal the twin's bit for bit** (`out/workflow-run.txt`).
- The assembly's artifact hash was identical across two rebuilds.

## 2. Correctness

**The Rust twin** (`probe0131 twin`) is written directly against `candle-transformers` and `tokenizers`, without rnx.
- **The rnx path equals it bit for bit:** 60 docs, 8 queries, 480 scores, and every control.

**The independent reference** (`reference.py`: torch 2.14.1+cpu, sentence-transformers 6.1.0, transformers 5.18.0), loading the same pinned local files (`out/compare.txt`):

| check | result | plan's bound |
|---|---:|---:|
| min cosine, docs | 0.9999999999991 | ≥ 0.9999 |
| min cosine, queries | 0.9999999999995 | ≥ 0.9999 |
| max score difference | 6.7 × 10⁻⁷ | ≤ 1 × 10⁻⁴ |
| identical top 5 | 8 of 8 queries | 8 of 8 |
| smallest gap, 5th to 6th | 0.0019 | (so the identical top 5 isn't luck) |

**Pipeline controls, each against the reference (min cosine ≥ 0.999999999998):**
- a text alone, and inside a mixed-length batch;
- an empty string, and whitespace only;
- a 40-sentence text, truncated at 256 tokens exactly as the reference does;
- 33 texts, so the final batch holds 1.
  - **The batch sizes are asserted:** `[32, 1]` under the production caps.
- **Halving:** under test-only lowered caps, the batches ran `[2, 1, 2]` for the mixed set and `[8, 12, 13]` for the 33. Their embeddings are bit identical to the unhalved ones, which is why the batch sizes are recorded as proof that halving happened. A single max-length text over the cap is a named refusal.

**Chunking** (`out/chunking.json`):
- 10,000 texts in one call against two calls of 5,000 each: the same row order, and a maximum difference of **0**. They were bit identical in this run.
- That is measured, not assumed; the plan claims only the tolerance.
- The cost: 60.4 s as two calls against 59.9 s as one.

**The mask reaches the model** (`Some(&mask)`, not Candle's all-ones default). The mixed-length and padding controls above would expose padding leaking.

## 3. The surface and its contract

**`df.strings(column)`** (Polars, hand-written):
- **Checked before any copy:** a `str` column, no nulls (refused by row), at most 65,536 rows, and at most 64 MiB summed with checked addition.
- **Reuse:** the frame is borrowed.
- **Tests** (`tests/dense.rs`): order, commas inside a value, a null, a non-string column, a missing column, 65,536 and 65,537 rows, just over 64 MiB, and reuse.

**`candle::TextEncoder::load(dir)`** (`adapters/candle/src/text.rs`):
- **Bounded reads:** each JSON file 64 KiB, `tokenizer.json` 16 MiB, `model.safetensors` 512 MiB, each refused from its metadata before the read.
- **The contract:**
  - `model_type` `bert`, `gelu`, absolute positions;
  - every size positive and bounded, with divisibility checked after the head count;
  - `pad_token_id` below the vocabulary;
  - exactly Transformer → Pooling → Normalize;
  - mean-token pooling only, at the hidden size;
  - `max_seq_length` from 2 to the position count;
  - `do_lower_case` false.
- **The tokenizer** is checked by its actual ids: every vocabulary id, added tokens included, is below `vocab_size`.
  - The pad token must exist.
  - `max_seq_length` must exceed the post-processor's special-token count, because `tokenizers` computes `max_length - added` unsigned and would underflow.
  - rnx sets padding and truncation itself.
- **Weights** are read through `load_buffer`, so there is no memory map and no `unsafe`.
  - **Every tensor must be F32,** because the VarBuilder would otherwise convert a consumed weight silently. That covers integer weights too.
  - **One cited exception:** an integer `embeddings.position_ids` (bare or `bert.`-prefixed), the transformers buffer the pinned file holds as I64. Candle's `BertModel` never reads it.
  - **Controls** (review round 1, R1):
    - an I32, I64 or U8 `word_embeddings.weight` is refused;
    - an F16 weight is refused;
    - an integer tensor under any other name is refused;
    - the position-ids buffer loads, bare or prefixed;
    - the pinned model still loads and matches the twin bit for bit.

**`enc.embed(texts)`:**
- **Borrowed and checked first:** the list is borrowed, never converted. The checks are 1 to 32,768 texts, each at most 64 KiB, at most 16 MiB in all, and `N × hidden` within `Dense`'s bound.
- **One shared `preflight`** (review round 1, R2) runs these checks for the Rune binding, the Rust entry point `embed_texts` and the test-support `embed_with_caps`, before any tokenizing or output allocation.
  - A control proves the Rust entry point refuses a 65,540-byte text, more than 16 MiB, 0 texts and 32,769 texts.
  - **What the controls show:** under `test-support`, the recorded batch sizes keep a sentinel through each refusal. That proves no successful run was recorded. That tokenization never *starts* is established by the code order: `preflight` runs before `worker::run`.
  - **A caveat for future tests:** the record is global state shared with the other embedding tests, without a lock. Per-call instrumentation, or serializing every participating test, should replace it before more such controls are added.
- **Tokenizing** works from the borrowed `&str`s.
- **Per batch, before any tensor exists:**
  - every encoded id is checked below `vocab_size`;
  - the padded length is checked against `max_seq`;
  - the three activation caps are checked with checked arithmetic: hidden 2²², feed-forward 2²⁴, attention 2²⁵.
- **Batches** shrink from 32 until the caps hold.
- **The pooling denominator** is checked before any division.
- **Normalization** is `x / max(‖x‖, 1e-12)`.
- **The joined worker** runs each call.

**`candle::similarity(a, b, names)`:**
- **Checked first:** the widths, `names_from`, and `Dense::check_shape(N, M)`.
- **The Rust entry point `similarity_named`** also runs `check_names` (after the count and shape checks) before any normalization. Controls cover a duplicate, an empty name and a 257-byte name (R2).
- **Dtypes:** f32 with f32 gives f32; otherwise both are computed in f64.
- **Stable normalization:** each row is scaled by its largest magnitude, its squares summed in f64. A genuine zero row is refused with its side and row.

**Tests:** 18 unit tests and 1 allocation control in Candle; 6 in Polars' `dense`. They run against a tiny generated BERT model (hidden 8, 2 heads, 1 layer, vocabulary 16) with a word-level tokenizer, so every refusal is tested without the download. Each is one named control:
- **every config field:** above the bound, zero, indivisible, a missing field;
- **modules and pooling:** the module order, a CLS pooling flag, the pooling dimension;
- **sequence length:** `max_seq` 1, 33 and 2 (2 is refused as no room beside 2 special tokens), and `do_lower_case`;
- **files:** a missing file, an oversized file, malformed JSON, truncated weights, an F16 weight;
- **token ids:**
  - a sparse vocabulary id;
  - a missing pad token;
  - a post-processor emitting id 40, which loads and is then refused at the batch;
- **texts:** an empty list, 32,769 texts, a 65,537-byte text, more than 16 MiB, a non-vector, a mixed list, with reuse after each;
- **caps:** halving, refusal, and overflowing products;
- **similarity:**
  - `f32::MAX` rows, subnormal rows and mixed magnitudes against exact values;
  - the f64 rule;
  - zero rows on either side;
  - widths, the name count, and M = 4,097;
  - reuse.

**Acquisition:** `probes/0131/fetch.sh` downloads the six pinned files with `curl` and checks their SHA-256 sums. `model.safetensors`' sum equals Hugging Face's LFS oid for that revision. rnx makes no network call.

## 4. Loading, throughput and memory (`out/measure.txt`, `out/reference.json`)

**Loading:**

| | rnx (Candle) | sentence-transformers (PyTorch) |
|---|---:|---:|
| load, warm (median of 10) | 0.099 s | 0.097 s |
| load, attempted cold cache | 0.097 s | — |
| phases | read 0.034 s, safetensors parse 0.032 s, model build < 0.001 s, tokenizer 0.013 s | |

**The cold-cache attempt** (`posix_fadvise(DONTNEED)` on the six files) is labelled an attempt. Its time equals the warm time, which suggests the pages stayed resident, or this NVMe is simply fast. It is not proof of a cold cache.

**Throughput (median texts per second):**

| input | rnx as shipped | rnx, 20 concurrent calls (probe only) | PyTorch (20 threads) |
|---|---:|---:|---:|
| one query | 25 (0.040 s) | — | 203 (0.0049 s) |
| batch of 32 | 137 (0.234 s) | — | 758 (0.042 s) |
| 60 docs | 145 | — | 843 |
| 10,000 corpus | 165 (60.8 s) | 1,000–1,045 (10.0 s; 9.6 s with `RAYON_NUM_THREADS=1`) | 949 (10.5 s) |

**Memory:**
- **Stage live figures** (rnx's tracked allocations):
  - 1.3 MiB at launch;
  - 91.9 MiB after the load (+90.6, the model);
  - the 10,000-text result keeps +14.7 MiB, which is the 14.6 MiB block itself.
- **High-water marks:**
  - **During the load:** the allocator peak reaches +177.9 MiB. That is the buffered load's transient double residency, the file buffer plus the parsed tensors, 1.96 × the model.
  - **Peak RSS:** 191 MiB after the load, and 289 MiB after embedding 10,000 texts. The allocator peak during that embedding is only +33.8 MiB, so the rest is the tokenizer's and Candle's transient buffers as seen by RSS.
- **The reference's peak RSS is 619 MiB.** The buffered load's doubling is a number now, for any later memory-mapped or streaming load to beat.

## 5. Why the CPU is slower, and how far that is fixable (probe-only)

**The profile** (`perf`, 640 texts):
- 58% in `gemm`'s AVX2 microkernel;
- about 15% in rayon and crossbeam scheduling;
- about 4% in a scalar `erf` (exact GELU).

**The CPU:** an i7-14700 (8 performance and 12 efficiency cores, 28 threads, AVX2 with no AVX-512). PyTorch runs MKL and oneDNN on AVX2.

**The thread sweep** (640 texts), from the table below:
- **Single-threaded,** Candle is only 1.6× slower than PyTorch: that is the kernel gap.
- **Multi-threaded,** Candle never gets below about 2.4 s, while PyTorch scales 4.7×.

| threads | Candle | PyTorch |
|---:|---:|---:|
| 1 | 4.75 s | 2.92 s |
| 4 | 2.50 s | — |
| 8 | 3.36 s | 0.62 s |
| 20 | 2.37 s | 0.70 s |
| 28 | 3.13 s | — |

**Batch-level concurrency** (`probe0131 bench-par`, `out/batch-parallel.txt`, with the environment and the worker's own thread counts recorded per run): N texts are split across T concurrent `embed` calls from T caller threads.
- **Each call runs on its own inference worker,** a scoped thread spawned by `worker::run`. Test-support instrumentation reports, inside that worker, Candle's matmul thread count (`get_num_threads()`, read from `RAYON_NUM_THREADS` on every call) and the size of the current rayon pool.

| texts | concurrent calls | `RAYON_NUM_THREADS` | worker: Candle threads / rayon pool | time |
|---:|---:|---|---|---:|
| 640 | 1 | unset | 20 / 28 | 3.90 s |
| 640 | 8 | unset | 20 / 28 | 0.75 s |
| 640 | 20 | unset | 20 / 28 | 0.65 s |
| 640 | 1 | 1 (probe-only, process-wide) | 1 / 1 | 4.68 s |
| 640 | 8 | 1 | 1 / 1 | 0.76 s |
| 640 | 20 | 1 | 1 / 1 | 0.64 s |
| 10,000 | 20 | unset | 20 / 28 | 10.0 s |
| 10,000 | 20 | 1 | 1 / 1 | 9.57 s |

- **The gain comes from concurrency, not thread confinement.** With Candle's default threading and no environment variable, 20 concurrent calls reach PyTorch's CPU (10.0 s against 10.5 s for 10,000). Setting `RAYON_NUM_THREADS=1` process-wide gains a further 4%.
- **Every run is bit identical** to one call.
- **Withdrawn (review round 1, R3):** the first draft credited each batch being confined to a one-thread rayon pool installed by the caller. That claim was wrong. `worker::run` spawns a fresh scoped thread, which doesn't inherit an installed pool. Inside it the pool was 28 and Candle's count 20, so the confinement never applied.

**This is not shipped here.** It changes `embed`'s execution model, so it's proposed as a follow-up record. That record's plan must budget the aggregate activations of concurrent batches, and keep the output order, the join and the error behavior.

## 6. Build cost and launch

**Cold build of the Candle adapter** (fresh target directories):
- 43.7 s at 0130, against 54.3 s with `candle-transformers`: **+10.6 s**, far under the plan's 3-minute threshold, so the vendored-BERT alternative isn't priced.
- **Binary:** 17.1 MB, against 24.1 MB (+7.0 MB, unstripped).
- **The combined Polars + Candle project** cold-builds in 6 min 8–13 s, against 6 min 4 s in 0129.

**The dependency check** (the stop rule): `candle-transformers` 0.11.0 with default features off, plus `tokenizers` with `onig` only.
- **No new native code:** `esaxx-rs`' C++ path sits behind its `cpp` feature, which nothing enables, and its build script is empty without it.
- **The only C build** is still 0129's bundled Oniguruma. There is no system library and no C++.

**Launch** (`probes/0131/launch.py`, 0129's method: 3 rounds of 60 interleaved launches against 0129's `78565ae`; `launch-results-{1,2,3}.json`):
- **Polars:** +1.51, −0.44 and −0.60 ms, so no change.
- **Candle:** +1.07, +0.47 and +0.33 ms, consistently positive; about +0.5 ms on 5.6 ms. That's the larger binary and four more registrations. The model loads only in `TextEncoder::load`.

## Suites

| suite | passed |
|---|---:|
| `rnx` core, release | 391 |
| `rnx` core, `test-support` | 436 |
| Candle | 19 (18 unit tests and the allocation control), with and without `test-support` |
| Polars, release default | 41 |
| Polars, release `test-support` | 264 |
| Polars, debug `test-support` | 265 |
| project tool | 81 |

**Notes:**
- **The project tool:** its standing `quiet::a_flooding_child…` flake (0129) failed once under the full load. It passed 5 of 5 alone, and the suite then passed 81 of 81.
- **The oracle:** `oracle-results.json` was restored (the standing `unique` row-order flip).
- **The freeze is unchanged:** no generated binding or `polars-gen` file changed.

## 7. Core change

`rnx::allocation` (the `allocation-peak` feature, a test or dev-dependency only) gains `live`, beside `peak` and `reset_peak`, for the stage live figures above.

The Candle adapter's `test-support` feature gains an optional `rayon` dependency and `last_threads()`, for the instrumentation in section 5.

## 8. For the GPU decision

**On this CPU, as shipped:**
- 10,000 documents embed in about 61 s;
- 100,000 would take about 10 minutes, with 3 chunks of 32,768 and 1 of 1,696 (`embed`'s cap);
- a single query takes 40 ms.

**With batch-level concurrency** (the measured probe), the same numbers are:
- about 10 s for 10,000;
- about 1.8 minutes for 100,000;
- PyTorch's CPU performance.

A query embeds in one batch, so its latency is still about 40 ms on the CPU; batch parallelism doesn't help a single query.

**What this implies:**
- The CPU gap to PyTorch closes without a GPU, in a CPU-only record that keeps the second-install principle.
- A GPU would then buy speed beyond PyTorch's CPU, at its setup and dependency cost. That trade is the user's decision.

## 9. Not done

- **Batch-level parallel execution** is the proposed next record, as above.
- **`:dep polars candle`: done after push.** Driven under a pty from the stock binary at `5f046e4`:
  - `:dep polars candle` built in 359 s, and a fresh session attached in 17.4 s;
  - at the prompt, the search returned the same ranked frame as the project run (0.7344043, 0.5348294, 0.385608, 0.36708972, 0.32479578);
  - the encoder presents as `TextEncoder[bert; 6 layers, 384 dims, max 256 tokens]`.
- **A memory-mapped or streaming load,** to remove the 1.96× transient.
- **Windows.**
