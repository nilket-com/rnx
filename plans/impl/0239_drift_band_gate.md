# Implementation plan 0239: Drift band gate and path-derived required stages

Status: Accepted

Paired ADR: ../adr/0239_drift_band_gate.md. Implementation has not started.

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

Results: pending. Publication: not authorized at draft stage.
