# Implementation plan 0274: async blocks awaited directly in an execution session

Status: Implemented

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


## Results

- Checked execution sessions accept a directly awaited `async { ... }` or
  `async move { ... }` block, including inside async functions and nested blocks.
  The existing secondary-body queue hands it to a captured-parameter function;
  ordinary certified calls preserve its separate return and try boundary.
  Await capability and the lexical diagnostic owner are separate metadata.
- Captured bindings are copied for a non-move block and heap payloads remain
  shared. Move uses the existing capture take and static moved-binding rules.
  Alias invalidation and a failed second take retain already completed effects;
  the block body does not run after capture transfer fails.
- A move of a binding needed by the checked returned delta is refused before
  input effects, located in the block and naming the binding. Omitting `move`
  or introducing a fresh shadow in a local block is a working remedy. The
  shadow keeps the ordinary payload-move and alias rules; it adds no implicit
  copy. A new top-level binding after the move has a distinct identity and can
  be saved normally.
- Bound, passed, stored, returned and later-awaited block futures remain
  refused with a local direct-await remedy. Engine Display and session remedy
  wording are checked together. Non-session refusal behavior is preserved.
  Async closures and select remain outside this record.
- A fresh block cannot transfer break/continue to an enclosing loop. The new
  boundary preserves specific located break, continue and missing-label
  diagnostics. Internal loop control, return, Result/Option question mark,
  original error locations, refusal before effects and next-input recovery
  have native controls. Errors use the lexical source owner.
- The retained-binding check uses validated envelope identities. Only a new
  move-block site creates a captured-identity set and walks the delta once;
  ordinary inputs add no binding map or binding scan. No proof solver, runtime
  operation, heap take implementation, inventoried executor item or policy
  was changed.
- Unoptimized frame allocations beside the base, counting page probes, are:

  | Function | Base bytes | Candidate bytes |
  | --- | ---: | ---: |
  | call | 1,112 | 1,112 |
  | call_non_variant | 9,272 | 9,272 |
  | expr | 1,624 | 1,624 |
  | expr_into | 936 | 936 |
  | statement | 920 | 920 |
  | block | 3,000 | 3,000 |
  | block_body | 5,624 | 5,624 |
  | inner | 600 | 520 |
  | operand | 56 | 56 |
  | operands | 1,288 | 1,288 |
  | inner_fn | 2,664 | 2,664 |

  No frame grows. Moving refusal construction out of line reduces the inner
  dispatcher by 80 bytes; the other measured frames are unchanged. Both normal
  and headroom stack controls pass. Nested blocks admit 126 levels and refuse
  127 at the existing item-depth limit of 128, both directly and inside async
  functions. Configured frame and fuel failures recover cleanly.
- Native signal controls interrupt block sleep and loopback HTTP waits in both
  engines. Pending sockets close, result-arm and later body effects do not run,
  and the next input succeeds. Native library, transport, ownership, stack and
  lint checks pass.
- The exact host-wait Miri inventory now contains fourteen cases, including
  four small block ownership controls: captured roots across slices, partial
  take failure, invocation drop after a move-block wait, and bounded unwind.
  The inventory change does not alter gate implementation.
- Measured parity totals are 292 -> 296 rows: Equal 245 -> 247, Equal by
  property 3 -> 3, Intentional difference 13 -> 15, Refused at admission 1 -> 1,
  Refused for a host function 9 -> 9, Wording 21 -> 21. The four new rows measure
  two equal results and two intentional future-value refusals. Exact observed
  pairs have mutation controls. Existing transcript and worker direct-block
  cases now execute equally; their ledger labels were refreshed. Historical
  raw report sections are unchanged. The generated pin is 159,626 bytes under
  the unchanged 163,840-byte cap.
