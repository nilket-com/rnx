# rnx 0135 evidence: long-text throughput, and the project-run budget

**The result:**
- **Part 1, the project-run budget (0133 F4b):** `rnx project run --budget N` works, and the halt names the flag of the path that ran. 0133's U1, as a project run with Polars and Candle, halts at the default budget, completes with `--budget`, and equals its twin.
- **Part 2, the throughput (0133 F13):** D1's 3,658 passages take **61.1 s (median of three), against PyTorch's contemporaneous 50.4 s (1.21×),** inside the 1.5× target. 0133 measured 364 to 403 s, and S0 measures 358.3 s here. The 10,000 short texts are unchanged.
  - **The schedule:** S1, a length-aware batch size at a target concurrency of 32.
  - **The finding that made S1 cheap:** halving re-tokenized. Each text is now tokenized once.
  - **A second finding:** 0132's transient factor wasn't conservative. It fell short at 512 tokens and, after review, for hidden-dominated configurations (up to 2.27×). The estimate is now per term, calibrated across the configurations the loader admits, and the fail-closed calibration gates pass: 384 configuration cases run, 27 pinned-model shapes run, and every one sits below its estimate.

## Part 1: `rnx project run --budget N`

**The change:**
- **The project tool** (`tools/project/src/workflow.rs`) accepts `--budget N` once, for `run` only. N must be 1 to `usize::MAX − 1`, `run`'s own range, and it's refused by name before the manifest is opened.
- **Forwarding:** the tool passes `--budget-hint project` and `--budget N` to the artifact's `run`, before `--source-map` and the entry (`assembly::command_checked` takes the flags).
- **The runner** (`src/runner.rs`) learns the hint (`runner::launched_by_project`), and its two halt messages name `rnx project run --budget N` on that path. Plain `rnx run` keeps `--budget N`, and `--budget-hint` accepts only `project`.

**Controls:**
- **The project tool** (`project_run_budget_is_checked_before_any_launch`): `0`, `-5`, `lots`, `1.5` and `usize::MAX` are each refused by name with the range; a missing value; a duplicate; `--budget` on `session`, `eval` and `build`; the largest value passes parsing.
  - `project_run_forwards_the_budget_before_the_source_map`: the forwarded flags, with and without a budget.
- **Core** (`tests/budget.rs`, `the_halt_names_the_flag_of_the_path_that_ran`): the halt text on both paths, and an unknown hint refused before the script starts.
- **End to end** (`probes/0135/project_budget.sh`, `out/project-budget.txt`): U1 composed as a project of Polars and Candle, from a clean worktree at `818443e`.
  - **At the default budget:** `halted: 2000000 instructions exceeded; rnx project run --budget N raises it`, exit 1.
  - **With `--budget 2000000000`:** it completes in 397 s (the S0 adapter at that commit), and all 25 rows equal 0133's twin.
  - **The path credit,** reported apart: 0133's F4b family is cleared on the `project run` path.

## Part 2: throughput

### 2a. Measured before choosing

**The probe:** `probes/0135/probe`, `probe0135`. D1's passages come from `twin0133 passages`, chunked exactly as U1 chunks them. The 10,000 texts are 0131's corpus. The model is 0131's pinned MiniLM, on one 28-CPU machine.

**First sweep, the S1 knob alone** (re-tokenizing on every halving; this sweep's numbers are superseded below):

| schedule | total | planning | execution |
|---|---:|---:|---:|
| S0 | 347.1 s | 1.08 s | 346.0 s |
| S1, C = 4 | 95.6 s | 8.19 s | 87.4 s |
| S1, C = 8 | 86.1 s | 17.98 s | 68.1 s |
| S1, C = 16 | 98.0 s | 37.84 s | 60.2 s |
| S1, C = 28 | 128.0 s | 64.03 s | 64.0 s |
| S0 at 4 × `AGG` (information only) | 98.4 s | 1.07 s | 97.4 s |

- **What it established:** execution scales with concurrency, 346 s down to 60 s. Candle runs one long batch on few cores, so long batches running one at a time leave the machine idle.
- **The new bottleneck:** halving re-tokenized the same texts, 32 + 16 + 8 + 4 encodes for a batch of 4.

**The fix, in `plan`:**
- **Tokenized once:** each text is tokenized once, unpadded; truncation is per text.
- **Padding:** a batch pads its own texts to its longest, on the right, with the pad id and a zero mask. That's exactly the tokenizer's `BatchLongest` padding for that batch, so halving re-pads instead of re-tokenizing.
- **The cache:** it holds at most one batch window. The planned ids and mask stay within `PLAN_PAYLOAD`, the only token copy that outlives the window.

**Bit for bit against an independent twin** (`out/bitwise.txt`). `twin0133 embed` tokenizes each batch itself, with the tokenizer's own padding, on the identical partition:

| partition | batches | rnx against the twin |
|---|---:|---|
| S0 | 115 | bit-equal, 5,618,688 bytes |
| S1, C = 8 | 914 | bit-equal |
| S1, C = 16 | 1,805 | bit-equal |

**Planner check:** the S1 rows above were dumped under round 1's estimate; their partitions are still bit-equal to the twin. The chosen default, under the final estimate, is checked the same way:

| partition | batches | rnx against the twin | against S0 |
|---|---:|---|---|
| default (S1, C = 32) | 3,441 | bit-equal | 3,625 of 3,658 rows bit-identical, lowest cosine 1.000000000 |

**The sweep under the final estimate** (2c), uncontended (`out/schedule.txt`):

| schedule | D1, 3,658 passages | batches | max running | allocator peak |
|---|---:|---:|---:|---:|
| S0 | 358.3 s | 115 | 2 | +740 MiB |
| S1, C = 8 | 74.4 s | 915 | 11 | +636 MiB |
| S1, C = 12 | 61.6 s | 1,712 | 22 | +532 MiB |
| S1, C = 16 | 64.2 s | 1,821 | 25 | +595 MiB |
| S1, C = 24 | 62.0 s | 3,103 | 28 | +366 MiB |
| S1, C = 32 | 60.5 s | 3,441 | 28 | +307 MiB |

**Choosing C.** One run each was within noise, so C = 12, 16 and 32 were run interleaved, three times each:

| C | runs | median | allocator peak |
|---|---|---:|---:|
| 12 | 67.1, 69.3, 70.1 s | 69.3 s | +548 to +582 MiB |
| 16 | 69.7, 68.9, 69.7 s | 69.7 s | about +530 MiB |
| **32** | **60.7, 61.1, 61.4 s** | **61.1 s** | **+290 to +314 MiB** |

- **PyTorch, contemporaneous** (`probes/0133/diagnostics/torch_timing.py`, right after; PyTorch 2.14.1, 20 threads, batch 32): 50.7, 50.2 and 50.4 s, a median of 50.4 s. A first triple of runs in round 1 measured 50.2, 50.4 and 50.3 s.
- **The target:** at most 1.5 × 50.4 = 75.6 s. S1 at C = 32 takes 61.1 s, **1.21×.**
- **At C = 32** the budget isn't the limit (at most 776 of 1,024 MiB in flight): the 28 workers are. Batches are 1 or 2 long texts.

**S2 (sort by length) and S3 were not built:**
- **S2's ceiling, measured** (`probe0135 waste`): S0's padding wastes 19.5% of D1's padded tokens. Sorting by length removes 19.1% of padded tokens and 28.4% of the attention work. So S2 could save at most about a fifth of execution.
- **Its cost:** batches that aren't contiguous, with the error-priority contract from the plan's review.
- **Why not:** S1 met the target with contiguous batches, so 0132's ordering and error semantics hold unchanged. The plan allows finishing with S0 and S1.

**The choice: S1 at `CONCURRENCY = 32`,** the fastest, with the lowest allocator peak. `concurrency: Some(1)` is 0131's batching exactly: C = 1 has no share, so a batch over the budget is still refused, not halved.

### 2b. What didn't change

S1 keeps batches contiguous and in input order. So the following are 0132's code, unchanged:
- ordered outputs and joined workers;
- the lowest-index error arbitration across planning and execution;
- the permit-based release, the shared preflight and conservative admission.

0132's controls pass under the new default: W equivalence, out-of-order completion, errors and panics (including after the model step), the cutoff barrier and the budget.

**A new control** (`the_in_flight_budget_bounds_concurrency_and_refuses_what_cannot_fit`): at a budget below the largest batch, S0 (`concurrency: Some(1)`) refuses as before. S1 halves the batch to fit its share, and every text is embedded.

### 2c. The in-flight estimate, re-derived per term

**0132's single factor of 4** applied to hidden plus feed-forward plus attention, and was calibrated on the pinned model at 256 tokens. It wasn't conservative:
- at 512 tokens (1.05 × the estimate);
- **for hidden-dominated configurations,** found in review: up to 2.27 ×, for example hidden 1024, feed-forward 1, 1 head.

The round-1 fix (`2 × states + 6 × attention`, fitted on the pinned model only) failed the same configurations.

**The measurement, replayable from the committed probe:**
- **Both commands are gates** that exit non-zero on any failure, with failure controls (`probes/0135/calibration_controls.sh`, `out/calibration-controls.txt`).
- **`probe0135 configs`** (`out/configs.tsv`) runs the adapter's own batch path, `run_batch` (forward and pooling), on zero-weight models built through `text::synthetic_batch` (`test-support`). Each case is warmed once, then its allocator peak is compared with its estimate. Every row prints its terms and `ok`, a named skip, or `FAIL`.
- **The grid:** 11 configurations at the admitted extremes (MiniLM-like; hidden-dominated at 1024 and 512; feed-forward-dominated; attention-dominated with 64 heads; BERT-large-like; all-maximal at 1024 / 4096 / 64 heads; tiny; mid; feed-forward-mid; hidden-and-feed-forward). Each runs 6 batch shapes up to 4 × 512, at 1, 2 and 12 layers, with all or half the tokens unmasked, alone and four batches at once.
- **`probe0135 case`** isolates one: depth grows the peak from 1 layer to 2 and then stops (6.10 hidden-state tensors from 2 to 24 layers, hidden-dominated).
- **`probe0135 calibrate`** (`out/calibrate.txt`): exact single-batch calibration on the pinned weights at 64, 128, 256, 384 and 512 tokens, 1 to 32 texts, alone, with S0 pinned (`concurrency: Some(1)`).
  - **Ruled out before running:** a shape the activation caps rule out is skipped by name (`text::full_length_fits`), never split and reported as that shape.
  - **Failing rows:** a run fails if its batches aren't exactly `[b]`, if its peak is above its estimate, or if its sequence isn't the encoder's full length.
  - **How the length is checked:** with W = 1, the in-flight maximum is the one batch's estimate, which rises strictly with the sequence length. So the actual length is recovered from it (`text::estimate_for`), compared with `text::max_seq`, and printed with b.
- **The grid is accounted:** `configs` enumerates its 396 expected cases (11 configurations × 6 shapes × 6 variants). Each must run or be skipped by name because the caps rule its shape out. Any other construction error, or a missing case, fails.

**Raw peak-to-term ratios** (a peak divided by one term, so each includes the other terms and the overhead): up to about 5.9 for hidden states (hidden-dominated), 2.2 for feed-forward states (feed-forward-dominated; about 3 MiB of it is per-batch overhead), and 5.3 for attention scores (attention-dominated). They bound the choice of coefficients, but they are not each a coefficient's floor. `FFN = 2` is below the raw 2.2 because the 4 MiB `FIXED` term carries that overhead. **What's validated is the combined estimate,** by the gate below.

**The estimate is now** `7 × hidden + 2 × feed-forward + 6 × attention + 4 MiB` per batch (`HIDDEN`, `FFN`, `ATTENTION`, `FIXED`):
- **`configs`:** of 396 expected cases, 384 ran and sit below their estimate, 12 were skipped by name as above the caps, and none failed. The worst is 0.896 alone (feed-forward-dominated) and 0.895 four at once. The worst per family: hidden-dominated 0.797, attention-dominated 0.834, all-maximal 0.804, MiniLM-like 0.784.
- **`calibrate`:** of 30 asked shapes, 27 ran, each at its full length, and sit below their estimate (0.41 to 0.79). Three were skipped by name as above the caps: 32 texts at 384 tokens, and 13 and 32 at 512. Round 2's evidence wrongly counted these three as checked: the planner had split them.
- **The failure controls** (`out/calibration-controls.txt`) all behave as required:
  - `calibrate` passes at 256 tokens, and at 384 with 32 skipped;
  - it fails with the S0 pin replaced by C = 32 (split batches), with the estimate scaled to 0.5, and on texts too short to reach the full length (40 strings `"a"` at the 64-token model, found in review: they had passed at 42 tokens);
  - `configs` fails with a configuration the loader refuses (hidden 8, 3 heads: 36 failures), and with the estimate scaled to 0.5 (290 failures).
- **The no-deadlock assertion** holds at 0131's caps: 265.3M ≤ 268.4M values.

**The departure from the plan, amended in the plan itself** (2c, "this rule is replaced"):
- The plan said the factor only ever rises. The hidden (7) and attention (6) coefficients rise above 0132's 4, and the feed-forward coefficient falls to 2.
- A feed-forward coefficient of 4 alongside those would break the no-deadlock assertion (298.8M > `AGG`).
- The replacing rule is on the combination, not each coefficient: every case of the fail-closed grid sits at or below its estimate.

### 2d. Correctness

**Gate 1, bit for bit on the chosen partition:** see `out/bitwise.txt` above. The complete workflow outputs are under "The workflows" below.

**Gate 2, against the old partition:**
- **Embeddings** (`compare.py embeddings`): every row's cosine with S0 is 1.000000000. That's 3,625 of 3,658 rows bit-identical at the chosen C = 32; round 1 measured 3,658 at C = 8 and 3,654 at C = 16.
- **Workflow outputs:** see "The workflows".

**The harness** (`probes/0135/compare.py`) keeps the two gates apart:
- **Strict parsing:** each file's header and every row's field count are checked. A duplicate identity, or a ticket in two components, is refused before any comparison. The row count is the count of identities.
- **Gate 2's exact part:** identities, ranks, bands, duplicate membership and U3's components and edges are exact. Scores are within 1e-5, and NaN or infinite values are refused.
- **Twelve mutation controls,** each failing the gate it targets:
  - one f32 bit fails gate 1 only; a score moved 2e-5 fails gate 2;
  - a changed document with identical numbers fails gate 2's exact part;
  - a NaN is refused by both;
  - review round 1's cases, refused by both gates: a duplicated U1, U2 or U3 row; a U3 row with an extra field; a ticket moved to another component; a ticket in two components; a removed edge; a header without its score column.
- **0133's replay** and its exact comparison are unchanged.

**The workflows** (`probes/0135/replay.sh`, `out/replay.txt`): 0133's U1, U2 and U3, each pasted into a session on `runner0134` with this tree's adapters. Each passed on its marker with a final `Ok(())`.

| workflow | session run | 0133 | gate 1 (twin at C = 32) | gate 2 (against 0133) |
|---|---:|---:|---|---|
| U1, semantic search over D1 | **60.1 s** | 364 to 403 s | 25 rows, bit-equal | identities exact, largest score change 0 |
| U2, near-duplicates in D1 and D2 | **66.9 s** | more than 360 s (0133 evidence) | 459 rows, bit-equal | identities and bands exact, largest change 1.2e-7 |
| U3, grouping D2 tickets | 3.8 s | | 254 rows, bit-equal | components and edges exact, largest change 0 |

- **The twin derives the S1 partition on its own** (`TWIN0135_CONCURRENCY`, its own tokenization and its own copy of the estimate rule). So gate 1 also proves the two partitions agree.
- **The mutation controls** behave as intended against the real outputs (`out/replay.txt`).

## 3. The target and the stop rule

- **The target:** met, at 61.1 s against 75.6 s.
- **No short-text regression** (`out/short.txt`; the 10,000-text corpus, interleaved, 5 runs each):
  - S0: 10.2, 12.8, 12.6, 12.7 and 12.6 s, a median of 12.6 s and a spread of 2.6 s;
  - the default: 10.3, 12.6, 12.7, 12.6 and 12.6 s, a median of 12.6 s;
  - the default plans 316 batches against S0's 313, because a few long-ish short batches halve under C = 32's share;
  - 0132's 12.55 s is for context.

## Gates

- **Suites:**

| suite | passed |
|---|---:|
| Candle, release | 41 |
| Candle, release `test-support` | 41 |
| Candle, debug `test-support` | 41 |
| Polars, release | 41 |
| project tool | 83 (+2) |
| `rnx` core | 392 (+1) |

  Clippy is clean on Candle with `test-support`.
- **0133's replay:** unchanged, and not loosened. The D1 workflows' old outputs are compared by this record's gate 2.
- **`:dep polars candle`, after push** (`out/dep/`): a clean worktree binary at the pushed `f0ee81f`, a fresh cache, and ordinary `:dep polars candle` sessions (`probes/0135/replay.sh`).
  - **The sessions:** U1, U2 and U3 each pass on their marker with a final `Ok(())`. Their run times are 59.5 s, 66.4 s and 3.7 s. The first `:dep` built the adapters in 389.5 s, and later sessions attached in about 16 s.
  - **Gate 1:** bit-equal to the twins at C = 32, with 25, 459 and 254 rows.
  - **Gate 2:** identities exact against 0133, with a largest score change of 1.2e-7.
  - **The controls:** all 12 behave as required.
  - **One harness note:** the worktree had no built twin, so the replay stopped at that step. The twin step and both gates were re-run with the main tree's `twin0133`, whose source is identical at `f0ee81f`.
