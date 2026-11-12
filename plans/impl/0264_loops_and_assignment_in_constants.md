# Implementation plan 0264: loops and assignment in constants

Status: Proposed

Paired ADR: ../adr/0264_loops_and_assignment_in_constants.md.

1. Pin a079db4a. Read the bounded evaluator's flow type, frame and
   locals, its block and call paths, the matcher of ADR 0262, how the
   loop and assignment forms arrive in the representation the evaluator
   reads, and how the run-time lowering gives them meaning.
2. Before editing, send a note: each form of the ADR at run time in both
   engines and as a legacy constant, with any disagreement; every loop
   and assignment form the language accepts and which are proposed; how
   break, continue and labels are carried without a native frame per
   iteration; what one iteration costs in work and where it is charged;
   what assignment to a part of a local needs from the value
   representation, and whether it is in or out; the work spent and the
   time for ten thousand and for the largest admitted number of empty
   iterations in both profiles; anything in the ADR the code makes
   wrong. If an inventoried item would change, send that instead and
   stop.
3. Implement the forms.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit checker.
   Bring the packet with the class of each new row, the totals before
   and after, the stack table, the work and time table, and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
