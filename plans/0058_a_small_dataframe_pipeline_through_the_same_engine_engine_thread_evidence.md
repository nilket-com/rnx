# 0058 gate 1 rerun: synchronous calls own and join their engine threads

Status: revised boundary and gate 1 rerun submitted for review, 2026-09-17.
Accepted stop commits e1556c9, 17c952f and rnx-bench 2c4a278 are pushed. Decision
904c312 specifies the scoped-thread response. The original failure and its
provenance remain in `_boundary_evidence.md` and their original bench results.
No root source, root dependency or product adapter has changed.

## The boundary actually exercised

A single generic engine function requires Send for its captured inputs and
returned value. A fallible std thread builder spawns a named scoped thread;
there is no entered Tokio context on that thread. The call joins it before
constructing a Rune-visible result on the caller. Rune wrappers and borrowed
references stay on the caller. Clone the underlying plan or copy the path before
crossing the boundary. The probe uses this for collect, CSV reading and its fixed
frame constructor. Expression/plan construction and bounded inspection do not
execute the engine and stay on the caller.

Spawn failure is a named Result error; this path is source-reviewed, not forced
by exhausting process resources. An engine error is returned after join. An
unexpected engine panic is joined and then resume_unwind continues it on the
caller. It is not disguised as a Polars Result. The unit test catches that unwind
in the test harness and sees one start, finish and join with zero active calls.

There is no detached job, adapter runtime or persistent adapter worker. Polars'
separate global pool may persist. There is also no cancellation improvement:
Rune waits synchronously, and budgets/interrupts do not stop an in-flight call.
The spawn/join cost is unmeasured here and belongs in gate 5's full launch costs.

## Rerun results

The same file pipeline that produced the accepted native panic now returns the
same rows as synchronous eval. No catch_unwind was added around this path. Both
runs have empty stderr. The smaller controls all exit zero:

| Control | Original stop | Scoped engine thread |
| --- | --- | --- |
| construct frame in file | 0 | 0 |
| collect in file | panic, 101 | 0 |
| collect in synchronous eval | 0 | 0 |
| collect in async-promoted eval | panic, 101 | 0 |

The complete driver passes with POLARS_MAX_THREADS=1 and again with 2, in fresh
processes. Both retain borrowed frame/plan/group-by/expression and array reuse,
repeated collect/aggregation, ADD producing Expr with operands still usable,
comparison methods, and missing-column failure followed by a healthy collect.
Each engine thread asserts that Tokio Handle::try_current returns an error;
this is measured context absence, not inferred from its name.

The nested helper runs a real collect through an inner engine call while its
outer engine thread is alive, joins it, then collects on the outer thread. Both
return height 3. An overlapping pair of independent engine calls synchronizes on
a barrier before each collects, so the concurrency case cannot pass by running
serially. Both also return height 3, even with a one-thread Polars pool. This is
within one process and shared engine pool, not two independent executables.

Those helpers are called from a real Rune file. After them, a returned engine
error is caught by Rune. At the next statement the counters are exactly:

| Starts | Finishes | Joins | Active | Maximum active | No-context checks |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 5 | 5 | 5 | 0 | 2 | 5 |

The two helper caller threads in the overlap fixture are themselves scoped and
joined. They are test orchestration, not an additional adapter queue. The nested
and overlap controls are therefore not susceptible to a single-worker queue
waiting on itself. They do not establish arbitrary recursion/resource limits.

## Session ownership and remaining gates

A retained session constructs a frame, collects, remains idle, collects again and
resets. The final statement reports three engine calls started, finished, joined
and context-checked, none active. /proc snapshots distinguish those now-joined
scoped threads from the global Polars pool and its event descriptors that remain
through reset. The session exits normally with empty stderr and is reaped.
The unit panic test and the Rune returned-error case cover separate failure paths.

The prior eight CSV header cases and four public-reader modes now also execute
through run, with unchanged answers and no runtime panic. Supplied schemas still
rename headers and inferred schemas still de-duplicate; raw first-record reading
still distinguishes them. This confirms the public-reader route, not production
same-handle rewind validation. That belongs to gate 2, along with Parquet I/O.
The observer is still a small bounded-access helper rather than the full preview
formatter. No notebook integration or timing gate was performed.

## Reproduction and scope

rnx-bench/probes/polars-boundary/README.md gives the exact commands. New logs,
binary/source hashes and both pool-size results are in
results/polars-engine-thread-0058. Historical stop results are unchanged.
The release build, formatting, one panic/join unit test, and all-targets Clippy
with warnings denied pass. Root suites were not rerun for a source-only bench
change and plan/evidence edits; no root production line changed.

The probe's only lockfile edit adds a direct Tokio dependency edge to inspect
thread context. It was already in the accepted resolved graph: all 385 packages,
versions and resolved features are identical. graph-check.json asserts that the
only changed node is the probe itself. Original Rust crate/Python wheel identities
and licence inventory remain applicable. No same-revision engine pair or Python
performance result has been manufactured by the thread fix.

The runtime stop is resolved by this measured adapter-side boundary. The revised
record and rerun are ready for review before creating adapters/polars. Gates 2–6
remain open, and a persistent worker or awaitable engine API remains a separate
decision rather than a shortcut hidden in this implementation.
