# ADR 0225: Huge-page advice for large column outputs

Status: Accepted

ADR 0224's comparison found rnx 2–3.5× slower than Arrow and Polars on 100M F64
arithmetic, with no first→warm improvement. The read-only attribution
(target/next-column-0225) found the cause. Every rnx map obtains a fresh
800 MB output mapping and touches it page by page: 195,313 minor faults per
call, with 190–270 ms of system CPU. Arrow advises its large buffers with
`MADV_HUGEPAGE` (about 400 faults), and its first call is 112 ms against
rnx's 260 ms. Polars retains backing between calls (warm, 0–2 faults). The
host policy is `madvise`, so no huge pages are used unless the program asks.

This record addresses first touch on every call. Later output recycling is
a separate record and addresses repeated calls only. When the core allocates
an output or column scratch buffer of at least a reviewed threshold (default
4 MiB) on Linux, it advises the fully owned capacity interior with `MADV_HUGEPAGE`
before the first write. The advice is best-effort:
- failure is ignored and recorded nowhere;
- other platforms and Miri compile it to nothing;
- no behaviour, semantics, layout or ownership changes.

The advice is one extern block (`madvise`, `getpagesize`) in the
existing primitives module, which already holds the crate's only `unsafe`.
No crate is added; std already links the C library on Linux. The SAFETY
contract is narrow: a valid, owned, not-yet-shared allocation range rounded
inward to page boundaries, and advice only (no memory is read or written).
`experimental_core` keeps `forbid(unsafe_code)`.

Acceptance:
- outputs bitwise identical, including NaN bits on x86-64;
- protected program Ir unchanged apart from reviewed fixed per-call pins
  (the advice call on large outputs only);
- page faults per 100M F64 call fall by at least 10×, measured by the 0225
  diagnostic method;
- quiet wall time at 100M (0224 harness, rnx cells only, first and warm)
  reported against 0224 as descriptive;
- small outputs below the threshold unchanged;
- Miri and non-Linux builds unaffected.

Runtime-only reservation wrappers exclude compiler metadata and borrowed inputs.
Cache the positive, power-of-two runtime page size; skip advice if it cannot be
validated. Advice occurs once after allocation/limit checks, before initialization.
NumericOutput does not issue duplicate advice.
