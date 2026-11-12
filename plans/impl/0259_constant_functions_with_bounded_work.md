# Implementation plan 0259: constant functions in an execution session, with bounded work

Status: Proposed

Paired ADR: ../adr/0259_constant_functions_with_bounded_work.md.

1. Pin a4dcb2d8. Read the collection of constant functions in the query
   layer and its recursion bookkeeping, the bounded evaluator and what its
   node count charges today, the legacy call path that must stay
   unreachable from collecting mode, and the fixed-route inventory.
2. Before editing, send a note with measurements: units spent beside
   legacy instructions for the reference functions and the proposed
   default; time to exhaust it; stack bytes per evaluator level, the
   proposed live-call and nesting limits with their margin, and the
   deepest simple recursion they admit beside legacy; how work is
   separated from the node count if they overlap today; how a callee
   diagnostic keeps its origin. If any inventoried item would change,
   send that instead and stop.
3. Implement collection, the read-only lookup, calls with isolated
   frames, return, the work counter and the depth limits.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, the required stages, preflight and the commit checker. Bring
   the packet with the measurements, the class of each new row, the
   totals before and after, the selected stages and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
