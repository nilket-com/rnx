# Implementation plan 0285: Retired probe scripts are deleted

Status: Implemented

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

## Results

The 151 probe scripts that `tools/python-files.txt` marked `delete` are
deleted, and their entries with them; the list holds 329 files. The 247
other files in their 49 probe directories are byte-identical to
107e2bde. `tools/final/tests/retired/probes-0285.txt` lists the 151
paths, and `tools/final/tests/deleted.rs` requires none of them to be
tracked and no current file to name one, by its path or as a dotted
module, outside `plans/`, `probes/` and that list, except four reviewed
lines, each in its file and with its exact count: the 0093 census
history, the 0143 generator's provenance and record 0118's two citation
strings (eight occurrences); planted cases cover dotted imports, a
changed or added citation, revision-qualified comments and prose
elsewhere, and commands in prose, tilde fences and indented blocks.

`tools/polars-gen/README.md` describes record 0093's census as history,
and `adapters/candle/tests/interchange.rs` names the generator of its
fixtures at 107e2bde. The record 0148 and 0149 model tests no longer
write output for the retired Hugging Face comparators: the generation
test still requires every logit to be finite and `chat_strs` to give the
full-logit path's ids, and the NLI test still scores the frozen pairs
batched and alone. Their external comparisons are history, and nothing
here replaces them.

The complete Rust final passed in 46.6 minutes, against ADR 0284's 48.3
minutes, with 1,775 memory.high events and no memory.max or OOM events.
An earlier attempt on the same candidate stopped at disk admission before
its first stage.
