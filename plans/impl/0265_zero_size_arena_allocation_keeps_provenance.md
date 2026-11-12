# Implementation plan 0265: a zero-size arena allocation keeps strict provenance

Status: Proposed

Paired ADR: ../adr/0265_zero_size_arena_allocation_keeps_provenance.md.

1. Pin f064fcb9. Read the arena's allocation paths and their callers,
   the Miri receipts of ADR 0264 that failed, and what the pinned
   toolchain offers for a dangling pointer without a cast.
2. Before editing, send a note: the committed case failing under Miri
   with its diagnostic; the proposed construction and why the address
   and alignment are unchanged; the sweep table of integer-to-pointer
   casts with the proposed disposition of each; whether any is inside
   an inventoried item; anything in the ADR the code makes wrong.
3. Fix the arena and the casts the note's ruling puts in scope. Restore
   the two tests of ADR 0264.
4. Commit the inventory.
5. Run Miri on the new inventory and on the 0264 inventory, a final in
   required mode, preflight and the commit checker. Bring the packet
   with the before and after receipts, the sweep table and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
