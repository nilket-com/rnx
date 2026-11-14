# Implementation plan 0277: the object methods the project's own scripts use

Status: Proposed

Paired ADR: ../adr/0277_object_methods_the_demos_use.md.

1. Pin 09c47eeb. Read how method calls are resolved and certified in
   the new engine's lowering, which methods on objects, vectors,
   strings, tuples and Option are admitted today and how receiver
   types are proved, the legacy implementations of the refused ones,
   and the fixed-route inventory.
2. Before editing, send a note with the measured set as the ADR asks
   (script, call, receiver type, legacy semantics, proposed route),
   which members need a new run-time operation and are cut, the
   refusals `journey.rn` meets after the method calls if any, the
   cost on ordinary lowering, and every inventoried item touched, by
   name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned tables.
5. Run both format checks, every lint recipe and the full root suite
   by hand; then the fixed-route screen, Miri, a required final
   (complete if gate tooling or census code changes), preflight and
   the commit checker. Bring the packet with the class of each
   changed row, totals before and after and the per-stage durations.
   Results state what the code does; no paths under target/. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
