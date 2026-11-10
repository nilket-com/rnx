# ADR 0238: Cross-program function calls for sliced invocation

Status: Accepted

Base: published 2ed1b182e1464c5abae4ce4042774fa41e8956cc (ADR 0237).

## Problem and scope

Persistent sessions compile every input as a new program against one context:
retained declarations are re-emitted, and bindings are restored from session
state. Legacy function values keep their unit alive, so a closure bound by one
input can be called by a later input. Fresh function values carry their owning
program's process-unique code id, and CallValue on a value from another program
fails with Detail::ForeignFunction. The routing inventory witness shows this:

	let n=7;let c=||n;let shared=[1];0
	shared.push(2);c()
	shared.len()

Legacy returns 0, 7, 2. Fresh pushes, then fails on c(). The push is already
visible, so the failure cannot be retried on legacy. This blocks every
persistent consumer: REPL sessions, the Jupyter worker and :dep.

This record is the first step of the persistent-session track. It adds calls to
function values and closures owned by other programs that share one Store, on
the sliced invocation path only, plus explicit program lifetime. Session wiring
(state in a Store, reset, ceilings, engine selection for async input) is ADR
0239. Jupyter, presentation and :dep follow in their own record. No rnx entry
point changes route in this record.

## Program set

The caller owns an explicit set of programs that may call each other, passed to
sliced preparation. No global or static registry. The entry program is a member.

Membership requires compatible provenance, and Program has no general Context
identity today. Add an unforgeable provenance: compilation records an identity
that only the compiling Context (and the runtime built from it) can produce, so
matching host hashes, an absent format certificate or equal source text never
count as compatible. State whether mutating a Context after compilation
invalidates its programs, and enforce that rule rather than document it.
Preparation refuses a set with mismatched provenance before any effect. The
boundary review accounts for any Program layout or caller changes this needs,
under the fixed-route gates below. Function values resolve by
(code id, function id) through the set. A value whose program is not in the set
is a typed runtime error with the current ForeignFunction meaning, raised
before argument transfer. It is never undefined behavior or a silent wrong call.

A foreign call runs the callee in its owning program's code: its constants,
nominal declarations, format authority, intrinsics and source records. Nominal
values already cross programs with the same published shape; keep that rule.
The session's refusal of changed type shapes stays where it is.

## Lifetime and retention

A program must outlive every invocation that executes it. The set is borrowed
for the whole sliced invocation, so it cannot change while code runs.

Release is a collection operation, not a query over caller-chosen roots. A
subset of roots cannot prove that another Root or a retained host alias does not
reference a program. At an idle boundary (no active invocation on the Store),
the caller asks the set to collect against every live root in the owning Store.
The collection traces function values, closures and their captures, iterator
and sort payloads, values pending destruction and cycles, and releases exactly
the programs nothing references. Cost is proportional to the live heap and is
paid only when the caller collects, never per instruction or allocation. No
reference counting on closure creation or heap hot paths unless measurement and
review approve it. Allocation failure during collection fails closed: nothing
is released, and the error is reported.

Timing is explicit: a program becomes unreferenced when its last value goes,
but it is released at the next collection call, not immediately. The session
(ADR 0239) decides when to collect, at least at input boundaries and on reset,
so retained memory follows live state between inputs. After release, the
program, its source text and its maps are gone.

Collection never releases a referenced program. A call through a value whose
program is absent from the set arises only when a caller builds or edits the
set deliberately; it is a typed error, distinct from collection.

## Execution semantics

Foreign frames are ordinary frames for fuel, slicing, interruption and errors:
each instruction is charged once against the same hard total; soft limits can
suspend inside a foreign frame and resume continues there; cancel releases
foreign frames like local ones. No user code runs uninterruptibly except host
calls, as in ADR 0237. Returns transfer back to the caller's program and pc.
Recursion and callbacks across programs (a foreign closure that calls back into
a value from the entry program, sort/iterator callbacks) follow the same rules.

Errors inside a foreign frame report that program's source location and the
complete call chain across programs, each frame resolved against its own
program. Today Failure::message takes one Program and the call chain stores
bare Locations, so the sliced path needs a failure representation or resolver
that records each frame's program and resolves its source map, field and
function names and format authority against that program. Diagnostics own
program references too: either materialize them before any collection, or
retain the programs an unresolved failure needs until it is rendered. Never
promise release while an unrendered failure still depends on a program. When a
program's metadata no longer exists (deliberate removal), an explicit
unavailable-function diagnostic is acceptable; never print a raw code id as a
name. Keep the complete path's Failure and its layout unchanged where possible.

Native policy: sliced execution stays interpreted (ADR 0237). Complete invoke
keeps today's behavior, including ForeignFunction for any foreign value. File
runs, columns and servers are unchanged.

## Fixed routes

The complete execution loop and its operation helpers keep their machine code.
Keep foreign-call state out of the shared Frame and Executor layout unless
review approves a measured change; the sliced path may keep parallel per-frame
program state. Parallel state needs transactional setup and rollback, so a
failure between updating the frame stack and the program stack leaves neither
changed.

Foreign owners are checked in more places than CallValue: call_closure
validates the owner against the current Code, sorting's SORT_BEGIN rejects
foreign functions and closures before the comparator call, and enrich_error
labels foreign owners and looks up arity in the current Code. The boundary
inventory enumerates all of these, any iterator validation, callee code
selection, return, pop and unwind, and per-program selection of format
authority, constants and sources. The 0237 drift guard gains exact, reviewed
sliced hooks only for those sites; every other mirrored region keeps exact
matching.

## Acceptance

The inventory session witness returns 0, 7, 2 on fresh through the program set,
with the push visible exactly once, and its verdict changes from blocked to
engine-ready (session wiring pending). Behavioral controls in the paired
implementation plan cover lifetime, errors, fuel, slicing and cancellation.
Miri covers set ownership, release with live references, stale calls and
teardown after foreign frames.

Fixed-route gate: the eight established CLI kernels at n=1/5, native off/on,
show exact program vectors against 2ed1b182. The ADR 0237 exception was bound to
its measured pair and does not carry over. Identity/instance checks run first
after the paired build; any fixed-loop identity or vector change stops work for
review. No timing campaign and no speedup claim.

Run host, integration, generated-project and cross checks, Clippy for rune and
rnx, preflight with the drift guard, and the commit checker before review.

## Boundary review

The pre-production inventory found that the Store cannot enumerate every
external reference to a program. Root is an affine {store, value} with no Drop,
export registers nothing, and a function value is a scalar with no heap slot,
so a returned function can be the only reference and stay invisible to any heap
scan. Store-wide collection as written above is therefore replaced by an
owning boundary. Tracking every Root globally would change shared hot paths and
is out of scope.

ProgramDomain is an opt-in owner for cross-program execution. It creates its
own Store and holds the program set, a root table and one Executor. It never
accepts an existing Store, never exposes a raw Root or &mut Store, and never
imports an external Root. Values leave it only as affine DomainRoot handles,
branded to the domain and checked by generation. The root table is the source
of external liveness. Value operations are limited to what the controls and
the inventory witness need; host conversions register through the domain.
Domain handles are not Send, like Store. Old Store and Root APIs and complete
execution are unchanged.

Dropped handles do not leak until close. A DomainRoot holds its slot handle
and a weak reference to the domain's release metadata, so a surviving handle
keeps neither the Store nor any program alive after close. Slot metadata is
reserved when the root is created or retained. Drop enqueues the slot once on
an intrusive pending-release chain: no allocation, no Store call, no script
callback, no cleanup. Every mutating domain operation drains the queue first,
collection drains it before tracing, and close releases everything. Registry
borrows never span user or host callbacks, Store cleanup or handle
destruction. A release refused before consumption keeps the root registered
and reports the refusal; a failure after consumption removes that ownership
exactly once and reports the cleanup failure. Collection does not proceed past
a failed or incomplete drain: it retains every program and returns the error.

A newly compiled program is held by an explicit ProgramKey lease until it is
released or its entry call is prepared. Collection keeps programs reachable
from the root table, pending Store work, leases and unresolved diagnostics. It
traces closures (owner and captures), every payload's children, iterator and
sort payloads, and cycles once; it builds the keep set in fallible scratch and
changes membership only after success. CollectionReport lists outstanding
leases and pending-release counts. This is program collection, not a heap
garbage collector. The packet states whether the host bridge can retain raw
core Roots; if it can, the bridge registers them before completeness is claimed.

Provenance is an execution-feature-only compilation epoch shared by Context,
RuntimeContext snapshots and Program: a nonzero process identity plus a checked
revision, never a pointer or format certificate. Every Context::install, and
any other audited mutable path, invalidates the epoch before mutating, failed
installs included. Programs and runtimes from one epoch stay compatible with
each other after the Context changes; a newer epoch cannot join a domain built
on an older one. A default RuntimeContext has no epoch and is rejected. The
packet lists the audited mutable paths. These layout changes are under the
identity gates, not assumed neutral.

Diagnostics: CrossFailure borrows the program set until its message and frames
are materialized into owned text, so collection cannot run while an unresolved
failure exists. Frames record their owning ProgramKey and location before
unwind. Allocation failure while expanding the chain keeps the primary failure
and reports the truncated capture explicitly. The complete path's Failure is
unchanged.

One sliced loop: the 0237 sliced executor becomes owner-aware. SlicedInvocation
stays the single-program API (one available program; recursive frames keep
coherent ownership), and cross-program invocation uses the same operation body
with per-frame owner bookkeeping. No third copy of the operation loop. The
complete Frame and Executor layout stays untouched where possible. Calls
reserve owner, frame, slot, diagnostic and result-table capacity, resolve the
callee and validate arguments before publishing; the owner push after a
published frame is infallible after reserve. Return retains the outgoing value
before leaving and pops the owner only with its frame. A release failure keeps
the stacks coherent and terminates with cleanup evidence; committed script
effects are never undone.
