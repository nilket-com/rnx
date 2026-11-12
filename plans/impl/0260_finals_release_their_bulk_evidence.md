# Implementation plan 0260: a final releases the bulk evidence it owns

Status: Proposed

Paired ADR: ../adr/0260_finals_release_their_bulk_evidence.md.

1. Pin 73a8e332. Read the routing and adapter routing test output
   helpers, the cross controller and its enrollment records, accept.py,
   child_result validation, gate finalization and the ownership record of
   ADR 0253.
2. Before editing, send a note: every bulk root a final writes today and
   who names it; every reader of a bound file and when it runs; what
   resumption reads; the proposed manifest fields and its size on a real
   final; the bound for kept failure diagnostics and their release point;
   whether fixed-route outputs are in scope; anything in the ADR that the
   code makes wrong. If an inventoried item or a band would change, send
   that instead and stop.
3. Implement the committed driver, assignment and ownership of the
   roots, the manifest, the ordered release and the release record, and
   revalidation from the small records.
4. Add the controls of the ADR.
5. Remove the existing stores as the ADR limits it and list entries and
   bytes.
6. Run the required stages, preflight and the commit checker. Bring the
   packet with disk use of the stores and free space before and after,
   the manifest size, the selected stages and the per-stage durations.
   The candidate is frozen once the packet is sent.

Results are added here by the implementer.
