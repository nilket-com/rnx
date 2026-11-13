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

- ADR 0268 removes the repeated session collection walk when no unleased
  program may have escaped, and stops when all candidate programs are found.
  Direct function/closure bindings are checked before unrelated data. A live
  candidate buried behind that data, or a dead candidate being proved dead,
  can still require a full walk. Investigate reachability metadata separately,
  without weakening ownership, cycle handling or idle release guarantees.

- ADR 0269, from the ADR 0268 measurement: an input referencing a held 5M-integer
  vector still spends about 140 ms in two formatting-admission traversals
  (unoptimized engine-dev), even though collection makes no graph walk.
  ProgramDomain::start and start_cross both prepare the entry. Investigate
  removing redundant admission / proving unchanged formatting authority,
  preserving all entry validation and fixed-route protections; no such
  admission or executor edit is included in 0268.
