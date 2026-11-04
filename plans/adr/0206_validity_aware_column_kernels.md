# ADR 0206: validity-aware column kernels

Status: Accepted

## Context

Column kernels execute pure, verified Rune expressions over borrowed typed
columns. Per-row bitmap traversal and generic value dispatch can dominate the
arithmetic, particularly for nullable data and two-input floating-point work.
Null payloads have no semantic value and must not cause observable failures.

## Decision

Intersect validity word-wise and process bounded physical column blocks. Packed
masks select valid lanes; invalid lanes contribute neither values nor overflow
flags. All-zero words skip arithmetic and all-one words use unmasked primitives.
Use runtime-selected precompiled Rust primitives, with a portable implementation
for every admitted operation. Keep safe typed slice loops where practical and
isolate unavoidable intrinsics behind checked wrappers. General checked integer
multiplication remains valid without SIMD. Floating-point operations preserve
Rune semantics without reassociation or implicit FMA.

## Invariants

- Immutable borrowed inputs remain owned for the synchronous call; no input
  column copy, per-row VM dispatch, or per-row allocation is introduced.
- Null rows consume no scalar-call fuel. Success commits the exact valid-row
  charge; refusal publishes no output and preserves the original allowance.
- Mask overflow flags themselves, including faults from constant expressions.
  Preserve original row and operation order when identifying the first fault.
- At most one speculative attempt precedes one ordinary scalar replay.
- Bound cancellation latency by the declared block size and account actual
  retained memory capacities under existing engine limits.
- Preserve deterministic zero null payloads and initialized packed tail bits.
- Column execution requires no change to interpreter opcode handlers.
- Prefer std/core-only implementations; dependency exceptions require review.

## Consequences

Word operations must handle independent chunk layouts, bit offsets and tails.
Scalar and SIMD implementations need differential controls, with native SIMD
validation separate from Miri's safe-path checks. Acceptance must measure the
whole map as well as isolated arithmetic, so validity and output costs cannot
be hidden by a kernel-only number. Record-specific targets and rollout belong
in the implementation plan. A changed decision requires a superseding ADR.
