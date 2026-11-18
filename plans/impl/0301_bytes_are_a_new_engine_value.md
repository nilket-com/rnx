# Implementation plan 0301: Bytes are a new-engine value

Status: Proposed

Paired ADR: ../adr/0301_bytes_are_a_new_engine_value.md.

1. Read the store, value and lowering code and the C16g oracle. Report
   the representation choice and every protected hook and executor arm
   for review, and have the user's authorization for the protected run
   edit, before production edits.
2. The type and the store payload alone; the fixed-route band. If it
   fails, stop for a ruling (variant-count-independent tag dispatch
   first, or park again).
   Then byte-string lowering, constants and the receiver certification.
3. The oracle's families: compare, index, methods, mutation, display,
   boundary and budget.
4. The host boundary in both directions, and session state, import and
   export.
5. Promote the oracle to enforced; session parity rows, census and
   inventory; store controls; mutations.
6. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.
