# Implementation plan: ADR 0215

Introduce a private numeric output owner selected only when preparation proves
specialized, unmasked flat execution. Preserve initialized allocation and
execution for generic/masked/Bool paths. Reserve the same capacity and account
for it without making unwritten elements visible through Debug or slices.

Provide write-only primitive destinations for I64Add, Fused, F64Add and F64Pair.
Use separate write-only entries while preserving the published ADR 0216
protected bodies and callers. Share arithmetic source where this preserves
the protected code generation; prove shapes before capture. Prove complete block and
tail stores for Portable and AVX2 before advancing execution; finalize the
length-zero Vec once after the existing final cancellation check. Retain
original-input replay and discard every incomplete output.

Add owner lifecycle/Miri, arithmetic boundary, tail, memory-limit, fuel and
cancellation controls, plus Arrow pointer-transfer coverage. Check protected
initialized paths explicitly. Review source, run cheap prerequisites and
targeted Miri, then paired small diagnostics. Present net instruction changes
and removed fill edges before agreeing targets for 10M/100M captures. Follow
with quiet serial 100M wall-time reporting and proportional host gates. Reuse
durable receipts only with the required source/recipe identity proof.

Restack on c213c23590f8efa44630c56add02e17d1ae10f61 and measure against
both this published ADR 0216 baseline and original 99790566. Fold the scoped
0216 result into the final benchmark report: its 4096/65536 controls passed;
10M/100M were not recaptured for that prerequisite.
