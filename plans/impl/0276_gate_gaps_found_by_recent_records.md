# Implementation plan 0276: gate gaps found by recent records

Status: Proposed

Paired ADR: ../adr/0276_gate_gaps_found_by_recent_records.md.

1. Pin 4d1cf05e. Read the required-stage selection and its tests, the
   failure bundle and the final's own diagnostics file, the cross
   stage's receipts, and the parity table's serializer, its readers,
   its size cap and the controls that mutate pinned rows.
2. Before editing, send a short note: the selection rule as it is and
   as proposed, with the old and new selections for records 0272 to
   0275; what each failing stage kind writes and what the bundle will
   take from it; a byte breakdown of the parity table and the
   proposed representation with its measured size and headroom; and
   whether the cap needs to move.
3. Implement the three decisions.
4. Add the controls of the ADR to the tooling's own tests.
5. Run both format checks and every lint recipe by hand, then the
   complete final (gate tooling changes), preflight and the commit
   checker. Bring the packet with the sizes, a sample bundle for each
   failure kind and the per-stage durations. Results state what the
   code does; no paths under target/. The candidate is frozen once
   the packet is sent.

Results are added here by the implementer.
