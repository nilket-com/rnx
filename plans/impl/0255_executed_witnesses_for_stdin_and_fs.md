# Implementation plan 0255: executed witnesses for standard input and the file system

Status: Proposed

Paired ADR: ../adr/0255_executed_witnesses_for_stdin_and_fs.md.

1. Pin 5ed9692b. Read how the host rows get their class and input in
   src/session/parity.rs and src/session/host_enrollment.rs, the existing
   child witnesses, and the thirteen functions.
2. Before editing, send a note: the witness form and the child setup, the
   path marker rule, the calls chosen for each function with their
   expected legacy outcomes, and the outcomes of a first comparison of the
   thirteen. If any comparison shows a wrong result in the execution
   engine, send that and stop.
3. Add the witnesses, class the rows by their comparison, and regenerate
   the pinned table.
4. Add the controls of the ADR.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the class of each of the thirteen rows, the totals before
   and after, the selected stages and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
