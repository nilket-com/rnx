# ADR 0235: AVX2 arithmetic in the F64 driver

Status: Withdrawn

Withdrawn after the final authorized correction retained portable per-block
instruction growth. No production or tooling changes ship. The following
proposal and exception history describe the experiment, not shipped behavior.
See the paired implementation record for results and disposition.

The read-only probe against published 1fe8ba598fefd521088b8354d884a81c92c4f9dd
found that separate AVX2 arithmetic primitives reduced warm 100M-row times by
15–24% across chain, subchain and mixchain on flat and alternating-null inputs.
Six fresh processes used all six three-build order permutations. Five cells
had separated observed process ranges; flat chain ranges overlapped. These
finite-only scratch measurements size the opportunity, not acceptance.
The simple portable rewrite increased instruction counts and is not selected.

Decision: runtime-dispatched AVX2 Add/Sub/Mul for the F64 driver's arithmetic
lanes only. Select the existing Engine once per map call, respecting
force_scalar. Use separate non-generic primitive entry points with safe
length-checked wrappers; unsafe code remains confined to the primitives
module. Resolve Floats, Words and Constant representations outside lane loops.
Do not add generic driver instances or new callers to protected primitives.
The portable path retains its existing arithmetic implementation. Keep the
flat, masked write-only, predicate, Boolean and generic drivers unchanged.
Keep BLOCK=1024, scratch allocation, result-array publication, validity,
null zeroing, cancellation, fuel and output ownership unchanged. Final-copy
removal is a separate record. No dependencies are added.

For column F64 Add/Sub/Mul, preserve expression order, validity and exact
non-NaN result bits, including signed zero, infinities and subnormals. No
FMA, reassociation or fast-math. Arithmetic NaN results must classify as NaN;
their sign and payload are unspecified across paths, representations and
architectures. Copying/storing an existing value remains bit-preserving.

Rationale: the published baseline's direct-routine audit captured 2,340 cases.
For driver slice/slice Add/Mul at length1024, lanes0–1007 select the right
quiet-NaN payload and lanes1008–1023 select the left. Flat and masked Mul
also disagree. There is no stable baseline operand-priority contract to
preserve. Single quiet-NaN inputs were preserved in the audit, but the
explicit arithmetic contract does not promise payload preservation. Signaling
NaNs and invalid arithmetic require new controls; the quiet-NaN audit is not
evidence for them. This semantic decision was settled by Claude+Codex under
the user's delegated authority before implementation.

Acceptance against published 1fe8ba598fefd521088b8354d884a81c92c4f9dd:
- All three operations and operand representations preserve non-NaN bits;
  NaN results pass classification controls, including signaling NaNs and
  invalid operations. Copy/store controls remain bitwise. Validity, Path,
  cancellation, fuel, tails and reuse behavior remain exact.
- Hardware parity covers AVX2 and forced scalar. Miri covers safe wrappers,
  portable execution and ownership. Assembly shows packed AVX2 arithmetic
  and no FMA; record scalar tails and dispatch placement.
- Instance guard and resolved identity protect unchanged routines. Unchanged
  routes have exact program vectors, except explicitly reviewed fixed
  per-call pins. Existing baseline exceptions do not authorize new growth.
  Force-scalar vectors are exact or reviewed fixed-only, with no slope.
- Changed F64-driver total Ir is lower at both64K and10M in each of the six
  probe cells: x*2+y, (x-y)*2+y, x*y+(x-y), each flat and alternating-null.
- At100M, warm median improves at least5% in EACH of those six cells. No
  separated slowdown at10M or100M in any measured cell. Use six fresh
  processes with exactly three AB and three BA orders per cell. Report
  first calls, per-process warm medians and observed ranges separately.
  Ranges are descriptive, not confidence intervals. No retry-to-pass.
- Core, scoped Miri, host/integration/generated/cross, gate-policy finalizer
  and commit checker pass before publication.

Stop on unexpected instance/identity drift or unchanged-path per-row or
per-block growth. Attribute and bring a decision before another structural
attempt; a rule exception needs the user. Do not weaken targets after results.


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
