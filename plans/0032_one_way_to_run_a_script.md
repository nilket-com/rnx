# rnx 0032: one way to run a script

Status: proposed 2026-09-13; revised the same day after review. The
thirty-second record of rnx, and the async foundation record 0031 gate 2
asks for. Three entry points drive the VM in two different ways today, and
neither way can wait on a future. After this record a script that can await
runs on a third way that can; the two old ways stay exactly as they are for
every script that cannot, and everything they promised still holds.

**Revision.** The first draft put every script on one sliced driver. Review
(Codex) found that a nested `async fn` of more than one slice's instructions
returned an error under it, and a probe found worse: resumed past that halt,
the execution completes with `()` in place of the nested value. Decision 1
and gates 3 and 4 are rewritten below; the title is kept, because the one
way is now the way the script's own shape chooses, and that is the decision.

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
5. **A budget halt inside a nested async function is unrecoverable.** An
   `async fn` awaited from Rune code runs as a nested execution wrapped in a
   future value; a budget halt inside it resolves that future to an error,
   and Rune drops a settled future. Resuming the outer execution afterwards
   completed with `()` where a loop summing to 1,999,000 should have been —
   silently. With the whole budget installed once, the same script returned
   1,999,000. A synchronous call is a frame in the same execution, not a
   nested one, so a synchronous execution's slices are resumable, as record
   0002's have always been. Rune 0.14.2 offers no per-instruction hook other
   than the budget: `VmDiagnostics` fires on function calls only.
6. A future that only reads a flag does not wake when the flag is set: a
   store to an atomic wakes no task. The pending-interrupt latency in 3
   came from a timer on a cadence, and that is what the driver uses.

That is enough to build on without inventing anything, and finding 5 is
what decides the shape of the decision.

## Decision

### 1. The script's shape chooses its path

A script that can await — one that writes `.await`, or declares an
`async fn` — runs on a new path: the whole execution under its whole budget
on a Tokio current-thread runtime, raced against a future that reads the
interrupt flag on a cadence while the execution is pending on a host
future. A script that cannot await runs exactly as it did before this
record: a file on `run`'s one call under one budget, a session input on
record 0002's sliced resume with the flag read between slices. Neither
synchronous path changes by a byte, and the evidence checks that.

The split is not a preference. Finding 5 says the budget cannot slice an
execution that may nest, and any execution that can await may nest; and
finding 6 with the absence of any other hook says a running execution can
be bounded by nothing but its budget. So an execution that can await is
not sliced, and one that cannot keeps the slices it always had. The test
for "can await" is the presence of the tokens that make it possible; a
false positive sends a synchronous script down the async path, where it
still runs correctly, and a false negative cannot happen.

All three paths report one of five outcomes — completed with a value,
halted for budget, interrupted, yielded, or failed with the VM's error —
and `run`, `eval` and the session interpret the outcome each in their own
words, as they do today. A budget halt is recognised as it always was, by
the budget guard being exhausted at the moment the execution settled; on
the async path the error's location is not consulted, because a halt
inside a nested future carries that future's location and is exhaustion
all the same.

### 2. What Ctrl-C ends, on each path

A session input that never awaits: at the next slice boundary, as always.
A session input or a file or an `eval` that is pending on a host future:
within one cadence of the flag, and the execution and its future are
dropped. A file that never awaits: not mid-loop, as it never was — record
0021 says why the bound stays, and this record does not remove it. A
script that can await and is running Rune code rather than waiting: not
mid-loop either; its budget is its bound, and the session's budget is the
ten-second net record 0002 chose. That last case is the cost of finding 5,
it is stated here rather than hidden, and it is the second thing worth
taking upstream after the startup cost: a cooperative interrupt hook that
does not go through the budget.

An interrupted run says `interrupted` on standard error and exits 130,
which is what a shell reports for a process ended by Ctrl-C. `eval` does
the same. The session prints `interrupted` and returns to the prompt.

### 3. The session's generated `main` is async when the input can await

For an input that can await, `pub fn main(__rnx_state)` becomes
`pub async fn main(__rnx_state)`, so it may write `.await` at the top
level. For any other input the wrapper is what it was, and `:debug` shows
the same text it always showed. Everything else about the generated source
is unchanged: the same bindings restored, the same delta
published, the same map from generated offsets to inputs. A declaration
the input makes — `fn f() {}` — is synchronous unless the input says
otherwise, as in a file.

### 4. One runtime per command, kept for a session

The runtime is built after the context, so records 0030's `version` and
`help` still answer before either exists. `run` builds one only for a file
that can await, uses it, and drops it on the way out; a file that cannot
never has one. The session builds one when it starts, whichever inputs
follow, and keeps it across inputs and across `:reset`; a reset frees the
session's values, not the runtime. It is a current-thread runtime with the timer
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
3. **The budget survives a pending poll, and a nested async function
   survives the budget.** Three scripts with the same instructions,
   awaiting at the start, the middle, and the end, need the same smallest
   budget, and that budget is several times a slice. One that awaits in a
   loop under a small budget halts for budget, not for anything else. And
   the review's script — a nested `async fn` of more than a slice's
   instructions — returns its value under a budget that allows it, from a
   file and from a session where the function was an earlier input.
4. **Interruption reaches a pending future, and the synchronous slices are
   still there.** A run, an `eval`, and a session input pending on the
   fixture end within a stated bound of Ctrl-C; the run exits 130 and says
   `interrupted`, the session returns to the prompt and the next input
   runs to completion. A synchronous session input in `loop {}` is still
   ended within a slice. A file in `loop {}` is ended by its budget, as it
   always was, and by nothing else.
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

1. The shape chooses, and nothing else does: no flag, no environment
   variable, no per-call option picks a path.
2. Nothing slices an execution that can await. If a future change needs
   to, finding 5 has to be found false first.
3. The synchronous paths are the old code. If either changes by a byte,
   that is not this record.
4. No spawned task. If a battery needs one, it gets a record.

## Risks

- **A host function that blocks the thread blocks the runtime.** True
  today of `host::process` and unchanged; the driver is not concurrency.
- **The 5 ms cadence costs a wakeup while pending.** Only while pending,
  and record 0020's own waits already poll on a cadence.
- **A running async script cannot be ended by Ctrl-C.** Stated in decision
  2. In `run` the default budget ends it in milliseconds; in the session
  the net is about ten seconds, and an input that can await and loops
  without awaiting is the one input that waits for it.
- **The shape test reads tokens.** `async` in a string sends a synchronous
  script down the async path. It runs correctly there; it only loses the
  slices, and only for that script.
- **Tokio joins the dependency tree.** `rt` and `time` only; record 0029's
  notices are regenerated.

## Forward

Record 0031's gate 3, JSON, which needs none of this, and gate 4, HTTP,
which needs all of it.
