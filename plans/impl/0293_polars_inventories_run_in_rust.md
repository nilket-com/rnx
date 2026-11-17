# Implementation plan 0293: Polars inventories run in Rust

Status: Implemented

Paired ADR: ../adr/0293_polars_inventories_run_in_rust.md.

1. Pin fe95b78e and generate the witnesses from the published Python,
   with controlled values, before any change.
2. The `inventories` stage mode, its validator and receipt, over an
   interceptable runner, using the shared `stage::build_jobs`.
3. Catalogue, scheduler, producer lists, README; retire both Python files
   with tombstones.
4. The Rust suites, the Python discovery, preflight and the commit
   checker; sign; source review; one complete Rust final; Results.

## Results

`polars-inventory-preflight` and `polars-inventories` run `rnx-final stage
inventories [--preflight] --out DIR`, a producer the child contract
admits; the scheduler maps it to the same lanes, weights and
`polars-suite` dependency. `stage::inventories` runs the program's
sequence through one `InventoryRun` seam, named steps and git queries
alike: the head and a clean `git status --porcelain`, the five lock
digests, the nightly check (alone in `--preflight`), the fetches in the
base environment (the stage's own, with the shared `stage::build_jobs`
width, no target directory, any inherited offline setting kept), the
extractor build in its own target, then per configuration its
documentation, a check that its crates are a list of documented names
(before the extractor runs), extraction and `stage::validate_inventory`,
which requires every field (a missing one is a `KeyError`) in Python's
short-circuit order and compares feature lists as multisets of names,
refusing anything but a list of strings (stricter than Python, which
would sort a string's characters or a lone non-string member), the generated-bindings check with exactly one `BUCKETS` and
one `RELEASE_FILE`, then the lock rehash and the head and tree rechecks.
A step's check row is recorded before its failure. The witnessed failure
shapes keep Python's reprs and `TRACEBACK.txt` last lines: the
assertions, a failing step's `(name, code)` tuple, and a failed git query's
`CalledProcessError`; other failures give an equivalent Rust diagnostic.
The live queries go through the controller's lock-free git with the
stage's environment, strict text decoding, and stderr left to the stage's
log, which is set in code and has no direct control. A process that
cannot start is named by its program and the OS error, never by its
environment, in this stage and every stage mode that uses `stage::wait`.

Retired: `tools/column/inventories.py` and `tools/column/test_inventories.py`,
whose validator control is now `programs.rs`'s; the Python list holds 307
files. The witness at fe95b78e covers the validator's twenty verdicts (missing,
null and mixed-type fields by Python's exception class) and nine runs (preflight,
pass, a failing extraction, a lock changed while every command succeeds,
a dirty tree, a changed head, an inherited offline setting, a scalar
crates refused before the extractor, a failing git query); the catalogue and selection fixtures record the two
replaced invocations as a reviewed successor.

The complete Rust final passed in 44.2 minutes, with 1555 memory.high
events and no memory.max or OOM events. All 60 Rust stage contexts, both
inventory stages included, carry the controller digest the manifest
records; polars-inventories ran its full mode for real, preparing the
three configurations' inventories in 14 steps with its five locks
unchanged.
