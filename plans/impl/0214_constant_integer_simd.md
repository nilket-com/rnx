# Implementation plan: ADR 0214

Add safe word-slice wrappers for checked constant addition and both subtraction
orders. Reuse the existing addition primitive without changing it; introduce
subtraction with explicit operand order and no constant negation. Resolve shapes
once per node/block inside the existing MASKED path. Preserve the unmasked path.

Add primitive boundary and whole-chain masked equivalence controls. Run format,
Clippy and focused tests before targeted Miri and the paired diagnostic. Verify
rescan execution for null-only overflow and retain bounded replay. Submit measured
targets before large captures, then collect proportional host gates and a
publication packet. No broad test reruns without a changed dependency or failure.
