# ADR 0233: AVX2 packed I64 comparisons

Status: Accepted

The predicate driver compares I64 columns one lane at a time: a scalar `cmp`
and `sete` per row, then bit packing. At 10M rows that is 6.58 of the 7.07 Ir
per row for flat `i64eq` (82–93% of the call across layouts). At 100M rows the
warm plain map takes 71–86 ms. In ADR 0224 (a different day; context only),
Arrow took 54.3 ms and single-thread Polars 33.1 ms for the flat case. An
SSE2 `pcmpeqd` pairing lost to the bounded scalar form in ADR 0223.

Decision: add separate AVX2 I64 comparison kernels, chosen by the existing
once-per-call `Engine` detection.
- `vpcmpeqq` covers `==` and `!=`; `vpcmpgtq` with operand order or inversion
  covers `<`, `<=`, `>`, `>=`. A movemask packs the results directly into
  output words.
- The kernels are new, non-generic primitive symbols, with `unsafe` confined
  to the primitives module behind the runtime AVX2 check and length asserts.
  The portable path, SSE2 code, existing initialized primitives, shared
  executor helpers and Boolean/F64 drivers are unchanged. No new generic
  instantiations.
- Semantics are unchanged: strict nulls, invalid rows false, and tail masking.

Acceptance against published `845572cb`:
- output bitmaps and validity are bitwise identical to the scalar path for
  all six I64 comparisons, across the 0226 layouts and lengths
  0/1/3/4/5/63/64/65/1023/1024/1025, including signed extremes;
- `force_scalar` is bitwise and Path exact; the existing scalar comparison
  symbol is exact under resolved identity. Whole-map force-scalar vectors allow
  only explicitly reviewed fixed setup, never per-row or per-block growth.
  Measure raw exact vectors first and stop on any slope;
- preserve driver-level `Path::Scalar`; primitive dispatch is observed directly
  by tests, without changing the public observation contract;
- every other protected path has exact program vectors, apart from reviewed
  fixed per-call pins. The instance guard and resolved identity pass;
- changed I64 comparison paths cost less total Ir per row at 64K and 10M;
- in the ADR 0224 harness at 100M (5 process samples, warm), `i64eq` is
  faster in all four layouts. First calls are reported;
- the portable and wrapper tests are Miri-clean, and the AVX2 kernels have
  hardware parity tests.

Stop rule: any per-row or per-block growth on an unchanged path stops the
work for attribution, with no codegen-tweak loop; a rule exception needs the
user.
