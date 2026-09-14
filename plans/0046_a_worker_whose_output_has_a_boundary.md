# rnx 0046: a worker whose output has a boundary

Status: implemented 2026-09-14; Windows transport execution remains open. The forty-sixth record of rnx, and the first
step toward a Jupyter kernel. It defines a persistent evaluation worker and
its parent-side collection contract. It does not implement Jupyter sockets,
kernelspec installation, notebook history, rich display or interactive stdin.
The transport and stream-boundary probe is the first implementation gate,
not an assumption that the platform work is already solved.

## Context

`Session` already owns persistent bindings, retained declarations, compilation,
source origins, instruction budgets and cancellation. It is separate from
rustyline. A notebook needs that session behind a machine interface, without
parsing its prompts or conflating script output with worker replies.

Source reviewed on 2026-09-14: rnx at `428d93d`, and the local evcxr checkout
at `3fae8c3529d5d0821c8af1e2c8c9b4d9eb1dc1e4`. Evcxr's runtime writes a
fixed `EVCXR_EXECUTION_COMPLETE` line to stdout. Its evaluator reads stdout
until that line, while stderr is forwarded separately. Its Jupyter forwarding
code chooses the parent request from `latest_execution_request` when sending
a queued string, rather than storing that identity when collecting the output.
The source permits a queued-output attribution race on either stream. This
record does not claim that a failing notebook was reproduced.

Rune 0.14.2's `print_impl` writes through Rust's stdout lock without flushing.
The explicit stdout flush in rnx's host exit path is not a per-input flush.
The worker must own its end-of-operation flush. A parent reading bytes rather
than lines cannot pull bytes still buffered inside the child.

The accepted completion rule is:

> A cell finishes only after its control reply and both stream barriers have
> arrived, and all preceding output has been published under that cell's identity.

A barrier proves an ordering boundary on one pipe. It does not prove that all
writers have terminated, and there is no total ordering across two pipes.
The limits of that distinction belong to this contract.

## Decision

### 1. A session worker, with a separate control channel

Add an explicit advanced entry point:

```
rnx worker --control-read N --control-write N
```

The parent creates two one-way control pipes, plus the worker's stdout and
stderr pipes, before spawning. `N` denotes an inherited file descriptor on
Unix or an inherited handle value on Windows. The platform layer validates
and takes ownership of these endpoints. They must be distinct from standard
input/output/error and usable in the declared directions. Invalid startup
arguments are refused with exit 2; unusable control transport is fatal with
exit 1. Successful shutdown exits 0.

The parent explicitly restricts inheritance to the intended worker handles;
the worker makes its control endpoints non-inheritable before evaluating any
code. Neither a `process::` child nor its descendants may retain them and
prevent the parent observing control EOF. Keep platform-specific endpoint
construction out of the protocol parser. Gate 1 must demonstrate this on
Unix and type-check the Windows mechanism, with Windows execution separately
required before that platform is marked verified. If ordinary inherited pipes
cannot meet these constraints, stop and revise the transport decision rather
than quietly replacing it with a public listener or standard-stream framing.

Control carries bounded UTF-8 JSON lines in both directions. Stdout and stderr
carry script bytes and the private stream barriers only. Stdin is the null
device, not the command channel. No shell parses a worker launch command.
The existing finite `process::run` API is not used to supervise this persistent
worker: the parent needs concurrent collection and explicit restart/termination,
not a call that waits for a child's final reply.

Worker startup installs the same scripting modules and empty `env::args`
snapshot as a session, and constructs one `Session`. No rustyline, history
file, personal colour config, splash, title or prompt is used. Worker-owned
presentation is plain even if global `--color=always` is supplied. Script
`print!` and `host::eprint` retain their existing raw-output behaviour.
Run, eval, help and version retain their existing dispatch/config paths.

### 2. A small versioned protocol and one operation in flight

The worker's first control message is `ready`, with protocol version 1, rnx
and Rune versions, and the numerical limits below. A parent must reject a
protocol version it does not support. This is not a public network service
or a stable third-party wire standard at version 0.0.0.

The initial requests are `execute`, `reset` and `shutdown`:

```json
{"id":1,"op":"execute","source":"let x = 41;","nonce":"<64 lowercase hex digits>"}
{"id":2,"op":"execute","source":"x + 1","nonce":"<fresh 64 lowercase hex digits>"}
{"id":3,"op":"reset","nonce":"<fresh 64 lowercase hex digits>"}
{"id":4,"op":"shutdown","nonce":"<fresh 64 lowercase hex digits>"}
```

IDs are positive integers no larger than `2^53 - 1`, strictly increasing for
this worker lifetime. Reset does not reset IDs. Overflow requires a new worker;
there is no wraparound. The nonce is 256 bits supplied freshly by the parent
from its OS random source for each request; deterministic fixture values are
allowed in the probe. The worker validates the spelling, not a claim of entropy.
Unknown keys, unsupported operations and invalid field types are protocol
errors, not Rune source. No request payload is interpreted as a REPL colon
command. Notebook execution counts are not worker request IDs.

One state-changing operation is outstanding at a time. The parent waits for
the complete boundary in decision 4 before sending another request, including
reset or shutdown. The worker sends a `settled` control reply, then waits for
an `ack` carrying that request ID before it admits another operation. An ack
means the parent has consumed both stream barriers and handed off all retained
output to its destination in order. A later Jupyter adapter must not ack just
because it received the control reply or saw empty pipes.

This makes the worker protocol useful without a notebook while leaving
completion, completeness, inspection and notebook-specific history/counting
to the assistance record. The worker's protocol reader is not an editor parser.

### 3. Execution reuses session semantics and returns structured results

`execute` calls the existing session adapter, with the existing source limit,
instruction budget, async-promotion rule and failed-input publication policy.
Take the session's memory sample at the same admission boundary as the REPL;
calling `Session::eval` alone does not reproduce that sampling policy.
A failed input may retain mutations to shared values, as in the REPL.

The settled reply identifies `id`, a reset epoch, and the admitted input index,
or null when the session refused before admission. The epoch starts at 1 and
increases only on successful reset. Origins for retained code carry that epoch
and the defining input index, plus line, column and excerpt. The parent can map
these stable identities to cells without deriving them from a displayed prompt
number. There is no worker `renumber` request in this record.

Successful values use the existing bounded plain renderer. Return `text_plain`
as a string, or null for unit, with no prompt/result-number decoration. Text
rendering does not require JSON serialization of the Rune value; functions,
cycles and other non-JSON values keep their existing renderer representation.
Rendering is completed before writing the stream barriers. The control JSON
serializer serializes this response structure, never an arbitrary Rune value.

Failures use the existing `Failure` variants: refused, compile, runtime,
interrupted, budget and over-ceiling. Return a stable category, the existing
plain diagnostic, and structured origin/budget/ceiling fields where present.
Do not infer an error kind by parsing the diagnostic's prose. Rune panics and
ordinary script errors settle the request and preserve the session; they are
not worker crashes. `host::exit` keeps session refusal semantics and does not
become permission for a cell to exit the worker. Its existing REPL-oriented
wording can be retained; adding machine operations does not reinterpret source.

`reset` clears the session under its existing contracts, including HTTP cleanup.
The worker must observe cleanup failure as a control failure rather than merely
printing it and reporting success; a fallible session entry point may be shared
with the current REPL wrapper without changing that wrapper's presentation.
Failure after destructive reset work is not rolled back: report state loss and
retire the worker instead of pretending the old bindings survived.

`shutdown` performs cleanup and emits its settled reply and barriers before
waiting for ack and exiting. The parent's shutdown deadline remains the escape
from a blocked cleanup or missing ack. Unexpected Rust panic/abort, transport
loss or worker exit before settlement is worker failure, not a synthetic
successful cell. Restart uses a new worker identity and loses bindings; no
silent replay or automatic re-execution of the cell is allowed.

### 4. Byte barriers on both streams, with an explicit parent acknowledgement

For each accepted request, including reset and shutdown, the worker performs:

1. Execute and construct the bounded control result (including cleanup).
2. Lock stdout, flush it, write its barrier, flush again, and release the lock.
3. Do the equivalent on stderr.
4. Write and flush the `settled` reply on the control pipe, then await ack.

The locks protect the worker's Rust writer sequence, not arbitrary native
writers sharing the descriptor. Never hold both stream locks together. The
parent drains both streams and control concurrently; waiting for control before
reading a full output pipe would deadlock this sequence. Arrival order across
the three pipes is not prescribed, even though the worker writes them in order.

The barrier spelling is a bounded ASCII frame with control delimiters:

```
RS RNX-WORKER-1:<id>:<stdout|stderr>:<nonce> US
```

Here RS is byte `0x1e`, US is byte `0x1f`, and the displayed spaces are not
written. No newline is inserted before or after a barrier. A fresh nonce makes
accidental collisions overwhelmingly unlikely; **it is not an unforgeability
or sandbox guarantee against code running in the worker**. This record does not
claim to withstand a script deliberately reproducing the full current marker.

The parent scans bytes, not lines, with a bounded partial-match buffer. A
barrier may span any number of reads. False prefixes and wrong-ID/nonce frames
are ordinary output, preserved byte-for-byte. Strip only the full expected
barrier on the appropriate stream. At EOF, a pending partial match is ordinary
output followed by an incomplete-stream failure, not a completed barrier.
No UTF-8 decoder sits before barrier detection. The later Jupyter record must
choose how arbitrary non-UTF-8 stream bytes become notebook text.

Tag each collected output chunk immediately with worker identity, request ID
and stream. Preserve per-stream order and retain that identity through queues.
A forwarding task never consults a mutable "current request" when publishing.
After a barrier, that stream is closed for the operation. Only the control
reply, both barriers, and completed forwarding/discard accounting together
permit ack. There is no empty-pipe or quiet-for-N-milliseconds shortcut.

### 5. Partial lines and late writers have stated limits

No-newline stdout is allowed to remain buffered until the worker's end-of-cell
flush; there is **no prompt-progress guarantee for partial lines**. The probe
checks that the final bytes arrive without an invented newline, not that they
arrive while the cell is still running. Newline-terminated output and stderr
can be forwarded as collected, subject to scheduling and backpressure. No new
print/println override or timer-driven flusher is introduced here. A flushing
print API, if needed by notebook progress output, is a separate decision.

Collection-time tagging fixes delayed forwarding of bytes already collected.
It cannot establish which code produced bytes written after a barrier. Bytes
collected after a stream's barrier and before the next operation starts are
reported as unassociated late output, never retroactively added to a completed
cell or saved for the next cell. Once a later operation begins, this raw pipe
cannot distinguish its writes from a surviving writer's: attribution is to
the active interval, not proven causal ownership.

The first worker supports sequential session execution with no promise of
cell-owned background output. Existing runtime tasks and process cleanup are
not replaced by a claim that every writer in the operating system is gone.
A fixture must demonstrate the late-writer limitation rather than claim to
solve it. If the first notebook must preserve causal identity for arbitrary
background writers, stop and redesign the output architecture before declaring
this protocol sufficient. Adding nonces cannot close that gap.

### 6. Bounded collection and responsive parent control

Choose these initial bounds, advertised in `ready` and exercised by fixtures:

| boundary | bound |
| --- | --- |
| one control JSON line, excluding newline | 256 KiB, checked before allocation growth |
| execute source after JSON decoding | existing session limit, 32 KiB |
| rendered result | existing default renderer limit, 16 KiB |
| retained/forwarded script output per request per stream | 2 MiB |
| pending output queue payload per stream | at most 2 MiB |
| parent startup, settlement-after-reply and idle shutdown waits | 5 seconds each |

The control-line cap accommodates JSON escaping of a maximum-size source.
Oversized or malformed control frames are fatal: do not try to resynchronize
inside arbitrary source text. A response that exceeds the control cap must be
bounded before serialization with explicit truncation metadata, not cut into
invalid JSON. A script diagnostic that must be shortened is labelled as such.
The settled reply retains required identity and category fields.

The parent keeps draining and scanning after the output cap, discarding excess
payload and reporting per-stream truncation separately from worker failure.
Cap output at collection, not after an unbounded queue. Neither an output cap
nor a full downstream queue may prevent the stream reader finding its barrier.
Late/unassociated output uses a separate bounded allowance under the same
2 MiB rule. No truncated bytes are falsely described as published. A Jupyter
adapter will publish a truncation indication before considering that cell idle.

The 5-second waits are parent-supervision defaults with declared scheduling
tolerance in evidence, not a deadline for every cell. No fixed wall-clock cell
deadline is introduced: native filesystem calls can block outside VM budgets.
The parent must remain able to kill/reap a failed or stuck worker independently
of evaluation, output forwarding or control-pipe reads. If forwarding a result
cannot finish within the settlement wait, fail the operation/connection; do not
send a success ack with bytes still attributed through mutable state.

Interrupt uses the platform's signal/control mechanism, not a command queued
behind evaluation on the serial control pipe. A worker emits `armed` for an
execute request **after** the session clears the old interrupt flag and before
execution proceeds. A parent that received an earlier interrupt request holds
it until armed, or discards it when the operation is refused without admission.
The boundary must be integrated with session admission; sending armed before
calling today's `Session::eval` would let that method erase the signal.

This retains the known limitation: synchronous session execution is sliced,
but an async execution running Rune without awaiting is bounded by its budget,
not a cooperative Ctrl-C hook. Hard termination remains possible with loss of
session state. It is not reported as an ordinary successful interrupt preserving
bindings. Parent signalling must not accidentally signal the notebook server.

Stdin is deliberately unavailable as a frontend prompt in this version. With
null stdin, `host::stdin` observes EOF on its first read and retains its current
one-read rule. Reset does not invent a fresh OS stdin. The protocol never places
source text where `host::stdin` could consume it. A later stdin-request protocol
must explicitly respect the frontend's ability to refuse interactive input.

## Gates

1. **Probe the boundary before integrating it.** A small worker/parent pair in
   rnx-bench uses the proposed inherited endpoints and exact barriers. Preserve
   source, lockfile if needed, commands, versions and raw traces. Demonstrate
   independent control, simultaneous stream draining and no control inheritance
   in a spawned child. Unix execution and Windows type checking are separate
   results; a Windows execution result is required to close Windows transport.
2. **Byte fidelity and barriers.** No-newline stdout, heavy stderr, simultaneous
   streams, embedded newlines/NUL/non-UTF-8 bytes, false marker prefixes and a
   previous request's nonce. Test every split of a barrier plus one-byte reads.
   Exercise the caps with continued draining, then compare retained prefixes
   and truncation metadata. An incomplete barrier at EOF cannot become success.
3. **Queued and late output.** Deliberately hold forwarding while collecting an
   operation's bytes; no next operation is admitted before ack. Collected chunks
   keep their identity when forwarding resumes. A delayed background writer
   demonstrates unassociated output between operations and the stated inability
   to infer provenance once another interval is active. Report, do not disguise,
   that limitation in the trace.
4. **Persistence and origins.** Two executions share a binding; a compile error,
   Rune panic and budget failure settle without worker restart. A retained
   closure's error names its defining input. Reset clears bindings and advances
   epoch, not request IDs. Unit yields no text result. Refusal before admission
   returns no admitted input index. Memory sampling follows the session's gate.
5. **Interrupt and death.** Inject an interrupt before admission and after armed,
   while running a synchronous loop, while awaiting, and while a supervised
   child is active. Gate against the existing async CPU limitation rather than
   promising it away. Kill the worker before each barrier and before the control
   reply; close a control endpoint, block a sink, and force flush failure.
   Every parent wait/teardown is bounded and reaps its worker. No failed boundary
   produces a completed-cell event. Exercise acknowledged shutdown and hard
   termination separately, with state loss explicitly reported.
6. **Integration and regression.** No splash, history/config read, terminal title
   or rnx-authored ANSI on worker startup; script output remains raw. The control
   channel survives script stdin reads and output resembling JSON. Invalid CLI
   handles are refused. Existing default/test-support suites run sequentially.
   Preserve a before release and compare ordinary command output and matched
   startup measurements in rnx-bench; keeping Jupyter dependencies separate
   does not by itself prove adding the worker has zero cost.

## Guardrails and stop conditions

One `Session`, one execution path, no compiler fork, no parsing terminal output
for values or diagnostics. Control never shares a script standard stream.
No copied evcxr code without licence/notice handling. No ZMQ or Jupyter protocol
dependency is needed for this record.

Stop for review if the probe cannot establish bounded parsing/collection,
control-handle isolation, interrupt admission ordering, or a shutdown path
independent of the evaluator. Stop if correctness requires replacing print
semantics, replaying a failed cell, ignoring missing barriers, or promising
causal ownership for late writers. Prototype results, not line-count estimates,
determine whether these decisions are ready to implement unchanged.
