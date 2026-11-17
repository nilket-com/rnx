# Implementation plan 0294: The cross launcher runs in Rust

Status: Proposed

Paired ADR: ../adr/0294_cross_launcher_runs_in_rust.md.

1. Pin 152b0c07 and generate the witnesses from the published Python,
   with controlled values, before any change.
2. The pin checks of `tools/engine_source.py` in Rust, with their
   controls.
3. The `cross` stage mode in its own module: entry points, sequence,
   owned target, terminal receipt and census evidence.
4. The verifier's launcher binding: the executable identity in the
   request and its check before and after; the wiring comparison on the
   launcher module.
5. Catalogue, trigger, producer lists, README; retire the Python files
   with tombstones and route updates.
6. The Rust suites, the verifier's own build, the Python discovery,
   preflight and the commit checker; sign; source review; one complete
   Rust final; Results.
