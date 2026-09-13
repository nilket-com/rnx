# Upstream enhancement draft: no way to stop a running script but the budget

Ready to file against `rune-rs/rune`; the report is everything below the rule.
This is an **enhancement request**, not a defect: what the budget does, it does
as documented. The gap is that a host embedding Rune has nothing else, and the
budget cannot be used for this without discarding a nested computation — which
is the companion draft, `0032_upstream_defect_nested_budget_halt.md`, and the
reason this one exists.

Not filed. Filing it is the operator's call. Measured on Linux on 2026-09-13
against 0.14.2, on one machine (Intel i7-14700, Rust 1.98.1). `main` was not
measured for this; the run loop was read, not run.

---

**Title:** A host has no cooperative interrupt: the instruction budget is the
only thing that can stop a running script

**Version:** 0.14.2. The mechanism is unchanged on `main` (0.15.0 at
`bb8e6937`) by reading; not measured there.

### What a host wants

An interactive host — a REPL, a script runner with Ctrl-C — needs to stop a
script that is running Rune code, promptly, and then either resume it or throw
it away. Two mechanisms exist and neither does it:

- **`budget`** bounds instructions, which is the right granularity, but a halt
  for want of budget cannot be used to slice an execution that may contain an
  `async fn` called with `.await`: the halt lands inside the nested execution's
  future, the future settles as an error, and `Future::poll` drops a settled
  future, so resuming the outer execution completes with `()` where the nested
  value should have been. Details and a reproducer are in the companion draft.
- **`VmDiagnostics`** has one callback, `function_used`, which fires on a
  function call. A loop that calls nothing never reaches it, so it cannot bound
  a running script, and it is a notification rather than a halt.

So a host that supports `.await` can interrupt a script that is *pending* on a
host future — by racing the execution against something of its own — and cannot
interrupt one that is *running*. Its only bound is the total budget, which for
an interactive session is deliberately large.

### What would close it

Any of these would do; they are listed in the order we would find them useful,
not as a preferred design:

1. A flag the host can set that the run loop reads where it already reads the
   budget, producing a distinct halt that behaves like a budget halt for a
   synchronous execution — resumable — and is safe to discard for any other.
2. The same, but always terminal: a host that means to stop a script usually
   means to discard it, and a halt it can recognise (see the companion
   enhancement on halt visibility) would be enough.
3. A documented statement that per-instruction interruption is out of scope,
   which is also an answer, and would tell hosts to bound scripts by budget
   alone and say so to their users.

### What it is worth

rnx is a scripting environment on released Rune. It interrupts a script that
waits and cannot interrupt one that computes. That is a promise it has to make
carefully to its users, and this is the only thing standing between the two
halves of it.
