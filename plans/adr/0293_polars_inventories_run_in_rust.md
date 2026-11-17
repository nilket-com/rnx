# ADR 0293: Polars inventories run in Rust

Status: Proposed

Base: published fe95b78e3a7ae154344063645227df058a922bda (ADR 0292).

## Problem

Two of the final's stages still run the Python stage program
`tools/column/inventories.py`: `polars-inventory-preflight`, which checks
that the pinned nightly toolchain is installed, and `polars-inventories`,
which prepares the pinned Polars inventories from source and checks the
generated bindings against them. The program only runs other programs and
checks what they leave behind.

## Decision

1. Both stages run as a stage mode of the controller's own executable,
   bound by `stage::bound` as the others are: `rnx-final stage
   inventories [--preflight] --out DIR`. It writes the receipt the Python
   program wrote.
2. It keeps the program's contract exactly:
   - a clean tree and its head, read before and checked after;
   - the digests of the three configuration locks and the two tool
     locks, unchanged by the run;
   - the nightly preflight, which alone is the `--preflight` stage;
   - the tool and configuration fetches, the extractor build in its own
     target, then for each configuration its documentation, its
     extraction and the inventory's validation against its pins;
   - the generated-bindings check, with the bucket and release constants
     read from `adapters/polars/tests/generated.rs` as before;
   - the same commands, environment, limits and order: the two fetch
     groups keep the base environment (the stage's own, with the shared
     build width, no inherited target directory, and any offline setting
     it inherited); every other step sets offline; only the extractor
     build sets its own target directory; a step's check row is recorded
     before its failure is refused;
   - fail-closed reading of the generator's constants (exactly one
     `BUCKETS` and one `RELEASE_FILE`), and feature comparison as
     multisets, duplicates counted.
   Preflight checks the head and a clean tree, records the initial lock
   digests and runs only the nightly check; the trailing lock rehash and
   head and tree checks belong to the full mode, which refuses a changed
   lock even when every command succeeded.
   Git queries and named steps go through one interceptable runner: the
   queries with the stage's environment and directory and strict text
   decoding, the clean-tree check from `git status --porcelain`.
3. Retired: `tools/column/inventories.py` and its test
   `tools/column/test_inventories.py`, whose control becomes the Rust
   validator's; the scheduler keeps the same resources, and the producer
   lists, the Python list and the column README follow, with tombstones.

## Constraints

- The same commands, checks, limits and resources; the shared budget,
  isolation and the 60-minute limit are unchanged.
- No new crate, no new Python file; the Python list only shrinks.

## Controls

- Witnesses from the published Python at fe95b78e, with controlled values:
  the inventory validator's verdicts over planted pins and inventories,
  and both modes' ordered commands and queries, environment, logs and
  receipt over intercepted runs: a failing step, a changed lock with
  every command succeeding, a dirty checkout and a head that changes.
- Both stage names, their scheduler lanes and weights, and `polars-suite`'s
  dependency on `polars-inventories` are unchanged; the catalogue and
  selection witnesses change only in the two replaced invocations.
- The Python discovery (`tools/column`) and the Rust suites pass.
- One complete Rust final.
