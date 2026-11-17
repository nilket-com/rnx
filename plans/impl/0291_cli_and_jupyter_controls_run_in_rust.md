# Implementation plan 0291: CLI and Jupyter controls run in Rust

Status: Implemented

Paired ADR: ../adr/0291_cli_and_jupyter_controls_run_in_rust.md.

1. Pin 427d0e32 and generate the witnesses from the published Python
   before any change.
2. The `cli` stage mode: the checker and its planted controls, the eight
   cases, the receipt.
3. The `jupyter` stage mode over the Rust child runner and the shared
   `stage::build_jobs`, with its children as one typed declaration; the
   route checker's `final STAGE` form for a Rust stage reads that
   declaration, and its controls.
4. Catalogue, scheduler, producer lists and docs; retire both programs
   with tombstones.
5. The affected checks, preflight and the commit checker; sign; source
   review; one complete Rust final; Results from its receipts.

## Results

`production-cli-controls` runs `rnx-final stage cli --binary B --out
DIR` and `jupyter-kernel` runs `rnx-final stage jupyter --out DIR`; both
are producers the child contract admits and `stage::bound` binds. The
scheduler keeps their resources: the Polars release lane for the CLI
stage, the root and Jupyter debug lanes for the Jupyter stage.

`stage::validate_cli` holds each run to the contract `cli.validate`
held, with its messages, and `stage::cli_controls` refuses the six
planted cases and passes the two good ones. `stage::cli` runs the eight
cases with the binary's digest taken before and checked after, reads each
run's output as Python's text mode did (strict UTF-8, universal
newlines), writes each run's stdout and stderr, and on an error writes the receipt's repr
and `TRACEBACK.txt`'s last line as Python did. `stage::jupyter` runs
`stage::jupyter_children`, the typed declaration of its three children,
through `child::run` under this executable as guardian, each with its
log, receipt directory, validation and 1800-second limit, with
`PYTHONDONTWRITEBYTECODE=1` and `CARGO_BUILD_JOBS` from one decision of
the shared `stage::build_jobs`, which every child reuses.

The route `final jupyter-kernel` holds through
`stage::declared_children`, the same declaration with inert paths, for a
command whose prefix is exactly `rnx-final stage jupyter`, which
`Entrypoints` reads; a Rust stage's own arguments are no route. Retired:
`tools/column/cli.py`, `tools/column/jupyter_controls.py`, and
`tools/column/test_column_protocol.py`, whose one control is now the Rust
checker's; the Python list holds 309 files. The witnesses at 427d0e32
cover the checker's eight verdicts, three CLI runs (pass, a skipped
output, a binary changed mid-run) and three Jupyter runs (inherited
width, a failing child, no client); the catalogue and selection fixtures
record the two command moves as a reviewed successor. Standalone, both
stages passed for real: eight CLI cases, and the two builds and the
pinned client's contract.

The first complete Rust final on this candidate failed at 15.9 minutes
in cross, which reported unreviewed census execution differences; the
per-case summary that would name the case and its cause was released
with the verifier's payload, so the cause is not known. The one approved
retry on the same commit passed in 38.5 minutes, with no memory.high,
memory.max or OOM events. It followed the failed run closely and likely
reused its build output, so its elapsed time is not comparable with
earlier finals. All 58 Rust stage contexts, the CLI and Jupyter stages
included, carry the controller digest the manifest records; the CLI
stage passed its eight cases, and the Jupyter stage's three children
each passed with their own receipt.
