# rnx 0055: HTTP cancellation belongs to its execution

Status: proposed 2026-09-16, for review before implementation. This is the
HTTP migration reserved by 0053, prompted by the accepted 0054 assembly stop.
It changes a root battery's ownership policy, not the server's admission shape
or HTTP library. No implementation is part of this draft.

## Context

At rnx `20f8310`, each HTTP State owns a reqwest client and a map of abort
handles. Every request spawns a Tokio task; a Rune future awaits its handle.
State::clear aborts all of that state's requests and drops its client, then
Runtime::drain_http starts block_on and waits for the entire runtime's alive-task
count to reach zero, with a 100 ms bound.

The 0054 assembly fixture proves that request ownership is already separate.
A failed handler's socket closes with a clean read of zero, while the healthy
handler's socket stays open and later returns its body. Completion accounting
is nevertheless global: clear panics inside a long-lived block_on, and outside
it times out waiting for the healthy handler's legitimate tasks. Gate 2 remains
open. Fixing that mismatch is a prerequisite to choosing a server shape.

0053 already provides a context-owned Scope. It stamps an operation whenever
that operation is polled during an execution, drops pending operations touched
by a failed execution, and revokes all on reset or retirement. It needs no Send
bound on a returned future. The PostgreSQL adapter uses it. HTTP does not yet.
Stock Extensions::none currently creates a disabled lifecycle; merely passing
that disabled state to HTTP would not enable tracking.

There are two additional facts to preserve in the decision. Reqwest/hyper still
spawn connection and pool tasks even if rnx stops spawning its request task.
Dropping a fetch future and client is not synchronously observed socket EOF.
And HTTP's native registrations currently accept owned String arguments,
consuming a caller's URL binding. 0052 fixed the same boundary with a borrowed
argument and an owned snapshot for the lazy future.

## Decisions

### 1. One tracked request, no rnx request task

Keep the four script names and response shapes from 0034: http::get,
get_bytes, request and request_bytes. The native registration returns a lazy
Scope::track future. On poll that future obtains the owner's client and drives
fetch and response conversion inline. It never spawns a request-supervisor task
or waits on a JoinHandle. Remove that supervisor's abort-handle map, ids,
Active guard and AbortOnDrop wrapper.

The underlying driver is still reqwest, with its existing TLS, proxy, redirect,
body, UTF-8 and deadline behavior. Hyper's internal tasks and the system
resolver are not claimed to disappear. This is not a fork, a new transport or
a change to the response cap. The guarded JSON reader remains json::parse.

Borrow URL and method strings at the native call and make owned snapshots
before returning. No caller borrow crosses an await. For the request forms,
copy/validate the option fields into the existing owned Request representation
at that boundary as well, including body bytes and headers. Store any validation
error in the returned future, so it is still a catchable Err when awaited;
this does not turn invalid options into a native panic or a different function
arity. This explicitly replaces first-poll option parsing with call-time
snapshotting. Creating an unawaited future may allocate/validate, but creates
no client, connection, DNS lookup or network request. The deadline starts with
request execution on first poll, not with snapshot creation.

The four calls preserve URL, method, options, header strings and body bindings
for subsequent use. Later reassignment or mutation does not change an already
created future's snapshot. Normal HTTP refusals keep their existing wording
and URL attribution. Lifecycle cancellation is the common catchable
`operation cancelled`, not a made-up reqwest transport error.

### 2. HTTP adopts the 0053 execution policy

Use the same enabled Lifecycle as the serving context's extensions, with an
HTTP scope created on that context's owning thread. Do not create an independent
HTTP execution counter or ambient fallback. HTTP installation requires that
explicit live scope. Activation must occur for stock serving contexts as well
as extension executables, before HTTP or extension builders capture a scope.

Move activation as needed behind the early command dispatch: version and help
must not start constructing a serving lifecycle or a context. Keep the pure
settings evaluator free of rnx modules, and retain the extension-builder counts
for version/help/selfcheck. Preserve the single begin/finish path and all
existing admission checks. Tests that install HTTP directly must provide an
explicit lifecycle too; no untracked compatibility path.

The behavior is now:

| Boundary | HTTP operations affected |
| --- | --- |
| Successful VM execution, including a caught Err | Pending tracked operations remain live. |
| Interrupt, runtime failure or budget halt after admission | Revoke pending operations polled by that execution. |
| Unrelated failed execution that never polled an older request | Preserve that older request. |
| Compile/admission refusal | No execution began; preserve existing operations. |
| Ctrl-C while editing a prompt | Abandon the edit, not previously admitted operations. |
| Reset | Revoke all context operations and discard the owner's cached client. Keep the context usable afterward. |
| Context retirement | Revoke all operations, discard the cached client and refuse new tracked work. |

This deliberately replaces 0034's interrupt-clears-all rule and the REPL's
current editing-time cancel_http call. It also adds runtime-failure/budget
revocation that HTTP did not have before. 0032's difference between a completed
run and an abandoned session input stays intact: use the execution outcome
already decided by that driver, not a second interpretation of the signal flag.
A revoked retained wrapper returns the cancellation error on repoll without
polling the disposed request again.

The client stays per owner and can be reused across successful and failed
executions. Do not discard it just to force every unrelated connection closed
on one input's failure. Reset and retirement discard it. Independent serving
contexts keep independent scopes and HTTP states; this record does not create
a shared client across those owners.

### 3. Revocation and transport closure are different operations

An execution's finish revokes only its affected tracked requests. A private
owner-cancellation path combines revoking all of that owner's operations with
discarding its client for reset/retirement. Neither starts block_on, waits for
task counts, or requires another runtime turn to drop the owned fetch future.
These paths must be callable from within a running runtime.

Connection tasks can still own sockets until the runtime progresses. Do not
report that synchronous cancellation has joined them. On a continuously driven
server runtime, the fixture must observe the cancelled connection close while
another owner's request stays live. It must not obtain that result by pausing
or draining the entire runtime, cancelling the healthy request, or swallowing
a cleanup error.

This changes 0034 decision 3 and gate 6 deliberately. A session interrupt no
longer discards every pooled connection or proves zero tasks before the next
prompt. A reset revokes all operations and releases the cached client, but is
not a runtime shutdown and no longer waits for a whole-runtime drain. At an
idle prompt, physical socket closure and asynchronous allocation release can
wait until the next runtime turn or final shutdown. Do not promise the old
immediate reset-memory baseline for transport-owned allocations. Bindings and
tracked request inners still clear as specified. These revised guarantees must
be stated in help/README and the current-status note on 0034; historical probe
results stay unchanged.

An inline retained future also stops making application-level progress while
it is not polled. A timeout is observed when the request is driven; it is not a
background watchdog that guarantees closure of an unpolled retained request.
A future selected away from can therefore retain resources until it is polled,
revoked or its owner ends, even while unrelated executions drive the runtime.
On repoll after the held fixture has exceeded its deadline, it must refuse
rather than start a new request with a fresh deadline. This consequence of
removing the spawned request task is part of the contract, not an omission.

### 4. Whole-runtime drain belongs only to whole-runtime teardown

Retain a separately named, bounded runtime drain for boundaries that own the
entire runtime, have stopped admission and can enter block_on legally. Close
all serving owners first, then drain. A handler ending is not such a boundary
when other handlers or owners remain. No server worker or public shutdown API
is added here; 0054 will invoke the private distinction in its fixture.

Normal session EOF/:quit and acknowledged worker shutdown must perform their
whole-runtime teardown before reporting clean completion. Worker shutdown must
report cleanup failure/state loss before settlement and preserve the existing
barrier/ack order; reset now acknowledges logical cancellation, not runtime
termination. A failed final drain is an error, not successful cleanup. Keep the
existing 100 ms asynchronous-drain allowance initially, with measured tolerance
reported separately. Do not wait for or count a started blocking OS resolver
as an owned async task, and preserve shutdown_background for that residual.

Run/eval and explicit process::exit retain their existing one-shot exit and
resolver-shutdown behavior. They must revoke tracked operations on the existing
exit paths, but no new global drain may overwrite a completed script's status
or add a wait for blocking DNS. Cover error exits as well as normal returns;
Drop alone is not sufficient for process::exit. A terminal cancellation method
must never accidentally retire the still-usable session lifecycle.

No second public lifecycle primitive is proposed. The public library surface,
Scope cancellation wording and adapter contracts remain as in 0053. The stock
HTTP battery now participates in the existing mechanism and pays its activation
cost; measure that cost rather than calling it zero.

## Acceptance gates

1. **Inline ownership and activation.** Source review finds no rnx-spawned
   request task or abort ledger. A real Rune future is lazy until first poll.
   After it starts, retain it strongly across the failing VM's disposal, revoke
   it, and prove its inner future was dropped before another runtime turn. Its
   repoll returns `operation cancelled`. Do not use socket EOF as the synchronous
   drop observation. Prove stock run, eval, session and worker have enabled
   lifecycles without any lifecycle-aware external extension.
2. **Session policy.** Independently gate unrelated interrupt, runtime failure,
   budget halt and pre-execution refusal preserving an older started request;
   polling that request in the failed execution revokes it. Gate a successful
   select, a caught HTTP Err, editing-time Ctrl-C, reset and subsequent reuse.
   Use started-but-unpolled retained futures, not only lazy unopened futures.
   Reset leaves no tracked request inners, and subsequent valid input remains
   admissible under the fixture's stated allocation ceiling. Report delayed
   transport allocation release separately; do not silently relabel it zero.
3. **Assembly isolation.** Rerun 0054's same-worker and cross-worker HTTP
   scenarios with the same held fixtures, active healthy peer, failure timing,
   clean-read-zero EOF distinction and final zero-task checks. Adapt only the
   installer/cancellation calls to the new private signatures and replace the
   old expected-stop assertions with successful in-runtime cancellation. Keep
   the historical stop fixture/results as before evidence. It cannot literally
   remain source-identical: it asserts the panic and timeout being removed.
   The healthy peer must still be open when the failed side's EOF is observed,
   and must subsequently return its original response. No zero-task condition
   while that peer remains live; final zero is checked after all owners end.
4. **Pool and deadlines.** Successful requests still reuse a socket. An
   unrelated failed input preserves both an active older request and the
   owner's healthy idle pool. Reset/retirement releases the pool; a continuously
   driven fixture observes clean EOF within a stated 500 ms test tolerance.
   At an idle prompt no immediate EOF guarantee is asserted. A request held
   unpolled beyond a short deadline refuses on repoll; creating an unpolled
   future itself neither opens a socket nor starts that deadline.
5. **Borrow and snapshot.** Call each of the four names twice with the same
   bound URL (and method/options for request). Both calls succeed and bindings
   remain readable. Create a lazy future, mutate/reassign URL, method, headers,
   body bytes/string and timeout options, then await it. The fixture receives
   the original request and the caller retains the new values. Invalid options
   still refuse catchably on await with the existing attribution.
6. **Teardown.** Normal session exit and worker shutdown revoke all operations,
   drop the client and drain only their own stopped runtime before completion.
   Keep worker barrier/ack order. Exercise final-drain failure and require the
   named failure/state-loss path; it must not acknowledge clean shutdown.
   The stalled-resolver run/eval/session/reset cases from 0034 still pass with
   their stated DNS residual. Explicit process exit preserves status behavior.
   Under a running runtime, owner cancellation never calls block_on.
7. **Unchanged response contract.** Run the existing loopback HTTP battery for
   method delivery, status versus error, headers, redirects, proxies, certificate
   verification, decoded cap, strict UTF-8, incomplete body, deadline and JSON
   handoff. Change only the lifecycle/borrow expectations explicitly listed
   above. All tests clear proxy/config environment and gate terminal conditions.
8. **Regression and cost.** Both root suites, formatting, notices, extension
   assembly/lifecycle fixtures, 0052 ownership and kernel worker recovery gates.
   Compare version/help, bare run/eval, JSON/CPU/string workloads, sequential
   connection reuse and cancellation cost with matched before/after binaries.
   State allocations and size delta; report regressions, not assumed drift.
   Source review plus early-path assertions prove no new version/help work.
   Windows execution remains unverified unless actually performed; distinguish
   full and isolated cross-checks as before.

## Stop conditions

Stop if a retained request cannot be revoked through Scope without a detached
rnx request task; if cleanup requires whole-runtime quiescence while a healthy
owner remains; if the held assembly fixture cannot obtain clean EOF without
cancelling its peer; or if the migration requires changing 0053's ownership or
public promises. Stop and revise if reset's deferred transport cleanup prevents
the subsequent-admission gate, rather than adding a hidden global drain.

Do not waive a failed gate by increasing the runtime-drain bound, adding an
unbounded async wait, replacing EOF with a read error, or reverting to cancel-all
on failure. If hyper's own tasks need a stronger owner-completion mechanism,
record that result before designing a new interface. HTTP server selection and
0054's admission-shape decision remain downstream of this migration.

## Risks and deliberate changes

This is a cancellation-policy change, including Ctrl-C at an idle prompt,
runtime failures, retained unpolled deadlines and reset transport completion.
It must not be presented as a purely internal refactor. Snapshotting options
moves pure validation/allocation to native-call time, while I/O stays lazy.
Enabling lifecycle tracking for stock contexts changes the previous disabled
fast path and may change allocation and code-layout measurements. Client drop
and fetch drop still do not cancel blocking system DNS, synchronously close
hyper sockets or establish whether a remote side effect occurred. No retry is
added. Historical evidence and gitignored reviews remain where they are.
