# Implementation plan 0296: Fixed-route policy, scoring and anchor cache in Rust

Status: Proposed

Paired ADR: ../adr/0296_fixed_route_policy_and_anchor_cache_in_rust.md.

1. Pin 96a809fe; generate the band and cache witnesses from the
   published Python with controlled values, before any change.
2. `tools/final/src/fixed_route.rs`: policy, transition, approval, source
   condition, index, compare, recipe, score; the anchor cache.
3. `tools/final/tests/fixed_route.rs`: the witness comparisons,
   interchangeability and mutations.
4. The Rust suites, the Python discovery, preflight and the commit
   checker; sign; source review; the required final; Results.
