# Implementation plan 0243: :dep restart keeps the session engine

Status: Proposed

Paired ADR: ../adr/0243_dep_restart_keeps_engine.md.

1. Pin fa46bd02. Read src/dep_transition.rs (prepare, flags, finish),
   src/repl.rs (the :dep branch and its refusal), the REPL argument parser
   in src/lib.rs, and the project session launcher.
2. Carry the engine choice as the existing enum through prepare, the
   transition result, finish and the replacement arguments. Parse it once
   per process boundary; reject anything else.
3. Give the project session launcher the same option and pass it through.
4. Remove the execution-session refusal of :dep and :depv only after steps
   2 and 3 pass their tests.
5. Add the PTY tests and the negative control from the ADR. Update README.
6. If a tools/column file changes, also make the fixed-route stage resolve
   Valgrind the way the gate does, or fail at selection. Otherwise leave it.
7. Run the required stages, preflight and the commit checker. Bring the
   packet to review.

Results are added here by the implementer.
