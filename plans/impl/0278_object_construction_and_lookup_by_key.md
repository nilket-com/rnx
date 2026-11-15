# Implementation plan 0278: object construction and key lookup independent of object size

Status: Proposed

Paired ADR: ../adr/0278_object_construction_and_lookup_by_key.md.

1. Pin 64fd789d. Read the record payload and every operation that
   finds a key in it (construction, owned input and JSON import, field,
   index, methods, comparison, cloning, display, roots and views), the
   old engine's insertion-ordered map, the store's byte budget and
   rollback, and the fixed-route inventory.
2. Before editing, send a note: the index design and its hash or
   worst-case bound, where it lives and what of the old engine it
   reuses, the threshold below which no index is built and the
   measurement that sets it, the payload size before and after, every
   operation's new cost, what remove does, and every inventoried item
   touched, by name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate any pinned table.
5. Run both format checks, every lint recipe and the full root suite
   by hand; then the fixed-route screen, Miri, a required final,
   preflight and the commit checker. Bring the packet with the cost
   table measured again, the class of each changed row, totals before
   and after and the per-stage durations. Results state what the code
   does; no paths under target/ and no development history. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
