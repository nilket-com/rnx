# Implementation plan: ADR 0233

Status: Accepted

From `845572cb`, read the predicate driver's I64 path (the bounded 8-lane form
selected by `const FLOAT`) and the `Engine` dispatch. Add AVX2 primitive
kernels for the six I64 comparisons that write packed `u64` words, with safe
wrappers that assert lengths and fall back to the existing code when AVX2 is
absent or `force_scalar` is set. Route only the I64 comparison word loop
through them. The tail and validity handling stay as they are, unless a
measured reason says otherwise; report that before changing them.

Tests (committed): bitwise and Path parity against the scalar path for all
six operations, the length set, signed extremes (`i64::MIN`, `i64::MAX`,
-1, 0), offsets and multi-chunk inputs, and nulls.

Gates:
1. local clippy, then the core and primitive tests, then the hardware
   parity tests;
2. Miri for the wrappers and the portable path;
3. the instance guard and resolved identity;
4. SMALL exact against `845572cb`, plus the changed-path total Ir at
   64K/10M;
5. 0224-harness timing for `i64eq` in four layouts;
6. the final chain (gate_policy finalizer, cross, commit checker).
Publish only on all-PASS; the bench follower reports the four cells, with
Arrow/Polars cited from ADR 0224 as cross-day context.

Keep driver-level Path::Scalar. Test primitive dispatch directly. Force-scalar
bitmaps and Path are exact; preserve the scalar comparison symbol under
resolved identity. Whole-map force-scalar vectors may carry only explicitly
reviewed fixed setup costs; no per-row or per-block growth. Measure raw vectors
first and stop on slope.
