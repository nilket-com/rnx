# ADR 0284: The Rust controller is the only final

Status: Proposed

Base: published 2b7323ae20325370e9d57d3862f5ebc1d9e51ad8 (ADR 0283).

## Problem

ADR 0283 put a Rust final controller beside the Python one, and both
ran the same complete final on one candidate. The Python controller
stayed as a witness. While both exist, every rule lives twice: stage
selection, the required stages, finalization and the stage list are
written once in `tools/column` (`accept.py`, `gate_policy.py`,
`required_stages.py`) and once in `tools/final`, and the Rust tests keep
the two in agreement by running the Python at test time. A change to
either is a change to both.

The witness cannot simply be deleted. Stage programs that stay Python
import parts of the controller's modules, and Rust tests import them
too. This record removes the Python controller and leaves one
implementation of each rule. The stage programs move in later records.

## Decision

1. `tools/final/final.sh` is the only way to run a final. `final.py`
   is deleted, and with it every function the Python controller alone
   reached. `final.sh`, the bootstrap admission, the 60-minute limit,
   owned-output cleanup, the fixed-route screen running alone, child
   receipt validation and the docs-only successor keep their meaning.
2. One implementation of each rule. Stage selection, the stage list,
   the required stages and finalization live only in the Rust
   controller: `accept.py` and `required_stages.py` are deleted, and
   `gate_policy.py` keeps only `record`, the record-identity reader two
   stage programs call (Table A). No Python program selects or finalizes
   a final after this record.
3. The four Python programs that call the deleted selection or
   finalization are disposed of one by one (Table B): the three
   historical recovery workflows are deleted, since their only use was
   finishing acceptances that are now published and nothing current
   runs them; the ADR 0242 session controls keep their development and
   Miri scopes and lose their `final` scope, whose job a Rust final of
   the explicit stages does.
4. Shared helpers stay where they are, with their meaning and process
   protocol unchanged, trimmed of code that only the Python controller
   reached: `child_result`, `owned_build` and `final_child`,
   `failure_detail`, `host_budget`, `package_gates`, the jobserver
   `Client` in `stage_scheduler`, and `assigned_root`, `canonical` and
   `sync_directory` in `final_evidence` (Table A). They move with the
   stage protocol's own record. No file is reorganized for style.
5. The Rust tests that ran the Python as an oracle keep their
   assertions against committed golden fixtures (Table C). Each fixture
   is generated once from the Python at 2b7323ae, by a method recorded
   beside it, with the root, output directory and owner replaced by
   placeholders; none is the Rust output copied back. The fixtures cover
   the stage list of every scope, the required-stage rules, stage
   resources and weights, the selection receipt of each scope, the
   environment digests, the producer classification and the
   diagnostics redaction. Fixtures hold no machine path, secret or bulk
   evidence.
6. Controls that only compared the two controllers retire, and say so.
   The cross-controller revalidation controls end here: ADR 0283
   accepted both directions on published finals, and that is their
   last witness. `CONTROLS.txt` keeps every row; a row whose Python
   control is deleted is marked as such, and its Rust test must still
   exist. Python stage-program tests stay and run.
7. Old evidence stays readable. The Rust controller keeps reading
   Python-written receipts and ledgers (schema 1), as revalidation and
   recovery of published finals require; nothing here rejects the
   format ADR 0283 accepted.
8. `rnx-final compare` stays, as a reproducibility check between two
   Rust finals of one candidate, with its fixtures. No record needs a
   second complete final because of it.
9. Current entry points and instructions name `final.sh`. Published
   plans and probes are history and are not rewritten.

## Table A: controller modules

| File | After this record |
| --- | --- |
| `tools/column/final.py` | deleted |
| `tools/column/accept.py` | deleted (the stage list lives in `tools/final/src/catalogue.rs`) |
| `tools/column/required_stages.py` | deleted |
| `tools/column/stage_outputs.py` | deleted (only `final`, `accept` and `resume` used it; Rust `outputs.rs` holds the rule) |
| `tools/column/stage_fakes.py` | deleted (test fakes of the Python controller) |
| `tools/column/gate_policy.py` | `git` and `record` only (`suite_stage`, `fixed_route_gate`) |
| `tools/column/final_evidence.py` | `assigned_root`, `canonical`, `sync_directory` (`clippy`, `fixed_route_gate`, `native_contracts`, `suite_stage`, `anchor_cache`); ledgers, release, recovery and revalidation deleted |
| `tools/column/stage_scheduler.py` | `Client` only (`native_contracts`); pool, resources, weights and workers deleted |
| `tools/column/host_budget.py` | the stage programs' width functions; `final_jobs` and `memory_events`, which only `final.py` calls, deleted |
| `tools/column/failure_detail.py` | what `child_result`, `fixed_route_gate` and `cross_program_gate` call; controller-only bundling deleted |
| `tools/column/child_result.py`, `owned_build.py`, `final_child.py`, `package_gates.py` | unchanged in meaning; dead controller branches only |

Their Python tests follow the code: a test of deleted code is deleted,
a test of retained code stays and runs.

## Table B: programs that select or finalize

| Program | Current callers | Disposition |
| --- | --- | --- |
| `chain.py` | `tools/column/README.md`; producer registration | deleted: reproduced a published prerequisite chain |
| `resume.py` | `tools/column/README.md`; producer registration; two tests in `test_column_protocol.py` | deleted with those tests: resumed a published, failed inventory run |
| `recover_cross.py` | `tools/column/README.md`; producer registration; one test in `test_column_protocol.py` | deleted with that test: finished a published cross-harness acceptance |
| `repl_session_controls.py` | ADR 0242 recipes | `development` and `miri` stay; `final` scope deleted |

The producer registrations of deleted programs stay in the Rust
contract table, so receipts they wrote remain readable.

## Table C: Rust tests that run Python today

| Test | Python it runs | After this record |
| --- | --- | --- |
| `src/catalogue.rs` tests | `accept.commands`, `accept.BULK_ROOTS`, `required_stages.required` | golden fixtures |
| `src/scheduler.rs` tests | `accept`, `stage_scheduler` resources and weights | golden fixture |
| `tests/selection.rs` | `gate_policy.select` for every scope | golden fixtures |
| `tests/selection_policy.rs` | `accept`, `gate_policy`, `child_result` producer classification | golden fixture; the producer rule runs on the Rust catalogue alone |
| `tests/compare.rs` | `final.environment_digests` | golden fixture |
| `tests/controller.rs` | `final.diagnostics` redaction | golden fixture |
| `tests/cross_revalidation.rs` | `final`, `stage_fakes`, `final_evidence.revalidate` | retired (Decision 6) |
| `tests/evidence.rs` | `final_evidence.tree` (payload identity) | golden fixture; `tree` is deleted from Python, since only the controller called it |
| `tests/controls.rs` | reads Python control names from `tools/column` | reads `CONTROLS.txt`; deleted modules' rows need only their Rust test |
| `tests/budget_interop.rs`, `tests/scheduling.rs` | `stage_scheduler.Client`, `native_contracts` | unchanged (retained helpers) |
| `tests/child.rs`, `tests/failure_witness.rs`, `src/pyjson.rs` | `child_result`, `owned_build`, `failure_detail`, Python `json` | unchanged (retained helpers) |
| `ci/test_commit_subjects.py` | `accept.commands` (commit discipline runs first) | asserts the same of the Rust catalogue's fixture |

## Constraints

- No check, stage or contract row is dropped except the controller's
  own, as the tables say. No new Python file; `python-files.txt` only
  loses entries.
- No new third-party crate. No real environment, argv or machine path
  in a fixture.
- No program starts a nested final, a second jobserver or a second
  driver lock inside a final.

## Controls

- Each golden fixture names its generating revision and method; a
  planted change to the Rust catalogue, required rules, selection,
  environment digests or redaction fails its test.
- A committed check refuses a live reference to a deleted module or
  entry point (an import, a command string, a registered stage) outside
  `plans/` and `probes/`.
- `CONTROLS.txt`: a row without its Rust test fails; a retained Python
  module's control without a row fails.
- Before signing, the affected Rust tests and the retained Python stage
  and helper controls run under a final-like environment (the final's
  jobserver variables, test threads and lock settings), and the stage
  classification at finalization is checked.
- On the signed candidate: one complete Rust final under 60 minutes,
  preflight, the Python list check, and the ordinary commit checks.

## Out of scope

The stage programs, the stage protocol (`child_result` and its
helpers), `ci/`, the measurement tools under `rune/tools/xtask` and the
151 files listed for deletion; each follows in its own record. The
open follow-ups of ADR 0282 and 0283 stay open: the baseline lint
cache, memory-high events, the fixed-delay interrupt tests, waking
queued one-token stages on returned tokens, and a recursive policy
digest for `tools/final/src`.
