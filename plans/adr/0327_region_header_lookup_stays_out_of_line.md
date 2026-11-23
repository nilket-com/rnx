# ADR 0327: Region header lookup stays out of line

Status: Accepted

Base: published df42ef8c1827b85f2473993d979555cb87964e01 (ADR 0326).

## Problem

0326 attributed the image's native-on Permute residual to the method
dispatcher closure. The image and the generated app dispatched the same
404,414 bytecodes at size 1. In the image, the closure also computed the
byte length for a scan over region headers 220,480 times, on paths
where the table was empty and the scan never ran. That was 38% of the
closure difference.

`ProgramRegions::header` (`region/cache.rs:415`) is marked `#[inline]`.
Its only caller (`execute.rs:1477`) already skips it when the table is
empty, the frame is resumed or the cancel hook is armed. Inlined, its
scan setup is scheduled into the hot dispatch loop anyway.

A scratch candidate on fef4fcb1 put the lookup out of line:
- Image Permute native-on execution fell 766,971 W at size 5.
- Native-off was unchanged on every kernel.
- The empty-table scan-length count fell to zero, with no helper calls.
- A nonempty-table workload (loops) did not slow down.
- The ordinary file gate passed 32/32 per record and 32/32 cumulative.

## Decision

1. The region header lookup is not inlined into the dispatcher. The
   caller keeps its empty, resumed and armed checks, so an empty-table
   dispatch pays no call and no scan setup. The implementer chooses the
   attribute form and records why.
2. Nothing else changes. No opcode, dispatcher structure, host ABI,
   project route or legacy engine change.
3. Scoring is image to image: the ordinary file gate across all kernels
   against published df42ef8c, and both project routes (empty and
   Polars/Candle) against their published df42ef8c images, in both
   native modes at both sizes. Report execution W and T, level and
   growth. No allowance.

## Controls

1. A profile shows zero header calls and zero scan-length computations
   on an empty-table dispatch path, and the header call count is
   unchanged on a nonempty-table workload.
2. Region selection, native entry counts, outputs and fuel accounting
   are unchanged. The existing region, cancellation and resume tests
   pass.
3. A nonempty-table workload (loops and shared_rows) shows no
   execution regression.
4. The file and project scores in Decision 3.
5. Finals: targeted pre-sign checks under the final's real recipe, then
   one complete final on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- Programs admitted today stay admitted, with the same results and
  effects.
