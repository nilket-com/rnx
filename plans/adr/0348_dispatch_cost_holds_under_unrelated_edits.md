# ADR 0348: Dispatch cost holds under unrelated edits

Status: Proposed

Base: published 2df16832 (ADR 0347).

## Problem

One instance of the dispatch loop, `Executor::invoke_args` with
a method program, compiles to more than one form from the same
source, and an edit to an arm the measured program never runs
picks the form. Measured on awfy_permute with native on at size
5, where that instance is entered once and dispatches 404,642
instructions, in self instructions of the loop:

- The image of ADR 0346 has 26,323,728. The image of ADR 0347
  has 25,065,956. ADR 0347 didn't change the function, and its
  guard hash is the same on both.
- On the base with one error detail changed outside the
  function, both arms as published give 25,065,956. A change to
  the `Len` arm alone, to the `Clone` arm alone or to both gives
  26,095,951 or 26,323,728.

The difference is 2.55 or 3.11 instructions for each instruction
dispatched. It is the resume marker of the native tier,
`resume_pc`. The cheap form keeps it in a register. The other
forms store it to the stack and load it back on every dispatch.
In this workload the frame's header table is empty and the
header lookup is never called, so the marker is kept current
with nothing to read it.

The per-record gate compares a record with the one before it.
ADR 0347 was credited about 1,257,000 instructions it didn't
earn, and the next record that edits either arm fails the 5 per
mille band by about that much with no work of its own on that
path. The held-iterator fix stopped on exactly this row.

## Decision

1. The cost of a dispatch on a frame with an empty header table
   doesn't depend on an edit to an arm the program doesn't run.
   The record measures this by Control 2 and doesn't rest on the
   form one build happens to get.
2. The means is the implementer's. The measured lead: a frame
   whose header table is empty needs no resume marker, so that
   path can do none of its work whatever the compiler does with
   the value. Begin there.
3. Behavior is unchanged. The cancel and fuel step runs at every
   charged instruction as now. A frame with a header table still
   doesn't re-enter a region at the pc an exit resumed at, and
   the frame-entry activation keeps the same rule. Results,
   errors, positions and budget slices are those of the base.
4. Scope: the dispatch loop of the new engine. `invoke_args` is
   in the gate's source inventory, so the record declares the
   transition under ADR 0346. No band and no gate rule changes.
   The count of legacy-only cases stays 67.
5. If the lead doesn't give Decision 1 under Control 2, stop and
   report with the counts for each instruction. No second
   mechanism without a ruling.

## Controls

1. The file gate against the published base, all per-record and
   cumulative rows. The awfy_permute native-on rows are no
   higher than the base. Results give the self instructions of
   the instance at sizes 1 and 5 on the base and the candidate.
2. The edits that picked a dear form on the base, each applied
   on the candidate in scratch and scored against it: the `Len`
   arm as a call, the `Len` arm with a cold branch, the `Clone`
   arm, and both arms. Each stays inside the per-record band.
   Results give the self instructions for each dispatch in every
   cell.
3. The machine code of the instance on the empty-table path has
   no store or load of the marker. Results give the executed
   count of the block.
4. A witness for each rule of Decision 3 on a frame with a
   header table: a region exit that resumes at a header pc, and
   the frame-entry activation. Each rule's removal, planted
   back, breaks its witness.
5. The instruction-accounting controls and the budget and
   cancellation tests pass unchanged.
6. The public admission corpus is replayed against the published
   base in both native modes. No source changes its outcome or
   route.
7. Both project screens, image to image against the published
   base.
8. Before signing, the whole rune-suite stage, the root-tests
   stage, the cross stage and the final-controls stage, each by
   its stage command. Then one complete final on the signed
   candidate, under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- No change to the legacy engine or the host ABI.
