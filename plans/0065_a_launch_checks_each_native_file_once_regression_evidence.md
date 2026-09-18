# 0065 gate 6: final launch slopes and regression

Status: passes on Linux, ready for review. Gate 5 is accepted with its original
1.016205 ms first-adapter result retained as a qualification. The strict threshold
is unchanged. This fresh matrix passes all six first-adapter comparisons; it does
not replace the accepted earlier journal or its failed gate entry.

## Source and measurement

Product source is published rnx `4855dbd`, byte-identical to accepted F5.
The bench baseline is `96a63ca`; final drivers and results are committed at
rnx-bench `a0dd0bf`. No production change is made in gate 6.
The final ordinary tool hash is recorded in `conditions.json` and matches F5.
All paths below are relative to `results/inventory-final-0065` in rnx-bench;
drivers and rerun requirements are in `probes/inventory-final/README.md`.

The headline repeats the unchanged 75-cell matrix, two repeats and 30 samples per
cell: 4,500 samples. Topology adds 108 cells and 6,480 samples; full-hash attachment
adds 120. All 11,100 timed samples are retained with validated outputs, one pinned
CPU (0), one Polars thread, two warmups and the same fixed-seed interleaving.
PTY first prompt uses xterm-256color at 120 columns by 30 rows. There were no
concurrent fixture builds during timing. Medians below are single-host observations.
Project cost means project launch minus its own matched direct artifact.

## Headline and the six first-adapter cells

These compact rows are medians across the six mode/repeat overhead medians.
`headline/summary.json` retains every direct, project, difference, increment and
fitted slope separately; `headline/gate.json` passes without changing the gate.

| Adapters | SHA-256 baseline | BLAKE3 format-only | Final reuse |
|---|---:|---:|---:|
| 0 | 17.638 ms | 12.252 ms | 12.318 ms |
| 1 | 22.539 ms | 16.343 ms | 13.166 ms |
| 2 | 26.663 ms | 20.112 ms | 13.419 ms |
| 3 | 30.735 ms | 23.854 ms | 13.702 ms |

Zero adapters still fingerprints the runtime: 443 files / 6,988,177 bytes.
That tree includes all three adapters at every count. Polars contributes 23 files /
984,808 bytes; PostgreSQL 21 / 517,330; the complete renamed copy 21 / 517,312.
The runtime floor falls by more than 5 ms in each comparison against baseline.
No shrinking third-adapter stub or removal of its bytes from the floor is used.

| Mode | Repeat | Original gate-5 first increment | Gate-6 first increment |
|---|---:|---:|---:|
| run | 1 | 0.775244 ms | 0.831128 ms |
| run | 2 | 0.802266 ms | 0.773964 ms |
| eval | 1 | 1.016205 ms | 0.842383 ms |
| eval | 2 | 0.832153 ms | 0.882050 ms |
| session | 1 | 0.868588 ms | 0.903061 ms |
| session | 2 | 0.987810 ms | 0.876945 ms |

The accepted eval miss stays visible above. Its original gate file, summary and
all samples are checked byte-for-byte against bench `96a63ca`. This closing run
was requested by review, not substituted for a failed earlier sample block.

## Topology and paths

Each slope below fits the four overhead medians at counts 0–3, then reports the
median across modes/repeats. Topology is explicit beside the launch cost.

| Shape | 0 adapters | 1 | 2 | 3 | Fitted ms/adapter |
|---|---:|---:|---:|---:|---:|
| eligible | 12.289 | 13.089 | 13.441 | 13.680 | 0.452 |
| external | 12.314 | 16.422 | 20.154 | 23.786 | 3.830 |
| git-editor | 12.351 | 16.674 | 20.488 | 24.290 | 3.967 |
| nested-repository | 12.274 | 16.374 | 19.687 | 23.065 | 3.573 |

Eligible roots use three tool-issued Git calls and 443 physical content reads
at every count. Independent paths use 3/6/9/12 calls and 443/466/487/508 reads.
In the eligible fixture, three tool-issued calls mean four Git processes because
Git itself starts a child; the counters count tool calls, not all processes.
`topology-counters.json` contains all 18 controls. The separate six-case
`roster/roster.json` keeps a 14,000-entry ignored target eligible, including counts
0–3, and proves the GIT_EDITOR fallback at one and three adapters.

A shell exporting **GIT_EDITOR**, or any other GIT_* variable, gets the independent
slope. External roots and nested repositories also retain independent inventory.
External adapters keep their full source trees with only their Cargo runtime path
changed; their assemblies were really built. The nested-repository block adds
independent Git administration to the same already-tracked adapter contents,
changes no input identities and removes only those created Git directories.
It runs as a separate block; the other topology shapes interleave together.
Every lock, Cargo lock and receipt remains byte-identical through the timing.

| Two-adapter path | Over direct (median across modes/repeats) |
|---|---:|
| eligible | 13.441 ms |
| deep | 14.667 ms |
| long | 13.745 ms |

The shallow, deep and equally long shallow roots have equal tree digests, file
counts and bytes. Each path gets a real assembly and its own direct control.
Deep minus shallow is 1.12–1.29 ms across the six cells; long-shallow minus shallow
is 0.22–0.33 ms. This supports a depth cost but does not attribute the entire
installed-runtime delta. Path-depth optimization remains outside this record.

## Full checks, attachment and installed use

| One-native cost | Baseline repeats | Final repeats |
|---|---:|---:|
| Full --verify eval | 95.386 / 95.365 ms | 55.501 / 55.478 ms |
| Full-hash attachment | 264.950 / 264.599 ms | 170.823 / 170.366 ms |

Attachment validates the full artifact hash and uses real compiler-invocation
traps with positive controls. It is not free. Shared and override Polars journeys
pass: CSV, schema, filter/group/sum, catchable error with retained frame, Parquet
round-trip, preview equality and reset. Runtime migration remains the separately
measured gate-3 operation, not a warmup or part of these launch rows.

The installed journey archives exactly rnx 4855dbd into a fixture checkout, and
uses the ordinary tool and launcher built from that same source snapshot. It
does not claim a separate reproducibility build at the copied path. The checkout
is physically renamed before the first engine build; cache is empty at that point.
The four journeys pass: cold installed Polars scratch, compilation-trapped second
scratch, cold combined Polars/PostgreSQL project and compilation-trapped relative
second consumer. Both mixed projects execute typed SQL on a private cluster.
Frames survive catchable errors, old bindings disappear at handover, history and
working directory persist, and the printed shell-quoted scratch command reopens.
Cold-target transitions took 114.25 and 115.14 s with registry sources cached;
second consumers took 2.09 and 1.63 s including preparation/probe/restart.
No original checkout path routes the generated manifests or consumer locks.
All sessions and the postmaster are reaped. No user cache or runtime was removed.

## Regression and fixture adaptations

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Root default | 376 | 0 | 0 |
| Root test-support | 419 | 0 | 0 |
| Root combined features | 438 | 0 | 0 |
| Tool default | 46 | 0 | 2 |
| Tool test-support | 47 | 0 | 2 |

Both otherwise ignored tool integrations run separately and pass. The support
suite also prints a nested one-test child result; 47 is the suite count, not the
sum including that child. Root/tool fmt, strict all-target tool clippy in both
configurations, both notices checks and a fresh default selfcheck pass.

Root source, Cargo files, notices, kernel, adapters and server are byte-identical
to pre-record 7cd3205. The normalized default graph matches, has one member and
contains neither Polars nor the PostgreSQL driver. Public docs expose exactly
main_with, Extensions, Scope and the three server types. Tool source and its
dependency graph are byte-identical to accepted F5. Windows is not executed or
newly type-checked here. Commands and logs are retained under regression/ and scope/.

Replays pass: 44 preparation, 17 startup, eight commitment, 25 discovery, old
scratch reopen, 90 installer publication, both eight-case repair matrices,
16 workflow groups, 37 cache-publication cases, 30 authenticated migration cases,
quoted PATH-absent recovery, and interactive argv/verification/no-build controls.
The cache matrix kills a real builder and verifies that its waiter builds; it
also kills waiters. Every owned fixture process is reaped.

Historical drivers are archived with their effective source and adaptations:
current digest fields and private prospective key use BLAKE3; malformed current
selection controls use format 2/unknown 99; missing-runtime text uses install.
Interactive old/malformed receipts now refuse unchanged and require explicit
build, rather than asserting obsolete implicit migration. Genuine old-format
migration is tested by the current 0065 workflow and authentication matrices.
No refusal assertion is silently removed.

One failed preparation setup is preserved under failed-doc-edit/. I edited tracked
root documentation during its build. Post-build revalidation correctly refused
the changed native inputs. The exact accepted root was restored, the unpublished
target retained and a fresh preparation run passed. This was a fixture mistake,
not a product failure. Final documentation is written after the build-dependent
checks; future projects see those tracked text changes and must relock as usual.

## Qualification and next record

Gate 5 retains its accepted 0.016205 ms miss; the unchanged closing matrix passes.
Reuse is eligible-topology dependent and preserves the documented single-read and
between-child observation windows. It does not provide an atomic source snapshot
or cache source checks across invocations. The SHA-256 reader remains private to
authenticated old-installation migration. Old runtime and assembly entries remain
retained for old tools, direct launches, sessions and kernels.
The removal record is next and must account for those references before deletion.
No removal, source metadata cache, GIT_* whitelist, depth optimization, parallel
hashing or Windows execution is smuggled into this closing checkpoint.
