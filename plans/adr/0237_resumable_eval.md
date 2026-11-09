# ADR 0237: Resumable invocation for one-shot eval

Status: Accepted

Base: published e7537fa1c88b8e5637535210a8f012bce8e648f4 (docs-only withdrawal).
Fixed production-source reference: 4ea32429a886a9ddc22f2d29c98b08a5fa3dbd36.
Accepted after the withdrawn 0236 macro experiment; boundary review precedes production edits.

## Problem and scope

Fresh Executor supports complete synchronous invocations, but not suspension
between instructions. rnx eval currently constructs Session and calls
Session::eval; src/execute.rs supplies the legacy synchronous sliced driver.
A fresh compile alone cannot replace that path: Ctrl-C, fuel, source maps,
lifecycle cleanup and effects must survive slice boundaries correctly.

Introduce an exclusively borrowed, resumable invocation and use it only for
admitted synchronous one-shot eval. Persistent sessions, REPL, Jupyter, :dep,
servers, async/generators and cross-program functions remain separate records.
Keep file-run, columns and existing complete Executor invocation routes intact.
Do not add library enrollments to this record.

## Invocation contract

Proposed API names (subject to boundary review): PreparedInvocation::start_sliced
consumes the preparation token and a finite total fuel allowance, returning
SlicedInvocation. It borrows Executor, Program and Store exclusively for its
lifetime. Arguments are transferred once at start; no reference to temporary
argument arrays is needed after transfer. Configuration cannot change while
active. One owner prevents concurrent resume, store close and program disposal.

resume(nonzero_slice) returns Limited, Complete(Root), or Failed(Failure).
Limited is a successful suspension, never a runtime error or language yield.
It retains frames, next PC, slots, host authority and remaining total fuel.
Completion/failure/cancel are terminal. Calling resume on a terminal token
returns a lifecycle error without effects or fuel changes. Dropping an active
token abandons it and releases frame/slot references; it never resumes work.
Explicit cancel/close exposes cleanup failures; Drop remains best-effort and
must not panic. Terminal teardown leaves Executor reusable under its existing
cleanup contract. No public access to a half-transferred frame or result.

Total fuel is fixed at start, monotonically decreases, and cannot be topped up
by resume. Soft slice accounting is distinct from hard fuel. Charge each
instruction exactly once, at the same semantic boundary as complete execution.
A charged operation completes or fails before suspension; calls/returns save
the correct continuation. Zero total permits no charged work. A zero slice is
invalid without mutation. A result/error on the last allowed instruction wins
over a subsequent budget check; hard exhaustion otherwise terminates. Errors
retain remaining fuel and are never converted into resumable Limited.

Host operations remain atomic with respect to soft slicing and may exceed a
slice. On the sliced path they draw from the same hard allowance through a new scoped
operation-boundary wrapper;
never install a fresh total on every host call or resume. No host TLS guard,
borrow of a temporary stack, or active callback may escape a resume call.
Synchronous host calls cannot be preempted. Interrupt latency is bounded by
slice work plus the current host call, not by a wall-clock guarantee.

## Cancellation and effects

At the rnx driver boundary, keep legacy eval's ordering: execute a slice, then
check the interrupt flag on Limited and Complete. Completion with an interrupt
becomes Interrupted (exit 130); an actual runtime error remains the error even
if the flag is set. On Limited, interrupt takes precedence over reporting an
exhausted total, matching slice_sync. A deterministic test seam checks races;
signal tests confirm integration, without sleep-based timing assertions.

The core token also permits explicit cancellation while suspended: it releases
live execution state, reports Cancelled, and cannot run again. Preserve the
existing Executor pending-cancel semantics on preparation refusal. Distinguish
this explicit API cancellation from the host driver's post-slice Ctrl-C check.

Committed output, filesystem/host effects and mutations through shared handles
stay committed. Cancellation/failure is not a transaction rollback. Only
unpublished temporaries, pending result and invocation-owned references are
released. No result/binding publication after failure. A one-shot eval retains
no state for a second input; this does not solve persistent session ownership.
Never fall back after start, including failure before the first visible effect.
Only typed pre-effect admission refusal can choose legacy.

## Execution architecture and fixed routes

Keep complete invoke_args and its existing helper bodies byte-identical to
4ea32429. Add only module wiring in execute.rs; put sliced execution in a real
child module with an explicit public re-export. No new Executor/Frame fields.
The complete operation body is authoritative. The sliced loop has a copy,
validated by a committed Rust-token drift guard, not macro extraction.

Compare opcode dispatch, post-match call transfer, error enrichment and terminal
cleanup. Exact enumerated hooks cover: suspension/PC checkpoint before the next
charged instruction; soft debit after hard debit; sliced host-budget adapter;
terminal outcome conversion. Each hook has exact expected tokens and count.
Do not discard arbitrary marker regions or globally normalize identifiers.
Fail on missing/duplicate anchors, extra/reordered arms and unmirrored changes.
Test the guard with mutations of operators/literals, transfer ordering, error
cleanup and hook cardinality/content. A mirrored change still needs semantic tests.

Reuse archived token/fuel/cancel/host/eval controls where applicable. Preserve the
fixed body verbatim; do not generate it or add inline/profile corrections. A
separate module does not guarantee CGU isolation, particularly under ThinLTO;
new symbols and helper callers can still perturb fixed machine code. No compiler
profile change, new crate boundary or hard isolation claim is in this scope.
Review the concrete copying/tokenization boundary before production edits.

For this first record, sliced eval executes interpreted fresh code even when
native policy is requested. Native regions are not yet resumable and their
soft-slice overrun/cancellation contract is not established. Record this as an
explicit execution-policy limitation, expose it in route diagnostics, and test
native on/off requests. Complete file/column/native invocation is unaffected.
A later record can add sliced native execution under the same fuel contract.

## One-shot eval integration

Retain the existing expression/declaration parsing, generated wrapper, source
mapping, returned-value rules and presentation. Add an explicit one-shot entry;
do not silently change Session::eval's routing for persistent consumers.
Share preparation logic without copying the parser or wrapper-generation rules.
The one-shot wrapper's empty state and result/delta tuple must pass fresh
admission and ownership conversion before effects. If a returned type cannot
be safely represented by the current renderer/bridge, refuse before start;
never run fresh and then retry legacy because conversion failed.

Select fresh for admitted synchronous input. Async and unsupported signatures,
loader/forms or unproven presentation paths use the existing legacy path before
execution. Preserve the compiler-based sync/async wrapper selection, not text
search for await. Explicit legacy selection remains available. Begin lifecycle
once after admission; drain/clear host resources on every terminal path under
the existing eval rules. Drive resume with execute::SLICE (10,000), carrying the
same invocation and hard balance across calls.

Map fresh values/errors through the existing eval output contract. Keep original
source IDs and Generated offsets alive through teardown. A failure after many
resumes reports the failing operation and complete call chain, not the resume
site. Internal PCs are not user source locations. Trace/debug events (where
exposed) are neither duplicated nor reset at slice boundaries; suspension itself
is not a user instruction. Capture failure diagnostics before releasing frames.

## Observable semantics and deliberate differences

Parity covers value, stdout/stderr, exit status, source location, call-chain
meaning, host-effect order, lifecycle cleanup and cancellation outcomes.
Existing finite fuel units remain the fresh core's instruction accounting;
legacy and fresh instruction counts need not match. Exact opcode budget/frontier parity
is required between sliced and complete fresh execution; budget-aware host
consumption is additionally charged only on sliced execution (ruling below). Against legacy, require
bounded termination, error precedence and existing documented budget behavior;
record any differing low-budget frontier explicitly, not as an exact match.

No promise of interrupting a host call or of rollback. Native requested policy
may execute interpreted fresh eval as described above. Do not promise every eval
becomes fresh: report admissions and retained legacy cases separately. Preserve
existing diagnostics contracts; any unavoidable textual divergence requires a
named fixture and review before acceptance. Async semantics do not change.

## Acceptance

Behavioral eval corpus parity and new suspension/cancellation controls are the
primary gates. Require proof of no replay, conserved fuel, stable source maps,
clean teardown and safe executor reuse, including fault injection. Miri covers
ownership/drop/resume and host bridge boundaries. Run host, integration,
generated-project and cross checks for changed rnx source, plus commit policy.

No speedup target or timing campaign. Fixed-run paths must show no regression:
exact unchanged program vectors on the eight established FIXED-MEASUREMENT
CLI kernels at n=1/5, native off/on,
with existing system-object accounting only. Identity/instance checks screen
changes early; they do not replace required behavioral or vector evidence.
No new per-call or per-block exceptions are authorized by this plan.

Review requirement: the FIRST post-build gate is an instance guard and resolved
identity comparison of the complete fixed-run loop and its operation helpers.
Behavioral results cannot authorize continuation past a failed identity gate.
Any fixed-loop identity or vector change stops work for review; no automatic
inline-attribute correction rounds. The interpreted-only eval policy must appear
in CLI route diagnostics or verbose output when native execution is requested.

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

Review addition: the mandatory Rust-token drift guard runs from ci/preflight.py,
not only as a unit test. An unmirrored edit must fail preflight before review.
Implement and review the guard/mutation controls alongside the source.

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
