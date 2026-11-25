# Implementation plan 0335: The final's longest chain starts early

Status: Proposed

Paired ADR: ../adr/0335_the_finals_longest_chain_starts_early.md.

1. Boundary: published 5c853ce8 and the 0333 and 0334 final receipts.
   From the cache routes and both `SCHEDULE.json` files, confirm which
   recipes of `rune-suite` write `target/debug`, when the last such
   write happens, and whether stage teardown writes there. Record any
   deviation from the ADR's figures before editing.
2. Choose between releasing the hold and a separate directory, and
   state the expected finish with its assumptions before changing
   code.
3. Change the exclusion. Keep producer order, quiet for `fixed-route`
   and the wait evidence, and record the release in the receipt.
4. If a directory is added, measure its prepared size and candidate
   growth, then update the disk declaration, its catalogue control
   and both admissions.
5. Add the receipt check of the published schedule, the
   counterfactual and the planted violations. If the predicted finish
   exceeds 3,000 seconds, return with the bounding chain before
   signing.
6. Move the unsandboxed gap-contract children's result handoff to
   final-owned, memory-backed temporary storage with a stated bound
   and cleanup on every exit. Route the access sidecar through the
   child's temporary directory and publish it after the deadline.
   Add the pinning, stuck-child, sidecar and cleanup controls. Retain
   the attribution receipt's figures for Results.
7. fmt, Clippy, preflight, the controller controls and the targeted
   checks under the final's real recipe. Sign. Source review, one
   complete final, Results.
