# Implementation plan 0256: if without else evaluates to unit in the execution engine

Status: Implemented

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

## Implementation and focused evidence

Ordinary and pattern-conditional lowering now write unit after a taken
block lowered with Mode::Discard when there is no fallback and the value
is requested. The block still evaluates effects and errors, without an
unused tail store; no instruction is added when its value is unused.
Final-else behavior is unchanged. Neither inventoried source file nor the
fixed-route policy is edited.

The pre-change audit compared nineteen forms: differences only for taken
ordinary if, taken else-if without final else, and matching if-let. While,
for, break without value, while-let, empty/semicolon blocks agree on unit;
a loop with break value and an if with final else agree on their values.

A pre-change compiler measurement fixed statement-position instruction
counts at 5, 8 and 18 for ordinary if, else-if and if-let. The committed
control requires those exact counts after the change, and pins value-position
counts at 6, 10 and 19 after discard-then-unit lowering. Core tests cover
unit results and single effects, including heap-valued branch results and
complete release. Session controls cover seven cases, effects once, errors
with retained mutations, and unchanged final-else results.

Seven new conditional parity rows are measured Equal; Equal rises from 166
to 173, and every existing row and class is unchanged. Constants remain
out of scope, with all six admission refusals unchanged.

Compatibility correction: source previously returning the taken branch's
value from a no-else if now returns unit, with no compatibility switch.

No new Miri run is proposed: the change emits the existing unit literal
through the existing destination-write/release path, already used by the
no-match branch and ordinary assignments. There is no new store,
materialization or unsafe operation. New native controls additionally
exercise discarding heap-valued tails and verify zero live entries.
Required final stages include Rune tests/clippy, fixed-route bands and
cross; their durations are recorded in the publication packet.

Authorized cleanup from the boundary inventory: removed reproducible Cargo
outputs target/0254/scratch/target (842217323 apparent bytes) and
target/0255/probe-build (1208690732 apparent bytes). All logs, receipts and
scratch sources were retained.
