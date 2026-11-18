# Implementation plan 0296: Fixed-route policy, scoring and anchor cache in Rust

Status: Implemented

Paired ADR: ../adr/0296_fixed_route_policy_and_anchor_cache_in_rust.md.

1. Pin 96a809fe; generate the band and cache witnesses from the
   published Python with controlled values, before any change.
2. `tools/final/src/fixed_route.rs`: policy, transition, approval, source
   condition, index, compare, recipe, score; the anchor cache.
3. `tools/final/tests/fixed_route.rs`: the witness comparisons,
   interchangeability and mutations.
4. The Rust suites, the Python discovery, preflight and the commit
   checker; sign; source review; the required final; Results.

## Results

`tools/final/src/fixed_route.rs` ports `drift_band.py` and
`anchor_cache.py`. Policy validation iterates the inventory as Python
does, so only a list of selector dicts passes. The anchor transition and
its approval run git through `child::git_command` and require an
ancestral, signature-verified, declared `plan: adr` commit. The source
condition passes the guard only string arguments. Counts are integers
only, and the bands use exact signed 128-bit arithmetic. The measurement
recipe and the score keep Python's shapes. The anchor cache holds
`flock` on the same `LOCK`, keeps the same `OWNERS.json` (schema 1) and
`ANCHOR.json` (schema 2), and checks the journal's structure (a list of
rows, each with an id and a state, and a current id whenever there is a
row) before any recovery or write. It
refuses ownership paths with a parent component before resolving them,
removes entries only after an identity check, and requires a records
list before checking row bindings. Nothing calls the port yet; ADR 0297
moves the orchestrator onto it.

Three witnesses from the published Python at 96a809fe are matched
exactly:
- the band: 83 cases, including exact limits, counts near 2^62,
  decreases and malformed inventories;
- the git checks: 19 cases over a repository signed with an ephemeral
  key and its own allowed-signers file, needing no host key, agent or
  trust configuration;
- the cache: 23 scenarios in 77 steps, comparing results, journals and
  whole trees through publication, damage, interrupted publication on
  either side of the rename, malformed journals and bundles, a stale
  row without a state, a populated journal without a current id, and
  parent-component paths.
A two-way handoff with the live Python cache shows each side publishing
for the other, lock exclusion in both directions, and each side
recovering the other's interrupted publications. Each refusal family
has a mutation that fails its control. Run once, not committed, the
Rust approval accepts the repository's own signed transition (plan
c909088d, from its parent to 96a809fe), as the Python does.

Review held the record three times: for malformed inventory, journal
and bundle lists and parent-component paths; for a stale row without a
state; and, at publication, for a populated journal without a current
id, a check the state repair had dropped. The required final passed in
2 min 16 s (`clippy-final`, `commit-discipline`, `controller-controls`,
`final-controls`, `format-final`, `python-list`).
