# Follow-ups

- ADR 0221: typed block execution for generic F64 chains, isolated from the
  shared executor and without adding callers to its operation helpers.

- Validation controllers: add a shared child-result helper that requires both
  exit code zero and a durable PASS receipt tied to the expected invocation and
  source identity. Missing, malformed, stale, or FAIL receipts must fail even
  when the process exits zero. Add controls for each disagreement. ADR 0215
  exposed local controllers returning zero after recording a failed assertion;
  its parent PASS was superseded by a FAIL correction before acceptance.

- Cleanup record: remove agent notification coupling and machine-specific paths from legacy column tooling (`rune/tools/xtask/column/ck1.py`, `cli_capture.py`, `measure.py`, `probe/build.rs`, and `fixtures/native/TABLE.json`); keep notifications in external launchers.

- F64 plan output initialization (8 Ir/row under the measurement environment): extend 0215's write-only ownership to the 0220/0221 drivers in a separate record, and explain why 0212 step-0 measured 0.4375.
