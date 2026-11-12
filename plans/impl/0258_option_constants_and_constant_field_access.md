# Implementation plan 0258: Option constants and constant field access in an execution session

Status: Proposed

Paired ADR: ../adr/0258_option_constants_and_constant_field_access.md.

1. Pin dffb48c1. Read the call and field-access arms of the bounded
   evaluator, how a resolved constructor is identified in HIR, the Option
   form of the constant value representation, the constant lowering and
   the variant instruction, and the fixed-route inventory.
2. Before editing, send a short note: how the two constructors are
   identified, the validation in lowering, the measured legacy
   diagnostics and locations for the field forms, and the Miri tests. If
   any inventoried item would change, send that instead and stop.
3. Implement the constructors, the materialization and the two field
   forms.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, the required stages, preflight and the commit checker. Bring
   the packet with the class of each new row, the totals before and
   after, the selected stages and the per-stage durations. The candidate
   is frozen once the packet is sent.

Results are added here by the implementer.
