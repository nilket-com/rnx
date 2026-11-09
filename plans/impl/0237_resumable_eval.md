# Implementation plan 0237: Resumable invocation for one-shot eval

Status: Accepted

Paired ADR: ../adr/0237_resumable_eval.md. Implementation has not started.

1. Pin the published baseline and enumerate current eval fixtures and wrappers.
   Record existing explicit-legacy, async fallback, budget, Ctrl-C, diagnostics,
   formatting and lifecycle behavior. Inventory any trace option actually exposed
   by eval. Build a route-labeled corpus rather than claim all inputs migrate.
2. Review the concrete sliced-token ownership/state layout, operation-sharing
   boundary, PC checkpoint and host fuel bridge before editing production code.
   Preserve complete entry signatures and dispatch policy. Review whether result
   and empty-state conversion can be proved pre-effect for admitted eval forms.
3. Commit the approved plan, then implement token preparation, resume and terminal
   cleanup. Restore fixed bodies byte-for-byte. Add a child-module sliced copy and
   mandatory token guard across dispatch, transfer, enrichment and cleanup.
   Enumerate exact hooks and test rejecting mutations before host wiring.
4. Add a one-shot entry using shared session source preparation and presentation.
   Leave persistent Session::eval, REPL, worker and servers on their existing paths.
   Add the fresh sliced driver to src/execute.rs and wire only CLI eval. Retain
   typed pre-effect refusal and async legacy routing; never runtime retry.
5. Build exact paired binaries in isolated role targets. FIRST run the fixed-loop
   and operation-helper instance guard and resolved identity gate. Stop on any
   mismatch before trusting behavioral work; no inline-attribute repair rounds.
   After identity PASS, require exact fixed-route vectors on the eight existing
   CLI kernels at n=1/5, native off/on. Core tests and targeted Miri may precede
   the production build; they never override an identity/vector STOP. Sign the single impl and bind acceptance to that exact source.
   Stop on vector growth; no automatic optimization round or exception.
6. Run eval parity and focused signal/lifecycle controls, host/integration/
   generated/cross, updated inventory witnesses and commit checker. Produce a
   packet with admissions, intentional policy differences and exact evidence.
   Publish only after review/all required gates; no benchmark report is needed
   unless the scope later adds a measured performance claim.

## Required controls

- Slice sizes 1, 2, 3, 10,000 and larger than total; totals 0, 1, exact completion
  and one less. Changing the partition preserves sliced result/effects/fuel/error; complete
  parity covers opcode fuel, with host debits accounted separately.
- Suspend at branches, call/return, recursive calls, closures, collection mutation,
  output and host calls; resume the next operation once, never redo the previous.
- Native requested on/off gives the specified interpreted sliced policy and the
  same semantics; complete native fixed-run controls remain unchanged.
- Host work crosses soft allowance but cannot exceed hard allowance. Nested host
  budget handling cannot mint fuel; no TLS state remains installed while suspended.
- Cancel before execution, at Limited, after a host effect, and at completion;
  runtime error and exhausted-total races have the specified precedence.
- Pending executor cancellation survives refusal; dropping an unstarted/prepared
  token has no effects. Resume-after-terminal and zero-slice calls do not mutate.
- Drop/explicit close after several suspensions releases roots exactly once;
  allocation/retain/release failures retain diagnostics and cleanup failures.
  Executor reuse after complete/error/fuel/cancel does not inherit frames or fuel.
- Aliased mutation/output survives later failure; result publication does not.
  A failing effectful native operation is invoked once and cannot trigger fallback.
- Error locations and nested call chains are invariant under slice partition;
  wrapper offsets point to user input. Trace/event sequence is continuous.
- Real CLI eval: expressions, declarations plus expression, unit, returned Err,
  formatting failure, compile refusal, admitted runtime error, async fallback,
  explicit legacy, budget halt and Ctrl-C exit 130. Include deterministic host
  effect and signal synchronization fixtures, not timing-dependent sleeps.
- Persistent sessions/Jupyter/:dep and file-run routing retain their old behavior.

## Evidence discipline

Commit reusable test/harness code in this record; scratch receipts under target/0237.
Each receipt binds exact revision, command, declared environment and child result;
parent success requires both process success and semantic receipt success. Durable
per-stage receipts allow proven unaffected stages to be reused. No retry-to-pass,
no polling or progress watcher. Block on exit, read terminal results once.

Results: pending. Publication: not authorized at draft stage.

Host-budget ruling: the complete fresh host bridge does not reconcile ambient
host consumption with its local opcode fuel. Sliced execution now installs a
finite ambient scope around each host call, reconciles its consumption before
returning (also on host failure/panic), and restores the outer scope. No TLS
survives suspension. Sliced/complete parity is exact for opcode charging; sliced
execution additionally charges budget-aware host work. This deliberate difference
preserves legacy eval's bounded host-work contract. Arbitrary uncooperative host
code remains non-preemptible. Totals use the finite usize::MAX-1 cap, as legacy
eval does. A missing or increased ambient balance fails closed, never mints fuel.
The complete-path gap is deferred to its own record and fixed-route gates.


No implementation has started. Reuse archived tests as source, not inherited
acceptance receipts. Fix the angle-bracket identity selector with positive and
negative controls before its first use. Compare identical binaries as a control,
then require unchanged fixed instance counts and resolved machine-code identity.
No automatic correction round, profile change, helper duplication or exception.

Review addition: the mandatory Rust-token drift guard runs from ci/preflight.py,
not only as a unit test. An unmirrored edit must fail preflight before review.
Implement and review the guard/mutation controls alongside the source.
