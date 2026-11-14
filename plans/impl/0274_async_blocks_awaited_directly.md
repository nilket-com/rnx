# Implementation plan 0274: async blocks awaited directly in an execution session

Status: Proposed

Paired ADR: ../adr/0274_async_blocks_awaited_directly.md.

1. Pin e4076bd3. Read how ADR 0273 lowers a direct script await and
   refuses the other forms, how an async block is represented after
   the legacy frontend (captures, body, calling convention), where it
   is refused today, and the fixed-route inventory.
2. Before editing, send a note with the measured legacy table the ADR
   asks for, the proposed lowering, how `return` and captures are
   kept, each refused form and where it is detected, the cost on
   ordinary lowering, any observable difference, and every inventoried
   item touched, by name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run the fixed-route screen first, then Miri, a required final
   (complete if gate tooling changes), preflight and the commit
   checker. Bring the packet with the class of each changed row, the
   totals before and after and the per-stage durations. Results state
   what the code does; no paths under target/ and no development
   history. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
