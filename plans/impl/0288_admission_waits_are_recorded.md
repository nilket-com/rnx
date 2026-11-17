# Implementation plan 0288: Admission waits are recorded

Status: Proposed

Paired ADR: ../adr/0288_admission_waits_are_recorded.md.

1. Pin 8092c4b5. Split `scheduler::ready` into the gate it fails
   (producers, lanes with the stage, alone), without changing its
   verdict, and name the gate each admission branch returns on.
2. A schedule recorder in the controller: per stage, a reason written
   only on change, its start and end; `SCHEDULE.json` written with the
   other terminal records and bound into the verdict.
3. Revalidation and comparison: the binding, the stage set and the
   order checks; times run-specific in the comparison.
4. The planted-schedule controls through the controller's existing test
   seams, and the tamper controls.
5. Run the affected checks in a final-like environment, preflight and
   the commit checker; sign; source review; one complete Rust final.
   Results state what the code does, the final's duration, and what its
   schedule record shows of its critical path.
