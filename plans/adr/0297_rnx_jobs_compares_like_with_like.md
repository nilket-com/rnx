# ADR 0297: rnx-jobs compares like with like

Status: Proposed

Base: published 6364832e07b0d4250fa097b01d2d14f288f1ccbb (ADR 0296).

## Problem

`rnx-jobs` (ADR 0295) estimates a running final's time against the most
recent complete PASS, whatever the running final's scope. Watching a
six-stage required final, the user saw "0m / ~37m est" against the last
37-minute complete final, while the run took about two minutes. The
fixtures planted only complete finals as references, so no control saw
it.

## Decision

1. The reference is the most recent final, other than the one shown,
   whose terminal receipt is a PASS and whose `SELECTION.json` has the
   same `scope`:
   - `complete`: the PASS must also be `complete_final`, as now; a
     complete final stays comparable across catalogue changes in its
     stage list;
   - any other scope (`required`, `explicit`, `docs`): the same set of
     selected stage names as the final shown.
2. The label names what was matched: "the last complete PASS", or "the
   last required PASS of the same N stages".
3. With no comparable PASS, the view says so (for example "no earlier
   required PASS of the same 6 stages to compare with"), with no time
   bar and no estimate.
4. Nothing else changes: discovery, liveness, receipts read as regular
   files only, the idle view and `--final` keep their behavior.

## Constraints

- No new crate, no Python.

## Controls

Recorded snapshots over planted finals of both scopes:
- a required final against an earlier required PASS of the same stages,
  with a more recent complete PASS present and ignored;
- a required final whose only earlier required PASS selected other
  stages: the fallback;
- a complete final against the last complete PASS, with a more recent
  required PASS present and ignored.
The existing snapshots keep their text. A mutation that ignores scope
fails a control. The required final.
