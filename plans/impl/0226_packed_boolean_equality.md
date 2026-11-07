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
