# Follow-ups

- After ADR 0212: remove numeric output zero-initialization only under a separate
  decision record. The current cost is 0.4375 instructions/row at 100M rows.
  Spare-capacity writes and publishing initialized length introduce an unsafe
  invariant that needs its own audit and failure/cancellation controls.
- After ADR 0212: accelerate generic mixed-mask expressions with validity-aware
  SIMD. The 49 instructions/row multiply3 mixed-mask path is mostly scalar
  arithmetic; eliminating copies alone cannot provide the larger SIMD gain.

- Column I64 Add/Sub with a constant still uses scalar execution in generic masked
  chains. Extend optimistic constant-operand arithmetic in a later record; retain
  invalid-lane fault suppression and bounded rescans (ADR 0213 follow-up).

- Validation controllers: add a shared child-result helper that requires both
  exit code zero and a durable PASS receipt tied to the expected invocation and
  source identity. Missing, malformed, stale, or FAIL receipts must fail even
  when the process exits zero. Add controls for each disagreement. ADR 0215
  exposed local controllers returning zero after recording a failed assertion;
  its parent PASS was superseded by a FAIL correction before acceptance.
