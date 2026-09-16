# rnx 0053: cancellation reaches the operation, not just the binding

Status: accepted 2026-09-16; implementation pending. This is the separate lifecycle decision required by 0052's
stop condition; it does not mark the PostgreSQL adapter implemented or
weaken its cleanup gate. API shape below is a proposal to review before
any public library change.

## Context

Record 0052's ownership prototype, in rnx-bench `fbc08a9`, passes seven
cases through real Rune execution. It also reproduces a counterexample:
a future published by input 1, polled through select in input 2, survives
input 2's interruption because input 1's binding still owns it. The socket
stays open despite zero spawned tasks. After the server's statement timeout,
the backend becomes idle; the unpolled client future still owns its socket.

This does not disprove Rust drop. It disproves the assumption that ending
an execution drops every native future it touched. The session deliberately
keeps old bindings on failure. HTTP handles this through private cancellation
state; 0051 exposes registration only. An adapter cannot obtain an equivalent
lifecycle signal through that public boundary.

The question is narrower than pooling: how does an extension revoke a pending
operation when the host cancels the execution using it, without deleting the
Rune value or requiring another poll? A resource can need cancellation even
when it has no spawned task and no active executor poll.

## Proposed decisions

### 1. Preserve bindings and revoke the operation's resources

Keep the session's existing publication and failure semantics. Cancellation
must not clear bindings, declarations, history or unrelated extension state.
The retained future remains a value; its native operation becomes terminally
cancelled. Polling it afterwards reports a catchable cancellation error and
must not reconnect, retry, resume a transaction, or dispatch work again.

Reset and normal host teardown revoke all pending tracked operations for that
serving context, including additional references to the tracked owner retained by an
adapter. This is not a promise about process abort or SIGKILL.

### 2. Cancellation follows the execution that polled the operation

Each serving context has an internal execution generation counter, advanced
when execution starts. Every poll stamps the tracked owner with the current
generation; touched means its stamp equals that generation. It is never
displayed, and renumbering cannot change it. Creation time alone is insufficient
because input 2 can first poll a future created by input 1. Counter exhaustion
must refuse further execution rather than wrap and reuse an identity.

Ctrl-C, a VM/runtime failure, or budget exhaustion cancels pending operations
touched by that execution. A successfully completed input does not cancel its
pending futures merely because a select chose another branch. An operation
retained from another input but not polled by the failed execution is not
silently cancelled. A caught host Result::Err is normal Rune control flow.
Compilation and admission refusals have run no user code and cancel nothing.

This policy deliberately preserves the useful difference between a pending
future and a cancelled one. It also avoids treating a typo in an unrelated
cell as an instruction to close every resource an application owns.

HTTP does not currently follow this policy. Its private cleanup cancels all
active requests on interrupt and reset, regardless of which input polled them;
ordinary runtime failures and budget halts do not clear them. This record leaves
that behavior unchanged. A later record, provisionally titled "HTTP follows the
execution that polled it" (number not assigned), must explicitly migrate HTTP
and gate the changed failure and unrelated-request behavior. Until then this
policy applies only to lifecycle-aware extensions; it is not a claim of one
cancellation policy across every battery.

The implementation must identify the shared execution boundaries first:
file run, eval/session, and worker. Cancellation is completed before reporting
settlement or returning to the prompt. Reset and teardown cancel all remaining
operations independently of their last execution identity.

### 3. The host needs revocable ownership, not a notification-only callback

A callback telling an adapter that cancellation happened does not by itself
solve ownership of a connection buried inside a retained future. The proposed
primitive is a tracked future whose inner native future is separately owned
and synchronously removable. The wrapper holds the owner strongly through Rc;
the registry holds it weakly. The owner holds an optional pinned boxed future
without Send or Sync bounds. The wrapper polls it; cancellation takes and drops it
before returning, even if the wrapper remains reachable from Rune.

Registration is lazy with respect to I/O: wrapping a future must not poll it
or open a connection. The registry must not create an ownership cycle that
keeps abandoned futures alive. Normal completion and dropping the last wrapper
release registry entries. A stale wrapper after reset or teardown must remain
cancelled rather than becoming associated with the next session generation.

The cancellation path runs between polls, not reentrantly inside the native
future's poll. No lock or mutable registry borrow may be held while calling
adapter poll/drop code. A cancellation token checked only on the next poll,
a detached task, an extra block_on, and a forced session reset do not satisfy
this contract.

This first primitive covers futures whose resources close synchronously when
the inner future is dropped, as 0052's driver does. It does not offer asynchronous
shutdown, pooling, arbitrary background tasks or cancellation of blocking work.
Native adapters remain trusted, as in 0051; hostile or blocking destructors
cannot be bounded by an instruction budget.

### 4. Expose the smallest additional supported boundary

Keep Extensions::with unchanged. Add Extensions::with_lifecycle whose builder
receives the lent Rune module and a clonable Scope. Scope exposes one operation:

```rust
fn track<F, T>(&self, future: F) -> impl Future<Output = Result<T, String>>
where
    F: Future<Output = Result<T, String>> + 'static,
    T: 'static;
```

Cancellation returns Err("operation cancelled".into()). There is no public
wrapper or error type and no second conversion path. This adds one public type,
Scope, and one Extensions method: the root public inventory grows from three
names to four, not five. Context construction and execution remain private.

One review recommendation needs a distinction established by compilation.
Rune 0.14.2's Function trait requires Send + Sync on the registered function,
while its async implementation requires only Future + 'static on the returned
future. A closure capturing Rc fails both registration bounds. A registered
async function holding Rc across await compiles. Consequently the tracked
future, its owner and its registry need no Send or Sync, but a Scope captured
by a registered closure cannot itself contain those Rc owners.

The accepted resolution is a Send + Sync Scope handle
containing only an opaque, never-reused context identity and its owning thread
identity. Identities come from a process-wide atomic counter; exhaustion
refuses construction rather than wrapping. The actual weak-owner registry
remains thread-local. track resolves
only that exact live context on its owning thread; another thread, a retired
context or a stale scope refuses without polling the supplied future.
Wrong-thread use returns "operation scope used on the wrong thread"; retired
context use returns "operation scope belongs to a retired context". These are
adapter errors, distinct from "operation cancelled". No fallback to the currently active context is permitted.
Retirement removes the registry, and stale handles cannot create it again.
This uses no unsafe Send/Sync implementation and moves no Rune value or native
future between threads. A handle being transferable is not permission to run
its operation on another thread.

The external fixture must prove these bounds with Rune's actual registration
API, including a non-Send inner future, two distinct contexts, wrong-thread
use, and a scope surviving context retirement. If safe lookup cannot preserve context
ownership, stop; do not add unsafe access to a borrowed or polled future.

### 5. Cancellation failure is visible and ends the affected host context

A panic while revoking an adapter operation is not successful cleanup. The
implementation must not print an ordinary settled/idle response and continue
with unknown resources. The proposed policy is a named lifecycle failure and
retirement of the serving context, with the worker reporting state loss through
its existing terminal-failure path. It must attempt remaining independent
revocations without holding the registry lock. Unwinding panic conversion and
restoration of other threads' hooks follow 0051's constraints; abort remains
outside them.

The worker reports failure category `runtime` with `state_lost: true`, follows
the existing terminal settlement/acknowledgement path and retires. The kernel
then takes its WorkerDied path rather than presenting a usable idle context.
The CLI emits the named lifecycle refusal and exits unsuccessfully; a session
must not return to its prompt after this failure. No new claim that arbitrary native
cleanup is bounded in time is made.

## Acceptance gates proposed for review

1. An external fixture registers a tracked, lazy native future. Creating it
   opens nothing. Dropping an unpolled value leaves no registry entry. Normal
   success, returned Err and final-wrapper drop release resources and entries.
2. Repeat all seven 0052 cases through Rune, preserving their descriptor checks
   before another input or runtime turn. No task-drain workaround is introduced.
3. Repeat the retained-binding counterexample: an earlier published future is
   polled through select and interrupted. The original socket inode is gone
   before settlement; later polling that retained value gives cancellation and
   opens no replacement socket. Other bindings remain intact.
4. Repeat retained-future cancellation for a runtime failure and budget halt.
   A caught Result::Err is not mistaken for VM cancellation. A pending operation
   not touched by that failed input remains usable. A successful select leaves
   its losing future usable, including a future published under an older input.
5. Reset and normal shutdown revoke pending operations even through additional
   references to the tracked owner. Renumber does not. Stale scopes/wrappers cannot attach
   themselves to a new generation. Registry size returns to baseline after a
   repeated-call fixture with bounded source storage.
6. Exercise run, eval, session and the notebook worker, with lifecycle failures
   before protocol settlement. An injected destructor panic proves retirement
   rather than false success: the worker reports `runtime` and `state_lost: true`,
   and the kernel follows WorkerDied. Other independent resources are still revoked.
7. Stock CLI/REPL/worker behavior stays byte-identical for fixtures without
   lifecycle extensions. Settings, help, version and selfcheck invoke no hooks.
   Existing 0051 extension assembly and public-private boundary tests are updated
   explicitly for the accepted API addition. Root and kernel suites, notices,
   formatting and inherited-clippy comparison pass; startup is measured.
8. The PostgreSQL prototype changes only its operation wrapper to use the new
   interface, then reruns its ownership evidence. Only after this is reviewed
   does 0052 resume implementing its database contract.

## Risks and decisions still requiring review

The core risk is the lifetime model, not the number of registration methods.
The current execution identity must be accessible during each native poll
without leaking a public Session. Any thread-local registry lookup must use
the exact context identity, never an ambient "current context" fallback. Reset, process::exit and worker retirement have
different exit paths; the integration audit must name each explicit cleanup
site because process::exit does not run Drop guards.

An operation can remain pending across successful inputs by design. That can
keep a resource open while the REPL is idle; the difference is that cancellation
and reset now have an explicit way to revoke it. Server-side transaction outcome
remains independent of client revocation, and no retry is implied.

This proposal chooses execution-scoped cancellation rather than cancelling all
extension work after any failed input. The HTTP policy difference is explicit. The capture-safe Scope handle and its
thread-local lookup are accepted with the distinct misuse errors above. Nothing
in 0052 authorizes implementing those choices silently inside the adapter.
