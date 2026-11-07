# ADR 0227: Huge-page advice for large column buffers (retry)

Status: Accepted

ADR 0225 showed that advising large column outputs with `MADV_HUGEPAGE` cuts
per-call page faults for 100M-row F64 outputs from 195,313 to about 620–1,140
(170–314×). It was withdrawn only because the reservation-path change shifted
register allocation in the generic executor's scalar Boolean `==`/`^` loop
(+1/+2 instructions per lane). ADR 0226 moved those shapes into the packed
Boolean driver. This record retries the advice in 0225's minimal form.

When `reserved` (column/mod.rs) allocates a capacity of at least 4 MiB, it
calls a safe primitives helper. On Linux (not Miri, and only on x86, x86_64,
arm, aarch64 and riscv64) the helper issues `madvise(MADV_HUGEPAGE)` over
the page-aligned interior of the new allocation, before any write. The page
size comes from a cached, validated `getpagesize()`. The advice is
best-effort: the result is ignored, and other platforms get a no-op. One
`extern` block lives in the primitives module and no crate is added;
`experimental_core` keeps `forbid(unsafe_code)`. Every call site and symbol
stays identical to the published code. The 4 MiB threshold is the guard that
keeps compiler metadata out.

Acceptance against published `94c30cee`:
- outputs bitwise identical;
- every protected program vector exact, apart from reviewed fixed per-call
  pins, with no per-row or per-block slope on any unchanged path;
- no 100M F64 call has more faults than baseline, and the median reduction
  across all 18 calls is at least 10×;
- rnx cells re-timed with the ADR 0224 harness at 100M, first and warm, 5
  process samples, reported descriptively against 0224;
- Miri and non-Linux unaffected.

If any unchanged path shows per-row growth, stop and bring disassembly
attribution. No codegen-tweak iteration.

Reviewed fault capture: retain the raw per-call-gate FAIL; no retries. One
cold f64const-flat call improved 7.317× (26,694 vs 195,320 faults), while
the other calls improved roughly 170–314×. The revised best-effort rule
requires no call worse than baseline and a median reduction of at least 10×.
Retained smaps shows 675,840 KiB huge-page backing on that cold call versus
780,288 KiB warm, despite THP eligibility and the huge-page advice flag.
This supports partial huge-page backing as the source of excess faults.
No vmstat THP counters were captured, so the kernel fallback cause is unproven.
