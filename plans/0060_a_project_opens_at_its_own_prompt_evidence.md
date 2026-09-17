# rnx 0060: Linux implementation evidence

Status: ready for review, 2026-09-17. The accepted plan is 9a5b510.
The ordinary product and external fixtures pass gates 1–6; this is not a claim
of Windows execution. Data and commands are in rnx-bench's
`probes/project-interactive/` and `results/project-interactive-0060/`.

## What changed

Only the project tool's workflow, assembly helper and README changed. The private
launch operation takes run/session/eval intent, verifies the same lock, inputs,
receipt and artifact once, then constructs the selected command. Only run enters
the map publication/check branch. Interactive command construction still requires
an artifact::Checked, carries leading presentation flags and no entry or map,
and launches repl or eval through the same exec boundary.

Parsing validates new-command arity and options before project work. It keeps
source as an OsString for rnx's existing argument validation, not shell text or a
lossy conversion. Eval requires one source after --, including empty source;
session has no positional payload. --verify works for all execution modes.
Run's existing forwarding remains unchanged. No compiler, runtime, extension,
configuration, rendering or signal behaviour is reimplemented in the tool.

Root source, public API, manifest, lockfile, notices, kernel, adapters and server
are unchanged. No dependency, receipt, manifest or lock format was added.
The README now shows explicit lock, build and session steps and describes scope,
working directory, configuration, source checking and the verification default.
The bench Polars README includes a copyable session sequence tested verbatim.

## Behaviour gates

The tiny API-compatible fixture observes argv rather than pretending to execute
Rune. For both generated and override forms and both new commands, it confirms
matching-stamp zero artifact reads, forced full reads with a positive byte-count
control, legacy migration once, malformed/missing/stale receipts, touch refresh,
differing replacement refusal and restored-time source edits refused before any
artifact reads. Exact argument checks include flags translated ahead of repl/eval,
empty, multiline, non-ASCII and dash-prefixed source, and early syntax errors with
a nonexistent manifest. A PATH trap shows no Cargo/rustc invocation. Existing
0059 tests retain the inert undetectable-edit case and publication-failure gates;
there is no second artifact verifier needing a different coverage contract.

The real project builds the Polars extension and a source mount. Its entry would
write a marker in the current directory, and neither mode creates it. With an
absent derived map directory, both modes work without creating it. With a malformed
map at the content-addressed name, both leave its bytes/mtime unchanged. Editing
its mapped dependency with restored mtime still refuses before launch. A module
declaration remains refused by the session/eval engine.

The ordinary release is driven through a controlling PTY with TERM=xterm-256color,
120 columns and 30 rows, isolated history and explicit configuration paths. Both
project and direct artifact journeys create CSV in the inherited temporary working
directory, retain a frame across inputs, filter/group/aggregate, preview the two
expected rows, recover from a missing-column error with the original frame,
write/read Parquet, and compare previews. Reset removes the binding while Polars
remains usable. Actual terminal Ctrl-C retains the prompt; quit and terminal EOF
exit zero. The project advisory lock can be acquired while the prompt is alive.
A piped session and the bad-settings/clean-eval control pass as well.

Nine eval cases match direct artifact status, stdout and stderr byte-for-byte:
ordinary, Unicode, empty, negative and multiline expressions, unresolved name,
catchable native failure, explicit exit and module refusal. Non-Unicode argv
matches the engine's existing status-2 refusal. Raw/readable PTY logs are retained.
The initial harness matcher missed a trailing carriage return at an already-open
prompt; correcting that matcher was the only journey setup correction. It happened
before successful observations and before timing; the product required no fix.

## Everyday launch measurements

Two fixed-seed interleaved repeats, twenty observations per product/boundary,
one pinned CPU, one Polars thread set before spawn, warm caches. All 240 samples
are retained with checked output and exit status. Builds and receipt establishment
are explicit preparation, outside timing. Source snapshots, generated lock and
artifact hashes are recorded. No root files changed during the measurements.

| Boundary | Project default (ms) | --verify (ms) | Direct (ms) |
| --- | --- | --- | --- |
| eval completion | 24.56 / 24.62 | 90.57 / 90.58 | 6.50 / 6.51 |
| session first prompt | 23.38 / 23.44 | 89.47 / 89.25 | 5.18 / 5.23 |

Eval is spawn/capture/wait for polars::lit(1).is_ok(). Session includes identical
PTY setup and spawn through its complete first prompt; quit and reap happen
outside that readiness interval and are still checked. Both overheads are about
18 ms in each repeat, below the 25 ms gate. These are one-host product launch
observations, not notebook-cell times or pipeline throughput. There is no engine
speedup claim. Native inventory remains unchanged; --verify retains its full cost.

## Regression and limits

- Tool default and test-support: 37 passed each, zero failures, two ignored each.
- Strict all-targets Clippy passes with warnings denied in both configurations.
- Formatting and notices pass; 100 packages, 63 texts, six existing unavailable.
- Original workflow: sixteen groups pass. Original 0059 contracts: seven groups
  pass, including failure/interrupt refresh and changed-during-hash refusal.
  Older bench results were restored after those reruns.
- Windows all-targets type-check passes. The pre-existing test-only unused
  UNIX_EPOCH import warning remains; product commands still refuse and no Windows
  execution is claimed.

No user history/config, kernelspec or database was touched. All fixture children
were reaped. Source patches reconstruct the measured tool from the plan commit;
final evidence/README metadata can stale saved project locks, as the native
working-tree fingerprint deliberately requires. Relock/build explicitly on a
later checkout. No mapped-source prompt loading, automatic frame display, :dep,
dynamic native loading, background verification or source-cache policy was added.
