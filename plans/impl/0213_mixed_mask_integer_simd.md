# Implementation plan: ADR 0213

Implement optimistic checked I64 SIMD in generic mixed-mask nodes, with one
masked scalar rescan per flagged node and final null output zeroing. Reuse
existing primitives and add checked subtraction at the audited boundary.
Keep portable paths and exact replay, fuel, cancellation and fault semantics.

Add focused primitive and whole-chain equivalence controls, including null-only
later-node overflow and output reuse. Run committed prerequisites, targeted
Miri and host controls before a small durable diagnostic. Review numerical
targets before large captures. Record adversarial flagged-block costs separately.
