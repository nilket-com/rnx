# rnx 0047: a notebook cell over the worker

Status: proposed 2026-09-14. The forty-seventh record, following the accepted
worker in 0046. This is the first usable Jupyter kernel: installation, execution,
text output, errors, interruption and restart. Completion, inspection, rich
media and interactive stdin are later records. No kernel implementation has
started under this record.

## Context

The worker is implemented at 60e79fe, with the boundary evidence in rnx-bench
071505c. It exposes execute/reset/shutdown, not completion or is_complete.
Its Python parent is a fixture, not an installed notebook supervisor. Its
render_bounded field describes use of a bounded renderer, not proof that a
particular result was truncated. Python 3 remains a declared acceptance-test
prerequisite; missing Python must fail the worker gate, not silently skip it.

The local evcxr checkout was read, including connection.rs, control_file.rs,
jupyter_message.rs, core.rs and install.rs. Its Jupyter crate selects zeromq
0.6.0 with Tokio and TCP. That is a candidate dependency configuration, not
measured compatibility or a proof of bounded buffering. No source is copied
by this draft. Any later copying keeps the applicable copyright and licence
and appears in the kernel's notices.

Normative references checked 2026-09-14:

- [Jupyter messaging](https://jupyter-client.readthedocs.io/en/stable/messaging.html)
  defines framing/signing, request fields and message sequencing. The published
  page now describes 5.5, including an XPUB change. This first kernel advertises
  **5.4**, with PUB IOPub, and claims no 5.5 subscription handshake or later
  registration-file handshake. Compatibility with the chosen client is gated.
- [Kernel construction and installation](https://jupyter-client.readthedocs.io/en/stable/kernels.html)
  describes connection files and kernelspecs. Installation will delegate directory
  selection to Jupyter instead of copying evcxr's directory-discovery code.

## Decision

### 1. Separate executable, same repository

Add `jupyter/`, an independent Cargo package with its own lockfile and workspace
boundary, building `rnx-jupyter`. It depends on the worker protocol, not Rune or
the rnx executable's internal modules. Ordinary rnx builds do not resolve or
compile the kernel's ZMQ dependencies. Keep rnx's version/release gates intact;
the kernel also remains unpublished at 0.0.0.

Start with a probe of zeromq 0.6.0, using only Tokio runtime and TCP transport,
against Python's real jupyter_client/pyzmq stack. Resolve and lock compatible
HMAC-SHA256 and UUID dependencies during that probe, recording the exact graph,
features, licences, build time and sizes before committing the implementation.
A copied dependency list is not evidence. If bounds or lifecycle require a
transport change, stop and revise this decision rather than conceal it.

CLI: `rnx-jupyter --connection-file FILE --rnx ABSOLUTE_EXECUTABLE`.
The kernel launches that exact worker with null stdin and explicit inherited
control endpoints. No shell, PATH rediscovery or parsing REPL transcripts.
Require worker protocol 1 and the expected rnx/Rune versions before accepting
execution. An incompatible worker is a startup refusal naming both versions.
The notebook launch directory becomes the worker's working directory; the
connection-file directory is not a script cwd. Environment is inherited.
Personal REPL config/history remains outside this path.

### 2. Wire format and independent channels

Use ROUTER shell/control/stdin, PUB IOPub and REP heartbeat. HMAC covers the
four serialized JSON dictionaries in wire order; preserve routing identities
and the original request header. Verify before dispatch. Support hmac-sha256;
an empty key explicitly disables authentication. Never log the key.

This version accepts TCP connection files with a numeric loopback address and
five distinct nonzero ports. Refuse wildcard/non-loopback addresses, other
transports and registration files with a named unsupported-setting error.
These are deliberate local-kernel limits, not claims about Jupyter's limits.
Read a regular connection file, capped at 64 KiB. Extra connection/message keys
are tolerated for protocol evolution; wrong types for fields actually used
are refused. Invalid signatures produce no execution and no authenticated
reply pretending the sender was known. Header IDs are opaque strings.

Heartbeat and control remain responsive independently of execution, worker
pipe collection and blocked IOPub sends. No shared lock may hold control behind
an executing cell or a publishing task. Kernel-info identifies Rune 0.14.2,
implementation rnx, protocol 5.4, file extension .rn and text/plain output.

### 3. Execution, counters and source identities

Validate the execute envelope before scheduling it. Follow Jupyter's silent,
store_history and stop_on_error semantics, including a reply on errors and the
execution counter on every execute reply. Maintain the notebook counter in the
kernel; never derive it from the worker input index. An empty silent counter
query need not admit a worker input. Silent execution still drains and accounts
for worker output; suppression is a deliberate sink, not an unfinished handoff.
Busy/idle status remains available for frontend scheduling.

Retain history when requested, in memory only, with oldest-entry eviction at
16 MiB or 10,000 entries. Do not implement history queries in this first record;
retrieval is deferred, not simulated with fabricated entries. Keep a separate
bounded mapping from worker generation/epoch/input to notebook count. Evict old
mappings at 10,000 entries; an unmapped origin keeps its worker identity rather
than borrowing the calling cell's count. Silent inputs have no invented visible
history number. Preserve the worker's plain diagnostic and attach structured
origin information as namespaced metadata.

Exactly one worker operation is active. Queue at most 64 execute requests and
4 MiB of their serialized payloads. On a script error with stop_on_error set,
fail the already-queued execute requests as ExecutionAborted without running
or counting them; new requests after that queue snapshot remain admissible.
Source past the worker's 32 KiB limit settles as a refused execution, not an
attempt to split a cell. Colon commands are source, not notebook controls.

Nonempty user_expressions receives a per-key UnsupportedFeature error after the
main execution, without evaluating extra source or mutating bindings. Never
pretend those expressions succeeded. No input_request is sent, even when the
frontend permits it: host::stdin keeps 0046's EOF/one-read semantics. Completion,
inspection, debugger, comms/widgets and rich displays are unsupported and are
not advertised. Do not add regex guesses for Rune assistance.

### 4. Results and the actual publication boundary

For nonsilent requests: busy, execute_input as applicable, stream messages,
then an execute_result for non-null text_plain or an error, execute_reply,
and finally idle. Use one ordered IOPub publishing owner. Across shell and
IOPub there is no promise of arrival order. Every message/chunk retains its
request's original parent header; never consult a mutable latest-request slot.

Use only text/plain. Ignore the worker's render_bounded field in notebook metadata. It identifies
a preview renderer, not actual truncation. Leave the worker protocol unchanged;
a real truncation indicator requires its own measured renderer follow-up.
Failure names are stable mappings of the worker categories; traceback is the
plain diagnostic split into lines. A Rune panic is an ordinary cell error.

Scan byte barriers before decoding streams. Incremental UTF-8 decoding preserves
valid characters across chunks, replaces invalid sequences with U+FFFD and
flushes an incomplete final sequence at the barrier/cap boundary. Record that
replacement occurred in rnx metadata. This is a notebook text policy, not a
change to worker byte fidelity. NUL/newlines remain JSON-representable text.

Keep 0046's 2 MiB per-stream raw collection cap and continue draining discarded
bytes. Emit one clearly rnx-labelled truncation notice per affected nonsilent
stream, with its discarded byte count. Give notices a separate fixed 1 KiB
allowance; they cannot recursively consume the script-output allowance.
Late output between operations has an empty parent header and a separate bounded
allowance; output during another interval retains 0046's causal ambiguity.

Do not ack until the worker reply and both barriers are present, and retained
output has been handed to IOPub or explicitly suppressed by silent policy.
This means acceptance by the local publishing transport, **not acknowledgement
by JupyterLab or proof a browser displayed it**. PUB has no per-message frontend
receipt. Ordinary connection/subscription startup must be tested against the
real client; no sleep is evidence of guaranteed delivery.

No successful reply/idle is fabricated after an incomplete worker boundary.
If publishing fails or stalls past the five-second settlement deadline, stop
accepting work, terminate/reap the worker and fail the kernel visibly. If the
channel itself is broken, failure messages may be undeliverable; process exit
is the fallback, not a false successful idle.

### 5. Interrupt, shutdown and state loss

Kernelspec interrupt_mode is message. An interrupt request acknowledges receipt
on control, not completion of cancellation. Hold a pending interrupt until the
worker's armed event; discard it for a request refused before admission. Deliver
only to the worker/process group selected for that worker, never the notebook
server. Reuse the existing platform signal mechanisms and preserve the async
CPU-loop limitation. No automatic kill deadline is added to ordinary execution.

Restart/shutdown can hard-stop a worker independently of all I/O. An idle worker
gets acknowledged shutdown with the 0046 five-second bound. An active worker
gets an interrupt and a bounded grace period, then force termination if needed;
use five seconds for the whole shutdown operation, not successive unbounded
waits. Reap the worker. Test descendant behavior separately; killing a Unix
worker is not assumed to kill its separately grouped process:: children.
A descendant surviving shutdown is a stop condition requiring an explicit
containment decision before this record is marked implemented.

Echo restart in shutdown_reply and exit; the Jupyter manager starts a new kernel.
Do not secretly respawn a worker under the same live kernel session. Unexpected
worker death reports WorkerDied/state loss where channels permit, fails queued
requests, and exits nonzero. No replay of cells. A new kernel has a fresh session
identity and counter, and has no old bindings.

### 6. Bounds need a transport probe

Application limits: 1 MiB total incoming multipart payload, 32 parts and 64 KiB
per routing/header field; unsupported binary buffers are bounded then refused.
Outgoing stream chunks contain at most 16 KiB raw data before UTF-8/JSON
conversion. Budget encoded publishing payload separately at 16 MiB, since raw
bytes can expand under JSON escaping. Do not count only queue item numbers.
When application admission is full, return a named busy/refusal rather than
retain another unbounded request. Unknown optional request types may be ignored.

These are application limits, not claims that checking an already allocated
ZMQ message bounds the transport's memory. Probe the crate's receive allocations,
internal queues/high-water behavior and disconnected/slow subscribers. Record
what is bounded, what may be dropped, and what remains owned after cancellation.
If the transport can accumulate unbounded data behind these checks, revise the
transport or configuration before implementing the rest. Do not sell PUB send
success as delivery or a total process memory ceiling.

### 7. Installation and acceptance

`rnx-jupyter install --rnx ABSOLUTE_EXECUTABLE` generates a temporary kernelspec
and invokes `jupyter kernelspec install --user --name rnx` without a shell.
Record absolute kernel and worker executable paths in argv, including the
connection_file placeholder. Display name is Rune (rnx), language rune.
Refuse an existing installation unless --replace was explicitly passed. This explicitly requires a working `jupyter` CLI; a Desktop-only installation
without that CLI is refused with installation guidance rather than falling back
to guessed directories. No auto-install/update during kernel startup. Refuse non-Unicode paths that cannot
be represented by the kernelspec JSON, naming the offending path.

Tests install into a temporary Jupyter data directory and use a dedicated Python
environment with recorded jupyter_client, pyzmq, nbclient and JupyterLab versions.
Do not change the user's active kernelspec as part of a test. The kernel package
gets reproducible third-party notices for its own dependency graph; rnx's
notices and default build graph must stay unchanged unless a separately stated
shared-code edit makes that necessary.

## Gates

1. Probe the chosen ZMQ transport against real Python sockets before integration:
   signed round trips, wrong signatures, routing prefixes, extra fields, empty
   key, malformed multipart, oversized frames, stalled publisher and heartbeat/
   control responsiveness. Preserve source, lockfiles, commands and raw traces
   in rnx-bench. Stop on unresolved unbounded buffering or lifecycle failures.
2. Drive execute with jupyter_client: persistent bindings, unit, compile/runtime
   errors, origins from retained closures, silent/empty requests, history/count
   choices, user_expressions refusals, queue limits and stop_on_error both ways.
3. Carry all worker byte-boundary gates through IOPub: partial lines, UTF-8 splits,
   invalid bytes, stale markers, caps, blocked handoff and immutable parent headers.
   Prove result/error precedes idle on IOPub; do not claim cross-channel ordering.
4. Interrupt while synchronous, awaiting and supervising a child; document async
   CPU behavior. Restart and shutdown with a blocked sink and surviving-descendant
   fixtures, worker death at partial boundaries, bounded reap and explicit state
   loss. Kernel-info/heartbeat/control must remain responsive during execution.
5. Install in temporary directories including spaces; open JupyterLab, select
   Rune (rnx), execute multiple cells, interrupt, restart, save and reopen an
   actual notebook. Preserve notebook and screen evidence alongside an automated
   nbclient run. Use a headless Chromium browser driven by Playwright on nano, with versioned
   browser/tooling and genuine page screenshots. Protocol tests alone do not
   establish the JupyterLab gate.
6. Run both rnx suites sequentially with Python present, kernel tests separately,
   notices and formatting checks. Compare rnx's ordinary command bytes, dependency
   graph and matched startup timings; measure kernel-ready and first-cell time
   separately from warm cells. Windows type checking remains distinct from an
   executed Windows/Jupyter acceptance run.

## Deferred

Completion/is_complete/inspect need a worker assistance protocol record. Rich
JSON/HTML/media, display updates, widgets, frontend stdin, disk-backed history,
remote kernels, registration handshake and arbitrary background-task ownership
are not quietly added here. This draft is ready for review; the transport and
process-containment probes precede an implementation claim.
