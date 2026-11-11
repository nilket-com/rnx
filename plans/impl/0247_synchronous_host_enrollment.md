# Implementation plan 0247: synchronous host functions on the new engine

Status: Proposed

Paired ADR: ../adr/0247_synchronous_host_enrollment.md.

1. Pin 10e71b7b. Read the host bridge and its certificates, host_policy in
   src/routing_inventory.rs, the 27 refused rows in
   tests/fixtures/session_parity.json, and the legacy registrations of the
   22 functions.
2. Bring a boundary note before editing: for each of the 22, the input and
   output shape it needs and whether the bridge has it; the list of missing
   shapes with ownership rule, size and whether the fixed operation loop or
   its inventory is touched; the functions each shape unlocks; and a
   proposed cut if the whole set is too large for one record.
3. Implement the agreed shapes and enrollments, cheapest first.
4. Add executed witnesses and regenerate the pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the before and after class totals.

Results are added here by the implementer.
