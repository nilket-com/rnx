# 0070 gate 2: the two spellings at the prompt

Status: passes on Linux, ready for review. The product is this record's
impl commit. The probe is `rnx-bench/probes/quiet-dep/`;
results in `rnx-bench/results/quiet-dep-0070/`. Gate 3 (the startup gate,
regression and slim) remains.

## The spellings

`:dep`, quiet — from a session started with its banner, recorded at a real
prompt (`journey.json`): `:dep polars` in a fresh HOME prints nothing between
the echo and the replacement's `[1] >` (zero lines, no banner) in 119 s; a
fresh session's `:dep polars` attaches in 8 s printing nothing; `:dep
postgres` in 44 s printing nothing; `:dep polars` again, already installed,
prints nothing. A failure (`--offline` with no Git checkout) prints
`resolve: <the error>`, the actual last lines of the output — the scratch
repair notice from the group-writable state directory among them, since it
is captured output — and the log's path; no framing. Ctrl-C prints the
interrupt error and the session continues; no process of the build remains.

`:depv`, verbose — held to 0067's journey line for line (`verbose.json`): a
declined request keeps `held` and writes nothing under the private state,
cache or data directories; the accepted request shows the notice naming the
revision and "Adding: polars", then `Continue? [y/N]`, then Cargo's lines,
`restart is beginning`, and the replacement prints `Reopen this scratch
session:`; `held` is gone; the history holds `:depv polars` and the
`history_marker` line; `:depv polars` again answers "already installed;
session unchanged"; `:depv --offline polars postgres` names "Adding:
postgres", "Already declared: polars" and "Offline", restarts without a
reopen notice (the same scratch project), and `postgres::query` exists. No
`dep.log` is written by verbose preparation.

## The commitment boundary

A cleanup or exec failure after the commitment is named under `:dep` too
(`exec_failure.json`): a test-support session paused at `before-exec`, its
artifact moved away, and the release prints `Error: "dependency restart exec
failed after cleanup: No such file or directory (os error 2)"` — the first
and only line after the echo, in the form the runner has always given a
terminal failure — and the session ends, as 0063 established.

## Compatibility and checks

The two peer directions (`compatibility.json`) as in gate 1: a new session
with the old tool refuses `:dep` by name and serves `:depv`; the old session
with the new tool is prompted and verbose. Root and tool formatting, tool
strict clippy in both configurations, tool suites 81 and 82, root suites 390
and 435.

## Documentation

The root README's first-use passage shows `:dep polars` followed by the next
prompt and says what `:dep` prints and where the rest goes; the tool guide's
"Requesting adapters from a live session" describes both spellings, the log's
contents, ownership and lifetime, the silent no-op, the announcement and the
reopen command under each spelling, and the older-tool refusal.

## Qualifications

The exec failure is provoked through the test-support pause, as 0063's
fixture did; a cleanup failure is the same path one point earlier and is
not repeated. Costs are single runs; gate 3 samples. No Windows claim.
