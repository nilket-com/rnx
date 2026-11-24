# Implementation plan 0334: `dbg!` runs on the new engine

Status: Implemented

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

## Results

All 14 source and test files carried over byte for byte from d5a2ded0.
The built-in macro and default variadic handler are certified by
identity. The capture handler and a foreign replacement still refuse
before writing.

In each native mode, 28 of the 31 stopped cases ran with fallback
disabled and matched published 6da26e43 on exit status, stdout,
stderr and file effects. The remaining three reached
`String::with_capacity(42)` at bytes 57..82 in l1-00177 and 66..91
in l1-02906 and l1-script-00177. The independent const-block control
l1-00291 still refused at 342..361.

C2 passed its 367 rows and supplemental controls. The old core's
cycle diagnostic included an address. Execution printed `[<cycle>]`
for the cycle and `<depth-limit>` at the depth bound. All 22 macro
certification tests and all three registry inventory tests passed.

Formatting and Rune Clippy passed. The file gate passed all 32
per-record comparisons against 6da26e43 and all 32 cumulative
comparisons. The empty project and Candle/Polars project each passed
64 image-to-image comparisons against builds from 6da26e43. Fixture
and stdout hashes matched across roles. Each screen evaluated level
and growth at sizes 1 and 5 for W and T in both native modes under
the existing per-record band.

The first replay stopped while the harness decoded its UTF-8 corpus
index under the C locale. The first paired-project lock stopped on a
missing registry entry in the private offline Cargo home. Explicit
UTF-8 decoding and fetching the locked dependencies into that home
allowed the measurements to finish on the same source snapshot.

The complete final on 0a5a428a passed all 107 stages. Preparation
promoted the checkpoint from the 0333 final and brought it to the
6da26e43 baseline in 2,030.0 seconds, outside the clock. The timed
final took 3,591.3 seconds of the 3,600, and the total wait was
5,621.4 seconds. The Rune suite started at 1,769 seconds and ran for
1,071. The fixed-route gate started at 2,907 and ran for 642. In the
0333 final the same stages started at 1,565 and 2,428 and ran for
851 and 670. The early disk check saw 212,104,065,024 bytes free
against 127,009,565,346 needed. The check before the clock saw
210,070,896,640 against 124,782,786,583. All 32 per-record
comparisons passed, 13 of them exactly, and the largest movements
were -0.202% and +0.132%. All 32 cumulative comparisons passed.
There were no memory.high, memory.max or OOM events, and the host
was in the performance profile. The developer cache snapshots matched
before and after the run, and the final added no temporary entries.
