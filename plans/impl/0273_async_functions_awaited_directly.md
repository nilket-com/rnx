# Implementation plan 0273: async functions awaited directly in an execution session

Status: Proposed

Paired ADR: ../adr/0273_async_functions_awaited_directly.md.

1. Pin f7a8e42f. Read how ADR 0270 admits and lowers a direct host
   wait and refuses the other forms, how the session collects and
   persists function declarations, where async functions are refused
   today in the frontend and the collector, the legacy semantics of
   calling and awaiting an async function, and the fixed-route
   inventory.
2. Before editing, send a note with: a measured table of legacy
   behavior for the direct form (argument order, effects before a
   wait, errors, moved arguments, recursion depth); the proposed
   lowering and why the body can run at the call; every form that
   must be refused and where each is detected; how an async function
   is distinguished from an ordinary one after collection; the cost
   on ordinary call lowering; any observable difference from legacy;
   and every inventoried item touched, by name. If any is touched,
   send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run the fixed-route screen first, then Miri, a required final
   (complete if gate tooling changes), preflight and the commit
   checker. Bring the packet with the class of each new row, totals
   before and after, the three slowest Miri cases and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
