# Implementation plan 0248: vector constants and constant diagnostics

Status: Proposed

Paired ADR: ../adr/0248_vector_constants_and_constant_diagnostics.md.

1. Pin 220246ad. Read constant handling in the execution frontend and
   lowering, the refusal-explanation path in src/session/domain_session.rs,
   and the three rows in tests/fixtures/session_parity.json.
2. Bring a short note before editing: which constant forms legacy accepts
   and the domain refuses (probe, do not guess), how each will be admitted
   and materialized, what is left out, and why the async text is reached.
3. Fix the refusal explanation first; it is independent of admission.
4. Admit the agreed constant forms. Fix the type diagnostic.
5. Also add the assertion carried from ADR 0247: every stock registration
   the host inventory marks refused has a specific wait reason, and no
   enrolled function is in the reason table.
6. Regenerate the pinned table, run the required stages, preflight and the
   commit checker. Bring the packet with before and after totals.

Results are added here by the implementer.
