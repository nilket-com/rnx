# ADR 0216: Explicit SIMD kernel blocks

Status: Accepted

Stabilize the protected AVX2 add_constant, fused, add_pair and zero_null kernels
before resuming ADR 0215 output initialization work. Baseline is published
99790566bbf72e1b1431df95fd0a6e4e89ad7e0b. ADR 0215 remains parked; this record
changes no output ownership, initialized wrapper interfaces, VM hooks or inputs.

ADR 0215 showed that unchanged kernels can acquire different unrolling under
changed module composition. Explicit source blocks are a measured experiment,
not a guarantee of compiler stability. Use two four-lane operations per main
iteration (eight lanes), a bounded four-lane remainder, then scalar zero to three
lanes. Start with the existing single overflow accumulator. Retain checked
arithmetic, null masking, target-feature detection and inline-never boundaries.
Do not add optimization attributes, barriers or black-box operations to steer
measurements. Add no dependencies.

Validate lengths zero through seventeen, block boundaries, extrema, both signs
of constants, overflow positions, masks, offsets and output guards. Preserve
Portable/AVX2 equivalence and row/PC replay, fuel and cancellation behavior.
Run cheap prerequisites and focused tests before targeted Miri and paired builds.

The decisive performance rule is program instructions no greater than the
published baseline on every protected path at both 4,096 and 65,536 rows,
first and warm calls. Retain existing object accounting and applicable ceilings.
Cover the standard numeric/Boolean layout matrix, generic integer constant
chains, generic floating-point chains and masked floating-point subtraction and
multiplication. Attribute library costs separately; do not substitute instruction
shape or whole-process improvement for the protected program comparison.

Compare protected assembly shapes across two exact recorded compositions:
this candidate alone and this candidate with an inert extra kernel compiled in.
Verify that the extra kernel is retained in the second binary without changing
measured script behavior. This control demonstrates only the tested composition
stability; it is not proof of insensitivity to every future change. Preserve both
binary hashes, recipes and the perturbation diff. Missing or ambiguous symbols
fail closed; resolve aliases explicitly rather than comparing the wrong body.

Make one measured attempt. Report any protected regression for a decision;
do not iterate or relax the rule automatically. Publish only after reviewed
all-PASS evidence and proportional host checks. Then restack ADR 0215 and
measure it against both the original baseline and published ADR 0216.
