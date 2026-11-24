# Implementation plan 0330: Final stages own their scratch space

Status: Implemented

Paired ADR: ../adr/0330_final_stages_own_their_scratch_space.md.

1. Boundary: published 0f3df71d and the 0328 FINAL-v1 and v2 evidence.
   Trace temp-writing stages and test families through the actual child
   environment and controller-assigned roots. Identify which exceptions
   use a private mount and which use a host path. Record deviations
   before editing.
2. Assign private stage scratch through the existing ownership and
   recovery paths. Keep producer lifetimes, bounded failure diagnostics
   and the census and fixed-route contracts.
3. Bind disk-only fixtures to the guarded bootstrap handoff. Preserve
   real build-path admission and account for the output filesystem.
4. Repair the leaking test families and verify cleanup independently of
   the controller's sweep. Keep caller-requested evidence.
5. Run the ownership, exception, disk and failure controls from the ADR,
   including the full final-controls command under the real recipe.
6. fmt, Clippy, preflight, sign and source review. One complete final;
   report its duration and any attributable leftovers.

## Results

The controller claims one private directory per stage. When the final
output has a Git or Cargo ancestor, it selects validated external
scratch. A child and its grandchild wrote there. Concurrent stages
used distinct directories. The success and failure paths released both the
private scratch and generated-project bulk, and the bounded failure
index survived. The recovery control kept a foreign sentinel.

The census still creates its capped work and temporary mounts.
Fixed-route capture uses its canonical environment, and the image
observer control runs with that same environment. Generated projects
share the validated external scratch location. Cross confines Cargo's
temporary-path override to its build namespace.

The engine budget-edge tests preserved explicitly requested output.
The Polars and Candle fixture controls left their sentinel as the only
entry in their private temporary roots after normal execution. A
planted unwind released its own fixture and kept the existing sentinel.

The following three runs tested the parked scratch implementation before
its rebase onto 0329. The first complete final on 33f6f192 failed in
controller-controls. A build-identity test asked Git to discover a
repository from a temporary directory. Git found the enclosing final checkout. The shared /tmp
comparison found no new owned entries. The successor checks configuration
ancestry at the controller boundary and accounts the generated-project
and stage scratch allocations on the selected filesystem.

FINAL-v2 passed all 107 stages, then failed cache finalization. A
process fixture hard-linked its test executable on the cache device.
Creating and removing the link changed the producer inode's ctime.
The fixture now executes the original harness in its private working
directory. A same-device control checks its inode, ctime and link count
before and after running the fixture.

A separate recovery test deleted its ledger after a deliberate drain
refusal. Its teardown now waits for the fixture processes and reclaims
the recorded roots through the ledger. Three earlier directories each
accounted for 8 KiB. FINAL-v3 left no new scratch entries in /tmp or
/var/tmp.

FINAL-v3 measured 11 extra native numeric_calls setup instructions
against an allowance of 5. All 11 belonged to Cranelift's
VCodeConstants::insert at size 5. The measured launch arguments,
working directory and ordered environment matched 0328. On one
retained anchor binary and an identical fixture, changing the U64
constant table's hash seed produced one extra key comparison and
exactly 11 extra setup instructions. Execution counts were identical.
The original run did not record its seed.

ADR 0329 fixes the measured Rust hash seed before the constant table is
created. This prevents the demonstrated seed-dependent variation from
recurring between launches. The failed pre-rebase receipt retains its
original verdict.

After the rebase, all ten selected pre-sign stages passed on e07d743c.
The first diagnostic used the wrong signing agent for the controller
fixtures and passed the diagnostic driver as the cached Cargo launcher.
The second passed final-controls, then fixed-route refused the driver's
test-thread environment override. Correcting the diagnostic environment
let fixed-route run against published 0f3df71d. All 32 per-record rows
and all 32 cumulative rows passed. No new stage-temporaries directory
remained in /var/tmp. Across the three diagnostic transactions, all
1,489 retained paths in the two affected cache lanes matched a producer.

The complete final on 494dca42 passed all 107 stages, with 53.3 minutes
of accepted run and 53.8 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile. Comparing owned entries in /tmp and
/var/tmp before and after the final found none added.
