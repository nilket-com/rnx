# Implementation plan: ADR 0229

Start from `c7d08d9b`. Read the masked path first: `filled`, `run_masked`,
`masked_float_constant`/`masked_float_pair`, and how all-valid, mixed and
all-null blocks are dispatched. Then design the smallest sibling route.

1. Primitives: add write-only masked F64 writers on `NumericOutput<f64>`.
   They cover all-valid blocks (existing calculation), mixed blocks
   (existing masked calculation, invalid lanes +0.0), all-null blocks
   (explicit +0.0) and masked tails. They are separate symbols; any shared
   source goes through a macro. Existing initialized symbols stay
   byte-identical.
2. Allocation and dispatch: for the masked fast F64 shapes only, reserve a
   length-zero `NumericOutput` instead of `filled`, and run the write-only
   route. The `canceled` type is the plain path's `never_canceled`, so no new
   monomorphs are created. The generic executor and `masked.rs` operation
   helpers are not edited; if the route needs them, stop and report.
3. Tests, all committed: bitwise parity against the initialized path over the
   0226 matrix plus all-null, all-valid, mixed and offset blocks at lengths
   0/1/63/64/65/1023/1024/1025; NaN payloads; invalid rows +0.0; fuel and
   one-short fuel; cancellation before and mid-run; memory refusal at the
   reservation boundary.

Gates: run the exact local clippy first. Then:
- the core column and primitive tests;
- Miri for the new writers;
- the duplicate-instance guard;
- SMALL exact vectors against `c7d08d9b`, plus changed-path Ir per row at
  4K/64K/10M;
- the 100M fault capture;
- 0224-harness timing, 5 process samples, for the four nullable const/add
  cells;
- the host, integration, generated and cross gates, and the commit checker.
Publish only on all-PASS. The bench follower carries the 0228 context
correction.
