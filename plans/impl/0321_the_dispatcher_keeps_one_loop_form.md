# Implementation plan 0321: The dispatcher keeps one loop form

Status: Proposed

Paired ADR: ../adr/0321_the_dispatcher_keeps_one_loop_form.md.

1. Boundary: the no-merge decision and the successor repair, reconciled
   against published 6e29884e. Record any deviation before editing.
2. Set the `rune` codegen-unit count and verify unit placement. Repair
   post-seal successor queries, with the successor agreement control.
   Turn off Cranelift pass timing and check the feature graph. Measure
   setup before and after across all kernels.
3. Freeze the plants and recipes. Run the loop-form, speed and behavior
   controls. Stop on a failed gate.
4. Add the release refusal counters and retire the 0320 debugger probe.
5. fmt, Clippy, preflight and the targeted checks. Sign. Source review,
   one complete final, Results.
