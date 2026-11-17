# Implementation plan 0289: rune-suite runs in Rust

Status: Proposed

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
