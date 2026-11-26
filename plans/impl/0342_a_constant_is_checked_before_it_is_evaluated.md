# Implementation plan 0342: A constant is checked before it is evaluated

Status: Proposed

Paired ADR: ../adr/0342_a_constant_is_checked_before_it_is_evaluated.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem with the automatic route and with the legacy
   engine forced, in both native modes. Inventory the checks the
   old core's assembly makes for a constant's code. For each, say
   whether HIR lowering, the new engine's evaluation or its
   ordinary lowering already makes it, or why it isn't a check of
   the source. Measure which code the old core visits, the order
   in which it reports several errors, and its outcome for
   constants that are never used.
2. Add the pass as Decisions 1 to 5 state and call it from the new
   engine's constant path. Add the witnesses of Controls 1 to 4.
3. Add the selection, order, inventory and function-body controls
   and both planted changes. Decision 6 applies throughout: decide
   its two listed differences yourself and record them.
4. Replay the public admission corpus against the published base.
   Send the list for review before signing if any source changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.

## Results

The original example now reports `Break outside of loop` at the
`break`, before evaluating the constant. The query starts the check
when it has a lowered constant expression or block. Each called const
fn is checked once per root, including callees discovered in a branch
whose value is false. An unused const item therefore gets checked, as
before, while an unreferenced const fn does not. The walk uses fallible
worklists for expressions, patterns and callees. It adds no depth or
evaluation budget.

The boundary inventory found nine source diagnostics. HIR already
reports unresolved names and labels. The existing routes also handle
closures, references, statics and a yield in a const fn. A yield in a
dead branch of a const item stays accepted. A call to an ordinary
function or a wrong-arity const call still depends on whether
evaluation reaches it. The legacy assembler's address, allocation and
opcode-construction failures are outside the source check. Two existing
evaluator tests now expect the structured diagnostic for a reached
float literal pattern or valued break from a for loop. The public
diagnostic and its span are preserved.

A structural exit in a conditional's test skips that arm. The pass
still checks the fallback, before any surviving arms. For example, in
`if {return 7;} {1=2;} else {3+=4;}` the compound assignment is the
first error. Replacing it with `3;` lets the constant return 7. A
`while` checks its body even when the condition returns. An error in a
caller takes precedence over its queued callees.

Decision 6 applies to a returning match guard. Consider
`const fn f(x){match x {0 if {return 7;}=>5, _=>9}}`.
Both engines give 7 for `f(0)`. For `f(1)` the new engine gives 9.
The old compiler pairs the wildcard's label with the earlier body and
gives 5. With `_=>{1=2;9}` the old core still accepts the program.
The pass checks the preceding arm bodies when a guard always leaves,
and stops at that guard. This preserves the working program. The
converse case is `0 if {return 7;}=>{1=2;5}, _=>9`. The old compiler
diagnoses the assignment in the shifted body. The pass omits that
body under Decision 6(a). The execution evaluator keeps the ordinary
match semantics.

Another Decision 6(a) witness is the body of this const fn.

    const fn f() { for y in [1] { continue; 1=2; } 7 }

Execution prints 7 for `f()`. The old compiler rejects the assignment
after `continue`. The same difference occurs with
`while false { continue; 1=2; }` and with a one-iteration `while` that
increments its counter before continuing. With
`while true { break; 1=2; } 7` both engines print 7. All four outcomes
hold with native code on and off.

An initial file screen failed the native-on shared_rows setup row at
both sizes by 72 instructions against an allowance of 11. Execution
and growth were unchanged. The image gained six relative relocations.
Each referenced a source location for a panicking HIR or slice index
in the new pass. Checked HIR lookups and slice iteration removed
those sites. The performance bands stayed as published.

The installed runner replayed the 142 boundary sources in fresh
processes, automatic and forced legacy, native on and off. It also
checked 81 combinations of error, location and dead-code shape on
those four routes. The automatic route surfaced the same first
diagnostic text and position as forced legacy in all 81. The
committed match-guard routing test passed. Removing the validation
pass broke the dead-forms diagnostic test. Ignoring structural
exits broke the accepted-constants test.

The public admission replay ran
`rnx --color=never run --debug-engine --budget N script.rn`
with each case's budget and either `--native=off` or `--native=on`.
All 3,202 cases kept the same route, status, output, diagnostic and
file effects in each native mode. The number
that succeeded on the legacy route was 76 in each mode on both the
published base and the candidate.

The file gate passed 32 per-record and 32 cumulative rows.
Shared_rows native-on setup T was 871,505 at both sizes on both
images. The empty project and the Polars/Candle project each matched
the published image across all 64 scored rows. The screens compared
published 4edbb2a4 with source snapshot c82301e8. Candidate 9005dfa5
added the initial Results to that tree. Format, four Rune
Clippy configurations, preflight and the whole native-profile Rune
library test binary passed (2,225 passed, 246 ignored).

FINAL-v1 failed on 9005dfa5 in final-controls. Its RESULT.json has
SHA256
8851cc2b570b040f64cfdf3b621034d96f7a06078fffdb8b9755c72a1d4d7d31.
The neighbor stage returned 0 when the test expected a source-change
failure. Its two sleeps allowed the writer to run outside the
neighbor's source checks. By agreement this record carries the
test repair. Both leaves now open an ignored FIFO after their source
snapshots. The writer changes the tracked file before sending a byte,
and the neighbor reads that byte before exiting. Each leaf must exit
successfully and each supervisor must report a source change. A
60-second supervisor timeout bounds a broken handoff.

Two neighboring tests also depended on timing. The Git index test now
sets an old modification time explicitly. The timeout-receipt test
no longer assumes two Python processes can start within its deadline.
The guardian integration test checks that the child and grandchild
are alive before arming the deadline and both are gone afterward.

The full child and guardian test binaries passed (15 and 9 tests).
With eight concurrent test processes sharing two CPUs, the repaired
neighbor test passed 64 times. The index, timeout-receipt and guardian
drain controls each passed 16 times under the same contention. Format,
final-tool Clippy and preflight passed. Production source is identical
to c82301e8, so the corpus, library and performance results above also
apply to the successor.

The first successor, 39f57e1e, was withdrawn before its final. The
renamed child test left row 79 of CONTROLS.txt pointing to a missing
test. The row now points to the guardian test that verifies both
processes are gone after a timeout.

The full final-controls command discovered 319 tests across 38 test
binaries. All 317 enabled tests passed, and two were ignored. The
control-table check and its guardian target both passed. The first
local run had inherited a custom Cargo target directory and failed
the scheduler's path check. The full run used the stage's default
target layout. Format, final-tool Clippy and preflight passed.

The second complete final ran on 8568d018 as FINAL-v2 and passed all 107
stages. RESULT.json has sha256
3d57656cb51d515f720ac7240c8af429d04efde56841818a9aa045d10cc8f3f2.
Entry through cleanup took 3,657.5 seconds against the 5,100 second
limit. The longest chain was the gate, which ran from 3,354 to 3,621
seconds. The final controls, which failed in FINAL-v1, ran from 3,115 to
3,217 seconds and passed. The Rune suite ended at 2,342 seconds with
980.3 seconds in the contracts. The gate reused the gate inputs
published with 4edbb2a4, so the base image was not rebuilt on the clock.
All 32 per-record rows passed and all 32 were exact. All 32 cumulative
rows passed. The fetch stayed offline and took 11.6 seconds. The build
peaked at 120,428,089,344 bytes and shared memory at 3,821,568 bytes.
The cleanup cleaned 37 targets and removed the run. The memory
controller reported no events. The run left no new entry in /tmp or
/var/tmp.
