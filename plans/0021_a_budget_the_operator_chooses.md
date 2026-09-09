# rnx 0021: a budget the operator chooses

Status: proposed 2026-09-09. The twenty-first record of rnx. `rnx run` stops a
script after two million instructions, and a real script needs more. This
record lets whoever runs it say how many, keeps two million as the default,
and does not let the script decide.

## Context

Record 0017 ported the graft verifier and checked it against the Python
original on a three-commit fixture, where it matched byte for byte. Run
against a **generated lineage of the size it was written for**, it does not
run at all:

| Commits | Rune port | Python original |
| --- | --- | --- |
| 10, 25, 50, 100 | exit 0, byte-identical to Python | exit 0 |
| 110 | exit 0 | exit 0 |
| 115 | `halted: 2000000 instructions exceeded` | exit 0 |

So the port completes 110 commits and no more, on a job whose real input is
1,346. The fixture never showed it because three commits is a thousandth of
the work. Two blockers were found together and this record is the first of
them; the second, that the port makes three `git cat-file` invocations per
commit where the original holds one `git cat-file --batch` open, is a
performance gap rather than an output gap at the sizes that pass, and is the
next record's.

**The sizing hypothesis, and what it turned out to be.** At about 18,000
instructions per commit, 1,346 commits suggested roughly 24 million — twelve
times the budget — from a ceiling observed between 110 and 115 commits. Gate 7
measured it instead: the port needs **between 24.4 and 24.6 million**, or
12.2 times the default. The hypothesis was close and slightly low, and it is
recorded here as a hypothesis that was checked rather than one that was
trusted.


## Decision

### 1. The operator chooses the budget; the script never does

`rnx run --budget N file.rn` runs with a budget of `N` instructions. Without
the flag it is two million, exactly as before, so nothing that runs today
changes.

The value comes from the command line and nowhere else. There is no host
function to raise it, no directive a file can carry, and no environment
variable — a script that could lift its own ceiling would not have one. The
person who typed the command is the one who decided how long they are willing
to wait, and that is the only place the number comes from.

### 2. The limit is not removed, only chosen

There is no `--unlimited`, and no value means unlimited. A budget is what
stops a script that will not stop on its own, and `run` has nothing else:
record 0013's `host::exit` needs the script to reach it, and Ctrl-C during a
file run is not yet a reliable stop. Removing the bound is a decision that
waits until cancellation is dependable, and this record does not make it.

`N` must be a whole number **from 1 to `usize::MAX - 1`**. Zero, a negative
number, something that is not a number, a value too large for a `usize`, and a
missing value are all refused before the script is compiled, with the same
wording rnx uses for any other bad argument, and the message states the range
so a reader learns it from the refusal.

**`usize::MAX` is refused, and that is the point of the range.** It is Rune's
sentinel for having no budget at all: `BudgetGuard::take` returns true without
decrementing when the value equals it
(`rune-0.14.1/src/runtime/budget.rs:110`). Accepting it would remove the bound
this decision keeps, quietly, through a number that looks like an ordinary
large one — the first draft of this cut did accept it, and a review caught it.
The boundary is derived from the platform's `usize` rather than written as a
64-bit literal, because the sentinel is whatever `usize::MAX` is on the
machine doing the running.


### 3. Exhaustion still ends the run, and says what to do

A script that spends its budget still stops with `halted:` on standard error
and a nonzero status. The message names the budget it exhausted and the flag
that raises it, because the reader of that line is the person who can act on
it. What the message must not become is advice to remove a limit that cannot
be removed.

### 4. The flag is rnx's, and everything after the path is the script's

`--budget` is read only before the script path, as `--debug-source` is.
Everything after the path belongs to the script verbatim, so a script that
takes its own `--budget` argument still receives it, and rnx does not look at
it. This is the rule record 0009 set for `--debug-source` and it is not
loosened.

### 5. What this record does not decide

Whether the port should make fewer invocations. Whether the session's own
budget or the prompt's should be settable. Whether a file run should be
interruptible — named in decision 2 as the thing that gates removing the limit
rather than raising it.

## Acceptance gates

1. **The default is unchanged.** With no flag, a script gets two million
   instructions, and a script that exhausted them before still exhausts them
   with the same message and status.
2. **A chosen budget is honoured.** A script that halts at the default
   completes with a larger budget, and a script that completes at the default
   halts with a smaller one. Both asserted, so the flag is shown to move the
   ceiling in both directions rather than merely being accepted.
3. **The budget is written in one place in the source.** A test shows the
   value handed to the runner is written twice — the default and the parsed
   flag — and that neither the host module nor the environment appears among
   them. This is a **source-layout check and not access enforcement**: Rune's
   `budget::with` is reachable from anywhere in the crate, and proving no host
   function could call it would mean enumerating the host surface, which this
   does not do. What keeps a script from raising its own ceiling today is that
   nothing exposes the budget to one; the gate would notice only a call
   written in the file it reads.

4. **Bad values are refused before anything runs.** `--budget` with zero, a
   negative number, a word, a decimal, or nothing after it fails with a
   nonzero status, a message naming the argument and the accepted range, and
   no sign of the script having started.
5. **The sentinel is refused and the largest real budget is not.**
   `usize::MAX` is refused, `usize::MAX - 1` is accepted and runs the script,
   and a value one past the sentinel — too large for a `usize` — is refused
   too. All three are written from `usize::MAX` rather than from a literal, so
   the case survives a platform where `usize` is not 64 bits. Verified to fail
   against the first draft, which accepted every positive `usize`.

6. **The script's arguments are untouched.** A script invoked as
   `rnx run f.rn --budget 5` receives `["--budget", "5"]` and runs with the
   default budget, because rnx stopped reading flags at the path.
7. **The full workload completes, and the hypothesis is replaced by a
   number.** Measured, both verifiers against the same generated 1,346-commit
   lineage, results before runtime because a faster wrong answer is worth
   nothing:

   - **Standard output identical**, byte for byte, and **exit status 0 from
     both**: 1,346 commits verified, the same tips, the same one invalidated
     signature, the same four conclusions.
   - **Budget needed: between 24.4 and 24.6 million.** 24.4 halts, 24.6
     completes. That is 12.2 times the default, and the first time this port
     has run the job it was written for.
   - **Runtime, mean of three: python 106 ms, the port 4,949 ms.** Forty-seven
     times, which is the three `git cat-file` invocations per commit — about
     4,038 of them at roughly 1.2 ms each accounts for the whole of it.

   The runtime gap is the next record's, and it is now a measured target
   rather than a projection: 4.9 s to something near python's 106 ms.

8. **The generator is checked in.** The lineage generator lives with the graft
   fixtures, beside the `build.sh` that builds the three-commit one, and is
   documented as the sizing harness rather than a replacement: the recorded
   diagnostics of record 0017's gates name commits from `build.sh`, whose
   construction this must not disturb.
9. **Nothing regresses.** The gates of records 0002 through 0020 pass, and all
   four ports still match their originals.

## Guardrails and stop conditions

1. One source for the number. If a second appears — an environment variable, a
   file directive, a host function — that is the defect this record exists to
   prevent.
2. The bound is not removed. If a workload wants more than an operator is
   willing to type, that is a conversation about the workload, not a switch.
3. The three-commit fixture is not touched. Its commit hashes appear in
   recorded diagnostics, so the sizing generator is a new file.

## Risks

- **A large budget makes a runaway script run longer.** That is the operator's
  choice now, made explicitly per invocation, and it is bounded: they typed a
  finite number. What it is not is a script's choice.
- **The real workload may need more than the hypothesis.** Gate 7 measures it
  rather than assuming it; if it needs far more, that is a fact about the port
  and an argument for the next record rather than for a bigger default.

## Forward

The persistent child. The original's `git cat-file --batch` is a concrete
request-and-reply protocol — write object ids to a child's standard input,
read a header and a payload of a stated length for each — and the interface
should be the smallest thing that reproduces it: partial reads, backpressure
when the child's pipe is full, cancellation while a request is outstanding,
and cleanup that cannot leave the child behind. It is a performance gap and not a fidelity gap — the
full workload now produces identical output and status from both, which gate 7
established before any timing was taken — so the interface should be specified
against that protocol rather than against the 47x.
