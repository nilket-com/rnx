# Implementation plan: ADR 0223

Follow ADR 0220/0221's structure. Classify in the existing preparation pass,
setting the existing `special` flag plus a new kind. Add one arm to the
out-of-line `drivers::run` dispatcher. Give the new module its own cursors,
an operand Src trait, and explicit block kernels for:
- F64 arithmetic (its own copy, not 0221's);
- I64 and F64 comparisons packing 64 lanes per word;
- word-wise Boolean operations.

The output bitmap is written once per block after validity masking, with no
double append.

Reuse the 0220/0221 exact-vector, pin, edge and primary-loop shape tooling
against `e4e6f465`, with any fixed per-call deltas pinned. Add targeted Miri
(deterministic floats for the exact suite), native bitwise comparison against
the generic executor, the small diagnostic for target review, the 10M/100M
matrix, then the host, integration, generated and cross gates and the commit
checker. Committed tools stay standalone.

Represent the new driver kind with the existing classification and result:
`special && !boolean_plan && output == Bool`. The existing Boolean branch
remains first; the out-of-line dispatcher selects predicates by result type
before its F64 branch. `invoke` and shared storage layouts remain unchanged.

Keep the existing interval allocation and BLOCK-word scratch slots. In this
driver, F64 slots use initialized u64 bit storage, and Boolean slots use only
the first BLOCK/64 words as packed bits. This preserves memory accounting and
slot-reuse safety at the cost of unused Boolean scratch capacity; compacting
mixed scratch is outside this record. Exclude the destination from source
views before every node write, and mask tails before any packed read/write.
