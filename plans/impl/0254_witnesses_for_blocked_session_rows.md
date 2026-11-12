# Implementation plan 0254: witnesses for session rows blocked by their setup

Status: Proposed

Paired ADR: ../adr/0254_witnesses_for_blocked_session_rows.md.

1. Pin 473df887. Read the literal session extractor and the blocking rule
   in src/session/parity.rs, the eight test sequences it marks, and how a
   row records its class and provenance.
2. Before editing, send a short note: the witness form, how a witness is
   tied to the test it mirrors so that drift fails, and the outcomes of a
   first comparison of the fourteen rows. If any comparison shows a wrong
   result in the execution engine, send that and stop.
3. Add the witnesses and the tie, class the rows by their comparison, and
   regenerate the pinned table.
4. Add the controls of the ADR.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the class of each of the fourteen rows, the totals before
   and after, the selected stages and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
