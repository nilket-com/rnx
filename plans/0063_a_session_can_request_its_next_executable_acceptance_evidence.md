# 0063 gate 6: ordinary regression and transition costs

Status: implemented, ready for review on Linux, alongside gate 5. Gates 1–4
are accepted. The source baseline is `b8ba8e6`; the exact test-instrumentation
patch is archived at `rnx-bench/results/session-final-0063/source.patch`.
Ordinary product policy, manifests, dependency lockfiles, notices, kernel,
adapters and server package are unchanged in this checkpoint. Evidence/status
edits follow the measured source snapshot and therefore change its native tree
fingerprint; they are not silently substituted for the measured inputs.

## Real transitions and phase costs

The ordinary release CLI and tool were frozen before the real dogfood rerun.
Both scratch journeys and both absolute/relative mixed projects pass: retained
history, lost bindings, prompt 1, unchanged working directory, correctly quoted
reopen command, real Polars transformations/Parquet, typed PostgreSQL query and
independent backend/process cleanup. Both attachment consumers have compilation
trapped with positive controls. The registry sources were cached; cold means a
fresh target, not downloads. Other validation builds ran during these cold
observations, so these are experienced elapsed costs, not isolated compiler
benchmarks.

| Consent to replacement prompt | Observed |
| --- | ---: |
| First scratch, real Polars cold target | 147.495 s |
| Second scratch, compilation forbidden | 1.562 s |
| First mixed project, real combined cold target | 152.705 s |
| Second mixed project, compilation forbidden | 1.438 s |

The notice gives the existing approximate 100-second precedent before consent;
this run's longer cold costs are retained rather than replacing the precedent
with a promise. Every raw transcript and compile invocation is archived.

`dogfood/phases.jsonl` observes output delivery at the PTY, rather than pretending
these are disjoint internal CPU intervals. Nearby author/resolve markers can
arrive together. Resolve took about 0.90–1.36 seconds; the two cold build/attach
envelopes took 146.08 and 151.74 seconds. Ready attachment envelopes took 373 and
402 ms. Startup-check plus subsequent checks/ready delivery took 30–40 ms.
Commitment announcement to first prompt took 5.4–7.3 ms for these empty owners.
Forwarding and polling are included and can move time across adjacent boundaries.

Two separate observations refine those envelopes:

- Gate 5's test-only monotonic stamps across the real tool and session give
  author, resolve, attach and cleanup boundaries for the small adversarial
  fixture. Successful old-owner cleanup envelopes, including editor/history and
  normal stack return, are 7.1–8.1 ms. Pauses are outside those intervals. These
  are real started HTTP/SQL owners, not the empty-owner Polars transition.
- The ordinary combined artifact's actual startup-probe child is measured below:
  session settings, both builders, eval 42, cleanup, exact readiness frame and
  exit. That isolates the child from the tool's validation/forwarding envelope.

## Matched everyday launches

After all other builds and fixtures finished, `probes/session-final/measure.py`
ran two fixed-seed interleaved repeats on one CPU with one Polars thread.
Project, direct and verify launch exactly the same combined artifact. Each cell
has 30 measured samples per repeat; ordinary persistent cells have 100. All
1,060 samples, commands, binary hashes and outputs/status assertions are retained.
Warmups are explicit and outside those samples. PTY dimensions are 120 by 30.

Medians in milliseconds, repeat one / repeat two:

| Workload | Direct | Project default | Project `--verify` |
| --- | ---: | ---: | ---: |
| Eval a Polars expression | 6.421 / 6.523 | 30.655 / 30.710 | 97.718 / 97.689 |
| Spawn to first prompt | 5.233 / 5.282 | 29.501 / 29.478 | 96.637 / 96.593 |

Default-over-direct is 24.19–24.27 ms: the unchanged 25 ms gate passes in every
mode and repeat. Full verification remains explicitly more expensive. The real
startup-probe child costs 6.451 / 6.515 ms. No claim that attachment is free or
that a cold compile has disappeared is made.

Matched stock baseline `78c514d` (before 0063) versus current ordinary release:

| Workload | Before | After |
| --- | ---: | ---: |
| Version | 1.967 / 1.950 | 1.924 / 1.951 |
| Eval 42 | 5.858 / 5.815 | 5.659 / 5.605 |
| Ordinary cell, already-running session | 0.351 / 0.355 | 0.355 / 0.356 |

These clocks include Python spawn/capture/wait, or PTY setup to full prompt.
They are matched product comparisons, not bare process startup claims. Ordinary
cells exclude startup and execute no dependency transition. Their difference is
under 0.005 ms; no material regression or speedup is claimed. The detached
baseline worktree is removed after archiving its commit and binary hash.

## Regression and qualifications

| Check | Result |
| --- | --- |
| Root formatting | Pass |
| Root default, serial | 376 passed |
| Root test-support, serial | 419 passed |
| Root combined test-support/server-runtime/project-sources, serial | 438 passed |
| Tool tests, both configurations | 40 passed each, two existing ignored each |
| Tool real opt-in integrations | Both passed |
| Tool formatting and strict all-target clippy, both configurations | Pass |
| Both notices and fresh default selfcheck | Pass |
| Default dependency tree against `78c514d`, paths normalized | Identical; no Polars or PostgreSQL driver |
| Generated public API | Existing Extensions, Scope, Rune re-export, main_with and three server types only |
| Root strict production clippy | Same 13 diagnostics on baseline and current; not clean, no new diagnostic |
| Root strict test-support clippy | Those 13 plus the existing worker fixture diagnostic; worker source byte-identical to baseline |
| Full root Windows GNU combined type-check | Pass with an isolated real MinGW compiler; no source substitutions |
| Tool Windows MSVC all-target support check | Pass, platform-disabled dead-code warnings retained |

The initial root MSVC attempt failed inside ring because this Linux environment
lacked `lib.exe`. Its failure log is retained. Unpacking a GNU cross-compiler
under `/tmp` and using the installed Windows GNU Rust standard library allowed
the complete root check to pass. No Windows executable was run; transitions
retain their explicit unsupported-platform refusal. Ordinary session code was
included in the complete root type-check.

The 44-group preparation and 17-group startup matrices pass again. Existing
cache commands, metadata/full verification, interactive contracts, Polars
override PTY, shared publication failures, legacy workflow and mapped PostgreSQL
workflow all pass. The legacy replay uses the frozen pre-cache tool solely to
create legacy locks; current commands build and launch. Its initial unadapted
0057 driver assumed a local artifact and is retained as a fixture error. The
accepted replay adaptation is archived, not hidden behind a changed assertion.
Published result directories are restored; inherited old result files were
excluded from the new archive by unchanged bytes and original nanosecond mtime.
The replay script now starts its output directories empty.

The existing Jupyter supervision, extended queue/interrupt/worker-failure cases,
and private-kernelspec notebook execution/save/reopen all pass with the fresh
ordinary worker. No kernel or worker is left running. Root suites also retain
ordinary run/eval/session interrupt checks. Private PostgreSQL clusters are
independently observed gone. This is Linux execution evidence only.

The next record is runtime installation, so stock `:dep` no longer requires a
source checkout selected through `RNX_DEP_RUNTIME`. Dynamic loading, binding
migration, mapped modules at the prompt and Windows transition execution remain
outside 0063.
