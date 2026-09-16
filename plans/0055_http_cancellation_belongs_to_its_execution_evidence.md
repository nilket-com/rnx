# 0055 evidence: HTTP cancellation belongs to its execution

Implemented and measured by Codex on nano/Linux, 2026-09-16. Plan `2c76d38`.
Implementation complete, review pending. This closes the measured cleanup stop
from 0054; it does not select the server's admission shape or HTTP library.

Executable fixtures and raw results are in rnx-bench commit `92388a5`,
`probes/http-lifecycle-0055/` and `results/http-lifecycle-0055/`. The assembly
harness records the root HEAD plus its exact tracked patch, probe hashes,
lockfile, compiler, CPU information, command and test executable hashes.
The previous `server-assembly-0054` evidence and fixture remain unchanged.

## Ownership and boundaries

The four HTTP registrations take borrowed URL/method strings and snapshot the
owned Request, including options, headers and body, at the native call. A
validation error is stored and remains a catchable attributed Err on await.
`Scope::track` owns the inline fetch and conversion. There is no rnx request
spawn, JoinHandle, abort handle, request id ledger or active-task guard.
Reqwest/hyper's internal tasks and the blocking system resolver remain.

Stock serving contexts now activate the same lifecycle used by extensions.
Activation is after version/help and pure settings evaluation. HTTP installation
requires its explicit scope. The public library API, root dependency graph,
manifest, lockfile, notices, kernel package and adapter implementation are
unchanged. Source review finds no alternate execution counter or untracked
HTTP registration. Existing begin/finish calls determine revocation.

Runtime failure, budget exhaustion and execution interruption revoke only
requests polled by that execution. A successful select or caught Err preserves
pending operations. An unrelated failed input and a compile refusal preserve
an older operation. Editing Ctrl-C no longer calls HTTP cleanup. Reset revokes
all and drops the client, while preserving the live context for later input.

`State::cancel` only releases that owner's cached client; lifecycle revocation
happens at the caller. Neither path enters a runtime. `Session::close` retires
its lifecycle, releases the client, then invokes `drain_shutdown` on the stopped
runtime it owns. Session EOF/:quit and worker shutdown use it. Reset does not.
Run/eval and explicit exits retain their existing one-shot status handling and
shutdown_background behavior, including the blocking DNS residual.

A lifecycle destructor failure also releases the cached client immediately
when finish retires the session; a unit gate checks this before return.

Worker shutdown performs cleanup before its two stream barriers and settlement,
then waits for ack. A final drain failure reports a runtime failure and state
loss, and exits 1 after ack. The injected never-ending task is compiled only
with test-support. No drain allowance was increased: it remains 100 ms.

## Gates

1. The root HTTP unit fixture wraps the real inline request with a drop
   observer, registers it as a Rune native future, and retains its Rune binding
   across executions. After a failing execution polls it, the observer is zero
   immediately on return, before another runtime turn. Repoll says
   `operation cancelled`. An unrelated runtime error or budget failure leaves
   the observer at one. Reset also drops the started inner synchronously;
   final close closes its socket. The separate lazy-call test invokes all four
   actual HTTP registrations and observes that no client was constructed.
   Invalid options are catchable on await and likewise construct no client.
2. Root integration tests preserve an older request through unrelated runtime
   failure and compile refusal, let unrelated work run past its deadline, and
   require the original deadline refusal on repoll. A touched runtime failure
   produces cancellation instead. The stock-worker fixture independently gates
   unrelated versus touched SIGINT, caught errors, reset and shutdown. The PTY
   test starts a real request, interrupts an unfinished edit, then receives the
   original response. Reset permits a subsequent `42`; the worker fixture uses
   an explicit 64 MiB allocation ceiling. No immediate reset-memory or EOF
   promise is inferred from these checks.
3. Both complete 0054 assembly runs pass with the real batteries. Same-worker
   cleanup runs inside block_on, obtains clean read-zero EOF for the failed
   side within the 500 ms fixture allowance, and observes the healthy side
   still open. That request later returns its original `"ok"`. Cross-worker
   isolation also passes. Both end at zero async tasks after all owners retire.
   The fixture retains both admission shapes and all ownership/builder-count
   assertions. No whole-runtime zero-task check runs while a healthy owner lives.
4. Pool tests keep the original healthy connection after cancelling another
   request, reuse it, then release it on reset and observe clean EOF after
   runtime progress. The request held past its deadline refuses on repoll.
   Creating a lazy future does not start networking or that deadline.
5. All four calls work twice with the same URL, method and options bindings.
   All four snapshot a URL before reassignment. The request fixture mutates
   method, headers, bytes and timeout after future creation and receives the
   original POST body and header; caller bindings retain their mutations.
   A string-body snapshot likewise survives mutation. Existing validation
   messages remain attributed to the original URL.
6. The final-drain unit test refuses a live task at 100 ms and succeeds after
   it ends. The actual-worker test injects that condition and checks failure,
   state loss, both stream barriers, ack ordering and exit 1, with a 500 ms
   external tolerance. Normal stock-worker shutdown of a held request observes
   clean EOF before settlement. The original stalled-DNS run/eval/session/reset
   tests pass. Existing explicit-exit and child-process status tests pass.
7. The existing loopback response suite passes: status, methods, repeated
   headers, redirects, proxy behavior, TLS verification, decoded cap, UTF-8,
   incomplete body, deadlines and guarded JSON handoff. No network or response
   policy was relaxed. Only lifecycle expectations change.
8. Default root suite: **374 passed**. Test-support suite: **417 passed**.
   Formatting and notices checks pass (124 packages; the pre-existing one
   unresolved licence entry remains). Existing external assembly and lifecycle
   fixtures pass. PostgreSQL's seven cases plus retained-interrupt pass on a
   private cluster, with zero query sockets before another input in the retained
   case. Real Jupyter supervision and extended recovery fixtures pass, including
   worker death, interruption, queued requests and descendant cleanup.

The assembly adaptation needs one extra mechanical change beyond installer and
cleanup signatures: it must poll the healthy inline future until the fixture
sees its request headers. The old one-poll prime relied on a detached task
making further application-level progress. The same-worker fixture polls it
alongside the request observation; the cross-worker fixture continuously drives
it on its own runtime until release. Held peers, failure timing, clean EOF and
subsequent original response remain the assertions. This is recorded explicitly
rather than claiming a source-identical replay or restoring a hidden task.

## Cost and delayed release

The baseline release binary was built before edits at the plan revision, whose
production source equals `20f8310`. It already contains 0053 and its recorded
roughly 2% JSON regression. This comparison does not use a hypothetical zero-cost
lifecycle baseline or claim to explain/recover that earlier regression.

Final release SHA-256:
`5f8a5cc653185408d12275b71627f163a0ab70922b599dc57ba36baf0f4208ab`.
Before: `0409ee9e21dc0c9affe7622c80bb6156e4cbfc944243910a622ead2c3b60fe79`.
Size: 15,187,704 -> 15,184,024 bytes (3,680 bytes smaller).

CPU 4, eight warmups and 80 process samples per label, interleaved ABBA blocks.
These wall times include the parent process's spawn/wait/capture overhead and
use no per-sample shell. Outputs agree byte for byte. Millisecond medians:

| Workload | Before | After |
| --- | ---: | ---: |
| version | 1.937 | 1.934 |
| help | 1.958 | 1.946 |
| bare run | 4.877 | 4.822 |
| bare eval | 5.322 | 5.281 |
| 10k JSON | 13.071 | 12.794 |
| CPU loop | 9.134 | 9.001 |
| string loop | 5.993 | 6.000 |

No speedup or causal code-layout conclusion is claimed. The string median is
0.12% higher; the other medians are lower. These are observed differences, not
an isolated performance cause. Version/help reach their returns before
lifecycle construction; extension fixtures retain the zero-builder assertions.

The private HTTP/1.1 fixture explicitly enables TCP_NODELAY. Each round reuses
one connection for 21 sequential requests. Two before medians are 0.630/0.600 ms;
after 0.611/0.588 ms. Signal-to-settlement is 7.55/7.48 ms before, 1.40/1.27 ms
after. These are different completion promises: the latter is revocation,
not a transport drain. They are observations, not a new timing guarantee.

One allocator transcript reports 1,838,920 baseline live bytes before and
1,839,174 after (254 more). Holding one unpolled future raises those figures by
2,046 and 3,717 respectively; the latter includes snapshotting and tracking.
These are global allocator request bytes, not RSS or an isolated accounting of
one component. Source storage and inspection also contribute.

A separate started-request transcript makes deferred transport release visible:

| Stage | Before | After |
| --- | ---: | ---: |
| held request before reset | 1,906,542 | 1,907,177 |
| immediately after reset | 1,839,072 | 1,868,693 |
| after 50 ms runtime progress | 1,839,381 | 1,839,929 |

The roughly 28 KiB net decrease between the latter two after samples is not
relabeled synchronous cleanup; the intervening input also adds source storage.
0034's current-status note and README explicitly replace its before-next-prompt
socket/drain and immediate transport-memory promises.

## Limits and incidental correction

No Windows execution or new Windows cross-check was performed. The existing
async CPU-loop interrupt limitation and blocking DNS residual remain. Inline
cancellation does not establish whether a remote side effect occurred. A retained
unpolled request may hold transport resources until repoll, revocation or owner
retirement. No stronger pool-completion primitive or new public API was added.

The full suite exposed an existing package README link to the independent
PostgreSQL workspace, which Cargo does not ship. Its link now uses the repository
URL, matching the package's existing link rule. No package/dependency contents
changed. Test-support inventory counts increase by one for the deliberately
stuck shutdown-task fixture; the shipped module inventory is unchanged.
