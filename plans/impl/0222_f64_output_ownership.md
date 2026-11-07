# Implementation plan: ADR 0222

Read `NumericOutput` and its ADR 0215 flat users, and the `f64_plan`
publication. Add the minimal safe writer methods on `NumericOutput<f64>`.
In batch preparation, allocate a reserved but uninitialized `Vec<f64>`
(length 0, capacity len) only for F64-classified plans. The driver wraps it,
writes each block in order, and calls `finish` before building the result. A
panic or early return must leave the Vec at length zero.

Add these controls:
- every validity word kind (all-valid, all-null, mixed) at tails and chunk
  edges;
- cancellation mid-batch, and a memory refusal;
- an exact comparison with the generic executor;
- Miri on the writer methods and the drop paths.

Reuse the 0221 exact-vector and pin tooling against `7c51bf60`, with any
new fixed per-call deltas pinned. Run the small diagnostic, then 10M/100M
first/warm, then a quiet serial timing pass at 100M (four layouts, 0221
binary versus candidate; descriptive). Then run the host, integration,
generated and cross gates and the commit checker. Remove the output-fill
BACKLOG line.

The existing `ColumnValues::F64(Vec<f64>)` carries the reserved allocation
with length zero inside the private `PreparedBatch`. Its logical row count
is already stored separately, and owned-byte accounting uses capacity.
Only `special && !boolean_plan` preparation selects this representation.
The F64 driver consumes the Vec into `NumericOutput`; no shared field or
variant is added. The generic and Boolean drivers never receive this state.
