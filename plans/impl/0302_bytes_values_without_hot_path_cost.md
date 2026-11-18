# Implementation plan 0302: Bytes values without hot-path cost

Status: Proposed

Paired ADR: ../adr/0302_bytes_values_without_hot_path_cost.md.

1. Boundary review of the three pieces and any inventoried item they
   touch, before production edits.
2. The type and payload base, re-screened on this baseline.
3. Pieces A, B and C, each built on the base and screened alone; stop on
   any failure.
4. The combination, screened.
5. The oracle within scope, the piece controls and mutations.
6. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.
