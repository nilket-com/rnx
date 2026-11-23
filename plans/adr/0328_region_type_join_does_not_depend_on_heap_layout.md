# ADR 0328: Region type join does not depend on heap layout

Status: Proposed

Base: published aa5af56504c1a7cc25d8e7f8315fcf31677d72b7 (ADR 0327).

## Problem

The `dbg!` candidate failed the file gate on native-on `vec_iter_bool`
setup work: +352 W against an allowance of 199. Total setup instructions
fell 15,325. The +352 came from two places:
- +316 in `region::function_types_in`. Before the vectorized join of
  successor slot types, the compiler emits a runtime check that the
  input and output ranges don't overlap. One comparison settles one
  address order, and the other order needs a second comparison. The
  second comparison ran on 123 joins instead of 44.
- +36 in the `memcpy` forwarding stub, from 18 more copies in glibc
  `_int_realloc`.

On the unchanged published binaries, holding one 96-byte allocation
before startup reproduces the full +352 at both sizes. Results and
execution are unchanged. All 123 ranges were disjoint. The gate cell
measures where the heap placed two buffers, not the work done, so any
change that shifts early allocations can fail it.

## Decision

1. The successor slot join in `function_types_in`
   (`experimental_core/region/mod.rs`) becomes a private, non-inlined
   helper taking `target: &mut [FTy]` and `incoming: &[FTy]`, and
   returning whether any slot changed. The slices already come from
   separate budgeted buffers. The exclusive borrow lets the optimizer
   drop the runtime overlap check. The emitted code is the proof.
2. The join always runs, including for a newly reached successor. Its
   result combines with first reach for work-list scheduling. The
   lattice, buffer allocation, work cutoff, ordering and error paths
   are unchanged.
3. No `unsafe`, no extra buffer, no allocator change. The `memcpy` stub
   variation stays as it is and stays visible in the receipts.
4. Scope: no opcode, dispatcher, host ABI or legacy engine change.

## Controls

1. All 25 ordered `FTy` pairs: the joined value and the changed flag,
   plus empty slices and an already-equal row.
2. The optimized join and its caller in the production image: no
   dispatch on address order between the two slices, and vectorization
   kept where it pays.
3. The frozen placement witnesses (plain, inert, 16/32 and 96/32 held,
   at both sizes), with caller plus helper self cost including the call.
   Join work must be identical across layouts, with unchanged results.
4. The file gate and both project screens, image to image against
   published aa5af565, counting any added call cost. No allowance.
5. Stop if the optimizer emits the overlap check again or the helper's
   cost breaks a band.
6. Finals: targeted pre-sign checks under the final's real recipe, with
   every measured delta checked against its band, then one complete
   final on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- Programs admitted today keep the same results and effects.
