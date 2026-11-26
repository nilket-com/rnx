# Implementation plan 0342: A constant is checked before it is evaluated

Status: Proposed

Paired ADR: ../adr/0342_a_constant_is_checked_before_it_is_evaluated.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem with the automatic route and with the legacy
   engine forced, in both native modes. Inventory the checks the
   old core's assembly makes for a constant's code. For each, say
   whether HIR lowering, the new engine's evaluation or its
   ordinary lowering already makes it, or why it isn't a check of
   the source. Measure which code the old core visits, the order
   in which it reports several errors, and its outcome for
   constants that are never used.
2. Add the pass as Decisions 1 to 5 state and call it from the new
   engine's constant path. Add the witnesses of Controls 1 to 4.
3. Add the selection, order, inventory and function-body controls
   and both planted changes. Decision 6 applies throughout: decide
   its two listed differences yourself and record them.
4. Replay the public admission corpus against the published base.
   Send the list for review before signing if any source changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.
