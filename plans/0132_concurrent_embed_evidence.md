# rnx 0132 evidence: concurrent batches in `TextEncoder::embed`

**The measure is met.** `enc.embed(texts)` now runs its batches concurrently: 10,000 texts take **12.55 s**, against 60.8 s in 0131 and PyTorch's CPU at 10.5 s. That is 1.195× PyTorch, inside the plan's 1.2× target, though narrowly.
- **Unchanged:** the results (bit identical to 0131 and to its Rust twin), the order, the surface and every refusal.
- **Bounded:** memory, by a budget that **was recalibrated during implementation** (section 2). Measured, the plan's uncalibrated estimate allowed 2.5 GB of peak RSS under a "512 MiB" budget.

## 1. What changed (`adapters/candle/src/text.rs`)

**Stage 1, planning (sequential):**
- 0131's batching: 32 texts, halved until the caps hold.
- Each batch is reduced at once to compact `u32` ids and mask, with the id, padded-length and pooling-count checks. Tokenizer `Encoding`s are dropped batch by batch.
- **The storage bound,** checked with checked arithmetic before each batch's vectors are allocated:
  - `PLAN_PAYLOAD` = 2 × 4 × 32,768 × 512 = 128 MiB of ids and mask, the largest input 0131's contract admits;
  - `PLAN_METADATA` is a separate allowance, derived from `size_of::<Planned>()` × 32,768.
- **Tokenizing is 3% of the 10,000-text call** (0.37 s).

**Stage 2, execution (concurrent):**
- W scoped workers claim batch indices from a counter.
- Admission is under one mutex and condition variable, against the in-flight budget.
- Each batch's rows are written into its own slice of one preallocated output, so the order holds by construction.
- **Every worker is joined** before `embed` returns.
- **W** = `min(batches, available_parallelism, 32)`. A single batch runs inline on 0129's outer worker, with no inner thread.

**Errors keep the sequential order:**
- Planning stops at the first failing batch j, and batches below j still execute.
- **The cutoff:** the lowest failing index is an index cutoff, changed only under the admission mutex followed by `notify_all`, so no waiter can miss it between its check and its wait.
  - A claimed index below the cutoff always runs, including a batch waiting for the budget.
  - Indices at or above it are skipped.
- **The error returned** is the lowest failing index's across both stages.
- **Panics, and the reservation as a permit** (review round 1, R1):
  - **Admission returns a `Permit`.** Its `Drop` releases the in-flight estimate and the running count, records the batch's outcome, moves the cutoff on failure and calls `notify_all`, all under the admission mutex.
  - **On every path:** success, error, and an unwind anywhere after admission. An unwind with no recorded outcome is that batch's "Candle panicked".
  - **One indexed `catch_unwind`** covers both the batch's model step and the publication of its rows into the output slice. So no unwind between admission and release can keep the reservation and strand a waiting lower-index batch.

**Instrumentation:** per call, not global. `Knobs` (workers, budget and injected faults) and the returned `Trace` (batches, executed and failed indices, failure order, peak running and in-flight, stage times) go through `embed_with`, which is test-support only. This replaces 0131's shared `LAST_BATCHES` sentinel in the controls; the probe still reads `last_batches()` and `last_threads()` after a successful call.

## 2. The budget, recalibrated (a deviation from the plan, stated)

**The plan** budgeted `AGG = 2²⁷` "activation values", with each batch estimated by its formula: `b × seq × (hidden + intermediate) + b × heads × seq²`.

**Measured** (`probes/0132/out/calibrate.txt`), with one batch alone, W = 1, against the allocator's peak:

| batch | formula estimate | real allocator peak | ratio |
|---|---:|---:|---:|
| 32 × 256 tokens | 156 MiB | 577 MiB | **3.70** |
| 8 × 256 tokens | 39 MiB | 144 MiB | 3.70 |
| 32 short texts (~40 tokens) | 8 MiB | 18.4 MiB | 2.29 |

**The consequence:** in the worst case (512 texts at 256 tokens), the uncalibrated budget admitted 3 batches. The allocator peaked at +1.73 GB and RSS at 2.5 GB, under a budget named 512 MiB. Candle's attention keeps several score-sized temporaries per layer (scaling, the mask, softmax). The plan said transients would be measured, not claimed, and this is that measurement.

**The change:**
- Each estimate is multiplied by **`TRANSIENT` = 4**, above the measured 3.70.
- The budget is **`AGG = 2²⁸` f32 values, 1 GiB**.
- The compile-time no-deadlock assertion becomes `TRANSIENT × (sum of 0131's caps) ≤ AGG`.

**Calibrated, the estimate covers the real peak** (`out/calibrate-after.txt`): real peak against estimate is 0.92 at 256 tokens and 0.57 for short texts.

**The trade-off:** a 32 × 256-token batch is now estimated at 624 MiB. So the longest batches run one at a time: the worst case takes 50.1 s instead of 22.9 s, with an allocator peak of +579 MiB (within 1 GiB) against +1.73 GB before. Short texts keep full concurrency.

**It is still an estimate for one call.** It is not a process-wide or allocator ceiling: RSS includes the resident model, the planned input and the allocator's retained pages.

## 3. Measurements (`probes/0132/out/measure.txt`)

28 CPUs (i7-14700); `RAYON_NUM_THREADS` unset; medians.

| input | 0131 | 0132 | PyTorch (0131's reference) |
|---|---:|---:|---:|
| one query | 0.0395 s | 0.0327 s (W = 1) | 0.0049 s |
| batch of 32 | 0.234 s | 0.219 s (W = 1) | 0.042 s |
| 60 docs | 0.415 s | 0.196 s (W = 2) | 0.071 s |
| 10,000 corpus | 60.8 s | **12.55 s** (W = 28) | 10.5 s |

**Single-batch latency is unchanged:** one query and a batch of 32 run on the outer worker as before, and the differences are within noise.

**The worker sweep, 10,000 texts:**

| W | 28 CPUs | 8 CPUs (`taskset -c 0-7`, `out/sweep-8cpus.txt`) |
|---:|---:|---:|
| 1 | 59.7 s | 44.7 s |
| 4 | 19.1 s | 22.3 s |
| 8 | 14.4 s | 21.2 s |
| 16 | 12.8 s | 21.0 s |
| 20 | 12.7 s | 20.8 s |
| 28 | 12.6 s | 20.9 s |

**The rule `W = min(batches, available_parallelism, 32)` is confirmed.** On 28 CPUs the gain flattens past 16, and the default (W = 28) is best. On 8 CPUs the default (W = 8) is within 2% of the best and doesn't oversubscribe: more workers don't slow it.

**Memory** (stage figures, with the components separately):
- **The 10,000-text corpus:**
  - output 14.6 MiB;
  - planned input about 3 MiB (313 batches);
  - allocator peak during the call +360 MiB, at most 817 MiB estimated in flight against the 1 GiB budget;
  - RSS 872 MiB, with the resident model about 90 MiB.
  - In 0131 the call's allocator peak was +34 MiB and peak RSS 289 MiB: concurrency trades memory for speed.
- **The worst case, 512 texts at 256 tokens:** allocator peak +579 MiB (the budget allowed 1 batch at a time); peak RSS 1,445 MiB, which includes the allocator's retained pages from the 10,000-text run before it.

## 4. Correctness

**Equivalence with 0131** (`probe0131 rnx` on the new engine, rerun after the permit change):
- **Bit for bit with the Rust twin:** 60 docs, 8 queries and 480 scores, plus every control.
- **Identical to 0131's own output.**
- **Halving:** the lowered-cap control gives the same batches as 0131 ([2, 1, 2] and [8, 12, 13]).
- **Chunking:** two 5,000-text calls are bit identical to one 10,000-text call.
- **The reference:** `compare.py` passes.

**Unit controls** (`text.rs`, a generated tiny BERT model; 25 tests, 100 of 100 parallel repetitions):
- **Equivalence:** W = 1, 2, 8 and 32, and the default, are bit identical on 10 batches whose long texts come first, so later batches finish first. The trace shows every index executed and the budget back to 0. A single batch runs with 1 worker.
- **A 512-token model** (`max_seq_length` 512, `max_position_embeddings` 512): texts reach the full 512 tokens; W = 1 equals W = 4; the planned storage is within `PLAN_PAYLOAD` = 128 MiB.
- **Errors:**
  - a middle failure returns its own error; two failures return the lower index's; a panic returns "Candle panicked";
  - across the stages: execution failure at 2 with planning failure at 5 returns 2's; planning failure at 2 with execution failure injected at 5 returns the planning error, and batch 5 never runs;
  - a planning failure at 4 still executes batches 0 to 3;
  - the budget is back to 0 every time.
- **The cutoff barrier:** batch 3 is held at admission, as if over budget, until a higher index fails; batches 3 and 6 both fail.
  - **The failure order is asserted: [6, 3].** Batch 6 failed while 3 was held, then 3 ran, and 3's error is returned. Run 20 times per test.
- **The unwind gap** (R1): with a budget of one batch at a time and 4 workers, so other batches wait, batch 2 panics after its model step returns and before its rows are published. Every one of 10 runs terminates with batch 2's indexed "Candle panicked", all workers joined, at most 1 running, and `inflight_end` 0.
- **Out-of-order completion, made deterministic** (the nit): batch 0 sleeps 200 ms before publishing, with 4 workers. The per-call completion order shows batch 0 finishing **last**, and the result is bit identical to W = 1.
- **The budget:**
  - with the budget lowered to the largest batch, at most 1 batch runs;
  - at twice that, at most 2;
  - a batch above a lowered budget is refused during planning ("above the in-flight budget"), and nothing waits.
- **0131's controls are unchanged:** the refusals, the caps and the shared preflight, which now asserts through the per-call trace that no batch was planned or run.

## 5. Gates

**The workflow:** 0131's script, unchanged, run in a Polars + Candle project (rebuilt): 0.63 s wall (0.80 s in 0131), 221 MiB peak RSS. **All 480 Parquet scores equal 0131's twin bit for bit** (`probe0131 check-script`). **`:dep polars candle`, after push** (the stock binary at `12559bb`, under a pty):
- it built in 276 s;
- at the prompt, the search returned the same ranked frame as 0131 (0.7344043, 0.5348294, 0.385608, 0.36708972, 0.32479578);
- the encoder presents as `TextEncoder[bert; 6 layers, 384 dims, max 256 tokens]`.

**Suites:**

| suite | passed |
|---|---:|
| `rnx` core, release | 391 |
| `rnx` core, `test-support` | 436 |
| Candle, release | 26 (25 unit tests and the allocation control) |
| Candle, release `test-support` | 26 |
| Candle, debug `test-support` | 26 |
| Polars, release default | 41 |
| Polars, release `test-support` | 264 |
| project tool | 81 |

**Unchanged:** the oracle (restored) and the freeze. **Clippy** is clean with `test-support`.

**Variance, stated:** the 10,000-text call measured 12.55 s (median of 3) in `probe0132 measure`, and 10.13 s as a single call in the equivalence run moments later. The target claim uses the slower, median figure.

## 6. Not done

- **The 1.2× target is met narrowly** (12.55 s against 12.6 s). The remaining gap to PyTorch is the 1.6× single-thread kernel difference (0131), which concurrency can't remove.
- **More concurrency for long texts** would need a smaller transient footprint inside Candle's attention, or a larger budget. Either is its own decision.
- **Cancellation** of an `embed` in progress, and concurrency across separate `embed` calls.
- **Windows.**
