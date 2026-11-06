# Implementation plan: ADR 0209

Implement a root Python checker derived from ket's metadata and lineage
validation, with rnx's explicit policy adaptations. Keep the implementation cap
in one constant. Verify both fixed adopted history segments, their signed
metadata, and the exact import boundary; require unique post-import record
numbers across both histories. Recognize only the pinned historical follower
record rather than granting a generic missing-ADR exemption.

Validate complete committed trees, first-parent plans, paired document paths,
opening ADR status and plan-only scope. Reject unreviewed merges, replacement
objects and shallow clones. Emit a machine-readable PASS/FAIL receipt. The
normal CLI has no base override. Tests may supply isolated policy fixtures.

Replace the imported engine records entry point with a delegating wrapper.
Run root checker controls and the real-history check before host compilation,
and preserve the engine prerequisite's existing early check. Document the
policy and the concurrency rule at the root. Keep historical checker sources
available in history; do not maintain two active implementations of the rule.

Test negative cases using temporary repositories and controlled signing keys,
including a valid two-parent import, a tampered import tree and a second-parent
bypass. Run the checker on the signed plan and final implementation, source
format checks and integration controls. Obtain source review before publication;
report receipt hashes and ancestry without rerunning unchanged runtime tests.
