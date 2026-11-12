# Implementation plan 0257: constant indexing, blocks and if in an execution session

Status: Proposed

Paired ADR: ../adr/0257_constant_indexing_blocks_and_if.md.

1. Pin 84560a7c. Read the bounded constant evaluator and its limits in
   the execution core, the constant query path and its guards, the
   materialization in lowering, the legacy evaluator for the three forms,
   and the fixed-route inventory.
2. The design was reviewed before this plan was committed: evaluator
   state with locals keyed by variable identity and restored on every
   exit, the measured legacy index kinds and diagnostics, first true
   branch only, one shared budget charged before allocation, inline
   constant blocks through the same evaluator, no materialization change.
   Send a note only if the code forces a departure from it or an
   inventoried item would change.
3. Implement the three forms.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, the required stages, preflight and the commit checker. Bring
   the packet with the before and after class totals, the selected stages
   and the per-stage durations. The candidate is frozen once the packet is
   sent.

Results are added here by the implementer.
