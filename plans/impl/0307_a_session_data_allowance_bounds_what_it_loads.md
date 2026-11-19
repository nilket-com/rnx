# Implementation plan 0307: A session's data allowance bounds what it loads

Status: Proposed

Paired ADR: ../adr/0307_a_session_data_allowance_bounds_what_it_loads.md.

1. Boundary note, read-only: which operator setting supplies a finite
   Store allowance and its default, how reset keeps it, how the cold
   owned input carries the remaining budget to the producer, and any
   inventoried item touched.
2. The scaled measurement design for the parser estimate (sizes, shapes,
   margin, held-out validation), reviewed before measuring.
3. The allowance and its carriage; then `read_csv`'s derived bounds; the
   early fixed-route screen.
4. Scaled parity, over-allowance, estimate, reset and charge controls;
   mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.
