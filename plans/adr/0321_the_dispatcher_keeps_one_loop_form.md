# ADR 0321: The dispatcher keeps one loop form

Status: Accepted

Base: published 6e29884eccc051d28c82e873b004cccd99ebfef4 (ADR 0320).

## Problem

`Executor::invoke_args` holds the recursive opcode loop. Its checked
operand read compares an index against `Executor.slots.len`, at offset
0x80 from the executor. Both compiled forms read that length from
memory. What differs is the executor base pointer: whether it stays in a
register. In 0320 an unused 13-line method flipped stock into a form
that reloads it from the stack on each of 70,869 visits at fib size 5.
Stock was repaired by excluding that method. The engine image still
compiles in the spilled form. Its fib size-5 execution is +389,949 W
and +389,910 T against stock, beyond the ordinary allowances of 370,237
and 370,267. It is the only native-on execution axis that fails. Record
6 cannot spend the cutover allowance on execution, so this blocks
cutover.

The two 0320 forms make the same calls with the same counts. With
registers and stack offsets normalized their code matches closely; the
spilled form adds 50 stack stores and 34 stack loads. The loop sits near
the allocator's spill threshold, and something outside it decides the
side.

The release profile uses thin LTO with the default 16 codegen units.
Codegen-unit merging is size-driven, so code added anywhere in `rune`
can change which items share the loop's unit. Four trials:
- A named dispatcher alone kept the reload. The permitted frame-local
  slot view removed it but cost 2,798,564 more dispatcher instructions
  at fib size 1, mostly new spills and slice reacquisition.
- `codegen-units = 1` for `rune` gave stock and image one native loop
  form, 602,220 instructions below the size-1 ceiling. But the
  interpreted dispatcher gained 5,101,868 in native-off fib, and every
  native-off kernel regressed 0.69% to 4.78%.
- A separately compiled dispatcher object, with one shared typed step
  over backend primitives, regressed size-5 execution by 80.4% on fib
  and about 188% on permute. The typed step alone, without the object,
  cost 56% on fib, and permute converted the active frame 583,457 times
  for owner operations. It was withdrawn.
- 8,192 units for `rune`, which rustc keeps as 479 unmerged units in
  both feature sets, passed every execution check. Stock and image
  matched exactly on fib dispatcher counts, native and interpreted. Only
  stock `shared_rows` setup failed: 153,004 to 153,306 W and 160,851 to
  161,153 T, against allowances of 37 and 11. The calls along the
  implicated paths are identical. `Function::operation` runs 103 times
  and its wrapper changed from a tail jump to a call and return, 5 more
  instructions each. Post-seal region analysis decodes successors 531
  times from encoded code although sealed code already holds the decoded
  instructions.

## Decision

1. `rune` is built with codegen units above its per-module count, so no
   size-driven merging happens. Unrelated code in another module then
   cannot change the dispatcher's unit. The rest of the workspace keeps
   its release settings. Actual unit placement is verified per build.
2. Post-seal region analysis reads successors from the sealed, decoded
   instructions through `Function::cached_ref(pc)`. One target
   computation and range check is shared with the encoded pre-seal
   validation path. Checked PC overflow, invalid targets and missing
   instructions are handled as before. No new CFG or cache, and no
   dispatch-loop edits.
3. Cranelift's optional compiler-pass timing is off. `rune` takes
   `cranelift-codegen` without default features, with `std`, `unwind`
   and `host-arch` only. The other Cranelift crates already disable its
   defaults. With timing on, `DefaultTimingToken::drop` carried
   accumulated wall-clock nanoseconds into seconds, at 4 instructions
   per carry. That made native-on `awfy_permute` setup growth differ by
   8 instructions between the two provider siblings on one engine
   image, depending only on elapsed time.
4. No protected item moves. `ci/execute_guard` and the phase observer
   are unchanged.
5. The speed gate stays the guard against future flips. This record
   removes the merging mechanism it found; it does not claim the loops
   are isolated from every change.
6. Not in this record: packaging (record 5) and cutover (record 6). No
   cutover allowance is spent here.

## Controls

1. Successors. Encoded and decoded successor queries agree on targets
   and errors for every instruction in the fixtures, including overflow,
   invalid target and missing instruction. The repeated post-seal decode
   calls are gone.
2. Features. The resolved feature graph for stock and image has no
   `cranelift-codegen/timing`, and no code or diagnostic uses its
   timing reports.
3. Plants, frozen before counts: plant A, the 0320 unused method (on
   the image, a second unused definition under a distinct name), and
   plant B, the independent registry-layout change from
   `ref-0316-tagged-core-fail` (c03e02ee). Each changes compiled input
   for its build kind. Unit placement is recorded for each build.
4. Loop form. Native-on and native-off fib at sizes 1 and 5, on stock
   and image, unperturbed and under both plants:
   - dispatcher self Ir and exact direct push and leave counts meet or
     beat the published figures for their mode, compared as integers;

     | Mode | Size | Self Ir | Push | Leave |
     | --- | ---: | ---: | ---: | ---: |
     | native off | 1 | 20,173,902 | 35,422 | 35,423 |
     | native off | 5 | 85,456,676 | 150,050 | 150,051 |
     | native on | 1 | 25,682,347 | 35,422 | 35,423 |
     | native on | 5 | 25,689,887 | 35,430 | 35,431 |

   - dispatcher self Ir is identical across the three builds of each
     kind, for both dispatchers.
5. Speed, with the deciding pairs and axes frozen before counting.
   - The independently based stock screen: native on and off, every
     kernel, setup and execution, size-5 level and size-5-minus-size-1
     growth, W and T, per record and cumulative.
   - Candidate stock against candidate ordinary provider plus image,
     with matched canonical paths, features and toolchain: native
     execution passes 5 per mille on W and T for every kernel. This is
     the cutover blocker; it must clear here.
   - The matched ordinary and extended image sibling gate passes.
     Cross-image setup is reported separately, for record 6.
6. Behavior: the sliced yield and resume correspondence, cancel before
   fuel, the first error location and PC, partial-retain rollback, reuse
   after failure, native side exit and replay, host failure, and depth
   and slot limits. The 0320 adapter controls pass.
7. Refusal counters, on the optimized build.
   - The v3 provider descriptor has no size field. It is not extended
     and no old storage is reinterpreted. A separately versioned,
     size-checked image-controls entry takes caller-owned observation
     storage.
   - The observation is bound per context and invocation, through
     teardown. Registration ends before the storage can be freed.
   - Allocations count when the provider successfully inserts an owned
     token, not from the process-global sequence. Debits accumulate the
     committed host-slot charge in bytes at admission, independent of
     invoke and release. Both counters only increase.
   - The caller reads before open and after close returns, without the
     old cookie.
   - All eight compile refusals record zero allocations and zero debits,
     with the invocation trap armed. A real admitted CSV advances both.
     An allocation-only and a debit-only planted mismatch each fail.
   - Old callers and the ordinary exports are verified. If the control
     feature changes the ELF, the observed and gate binaries are
     recorded by hash.
   - The 0320 debugger probe is retired once these pass.
8. Finals. Targeted pre-sign checks, then one complete final on the
   signed candidate. A failure keeps its evidence and stops.

## Constraints

- Target: x86_64-unknown-linux-gnu. No new third-party crate. Tabs.
- ABI v1, v2 and v3 callers keep working unchanged.
