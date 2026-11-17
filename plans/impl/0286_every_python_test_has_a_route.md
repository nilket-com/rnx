# Implementation plan 0286: Every Python test has a route

Status: Proposed

Paired ADR: ../adr/0286_every_python_test_has_a_route.md.

1. Pin 0bacd2c7. Add the Rust control that runs the controller's disk
   rule and `ci/disk_space.py` on the same planted paths and requires
   them to agree; run `ci/test_disk_space.py` and
   `ci/test_disk_launchers.py` in preflight.
2. Add the two stages of decision 3 to the Rust catalogue, and update
   by hand, for exactly those two stages, each witness fixture they
   change, marking it a reviewed ADR 0286 successor of the 0284 witness.
3. Retire the instruments and data of decision 4; extend the tombstone
   check; update the data registry.
4. Add `test_callgrind_n1_cold.py` to `TESTS` with its discovered count.
5. Add the route column and its Rust check with planted cases.
6. Run the affected checks in a final-like environment, preflight and
   the commit checker; sign; source review; one complete Rust final.
   Results state what the code does and the final's duration.
