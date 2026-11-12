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

## Implementation for source review

The existing clock predicate file is compiled once as a test-only crate
module; census re-exports it. Its single-reading contract is unchanged:
wall before <= value <= after+1, monotonic 0 <= value <= elapsed+1,
and an empty clock tag imposes no clock constraint. Boundary controls pin
these behaviors. Session rows use the same wall predicate and a named
pair-envelope property for monotonic clocks.

Each monotonic pair belongs to one session, with the second reading in a
later input after a measured pause of at least 20 ms. Bounds are the actual
gap minus 1 ms and enclosing span plus 1 ms, converted to the clock's unit.
Although both sessions share the process origin, the property never compares
readings across sessions and does not depend on that shared origin.
Canonical i64 presentation, explicit i64 type probes and retained binding
reads are verified before only numeric reading slots are normalized. Raw
observations remain under target; the pin contains the named property.
Property classification precedes ordinary equality, including coincident
raw readings. Constant, wrong-type, wrong-unit in both directions, decreasing
pair, changed binding/type and changed non-reading fields have controls.

Focused clock controls PASS (4 tests); regenerated classification pin PASS.
Only now_ms, monotonic_ms and monotonic_ns move, all to Equal by property.
Counts: Equal stays 194; Equal by property becomes 3; Admitted, not executed
has no remaining rows and disappears from the summary. Other classes stay:
Intentional difference 3, Not classified 4, Refused at admission 1,
Refused for a host function 10, Wording 20.

Source review, the committed 50-run campaign under build load, required
final and publication checks remain pending. No engine, host-function or
inventoried executor code is changed; no newly reached unsafe path needs Miri.

Source review accepted with two repairs: observation files are opt-in through
RNX_CLOCK_OBSERVATIONS; failing rows retain raw evidence without that variable.
The type and binding controls now isolate each failure against a consistent
positive stub, and wall property boundaries are tested directly. Five focused
clock controls pass. The three former default observation files were removed.
