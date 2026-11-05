# ADR 0206 implementation: rnx column-kernel follower

Status: Accepted

Follows the Rune decision at
https://github.com/nilket-com/rune/blob/fed878b5aea151a878462d71626c9a4d589a4273/plans/adr/0206_validity_aware_column_kernels.md.
No duplicate ADR is introduced in this repository.

Pin Rune to published fed878b5aea151a878462d71626c9a4d589a4273 and update
Cargo.lock. Replace the successful-column path's byte-at-a-time validity
popcount with safe u64 word popcounts and a bounded byte tail. The published
ColumnResult exposes validity, len and remaining_work, but no cached valid-row
count. Keep this host-only fix within the follower; do not infer a row count
from a fuel allowance. Preserve None => all rows valid, packed tail semantics,
path diagnostics, budgets, cancellation and Arrow ownership.

Validate counts against an independent bit reference for empty, partial-word,
full-word and multiword masks, all-valid/all-null/mixed patterns and tail
padding. Run formatting and Clippy prerequisites before expensive checks.
Capture paired production whole-map results against rnx 0205 using identical
fixtures and boundaries, with positive-presence controls, six numeric/Boolean
kernels and nullable/chunked layouts. Preserve CK2 ceilings and the exact
reviewed exception identity; do not generalize its scope to new binaries.
Attribute any failed comparison before changing code or rules.

Use durable identity-bound case receipts and resume only validated completed
cases. Complete required fresh acceptance and old-path controls, then submit
the final publication packet. Parallel Ir/Miri runner work follows this record.
