# rnx 0137 evidence: Candle's shape, indexing and composition families, with passage-to-document pooling

**The result:**
- **Coverage:** **137 of 220 core `Tensor` identities (62.3%),** counted mechanically against the pinned source (`out/count.txt`). That's 0134's 97 plus 40 new, in G (23), H (8) and I (9), with none missing and no duplicates across the two manifests.
- **Pooling:** `candle::segment_mean`, the passage-to-document mean, completes 0136's chunking. **E4** uses it to find near-duplicate tickets from **whole** tickets, bit-equal to its twin.
- **E5,** a tour of the new families, is bit-equal to its twin.

## 1. The operations (`adapters/candle/src/tensor_ops/composition.rs`)

**The plan's amended contracts, as implemented:**
- **Bounds independent of the value count:** `AXIS` = 2^24 bounds any axis turned into allocations, and `PARTS` = 4,096 bounds a vector of tensors.
- **Checked before Candle:** every output, intermediate and expanded operand goes through 0134's `capped` (the count and the stride products).
- **Axes against element indices:** axes may be negative. Element indices are non-negative, and a negative one is refused, not wrapped.
- **Index tensors** are read and bounds-checked in one pass before Candle sees them, and the first bad position is named.

**Per operation, from the review:**
- **`repeat`:** a multiplier of 0 is refused (Candle treats it as 1). Each multiplier is at most `AXIS` (Candle's `vec![&inp; m]`), and each axis's intermediate is capped.
- **`pad_with_same`:** its reference list, left + right + 1, is at most `AXIS`.
- **`flip`:** each flipped axis is at most `AXIS` (Candle's i64 index vector).
- **`chunk`:** the count is 1 to `PARTS`.
- **`to_vec3`:** d0 and d0 × d1 list nodes are at most `AXIS`, independently of the leaves.
- **`roll`:** the axis is at most `i32::MAX` (Candle's i32 arithmetic), and any i64 shift, `MIN` included, is reduced in i64 first.
- **`unfold`:** the integer window count must equal Candle's f32 count, and the stride × step product is checked.
- **`cumsum`:** n² is checked, and batch × n² above rank 1 (`broadcast_matmul` concretizes `triu2`), so a 1-D axis is at most 4,096.
- **`slice_assign`:** Candle's mask and padded intermediates are capped. It takes `[start, end]` or `(start, end)`.

**`segment_mean(values, segments, n)`** is Candle's own calls:
- `zeros(n, w).index_add(segments, values, 0)`, divided (broadcast) by the counts from `index_add` of ones;
- an empty segment is refused by name;
- the output, counts, ones and division copy are each capped before allocation;
- non-finite values follow IEEE.

**Controls** (5 unit tests in `composition.rs`, `tests/composition_alloc.rs` and `tests/composition_script.rs`):
- **Value equality, bit for bit with the direct Candle call,** for every identity in G, H and I:
  - negative axes;
  - `roll` with shifts 1, −1, 7, `i64::MIN` and `i64::MAX`;
  - `chunk` with more chunks than elements;
  - views made contiguous;
  - both index dtypes;
  - `slice_assign` in both range forms;
  - receivers unchanged.
- **The review's refusal cases, each by name before Candle:**
  - `repeat` with 0, and a 2^30 multiplier on `zeros([0, 1])`;
  - `chunk` above `PARTS`, `flip` on `zeros([0, AXIS + 1])`, `pad_with_same`'s reference list, and `to_vec3` of `[2^20, 2^20, 0]`;
  - `roll` on `zeros([0, 2^32])`, and on an empty axis;
  - `unfold`'s f32 count disagreeing, at an axis of 2^25 + 1, with a zero-value tensor;
  - `get` on rank 0 and on an empty axis, and negative element indices with their positions;
  - index dtype and rank, `gather` and `scatter_add` shapes, and `slice_assign` ranges and source shape;
  - `cumsum` at 5,000, and a genuine batched bound;
  - `cat` and `stack` shapes and an empty list, an incompatible broadcast, a repeated permutation axis, `dot` and `mv` shapes, `round_to` and `elu` arguments.
- **Allocation peaks,** through a real Rune VM, one test: oversized `repeat`, `cat`, `unfold`, `cumsum` and `segment_mean`, and (from review round 1) a malformed `slice_assign` range and an over-long `flip` or `log_sum_exp` axes list, are each refused under 256 KiB, measured from after their inputs exist.
- **`segment_mean`:**
  - **Same-call parity, authoritative:** bit-equal with the direct Candle calls.
  - **The f64 cross-check,** gated only on its stated well-conditioned input (unit-norm rows), within 1e-6 + 1e-6·|f64|.
  - **Cancellation, reported, not gated:** f32 rows `[1e8, 1, −1e8]` give Candle 0, and the f64 mean is 1/3. The test asserts both, to keep the difference visible.
  - **NaN** propagates.
  - **Refusals:** an empty segment, out-of-range and negative ids with their positions, a row mismatch, n = 0, the rank, and the dtype.

**Review round 1, four findings, all reproduced by Codex through the Rune VM, and fixed:**
- **R1, `slice_assign` took the script's range containers.** `rune::from_value` on a nested tuple or vector moves its shared storage, so after a successful call the script's `ranges` were unreadable.
  - **The fix:** each range is **borrowed** (`borrow_tuple_ref`, or a borrowed vector), checked for exactly two integers, and only those are read.
  - **The control** (`tests/composition_script.rs`, through a real VM): the script rereads its ranges in both forms after a success, after a later range's refusal, and after the source's refusal.
- **R2, malformed containers allocated before their refusal.** A 131,072-entry inner range cost 1 MB before "needs 2 ranges", because the old conversion built the vector. 0134's `axes` reserved an unchecked length, so `flip` with 131,072 axes cost 1 MB.
  - **The fix:** the range borrow checks the length first, and `axes` refuses more axes than the rank before reserving.
  - **The controls,** in the allocation test, from after the inputs exist: `slice_assign`, `flip` and `log_sum_exp` are each refused under 256 KiB.
- **R3, metadata wrapped.** `zeros([0, i64::MAX]).repeat([1, 2])` was admitted with an axis of 2^64 − 2, which `dim` read as −2.
  - **The fix:** 0134's `capped`, which every created or inferred shape passes through, now also requires every axis and every stride product to be an `i64`. `unfold`'s stride × step is checked against `i64::MAX` too, and `dim` and `stride` read back with checked conversions.
  - **The controls:** `repeat` and `pad_with_zeros` past `i64::MAX` on a zero-value tensor are refused; a zero-value `unfold` whose stride × step passes `i64::MAX` is refused; a valid `i64::MAX` axis reads back exactly.
- **R4, `cumsum` counted the matrix's row axis as a batch.** `zeros([2048, 8]).cumsum(0)` was refused, while direct Candle succeeds with a 2048 × 2048 `triu2` (4.2M values).
  - **The fix:** the batch is the transposed shape's leading axes, excluding its own rows and columns. The expanded `triu2` operand goes through `capped`.
  - **The controls:** `[2048, 8]` along axis 0 is now accepted and bit-equal to Candle (replacing the test that had expected the wrong refusal). `[5, 3, 2048]` along the last axis, 5 × 2048 × 2048, is a genuine batched refusal.

## 2. Examples (`probes/0137`, `out/examples.txt`)

**E5, the tour** (`e5_tour.rn`): cat, stack, permute, index_select, gather, cumsum, unfold, slice_assign (with its receiver unchanged), index_add and segment_mean on small readable tensors. Its 11 results are **equal in shape and f32 bits** to `twin0133 e5`'s.

**E4, near-duplicate tickets from whole tickets** (`e4_pooled_duplicates.rn`, Candle only):
- **The steps:** each D2 ticket (title, newline, the whole body) is chunked by `enc.chunk(text, 0)`. The passages are embedded and pooled per ticket by `segment_mean`, the rows renormalized, scored by `matmul`, and every pair at or above U3's frozen 0.80 written out.
- **The run:** 252 tickets and 458 passages; embedding takes 5.8 s, and the session run 8.0 s.
- **Parity:** `twin0133 e4` chunks independently (0136's twin chunker), embeds on 0135's S1 partition, and pools by the same Candle calls. Its 3 pairs are **equal in f32 score bits.**
- **The finding, reported and not gated:** whole-ticket pooling changes which pairs reach 0.80.

| | pairs at or above 0.80 |
|---|---|
| 0133's U3 (title + 180 words, truncated) | #521 / #525 (0.813), #23 / #583 (0.832) |
| E4 (whole tickets, pooled) | #724 / #775 (0.811), #755 / #810 (0.862), #909 / #912 (0.803) |

- **Judging the pairs by their titles:**
  - #909 "async paramters error" and #912 "async is not Work" are a plausible duplicate;
  - #724 and #775 are two LSP setup failures;
  - #755 (`#[rune::function]` with a 6th argument) and #810 (`Any::downcast_borrow_ref` with eyre) aren't duplicates, though both are binding-error reports with similar templates.
- **0133's two pairs** fall below 0.80 once each ticket's whole body counts. This is one corpus at one threshold; it shows the pooling's effect, not a better detector.

## Gates

- **The count:** `count.py` on both manifests reports 137 of 220, none missing and no duplicates, and exits 0 (`out/count.txt`).
- **E4 and E5** are bit-equal to their twins; the `segment_mean` cross-check passes on its stated input.
- **Launch** (`probes/0137/launch.py`, 3 interleaved rounds against 0136's `a959f21`): deltas −0.22, −0.24 and +0.10 ms, within noise for 41 more registrations.
- **Suites:**

| suite | passed |
|---|---:|
| Candle, release | 61 (+7) |
| Candle, release `test-support` | 61 (+7) |
| Candle, debug `test-support` | 61 (+7) |
| Polars, release | 41 |
| project tool | 82 or 83 (a pre-existing flaky test, below) |
| `rnx` core | 392 |

  Clippy is clean on Candle with and without `test-support`. After review round 1, the suites, E4, E5 and both replays were rerun: E4 and E5 are still bit-equal, and the replays pass.

  **A pre-existing flaky test, reported and not fixed here:** `workflow::quiet::tests::a_flooding_child_is_drained_and_reported` (0070's) redirects the test process's own stdout and stderr into its `dep.log`. Libtest's line for any test that finishes in parallel during that window lands in the log, and its prefix assertion fails ("reopen with: fixture\ntest tests::…").
  - **Measured:** it failed in 3 of 8 full runs, and in 3 of 8 with 0135's project tests skipped, so 0135's tests aren't the cause.
  - **The tool's code is unchanged since 0135.** Isolating the test (running it in a child process, or serializing it) is a follow-up, outside this record's scope.
- **0135's and 0136's replays,** on this adapter (`out/replays.txt`), pass unchanged:
  - 0135's U1, U2 and U3 are bit-equal to their twins, with gate 2's largest change 1.2e-7 and all 12 controls ok;
  - 0136's U1′ is bit-equal at both overlaps, with hit@1 5 of 5 and 4 of 5.
- **After push** (`out/dep/after-push.txt`): a clean worktree binary at the pushed `ad94814`, with a fresh cache.
  - **In ordinary `:dep candle` sessions:** E5 passes and is equal to its twin, with 11 results. E4 passes in 8.0 s, with 3 pairs equal in f32 bits.
  - **In `:dep polars candle` sessions,** through 0136's replay: U1′ is bit-equal to its twin at overlap 0 (70.9 s, hit@1 5 of 5) and at 32 (80.3 s, hit@1 4 of 5).
  - **The first run of this check** stopped when the scratch filesystem's quota filled; old caches from closed records were cleared and it was rerun.

## Not done

- the excluded identities (plan §1), GPU, training, and new architectures;
- a quality claim for E4's pairs beyond the reported finding.
