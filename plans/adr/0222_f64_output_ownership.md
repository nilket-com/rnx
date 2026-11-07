# ADR 0222: Write-only output for the pure-F64 plan driver

Status: Proposed

The pure-F64 plan driver (ADR 0221) still zero-fills its whole output before
computing, then overwrites every row. That pass writes 8 bytes per row a
second time. At 100M rows it costs 0.4375 Ir/row: glibc uses its
non-temporal memset path, 7 instructions per 128 bytes. At small sizes it
uses `rep stosb`, which Callgrind counts per byte, so the same pass shows as
8 Ir/row. Instruction counts therefore understate the cost at scale. The real
cost is a redundant 800 MB memory pass per 100M-row F64 output. ADR 0215
removed the same pass from the specialized flat paths, with descriptive
quiet timings of roughly 28% lower wall time.

Give the F64 driver write-only output through the existing `NumericOutput<f64>`
owner. Its length stays zero until `finish` checks that every row was written,
then publishes once. Add only safe writer methods that copy from fully
initialized sources: a block of result bits, a constant fill, and a small
initialized staging array for mixed validity words. No new `unsafe` and no
`MaybeUninit` exposure beyond the existing owner. Writes happen in row order,
once per block. Partial execution, panic, cancellation and refusal publish
nothing and drop the allocation cleanly.

Make the allocation change at the existing per-call batch preparation point,
selected by the plan classification. Reserve capacity instead of
zero-filling. Do not change the layouts of `PreparedBatch`, `BatchKind`,
`PreparedToken` or `ColumnValues`. Boolean, generic and specialized paths are
unchanged. The Boolean bitmap output (one bit per row) is out of scope.

Acceptance against published `7c51bf60`:
- results are bitwise identical, including NaN bits on x86-64 and the null
  payload of +0.0;
- every non-F64-plan shape keeps an exact protected function vector, apart
  from reviewed fixed pins;
- F64 plan whole-map Ir must not increase at any measured size, and the
  output-fill edge disappears;
- targeted Miri (deterministic floats for the exact suite) covers the
  writer and drop paths;
- descriptive quiet timing at 100M, first and warm, is reported next to the
  0221 timing. It is not a gate and supports no statistical claim.
