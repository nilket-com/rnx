# Implementation plan: ADR 0227

Reapply the ADR 0225 minimal source (archived bundle, ref
`refs/archive/adr0225/minimal`, commit `26271ba2`) on top of `94c30cee`:
- the primitives helper and its tests;
- the arch-gated extern block;
- one call at the end of `reserved`.

Check that the diff against `94c30cee` is limited to those lines.

Before launching anything, run the exact local clippy command. Then run the
core controls and the targeted `huge_page_tests` Miri. Rerun the 0225 fault
diagnostic and the small exact-vector comparison against `94c30cee` across
the full 0226 protected matrix. If that passes, pin any fixed deltas, run the
10M/100M matrix, then the 0224-harness rnx timing cells (5 process samples)
and the host, integration, generated and cross gates, plus the commit
checker. Then publish the bench report.

Reviewed fault capture: retain the raw per-call-gate FAIL; no retries. One
cold f64const-flat call improved 7.317× (26,694 vs 195,320 faults), while
the other calls improved roughly 170–314×. The revised best-effort rule
requires no call worse than baseline and a median reduction of at least 10×.
Retained smaps shows 675,840 KiB huge-page backing on that cold call versus
780,288 KiB warm, despite THP eligibility and the huge-page advice flag.
This supports partial huge-page backing as the source of excess faults.
No vmstat THP counters were captured, so the kernel fallback cause is unproven.

User-approved instruction exception: the measured 79be56cb/42c9ffd4 pair
permits exactly +1 instruction per all-valid multiply3 execute call. Mixed
calls save exactly two instructions; flat F64 pair calls save four. The
scorer derives absolute calls from chunk boundaries and validity, including
partial words and blocks. Other function differences remain exact fixed
pins. Every pin has a +1 rejection control. Raw measurements are retained.
Confirm at 10M/100M before the five-process 100M timing comparison. Timing
must show improvement before publication; otherwise stop for review.

Large capture retained its raw FAIL and passes reviewed rescore. Advice and
page-size initialization are exact first/warm constants, selected separately
by data-buffer and validity-buffer 4 MiB classes. In particular, 10M Boolean
bitmaps remain below the threshold; 100M bitmaps exceed it. No measurements
were repeated. The executor and pair formulas match both large sizes.
