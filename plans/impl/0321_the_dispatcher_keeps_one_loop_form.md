# Implementation plan 0321: The dispatcher keeps one loop form

Status: Implemented

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

## Results

Stock and image each placed Rune in 479 codegen units in all three
builds. On fib, both frozen source changes preserved the unperturbed
dispatcher instruction counts in both execution modes at both sizes.
All counts matched the four published figures in the ADR, including
the number of calls that entered and left frames.

Stock passed all 32 comparisons against its predecessor and all 32
against the cumulative anchor. Reading sealed successors removed 531
repeated decodes in shared_rows setup. At size 5 the setup work count
fell from 153,306 with unmerged units alone to 120,667. The published
stock count was 153,004.

All native execution comparisons between candidate stock and the
ordinary image passed. At fib size 5 the image used 36 more work
instructions and 82 fewer total instructions. All 32 comparisons
between the two provider siblings passed. Cross-image setup remains a
cutover measurement. At both sizes the image's fib total exceeded stock
by 105,526 instructions, and its integer and Boolean vector setup work
exceeded stock by 256 and 296 respectively.

Neither resolved graph enables Cranelift pass timing. AWFY Permute
setup still made two environment lookups from JIT function definition
in each binary. Cranelift's JIT checks PERF_BUILDID_DIR when recording
generated functions for perf.

The 22 focused behavior tests and the file adapter controls passed.
With release observation enabled, all eight trapped compile refusals
recorded zero provider allocations and zero committed host-slot bytes
through close. The admitted CSV allocated one provider value and charged
2,256 bytes. Either a planted allocation or a planted byte failed the
refusal check. The ordinary image kept its three existing exports.

The complete final on 4391f52a passed all 104 stages, with 51.0 minutes
of accepted run and 51.8 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile.
