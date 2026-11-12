# Implementation plan 0263: clock functions are executed and compared by property

Status: Proposed

Paired ADR: ../adr/0263_clocks_compared_by_property.md.

1. Pin 085850ee. Read where the host rows of the pinned table are
   produced and classified, the clock predicates in the verification
   code and who calls them, the three time functions and what each
   documents, and how a session row with two inputs is expressed.
2. Before editing, send a note: what each of the three inputs returns in
   both sessions today when run by hand; the proposed property for each
   row with its tolerances and where they come from; how the existing
   predicates are shared; how the class is represented so that Equal
   cannot absorb it; whether "Admitted, not executed" keeps any row;
   anything in the ADR the code makes wrong. If an inventoried item
   would change, send that instead and stop.
3. Implement the class, the properties and the execution of the three
   rows.
4. Add the teeth controls and regenerate the pinned table.
5. Run the repeated run under load, Miri if any unsafe or engine code is
   reached that was not before, a final in required mode, preflight and
   the commit checker. Bring the packet with the class of each moved
   row, the totals before and after, the repeated-run counts and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
