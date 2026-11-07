# Implementation plan: ADR 0226

Extend `boolean.rs` with word kernels for XOR, XNOR and NE, and extend the
pure-Boolean classification in `plan.rs` to admit Boolean `^`, `==` and
`!=`. Reuse the 0220 traversal, output-once publication and validity
masking.

Add controls to the existing Boolean test suite, comparing against a
separately prepared generic execution. Run targeted Miri on the new
kernels. Reuse the 0220/0223 exact-vector and pin tooling against
`b177e963`, adding `boolne` to the fixtures. Then run the small diagnostic
for target review, the 10M/100M matrix, and the host, integration,
generated and cross gates plus the commit checker. Publish the bench report
afterwards.

Reviewed small-diagnostic policy: non-Boolean paths use exact function vectors
plus fixed preparation pins, enforced against the measured binary pair. The
changed Boolean driver uses raw whole-map ceilings: AND/OR/EQ/NE/XOR at 2
Ir/row flat and 4 nullable; chains at 2.5 flat and 5 nullable. New shapes must
also use at most 10% of baseline instructions. Apply at 10M and 100M rows,
first and warm, in all layouts. Report the measured AND/OR/chain increases
of 6/3/13 instructions per 1024-row block explicitly.

Cross-check repair: this record refreshes the cross census fixture left stale
by ADR 0224, whose gate set omitted cross. The committed refresh tool validates
explicit before/after revisions and reviewed paths; only the live src/time.rs
pin changes. Historical provenance remains untouched. Rerun cross only; reuse
the other passed stages with a tooling/documentation-only diff proof.

The recovered cross run exposed a second stale ADR 0224 witness: registry_rows
remained 62 after adding time::monotonic_ns. The checked helper also verifies
the exact before/after registry arrays and permits only the new item plus the
source-proven monotonic_ms documentation wording. The count becomes 63;
historical records remain unchanged. Both failed cross receipts are retained.
