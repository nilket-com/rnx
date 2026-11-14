# Implementation plan 0273: async functions awaited directly in an execution session

Status: Implemented

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

## Results

- The collector admits async function bodies only for checked session entries;
  ordinary functions retain the legacy await diagnostic. Direct script awaits
  emit ordinary certified calls. Async function values and unawaited calls are
  refused by name before effects. Async blocks, closures and select stay refused.
- Three declaration-transaction controls cover both incompatible redefinition
  directions, including an async caller with a previously bare ordinary call.
  They preserve declaration text, bindings, aliases and subsequent calls.
- Engine and session diagnostics are compared for all three script-await
  variants with a named caller. The exact reviewed classification pairs have
  mutation controls on both sides.
- Existing call ownership is reused. The new core controls cover retained heap
  arguments, one host effect across one-instruction slices, the exact configured
  frame frontier, failed-call cleanup and subsequent invocation.
- The ordinary lowering stack allocations beside the base are unchanged:
  call 1,112; call_non_variant 9,272; expr 1,624; inner 600; operand 56;
  operands 1,288; inner_fn 2,664 bytes. The prologue measurement includes every
  page-probing subtraction. Normal and headroom stack controls pass.
- Native controls execute the fetch example against loopback and interrupt
  nested sleep/HTTP waits with real signals in both engines. Error and result
  propagation, early return, moved arguments, source locations, refusals before
  effects and terminal fuel exhaustion are covered.
- The existing exact host-wait Miri inventory includes the three new script
  ownership/refusal/frame-cleanup cases. No gate implementation was changed.
- Measured parity totals: 284 -> 292 rows; Equal 241 -> 245, Equal by property
  3 -> 3, Intentional difference 10 -> 13, Refused at admission 1 -> 1,
  Refused for a host function 9 -> 9, Wording 20 -> 21. The old sections are
  unchanged and the row-oriented pin remains below the existing cap.

Evidence: target/0273/LEGACY-v1/RESULT.json, DEPTH-v1/RESULT.json,
DEV-v5/RESULT.json, NATIVE-v3/RESULT.json, FRAMES-v1/RESULT.json and
REVIEW-v1/{wording,classifications}.log. These distinguish source, behavior,
stack and classification controls from final acceptance.
