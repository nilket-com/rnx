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

## Implemented boundary and development evidence

The complete execute.rs source is byte-identical to 4ea32429 after removing
only the child-module declaration and explicit public re-export. The sliced
module has a copied operation body, post-call transfer, enrichment and cleanup.
The standalone ci/execute_guard tool uses syn/quote/proc-macro2 without building
Rune or rnx. Preflight invokes it with its own cached target directory. The
measured warm guard run was about 1.4 seconds including compilation checks.
Mutation controls reject unmirrored edits, extra/reordered arms and hook changes.
Errors identify the first differing token position, context on both sides and
region. Path normalization applies only to reviewed AST paths and macro token
paths, never literal strings or arbitrary identifiers.

The rustfmt::skip annotation on the module is a formatting exemption: rustfmt
changed optional commas and arm braces in the copied body, which the deliberately
strict token comparison treats as drift. Preserve the complete body's token form
and mirrored layout instead of broadening normalization. This does not exempt
other source or the checker itself from formatting.

DEV-v1 stopped on that token difference. DEV-v2 stopped on duplicate module
wiring produced by the scratch restoration script. Both were corrected before
DEV-v3, which passed executor controls and host/eval type checking. The fixed
source byte proof and repaired angle-bracket symbol-selector controls pass.
These are development controls, not acceptance of runtime migration. The first
paired identity and fixed-vector gates remain mandatory; no tolerance is added.

## User-approved measured exception

The user-approved narrow exception (review seq30084) accepts the measured
BUILD-v1 pair: baseline e7537fa1c88b8e5637535210a8f012bce8e648f4 and candidate
88627390e59ee85bf5057b0ecc15d4ce578688f1. It authorizes remaining tests,
not publication. All prior identity STOP evidence remains disclosed.

The exact binary hashes, workload/function inventories and deltas are committed
in tools/column/fixed_route_0237.json. The scorer re-parses retained profiles and
verifies their hashes and binary identities. It rejects any unlisted function,
case, size or binary pair, and any additional instruction or per-n-step cost.
No general tolerance, extrapolated workload allowance or future candidate
permission is introduced. Unknown/JIT address-keyed differences and system
counts are frozen to this capture as well; no new noise band is granted.

Program instruction deltas, candidate minus baseline:

| Workload | Native | n=1 | n=5 | Two-point delta formula |
|---|---|---:|---:|---|
| awfy_permute | off | +203296 | +1016504 | -6 + (203302) * n |
| awfy_permute | on | +179989 | +180305 | 179910 + (79) * n |
| awfy_sieve | off | +30238 | +151214 | -6 + (30244) * n |
| awfy_sieve | on | +612 | +1084 | 494 + (118) * n |
| fib | off | +88550 | +375124 | 33813/2 + (143287/2) * n |
| fib | on | +250841 | +251053 | 250788 + (53) * n |
| loops | off | +19996 | +100004 | -6 + (20002) * n |
| loops | on | +152 | +160 | 150 + (2) * n |
| numeric_calls | off | +29996 | +150004 | -6 + (30002) * n |
| numeric_calls | on | +443 | +451 | 441 + (2) * n |
| shared_rows | off | +9 | +69 | -6 + (15) * n |
| shared_rows | on | -9 | +51 | -24 + (15) * n |
| vec_iter_bool | off | -33341 | -166673 | -8 + (-33333) * n |
| vec_iter_bool | on | +102 | -630 | 285 + (-183) * n |
| vec_iter_i64 | off | +9995 | +49999 | -6 + (10001) * n |
| vec_iter_i64 | on | +233 | -447 | 403 + (-170) * n |

Formulas describe the two measured sizes only. Interpreted-path costs grow with
work, and some native-on deltas also grow; they are not all fixed setup costs.
These are instruction counts, not wall-time measurements. Shared caller and
compiler partition effects are consistent with the evidence, not proof that
every resumable design must pay this cost. ADR0234 failed timing, not the same
identity criterion. Keeping fixed source byte-identical did not keep fixed
machine code or instruction counts identical.

The identity report is now reviewed evidence for this candidate, not a blanket
waiver. Behavioral eval, signal cancellation, fuel/no-repeat, ownership/Miri,
host/integration/generated/cross, preflight and commit checks still must pass.
Any production correction invalidates the measured-pair reuse until reviewed
and revalidated. Tooling/docs-only amendments must prove runtime source identity
to the measured candidate. Bring the completed packet back before publication.

## Budget diagnostic recovery

The behavioral budget test initially expected the word `budget`. Both fresh
and explicit-legacy eval actually emitted the identical established diagnostic
`halted: 100 instructions exceeded`, exited 1, and printed `once` exactly once.
The test now checks that exact text and identifies the variant and stderr on
failure. This is a test-only correction; runtime sources and the measured binary
pair are unchanged. The original failed receipt is retained.

## Host lint recovery

The first final host run stopped on two Clippy diagnostics: the intentionally
large inline Failure variant and an unnecessary test-only drop of Root. A
scoped lint allowance preserves the allocation-free outcome representation;
the test retains its result binding without the no-op drop. The exact diff
contains no operational runtime changes. The token guard and preflight pass.
The measured-pair exception and prior behavior/Miri evidence are retained under
this lint-only and test-only scope; the full host/integration/generated/cross
sequence is rerun. Publication still requires review.

## Compact pins and final host lint recovery

The second host run found the same large-enum lint on the CLI FreshOutcome.
It now carries the same scoped allowance. An exact-diff proof against the
behavior-tested revision permits only those two lint attributes and the prior
test-only binding correction; operational runtime code is unchanged.

The exception policy stores SHA-256 digests of canonical JSON containing each
partition function inventory and its nonzero delta vector, plus function counts
and delta totals. Revisions, binary hashes, workloads and system deltas remain
explicit. This preserves the original exact gate, including zero-delta inventory
membership. The full original policy and raw profiles remain local evidence.
The compact policy is 14,868 bytes. Ten controls pass, including redistributed
counts and zero-delta unlisted functions; rescoring the retained profiles passes.
The full final sequence is restarted; no publication authorization is implied.

## Routing inventory recovery

Final v3 passed host checks and commit discipline, then stopped on the stale
one-shot eval source witness. The test/controller-only routing inventory now
recognizes eval_one_shot with pre-effect fallback; blocked entry counts are
derived from the entries and fall from eleven to ten. Its API description now
acknowledges sliced invocation. Historical probe snapshots remain unchanged.
README distinguishes interpreted sliced eval from the remaining legacy sessions,
Jupyter and servers. Production runtime code is unchanged. Final v4 restarts
the complete required sequence and sends one terminal notification on exit.
