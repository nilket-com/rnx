# Implementation plan: ADR 0234

Status: Accepted

Start from `bc4a2065b04f9f023f1275de63b7e15deabc37db`. Commit the plan pair before implementation; use one signed implementation
commit.

Boundary and audit:
1. Define a predicate-only 4096 constant, visible to its allocator. In
   `column/predicate.rs`, use it consistently for WORDS, count cap, slot
   offsets, split lengths, and Sources::slot subtraction. Assert positive
   size and divisibility by 64. Leave shared BLOCK untouched.
2. In ordinary `allocate_batch`, select the larger scratch stride only when
   `self.special && self.driver() == Driver::Predicate`. Reuse the existing
   driver classifier. Preserve all other allocation sizes and checked memory
   accounting. Do not change PreparedBatch layout, route admission, or add
   another driver instance. Inspect generated setup code before measurements.
3. `allocate_batch_reusing` admits F64 outputs only; predicates fall back to
   ordinary allocation before its two scratch calculations. Leave those
   calculations unchanged and test this fallback. `owned_bytes` already uses
   actual scratch capacity and must continue to do so.
4. Keep `masked::intersect_validity` at BLOCK=1024. It builds the full output
   bitmap before execution. `BitSlice::read_words` checks the requested length
   and exact ceil(len/64) destination length, not BLOCK; predicate reads can
   span 4096 rows. Preserve unaligned physical offsets and bounded tails.
   `append_words` publishes arbitrary bit ranges. Test both at chunk seams.
5. Retain minimum-of-all-input-chunks count selection, including empty-chunk
   skipping. Numeric intermediates require a full 4096-element slot; packed
   intermediates use the first ceil(count/64) words of that same slot.

Committed tests:
- Lengths 0/1/63/64/65/1023/1024/1025/4095/4096/4097/8191/8192/8193;
  bit offsets 0/1/7/63, mismatched input chunks, empty chunks, seams just
  before/after 1024 and 4096, and partial final words.
- All six I64 comparisons, pair and both constant orders, signed extremes,
  native and force_scalar; F64 comparisons including NaN/infinity/signed zero;
  F64 arithmetic feeding comparisons and Boolean combinations with live/reused
  intermediate slots. Strict-null and invalid-tail parity with an independent
  oracle; unchanged Path and fuel charge.
- Predicate scratch capacity and owned-byte accounting, checked size overflow,
  tight memory refusal, and reuse fallback to normal predicate allocation.
- Cancel before execution, between full/shortened blocks, and at the final
  poll. Check no completed output on refusal. Scope instrumentation to tests.
- Scorer controls reject an extra instruction beyond a reviewed formula,
  unexpected unchanged-path slope, wrong pair identity, and missing coverage.

Ordered gates:
1. Local clippy and focused core/hardware tests; scoped Miri for portable,
   allocation, offsets, scratch reuse, and cancellation. Deterministic-floats
   only for suites whose existing parity contract requires it.
2. Signed paired production builds from fresh hash exports, no worktrees.
   Instance guard and resolved identity before SMALL. Report changed predicate
   frames separately; no blanket exemption for unrelated routines.
3. SMALL protected matrix plus predicate cases: i64eq/ne/lt/le/gt/ge,
   f64gt/f64le, mixedor and arithmetic predicates, in flat, alternating,
   random, and sliced-chunked-null layouts. Include the portable harness.
   Derive explicit block/chunk call-count formulas from per-function vectors
   and disassembly. Separate the unchanged 1024 validity cost from predicate
   costs when comparing the global sweep. Review pins and timing targets
   before large capture; no automatic inheritance of old-pair pins.
4. Durable parallel Ir at 10M/100M for changed predicates and required
   protected controls; judge total Ir and split program/system/unknown costs.
   Record actual chunk lengths rather than assuming ceil(rows/4096) for
   sliced inputs. Preserve all failures, no flake retries.
5. Separate quiet timing: six fresh processes/build/cell, one first and five
   warm calls, fixed 120-second cooldown per round, both sizes, all four
   layouts for the I64 and mixed/F64 cases above. Alternate AB/BA by round for an exact 3:3 split in every cell. Verify
   the schedule cannot collapse to a fixed order. Report process medians,
   observed ranges, faults, conditions, and first/warm results. Estimate wall
   time before launch; any matrix reduction needs review first.
6. Host, integration, generated-projects, cross, gate-policy finalizer and
   commit checker. Terminal PASS requires both child exit success and valid
   receipts. Reuse evidence only under explicit source-scope proof.

Send the publication packet with changed-route curves, unchanged-route proof,
actual scratch/cancellation costs, and limitations. Publish only after review
and all required gates pass. The bench follower reports driver-specific
results and retains the shared 1024 decision; no cross-day comparison claims.

## Disposition: withdrawn

The 4096-row implementation is preserved in a verified local Git bundle,
with its original SMALL, LARGE and TIMING receipts. No runtime change ships.
Production predicate.rs and run.rs are restored exactly to published
`bc4a2065b04f9f023f1275de63b7e15deabc37db`. Boundary tests and measurement tools, including exact pins, remain in the
verified archive. The withdrawal commit changes only this paired documentation,
as required by the commit checker.

All large instruction-count gates passed. TIMING-v1 completed successfully
as a capture but FAILED acceptance: at 100M, both motivating expressions had
to improve by at least 5% in every layout. Four cells missed that target.
There is no rescoring, retry, or post-result exception.

| Expression | Layout | Before ms | After ms | Change |
|---|---|---:|---:|---:|
| i64eq_pair | flat | 92.0 | 82.2 | -10.7% |
| i64eq_pair | alternating | 88.3 | 91.7 | +3.9% |
| i64eq_pair | deterministic-random | 95.7 | 87.3 | -8.8% |
| i64eq_pair | sliced-chunked-null | 77.5 | 72.1 | -6.9% |
| mixedor | flat | 125.1 | 115.2 | -7.9% |
| mixedor | alternating | 139.7 | 134.1 | -4.0% |
| mixedor | deterministic-random | 133.7 | 129.8 | -2.9% |
| mixedor | sliced-chunked-null | 104.9 | 103.5 | -1.3% |

These are medians of six fresh processes, each with one first and five warm
calls, exactly three AB and three BA rounds per cell, and a fixed 120-second
cooldown per round. No first/warm case had separated slowdown ranges, but
overlap does not establish equivalence. At 64K, flat arithmetic-then-compare
had a 1.96x warm median with overlapping ranges. This is a warning signal,
consistent with the larger fixed scratch zero-fill cost, not a proven cause.

Flat gains did not establish the required broad benefit across null layouts.
A future design may size scratch to input and choose block size once per call,
with the size selected by evidence; it must be a separate record.

Withdrawal validation: prove an empty production diff against the published
baseline, run the final host/integration/generated/
cross chain with receipt and exit checks, then the commit checker. The empty
production diff replaces a redundant performance recapture. Retain all failed
experimental receipts. No benchmark follower is needed for unshipped behavior.

The initial withdrawal retained tests and tools; its core controls passed, but
FINAL-v1 failed the sequence.withdrawal policy at preflight. The corrected
withdrawal restores every non-plan file to the published baseline. No checker
exception is introduced. The original failure receipt is retained.
