# Implementation plan 0254: witnesses for session rows blocked by their setup

Status: Implemented

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

## Implementation and focused evidence

Seven explicit recipes mirror seven source functions and eight unread setup
sites. The checks recipe replays both the budget change and the external
write, then the hundred increments. The literal extractor and its blocking
rule remain unchanged. Only an exact sequence/ordinal/input/blocker match
can attach a measured witness to a blocked row.

The source-function token digest and recipe digest pin a reviewed
correspondence, not a proof of semantics. Each replay separately asserts the
legacy test's expected values and output. Controls alter an input in every
recipe and mirrored source function, retain blocking without a witness,
and reject different captured output.

All fourteen comparisons are Equal: checks rows 11 and 14; generated
bindings row 1; assignment row 8; aliases rows 3–6; closure rows 3–4; format
rows 2–4; template row 2. Equal rises from 139 to 153 and Not classified
falls from 18 to 4. No other row changes class; all other report sections
and work groups are unchanged. Existing sequence summaries count the newly
resolved successful rows, but still mark these sequences as not clean
end-to-end because their unread setup provenance remains.

The private external paths have equal-length role names and never enter the
pinned report. The original failed-write side effect is checked before its
explicit removal. The test-only budget helper lives in the existing test
module; production engine/session/host behavior is unchanged.

Focused witness controls and the regenerated pinned-classification check
pass. Final required-stage results and durations are supplied in the
publication packet.

The generator retains non-Equal observations; the pinned-table comparison is
the gate. A synthetic differing-output witness is preserved through the same
exact-key attachment path. Test execution requires the coreutils sha256sum
command to check the reviewed correspondence digests.
