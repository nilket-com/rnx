# Implementation plan 0284: The Rust controller is the only final

Status: Proposed

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
