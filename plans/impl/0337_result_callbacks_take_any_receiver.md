# Implementation plan 0337: Result callbacks take any receiver

Status: Proposed

Paired ADR: ../adr/0337_result_callbacks_take_any_receiver.md.

1. Boundary: the published base. Replay the 14 ownership
   observations and the shapes of Control 2 on both engines with
   fallback disabled. Record what refuses, where, and what the old
   core does. Record the image's relative relocation count.
2. Fix the gate rule first. Measure the unit with zero, one, four
   and five planted relocations at both sizes, and 12 instructions
   of fixed setup outside the loader. Name the linker output and the
   profiled function that carry the cost. ADR 0336's finding of 12
   instructions is a lead, not this measurement. Report the unit and
   the outcomes of Control 7 for review. Score no product code of
   this record before that review.
3. Admit the forms of Decision 1 with the order, checks and values
   of Decisions 2 and 3. Report before going on if a common receiver,
   such as a parameter or the result of a script call, can't be
   admitted under Decision 5 within what Decision 11 allows.
4. Add the parity table, the ownership outcomes, the dispatch
   controls and the refusal messages. Update the contract rows and
   the affected inventory witnesses.
5. Admit `is_readable` as Decision 6 states. Replay ADR 0336's six
   cases and the deferred `is_readable` case from full source in
   both native modes, and confirm the other 19 keep their boundary
   refusals.
6. Score the file gate and both project screens before signing. For
   a comparison that uses the alternate, report the raw increase, the
   loader's part and the remainder.
7. fmt, Clippy, preflight and the targeted checks in separate state.
   Sign. Source review, one complete final, Results.
