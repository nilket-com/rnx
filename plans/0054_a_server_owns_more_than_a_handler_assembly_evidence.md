# 0054 gate 2: real context assembly, and a cleanup stop

Status: measured 2026-09-16 against rnx `c28cf22`. **Gate 2 is not closed.**
The two admission shapes assemble and the ownership isolation checks pass,
but today's HTTP cleanup cannot serve a long-lived multiplexed runtime.
No admission shape, HTTP boundary or HTTP library is selected by this evidence.

## Reproduction and scope

The source is in rnx-bench `probes/server-assembly/`; raw evidence is in
`results/server-assembly-0054/run-0` and `run-1`, with a summary alongside.
Each run pins the root revision, probe and harness hashes, root lockfile,
release test binary, compiler, CPU layout and command. The test is pinned to
CPUs 2 and 4 on nano, distinct physical cores. Each has three samples per
shape/workload. No public network or system database is involved.

The harness archives rnx into a temporary directory, adds a private test
module and a cfg(test) accessor to the existing Runtime, and runs one selected
release unit test with test-support. It changes no manifest, lockfile, ordinary
library API or shipped source. The temporary checkout is removed. Root suites
are not claimed as rerun; the root implementation is unchanged from the
accepted baseline. The two logs each show the one probe test passing, which
means the stop below was reproduced, not waived.

The factory calls the real core, fs, path, time, text, HTTP and env installers,
then Extensions::with_lifecycle/install_with. No copied battery or replacement
Scope is used. Its private asynchronous VM wrapper applies one whole budget
and calls async_resume once, like the existing async path; it is not a new
supported server entry. The existing synchronous drive_async owns block_on and
is not called from inside the prototype's long-lived runtime. No budget halt
is resumed. Native blocking and the nested-async resumption defect are unchanged.

## What is shared, and what is constructed

One schema context compiles one Unit per complete run. That Unit crosses
worker threads and is reused throughout the run. Each serving context gets a
separate RuntimeContext, Lifecycle, Scope and HTTP State. This preserves the
current closure-capture design and demonstrates compatibility for the fixture's
identical registrations. It does not prove sharing one stateful RuntimeContext.

The two executor threads each own a current-thread runtime and local task set.
Every handler has its own VM, budget and Rune values. Responses are converted
with rnx's guarded JSON writer before crossing a thread boundary. env::args
captures identical immutable host strings and returns fresh Rune values; env
variable reads retain their existing process-wide behavior.

Serial admission creates one serving context per worker, reused only for
serial executions. Multiplexing admits at most four VMs per worker, each with
a fresh serving context. No concurrent handlers share a lifecycle state.

Builder, context and retirement counts are assertions. For each awaiting batch,
serial admission uses two builders and two contexts; multiplexing uses seventeen.
The CPU batch uses two in either shape; saturation uses two versus three. The
schema builder is counted separately once. Across a full run there are 93
context/builder invocations and 93 retirements, including the isolation cases.
Every retirement also refuses a new tracked call with the retired-context
message. The library's existing once-per-process entry contract is not changed;
these are explicit private-factory counts, not an accepted new public promise.

## Scheduling measurements

Awaiting batches enqueue sixteen 40 ms sleepers, then one quick request behind
worker zero's queued work after observing initial VM entry. CPU batches observe
a loop starting on worker zero, then send healthy work to worker one. Saturated
batches observe both loops before sending healthy work to worker zero. Each
loop gets 10,000,000 instructions and finishes with the pinned VM's limited halt.
All healthy values, CPU failures and per-case build/retirement counts are asserted.

Healthy dispatch-to-completion medians, milliseconds (not HTTP latency):

| Shape | Awaiting batch, run 0 / 1 | One CPU worker, run 0 / 1 | Both CPU workers, run 0 / 1 |
| --- | ---: | ---: | ---: |
| One VM per worker | 329.98 / 329.76 | 0.027 / 0.025 | 57.74 / 59.14 |
| Four VMs per worker | 125.35 / 130.92 | 3.069 / 2.961 | 59.70 / 59.48 |

For awaiting batches, completion rate rises from 51.5 requests/s to 108.5–115.9
requests/s. These are finite-batch dispatch rates, not HTTP throughput or a
steady-state capacity claim. Multiplexing improves I/O concurrency but does not
provide CPU preemption. Fresh context construction also adds latency to the
spare-worker CPU case, unlike a reused serial context.

Awaiting-batch context construction sums 14.9–15.3 ms for serial admission and
93.4–103.7 ms for multiplexing. These are sums of wall-clock construction
intervals, not CPU time. Serial initialization precedes request submission;
multiplexed construction happens after submission and is included in latency.
Compilation is timed separately in the raw schema event.

Peak tracked allocation growth for awaiting batches is about 4.57 MB versus
7.64 MB. The real test-support allocator measures the whole process, including
fixture queues, result rows and thread/runtime allocations; this is not RSS.
The retained-delta samples after thread joins still include result rows and
harness objects (about 50 KB for the awaiting batch), so they are not labelled
leaks or a production retained-memory bound. Matching batches are compared;
no generic context-pooling optimization is assumed.

## Lifecycle isolation

The native extension retains tracked futures strongly in a fixture map, beyond
the lifetime of a failing VM. It first starts an older operation in context A.
Context B then starts its own operation, selects a short timer and panics.
B's operation must yield `operation cancelled` when polled again; A must remain
live and later return its original value. Both same-worker multiplexing and
cross-worker cases pass. Contexts retire, retained wrappers are removed, and
live inner-operation counters reach zero. This is stronger than merely dropping
all values at the end of a request.

## HTTP ownership passes; the cleanup interface stops assembly

Separate actual HTTP states issue requests to two held loopback fixtures. B
fails while its request is pending. Its fixture sees clean read-zero EOF;
A sees no EOF and later receives `ok`. A read error is refused as evidence of
clean closure. With separate worker runtimes, invoking B's cleanup between
block_on calls drains B to zero while leaving A alone. Both complete runs pass.

On the same runtime, two independent problems reproduce:

1. Calling State::clear from inside the long-lived block_on panics when
   Runtime::drain_http tries a nested block_on. The ordinary panic catcher is
   used only to record the failure; no production continuation is proposed.
2. As a diagnostic control, pausing the runtime and calling clear outside it
   returns `HTTP cleanup did not finish within 100 ms`, measured at 103.1 and
   103.3 ms. There are four runtime tasks before this control. A's healthy
   HTTP request is still legitimately pending, so the whole runtime cannot
   reach zero. B nevertheless closes cleanly and A survives.

After recording the stop, the fixture releases A, verifies its response, closes
both states, observes zero runtime tasks and joins both socket-fixture threads.
Nothing is left running. This cleanup of the experiment is not a workaround
that makes the proposed server contract pass.

The source agrees: State::clear aborts that state's requests and drops its
client, then calls a synchronous drain whose completion condition is the entire
runtime's task count. Ownership of requests is distinct, but completion accounting
is not. Serial admission also needs an await-compatible boundary if its worker
stays inside one long-lived block_on; multiplexing additionally needs cleanup
scoped to an owner while unrelated work remains live.

## Consequence

Keep gate 2 open. Specify and measure an await-compatible, owner-scoped HTTP
cleanup path before selecting admission or proceeding to gate 3's HTTP boundary
and library choice. That can begin privately; this result does not itself
justify a new public lifecycle primitive. Preserve existing CLI/session behavior,
and do not substitute a global runtime drain or cancel healthy handlers to meet
a zero-task assertion. The registration/context construction cost is measured
and belongs in the subsequent shape decision; it is not a reason to silently
share captured scopes.

The fixture's initial compile-name shadowing, Debug-based output assertion and
Python executable-hashing typo were corrected before these two complete runs.
They were harness defects, not rnx findings. No gate was weakened to obtain the
final results, and the actual HTTP failures remain explicit stop evidence.
