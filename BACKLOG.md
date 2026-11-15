# Follow-ups

- ADR 0277: index Object lookup, which still scans the field vector while
  stock uses a hash index. Measure present/absent get and construction
  (insert loop and json::parse import) at 8, 1,000 and 100,000 fields,
  unoptimized and optimized. Store::record duplicate-key validation and
  insertion also scan preceding keys; if measured construction is quadratic,
  address it as the next record before other work. Preserve ownership,
  field order, moves, memory charges and before-effect refusals.

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

- ADR 0269 residual: once a Store has attempted a nominal type denied by an
  incoming formatting certificate, later admissions use the exact argument
  walk even after that value dies. Session compilation refuses known
  construction of such a type; at the public embedding boundary the measured
  5M-integer preparation still takes about 71 ms in engine-dev after the
  denied type is discarded. Investigate a narrower proof without weakening
  admission, ownership, allocation failure or fixed-route protection.

- ADR 0270: await inside user functions belongs with the async-function and
  future-value contract. Ordinary functions currently retain legacy's
  `.await`-outside-async diagnostic; direct waits cover the input expression.

- ADR 0270 follow-up: selective inspection of host object inputs. A snapshot
  reads within the snapshotted tree (scalars, strings, vectors, tuples, objects,
  Option) down to its depth/frontier bounds. Result, struct, enum, closure,
  function and host values, and values beyond a bound, are unentered diagnostic
  leaves. Moved values within the part read are access errors at the call,
  including ignored fields; moved values hidden under a leaf are not inspected
  and cannot be obtained by the handler. Preserve this boundary when designing
  selective inspection.
