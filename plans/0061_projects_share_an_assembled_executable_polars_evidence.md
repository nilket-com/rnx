# 0061 gate 5: two Polars applications share one retained assembly

Status: ready for Linux review. Gate 4 is accepted and pushed at rnx `3c4ed67`
and rnx-bench `4b58319`. This gate changes documentation and adds a bench fixture;
no product code, dependency, source verification or launch policy changes.
Gate 6 remains open.

## Reproducible source and setup

All product measurements use the ordinary release tool and native rnx/Polars
sources from published rnx `3c4ed67a2e34d2e678bdabd7d9233a2d7c0cc420`.
The bench base is published `4b5831938e2a14340b0ab4df327747b0d0f84333`.
The new `probes/cache-polars/` scripts and `results/cache-polars-0061/` contain
commands, manifests, Rune scripts, both lock pairs and receipts, the ready
document, tool/artifact hashes, raw Cargo logs, PTY transcripts and all samples.
There is no instrumented product binary or source patch.

The two projects live outside the native roots in the probe's ignored target
directory. Each declares the same canonical runtime and Polars adapter. Their
different Rune scripts print distinct consumer markers and execute the shipped
CSV, filter, group-by, sum, sort, collect, create-new Parquet, read-back and
repeated-collect pipeline. The cache is fixture-private; no user cache, project
or kernelspec is modified.

Setup refuses a pre-existing cache. Lock creates no compiled entry. The first
build uses a new final cache-owned assembly path and an empty target; no Cargo
products are copied or seeded. All commands use `--offline`, with dependency
sources already available in Cargo's registry cache. Thus there are **zero
downloads** in the measured build, not downloads hidden inside compilation.
This is a cold **target**, not cold filesystem pages or a fresh dependency cache.
Build uses the host's available CPU set (0–27). Later attachment and launch
measurements pin CPU 0 and set `POLARS_MAX_THREADS=1` before spawn. No compilation
runs alongside those timing samples.

Machine: Linux x86-64, Intel Core i7-14700. Exact kernel, CPU information, rustc,
Cargo and affinity are retained in the conditions files. Absolute paths and
native-tree fingerprints are intentional inputs: moving the cache or changing
even this repository's tracked documentation requires fresh lock/build work.
These measured projects are not claimed to remain current after this evidence
commit. Published `3c4ed67` preserves the actual source state measured.

## Cold build, attachment and ownership

| Operation | Wall time | What it includes |
| --- | ---: | --- |
| First project lock | 1,067 ms | Offline resolution, snapshots and lock publication |
| First shared build, empty target | 105,704 ms | Compilation, validation, ready and receipt publication |
| Second project lock | 1,068 ms | Its own sources and lock, same native identity |
| First second-project attachment | 566 ms | Ready-hit validation including full artifact hash and receipt |
| Ten further pinned attachments | 315 ms median | Full validation and receipt publication every time |

The initial lock/build/attachment observations used the host's available CPU set;
the ten repeated attachments were pinned. The latter range was 314–317 ms.
They are reported separately rather than used to relabel the initial attachment.
The Cargo build log reports 1m44s compilation; the outer command clock additionally
includes all project checks and publication. No speedup is inferred against older
cold-build observations made under different conditions.

Both project locks select the same assembly key and both version-3 receipts
identify the same executable path and digest, but different project-lock digests.
There is exactly one entry. The executable is 107,436,336 bytes. The cold-build
log records compilation of the real dependency graph and assembled application.

Before attachment, PATH traps are positively tested with `cargo build`,
`cargo metadata` and a non-allowed rustc invocation: each refuses with status 91
and writes a marker. Only exact `cargo -V` and `rustc -Vv` are passed through.
The marker is then cleared. The first second-project attachment and all ten
repeated attachments succeed, report `attached shared`, and leave no trap marker.
Zero compilation is an invocation assertion, not a conclusion from elapsed time.
Repeated attachments leave ready bytes, executable bytes, inode and mtime intact.
Each attachment still performs the production full artifact check against the
ready digest; its cost has not been removed or mixed into the everyday row.

Whole-entry allocated disk use from `du -s -B1` is **1,570,058,240 bytes**
(1.46 GiB), and apparent size is 1,558,701,046 bytes. File-wise assembly/target/
artifact subtotals are saved too, but their sum can double-count Cargo hard links;
the whole-entry number deduplicates them. The README now names this measured
cost. Retain the complete directory, not just the executable. This gate adds no
eviction, relocation, or executable-only lifetime policy.

## Sessions and eval

Both consumers open real 120×30 xterm-256color PTYs through the project command.
The journey creates a CSV in the caller's directory, binds a frame and a lazy plan
over several inputs, previews the aggregate, gets a catchable missing-column
error, then previews the original frame, writes/reads Parquet and recollects the
original plan. Reset removes the binding while the Polars extension remains.
Every prompt assertion passes; both sessions quit with status zero and are reaped.
Eval success and a catchable missing-file result match the direct artifact's
status/stdout/stderr. The raw terminal streams and readable transcripts are saved.

## Everyday launch, full verification and direct control

Two interleaved repeats, twenty samples per project/mode/launch cell, fixed seed
61500, **720 samples retained**. One warm-up per cell establishes maps and warm
pages; it is outside measurement. No receipt rewriting or timestamp manipulation.
There is no shell in the timed path. Pipeline/eval clock from spawn through exit;
session clocks PTY allocation and spawn through the complete first prompt, with
quit and reap outside that interval. Temporary output directories are prepared
and removed outside timing. Every pipeline sample verifies its exact preview,
consumer marker, Parquet roundtrip, repeated collect and successful exit. Every
eval prints `true`; every session presents exactly the expected first prompt.

Medians in milliseconds, repeat one / repeat two:

| Consumer and workload | Default | `--verify` | Direct |
| --- | ---: | ---: | ---: |
| First: pipeline | 31.03 / 30.90 | 97.23 / 97.28 | 10.78 / 10.80 |
| Second: pipeline | 31.09 / 31.01 | 97.32 / 97.22 | 10.71 / 10.72 |
| First: eval | 26.68 / 26.71 | 92.74 / 92.73 | 6.54 / 6.58 |
| Second: eval | 26.66 / 26.64 | 92.94 / 92.82 | 6.55 / 6.59 |
| First: first prompt | 25.42 / 25.38 | 91.55 / 91.68 | 5.32 / 5.37 |
| Second: first prompt | 25.42 / 25.43 | 91.40 / 91.52 | 5.26 / 5.38 |

The default-over-direct median difference is 20.02–20.37 ms. Every project/mode/
repeat passes the unchanged **25 ms** limit. Full verification and explicit
attachment remain separately visible. No source-check exemption or faster input
cache was needed. This measures whole product launches; it is not a new Python
comparison, a promise of instantaneous dependency loading, or an attribution of
every overhead phase.

## Review boundary

Gate 5 passes its two-consumer, interactive, zero-compilation attachment, size and
timing requirements. The fixture's processes have exited; the private cache is
retained whole for inspection. Root source/API/default graph, all manifests and
lockfiles, notices, adapters, kernel and server remain unchanged. The only rnx
changes are this evidence, plan status and the tool README. Broad tool/root and
command-regression checks remain gate 6; Windows execution is not claimed here.
Tool formatting and the unchanged notice inventory check pass. Fixture Python
syntax parses, and all 720 saved samples match the append-only measurement journal
with twenty observations in each of the 36 project/mode/launch/repeat cells.
