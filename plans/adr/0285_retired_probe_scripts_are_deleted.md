# ADR 0285: Retired probe scripts are deleted

Status: Implemented

Base: published 107e2bde2cad0888d82e4763e60dbd55f22ebb65 (ADR 0284).

## Problem

`tools/python-files.txt` marks 151 tracked Python files `delete`: scripts
under `probes/` that ran the experiments of finished records, 16,048 lines
in 49 probe directories. Nothing current runs or imports them, but they
are a third of the Python left, and each could still be edited or
imported again. Five of them are named by current files, two of those by
model tests that write output only for them to read.

## Decision

1. The 151 paths listed `delete` in `tools/python-files.txt` at 107e2bde
   are deleted, and only those. The probe directories' other files
   (frozen inputs, Rune corpora, fetch scripts) stay, byte for byte, as
   do the data-consumer registrations, the producer registrations and
   the native-contract tests.
2. The five current references:
   - `tools/polars-gen/README.md` describes `probes/0093/census.py` as
     history, naming it at 107e2bde.
   - The eight `cite` strings in
     `tools/polars-gen/releases/0.55.2-joins.toml` that name
     `probes/0118/census.py` stay: they cite a record's source, they are
     not recipes.
   - `adapters/candle/tests/interchange.rs` keeps its committed fixtures
     (`tests/data/0143`) and tests, and names their generator,
     `probes/0143/fixtures.py` (NumPy and Python's `zipfile`), at 107e2bde
     revision.
   - `adapters/candle/tests/nli_model.rs` and `generate_model.rs` lose
     the `RNX_NLI_GATE_OUT` and `RNX_GEN_GATE_OUT` requirements and the
     JSON and raw-f32 output written only for `probes/0148/hf_gate.py`
     and `probes/0149/hf_gate.py`. Their computations and assertions
     stay: the generation test still requires finite logits and the same
     ids from `chat_strs` as from the full-logit path; the NLI test still
     scores the frozen pairs batched and alone. The ignored entry points,
     model requirements, frozen inputs and calibration tests stay. The
     comparisons with the external sentence-transformers and
     transformers implementations that records 0148 and 0149 accepted
     are history; their Python comparators are retired, and nothing here
     replaces that external reference.
3. A committed list of the 151 deleted paths is the tombstone. A
   committed check requires none of them to be tracked, and refuses a
   current path to, import of or command running one, outside `plans/`,
   `probes/`, the eight reviewed `cite` strings and generator provenance
   that names a revision.
4. `tools/python-files.txt` loses those 151 entries and nothing else.

## Constraints

- No other file is deleted or changed in meaning. No new Python file. No
  new runtime behavior, benchmark or model run.

## Controls

- The tracked deletion equals the 151 listed paths; the Python list
  check passes with 329 files.
- The retained files of every touched probe directory are byte-identical
  to 107e2bde.
- The reference check, with planted cases: a path in a current file, an
  import, a command; a citation without a revision; and the allowed
  history.
- The catalogue's script-existence check, the Candle model tests'
  compilation and their non-ignored tests, and one complete Rust final.

## Out of scope

The stage protocol (`child_result` and its helpers) and the stage
programs, which follow in their own records; the probe directories'
data, which the data-consumer registry governs.
