# Implementation plan 0323: The final owns its build caches

Status: Implemented

Paired ADR: ../adr/0323_the_final_owns_its_build_caches.md.

1. Boundary: inventory every Cargo writer in the complete catalogue,
   nested writers included, against published 555cdb1a. Record each
   writer's lane key, or its transient owner, and any deviation before
   editing.
2. Add owned, locked lanes, one per existing sharing group, and route
   every writer to one. Keep the fixed-route environment pinned.
3. Add per-recipe marks from Cargo's JSON stream, the drained sweep,
   whole-lane retirement and reclamation before admission. Keep the
   existing output parsers.
4. Move record scratch for `polars-inventories` and `polars-suite` to
   stage lifetime. Add a lane only where timing justifies it.
5. Run the incremental A/B and set the per-lane choice from it.
6. Add the churn and admission controls. fmt, Clippy, the full
   final-controls command and the targeted checks. Sign.
7. Claude and Codex agree on the one-time migration list and delete it.
   Source review, one complete final, Results with the footprint report.

## Build measurements

The incremental comparison used the published 0322 sources, 11 Cargo
jobs, the same feature sets and four builds per treatment. All eight
unchanged repeats rebuilt zero units. Polars default kept the declared
incremental setting. Its adapter edit took 12.55 seconds and its Rune
edit took 37.17 seconds, against 82.53 and 103.99 with incremental
disabled. Candle test-support also kept the declared setting. Its
adapter edit took 22.91 seconds against 39.31, for 1.67 GB of extra
allocated space after the Rune edit. Candle default saved 4 seconds on
the adapter edit and 0.41 on the Rune edit. Polars test-support gained
no consistent time for its additional 9 GB. Those two recipes and the
remaining reusable builds disable incremental compilation.

A second trial built both selected recipes in each adapter's shared
target. The second cold Polars recipe reused 406 of its 470 units. The
second Candle recipe reused 305 of 330. Both unchanged repeats rebuilt
zero units. The table gives allocated decimal GB after the sweep.

| Sharing group | Cold | Unchanged | Adapter edit | Rune edit | Non-executable regular files after Rune edit |
|---|---:|---:|---:|---:|---:|
| Polars debug | 50.80 | 50.80 | 52.19 | 52.24 | 8.08 |
| Candle debug | 14.64 | 14.64 | 14.98 | 15.04 | 3.57 |

Cargo reused the same output names across these edits. The measured
sweeps reclaimed zero bytes. The feature-retirement control exercises
deletion separately and verifies a fresh repeat of the surviving
configuration. The footprint measurements sample build boundaries.
Transient compiler and linker peaks were unmeasured.

## Results

The final routes 103 recipe identities to 36 existing target sharing
groups and a separate bootstrap lane. The fresh-build witnesses kept
their own transient targets, and all four released them after passed
and failed finals.

Before the first final, Claude and Codex agreed on 120 Cargo build
directories in the checkout and deleted them. The old controller target
was kept intact under another name. Free space rose from 126.14 to
347.92 GB.

The first complete final started with empty lanes. All 107 stages
passed, but the run took 68.1 minutes against the 60-minute budget, so
no marks were committed. Cold compilation accounted for the difference.
For example, clippy took 267 seconds instead of 6, and the fixed-route
stage took as long as before. A rerun of the same candidate was then
refused at admission. It needed 312 GB, because the full cold footprint
was charged on top of about 110 GB (103 GiB) already allocated. The fix charges a lane
without committed marks only for its cold footprint minus its allocated
bytes, with a 1 GiB floor. Against the refused state, that needs 205 GB.

The complete final on 725f5d28 passed all 107 stages, with 42.9 minutes
of accepted run and 43.4 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile. Its 37 lanes held 112.15 GB allocated,
of which 57.48 GB were marked test executables. The largest were 52.56
GB for Polars, 18.07 GB and 15.53 GB. This first commit swept 0.65 GB.

The measured edits reused Cargo's output names. Configuration turnover
can leave superseded outputs; the earlier inventory counted 77
superseded test executables. A cold start still exceeds the 60-minute
budget. A measured cold budget is a follow-up.
