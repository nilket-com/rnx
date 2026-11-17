# ADR 0294: The cross launcher runs in Rust

Status: Proposed

Base: published 152b0c07861f3c76695091ab6f7ea50fd44b3461 (ADR 0293).

## Problem

The `cross` stage still starts from `tools/verify.py`: it checks the pinned
engine sources, prepares a private Cargo graph, builds the Rust verifier
inside a `bwrap` namespace, runs it and its host check, and publishes the
receipt, with ADR 0292's census failure evidence. The verifier binds that
Python file by its digest as its launcher, and compares it between the
real checkout and a registration fixture.

## Decision

1. **Entry points.** The launcher is a stage mode of the controller's own
   executable: `rnx-final stage cross [--suite census|enrollment|all]
   [--fixtures-root DIR] [--registration-fixture DIR] [--jobs N] [--out
   DIR]`, refusing the historical engine overrides exactly as now.
   - Inside a final, the catalogue's `cross` stage runs it, bound by
     `stage::bound` and the producer protocol to the controller's context
     before work and at publication; its output must be the assigned
     `RNX_FINAL_CROSS_ROOT/payload`.
   - Inside a final, the assigned payload is required before the
     launcher touches the filesystem.
   - Standalone, the default output is under the ignored
     `target/rnx-verify`; an explicit `--out` outside the checkout stays
     supported, outside the Rune source tree; an in-tree `--out` must be
     in that ignored subtree. The suite and fixture options and the
     historical-engine refusals are kept.
2. **The same sequence and checks.** The prerequisites (`bwrap` and Cargo
   present, the `bwrap` version, the job count within the CPU affinity);
   the pin consistency check, written as `PIN-CONSISTENCY.json`; the private
   manifest and lock in the output; the metadata resolution inside the
   namespace and its check against the canonical engine; the verifier's
   test-binary build and its location from Cargo's artifact stream; the
   verifier run, then its host check whenever the verifier left a bound
   child, a controller failure included; the missing-child failure after a
   successful controller run; the original failure kept primary when the
   host check or cleanup also fails; the controller's result frozen before
   the launcher's receipt;
   the owned build target released, its cleanup recorded; and the
   terminal receipt, with ADR 0292's census case, reason and bounded
   evidence. The owned build uses the controller's `owned::Owned`; every
   child's whole process group is drained before the owned target is
   released, never only its leader reaped. The census receipt and its
   retention control run on this production path.
3. **Launcher binding.** The request names the launcher as the running
   executable, its path and SHA-256, taken from the process itself, never
   from a caller's argument, and a final checks it against the stage's
   context before work and at publication. The verifier checks the named
   file (no link, the recorded digest) and rechecks its identity at its
   existing after-binding boundary. The registration
   fixture invariant stays: the two roots must agree on the verifier
   sources, the test-only entry in `src/lib.rs`, and the checked-in Rust
   launcher wiring, the cross launcher's own source module under
   `tools/final/src`, which replaces the comparison of `verify.py`. An
   alternate root needs no separately built launcher, and a fixture whose
   `cross.rs` differs fails even when the launcher's digest is unchanged.
4. **Triggers and stages.** A change to `tools/final/src/cross.rs`
   requires the `cross` stage, as a change to `verify.py` did. The
   `pin-controls` and `engine-source-controls` stages are removed: their
   checks run as Rust tests through the existing final-suite route, and
   the selection witnesses record the two removed rows as a reviewed
   successor.
5. **Retired:** `tools/verify.py` and its test `tools/test_verify_pins.py`,
   and the pin checks `verify.py` used from `tools/engine_source.py`,
   ported with the launcher. Shared Python helpers with other live
   consumers are called out separately:
   - `tools/engine_source.py`'s remaining consumer is its own test,
     `tools/test_engine_source.py` (stage `engine-source-controls`): both
     retire here. The Rust port carries every case: external and duplicate
     sources, path escapes, an empty resolved graph, the required live lock
     graphs, and the historical-tree exclusion;
   - `tools/column/owned_build.py` keeps other live consumers and stays;
   - `tools/column/host_budget.py` is left with only its own test and is
     named for a later cleanup record, not changed here.

## Constraints

- The same checks, namespace, limits and resources; the shared budget,
  isolation and the 60-minute limit are unchanged.
- No new crate, no new Python file; the Python list only shrinks.

## Controls

- Witnesses from the published Python at 152b0c07, with controlled
  values: the pin checks' verdicts, the launcher's argument refusals, and
  its ordered commands, environment and receipt over intercepted runs,
  the controller-failure path included.
- The executable binding: a wrong binary or digest is refused by the
  verifier's binding.
- The registration fixture: a changed `cross.rs` in the fixture is
  refused, as a changed `verify.py` was, even with the launcher digest
  unchanged.
- The required-mode selection shows the new `cross.rs` trigger.
- The sequence, replacing the Python source-text assertion: the host
  check runs after the controller, the controller's result is preserved
  before the receipt, and cleanup is recorded, observed in the actual
  order, including a controller failure with a bound child.
- The 0292 census retention controls drive the Rust receipt.
- One complete Rust final.
