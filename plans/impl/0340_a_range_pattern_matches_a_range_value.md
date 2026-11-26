# Implementation plan 0340: A range pattern matches a range value

Status: Proposed

Paired ADR: ../adr/0340_a_range_pattern_matches_a_range_value.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. List the
   sites where the language takes a pattern and the lowering each
   one uses. Measure the old core's outcome for the planted forms
   of Controls 2 to 7. Record each outcome and the route taken.
2. Lower the range pattern as Decisions 1 and 2 state, and accept
   the range expression as a root as Decision 3 states. Add the hit
   and miss witnesses for the six types.
3. Add the field-pattern, site and heap-endpoint controls. Report
   before going on if a site or a proof needs a change beyond
   Decision 8.
4. Add the remaining controls: the receiver of ADR 0339, the four
   compile-time rejections and the two planted changes.
5. Replay the public admission corpus against the published base.
   List every changed source as Control 9 states. Send the list for
   review before signing if anything but the six cases changes.
6. Score the file gate and both project screens.
7. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.

## Results

On published a4292836, of the six functions in the shared L1 source,
only range_match needed fallback when compiled alone. A local range
as scrutinee was refused too. Match arms use pattern_match, if let
uses pattern_conditional, and while let uses pattern_loop. Each
calls pattern_root and pattern_tree. Local bindings, function and
closure parameters, and destructuring for bindings use pattern_local
and the same tree walk. Nested patterns are children of that walk.
All these runtime sites now admit the range patterns. Select still
refuses the enclosing expression. The separate constant evaluator
still refuses a range value: the planted const match fell back with
FrontendDiagnostic and returned 2 on both revisions.

The type test uses the same six hash-to-tag mappings as a range type
value. A miss branches before any field read. On a hit the ordinary
field instruction reads just the requested endpoints. The file probes
on both native settings exercised hits, wrong range types and
non-ranges, nested and guarded fields, misses at each binding site,
retained heap endpoints, call order, and the receiver control of
0339. The four invalid patterns kept their previous diagnostics,
positions and frontend fallback. The wildcard-only and plain-binding
matches on a range-expression root also moved to execution, yielding
7 and 2 as before. There were 56 probe sources in automatic and
forced-legacy modes on each native setting. Status, output and
located errors matched the reference.

The replay ran rnx --color=never run --debug-engine --native=MODE
--budget BUDGET SCRIPT on the 3,202 inputs of tests/routing_cli.rs
with their assigned budgets and live clock properties. With native
code off and on, l1-00398 through l1-00403 moved from a Match
refusal to execution, with identical successful results. The count
of working legacy-only inputs fell from 83 to 77 in each mode.
Separately, l1-00073 is the compile-only copy of the same source.
It also moved from a Match refusal to execution. Both runs exited
1 with empty stdout and the same missing-main diagnostic with no
source position. No other input changed route, and none of the
3,202 changed exit, script output, diagnostic or file effects.

The committed range-pattern file tests passed through the installed
candidate in both native modes. The heap control verified aliasing
of the bound endpoint and its range and returned the store to zero
live entries and zero payload bytes. With the type test removed,
an object and a script struct both took the range arm: the test
returned 113 instead of 3. Restoring the old refusal failed the
same test at compile time with Match. The whole Rune library passed
2,192 tests (245 ignored) in the normal profile and 2,222 (246
ignored) in the native profile. A preliminary native launch reused
the old-refusal plant's test executable through a shared Cargo
target path. The passing native suite built the candidate in a
fresh directory used by neither plant. Format, preflight, four Rune
Clippy configurations and the root Clippy comparison passed.

The file gate passed all 32 per-record and all 32 cumulative rows.
Both project screens, empty and Polars/Candle, passed all 64 rows
against newly built images of the published base under the existing
limits. The model, sales, search, generated adapter and two column
fixtures kept their successful output and routes in both modes.
Production sources match measured snapshot 5d7054e8 byte for byte.
The signed tree adds public-entry regression checks and these
Results to that snapshot; the added tests ran against its installed
executable before signing.

The ADR's last constraint says a program without a range pattern keeps
its route. Before signing we agreed to read it by its purpose, that no
program loses or changes behavior. A range-expression root with no range
pattern, and the compile-only copy l1-00073, move from the legacy route
to the new engine with the same exit, output and effects. That move is
within the record. A move to the legacy route, or any change of output,
would still break the constraint, and none was found.

The one complete final ran on 2a145810 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
f25ec5159d5f41f890d9e124b6e76c7d57fcb30d66e7efecc2f577d46682d464.
Entry through cleanup took 3,676.5 seconds against the 5,100 second
limit. The longest chain was the gate, which ran from 3,400 to 3,634
seconds. The Rune suite ended at 2,322 seconds with 1,087.0 seconds in
the contracts. The gate reused the gate inputs published with a4292836,
so the base image was not rebuilt on the clock. All 32 per-record rows
passed and all 32 were exact. All 32 cumulative rows passed. The fetch
stayed offline and took 11.7 seconds. The build peaked at
120,426,352,640 bytes and shared memory at 28,106,752 bytes. The cleanup
cleaned 37 targets and removed the run. The memory controller reported
no events. The run left no new entry in /tmp or /var/tmp.
