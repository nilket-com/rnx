# Implementation plan 0256: if without else evaluates to unit in the execution engine

Status: Proposed

Paired ADR: ../adr/0256_if_without_else_is_unit.md.

1. Pin 485babed. Read the conditional and pattern-conditional lowering in
   the execution core, the legacy assembly of the same forms, and the
   fixed-route inventory.
2. Run the audit of unit-valued forms in both engines. Send the list
   before changing the pinned table. If it shows a difference outside the
   if forms, send it and continue with the if forms only.
3. Change the lowering so that the value is unit on every path when there
   is no else, with no extra instruction when the value is unused.
4. Add the rows and controls of the ADR and regenerate the pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the audit, the class totals before and after, the selected
   stages and the per-stage durations. The candidate is frozen once the
   packet is sent.

Results are added here by the implementer.
