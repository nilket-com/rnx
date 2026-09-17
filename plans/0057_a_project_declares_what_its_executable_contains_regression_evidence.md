# 0057 gate 6: root regression and measured project costs

Status: submitted for review against accepted gate 5, rnx e50eda3 and bench
2d64744, both pushed. Gates 1–5 are accepted. This evidence does not silently
waive the existing Clippy failures or describe Windows as executed. No product
source, manifest, lockfile or notices changed in this gate.

## Checks

On Linux, Rust 1.98.1, TERM=xterm-256color, root configurations ran serially
with one test thread. Default: 375 passed. Test-support: 418 passed. Combined
test-support/server-runtime/project-sources: 437 passed, including five
compile-fail doc tests. No failures or ignored root tests. The packaged-manifest
and README-link checks pass in all three suites. The release selfcheck passes.

The independent tool passes 27 tests in both configurations, with two legacy
integration tests ignored in each (not claimed here as executed). Its formatting,
Clippy with warnings denied in both configurations, and notices pass. Root
formatting and notices pass too. Root's default cargo tree is byte-identical to
07f6709 after replacing only the checkout path; no project-tool dependency enters
the stock graph. The root workspace, kernel, adapter and server are unchanged.

Root all-targets Clippy does **not** pass on this compiler. Both 07f6709 and the
current tree report the same nineteen source-located findings: seventeen warnings
and two errors for non-octal zero permission literals in tests/config.rs and
tests/fs.rs. Strict Clippy also fails on the baseline warnings. The fixture compares
the sorted severity/message/location triples and proves equality, including the
server feature on both sides and project-sources on the new side. No findings
were suppressed and no unrelated lint cleanup was folded into this record.
This is a baseline qualification for review, not a clean-Clippy claim.

The project tool type-checks for x86_64-pc-windows-msvc. The full root check with
all features stops in ring's native build because this Linux machine lacks
lib.exe. That failure log is retained. An earlier tool invocation accidentally
selected the uninstalled GNU target; its log is labelled wrong-target, followed
by the successful MSVC check. Neither is Windows execution. The tool's explicit
Windows command refusal from gate 5 remains.

## Unchanged CLI and stock startup

The before binary is built from 07f6709 in a separate worktree and target; after
is a fresh ordinary release build of e50eda3. Both use locked offline builds,
the same compiler and default features. Source/binary identities are recorded in
rnx-bench/results/project-regression-0057/build.json. Cache was copied for the
baseline build only; these are launch measurements, not build-speed comparisons.

Twenty-five cases compare exit status, stdout and stderr exactly: version/help,
run/eval, argument forwarding, session bindings/renumber/reset, plain-file compile
and runtime diagnostics, missing-method recovery, returned errors, structs,
unit, stream output, budget refusals and exhaustion, debug-source output, and
JSON/CPU/string programs. All match. There is no terminal redraw comparison in
this gate; existing pty tests are in the root suites.

Hyperfine -N (no shell), CPU 4, ten warmups and 100 measured launches per binary
per workload, before/after followed by after/before, with no concurrent builds or
tests. Values below are medians in milliseconds, before → after for each repeat.

| Stock command | Repeat 1 | Repeat 2 |
| --- | --- | --- |
| version | 0.558 → 0.533 | 0.540 → 0.537 |
| help | 0.544 → 0.538 | 0.542 → 0.540 |
| run 42 | 3.713 → 3.667 | 3.720 → 3.661 |
| eval 42 | 4.062 → 4.011 | 4.066 → 4.013 |
| JSON 10k | 11.657 → 11.471 | 11.654 → 11.480 |

No measured workload regressed in either repeat. The small lower medians do not
establish a speedup or its cause. Raw samples and hyperfine's spread/outlier
reports are retained separately from the Python-clock project measurements.

## First build, warm build, and verification cost

The product fixture declares the real rnx runtime, PostgreSQL adapter, accepted
plain extension and mapped words source. The timed input returns 3 using the two
small extensions; it does not connect to a database. Gate 5 already established
the query contract on a private cluster. No synthetic runtime substitutes for
rnx in this build-cost measurement.

First build starts with an empty .rnx including an empty compilation target;
registry source packages are already cached and Cargo runs offline. No compiled
artifacts are copied. Build is followed immediately by another product build.
These are single wall-clock observations on this host, including the tool's
checks, not a distribution of compile speed or a network-install estimate.

| Operation | Observed time |
| --- | ---: |
| lock | 0.758 s |
| first build, empty target | 37.896 s |
| warm build, same target | 0.550 s |
| direct artifact + source map, median | 4.982 ms |
| project run, median | 45.591 ms |
| project verification/launch difference | 40.609 ms |

The run pair uses the exact same generated executable, entry and map. Three
warmup pairs, twenty measured pairs with alternating order, CPU 4. Python's
perf_counter_ns includes spawn/wait on both sides. The difference covers tree
inventory/hashing, lock and receipt validation, executable hashing, capability
handshake and launch work; it is **not** a pure SHA-256 microbenchmark. It is
explicit opt-in overhead, not stock-rnx startup. Root fingerprints cover the
whole tracked repository, so this number depends on the checkout's input size.

Receipt, lock identity, all samples, tool hash and build chatter are retained.
Final evidence edits naturally stale the fixture's native-root fingerprint;
these are historical snapshots, not reusable locks promised for the final tree.

## Reproduction and remaining qualifications

rnx-bench/probes/project-regression contains compare.py, startup.py,
project_cost.py and summarize.py, with commands and setup in README.md. Its raw
logs and derived summary are in results/project-regression-0057. The summary
asserts identical baseline Clippy findings and the unchanged default graph.

Gate 6 is ready for review with the baseline Clippy qualification above. Windows
execution and native toolchain setup remain open, as does the project command's
Windows supervision implementation. Nothing here changes gate 5's refusal of
self-referential input layouts, its trusted/non-hermetic Cargo contract, or the
server and notebook source-loading boundaries.
