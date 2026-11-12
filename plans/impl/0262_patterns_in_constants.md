# Implementation plan 0262: patterns in constants

Status: Proposed

Paired ADR: ../adr/0262_patterns_in_constants.md.

1. Pin 294997ae. Read the bounded evaluator's let, if, block and call
   paths and its frame, the pattern representation it receives, how the
   run-time lowering matches the same patterns, and the stack tests of
   ADR 0259.
2. Before editing, send a note: each form of the ADR at run time in both
   engines and as a legacy constant, with any disagreement; every
   pattern kind the language accepts and which are proposed; how one
   matcher serves the four forms; what is moved and what is copied when
   a part is bound, with the charge; frame size of the matcher per level
   in both profiles and the effect on the limits; anything in the ADR
   the code makes wrong. If an inventoried item would change, send that
   instead and stop.
3. Implement the matcher and its four callers.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit checker.
   Bring the packet with the class of each new row, the totals before
   and after, the stack table, and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
