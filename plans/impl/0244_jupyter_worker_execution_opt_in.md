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

## Reviewed boundary

The reviewer changed domain arming to the legacy point: size/ceiling admission,
interrupt clear, retain/number input, then armed before compilation. Every numbered
refusal therefore has the same armed/settled shape and input identity as legacy.
A failed armed send compiles and executes nothing. The interrupt-before-armed
control is after the single clear; a flag arriving then survives to execution.
This supersedes the ADR's earlier wording about refusal before armed.

InteractiveSession owns worker rendering and returns owned bounded text/HTML or
Failure. Legacy HTML behavior remains unchanged; plain-only applies to domain.
Domain notes drain before the stderr barrier. The execution-only readiness fields
are engine=execution and capabilities html=false, async=false. Existing protocol-1
readers accept the addition; the selected supervisor rejects mismatched capability
or engine claims. Default ready and default kernelspec remain unchanged.

The independent rnx-execution kernelspec carries --engine=execution. Kernel restart
reuses it. Existing public legacy launch/install entry points remain wrappers.
The existing worker inventory row changes in place, along with the two stale
0243 descriptions. The path table now requires Jupyter package and installed-kernel
tests for jupyter-only edits. Fixed-route Valgrind resolves explicit/configured
roots or PATH and fails before expensive stages if unavailable.

## Comparison coverage

Equal: scalar/unit results, aliases/closures, stdout/stderr association, armed and
settled identities, reset, shutdown, ceiling refusal. Wording: compile/runtime
text, with category and structured origin compared exactly (including old closure
origin). Policy: async refusal with unchanged domain state, HTML unavailable;
legacy async behavior remains asserted. Native admission has an armed callback
control proving neither host function nor presenter executes. Lost armed transport
is tested on real pipes and by reusing the same domain session afterward.
Before/after-armed interruption controls leave the worker usable.

Existing worker tests remain unmodified. The parent fixture only gains an optional
engine parameter; its existing callers retain their original argv. Installed-kernel
validation installs into temporary directories, executes, restarts, proves empty
state and continued domain refusal, and compares default kernel JSON.
