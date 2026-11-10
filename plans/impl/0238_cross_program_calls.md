# Implementation plan 0238: Cross-program function calls for sliced invocation

Status: Accepted

Paired ADR: ../adr/0238_cross_program_calls.md. Implementation is local; required gates passed and publication review is pending.

1. Pin 2ed1b182. Read the sliced invocation module, CallValue, call_closure,
   sorting's SORT_BEGIN and comparator path, iterator validation, enrich_error,
   Failure::message and call-chain rendering, and how Program owns code ids,
   constants, sources and format authority. Inventory every site that compares
   a function's code id with the current program or resolves names, constants
   or sources through the current program.
2. Bring a boundary proposal for review before production edits: provenance
   mechanism and Context-mutation rule; program set ownership and API; where
   per-frame program identity lives on the sliced path and its transactional
   setup and rollback; callee code selection, return, pop and unwind; how
   callbacks reach foreign code; the collection operation, its cost and
   fail-closed behavior; the sliced failure representation or resolver and how
   it keeps programs needed for rendering; the exact drift guard hooks. Include
   any Program layout or caller change, the expected effect on the complete
   loop (none) and how the paired build will prove it.
3. After review, implement in the rune crate: provenance, program set, sliced
   foreign calls and returns, cross-program diagnostics, and collection.
   Extend the drift guard with the reviewed hooks and mutation tests that
   reject an unmirrored change at each hook.
4. Update the routing inventory session witness to run through the program set.
   Change the persistent-session row's requirement text to session wiring;
   keep its route blocked until ADR 0239.
5. Build exact paired binaries in isolated role targets. FIRST run the instance
   guard and resolved identity comparison of the complete loop and helpers.
   Then exact vectors on the eight kernels. Stop for review on any change; no
   exception rounds.
6. Run the controls below, Miri, host/integration/generated/cross, Clippy,
   preflight and the commit checker. Bring the packet to review before
   publication.

## Required controls

- The witness inputs give 0, 7, 2; shared.push takes effect once.
- Foreign closures with captures, plain function values, foreign values passed
  as arguments, returned from foreign calls and stored in collections.
- Recursion across programs and chains of three programs. Callbacks: sort with
  a foreign comparator, iterator adapters with foreign closures, and a foreign
  closure calling back into an entry-program value.
- Slice sizes 1, 2, 3 and larger than total with suspension inside foreign
  frames; resume continues the foreign frame; total fuel is conserved and
  matches the all-local equivalent where the program shape allows.
- Interrupt and cancel while suspended in a foreign frame; budget exhaustion
  inside a foreign frame reports the foreign location.
- Errors in foreign frames: location in the defining program, full call chain
  with each frame resolved against its own program, function and field names in
  the message. A failure rendered after collection still resolves, or the
  programs it needs were retained. Deliberately removed metadata gives the
  unavailable-function diagnostic, never a raw code id.
- Value whose program is not in the set: typed error before argument transfer,
  no effects from the call. Sets mixing programs from another Context, from a
  Context mutated after compilation (if the rule invalidates it), or with forged
  or missing provenance: refused at preparation, before any effect.
- Collection: bindings that keep zero, one and several programs alive; captures
  nested in collections and closures; iterator and sort payloads; values
  pending destruction; cycles; a program referenced only by a Root the caller
  did not mention. Unreferenced programs are released at the collection call,
  not before; referenced ones never are. Injected allocation failure releases
  nothing and reports the error. Collection during an active invocation is
  impossible or refused.
- Parallel frame state: failure injected between the frame-stack and
  program-stack updates, on call, return and unwind, leaves both unchanged.
- Complete invoke with a foreign value still fails with ForeignFunction.
- Executor reuse after foreign-frame failure, cancel and completion inherits no
  frames, fuel or program references.

## Evidence discipline

Commit reusable tests and harness code in this record; scratch receipts under
target/0238. Each receipt binds revision, command, environment and child
result; success requires process success and semantic success. When a queued
sequence fails, report the failing stage at once. No retry-to-pass, no polling.

Results: PASS with the reviewed exception below. Publication requires review.

## Boundary review additions

The ADR's Boundary review section supersedes Store-wide collection. Add:

- ProgramDomain sealing: no raw Root, &mut Store or external Root import is
  reachable from its public API; branded and generation checks reject handles
  from another domain or a released slot.
- Release queue: duplicate enqueue prevention, retained aliases, handles
  dropped before collection, close with outstanding handles, a handle dropped
  after close, and injected failure during drain both before and after Store
  consumption. Collection after a failed drain retains every program.
- Leases: a compiled program survives collection before its first call;
  released leases let it go; CollectionReport shows outstanding leases and
  pending releases.
- Provenance: install on a shared Context, failed install, snapshots from one
  epoch after a later install, a newer epoch refused by an older domain, and a
  default RuntimeContext refused.
- Single sliced loop: eval_sliced and every 0237 token control pass unchanged;
  recursion within one program keeps coherent owner state.

## Implementation audit

The implementation uses the existing sliced operation loop with a parallel
program-owner stack. Frame and Executor layouts and the fixed operation loop
remain unchanged. Owner capacity is reserved before argument transfer;
publication after frame setup cannot allocate. A failed leave truncates owner
state to the actual frame stack, retaining a departed frame for diagnostics.
Test-only injection covers failures before/after publication and return.
Setup failures preserve both the primary error and cleanup failure through
StartError; only its Prepare/Refused variant permits fallback.

Context::install invalidates the private epoch before inspecting the module,
including duplicate/no-op and failed installs. Its trait installer callbacks
mutate only under that entry. StdFormatEpoch is private construction-time
format authority; its module installation still passes through install.
The test-only raw-hash fixture is not an embedding API. Runtime snapshots
retain their original epoch. A different Context, newer epoch or unstamped
runtime cannot enter the domain.

The host bridge copies closed scalar/String inputs into legacy values; it
rejects function/aggregate inputs and accepts only certified closed outputs.
It exposes neither Store nor Root to a callback. Domain handles hold Weak
registry metadata, and no public method returns raw Root or mutable Store.
Collection traverses registered roots plus pending Store work, through payload
children and closure program owners, with a visited set for cycles. The keep
set is complete before membership changes. Drain failures preserve programs
and distinguish refusal before consumption from failure after consumption.

CrossFailure borrows the program set and exposes program-resolved frames;
the ordinary Failure call_chain is cleared because its bare locations cannot
represent several source namespaces. Materialization is fallible and does not
replace the primary failure. Missing metadata renders function unavailable.
Iterator admission is unchanged: an argument of unknown arity cannot be used
as map(f); the supported local closure map(|x|f(x)) exercises foreign callbacks.
Persistent-session routing remains Blocked; only the engine witness changes.

Reusable controls: tools/column/cross_program_controls.py, with development,
behavior and Miri scopes. Receipts bind source, command, declared environment,
child exit, semantic test presence and log digest. Development controls and the
existing executor suite passed before signing; final evidence is listed below.

## Reviewed measurement exception (2026-11-10)

The user approved an exact measured-pair exception after the identity STOP.
It binds baseline 2ed1b182e1464c5abae4ce4042774fa41e8956cc and measured
candidate 1a3196d4ea88dcb397f544644d44bd4d784a81e2, with binary hashes in
fixed_route_0238.json. The 32 listed workload/size/native-mode pairs bind
function inventory and delta digests (including zero-delta membership), plus
system totals. One-extra-instruction controls reject growth in every case.
There is no extrapolation, general tolerance or publication authorization.
A runtime-changing rebuild invalidates this exception. Tool/doc-only amends
must prove runtime source identity to the measured revision.

At n=5, native-off total deltas range from -0.305% to +0.0844%; seven of eight
improve, while Bool-vector increases. Native-on totals range from -0.1430%
to +0.1225%, predominantly fixed setup/system costs. Native vector I64/Bool
retain positive program growth of +680/+732 instructions from n=1 to n=5;
these are explicitly pinned, not classified as noise or fixed cost.

PreparedInvocation::invoke grows from 6573 to 16616 bytes chiefly because
native-cache construction (formerly a call to region::cache::build) is inlined.
The file route includes this wrapper in its measured boundary. A separate
provenance-only build changes 20 protected shapes, while leaving the seven
invoke_args, host_call and push_closure_call sizes unchanged; it grows the
wrapper only to 6647 bytes. This attributes some drift to provenance, but not
all full-candidate drift. Equal sizes alone do not prove instruction identity.

Evidence: target/0238/ATTRIBUTION.md, ATTRIBUTION-DATA.json,
FUNCTION-GROWTH.json, MEASUREMENT-v1-CAPTURE.json, MEASUREMENT-v1/RESULT.json,
PREPARED-ATTRIBUTION and PROVENANCE-v2/RESULT.json. Original strict FAIL receipts
remain intact. cross_program_exception.py rescoring requires the original
binary hashes and rechecks raw profile hashes and accounting before acceptance.

## Final gate evidence

The measured runtime is retained through lint-only annotations/spacing, test
and tooling amendments; exact inverse-edit source proofs bind the original
binaries. Approved rescoring and 65 negative controls pass in
`target/0238/APPROVED-EXCEPTION-v1.json`.

`target/0238/REMAINING-v4/RESULT.json` records final host Clippy, Rune Clippy,
root tests, inventory, adapter unit/integration, generated projects, cross,
production CLI build/controls, preflight and commit discipline PASS on
15a3d77e66393f391ab8a694c0db7d52010c715f (before this evidence-only amendment).
Behavior PASS is retained from REMAINING-v2: domain, existing executor/token,
eval CLI and session witness. The reuse receipts prove the scoped subsequent
tooling/test differences.

The initial monolithic Miri suite exceeded its three-hour timeout. It was
replaced by exact-name shards using the existing ADR 0210 durable runner,
prepared-target isolation and memory admission. The live inventory must equal
all 18 domain and 6 sliced tests, with no omissions or ignored tests.
REMAINING-v3/miri/RESULT.json records all shards PASS with strict provenance.

Root tests exposed an inherited source-layout assertion that counted
`eval_budget` as `budget`. The checked source files and test were identical to
the published baseline (BUDGET-PREEXISTING.json). The test-only repair matches
the run binding exactly and retains detection of unexpected assignments.
The complete root suite passed afterward; no runtime fix was needed.

No general performance band is introduced by this record. Persistent-session
routing remains blocked pending its own integration record.
