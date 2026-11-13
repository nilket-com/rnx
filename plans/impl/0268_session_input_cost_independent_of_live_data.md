# Implementation plan 0268: the cost of a session input does not depend on live data

Status: Proposed

Paired ADR: ../adr/0268_session_input_cost_independent_of_live_data.md.

1. Pin 2b1b1c43. Read the program domain and its collection, the
   store's reachability walk, the session's call after each input and
   its revision maps, every producer and consumer of function values
   and closures in the store, and the fixed-route inventory.
2. Before editing, send a note with: a phase timing of one input in
   the three sessions of the ADR on the pinned commit, so the walk's
   share is measured and not inferred; the alternatives considered
   (for example reachability metadata kept at construction, counts
   per program, or collection on a schedule) with what each costs on
   ordinary heap operations and what each does to release timing; the
   one proposed, the list of function-value paths the ADR asks for,
   and the release bound. If any inventoried item would change, send
   that instead and stop.
3. Implement the design and its failure handling.
4. Add the controls of the ADR, including the long rebinding session
   and the three measured sessions. Large fixtures run natively; Miri
   covers the ownership paths at small sizes.
5. Run Miri, a complete final if any gate tooling changes and a
   required final otherwise, preflight and the commit checker. Bring
   the packet with the measurements before and after, the release
   bound, the fixed-route result and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
