# 0063 gate 4: real Polars and PostgreSQL sessions

Status: implemented, ready for review on Linux. Gates 1–3 are accepted; gates 5
and 6 remain open. The source baseline is `f60af09`. The exact measured changes,
including the README inputs, are archived in the bench's
`results/session-dogfood-0063/source.patch` (SHA-256
`745fa67208993f5d78e0cd6b37958c0c7519d546ff44ed849d1bb46969f15f08`).
The later evidence/status edit changes the root native fingerprint. Conditions
record the frozen ordinary binaries and the actual generated artifacts.

## Product change

A new scratch handover includes a quoted reopen command supplied by the project
tool, using its actual executable and the retained manifest. The private ready
frame accepts this optional field; project handovers retain the previous shape.
The REPL carries the notice into the replacement, where it prints after serving
context/session initialization and history loading, before the first prompt.
There is no precommit claim that the replacement has started. The announcement
is tied to the replacement PID so children cannot repeat their parent's notice;
a later handover removes or replaces it. This is trusted routing/display metadata,
not a capability or an additional public flag.

The runtime, terminal ownership, lifecycle, startup probe, fingerprint and shared
cache policies are unchanged. The root/tool READMEs now describe the actual
workflow, runtime/tool selection, retained scratch state, binding loss, history,
offline mode, two builder invocations and the commitment boundary. No dependency,
manifest, lockfile, notices, kernel, adapter or server package changed.

## Real journeys

`probes/session-dogfood/check.py` runs the actual Polars and PostgreSQL adapters,
not catalogue-shaped stubs. Its final run starts with an empty private target
cache and state root; registry sources are cached, so this is not a cold download.
It uses ordinary product binaries frozen by setup, xterm-256color at 120 columns,
one Polars thread and a working directory separate from all project directories.

* A stock session explicitly selects the source runtime. Declining writes no
  scratch or cache. Consent builds the Polars assembly, runs the startup check,
  and replaces the same PID with prompt 1. The replacement prints the reopen
  command. The state root contains a space and apostrophe; executing that exact
  command through `/bin/sh` reopens the retained scratch with Polars live.
* A second stock session creates a different scratch project and attaches to the
  identical assembly key and artifact digest. Compilation is forbidden by both
  Cargo and rustc traps with rejecting positive controls. Read-only target/version
  queries still delegate to the real tools. The compile log stays empty; reuse is
  not inferred from speed.
* Projects with absolute and relative runtime declarations start with Polars.
  `:dep --offline polars postgres` reports Adding postgres and Already declared
  polars, adds the lifecycle hook in the original path form, then restarts with
  both namespaces. The first builds the combined assembly; the second attaches
  to it with compilation forbidden. Each executes a parameterized text/int8 query
  on a private Unix-socket cluster. Quotes, SQL-looking text, a backslash and an
  emoji return exactly, alongside integer 42. Repeating an installed dependency
  request is a no-op.
* Each handover verifies the same PID, foreground terminal ownership, no child
  processes, fresh prompt numbering, missing old bindings and unchanged working
  directory. Up-arrow recalls saved input; Ctrl-C discards it, and its binding
  is still missing, proving history was not replayed.
* The scratch and both projects bind a frame, filter/group/aggregate/sort across
  several inputs, recover from a missing-column result, preview the original
  frame, write uncompressed Parquet, read it back and compare previews. Repeated
  collection still works. The rows are `a | 2` and `🦀 | 7`; CSV/Parquet outputs
  land in the caller's directory.
* Quit reaps every session. The database observer sees no tagged backend after
  each project quits, and the private postmaster is gone after cluster cleanup.
  No user project, state, cache, settings or history is touched.

| Journey | Consent to new prompt | Compilation |
| --- | ---: | --- |
| First stock → Polars | 111.577 s | Real cold target |
| Second stock → Polars | 2.010 s | Forbidden; none |
| Absolute project → Polars + PostgreSQL | 117.514 s | Real cold target |
| Relative project → Polars + PostgreSQL | 2.107 s | Forbidden; none |

These are descriptive consent-to-new-prompt observations, not gate 6's matched
latency benchmark. Compile logs, raw PTY transcripts, declarations, identities,
reopen output and process checks are retained with the matrix.

## Fixture corrections and checks

Two fixture mistakes are preserved under `fixture-correction/`. The first assumed
that a missing-binding compile error did not consume an input number. The fresh
prompt is now asserted at handover and binding loss separately. The second
mistook Cargo's `rustc --print` target-information query for compilation. The
trap now permits read-only queries and still rejects real compilation, with
positive controls. After these corrections, the recorded final run starts fresh
and completes the entire driver without a resume path. Product source stayed
frozen throughout; neither case required weakening a product check.

Root formatting and the serial default suite pass (376 tests). Tool formatting,
strict all-target clippy and 40 tests pass in both configurations (two existing
ignored tests each). Both notices checks are current. Root strict clippy still
reports the same 13 pre-existing diagnostics, with no new one. The full root
feature matrix, postcommit failure matrix and matched launch measurements remain
in gates 5–6; this checkpoint does not close the record.
