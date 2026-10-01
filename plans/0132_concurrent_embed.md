# rnx 0132: concurrent batches in `TextEncoder::embed` (CPU)

Status: plan, the user's go after 0131. This is CPU only, and comes before any GPU work.

**The finding it acts on** (0131, section 5):
- `embed` runs its batches one after another. Candle's matmul barely scales across threads on these small products, so 10,000 texts take 61 s against PyTorch's 10.5 s on the same CPU.
- Twenty concurrent `embed` calls over slices of the input took 10.0 s, with no environment variable, and the results were bit identical.
- The gain is concurrency between batches, not thread confinement (0131's withdrawn claim).

**The measure:** `enc.embed(texts)` itself runs its batches concurrently.
- **Unchanged:** the script's surface, the results (bit identical to the sequential order), the output order, every refusal and the error behavior.
- **Bounded:** memory, by an aggregate activation budget.
- **Target:** the 10,000-text corpus within 1.2× of PyTorch's CPU (≤ 12.6 s on the reference machine).

## 1. The design

**Two stages inside `embed`, after 0131's shared preflight (unchanged).**

**Stage 1, planning (sequential, as today):**
- **Batching:** batches of 32, halved until 0131's per-batch caps hold.
- **Per batch:** the encodings are reduced at once to compact `ids` and `mask` vectors, `u32`, `batch × seq` each.
  - Every id is checked below `vocab_size`, and the padded length against `max_seq`, as in 0131.
  - The tokenizer's `Encoding`s (tokens, offsets, strings) are dropped batch by batch, so they never accumulate across the whole input.
- **Why sequential:** the batch boundaries depend only on the texts and the caps, so they are identical to 0131's. That is what keeps the results bit identical.
- **Its bound, for every model 0131's contract admits:** `max_seq_length` can reach `max_position_embeddings` = 512, not just the pinned model's 256.
  - The planned ids and mask are at most 2 × 4 bytes × 32,768 × 512 = **128 MiB**; 64 MiB is the pinned model's figure.
  - The per-batch metadata (range, padded length, estimate; 32 bytes a batch, at most 32,768 batches) counts toward the same bound.
  - The running total is checked, with checked arithmetic, **before** each batch's vectors are allocated.
  - A control uses a generated model with `max_seq_length` 512 and texts that reach it.
- **Its cost** is measured; `tokenizers` already parallelizes inside `encode_batch`.

**Stage 2, execution (concurrent):**
- **Workers:** W scoped worker threads, all joined before `embed` returns.
- **Dispatch:** each worker claims the next batch index from a shared counter.
- **Admission:** a batch starts only if the aggregate budget allows it (see below).
- **The model step** is unchanged: forward with the mask, then mean pooling, then normalization.
- **Writing results:** each batch's rows go into their own disjoint slice of one preallocated output (`N × hidden` f32, already checked against `Dense`'s bound by the preflight). Order is preserved by construction.

**W, the worker count:**
- `W = min(batches, available_parallelism, 32)`;
- **a single batch runs directly on 0129's outer worker,** with no inner thread, as today. Its latency is measured against 0131, not assumed unchanged;
- the evidence measures a sweep (W = 1, 4, 8, 16, 20, 28) and either confirms the rule or changes it, stating why.

**Candle's own threading is unchanged:** no environment variable is set or read by rnx. 0131 measured `RAYON_NUM_THREADS=1` as only 4% faster, and setting it would be process-wide.

## 2. The aggregate activation budget

**Each batch's estimate,** from 0131's three per-batch caps:

`b × seq × hidden + b × seq × intermediate_size + b × heads × seq²`

The estimate is checked arithmetic over the planned shape, which is known before the batch runs.

**The budget:** `AGG = 2²⁷` values (512 MiB of f32) in flight across this call's workers.
- **What it is:** an estimate of one invocation's in-flight model work. It is not a process-wide limit and not an allocator ceiling.
- **What the evidence reports separately:** the planned input buffers, the output, the model's resident weights, and Candle's measured transients.
- A worker waits (a mutex and condition variable) until the in-flight sum plus its batch's estimate fits.
- It adds its estimate when it starts the batch, and subtracts it when the batch finishes, on success, error or panic.
- **No deadlock:** one batch's largest possible estimate (the sum of 0131's caps, 2²² + 2²⁴ + 2²⁵ ≈ 5.5 × 10⁷) is below `AGG`. A compile-time assertion keeps it so, so a lone batch always fits.
- **Under a lowered test-support `AGG`:** a batch whose estimate can't fit is refused once its shape is known, during planning, before any waiting or execution.

**At the pinned model:**
- short texts (32 × 40 tokens): about 1 million values per batch, so W is the only limit;
- the longest batches (32 × 256 tokens): about 41 million values per batch, so at most 3 run at once.

**This bounds the model's activations, not every transient.** Candle's own temporaries (softmax and GELU copies) are measured as peak memory, as 0131 did, not claimed.

## 3. Errors, panics and order: the sequential order, preserved

**The rule:** the error returned is the one sequential `embed` would have met first. Sequentially, batch i's model step runs before batch j's planning whenever i < j, so arbitration spans both stages.

**Planning failures:**
- Planning stops at the first failing batch j (an id out of range, a padded length, a lowered-budget refusal). Its error is recorded.
- Batches 0 to j − 1 still execute.
- If any of them fails, the lowest such index's error is returned. Otherwise, the planning error at j is.

**Stopping is an index cutoff, not a global flag:**
- `cutoff` is the lowest failing batch index so far, kept as an atomic minimum. It starts at the planning-failure index, or the number of batches.
- Workers claim indices from a monotonic counter.
- **A claimed index below `cutoff` always runs,** even after a higher index has failed. That includes a batch waiting for the budget: a lower failure can't be stranded behind a higher one.
- **Indices at or above `cutoff` are skipped,** whether claimed, waiting or not yet started.
- Every change to `cutoff` or to the in-flight sum wakes all waiters (`notify_all`). A waiter whose index is now at or above `cutoff` leaves without running.
- Every worker is joined before `embed` returns. The error is the lowest failing index's, across both stages, and no partial result is returned.

**Panics:**
- Each batch runs under `catch_unwind`, so a panic is that batch's error, `"{op}: Candle panicked"`, at its index.
- The budget is released by a guard, on success, error and panic alike.

**The boundary:** the whole two-stage run happens inside 0129's `worker::run`.

## 4. Controls

**Equivalence** (test-support W override): W = 1 against W = 8 against the default, bit identical. This covers every 0131 control (alone, mixed, empty, whitespace, truncation, 33 texts, halved batches) and the 10,000-text corpus. All are also checked bit for bit against 0131's Rust twin.

**Order:** batches with very different lengths, so later batches finish first, still give rows in input order.

**Errors and panics** (test-support fault injection, by batch index and stage):
- a failing middle batch returns its own error;
- two failing batches return the lower index's;
- a panicking batch returns "Candle panicked";
- **across stages:**
  - an execution failure at index 2 with a planning failure at index 5 returns index 2's;
  - a planning failure at 2 with an execution failure injected at 5 returns the planning error, and batch 5 never runs;
- **the cutoff, with a deterministic barrier** (test-support hooks):
  - the higher-index batch (say 6) is forced to fail first, while the lower failing batch (say 3) is held waiting for the budget;
  - batch 3 must still run, and its error is returned;
  - batches above 3 are skipped;
- in every case, all workers are joined (the worker count drops to zero), the budget returns to zero, and the script's list stays usable.

**The budget:**
- with a test-support lowered `AGG`, the recorded maximum in-flight sum never exceeds it, and the recorded peak concurrency matches what the budget allows;
- a batch larger than a lowered `AGG` is refused once its shape is known, during planning, before any waiting. It follows the planning-failure rule above, so it is never a deadlock.
- **the 512-token control:** a generated model with `max_seq_length` 512, checked for the planned-storage accounting and for the results being identical with W = 1.

**0131's refusals** are unchanged: same messages, same order.

## 5. Measurements (`probes/0132`)

- **Throughput:** one query, a batch of 32, 60 docs and 10,000 texts, against 0131 and PyTorch (`probes/0131/reference.py`), with medians and the thread count. Tokenization's share is reported separately. **Single-query latency is measured** against 0131.
- **The W sweep:** W = 1, 4, 8, 16, 20 and 28 on the 10,000-text corpus.
- **A smaller machine:** the same corpus under `taskset` with 8 CPUs, so the default doesn't oversubscribe badly.
- **Memory:** stage live figures and high-water marks (peak RSS and the allocator peak), with the components reported separately: the planned buffers and batch metadata, the output, the resident model, and the transients.
  - For the 10,000-text corpus.
  - For a worst case of 512 texts at 256 tokens, which drives the budget to its 3-batch limit: peak RSS against the budget's 512 MiB plus the model.
- **Launch:** unchanged, since nothing runs until `embed`.

## Gates

- **The workflow:** 0131's script, unchanged, runs in a Polars + Candle project, with its 480 scores bit for bit against 0131's twin; and with `:dep polars candle` after push.
- **Suites:** core, Polars' three, Candle's and the project tool's.
- **The freeze** is unchanged.
- **Platforms:** Linux only.

## Stop rules

- If concurrent batches can't match the sequential results bit for bit: stop and report why. (They should: each batch is computed independently on the same partition.)
- If the 1.2× target isn't reached within the budget: report the measured throughput and the limiting factor. Neither the budget nor the target is changed silently.

## Out of scope

- GPU.
- Changing Candle's threading or environment variables.
- Cancelling an `embed` in progress (an embed was uninterruptible before, and still is).
- Concurrency across separate `embed` calls.
- Pipelining tokenization with execution (measured first).
