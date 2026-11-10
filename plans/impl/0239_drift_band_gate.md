# Implementation plan 0239: Drift band gate and path-derived required stages

Status: Accepted

Paired ADR: ../adr/0239_drift_band_gate.md. Implementation complete; publication awaits review.

1. Pin 3fe44bf9. Read gate_policy.py, accept.py, child_result.py,
   executor_identity.py, fixed_route_vectors.py, fixed_route_exception.py, the
   0238 cross-program gate and Miri runner, ci/execute_guard and the protocol
   v2 accounting. List which pieces are record-specific and which are shared.
2. Bring a boundary note for review before editing:
   - the fixed-route source inventory and how items are fingerprinted across
     revisions;
   - the policy files (limits, kernels, inventory, anchor, approval binding)
     and what the checker verifies mechanically versus what review attests;
   - how anchor evidence is captured, keyed and judged stale;
   - the exact command and feature recipe of every required stage, the path
     table, and the fixed-route receipt contract in accept.py/child_result;
   - the Miri runner interface, key composition and receipt layout.
3. Implement the source condition, the band and cap tool, the policy files and
   anchor evidence. Wire them into one reusable fixed-route gate so the next
   engine record needs no new gate script.
4. Replace gate_policy.required() with the path table and enforce required
   stages and the fixed-route receipt at finalization.
5. Apply target/0239/miri-cache.patch and generalize it into the single Miri
   runner described in the ADR.
6. Capture the anchor evidence for 3fe44bf9 under the declared recipe.
7. Run the controls below, the required stages for this record's paths,
   preflight and the commit checker. Bring the packet to review.

## Required controls

- Band: each inequality exactly at, one Ir over and well below; decreases;
  every kernel and native policy; a missing fixture; outputs differing;
  B5 <= B1; recipe, toolchain or CPU mismatch.
- Cap: exceeded while the band passes; identity PASS without anchor evidence;
  stale anchor evidence; anchor change without an approval record, with a
  bare string, and with a record outside the ancestry.
- Source condition: fixed-loop edit with identical machine code; equal edit
  to both copies; inventory item added, removed or duplicated.
- Replay of retained 0237 and 0238 vectors: per-record verdicts recorded.
- Required stages: 0237-shaped paths require root-tests; added, removed and
  renamed paths; manifest and lockfile changes with before and after graphs;
  docs-only requires no cargo stage; an empty suite fails; deselecting a
  required stage or omitting the fixed-route receipt fails acceptance.
- Miri: real prepare, test and restart smoke; kill after some tests pass and
  resume; corrupted artifact; changed source digest, toolchain, MIRIFLAGS or
  features; inventory that disagrees with the live list; an external path input.

## Evidence discipline

Commit the tools and their tests. Scratch receipts under target/0239. Message
Claude only for design questions, gate failures and the packet, as requests.

Results: source inventory, tooling controls, retained W/T replay, real Miri
prepare/restart and fresh anchor capture PASS. Final acceptance receipt is
recorded in the publication packet. Publication requires review.

## Reviewed boundary (seq 30251)

Apply the two exact integer inequalities to BOTH program W = rnx_Ir +
other_Ir and raw total T = W + dynamic_Ir, at 5/1000 per record and 10/1000
against the anchor. This supersedes the ADR draft reference to the column
per-row libc allowance: CLI n=1/5 are workload scales, not row counts. Dynamic
totals and deltas remain visible. Neither metric may fail. Retained 0237 and
0238 replay passes both metrics; it is per-record evidence, not cumulative
anchor evidence. The source inventory runs before identity.

The reviewed boundary permits generalized cache regeneration only after
explicit invalidation of corrupt artifacts; never silently trust a corrupt
PASS. Published 0238 stays unchanged.

Review correction (seq 30258/30261): anchor transitions bind their old anchor
to the baseline policy, supporting successive links. Measurement cache keys
exclude scoring code and bind the measurement matrix/limits/anchor instead.
The builder itself applies the declared environment allowlist. An anchor
capture collects vectors without a redundant self-identity comparison; real
record comparisons still use the existing identity gate.

Development evidence: tooling controls and source inventory PASS; real short
Miri preparation/test and independent restart PASS, with preparation and test
reused on restart. Initial anchor attempt stopped before capture on the old
identity resolver's unsupported label. Corrected anchor capture PASS after source review; anchor self-score passes
both W and T inequalities. Final acceptance uses the mandatory tooling stages
on the clean signed implementation. No product-runtime sources changed.

Review correction (seq 30281): builds use the resolved pinned toolchain PATH
and fixed C locale/UTC. Anchor capture keys bind rebuilt binary bytes and
execution inputs, not the build environment. Each gate rebuilds the anchor;
identical bytes permit reuse only after checking capture inputs and artifacts.
Legacy capture import verifies its preserved manifest and full run environment,
retains the original receipt, and records the current build and import proof.
Different bytes or measurement inputs require a new capture.
