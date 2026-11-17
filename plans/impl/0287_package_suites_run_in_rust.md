# Implementation plan 0287: Package suites run in Rust

Status: Proposed

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
