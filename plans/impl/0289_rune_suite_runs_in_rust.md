# Implementation plan 0289: rune-suite runs in Rust

Status: Implemented

Paired ADR: ../adr/0289_rune_suite_runs_in_rust.md.

1. Pin 837f2b5d. Generate the witnesses from the published Python
   before any change: recipes and twins, `run_case`'s arguments and
   environment over planted cases, catalogue verdicts over planted rows.
2. Move `discovered` and `executed` into `child_result.py`, with their
   parity control; move the anchor-cache invoke assertion to the Rust
   stage; move the host-budget control.
3. Port the recipes and twins into `stage.rs`'s suite path, replacing
   the rune-suite refusal; the catalogue entry uses the stage program.
4. The joining client in `job-budget`'s `fifo` module, with its
   controls.
5. A contracts module: manifest validation, the build, its
   compilation-unit check and its target's release, the case runner,
   the token rules over the client, and the catalogue through the
   finalizer's existing check, so there is one.
6. Retire the Python files and tests, with tombstones and route-registry
   updates; move the host-budget control; switch the two Rust tests to
   the Rust client.
7. The affected checks in a final-like environment, preflight and the
   commit checker; sign; source review; one complete Rust final; Results
   from its receipts.

## Results

`rune-suite` runs as `rnx-final stage suite --stage rune-suite`. Its
recipes are the catalogue's, the native manifest's libraries and the six
API-boundary twins, whose counts are now one table, `catalogue::TWINS`,
that the stage holds each twin's discovery and passes to. `stage::receipt`
writes the receipt `suite_stage.main` wrote, with `contracts` from
`contracts::run`, which must pass.

`contracts` ports `native_contracts.py`: manifest validation and the
catalogue through the finalizer's existing `native_manifest` and
`catalogue_check`; the one execution-and-serde recipe, the one Rune test
executable Cargo reports, its DWARF compilation units naming this
checkout, and `BINARY.json`; each case's discovery under the stage's
environment and execution under only the declared allowlist, fixture
folders, case variables and receipt and source bindings, the sandbox when
declared, the result-line, row, stderr and difference checks, the
appended row differences, and the per-case binary digest. A differences
file must hold its list, as Python's KeyError required, and a case's
limit keeps its fractions of a second. Every contract
process runs in its own process group, drained before its call returns;
past its limit it is terminated, then its group killed. The build target
is an `owned::Owned` directory, the port of `OwnedBuild`: claimed with an
ownership record, released once the build's group has ended and before
any case runs, its binary, receipt and top-level logs kept and journaled,
with a cleanup record for the stage's context.

`job_budget::fifo::Client` ports the Python client over the existing
transport: it joins the controller's pool through `MAKEFLAGS`, acquires a
count of tokens, or one from the stage's own slots or the pool,
whichever frees first, and each `Permit` returns every token to its
source when released or dropped, on a panic too. The `Pool` stays the
sole owner. An exclusive case holds every token its stage does not;
shared cases run at most the case width, each on one permit.

`child_result.py` now holds `discovered` and `executed`, held to the Rust
parsers by a control; the anchor-cache invoke assertion is a control on
`stage::Live::invoke`; the host-budget control is in
`test_host_budget.py`. Retired: `suite_stage.py`, `native_contracts.py`,
`stage_scheduler.py` and their three tests (tombstones in
`tests/retired/tools-0289.txt`); the Python list holds 312 files. The
witnesses at 837f2b5d cover the recipes and twins, sixteen planted cases,
the manifest and catalogue verdicts, eight builds and the receipt; the
catalogue and selection fixtures record `rune-suite`'s command move as a
reviewed successor.

The complete Rust final passed in 46.9 minutes, with 1841 memory.high
events and no memory.max or OOM events. All 56 Rust stage contexts,
rune-suite's included, carry the controller digest the manifest records.
rune-suite passed its 11 recipes and all 50 native contract cases, the
contracts in 12.0 minutes; it started at 15.0 minutes, after token
refusals at 6.6 and 11.0, and ended at 33.3. From 23.2 to 33.3 minutes
the controller was blocked with only rune-suite running, while
budget-controls and suite-adapter-candle, last refused tokens at 22.6
and 22.4, waited; fixed-route then ran alone from 36.1 to 45.8.
