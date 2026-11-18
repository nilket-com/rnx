# Implementation plan 0298: Fixed-route runs in Rust

Status: Proposed

Paired ADR: ../adr/0298_fixed_route_runs_in_rust.md.

1. Pin 7991e30d; generate the witnesses from the published Python with
   controlled values, before any change.
2. `build_environment`, `capture_environment`, the recipe, `capture_key`,
   `reuse_anchor` and `verified`; the stage wrapper for Python children.
3. `rnx-final stage fixed-route`: the sequence, retention and receipt.
4. Catalogue, scheduler, producer lists, README; retire the Python with
   tombstones; move the remaining Python cases to Rust controls.
5. The Rust suites, the Python discovery, preflight and the commit
   checker; sign; source review; one complete Rust final; Results.
