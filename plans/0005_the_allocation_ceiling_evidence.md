# rnx 0005 evidence: the allocation ceiling

Implementation of `plans/0005_the_allocation_ceiling.md`, 2026-09-08, on
Linux x86_64 against upstream Rune 0.14.1 with no patch. Status of the
record stays proposed until reviewed. Figures below are from a debug build
on this machine.

## What landed

- `src/memory.rs`: one counter, an `AtomicUsize` adjusted by each event, so
  a sample is one load of one value. The `GlobalAlloc` implementation
  accounts for allocation, zeroed allocation, deallocation, and both
  outcomes of reallocation; the hooks allocate nothing and cannot unwind.
  `live()` returns `None` when the `count-allocations` feature is compiled
  out. `record_baseline()` uses a compare-and-exchange against a sentinel,
  so the reference point is recorded once and no later call can replace it.
- `src/session.rs`: a `ceiling` and a latched `over_ceiling`. `sample()`
  reads the figure and latches when it is at or above the ceiling; `eval`
  refuses on the latch without re-testing. `DEFAULT_CEILING` is 512 MiB,
  overridable by `RNX_MEMORY_CEILING`.
- `src/repl.rs`: `handle` takes one input and returns an outcome, so every
  disposable thing it made is dropped when it returns. The loop then
  drops the input buffer, refreshes the completion snapshot, and takes
  exactly one sample, at one site, covering every kind of command. The
  input buffer is dropped before the sample rather than at the end of the
  iteration, because the record excludes it from the figure. The baseline
  and the first sample are taken after initialization and history loading
  and before the first evaluation is admitted. A reset that leaves the
  figure at or above the ceiling prints that restarting may be necessary.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 a thousand retaining inputs | pass, with the numbers below | scripted session, and `tests/repl.rs::gate_0005_the_ceiling_latches_*` for the latch and the messages |
| 1b every command is sampled | partly by test, partly by construction; see the note below | `gate_0005_the_ceiling_latches_and_a_reset_that_cannot_recover_says_so` |
| 2 the figure moves with the payload, under control | pass: two arms in one session, control then payload | `gate_0005_the_figure_moves_with_the_payload_against_a_control` |
| 2b the hooks account correctly | pass: null allocation, zeroed allocation, growing, shrinking, same-size, and failed reallocation, on the counter's own instance, so the result owes nothing to what the rest of the process allocates. The installed counter is exercised in its own process by the payload gate, not by a process-wide comparison inside the parallel unit suite | `src/memory.rs::every_event_is_accounted_and_a_failure_is_not` |
| 3 a failed input still charges | pass, measured: an input that grows a retained vector 16 MB past a ceiling 4 MB above the current figure and then panics leaves the growth in place; the sample after the failure charges it, the next evaluation is refused, and `:vars` still shows the mutated binding | `gate_0005_a_failed_input_that_grew_shared_state_still_charges` |
| 4 reclamation is listed | pass, measured, cycles included; see the table below | scripted session and `gate_0005_a_self_referential_value_is_not_reclaimed_by_a_reset` |
| 5 a reset that cannot recover says so | pass: with a one-byte ceiling, the reset reports, evaluation stays refused, and inspection answers | `gate_0005_the_ceiling_latches_*` |
| 6 the cost is on the record, and a disabled build says so | pass; see the timings below | both builds of this revision |
| 7 not a hard limit | pass: an input allocating 16 MB past a ceiling 4 MB above the current figure returns its value, and the refusal arrives at the next input | `gate_0005_an_input_past_the_ceiling_is_not_interrupted` |
| 8 nothing regresses | pass: 40 unit tests and 13 pseudo-terminal tests green, formatting clean, spike self-checks pass | whole suite |

### Gate 1, measured

One thousand inputs, each binding a closure, into a session with the
default ceiling:

| Point | Tracked live allocation request bytes |
| --- | --- |
| Startup reference point | 1,780,131 |
| While running the thousand inputs | reached 537,530,080 and the ceiling refused the rest |
| After `:reset` | 1,793,367 |

The default ceiling of 512 MiB was reached partway through, so the gate
also exercised the refusal: every later input was refused with the message
naming `:reset`, and the latch held without re-testing.

### Gate 4, reclamation

| Case | Result |
| --- | --- |
| A thousand retained closures and their units | reclaimed: 535,749,949 bytes of growth fell to 13,236 above the startup reference point, 99.998% returned |
| Retained source text and source maps | reclaimed: the source component went from 9,891,933 bytes to 0 |
| Residue after reset | 13,236 bytes above the reference point, not reclaimed. It is not attributed: history and editor storage grow with the session's inputs by design, and the counter cannot say which allocation is whose |
| A self-referential value holding 8 MB | **not reclaimed**: 8,397,519 bytes above the reference point after `:reset`, against under a megabyte for the same payload without a cycle. Rune reference counts values and has no cycle collector |

A self-referential value is now measured, and it does not reclaim. An eight
megabyte string placed inside a vector that then holds itself survives
`:reset` in full: the figure stayed 8,397,519 bytes above the startup
reference point afterwards, against a control that dropped back under a
megabyte with the same payload and no cycle. The reason is that Rune's
values are reference counted with no cycle collector, so the cycle keeps its
own payload alive and nothing the session drops can reach it. A reset cannot
recover that memory; only restarting can, which is why the message says so.

### Gate 6, the cost of counting

One revision, one workload, the hundred-input self-check, three runs each:

| Build | Runs | Median |
| --- | --- | --- |
| counting compiled in | 24.9, 26.6, 25.4 ms | 25.4 ms |
| counting compiled out | 20.2, 20.3, 21.2 ms | 20.3 ms |

About 25% on a workload that is compile-and-execute heavy and therefore
allocation heavy. The build with counting compiled out reports
`allocation accounting is disabled in this build: no figure is tracked and
no ceiling is enforced.` and prints no figure and no ceiling.

## A finding worth its own record

A thousand inputs each retaining a closure cost about 536 MB, roughly 536 KB
an input, and reached the default ceiling. A thousand inputs that retain no
closure cost 3.7 MB, about 3.7 KB an input.

The difference is not the compiled units. A unit is about 3.7 KB here, which
is what the second figure is made of. What a retained closure pins is the
runtime context: `FnOffset` holds an `Arc<RuntimeContext>` beside its
`Arc<Unit>`, the session builds a fresh runtime context for every input in
`execute`, and one runtime context measures 462,739 bytes. So every retained
function value keeps its own half-megabyte copy of the standard library's
runtime alive.

Building the runtime context once and sharing it is the cut, and unit
retention is a real but much smaller second question at 3.7 KB an input.
Both belong in their own record with these numbers in front of them.

## Note on gate 1b

The record says every command is sampled: evaluation, inspection, reset,
and abandoned input alike. What the tests establish directly is that a
reset takes a sample, since a one-byte ceiling re-latches through the new
session, and that the startup sample precedes the first evaluation, since
the first evaluation is refused. That inspection commands and abandoned
inputs are sampled is established by construction, one sample site after
`handle` returns, on every path, and not by a test that drives the ceiling
with inspection commands alone. Forcing that case would need a ceiling
tuned between two measurements of a live process, which would be a flaky
test rather than a stronger one.

## Limits stated

- The figure is tracked live allocation request bytes. It counts requests
  through Rust's global allocator only, excludes native allocations outside
  it, mappings, thread stacks, and child processes, is not resident memory,
  and is not the session's share of anything. `:memory` says all of this.
- The startup reference point and the net change are explanatory. Neither
  gates anything, because a difference can fall while the session grows.
- The ceiling is checked between inputs, so the input that crosses it
  completes first.
