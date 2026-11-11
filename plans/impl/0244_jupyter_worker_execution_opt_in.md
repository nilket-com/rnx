# Implementation plan 0244: opt-in Jupyter worker on the program domain

Status: Proposed

Paired ADR: ../adr/0244_jupyter_worker_execution_opt_in.md.

1. Pin 76e376a3. Read src/worker.rs, Session::eval_with_armed in
   src/session.rs, src/session/domain_session.rs and interactive.rs, and
   jupyter/src/{main,install,kernel,worker}.rs.
2. Bring a short boundary note for review before editing:
   - the engine-neutral operation and where arming sits in DomainSession;
   - how the worker and kernel receive the engine choice, and the
     kernelspec shape for the opt-in kernel;
   - the exact ready addition and why protocol-1 readers accept it;
   - the test table format and the stages the path table selects.
3. Implement arming on the domain session, the engine-neutral operation and
   the worker and kernel plumbing.
4. Add the table, the controls and the kernel-level test.
5. Run the required stages, preflight and the commit checker. Bring the
   packet to review.

Results are added here by the implementer.
