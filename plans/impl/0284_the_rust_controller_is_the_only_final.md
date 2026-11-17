# Implementation plan 0284: The Rust controller is the only final

Status: Implemented

Paired ADR: ../adr/0284_the_rust_controller_is_the_only_final.md.

1. Pin 2b7323ae. Generate the golden fixtures of Table C from the
   Python at that revision, in a scratch clone, with the root, output
   directory and owner as placeholders; commit each with the command
   that made it and the revision. Point each Rust test at its fixture
   and plant one change per fixture to show the test fails.
2. Retire `tests/cross_revalidation.rs` and port `tests/controls.rs` to
   read `CONTROLS.txt` alone, marking the rows of deleted Python
   controls.
3. Delete the Python controller and the Table B programs, then trim
   the retained modules of Table A to what their remaining importers
   call, with the tests of deleted code. Update `python-files.txt`,
   `tools/column/README.md` and any current instruction that names
   `final.py`, and the committed check for live references to deleted
   modules and entry points.
4. Run the affected Rust tests and the retained Python stage and helper
   controls under a final-like environment, and check finalization's
   stage classification, before signing.
5. Sign the candidate. Run one complete Rust final through
   `tools/final/final.sh` in its own unit, then preflight and the
   commit checker. Results state what the code does, the final's
   duration against ADR 0283's, and carry no paths under target/ and
   no development history.

## Results

`tools/final/final.sh` is the only final. `final.py`, `accept.py`,
`required_stages.py`, `stage_outputs.py`, `stage_fakes.py`, `chain.py`,
`resume.py`, `recover_cross.py` and `accept_remote.py` are deleted, with
the Python tests of the controller alone (`test_final_evidence`, `test_final_successor`,
`test_gate_policy`, `test_required_stages`, `test_stage_outputs`).
`gate_policy.py` is the record identity (`git`, `record`) without a
required-stage list; `final_evidence.py` holds `sync_directory`,
`canonical` and `assigned_root`; `stage_scheduler.py` holds the jobserver
`Client`; `host_budget.py` loses `final_jobs` and `memory_events`;
`package_gates.py` keeps the recipes `suite_stage` runs and loses the
package table; `owned_build.py` loses the recorded-group drain only
recovery called. The ADR 0242 session controls keep their development
and Miri scopes. Retained Python tests that ran the controller now test
their helper alone: the failure-retention controls release a payload by
removing it, and the jobserver controls build their pool and worker
locally. The native-contract finalization control and the catalogue's
inherited-gate control are Rust tests.

Remote acceptance on a verifier host is retired: its launcher started
`accept.py`, the Rust entry has no remote mode, and a new launcher needs
its own record and a check on that host. `child_result.validate_remote`
and its control stay for historical remote finals.

The Rust tests that ran Python as an oracle compare against fixtures in
`tools/final/tests/witness`, each generated once from the Python at
2b7323ae by the method it records, with placeholders for the root, final
directory, tools, the cross output's path digest and the recipe file's
name: the stage list and bulk roots, the required stages of 30 path
sets, each stage's resources, weight, environment and producers, the
selection receipt of every scope (the working tree's source digest
computed live by the retained `child_result.source`), the environment
digests, the diagnostics redaction (at a fixed-length path, whose length
the shared budget counts), the payload tree identity, and the shape of a
schema-1 ledger. Two finals the Python controller wrote are committed;
the Rust controller revalidates them wherever they lie and refuses them
failed, tampered, unreleased or rootless, as it does its own; a schema-1
ledger left by a dead owner is recovered. The producer rule is checked
against the Rust catalogue by both the Rust contract and the Python
stage protocol. `CONTROLS.txt` marks the 86 controls deleted with the
controller; each keeps its Rust test, and every Python control still
present is listed. A committed check refuses a current path to or
import of a deleted program outside `plans/`, `probes/`, the witness
fixtures and `CONTROLS.txt`, with planted cases, and requires every
script a stage runs to be tracked. `tools/python-files.txt` lists 480
files.

The complete Rust final passed in 48.3 minutes, against ADR 0283's 47.1
minutes. The service reported 1,793 memory.high events and no
memory.max or OOM events; the cause of the increased memory pressure
has not been attributed.
