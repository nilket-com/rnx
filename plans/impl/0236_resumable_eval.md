# Implementation plan 0236: Resumable invocation for one-shot eval

Status: Withdrawn

Withdrawn: the macro extraction changed fixed interpreter machine code and
introduced workload-dependent instruction costs. No production or tooling
changes ship. The proposal below records the experiment, not shipped behavior.

Paired ADR: ../adr/0236_resumable_eval.md. Implementation has not started.

1. Pin the published baseline and enumerate current eval fixtures and wrappers.
   Record existing explicit-legacy, async fallback, budget, Ctrl-C, diagnostics,
   formatting and lifecycle behavior. Inventory any trace option actually exposed
   by eval. Build a route-labeled corpus rather than claim all inputs migrate.
2. Review the concrete sliced-token ownership/state layout, operation-sharing
   boundary, PC checkpoint and host fuel bridge before editing production code.
   Preserve complete entry signatures and dispatch policy. Review whether result
   and empty-state conversion can be proved pre-effect for admitted eval forms.
3. Commit the approved plan, then implement token preparation, resume and terminal
   cleanup. Share opcode semantics at source level; keep fixed loops free of
   runtime slicing state. Add behavior tests before host routing changes.
4. Add a one-shot entry using shared session source preparation and presentation.
   Leave persistent Session::eval, REPL, worker and servers on their existing paths.
   Add the fresh sliced driver to src/execute.rs and wire only CLI eval. Retain
   typed pre-effect refusal and async legacy routing; never runtime retry.
5. Build exact paired binaries in isolated role targets. FIRST run the fixed-loop
   and operation-helper instance guard and resolved identity gate. Stop on any
   mismatch before trusting behavioral work; no inline-attribute repair rounds.
   Then validate focused core controls, targeted Miri and unchanged fixed-route
   exact vectors. Sign the single impl and bind acceptance to that exact source.
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

Commit reusable test/harness code in this record; scratch receipts under target/0236.
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

Development source now includes the sliced token, shared lexical operations,
sliced-only host budget scope, explicit one-shot session entry and CLI flags.
Persistent Session::eval remains legacy. New CLI controls cover values, output,
errors, async refusal, route disclosure, bounded loops and synchronized Ctrl-C.
Executor controls passed (172 tests, 1 ignored) and host wiring type-check passed.
The paired release build and first identity gate are pending; CLI behavioral
controls are not acceptance evidence until that gate permits continuation.
The gate uses a conservative superset of emitted experimental_core routines plus
the fixed file-run wrapper, excluding only the named new sliced routines. It
checks instance sizes first, then resolves only reviewed instruction forms and
fails closed on unknown relocation forms. This protects heap helpers as well as
the complete loop. No fixed-path identity change is accepted by this record.

## Disposition and evidence

The candidate b6a72bb275d44ab79cef4353493e0022696dab19 is preserved in a
verified local implementation bundle. Every non-plan file is restored to
4ea32429a886a9ddc22f2d29c98b08a5fa3dbd36. No host routing change ships.

The implementation expanded one lexical opcode/error/cleanup definition into
complete and sliced loops. The paired production builds passed. The first
identity checker had a selector bug for angle-bracket impl symbols; an independent
exact-name audit then proved all seven complete invoke_args instances changed
size: -6, +33, +439, +301, +84, +879 and +298 bytes. Original failure receipts
are preserved; no identity exception was granted.

The authorized measurement-only comparison used the eight existing CLI old-path
kernels at n=1 and n=5 with native off/on. All captures, output parity and
nonvacuous interval checks passed. Positive program growth fails acceptance.
These are program instruction deltas, candidate minus baseline:

| Workload | Native | Program delta n=1 | Program delta n=5 | Extra program growth |
|---|---|---:|---:|---:|
| awfy_permute | off | +34636 | +173204 | +138568 |
| awfy_permute | on | +13026 | +13214 | +188 |
| awfy_sieve | off | +30211 | +151079 | +120868 |
| awfy_sieve | on | +585 | +949 | +364 |
| fib | off | +88550 | +375124 | +286574 |
| fib | on | +251245 | +251457 | +212 |
| loops | off | +19996 | +100004 | +80008 |
| loops | on | +152 | +160 | +8 |
| numeric_calls | off | +29996 | +150004 | +120008 |
| numeric_calls | on | +443 | +451 | +8 |
| shared_rows | off | +9 | +69 | +60 |
| shared_rows | on | -9 | +51 | +60 |
| vec_iter_bool | off | -33341 | -166673 | -133332 |
| vec_iter_bool | on | +102 | -630 | -732 |
| vec_iter_i64 | off | +9995 | +49999 | +40004 |
| vec_iter_i64 | on | +233 | -447 | -680 |


Native-off loop, numeric-call, fib and sieve growth is inside the complete
invoke_args body; PreparedInvocation::invoke saves a fixed 5 instructions.
This is not libc noise or only setup cost. No wall-time claim is made. The
separate method-region/CGU suite was not measured; no further measurement was
needed to reject this design. Capture PASS is not acceptance PASS.

Evidence retained locally: BUILD-v1/pair/RESULT.json,
BUILD-v1/FIXED-LOOP-ATTRIBUTION.json, FIXED-MEASUREMENT-v1/RESULT.json,
per-case profiles and FIXED-REPORT.md under target/0236. Core controls passed
172 tests with one ignored, and host wiring type-checked. The new eval CLI
behavior controls, Miri and runtime final acceptance were not completed after
the stop. No claim of migrated eval acceptance is made.

The host-fuel audit found that complete fresh invocation does not reconcile
ambient host consumption with local opcode fuel. The reviewed sliced-only
finite TLS scope restores the outer scope and fails closed on missing/increased
balance. This deliberate parity qualification carries into 0237; complete-path
host budgeting remains a separate follow-up. The rejected code is archived,
not shipped as a partial fix.

Disposition: ADR0237 retains the one-shot eval contract but restores fixed
function source exactly and uses a sliced copy with a mandatory token-level
drift guard. Its revised plan requires review before implementation. No hard
CGU isolation is promised.

Withdrawal gates: empty non-plan diff, signed commit checker and the existing
host/integration/generated/cross final chain. Publish only on all-PASS. No
benchmark follower is needed.
