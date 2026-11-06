# ADR 0219: Source-determined generic column executor shape

Status: Proposed

ADR 0218 was withdrawn. Its Boolean gains held in every attempt, but each of
four boundaries changed how the compiler inlined the generic column executor,
and unrelated paths regressed:
- a new operation arm;
- an outlined fallback;
- a prepared step arm;
- a second const-generic instance selected once per call.

The regressed paths were mixed-null F64 chains (including +7.2 Ir/row in the final attempt) and I64
multiply3. The cause was never the new work itself. The inlining of
`operation`, `add_pair`, `destination`, `Sources::get`, `output_value` and
`zero_i64` was decided by heuristics that react to the executor's overall
composition. ADR 0216 fixed the same fragility for the protected SIMD
kernels. This record does it for the generic executor, so later records can
add work without codegen roulette.

Make the inline boundaries of the generic block path explicit in source. That
path runs from the block loops through `execute`, `execute_inner`,
`operation`, the scalar and integer loops, and the operand and destination
helpers. Prefer freezing the decisions the compiler makes at `e05ce43c`. A
different boundary is acceptable only where it measures no worse. No
semantic, data layout, fuel, fault or observability change.

Prove isolation with a two-composition control. A diagnostic-only build
configuration (not a Cargo feature and not in production builds) adds a
realistic perturbation: the withdrawn ADR 0218 Boolean step arm and a second
const-generic executor instance. The function vectors of every protected and
generic path must be identical between the normal and perturbed builds, at
4096 and 65536 rows, first and warm. The control is committed and reusable,
so ADR 0220 can bring back the Boolean work against it.

Acceptance against published `e05ce43c`, under the existing program and
ceiling rules:
- generic paths (F64 and Bool chains, multiply3, mixed-mask I64) are no worse;
- specialized paths are exact, apart from reviewed fixed per-call constants
  pinned by exact vector with a +1 control;
- whole-map ceilings hold at 10M/100M.

This is a codegen-shape record. Run the functional suites and native controls.
Miri is not required unless unsafe code or semantics change.
