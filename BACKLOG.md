# Follow-ups

- ADR 0221: typed block execution for generic F64 chains, isolated from the
  shared executor and without adding callers to its operation helpers.

- Boolean plan output ownership remains initialized; consider separately from ADR 0222's numeric output owner.

- Predicate-driver I64 compare packing with SSE4.1/AVX2 target features (SSE2 pcmpeqd pairing lost to bounded scalar in ADR 0223), plus AVX2/wider F64 comparison packing; measure against the selected per-type shapes.

- ADR 0267 legacy-runtime removal dependency: the execution Store's host-value
  payload owns a `runtime::Value`, the legacy runtime's native `Any` object.
  Existing registered host handlers receive it through the existing typed host
  bridge. Removing that runtime must replace this native ownership/borrow and
  handler ABI as well; the execution engine does not inspect the native object.

- ADR 0267 cost attribution: ProgramDomain::collect_programs traces every
  live data child after each session input, even a scalar query on a large
  vector. At 5M elements, 100 queries took about 57.4 s already on 093e7caf
  in engine-dev. Investigate program-reachability metadata or collection
  policy without weakening whole-store ownership and idle release guarantees.
