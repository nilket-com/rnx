# ADR 0286: Every Python test has a route

Status: Proposed

Base: published 0bacd2c76eb554ad5111dcb4e981e29b546bf8ab (ADR 0285).

## Problem

Of the 129 Python test files left, the complete final runs 62 (the
`tools/column` and `tools/routing-bench` discoveries and
`tools/test_verify_pins.py`) and preflight runs four more. Another 51 run
only outside the final, by maintained commands: 46 measurement controls
by `cargo xtask suite measure-checks`, five supplemental-Miri controls by
`rune/tools/xtask/supplemental/run.py`'s own prerequisite. The last 11
run nowhere, so nothing notices when they or their subjects break, and
nothing records which of these routes a test has. Some of those 11 test
live code; some test instruments of finished records.

The inventory, pinned at the base with Python 3.14.4 and unittest's
loader (each file imports cleanly; a failed import or discovery counts as
an error, not as no tests): 227 test cases in those 62 files, 173 in the
51 routed outside the final (143 measurement, 30 supplemental) and 54 in
the 11 with no route.

## Decision

1. Every tracked Python test file names its route in
   `tools/python-files.txt`, as a third column on its row:
   - `final <stage>`: a stage of the Rust catalogue that runs it, by
     path or by a discovery whose directory and pattern select it;
   - `preflight`: `ci/preflight.py` runs it;
   - `xtask measure-checks`: listed in `measure_checks.rs` `TESTS` with
     its discovered count;
   - `workflow supplemental`: `supplemental/run.py`'s prerequisite
     discovers it;
   - `deferred: ` and why, for a retained family whose route is not
     resolved here.
   A Rust check validates each route against the entry point it names
   (the stage's command, preflight's source, the `TESTS` list, the
   workflow's discovery), and refuses a test file with no route, an
   unknown route, or a `deferred` route without a reason. A `deferred`
   route is not a gate, and the record says so.
2. The disk rule keeps one value and gains a route, and moves to Rust
   with its callers. The Rust controller checks the 20 GiB floor itself
   (`controller::disk_check`); five Python launchers ask
   `ci/disk_space.py` before they build or start a unit
   (`borrowed_controls.py`, `ir_launch.py`, `launch.py`,
   `paired_build.py`, `supplemental/run.py`). Calling a Rust binary from
   those Python launchers would need a bootstrap that itself builds
   before it can refuse on low disk, which costs more than the 36-line
   rule. So the Python rule stays the launchers' only copy until they
   move to Rust and call the shared library directly; a Rust control
   requires its floor and its handling (every path and
   `CARGO_TARGET_DIR`, a missing path judged by its nearest existing
   ancestor, the boundary at exactly the floor) to equal the
   controller's, by running both on the same planted paths; and its two
   tests (`ci/test_disk_space.py`, `ci/test_disk_launchers.py`, the
   launchers asking before they build) run in preflight.
3. Two live tools gain a final route as separate stages, each with its
   own receipt: `tools/test_engine_source.py` (6 cases; its subject is
   read by `tools/verify.py`, the cross stage) and
   `tools/test_refresh_census.py` (3 cases; its subject is the census
   refresh workflow `tools/verify/README.md` documents). They port with
   `tools/verify.py` later.
4. Three finished-record instruments are retired, with their tests:
   `tools/redact_metadata.py` (the one-time metadata redaction of record
   0231; its data, `tools/0231_metadata_redactions.json`, keeps its
   registry entry), and the one-record replay instruments
   `rune/tools/xtask/measure/n3_qualification.py` and
   `e1a_qualification.py`. Their only data, `qualifications/n3.json`,
   `qualifications/n3-proof.json` and `qualifications/e1a.json`, has no
   other consumer and is deleted, recoverable at the base; the registry
   entry for `n3-proof.json` goes with it. The other qualifications (n5,
   s8) and their tests are unchanged.
5. `test_callgrind_n1_cold.py` (10 cases, testing the live
   `callgrind_rules.py`) joins `measure_checks.rs` `TESTS`. That puts it
   on the maintained measurement route, not in the final.
6. The measurement and supplemental controls keep their existing
   routes in the registry (`xtask measure-checks`, `workflow
   supplemental`), which the check validates; the record states that
   both run outside the final, and joining either to the final is its
   own measured decision. `deferred: ` is reserved for the two families
   whose execution route is unresolved:
   `rune/tools/xtask/column/test_cli_cache.py` and `test_cli_rules.py`
   (their subjects are a registered producer and its helpers), and
   `tools/column/compare/test_compare.py` (the Polars, Arrow and DuckDB
   comparison harness, which needs its own environment).
7. The deleted files join the tombstone check of ADR 0285, with their
   own list.

## Constraints

- The complete final stays under 60 minutes. The two new stages are
  seconds long and run in the shared budget. No new scheduler, no
  launcher bootstrap.
- No new Python file; `tools/python-files.txt` only loses entries.

## Controls

- The route check, with planted cases: a test file with no route, a
  stage that does not run it, a preflight that does not run it, a
  `TESTS` entry missing or with the wrong count, a `deferred` route
  without a reason.
- The disk rule: the controller's and the launchers' rule agree on a
  passing and a failing path, a missing path judged by its ancestor,
  exactly the floor and `CARGO_TARGET_DIR`; the two Python disk tests
  run in preflight.
- The witness fixtures that the two new stages change (the catalogue,
  scheduling and selection fixtures) are marked as reviewed ADR 0286
  successors of the 0284 Python witness, naming the exact addition;
  nothing else in them changes, and nothing is regenerated from the
  Rust output.
- The tombstone check over the retired files and data.
- One complete Rust final.

## Out of scope

Porting the measurement and supplemental families, the CLI capture
tools, the comparison harness, `tools/verify.py`, the disk launchers and
the stage protocol; joining the maintained routes to the final.
