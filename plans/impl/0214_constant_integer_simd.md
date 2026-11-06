# Implementation plan: ADR 0214

Add safe word-slice wrappers for checked constant addition and both subtraction
orders. Use a separate const-generic Add/Sub primitive with explicit operand order
and no constant negation. Sharing the standalone addition primitive changed
its loop unrolling in the first diagnostic, so preserve its original callers. Resolve shapes
once per node/block inside the existing MASKED path. Preserve the unmasked path.

Add primitive boundary and whole-chain masked equivalence controls. Run format,
Clippy and focused tests before targeted Miri and the paired diagnostic. Verify
rescan execution for null-only overflow and retain bounded replay. Submit measured
targets before large captures, then collect proportional host gates and a
publication packet. No broad test reruns without a changed dependency or failure.

Measured correction: neutralize invalid input lanes to zero for constant addition
and to the constant for either subtraction order. This avoids null-only rescans;
valid overflow retains the existing bounded masked rescan. Keep constant-shape
dispatch out of line to protect existing pair/multiply loops. Measure both benign
blend cost and adversarial cost, plus zero protected-path growth. Verify zero
null-only rescans and a separate valid-overflow rescan witness.
