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
