# Follow-ups

- Gate policy: every record touching rnx `src/` must include `cross`; enforce this in the committed gate selector. ADR 0224 omitted it and left the time-module census pin stale.

- ADR 0221: typed block execution for generic F64 chains, isolated from the
  shared executor and without adding callers to its operation helpers.

- Validation controllers: add a shared child-result helper that requires both
  exit code zero and a durable PASS receipt tied to the expected invocation and
  source identity. Missing, malformed, stale, or FAIL receipts must fail even
  when the process exits zero. Add controls for each disagreement. ADR 0215
  exposed local controllers returning zero after recording a failed assertion;
  its parent PASS was superseded by a FAIL correction before acceptance.

- Tool-test discovery: rename the duplicate `test_protocol.py` modules or isolate
  their imports so column-suite discovery cannot select the measure-suite module.
  ADR 0231 preserved the discovery failure and verified every column test file
  in a separate process; fix the aggregate discovery command in a tooling record.

- Boolean plan output ownership remains initialized; consider separately from ADR 0222's numeric output owner.

- Predicate-driver I64 compare packing with SSE4.1/AVX2 target features (SSE2 pcmpeqd pairing lost to bounded scalar in ADR 0223), plus AVX2/wider F64 comparison packing; measure against the selected per-type shapes.
