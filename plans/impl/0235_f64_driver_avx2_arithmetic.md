# Implementation plan: ADR 0235

Status: Withdrawn

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

## Arithmetic NaN assertion inventory

- `rune/crates/rune/src/experimental_core/column/tests.rs`,
  `f64_plan_bitwise_chunks_masks_and_output_reuse`: the cross-driver result
  vector comparison now canonicalizes only arithmetic NaN results before
  comparing, implementing classification-only parity. Returning input 0
  remains bitwise, including signaling/quiet NaN payloads. Validity, fuel,
  valid-row count and all non-NaN result bits stay exact. The test now compares
  automatic SIMD, forced scalar, and the generic oracle.
- The same file, `f64_plan_write_only_null_words_and_cancellation`: cross-driver
  arithmetic NaN results now compare by classification. Automatic dispatch
  exercises the AVX2 loop and its mid-batch/final cancellation; invalid-lane
  zeros and all non-NaN bits stay exact.
- No other existing assertions have been relaxed. Existing finite slot-reuse,
  ownership, copy/store, bitmap and initialized primitive checks stay intact.

New primitive controls exercise all nine source representations and three
operations, boundary lengths, sliced input/output buffers, distinct quiet and
signaling NaNs, signed zero, infinities, subnormals and finite extremes, with
output canaries and input immutability. Token detection and pre-write bounds
checks have independent controls. AVX2 hardware coverage must be required by
the controller; Miri explicitly exercises the portable branch instead.

Build freshness: the first paired capture was rejected before diagnostics
because the candidate log lacked a fresh Rune compilation. The shared
`tools/column/paired_build.py` now exports both hashes at the same source
path and uses separate, newly created equal-length target directories.
Positive compilation records for Rune/rnx/rnx-polars are mandatory. The
portable builder uses the same target isolation and Rune/rnx freshness check.
Controls reject the observed stale-Rune log and reuse of an existing target.
The failed receipt is retained; both roles are rebuilt before guards/capture.

Successful builders remove their role-specific Cargo target only after the
copied binary is hashed, synced and recorded in a PASS build receipt. A
separate cleanup journal records the removed path and logical/allocated file
bytes, then confirms deletion and re-verifies the retained binary. Logs,
archives, copied binaries and receipts remain. Controls check that a wrong
hash prevents deletion and that the retained artifact still verifies.
BUILD-v2 began before this addition; clean its caches only after guards pass,
using the same helper and recording a separate cleanup receipt.


Bounded structural correction after BUILD-v2: the duplicated F64 loop caused
three `publish` instances to become out-of-line. Separately, unchanged
`masked::first_fault` gained a call to `run::input` (the helper's sole source
caller). The reviewed single correction marks both helpers inline(always)
to restore their baseline inlining. No second attempt is authorized if
protected/portable vectors retain row/block growth. The preceding binaries,
identity PASS, instance FAIL and caller disassembly remain in the receipts.

BUILD-v3 stopped before compilation because a scope parser received forced
ANSI-colored Git diff output. Column tooling now explicitly passes
`-c color.ui=false` to Git subprocesses, including the shared source/gate and
build helpers. A forced-color regression control verifies plain diff content;
all column Python unit tests pass. Failed BUILD-v3 remains; BUILD-v4 uses the
same two-attribute production correction, without another structural attempt.


## Reviewed fault-replay identity exception

The user approved a narrow exception for `masked::first_fault` after the
bounded correction. BUILD-v4/IDENTITY-v4 retains its original FAIL: this
cold overflow-replay routine differs in type-dispatch layout. All other
246 protected routines must remain identical under the resolved identity
check. This is not permission for hot-path or forced-scalar row/block growth.

The measured pair is baseline
`1fe8ba598fefd521088b8354d884a81c92c4f9dd` and candidate
`85fc88d5f69b766c23fa751284f8d8628967e65e`, with binary SHA-256 values
`5bd6e58c186a315d1c86e10021eadb4bbe619361f69ab21b770d91da4b3c2b94`
and `70babba1e59d147bc6cfef0e86ae05145682d76badd870e362d18ef55681461c`.
Later test/tooling-only amendments require explicit production-scope proof.

Behavioral acceptance requires
`column::tests::first_fault_replay_preserves_flat_masked_and_generic_errors`:
flat and masked fused execution and generic pair-operation chains must
report the same first fault row, Overflow cause, remaining work, source
location, call chain and formatted error as reference execution, in automatic
and portable modes. Invalid-lane overflow must be ignored. This supplements
the existing core boundary/fault-row and exhaustive fuel-stop controls.
The new control must pass before the identity exception is accepted.

SMALL captures retain their measurement-only receipts. Promotion requires
separate acceptance scoring, reviewed fixed setup pins with +1 rejection
controls, and no new hot-path or forced-scalar slope. A slope stops the
record; no further production correction is authorized.


Final user-authorized correction after portable measurement: add only
`#[inline(always)]` to `f64_plan::Sources::get`. The prior STOP and portable
+169..406 Ir/block evidence remain. Require get, publish and input to be
inlined, no new helpers, resolved identity under the reviewed cold-replay
exception, unchanged SMALL exact, and portable exact or reviewed fixed-only.
Any remaining slope or newly outlined helper withdraws this record; no further
production attempt. Rebuild from fresh per-role targets and check the actual
Valgrind executable before capture. Reuse earlier Miri only with the exact
attribute-only production diff; rerun core controls and the replay test.


## Disposition: withdrawn

The final authorized Sources::get annotation removed its outlined calls but
portable f64_plan::run still gained work per block. The pre-agreed stop rule
therefore withdraws ADR0235, without another attempt or a new exception.
Every non-plan file is restored exactly to published
`1fe8ba598fefd521088b8354d884a81c92c4f9dd`. The signed implementation
`a6df295f4bc0e71a6f58dc3994df85f6eb76ab9b` and its plan are preserved in a
verified local bundle, with hashes of retained measurement receipts. The
withdrawal ships only these paired documents.

The final SMALL unchanged paths are exact. At 64K warm rows the measured
complete-map instruction counts per row were:

| Expression | Layout | Baseline Ir/row | Candidate Ir/row |
|---|---|---:|---:|
| x*2+y | flat | 10.359 | 4.952 |
| x*2+y | alternating | 18.456 | 12.918 |
| (x-y)*2+y | flat | 14.804 | 6.181 |
| (x-y)*2+y | alternating | 22.903 | 14.148 |
| x*y+(x-y) | flat | 15.876 | 6.429 |
| x*y+(x-y) | alternating | 23.989 | 14.410 |

The earlier finite-only scratch probe showed 15–24% wall-time improvement.
That is opportunity sizing, not final candidate timing: no final LARGE or
six-cell timing gate ran after the portable failure.

Portable first and warm captures at 4096 and 65536 rows show these slopes:

| Expression | Flat extra Ir/1024-row block | Alternating extra Ir/block |
|---|---:|---:|
| x*2+y | 263 | 73 |
| (x-y)*2+y | 253 | 63 |
| x*y+(x-y) | 251 | 61 |

Sources::get is now inlined, but the larger split function changes code
emitted for the portable run body itself. These are row-dependent costs,
not fixed preparation overhead. The portable path serves aarch64 and x86
without AVX2 as well as force_scalar, so the regression matters to users.

The earlier first_fault identity exception was explicitly approved and
behavior-tested across flat, masked and generic replay: first row, cause,
remaining fuel, source location and trace match reference execution. It did
not waive portable slopes. The other protected routines remained exact.
No exception was granted for this final failure.

Evidence: FINAL-CORRECTION-v2/RESULT.json records capture completion, not
acceptance; PORTABLE-MEASUREMENT-v3/RESULT.json has raw_vector_status FAIL.
SMALL-MEASUREMENT-v2/VECTORS.json, BUILD-v5/REVIEWED-GUARDS.json and the
prior failed receipts remain in the local evidence directory.

Backlog: consider AVX2 arithmetic in a separate non-generic function with
its own helpers, keeping the portable run body unchanged. This requires a
new reviewed record, not another attempt here. After withdrawal, refresh the
old-core removal inventory read-only before choosing the next implementation.

Withdrawal gates: empty non-plan source diff, final host/integration/generated/
cross chain, and commit checker. Publish the docs-only withdrawal on PASS.
No benchmark follower is needed for this unshipped implementation.
