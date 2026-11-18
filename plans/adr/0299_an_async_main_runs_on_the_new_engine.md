# ADR 0299: An async main runs on the new engine

Status: Proposed

Base: published 7ba74521317f341d3f17a63ce93827bd7f556827 (ADR 0298).

## Problem

A file whose entry is `pub async fn main` never runs on the new engine.
The frontend admits an async function only inside a session wrapper
(`Collector::allows_async_functions` is `opaque_session.is_some()`), so
`rnx run` compiles the file, records `Refusal::AsyncFunction` and falls
back to the old engine before any effect:

```text
$ rnx run --debug-engine main.rn     # pub async fn main(args) { time::sleep(1).await; args.len() }
rnx: engine=legacy refusal=AsyncFunction
0
```

This is the last refusal in the old-core census: 91 of 92
legacy-admitted inputs admit on both engines. The remaining one, census
0212, is a zero-argument admission witness, `pub async fn main() {
time::sleep(1).await }`, and stays admission-only. It is roadmap item B2
of the 2026-11-14 removal inventory.

The pieces already exist on the session route. A session can admit an
async entry and its awaits, and `domain_session::awaiting::run` drives
the invocation in slices with interruption checked between slices. A
waiting host call blocks inside a slice on the session's runtime, lent
through `Runtime::with_host_wait`, and `host_wait_interrupted` ends it.
The file route uses none of that: it calls `prepared.invoke(budget)`
inside `Runtime::run_sync`, and `with_host_wait` refuses to start inside
an active runtime context.

Two arities are involved. `rnx run` passes one argument, the argument
list, on both engines. A zero-argument `main` therefore fails on either
engine with the same diagnostic, the async one on the old engine today:

```text
runtime error (no source position is available for this fault): Wrong number of arguments 1, expected 0
```

So census 0212 (an admission witness with zero arguments) and a real
`rnx run` (one argument, through the actual CLI) are different
witnesses, and both are needed.

## Decision

1. The file route admits a declared `pub async fn main` whose awaits are
   all direct awaits of already enrolled waiting host functions in its
   own body. Nothing wider in this cut: separate async script functions,
   inline async blocks, a future as a value, an unawaited async call,
   `select`, generators and any other await form keep their typed
   compile-time refusals, and so their fallback before effects.
   Admission is an explicit file-entry compile policy, separate from the
   session's opaque-value envelope: the session's
   `allows_async_functions` gate is not flipped, and ordinary
   `frontend::compile` callers and session admission keep their current
   contract. The census uses the same file-entry rule as `rnx run`. The
   caller learns that the selected entry is async from compiler
   metadata; a synchronous entry stays on the existing complete
   invocation.
2. The admitted async entry runs on the new engine's existing waiting
   execution, `PreparedInvocation::start_sliced` under
   `Runtime::with_host_wait`, without a `ProgramDomain` or the session's
   diagnostic wrapper. File execution has its own completion and error
   conversion, reusing the file renderer and lifecycle. The structured
   host-wait cancellation marker is consumed inside the runtime loan,
   which clears it on drop. A synchronous entry keeps its current path
   and cost unchanged.
3. The file run keeps the old engine's observable contract for an async
   entry:
   - a value: printed as now, exit 0;
   - a runtime error: located at its source position, exit 1, as the
     old engine reports it;
   - an exhausted budget: `halted: N work units exceeded; --budget N
     raises it`, exit 1;
   - completion precedence as in the old file driver: a settled return
     or an unrelated runtime failure keeps its result;
   - SIGINT during a pending host wait: `interrupted`, exit 130, the
     lifecycle closed, ended at the host wait's cancellation cadence,
     as the old engine does;
   - SIGINT while Rune code runs: the same result at the next slice
     boundary. This is a deliberate improvement: the old driver cannot
     preempt a running poll and may finish or exhaust its budget first.
     No wall-clock bound is promised inside a running synchronous host
     handler;
   - wrong arity: the diagnostic above, exit 1, unchanged on both
     engines.
4. No runtime retry. A refusal is decided at compile or prepare time,
   before any effect. After preparation, errors and cancellation are
   terminal: every outcome, including an error or interruption after an
   effect, is the new engine's own and is never rerun on the old engine.
5. Census 0212 closes at the census entry under the file-entry
   admission rule. Its class becomes `both-admit` and it stays
   admission-only, a reviewed exception with a specific reason naming the
   zero-argument async entry; execution is the job of the CLI witnesses. The census tables and the dated
   inventory follow; the census reports 92 of 92.

## Constraints

- No change to the synchronous file path's behavior or measured cost,
  to session behavior, or to any old-engine path.
- The session's opaque-value admission is not widened. If the waiting
  driver can be shared with the session without carrying opaque-session
  admission, it is shared; the plan's implementer reports which parts
  were reused and which were not.
- The admission switch lives in the protected frontend and the driver
  touches the protected execution boundary (invocation, budget,
  interruption, host waits). Both get a source review before the final.
- No new crate. Tabs. No new Python file.

## Controls

- `rnx run --debug-engine` over executable `pub async fn main(args)`
  witnesses, each reporting `engine=execution`:
  - a completed wait returning a value derived from the arguments;
  - a located runtime error after a wait, with the old engine's message
    and position;
  - an exhausted budget, before and after a wait;
  - SIGINT delivered while a host wait is pending, and while a Rune loop
    runs, each readiness-driven, not timed: exit 130, `interrupted`, the
    lifecycle closed;
  - a settled return and an unrelated runtime failure keep their
    results;
  - a counting host call followed by a terminal error, and by an
    interruption: the count shows one call, so nothing was rerun.
- Zero-argument `pub async fn main()` on `rnx run`: the arity diagnostic
  above and exit 1, pinned on both engines.
- Refusals before effects: each await form outside decision 1 (an async
  script helper, an inline async block, a stored future, `select`) keeps
  its refusal, runs on the old engine, and a counting host call before the
  refused form proves no effect ran on the new engine first.
- The synchronous entry's path is unchanged: its existing controls pass
  and the gate band holds.
- Census: 0212 `both-admit` under the file-entry rule, 92 of 92, its
  admission-only reason reviewed.
- Ordinary `frontend::compile` callers and session admission are
  unchanged: their existing controls pass, and an async `main` compiled
  without the file-entry policy still refuses.
- A mutation per admission rule and per driver contract fails its
  control.
- One complete final.
