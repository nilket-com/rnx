# Implementation plan 0341: A script method's result keeps its proof

Status: Proposed

Paired ADR: ../adr/0341_a_script_methods_result_keeps_its_proof.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. Confirm the
   cause in the proof graph. Measure the old core's outcome for the
   planted forms of Controls 2 to 5. Record each outcome and the
   route taken.
2. Add the edge as Decisions 1 to 3 state, with the witnesses of
   Controls 1 to 3. Report before going on if the edge needs a
   change beyond Decision 6.
3. Add the remaining controls and the two planted changes.
4. Replay the public admission corpus against the published base.
   List every changed source as Control 7 states. Send the list for
   review before signing if anything but the one case changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.

## Results

A method call now depends on its receiver and the return of the script
body selected for that receiver and arity. The proof carries the
selected body's parameter and Context obligations. On the published
base, the linked list ran through the legacy route with a Method
refusal. Changing only the method name from next to step removed the
refusal. A script function returning the same iterator already worked.

Two methods named produce exposed a false cycle in the first graph.
B.produce called A.produce, which returned X directly. Within a
component containing possible method targets the solver now waits for
the receiver, then the selected return. The chained call returns 7 on
the new route. A real recursive or mutually recursive return still
ends the proof. Two unrelated make methods returning X and Y retain
their separate results. An uncertain receiver, a wrong arity and a
partial match keep their previous routes. On a match miss, unit.len()
returns 0 and unit.next() reports the missing method at the call site.

The earlier three-form development patch remains parked. Its constant
evaluator returned a value for a called const fn containing an
unreached break outside a loop. The old compiler rejected the same
function. The published engine already exhibits this difference for
an ordinary const item. Static validation of constants is assigned to
0342 before either new constant form can ship.

Only the after_admitted field changed in the historical
associated_return_unmodeled fixture. Its measured refusal and its
stock result of 7 remain in the fixture. Removing the return edge
sent the linked list back to the legacy route. Selecting an arm for
an uncertain receiver erroneously admitted a call through a parameter
and printed 9 9 instead of 7 9 in the measured build.

The first file screen failed the shared_rows native-on setup T
comparison by adding 85 instructions at both sizes. The version
that distinguished the false cycle added 265. In those images the
loader accounted for 84 and 264 instructions respectively. Their
engine relocation inventories added seven and 22 tracked locations
in receiver_proofs.rs. Neither had execution growth on that row.
The repair uses checked ids for the new graph accesses and drops a
proof on an invalid id. A debug build asserts the same invariant.

The public CLI replay uses the 0340 inventory, budgets and
clock-property checks. Of 3,202 entries in each native mode, the
working legacy-only count falls from 77 to 76. The linked-list test
l1-00357 now exits 0 on execution. Its compile-only copy l1-00063
also moves from Method to execution and still exits 1 with the same
missing-main diagnostic and no source position. Every other entry
keeps its route. The comparison covers status, script output,
diagnostics and file effects.

On the checked source the file gate passed all 32 per-record and all
32 cumulative rows. The engine has eight fewer relative relocations
than the published base. The shared_rows setup T fell from 871,600
to 871,505 at both sizes. The installed file runner passed the 41
semantic probes in both native modes, comparing automatic and forced
legacy outcomes and recording the route separately. The committed
routing test passed on that runner too. In separate build state the
Rune library passed 2,192 tests in the normal profile (245 ignored)
and 2,222 in the native profile (246 ignored). Format, the four Rune
Clippy configurations, the routing-test lint check and preflight
passed.

The replay invoked rnx --color=never run --debug-engine with the
case's --budget and --native=off or --native=on on the adapted
script.rn. Both project comparisons, empty and Polars/Candle,
passed all 64 rows each against builds of 8f4d4c09. The file and
project measurements used source snapshot a35e8bb6. This
implementation commit adds only these Results to that tree. The
engine version embeds its build revision.

The one complete final ran on af9ac4e7 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
dd70d4e10474a9ec1b29efb262b57373299d4df8095aad85797b9a50a9d87a15.
Entry through cleanup took 3,742.8 seconds against the 5,100 second
limit. The longest chain was the gate, which ran from 3,460 to 3,697
seconds. The Rune suite ended at 2,343 seconds with 1,105.4 seconds in
the contracts. The gate reused the gate inputs published with 8f4d4c09,
so the base image was not rebuilt on the clock. All 32 per-record rows
passed. Twenty-three were exact and nine were below the base. Seven
native-on T rows fell by 8,531 to 39,816 instructions at size 1, at most
0.2026 percent: fib, loops, numeric_calls, awfy_sieve, awfy_permute,
vec_iter_i64 and vec_iter_bool. In each the setup phase fell. Execution
moved only on awfy_permute, whose row fell by 37,560 at size 1 and
35,806 at size 5; its execution growth rose by 1,754 against an
allowance of 198,073. The awfy_permute native-off T row fell by 21 and
its native-on W row by 120. All 32 cumulative rows passed. The fetch
stayed offline and took 11.5 seconds. The build peaked at
120,420,446,208 bytes and shared memory at 5,668,864 bytes. The cleanup
cleaned 37 targets and removed the run. The memory controller reported
no events. The run left no new entry in /tmp or /var/tmp.
