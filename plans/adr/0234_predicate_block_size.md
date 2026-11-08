# ADR 0234: Predicate-driver block size

Status: Accepted

The read-only sweep on published `bc4a2065` does not justify changing the
shared 1024-row block. It does identify a predicate-specific opportunity.
At 4096 rows, flat I64 equality warm medians were 12.1% faster at 10M and
11.2% faster at 100M. Flat mixed predicates were 8.1% faster at 100M with
separated observed process ranges, but 1.3% slower at 10M with overlapping
ranges. At 8192, I64 equality was only 0.7% faster at 10M and 13.9% faster
at 100M. These are five-process measurements, not confidence intervals.
The sweep changed the global constant, including validity construction;
it is supporting evidence, not acceptance evidence for this narrower change.

Decision: use `PREDICATE_BLOCK = 4096` only for predicate execution
and its scratch-slot stride. Keep shared `BLOCK = 1024`, the Boolean driver,
F64 output drivers, generic executor, primitives, and validity construction
unchanged. Choose 4096 over 8192 for the stronger 10M I64 result and the
smaller cancellation and storage costs. Do not introduce a runtime block-size
option or additional executor instantiations.

The predicate driver already shortens a block at the earliest input chunk
boundary. Its packed word arrays become 64 words each (1536 bytes total,
up from 384). Each predicate scratch slot becomes 4096 u64 elements
(32 KiB, up from 8 KiB), including slots holding packed Boolean intermediates.
Preserve the current slot layout; scratch compaction is outside this record.
Allocate and charge the larger capacity with checked arithmetic and the
existing memory allowance. A tight allowance can therefore refuse a predicate
plan that previously fit; never undercount it or silently bypass the limit.
No persistent buffer retention is introduced.

Cancellation: at most 4096 rows, shortened by chunk boundaries, execute
between predicate polling points, versus 1024 before. Every node for those
rows can run before the next poll. Keep the final predicate poll. This is a
row bound, not a wall-time or end-to-end latency guarantee; allocation/setup
is outside it. Validity construction retains its existing 1024-row bound.
Fuel charging remains per valid row and independent of execution block size.

Acceptance against published `bc4a2065`:
- Bitwise output, validity, tail bits, valid-row count, remaining work, and
  Path agree for I64/F64 comparisons and mixed Boolean/F64-arithmetic
  predicates, including forced scalar, offsets, empty chunks, and boundaries.
- Larger scratch slots obey the memory allowance and remain correctly
  separated when the interval allocator reuses slots. Cancellation returns
  no partially published result and respects the stated row bound.
- The instance guard and resolved identity protect unchanged routines.
  Unchanged routes have exact program vectors except explicitly reviewed
  fixed per-call setup pins; no new per-row or per-block regression is allowed.
  Existing accepted baseline costs do not authorize additional growth.
- Predicate vectors follow reviewed call-count formulas for 4096-row blocks,
  including actual chunk shortening and tails. Compare them with both 4096
  and 8192 sweep vectors; do not demand whole-map identity to a global sweep
  that also changed validity construction. Preparation and unchanged validity
  costs remain separately visible. Native and forced-scalar paths are measured.
- Changed predicates have no total-Ir growth at 10M/100M; report first and
  warm calls separately. Scope any system-library allowances explicitly.
- Counterbalanced timing covers I64 and mixed/F64 predicate cells at both
  10M and 100M, with six fresh process rounds and exactly three AB and
  three BA orders per cell. Require a demonstrated benefit in the motivating 100M cells
  without a reproducible regression elsewhere. Review exact timing targets
  after the small diagnostic and before large capture; no post-result waiver.
- Core tests, hardware parity, scoped Miri, host/integration/generated/cross,
  gate-policy finalization, and the commit checker pass.

Stop on unexpected unchanged-path slope or identity/instance drift. Attribute
it and bring a decision before another structural attempt or rule exception.
Do not absorb a global block change, Boolean optimization, or primitive rewrite.
