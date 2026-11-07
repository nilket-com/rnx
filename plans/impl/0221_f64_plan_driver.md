# Implementation plan: ADR 0221

Follow ADR 0220's structure. Add a classification bit in the existing
preparation pass. Route from `invoke` once per call. Give the new module its
own chunk cursors, typed f64 block scratch and output write, and reuse no
generic executor functions or protected SIMD kernels. Write the arithmetic
loops in an explicit fixed-width block shape and record their disassembly as
the new kernel's shape baseline.

Reuse the ADR 0220 exact-vector tooling (`boolean_*` generalized, or an
`f64_plan_*` sibling) and its pin mechanism against published 0220. Run
targeted Miri on the safe boundaries, native controls including a bitwise
equality sweep against the generic executor, the small paired diagnostic for
target review, the 10M/100M matrix, then the host, integration, generated and
cross gates and the commit checker. Committed tools contain no home paths and
no agent-infrastructure calls.
