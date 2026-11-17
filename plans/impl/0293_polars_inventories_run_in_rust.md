# Implementation plan 0293: Polars inventories run in Rust

Status: Proposed

Paired ADR: ../adr/0293_polars_inventories_run_in_rust.md.

1. Pin fe95b78e and generate the witnesses from the published Python,
   with controlled values, before any change.
2. The `inventories` stage mode, its validator and receipt, over an
   interceptable runner, using the shared `stage::build_jobs`.
3. Catalogue, scheduler, producer lists, README; retire both Python files
   with tombstones.
4. The Rust suites, the Python discovery, preflight and the commit
   checker; sign; source review; one complete Rust final; Results.
