# ADR 0221: Pure-F64 column plans run in their own driver

Status: Accepted

Multi-operation F64 expressions such as `x*2.0+y` run through the generic
scalar executor. At 100M rows, warm whole-map cost is 104.03 Ir/row with no
nulls and 126.55 with mixed nulls. A single F64 operation (`x+1.0`, `x+y`)
already has specialized paths at 0.78 to 4.55 Ir/row. ADRs 0218 and 0219
showed that changing or instancing the shared generic executor perturbs its
existing paths. ADR 0220 showed that a separate whole-plan driver, chosen
once per call, isolates cleanly.

Route pure-F64 plans with two or more arithmetic operations to a separate
driver in its own module. A plan qualifies when every computed node is F64
`+`, `-` or `*` over F64 inputs
and constants. Single-operation plans keep their specialized paths. The driver
evaluates the whole plan in blocks of typed f64 lanes, with typed scratch for
intermediates, over every lane regardless of validity. It then applies the
established null payload rule once per block, before the single output write.

Results must be bit-identical to the generic executor for every lane: the
same operation order, IEEE semantics, NaN and signed-zero propagation, and
no fused multiply-add or reassociation. Validity, fuel, cancellation,
refusal, payload, padding and path observability match the existing
contracts. NaN payload and sign equality is verified natively on x86-64, the
supported target; Rust does not guarantee it across platforms, so no
cross-platform payload claim is made. Invalid output lanes contain zero bits (`+0.0`). Division is
unsupported by the existing column planner and remains out of scope.

Do not edit the generic executor or its callees, and add no new callers to
them or to the protected SIMD kernels. The new driver's loops are its own,
written in a source-explicit block shape (the ADR 0216 lesson) so their
vectorization does not depend on surrounding code. Classification happens
inside the existing preparation pass.

Controls must cover the following:
- asymmetric and offset validity;
- misaligned chunks;
- empty, all-null and tail-only inputs;
- the same column used twice;
- constants on either side;
- NaN, infinities, signed zero and subnormals;
- deep chains and output reuse;
- exact bitwise equality with the generic executor;
- fuel and cancellation.

Acceptance against the published ADR 0220 head:
- every non-qualifying shape keeps an exact program function vector, apart
  from reviewed fixed per-call pins;
- protected SIMD shape and hot-loop counts are unchanged;
- the new kernel's own block shape is recorded and gated;
- F64 chain targets are set after the small diagnostic, then confirmed at
  10M/100M first and warm.
