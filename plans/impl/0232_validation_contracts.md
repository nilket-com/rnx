# Implementation plan: ADR 0232

Status: Accepted

1. Freeze the controller-edge inventory and distinguish raw commands from children
   that own terminal receipts. Record explicit receipt locations/schema adapters;
   no implicit fallback from failed receipt validation to raw-command mode.
2. Add tools/column/gate_policy.py: derive record range from its plan, compute
   required gates, create the canonical selection manifest and validate final
   evidence. Integrate accept/chain/resume/recover_cross; mark development scopes
   as development, not final acceptance. Include inherited gates when validating
   recovery so a subset cannot silently drop cross.
3. Add tools/column/child_result.py backed by existing atomic_json/Run durability.
   Implement context creation, bound producer completion, terminal validation,
   explicit leaf/sidecar adapters, artifact hashing and fail-closed error receipts.
   Thread this contract through the inventoried parent and producer entry points.
   Catch launch/setup failures as well as child failures. No polling or retries.
4. Add isolated fake-child and Git-repository controls. Test exit 0 with every bad
   receipt, exit nonzero with PASS, stale source/invocation, invalid artifacts,
   interruption/resume, source renames/deletions and omitted or spoofed cross.
5. Rename the colliding column test_protocol module and run aggregate discovery
   with the measure-import contamination reproduced. Preserve all test cases.
6. Run affected CI/tool suites, small real raw-command and receipt-controller
   smoke tests, and one cross integration. Bind receipts to the final source and
   show production runtime diff empty. No long numerical/performance gates.
7. Remove completed backlog items, run hygiene and the unchanged commit checker,
   sign the single implementation and send the final review packet.

Required-cross triggers include root and rune Cargo.toml/Cargo.lock,
tools/verify.py and tools/verify/**, as well as src/ and rune/crates/.
A general environment or scheduler redesign is out of scope.
