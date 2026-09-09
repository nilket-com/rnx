# rnx 0020: waiting for a child without sleeping through it

Status: proposed 2026-09-09. The twentieth record of rnx. `host::process`
waits for a child by polling every 5 ms, so a child that finishes in a
microsecond still costs 5 ms. This record shortens the first wait and grows it
back, and keeps every guarantee the loop already carries.

## Context

Record 0017's port of the graft verifier makes three `git cat-file`
invocations per mapped commit. Asking why it is slow produced this, measured
on 1,346 calls — the size of the real commit map — on rustc 1.95.0,
`x86_64-unknown-linux-gnu`, release build:

| What | total | per call |
| --- | --- | --- |
| git's real work, one `cat-file --batch` process | 0.01 s | 7 µs |
| OS spawn alone, bash looping `/bin/true` | 0.71 s | 0.53 ms |
| git as separate processes, bash loop | 1.96 s | 1.46 ms |
| bare `true` through `host::process` | 7.17 s | 5.33 ms |
| git through `host::process` | 7.20 s | 5.35 ms |

The last two lines are the finding: **the child's identity does not matter**,
because both finish inside the first sleep. Patching the interval to 100 µs
and re-running the same two probes gave 0.63 s and 1.87 s — and 1.87 s is
bash's own 1.96 s, so with the sleep out of the way rnx costs nothing over a
shell. Total CPU went **down**, 0.70 s against 0.97 s, so the sleeping was
not saving work; it was only adding latency.

**What this does not establish.** These are short children. Nothing here
measures what polling costs while a long-lived child runs, and nothing here
shows that sleeping buys nothing in that case — it plainly buys something,
which is why the interval exists. A flat 100 µs poll would wake ten thousand
times a second for as long as a child lives. Decision 1 is shaped by that,
and gate 2 is what settles it with numbers rather than with this record's
opinion.

**What is a projection, not a measurement.** The verifier's cost at ~4,038
invocations — about 21.6 s now, about 5.6 s with this cut, about 0.03 s of git
work if a persistent child ever replaced the per-commit calls — is arithmetic
on the per-call numbers above, because the real inputs to that migration were
not kept. It is written as a projection wherever it appears and is not an
acceptance gate. Measuring it needs a lineage of that size built for the
purpose, which is the Forward's job and not this cut's.

## Decision

### 1. The wait is a fraction of how long the child has already run

Each pause **requested** is an eighth of the child's life so far, floored at
100 µs, capped at 5 ms, and never longer than the time left before the
deadline. The floor keeps a child that exits at once from waiting on a fixed
sleep; the cap **is** the interval this loop used for everything before, so a
child that runs for a minute is polled no harder than it ever was, and it is
reached once the child has run 40 ms.

That describes what is asked for, and nothing more. It is **not** a bound on
how quickly a child is noticed: the floor makes the pause longer than an
eighth for any child younger than 800 µs, and a sleep is a request a scheduler
may overrun by however long it likes. What the shape actually buys is gate 1
and gate 3, measured, and the rest of this record does not promise past them.


**Doubling from a short start was tried first and measured worse.** It reaches
a millisecond just as a `git cat-file` finishes, so the last sleep overshoots
by most of one: 1,346 git invocations came to 2.61 s against bash's 1.91 s,
where the proportional wait gives 1.89 s. A rule tied to the child's own
lifetime has no such interval to land badly against.

### 2. No sleep outlives the deadline

Each sleep is the smaller of the current interval and the time left before the
deadline. A deadline is therefore overshot by at most the time the loop needs
to notice it, rather than by up to a whole interval, which is an improvement
this cut gets for free and a gate asserts.

### 3. Everything the loop already guarantees stays

Named here so that a faster loop cannot quietly drop one of them:

- The deadline still ends the wait, still reports `timed_out`, and still kills
  the child's **process group**, so a descendant that outlived its parent
  within that group does not keep rnx waiting. One that left the group does —
  see the limitation below, which this cut inherits rather than introduces.
- The interrupt flag is still read on every iteration, so Ctrl-C still
  cancels, still reports `cancelled`, and now does so sooner.
- The process group is still killed after the wait, whatever ended it.
- Both capture threads are still joined — unconditionally, which is what the
  limitation below is about — and each stream still carries its own truncation
  flag, as record 0016 decided.

### 4. What this record does not decide

The persistent child. This cut changes the cost of one invocation; whether the
verifier should make four thousand of them is a different question, and it is
deliberately left until the baseline is corrected, because the number worth
solving is the one that remains after this. Record 0017's four thousand
invocations remain a shape rather than a requirement.

## Acceptance gates

1. **Short children cost what the shell costs.** Measured on the built
   binary, 1,346 calls each, same host and build as the Context:

   | | before | after |
   | --- | --- | --- |
   | bare spawns through `host::process` | 7.17 s | **0.64 s** |
   | `git cat-file` through `host::process` | 7.20 s | **1.89 s** |
   | the same git loop in bash | 1.91 s | 1.91 s |

   Eleven times faster for a bare spawn, and git through rnx is now within
   noise of git through a shell, which is the ceiling this cut was aiming at.

2. **A long-lived child costs a fixed handful of extra wakeups, and no
   measurable CPU.** Measured with `strace -c` on a child that sleeps two
   seconds: **414 sleep syscalls against 389** with the flat interval, and
   0.00 s user / 0.01 s sys either way. So sleeping does buy something for a
   long-lived child — 25 wakeups' worth, the ramp to the cap, once per call —
   and it is not free, only unmeasurable in CPU at this scale. That is the
   distinction the Context refuses to blur, and this is the number that
   settles it.

3. **Interruption is at least as prompt, and sooner early on.** Latency from
   a SIGINT to the process returning with `cancelled`, mean of five runs on a
   child that sleeps five seconds:

   | SIGINT sent | before | after |
   | --- | --- | --- |
   | 300 ms into the child | 7.40 ms | 7.21 ms |
   | 2 ms into the child | 7.46 ms | **4.54 ms** |

   Unchanged once the wait has reached its cap, and about three milliseconds
   sooner while it is still small. The floor of roughly 4.5 ms is rnx's own
   exit — killing the group, joining both capture threads, leaving the
   process — not the wait, which is why the early case does not go lower.

4. **A deadline still ends the wait, and no sleep outlives it.** A child that
   outlives its deadline returns `timed_out` with the child killed, measured
   unchanged: a 40 ms deadline returns in 52 ms and a 1 ms deadline in about
   14 ms, before and after alike. At **those two deadlines** rnx's own startup,
   about 12 ms, is the larger term — which says nothing about a deadline of
   seconds, where it plainly is not, and this record does not claim otherwise.
   **The clamp earns no wall-clock improvement at the deadlines measured, and
   is not claimed to.** It is a property of the loop, and it is asserted
   arithmetically instead: `next_wait` is a function, and a unit test pins its
   floor, its growth, its cap, and that the deadline wins whenever it is
   nearer than the wait would be.


5. **An ordinary descendant holding a pipe still does not hang the call.** The
   `sleep 10 & wait` case returns at its deadline — measured 51 ms against 50
   ms before, for a 40 ms deadline — with `timed_out` set, the process group
   killed, and no orphan `sleep` left running afterwards. That descendant
   stays in the child's process group, which is why killing the group ends it;
   the limitation below is the case it does **not** cover.

6. **Capture is still complete and still per-stream.** The gates of records
   0013 and 0016 pass unchanged: both streams captured, each with its own
   truncation flag, and a refused non-UTF-8 stream named.
7. **Nothing regresses.** The gates of records 0002 through 0019 pass, and all
   four ports still match their originals.

## A limitation this cut does not fix

**A descendant that leaves the process group and holds a captured pipe open
extends the call for as long as it lives.** The deadline bounds the wait for
the child; it does not bound the two capture joins that follow, and the group
kill cannot reach a process that left the group. Measured, all with a 40 ms
deadline:

| Child | Returns after |
| --- | --- |
| `sh -c 'sleep 1 & wait'` — descendant in the group | 0.05 s |
| `sh -c 'setsid sleep 1 & wait'` — escaped, holds the pipe | 1.01 s |
| `sh -c 'setsid sleep 3 & wait'` — escaped, holds the pipe | 3.01 s |
| `sh -c 'setsid sleep 3 >/dev/null 2>&1 & wait'` — escaped, pipe closed | 0.05 s |

The overrun is exactly the escaped descendant's lifetime, so it is not bounded
by anything rnx controls. `timed_out` is still reported correctly; what is
wrong is when the call returns.

Reproduce any row with:

```
rnx eval 'host::process("sh", ["-c", "setsid sleep 1 & wait"], 40)'
```

**There is deliberately no test for this.** A timing assertion cannot detect
the repair: a threshold low enough to catch a hanging call is sensitive to
machine load, a threshold high enough to be stable can be exceeded by
scheduling delay after the joins are bounded, and the harness that would run
it takes an unbounded `Command::output()`, so a hanging regression would hang
the suite instead of failing it. The reproducer and the measurements above are
what carry this limitation until it is fixed, and the gate for it — bounded
and synchronised rather than timed — belongs with the cut that bounds the
joins.


This predates this record — the interval had nothing to do with it — and
fixing it means bounding the joins rather than joining unconditionally, which
changes what a truncated capture means and so wants its own record and its own
evidence. It is named here so that gate 5 cannot be read as covering it.

Gate 1's and gate 3's numbers are measurements reported here, not assertions

in the suite: a timing test would fail on a loaded machine and would then be
teaching the reader to ignore it. What guards the behaviour instead is the
unit test on `next_wait`, which fails if the wait ever goes back to a flat
interval or stops respecting the deadline — both checked by reverting each and
watching it fail.

## Guardrails and stop conditions


1. The cap is not raised above today's interval. This cut is allowed to make a
   short wait cheaper; it is not allowed to make a long wait more expensive,
   and gate 2 is what proves which it did.
2. No guarantee in decision 3 is traded for latency. If one of them cannot be
   kept with a shorter first wait, the record says so rather than dropping it.
3. The verifier's projected totals stay labelled as projections until someone
   runs the verifier on a lineage of that size.

## Risks

- **A short wait means more wakeups on a child that is almost but not quite
  instant.** A child taking 300 µs is polled three times instead of once, and
  a two-second child costs 25 extra wakeups. Measured at no CPU difference,
  which is gate 2, but it is a cost and not nothing.
- **The rule reads the clock every iteration.** `Instant::now` was already
  called each pass for the deadline, so this adds no syscall; the wait is
  computed from a value the loop had already taken.
- **Nothing carries between calls.** Each invocation times its own child from
  its own start, so there is no state to get wrong and a slow child cannot
  make the next call sluggish.

## Forward

Bound the capture joins, so a descendant that left the process group cannot
hold a call open past its deadline. That is the limitation above, it predates
this cut, and it needs a decision about what a capture cut short by a deadline
reports — which is why it is not folded in here. It also needs a gate that is
bounded and synchronised instead of timed, and a harness that cannot hang:
the reason this record ships a reproducer rather than a test.

Then re-measure the persistent-child case against the corrected baseline, and only
then decide whether the interface is worth its requirements — standard input
delivery, backpressure, cancellation, and cleanup. A second script that reads
bytes stays unclaimed: the repository has no such need today, which was
checked rather than assumed.
