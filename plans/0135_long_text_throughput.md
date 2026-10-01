# rnx 0135: long-text embedding throughput, with the project-run budget first

Status: plan, agreed in order with Codex after 0133's findings and 0134; amended after Codex's review (error priority under sorted batching, two comparison gates, a contemporaneous short-text baseline). It has two parts, kept separate in the code, the controls and the evidence.

- **Part 1 (0133 F4b), a CLI prerequisite.** `rnx project run` has no budget option, yet its halt message tells the user `--budget N raises it`. Any real document workflow is blocked on that path.
- **Part 2 (0133 F13), the throughput.** Embedding real documents is about 8× slower than PyTorch on the same CPU: 0133's 3,658 D1 passages took 364 to 403 s, against PyTorch's 49 to 52.5 s.
  - **Established:** 0132's calibrated budget serializes long batches. A 32 × 256-token batch is estimated at 624 MiB against the 1 GiB `AGG`, so one runs at a time. 0133's twin, embedding sequentially, took 371 s.
  - **A hypothesis to measure, not a claim:** smaller batches, or a schedule by padded length, recover the concurrency.

## Part 1: `rnx project run --budget N`

**The option:** `rnx project run --manifest M [--budget N] [-- args]`, run's own `--budget` semantics.
- **N:** 1 to `runner::LARGEST_BUDGET`, refused by name otherwise.
- **Forwarded:** to the assembled artifact's `run` command as `--budget N`, before `--source-map` and the entry.
- **Accepted once:** a repeated `--budget` is refused, as other duplicates are.

**The halt message** names a flag that exists on the path that ran:
- `rnx run` keeps "`--budget N` raises it";
- a project run says "`rnx project run --budget N` raises it".

The runner learns which by a flag the project tool passes (`--budget-hint project`). The plain `run` default is unchanged.

**Controls** (project tool tests and a probe):
- a project script that halts at the default budget;
- the same script passing with `--budget`;
- `--budget 0`, a non-number, a value above the largest, and a duplicate, each refused by name;
- the halt text on both paths;
- **0133's U1 composed as a project run with `--budget`.** This is the path credit F4b's family carried in 0133, reported separately from Part 2.

## Part 2: throughput

### 2a. Measure before choosing (the baseline is unchanged 0132)

**The same model, inputs, tokenization and thread settings for every candidate:**
- 0133's D1 passages (3,658, mostly near 256 tokens);
- 0132's short corpus (10,000 texts);
- 60 docs;
- one query.

**Timing:** end to end, plus the stage times (planning and tokenizing, then execution) from 0132's `Trace`.

**The candidate schedules,** each within 0132's per-batch caps and its `AGG`, admission and ordering:

| candidate | schedule |
|---|---|
| S0 | the unchanged baseline |
| S1 | a length-aware batch size: start at 32 and halve until the batch's in-flight estimate is at most `AGG / C`, for a target concurrency C (for example C = W, or 8), so long batches run several at a time |
| S2 | sort by tokenized length, then batch, so padding waste falls; outputs are restored to input order through the recorded permutation |
| S3 | S1 and S2 together |

**The sweep:** C values and batch sizes, reported with the in-flight estimate, the allocator peak and RSS kept separate. `AGG` is not a process-RSS bound.

**PyTorch, contemporaneous:** `probes/0133/diagnostics/torch_timing.py`, on the same machine and in the same session as the candidates.

**The choice:** the fastest candidate that meets the gates. The evidence states why.

### 2b. What doesn't change

- ordered outputs, by construction;
- joined workers;
- the lowest-index error arbitration across planning and execution;
- the permit-based budget release;
- the shared preflight;
- conservative admission (`AGG`, the compile-time no-deadlock assertion).

**Error priority when batches aren't contiguous (S2 and S3).** "Lowest index" means the lowest **original input** index, as a script sees it, never the sorted order:
- **A per-text planning failure** (tokenizing, a length refusal) has its own text's original input index.
- **A batch-level failure** (a model error or a panic in a worker) has the batch's **minimum original input index** as its priority. It names the batch and its index range, not a specific text, since the model step can't attribute a failure to one text.
- **The cutoff** is the lowest priority failed so far. Planning and execution stop only batches that **cannot beat it**, meaning a batch whose minimum original index is above the cutoff. A batch later in sorted order that holds an earlier input still runs. Stopping a sorted prefix is not enough, and the controls prove that.
- **Budget waiters** follow the same rule: a waiter whose minimum original index is below the cutoff keeps waiting and runs; one above it is released without running.
- **Successful outputs** are restored to input order through the recorded permutation, and the restoration is checked as a permutation (each slot is filled exactly once).

**The planning-storage bound holds.** Token ids, masks, lengths and the permutation are all counted in the shared preflight, with no second unaccounted copy of the tokens: sorting moves indices, not token vectors.

**If these semantics can't be kept in S2 or S3,** the candidate is excluded, and the record finishes with S0 and S1 under the stop rule.

### 2c. The transient factor

**`TRANSIENT = 4` was calibrated at 256 tokens** (a real peak at most 0.92 of the estimate).

**If the chosen schedule changes the batch shapes in flight,** the factor is re-measured:
- across the supported sequence lengths, including 512 (a 512-token generated model, as in 0132's control);
- across concurrent shapes (several smaller batches at once);
- with the allocator peak against the estimate, reported per shape.

The factor only ever rises to stay conservative.

**Amended after the implementation's first review: this rule is replaced.** One factor across all three terms cannot be both conservative and within the no-deadlock assertion:
- **The measurement:** across the configurations the loader admits, hidden states cost up to about 6 tensors each, feed-forward states about 2, and attention scores about 5. 0132's single factor of 4, calibrated on one model at 256 tokens, fell short for hidden-dominated configurations (up to 2.27×) and at 512 tokens.
- **The replacement rule:** the estimate has one coefficient per term (hidden, feed-forward, attention) and a fixed per-batch term. The coefficients are not each a measured maximum: a raw peak-to-term ratio includes the other terms and the overhead. The rule is on the combination: every case of a fail-closed grid across the admitted configurations, shapes and depths, alone and concurrent, must sit at or below its estimate, with expected coverage accounted and failure controls proving the gate fails.
- **The departure, stated:** the hidden and attention coefficients rise above 0132's 4, and the feed-forward coefficient falls below it. A feed-forward coefficient of 4 alongside the measured hidden and attention coefficients would break the compile-time no-deadlock assertion at 0131's caps.
- **The evidence:** that grid (warmed allocation controls across hidden-, feed-forward- and attention-dominated configurations, alone and concurrent), replayable from the committed probe and exiting non-zero on any failure.

### 2d. Correctness

**A new partition** (S1's batch sizes, or S2's order) means a **bitwise Rust twin on the identical new partition**: the twin runs the same batches in the same composition, so every embedding is compared bit for bit.

**Two separate comparison gates,** neither standing in for the other:
1. **Bitwise, on the chosen partition:** every complete workflow output from the new adapter is compared bit for bit with an independent Rust twin running the identical batches.
2. **Tolerance, against the old partition:** the new embeddings and scores are compared with retained outputs from 0131/0132 and 0133 under a **declared tolerance**:
   - every embedding's cosine ≥ 0.999999;
   - every score within 1e-5;
   - **exact, not toleranced:** dimensions and row identities, rankings, threshold bands, duplicate membership, and U3's components and edges;
   - NaN and infinite values in either input are refused, not compared.

**The harness:** a separate 0135 comparison mode (`probes/0135/compare.py`). 0133's historical replay and its exact comparison stay unchanged, and are not loosened globally to make this pass.

**Mutation controls** break each property independently, so a tolerance can't mask a malformed output: a one-bit change (fails gate 1 only), a score moved past 1e-5 (fails gate 2), a changed row identifier or duplicate membership (fails the exact part of gate 2 even with identical numbers), and a NaN.

Bit identity across partitions is reported only if measured (0131 and 0132 both observed it for masked padding).

**The controls** carry over from 0132: equivalence across W; out-of-order completion; errors and panics, including after the model step; the cutoff barrier; the budget.

**With S2 or S3, controls where sorted order disagrees with input order:**
- **across stages:** a planning failure at a late input index while a model failure hits an earlier batch in input order (and the reverse);
- **a late sorted batch holding an earlier input** fails after an earlier sorted batch already failed: the earlier input index wins;
- **budget waiters:** a waiter holding an earlier input than the cutoff runs, and one above it is released;
- **a panic** in a batch, and its priority;
- **successful outputs** restored to input order, checked against S0.

## 3. The target and the stop rule

- **The target:** D1's 3,658 passages within **1.5× the contemporaneous PyTorch time.** About 80 s is only an approximation of that.
- **No short-text regression,** gated against a **contemporaneous S0** on the same host, not against the historical number alone:
  - the 10,000-text corpus, S0 and the chosen candidate interleaved, 5 runs each;
  - **the noise rule:** a regression is a candidate median above S0's median by more than S0's own spread (max − min) across its runs;
  - 0132's 12.55 s is reported for context.
- **The chosen candidate must pass every gate above.** Missing the 1.5× target is a reported result, not a reason to widen the record.
- **If the target is missed:** report the bounded result and the measured bottleneck (for example Candle's attention kernel, or memory bandwidth), rather than growing the record.

## Gates

- **0133's replay** (`probes/0133/replay.sh`) is unchanged. If the partition changes, its exact comparisons against old outputs are expected to fail for the D1 workflows. That is what the 0135 harness's two gates cover instead, rather than a loosened replay. If the partition is unchanged (S0 chosen), the replay passes as it is.
- **Suites:** core, Polars, Candle (release, `test-support` and debug), and the project tool.
- **`:dep polars candle`** after push.

## Out of scope

- Token-aware chunking (0136).
- New tensor operations (0137).
- GPU.
- Changing Candle's kernels or its thread pool.
