# Implementation plan 0267: an opaque host value in an execution session

Status: Proposed

Paired ADR: ../adr/0267_opaque_host_values_in_an_execution_session.md.

1. Pin 093e7caf. Read how the legacy session holds a host-typed value
   and dispatches its methods; the execution engine's value and store
   representations and what "Root" means in the host bridge; how the
   host bridge marshals arguments and results today and where it
   refuses; the presenters; the fixed-route inventory.
2. Before editing, send a note: the representation proposed for an
   opaque value and who owns it; how a shared-reference receiver
   reaches the host function without the engine seeing inside; the
   capability table of the ADR with a proposed disposition per row
   and its cost; how a `Dense` is obtained in tests only; every
   inventoried item the design touches, by name. If any is touched,
   send the note and stop.
3. Implement the representation, ownership, receiver passing and
   display, and the named refusals.
4. Add the controls and the parity rows; regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit
   checker. Bring the packet with the capability table, the class of
   each row, the totals before and after, and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
