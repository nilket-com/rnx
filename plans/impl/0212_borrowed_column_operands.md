# Implementation plan: ADR 0212

Introduce an operand location enum in the prepared plan. Input and constant nodes do not
consume scratch slots; only intermediate-producing nodes participate in slot allocation.
Resolve typed input slices per physical chunk/block and constants as scalars. Split owned
scratch safely around each destination so repeated shared operands are legal and writes
cannot alias reads. Final nodes write typed or packed owned output directly, including
identity/constant returns. Keep original-node order for fault replay and original inputs alive.

Extend the existing primitive boundary as needed to accept typed slices and scalar constants
without staging. Preserve optimized arithmetic paths, scalar controls, null masking and first
fault ordering. Add valid_rows to the prepared batch and completed result, populated by the
existing intersection/count pass; remove the host recount. Evaluate bitmap word borrowing
against existing block specializations before replacing them.

Start with format, Clippy, focused scalar/SIMD and host controls, short borrow-focused Miri,
and a small durable parallel instruction diagnostic. Add negative controls for hidden input
copies and invalid count propagation. Use the established durable pool for final 10M/100M
first/warm captures, with exact identities and retained failures. Review source and numerical
criteria before final acceptance; no publication until reviewed gates pass.

## Follow-ups

Record in BACKLOG: output zero-initialization removal requires its own ADR and audited
initialization invariant (spare capacity plus length publication), not this change. It costs
0.4375 Ir/row on numeric output in the existing 100M profiles.

Record in BACKLOG: generic mixed-mask SIMD arithmetic, including multiply3, is separate work.
Borrowing removes copies but does not remove the dominant scalar masked arithmetic cost.
