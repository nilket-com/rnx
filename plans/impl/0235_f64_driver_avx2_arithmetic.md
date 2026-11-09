# Implementation plan: ADR 0235

Status: Accepted

Start from published 1fe8ba598fefd521088b8354d884a81c92c4f9dd. Commit the
accepted plan before production edits; one signed implementation commit.

1. Inventory f64_plan::lanes/arithmetic callers, Engine selection, existing
   FloatOp and primitive boundaries. Record emitted instance counts and
   protected symbols before edits. Add separate non-generic Add/Sub/Mul
   primitive entries for the driver; select Engine once per map call and
   resolve operand representation outside loops. Do not introduce generic
   driver instances, share protected raw kernels with new callers, or change
   result-array publication. Send the concrete boundary for review first.
2. Preserve the portable lanes implementation and force_scalar. Safe wrappers
   validate output/input lengths, including Words storage and constant
   operands; no materialized full-column conversions. SIMD loads/stores must
   remain within live slices. AVX2 requires runtime detection; tails use the
   same ordered operation. Keep core forbid(unsafe_code).
3. Add operation/representation parity controls over lengths
   0/1/3/4/5/7/8/9/15/16/17/63/64/65/1023/1024/1025, offsets, chunk boundaries,
   null layouts, slot reuse, output reuse, and forced scalar. Inputs include
   signed zeros, subnormals, finite extremes, infinities, quiet/signaling NaNs
   with distinct signs/payloads, and invalid inf-inf/0*inf operations.
   Use bitwise equality unless the expected arithmetic result is NaN, then
   require is_nan. Check NaN propagation through multi-node chains without
   fusing operations. Copy/store existing values retain exact bits.
4. Audit existing cross-path NaN tests, including column/tests.rs arithmetic
   and reuse comparisons and primitive tests. Change only assertions that
   promise arithmetic NaN payload parity. List each actual changed test name,
   prior assertion and replacement in the completed implementation record;
   do not broadly weaken bitmap, non-NaN, copy or ownership assertions.
5. Run clippy/core/hardware controls, then scoped Miri. Capture production
   instance and resolved-identity guards before timing. Record packed
   Add/Sub/Mul, absence of FMA and once-per-call Engine selection.
6. Capture raw SMALL protected vectors and force_scalar vectors before pins.
   Stop on unexplained slopes. Send attribution and proposed fixed setup
   pins for review; controls must reject pin+1, changed candidate identity,
   and added row/block growth. Measure all six changed cells at64K/10M.
7. Run the paired10M/100M timing with six processes, three AB/three BA per
   cell, existing per-round cooldown and quiet resource admission. Match
   inputs, call semantics and reuse policy between builds; follow the probe's
   first ordinary call plus five consuming-reuse warm calls. Oracle checks
   occur outside timers. Record faults, cycles/time, order and raw samples.
   At100M each six-cell warm median must improve>=5%; no separated slowdown
   at either size. Report first calls separately. Durable receipts permit
   resume of missing cases, not retries of completed failures.
8. Finalize host controls, integration, generated projects, cross, gate policy
   and commit checker on the signed candidate. Explain reuse of earlier
   receipts with exact source/tooling scope proof. Send publication packet;
   publish only after all required gates pass and publication review.

Evidence inventory: target/next-f64-chain/PROBE-REPORT.md, PROBE-EVIDENCE.json,
IR-v1/RESULT.json, TIMING-v1/RESULT.json; NAN-AUDIT/REPORT.md, RESULT.json,
CLASSIFICATION.json and MANIFEST.json. Preserve receipt hashes in the final
record. Probe results are not candidate gate receipts.

Bench follower after publication: same-day matched results for all six cells,
first/warm distinction, NaN contract, scoped protected coverage, and any
reviewed fixed pins. Older Arrow/Polars results are cross-day context only.
