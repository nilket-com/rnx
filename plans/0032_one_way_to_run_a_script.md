# rnx 0032: one way to run a script

Status: proposed 2026-09-13. The thirty-second record of rnx, and the async
foundation record 0031 gate 2 asks for. Three entry points drive the VM in
two different ways today, and neither way can wait on a future. After this
record there is one way, it can, and everything the two old ways promised
still holds.

## Context

`run` calls `Vm::call` once under one budget (record 0021). The session
resumes a `VmExecution` in slices of 10,000 instructions under a budget
each, checking the interrupt flag between slices (record 0002). `eval` goes
through the session. None of the three can execute `.await`: Rune halts
with `Awaited` and the synchronous resume reports it as an error.

Record 0031 needs `.await` for the batteries it selects — `http::get`,
`fs::read_to_string`, `time::sleep` all return futures — and asks the
foundation to prove, for all three entry points:

- synchronous scripts unchanged;
- instruction accounting that survives a pending poll, with no fresh budget
  on wakeup;
- interruption while a future is pending and while pure Rune code runs;
- source-positioned diagnostics after an await;
- what a failed or interrupted input leaves behind;
- shutdown: no pending work outlives an input.

Before writing this, a probe against Rune 0.14.2 answered what the source
alone does not (its results are in the evidence):

1. `Vm::execute` on a `pub async fn main` runs the body as the head
   execution — "everything is just async when called externally", says
   `set_entrypoint` — so `async_resume` returns `Complete(value)` directly,
   not a future value, and a budget halt leaves it resumable exactly as it
   does for a synchronous function. A CPU loop of about 130,000
   instructions took 13 slices of 10,000 whether or not an await sat in
   the middle of it.
2. `budget::Budget<F>` restores the remaining budget on each poll and saves
   it after, so a slice that goes pending resumes with what it had left.
3. An interrupt flag set by another thread during a five-second pending
   sleep was observed within two milliseconds, by selecting the slice
   against a future that checks the flag on a five-millisecond cadence.
   During a pure CPU loop it was observed at the next slice boundary.
4. Dropping an execution whose host future is pending drops the future.
   Nothing was left running; nothing had been spawned.

That is enough to build on without inventing anything.

## Decision

### 1. One driver, three callers

A new module owns execution: given a VM, an entry point, its arguments, a
slice size and a total budget, it resumes the execution slice by slice on
a Tokio current-thread runtime and returns one of four outcomes —
completed with a value, halted for budget, interrupted, or failed with the
VM's error. `run`, `eval` and the session call it and interpret the
outcome each in their own words, as they do today.

Each slice is `budget::with(min(SLICE, remaining), async_resume())`
selected against an interrupt future. A budget halt is recognised as it is
now, by two structural facts: the error carries no location, and the
budget guard is exhausted at the moment the poll settled. Nothing reads
the error's text. When a slice halts for budget, the driver spends it,
checks the flag, and starts the next; when the flag is seen, whether at a
boundary or while pending, the driver drops the execution and reports
interruption.

### 2. `run` becomes interruptible, and its budget is unchanged

Slicing a run's budget does not change it: the slices sum to the budget
the operator chose, the last slice is whatever remains, and exhaustion
reports the same line it does now. What changes is that Ctrl-C ends a run
at the next slice boundary or as soon as a pending future is dropped.
Record 0021 said the bound could not be removed until "a file run being
reliably interruptible"; this record makes it so and still does not remove
the bound, which is record 0021's decision 2 and stays.

An interrupted run says `interrupted` on standard error and exits 130,
which is what a shell reports for a process ended by Ctrl-C, so a script
around rnx reads it the same way. `eval` does the same. The session prints
`interrupted` and returns to the prompt, as it does now.

### 3. The session's generated `main` is async

`pub fn main(__rnx_state)` becomes `pub async fn main(__rnx_state)`, so an
input may write `.await` at the top level. Everything else about the
generated source is unchanged: the same bindings restored, the same delta
published, the same map from generated offsets to inputs. A declaration
the input makes — `fn f() {}` — is synchronous unless the input says
otherwise, as in a file.

### 4. One runtime per command, kept for a session

The runtime is built after the context, so records 0030's `version` and
`help` still answer before either exists. `run` and `eval` build one, use
it, and drop it on the way out. The session builds one when it starts and
keeps it across inputs and across `:reset`; a reset frees the session's
values, not the runtime. It is a current-thread runtime with the timer
enabled and nothing else: no worker threads, no I/O driver until a battery
record needs one and measures it. The driver spawns no task, so there is
nothing to outlive an input.

### 5. What a failed or interrupted input leaves

Exactly what it leaves today. The session publishes bindings only after a
completed run; an interrupted or failed input publishes nothing, though a
shared value it mutated stays mutated, as record 0002 says. The pending
future, if any, is dropped with the execution before the outcome is
returned, so by the time the prompt reappears no future of that input
exists. A host function that spawned native work would have to stop it in
its own drop; the foundation adds no such function and record 0031 makes
that each battery's burden.

### 6. What this record does not decide

- Any battery. No module is installed; `.await` has nothing to await
  except a test fixture.
- The I/O driver, worker threads, or blocking pools: the HTTP record
  measures what it needs.
- The memory ceiling's treatment of runtime allocations: they count, as
  record 0031 says, and the evidence reports the baseline shift; whether
  the ceiling should exclude them is not this record's question.
- Ctrl-C at the prompt, which rustyline owns, and Windows console events,
  which record 0025 owns.

## Acceptance gates

1. **Synchronous scripts are unchanged.** Every existing suite passes.
   The output and exit status of `eval`, `run`, and the session for a
   synchronous script are byte-identical before and after, checked on the
   committed JSON workload and the eval-outcome gates.
2. **`.await` works at all three entry points.** A test-support fixture,
   `host::test_pending(ms)`, resolves after a timer. A file whose `main`
   awaits it, an `eval` that awaits it, and a session input that awaits it
   at top level each complete with the right value. A synchronous `main`
   in a file still runs.
3. **The budget survives a pending poll.** Under a chosen budget, a script
   that spends N instructions, awaits, then spends N more halts at the
   same budget as one that spends 2N without awaiting; and one that awaits
   in a loop under a small budget halts for budget, not for anything else.
4. **Interruption reaches a pending future and a CPU loop.** A run and a
   session input pending on the fixture end within a stated bound of
   Ctrl-C; a run and an input in an infinite loop end within one slice.
   The run exits 130 and says `interrupted`; the session returns to the
   prompt and the next input runs to completion.
5. **Diagnostics keep their place.** A runtime error after an await names
   the file, line, and column under `run`, and the input under the
   session, as today.
6. **Nothing is left pending.** After an interrupted input the session's
   next input runs; after an interrupted run the process exits without
   waiting. The evidence states what was checked.
7. **Cost is measured.** `version`, `help`, `eval 42`, `run` of a trivial
   file, and the JSON workload, before and after, under the bench's
   conditions; and the session's startup baseline from `:memory`.

## Guardrails and stop conditions

1. One driver. If `run` and the session need different loops, stop: the
   record has not found the one way.
2. No fresh budget on a wakeup. If the count of slices for a script
   changes when an await is added, stop.
3. No spawned task. If a battery needs one, it gets a record.

## Risks

- **A host function that blocks the thread blocks the runtime.** True
  today of `host::process` and unchanged; the driver is not concurrency.
- **The 5 ms cadence costs a wakeup while pending.** Only while pending,
  and record 0020's own waits already poll on a cadence.
- **Tokio joins the dependency tree.** `rt` and `time` only; record 0029's
  notices are regenerated.

## Forward

Record 0031's gate 3, JSON, which needs none of this, and gate 4, HTTP,
which needs all of it.
