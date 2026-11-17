# Implementation plan 0285: Retired probe scripts are deleted

Status: Proposed

Paired ADR: ../adr/0285_retired_probe_scripts_are_deleted.md.

1. Pin 107e2bde. Commit the list of the 151 paths that
   `tools/python-files.txt` marks `delete`, as the tombstone the
   reference check reads.
2. Delete those paths and their entries in `tools/python-files.txt`.
   Show that every other file in each touched probe directory is
   byte-identical to 107e2bde.
3. Reword the 0093 census passage in `tools/polars-gen/README.md` as
   history, and name the 0143 generator, with its NumPy and `zipfile`
   dependencies, at 107e2bde in `adapters/candle/tests/interchange.rs`.
4. Remove the gate writers from `adapters/candle/tests/nli_model.rs` and
   `generate_model.rs`, keeping their computations, assertions, ignored
   entry points and frozen inputs; their module documentation says the
   external comparisons of records 0148 and 0149 are history.
5. The reference check over the tombstone, with planted cases; the
   Candle tests compile and their non-ignored tests pass.
6. Run the affected checks in a final-like environment before signing;
   sign; one complete Rust final through `tools/final/final.sh`; then
   preflight and the commit checker. Results state what the code does
   and the final's duration.
