# Implementation plan: ADR 0220

Read the archived 0218 and 0219 attempts first; the `boolean_pair` word logic
and its tests can be reused. Add classification to the existing preparation
pass, storing the result as plan data. Route to the new driver in `invoke` (or
the nearest per-call point) before the existing traversal begins. The new
module owns traversal over chunks and validity runs, fuel and cancellation
checks, and packed scratch for intermediates. Reuse the bitmap and lease
helpers. Do not call `operands`, `operation` or the generic block loops.

Adapt the archived exact-vector comparison tooling (unknown objects by
totals) to compare the candidate against published `e05ce43c` for all
non-qualifying shapes. Include the SIMD shape and hot-loop controls. Run
targeted Miri on the new module's safe boundaries, functional and native
controls, then the small paired diagnostic for target review, then the
10M/100M matrix, host/integration/generated/cross gates and the commit
checker. Update `BACKLOG.md`: drop the entries completed by 0213–0215, record
0221 (F64 typed blocks, which must likewise avoid new callers into the shared
executor), and keep the remaining follow-ups.
