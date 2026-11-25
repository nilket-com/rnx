# Implementation plan 0339: A match that can miss proves no receiver

Status: Proposed

Paired ADR: ../adr/0339_a_match_that_can_miss_proves_no_receiver.md.

1. Boundary: the published base. Reproduce the two observations of
   the ADR's Problem on both engines in both native modes, with a
   counted side effect in the method body. Probe the other forms
   whose value can be the implicit unit, as Decision 8 names them.
   Record each outcome and the route taken.
2. Add the predicate of Decision 2 where both proofs and the
   `Result` rule can use it. Repair the lowering as Decision 3
   states. Add its hit and miss witnesses.
3. Repair the proof graph as Decision 4 states. Add its witnesses
   through a local, a parameter, a script return and a nested
   match. Report before going on if the possible unit can't be
   carried through a join without a change beyond Decision 9.
4. Add the remaining controls: the positives, the refusal with its
   position and no earlier effect, and the two planted proofs.
5. Replay the retained proofs and the public admission corpus
   against the published base. List every changed source as Control
   7 states. Send the list for review before signing if any working
   public behavior is lost.
6. Score the file gate and both project screens.
7. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.

## Results

On published 9f993088 a miss ran the script method on unit through a
local, a parameter and a function return. The guarded and nested
misses did the same. The partial match with a returning arm and a
trailing semicolon also admitted the wrong receiver through a
parameter or return. On the candidate the local m dispatches on the
actual value. A hit enters the body once. A miss reports the missing
method at the call and skips the body. For the unproved capacity
call the frontend refuses at the call, before execution, for both
inputs. File fallback then runs the program once. Wildcard and
plain-binding matches retain their direct call. All 168 observations
of the candidate (14 forms, three call sites, both native modes,
automatic and forced legacy) matched the reference on exit, script
output and diagnostics. Compared with the base there were 22
corrected observations and eight further changes of route on hits.

For each of the six other implicit-unit forms the base already gave
the missing-method error with one entry effect and no method-body
effect. The local m used execution and the parameter and return
capacity calls fell back. The candidate gave the same results in
both native modes.

We replayed the cases from tests/routing_cli.rs through both
installed executables with --debug-engine, once per native mode.
Each mode covered 3,202 inputs, including the census and L1,
the three plain-file demos, benchmark kernels and routing controls.
The count of successful cases needing legacy was 83 on the base and
83 on the candidate in each mode. No corpus source changed exit,
script output, diagnostic, route or file effects. The two live clock
cases used their existing clock properties. The corrected observations
above come from the planted programs, outside that public corpus.

The replay command was rnx --color=never run --debug-engine
--native=MODE --budget BUDGET SCRIPT for each adapted case, with its
published budget and the same sandbox path for both binaries.
Model, search, sales, generated_e2e and the two column CLI fixtures
also matched on both paired project executables in both modes,
including route, exit status and both streams. Search used the six
model files verified by the demo's pinned checksums.

Restoring each published proof separately broke its own regression
test and left the other test passing. The constructor, static-method
and dynamic-method contracts passed. The full Rune library passed
2,191 tests (245 ignored) in the normal profile and 2,221 (246 ignored)
in the native feature profile. The first ad hoc constructor launch
omitted C15_OUT and stopped before its first case. With the required
output directory supplied the contract passed. Format, preflight,
the four Rune Clippy configurations and the root Clippy comparison
passed. A separate blanket -D warnings invocation on the root library
reported existing diagnostics. The repository's root Clippy stage
reported zero added diagnostics.

Against 9f993088 the file screen passed all 32 per-record and all 32
cumulative rows. The empty project and the Polars/Candle project
each passed all 64 rows against freshly built project images of the
base. Both used the existing limits. These screens measured snapshot
b610b660; only this Results section was added after measurement.

The one complete final ran on bfb84764 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
25d5d506cf8f9c3afc83159881e49909285c6f6835ef8f0baaaf727a136476cf.
Entry through cleanup took 3,937.0 seconds against the 5,100 second
limit. The longest chain was the Rune suite, which ended at 2,859
seconds with 1,559.7 seconds in the contracts. The gate then ran from
3,656 to 3,891 seconds. This was the first final to reuse published gate
inputs. The lookup verified the published anchor for 9f993088, so the
base image was not rebuilt on the clock. All 32 per-record rows passed
and 25 were exact. The other seven, all on installed native-on setup T,
grew by the same count at both measured levels, between 8,531 and
116,046 Ir and at most 0.2031 percent, inside the existing limits. All
32 cumulative rows passed. The fetch stayed offline and took 11.6
seconds. The build peaked at 120,425,025,536 bytes and shared memory at
18,436,096 bytes. The cleanup cleaned 37 targets and removed the run.
The memory controller reported no events. The run left no new entry in
/tmp or /var/tmp.
