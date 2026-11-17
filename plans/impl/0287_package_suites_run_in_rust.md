# Implementation plan 0287: Package suites run in Rust

Status: Implemented

Paired ADR: ../adr/0287_package_suites_run_in_rust.md.

1. Pin f0a66636. Before any edit, generate from the Python at that
   revision, in a clean scratch clone, the witness fixtures for every
   converted stage's commands (`suite_stage.recipes`, `clippy.py`'s
   construction) and for `clippy.diagnostics` on planted Cargo message
   lines, recording the method and placeholders.
2. The producer side of the stage protocol in Rust, with its controls,
   and the child contract's recognition of the two stage modes.
3. The stage mode: recipes from the catalogue tables, the suite and
   package runs with the existing discovery and execution helpers, the
   two Clippy comparisons with their baselines, and `VALGRIND_INCLUDE`.
   Port the Python controls of the table.
4. Rewrite the 55 commands, derive their scheduling from the stage
   classification, and add the successor notes to the catalogue,
   scheduling and selection fixtures.
5. Delete `package_gates.py`, `clippy.py`, `tools/column/verify.py`,
   `borrowed_controls.py` and `test_borrowed_controls.py`; trim
   `suite_stage.py` to `rune-suite`; extend the tombstone check; update
   `tools/python-files.txt`, and replace the column README's `verify.py`
   commands with the Rust final's workflow.
6. Run the affected checks in a final-like environment, preflight and
   the commit checker; sign; source review; one complete Rust final.
   Results state what the code does and the final's duration.

## Results

`rnx-final stage suite` and `rnx-final stage clippy` (`tools/final/src/stage.rs`)
run 55 of the final's stages: the 53 that ran `suite_stage.py` other than
`rune-suite`, and `clippy` and `adapter-clippy`. Their catalogue commands
name the logical program `rnx-final`; the child runner runs it as the
controller's own executable and records that executable's path and
digest in the stage's context, and the child contract takes a Rust
stage as a producer only in those two modes of the logical program.
Inside a final, a stage refuses before any child unless its context
names this executable (path and digest), this invocation and this
output directory, and binding rechecks the executable. The
recipes are the catalogue's own (`catalogue::recipes`), and
`package_gates.py` is gone. The Clippy comparisons keep their baselines
(the fixed published commit for `clippy` and `adapter-clippy`, the
record's baseline for the adapter, project and package lint stages),
their target directories, features and the Polars warning flag, and the
diagnostic identity with its counts; baselines unpack from `git archive`
into the final's owned baseline root, no member leaving it. The column
probe's Valgrind is resolved as before (a configured root, else the
first executable on PATH, either way with an executable `bin/valgrind`)
and its header checked before its Cargo runs. A child past its limit is
killed through a pidfd opened before it can be reaped; a stage stopped
once its Cargo is ready has its detached descendants drained by its
guardian.

The producer side of the protocol is Rust (`tools/final/src/producer.rs`):
a receipt bound to its validation context, a prior PASS replaced only on
an explicit resume of unchanged bytes and archived first, a completed
FAIL never, one terminal envelope written after the receipt is durable
and only while the candidate, source and declared environment hold. An
unbound run writes no binding.

Witness fixtures generated at f0a66636 hold the Rust mode to the Python
programs: every converted stage's children in order (command, directory,
log) and its rows, both Clippy comparisons' children with their target
directories, and the diagnostic identity on nine planted cases; a
planted change to each is caught. Scheduling is unchanged: the 0284
scheduling fixture's rows hold for every converted stage, and the
catalogue, scheduling and selection fixtures record the command change
as a reviewed successor. `suite_stage.py` runs `rune-suite` alone, with
its API-boundary twins and native contract cases. `clippy.py`,
`tools/column/verify.py` (ADR 0205's prerequisite workflow, whose README
commands give way to the final's), `borrowed_controls.py` and its test
are retired behind a tombstone; their producer registrations stay for
their receipts. The list holds 318 files.

The complete Rust final passed in 54.1 minutes, against ADR 0286's 45.7
minutes, with 864 memory.high events and no memory.max or OOM events.
The cause of the longer elapsed time has not been established.
