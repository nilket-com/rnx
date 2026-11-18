# Implementation plan 0306: A native value from a fallible producer

Status: Proposed

Paired ADR: ../adr/0306_a_native_value_from_a_fallible_producer.md.

1. Boundary note, read-only: the widened signature rule, the host
   bridge's cold output path for `Result<T, String>`, the input
   contracts for `read_csv`, its bounds, and every inventoried item
   touched with its exact hashes.
2. Record the old engine's REPL outputs for every control first.
3. The signature rule and the cold output import; then the `read_csv`
   enrollment; the early fixed-route screen. Stop on a band failure.
4. Session, bound, charge, registration, refusal, routing and inventory
   controls; mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.
