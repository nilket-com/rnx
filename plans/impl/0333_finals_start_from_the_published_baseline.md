# Implementation plan 0333: Finals start from the published baseline

Status: Proposed

Paired ADR: ../adr/0333_finals_start_from_the_published_baseline.md.

1. Boundary: published ee52b405 and the 0332 schedules and logs. Take
   the cache inventory in Control 1, including the bootstrap and
   stage-owned caches, and return any gap before editing.
2. Separate the canonical final lanes from pre-sign and branch state.
   Give the final its own dependency home and private invocation
   caches for stage children, and stage anchor-cache writes.
3. Add the baseline checkpoint and its preparation before the
   bootstrap: continuous exclusive ownership, the in-use mark before
   the first canonical write, restore or rebuild from baseline inputs
   alone, refusal of an unprovable lane, and its own report.
4. Keep the end-to-end clock from admission. Bind the baseline
   identity, preparation outcome and monotonic boundaries into the
   receipt, report the three durations, and record a time-only failure
   as the final.
5. Promote only a verified terminal publication-eligible PASS, by
   build inputs, including a docs-only successor.
6. Add the planted routes, the write-boundary control and the real
   preparation measurements.
7. Fix the competing-client scheduling fixture and add its regression
   control and loop.
8. Refuse a disk shortfall before preparation and before the clock,
   with its planted control. Report the 16b0a07a final in Results.
9. fmt, Clippy, preflight and the targeted checks in separate state.
   Sign. Source review, one complete final, Results.
