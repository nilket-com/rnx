# Implementation plan 0334: `dbg!` runs on the new engine

Status: Proposed

Paired ADR: ../adr/0334_dbg_runs_on_the_new_engine.md.

1. Boundary: published 6da26e43 and the fallback-deletion gate evidence.
   Confirm the macro and handler sites and the C2 observations. Record
   any deviation before editing. Verify the default raw handler and the
   separate capture handler. The step 1 probe found address and
   code-offset text that the old core itself doesn't reproduce; Decision
   3 governs it.
2. Certify the built-in macro and the installed handler by identity.
3. Lower the call with the old core's evaluation order, stdout locking
   and unit result. Format with the old core's text, and the documented
   identity text only where Decision 3 allows it.
4. Add the parity table, the order and effect controls, and the
   handler refusal controls. Update the C2 dispositions and affected
   inventory witnesses. Replay the 31 dbg rows (11 originating examples)
   from the published comparison in both native modes and distinguish
   completions from subsequent refusals. Leave the const-block row 00291
   as a control.
5. Score the file gate and both project screens before signing. Check
   every delta against its band.
6. fmt, Clippy, preflight and the targeted checks under the final's real
   recipe. Sign. Source review, one complete final, Results.
