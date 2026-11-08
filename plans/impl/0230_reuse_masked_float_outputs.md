# Implementation plan: ADR 0230

From `8d104dd1`, extend `allocate_batch_reusing` so it admits
`masked_float::eligible` plans as well as flat and F64-driver ones, and select
`Output::F64(NumericOutput::new(previous, len))` for them, mirroring the
fresh `allocate_batch` selection added in ADR 0229. Keep the refusal sequence
identical to `allocate_batch`; extend the 0228 refusal-table test with masked
cases.

Tests (committed):
- bitwise and Path parity of `map` and `map_reusing` for the masked shapes:
  all-null, all-valid, mixed and offset blocks; lengths
  0/1/63/64/65/1023/1024/1025; multi-chunk inputs; NaN payloads;
- pointer reuse for an exclusive donor; fallbacks for a clone, a slice, an
  oversized or undersized donor and a zero-length output;
- cancellation and refusal after `previous` is consumed;
- the scaled large-then-small retention test with a masked donor.

Gates:
1. local clippy, then core and adapter tests;
2. Miri for the reuse tests;
3. the instance guard and resolved identity;
4. SMALL exact against `8d104dd1`;
5. the 100M reuse-vs-plain fault capture;
6. 0224-harness timing for the six nullable cells;
7. host, integration, generated and cross, then the commit checker.
Publish only on all-PASS; the bench follower reports the six cells.

Apply the user-approved fault-scope correction recorded in ADR 0230. Seal
the measured binary and receipts; reject +1 output fault, a moved pointer,
missing phases/cases, or a failed warm timing target. Total faults remain in
the accepted report alongside the original failing receipt.
