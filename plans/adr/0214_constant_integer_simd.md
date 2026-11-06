# ADR 0214: Optimistic constant integer arithmetic in masked chains

Status: Accepted

Extend ADR 0213's generic masked I64 nodes to checked SIMD Add/Sub with a
constant on either side. Reuse per-node overflow flags, one masked scalar
rescan per flagged node/block, final invalid-output zeroing and the MASKED
const-generic split. Preserve row-first/PC-first fault replay, exact fuel,
cancellation and initialized storage. Do not materialize constant columns.
Handle MIN/MAX constants without overflowing negation, including constant-minus-
column. Keep portable execution and truthful path reporting.

The standalone specialized x+c path and unmasked generic paths remain unchanged.
No output ownership changes, dependencies, VM hooks or F64/Bool expansion.
Output initialization ownership remains a separate follow-up.

Test primitive equivalence, both operand orders, extreme constants and input
values, tails/offsets, all validity patterns, null-only and valid overflow,
competing row/PC faults and later-node overflow. Compare Auto/Portable/reference
results, validity, faults and work. Reuse the ADR 0213 rescan protocol and prove
that adversarial null payloads actually trigger its rescan; report benign and
adversarial costs separately.

Baseline is published ADR 0213 at 49eb4453cf333e926357ecd71448175449d5a07c.
Sequence: signed plan, implementation review, prerequisites and focused tests,
targeted Miri, small paired diagnostic, then reviewed targets and durable parallel
10M/100M captures. Protected flat/sliced and unrelated kernels require zero
program growth apart from separately attributed, reviewed, pair-pinned fixed
preparation constants with zero slope. Retain existing ceilings and library
accounting rules. Do not infer numerical targets from different expressions.
Run proportional host integration/generated/cross controls; old paths only if
source scope requires them, with explicit disclosure if not re-measured.
