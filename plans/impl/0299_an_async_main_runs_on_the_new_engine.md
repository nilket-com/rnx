# Implementation plan 0299: An async main runs on the new engine

Status: Implemented

Paired ADR: ../adr/0299_an_async_main_runs_on_the_new_engine.md.

1. Record the old engine's outputs for every witness first: value,
   located error, budget, interruption and arity.
2. Report the protected-source hooks for review before code changes: the
   file-entry compile policy, how a file caller learns the entry is
   async, and the sliced waiting invocation's completion and error
   conversion.
3. Admit a declared `pub async fn main` with direct awaits of enrolled
   waiting host functions, under the file-entry policy only; every other
   async form stays refused at compile time.
4. Run the admitted entry through `start_sliced` under
   `with_host_wait`; completion precedence, budget, interruption, located
   errors and exit codes as decided.
5. The witnesses, refusal and no-retry controls, census 0212 and its
   tables, mutations.
6. fmt, Clippy, the suites, the census, preflight and the commit
   checker; sign; source review of the protected boundary; one complete
   final; Results.

## Results

`rnx run` compiles through `frontend::compile_file`, which admits a
declared public, top-level `async fn main` (`Call::Async`, no impl
item, a declared body, the root `main` hash, public `ItemMeta`) at both
async-function refusal sites. It returns the entry mode apart from
`Program`. The body lowers under `AwaitPolicy::FileHost`, which reaches
only the enrolled wait-adapter arm, with its arity and adapter checks.
Under it, async blocks refuse as `AsyncBlock` and script awaits as
`Await`; closures keep `Refuse`. Ordinary `frontend::compile` and
opaque-session callers keep their admission, and a declared async
helper still refuses the whole file before effects.

A synchronous entry keeps the complete invocation. An async entry runs
under one `Runtime::with_host_wait` loan, through
`PreparedInvocation::start_sliced` with the full budget and 1024-work
slices. SIGINT is checked before each resume; a settled value or an
unrelated failure keeps its result. The host-wait cancellation marker is
consumed inside the loan. Every outcome after preparation is terminal,
through the file renderer and lifecycle, with exit 130 and `interrupted`
on interruption. A SIGINT during a synchronous host call, such as one
running a child, keeps that host's ordinary result, and the run stops at
the next slice boundary if the invocation is still running then. The pending host wait ends at its own
cancellation cadence, as on the old engine.

Census 0212 is `both-admit` under the file-entry rule and stays
admission-only with the reason `zero-arg-async-file-entry`, which holds
for that case alone. The census reports 92 of 92, and its 71 executed
comparisons show no difference. The CLI controls compare against the
old engine byte for byte for argv, wrong arity, a located error after a
wait, the budget before and after a wait, and SIGINT during a pending
host wait, which is driven by readiness. Separately:
- SIGINT during a running Rune loop, also driven by readiness, stops the
  new route at a slice boundary with exit 130. This is the new route's
  improvement and is not compared with the old engine;
- async helpers, inline async blocks, stored futures and `select`
  decline before effects, each effect seen once.
Six mutations were rejected: ordinary compile, a private entry, broad
await admission, completion precedence, pending-wait classification and
a retry after an effect.

The first complete final failed in `final-controls` alone. The cause was
the ADR 0290 scheduler control
`capacity_returned_after_the_recheck_is_not_lost`, intermittent and
outside this record: 2 failures in 200 isolated runs. Its arming takes
the first token event after the chunk write to be the write's own,
while a late notice from an earlier return can come first. The
controller started the waiter, so nothing was lost. Its fix follows as
the first fix commit.

The unchanged retry passed all 101 stages in 36.2 minutes, with no
memory.high, memory.max or OOM events. The fixed-route band holds on
the synchronous route: all 32 per-record rows pass, the largest change
+0.111%, and all 32 cumulative rows pass. The anchor cache missed, with
reason `key differs`, as in every final since 0282.
