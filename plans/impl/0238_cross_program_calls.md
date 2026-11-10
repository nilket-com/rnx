# Implementation plan 0238: Cross-program function calls for sliced invocation

Status: Accepted

Paired ADR: ../adr/0238_cross_program_calls.md. Implementation has not started.

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

Results: pending. Publication: not authorized at draft stage.

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
