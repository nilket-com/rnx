# Implementation plan 0272: a refusal names a remedy that works where it is shown

Status: Proposed

Paired ADR: ../adr/0272_a_refusal_names_a_remedy_that_works_here.md.

1. Pin 21e38e78. Read every place a session refusal is formatted, how
   the terminal session, the worker behind the notebook kernel and a
   `:dep` restart each create the session, how the kernel names are
   defined for installation, and the tests that pin refusal text.
2. Before editing, send a short note: the list of formatting sites;
   where the front end can be stated once and how it reaches them;
   the proposed notebook and neutral wording; whether any engine-crate
   text carries a remedy; and which pinned texts change.
3. Implement the decision.
4. Add the controls of the ADR.
5. Run a required final (complete if gate tooling changes), preflight
   and the commit checker. Bring the packet with the before and after
   text per front end and the per-stage durations. The candidate is
   frozen once the packet is sent.

Results are added here by the implementer.
