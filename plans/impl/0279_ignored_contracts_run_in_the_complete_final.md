# Implementation plan 0279: ignored contracts run in the complete final

Status: Proposed

Paired ADR: ../adr/0279_ignored_contracts_run_in_the_complete_final.md.

1. Pin aa12d6db. Read the native manifest's contract and ignored
   entries, the contract harness under the gap_hunt tests, how the
   complete final selects the Rune suite and its catalogue check, and
   the 11 failing contracts.
2. Make each contract compare every row before failing, then run all
   11 and collect every differing row.
3. Before editing any expectation, send a note: per contract, each
   differing row with its class from the ADR, the record that caused a
   changed refusal, the old-engine or ruling evidence for each admitted
   row, and every defect. If a defect needs more than a small fix, or
   any row fits no class, send the note and stop.
4. Update the expectations and fix the small defects. Add the manifest
   contracts to the complete final with the catalogue check, and the
   two negative controls.
5. Run both format checks, every lint recipe, the full root suite and
   the full Rune suite with the ignored contracts by hand; then the
   fixed-route screen if the engine source changed, a complete final,
   preflight and the commit checker. Bring the packet with the class
   counts, the rows totals and the per-stage durations. Results state
   what the code does; no paths under target/ and no development
   history. The candidate is frozen once the packet is sent.
