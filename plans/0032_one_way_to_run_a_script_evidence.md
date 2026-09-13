# rnx 0032 evidence: one way to run a script

Measured on Linux on 2026-09-13, on `nano` (Intel i7-14700, kernel
7.0.0-31), Rust 1.98.1 (the machine's default toolchain; the manifest records
1.95 as the tested one), Rune 0.14.2, tokio 1.53.1 with `rt` and `time`.
"Before" is the tree at the plan commit (0e718ae), built into a worktree of
its own with `cargo build --release --locked`; "after" is the implementation
as committed. Timings are `hyperfine 1.20.0 -N`, pinned with `taskset -c 4`.

## The first implementation, and what review found

The first implementation (a668f57) ran every script on one driver that
resumed the execution in slices of 10,000 instructions under the budget.
Review (Codex, `rnx-context-profile/reviews/0032.md` at 219ca5d) found that
this script, under `--budget 2000000`, printed
`Halted for unexpected reason \`limited\`` at the await and exited 1:

```
async fn work() { let n = 0; for i in 0..2000 { n += i; } n }
pub async fn main(_) { work().await }
```

Reproduced against that binary before anything changed. Then a probe, a
scratch crate against Rune 0.14.2 and tokio, measured the alternatives:

| probe | outcome |
| --- | --- |
| nested loop, sliced at 10k, resume after every budget halt | `()` after 2 slices — **wrong, silently** |
| nested loop, sliced at 10k, with an await inside the function | `()` — the same |
| nested loop, whole budget installed once | `1999000` |
| nested function that awaits a timer, whole budget once | correct |
| `loop {}` under a budget of 50k, whole budget once | budget halt, no location |
| 5 s pending sleep raced against a future that only reads the flag, flag set at 50 ms | ended at 3.0 s, by the probe's own deadline: the flag woke nothing |
| `loop {}` under 500k raced against the same future, flag already set | ran to the budget in 6 ms: a running execution is never preempted |

The mechanism, from Rune's source: `call_async_fn` wraps a nested
execution as `Future::new(async move { execution.async_complete().await })`;
a budget halt inside it is an error out of that block; `Future::poll` on a
ready result sets its inner future to `None`. Resuming the outer execution
then completes with the unit its await left behind. `VmDiagnostics` has one
callback, `function_used`; there is no per-instruction hook but the budget.

So the implementation was replaced: a script that can await runs under its
whole budget once, raced against a future that reads the flag on a 5 ms
interval (a timer wakes the task; a flag does not); a script that cannot
await runs on the unchanged synchronous path it always had.

## The third review, and what it found

1. **A file's shape could not be read after all.** A file whose entry point
   is reached through an alias —

   ```
   mod inner {
       pub async fn work(_) { host::test_pending(5).await }
   }
   pub use inner::work as main;
   ```

   — has no `async` token anywhere near `main`, so the parsed `main` item
   the previous cut looked for does not exist. It took the synchronous path
   and halted: `Halted for unexpected reason \`awaited\``. The synchronous
   alias beside it worked, which is what made it a shape problem rather than
   an alias problem. `Unit::function` and `UnitFn` are crate-private, so the
   compiled unit cannot be asked either. So no file is classified now: every
   file takes the driver that copes with either, which is measured below to
   cost nothing, and the alias runs and prints `5`.

2. **A spent slice was reported as a spent budget.** The sliced path forwards
   the guard it read, and that guard belongs to one slice of ten thousand
   instructions, not to the input's two billion. This input —

   ```
   let n=0; for i in 0..1664 { n += i; } let z=0; panic!("boom")
   ```

   — panicked as its first slice ran out and was told the budget of
   2,000,000,000 instructions had been exhausted. The sliced path now names
   the budget only when the slice that ran out was the one that would have
   spent it, and the panic above is reported alone.

## The second review, and what it found

Review found two more, both reproduced against the implementation before
anything changed:

1. **The shape test read tokens out of the text.** `let text = ".await";
   loop {}` took the async path and so lost the slices that make Ctrl-C end
   a loop, and a `select` — which awaits without writing the word — was not
   recognised at all. Rune refuses `.await` **and** `select` outside an
   async function, in the same words, so the compiler can answer the
   question: the synchronous wrapper is compiled first, and its refusal is
   the answer. A file is answered from the parsed `main` item's `async`
   keyword. Measured afterwards, by the wrapper `:debug` prints:

   | session input | wrapper |
   | --- | --- |
   | `let text = ".await"; 1` | `pub fn main` |
   | `host::test_pending(1).await` | `pub async fn main` |
   | `let a = 1; select { r = a => r }` | `pub async fn main` |
   | `(async{42}).await` | `pub async fn main` |
   | `let n = 0; for i in 0..3 { n += i; } n` | `pub fn main` |
   | `async fn f() { host::test_pending(1).await } 1` | `pub fn main` |

   The last is the one no scan of the text can get right: the declaration is
   hoisted above the wrapper, so the input that declares an awaiting
   function does not itself await.

2. **The budget report stood in front of a real failure.** `pub async fn
   main(_) { panic!("boom") }` under `--budget 4` reported only
   `halted: 4 instructions exceeded`; under 5 it reported the panic. The
   async path was classifying on the guard alone, and a failure on the last
   permitted instruction leaves the guard at zero exactly as a halt does.
   It now classifies as both synchronous paths always have, on the absence
   of a location **and** the guard, and names the budget after a located
   error rather than instead of it:

   ```
   runtime error at /tmp/boom.rn, line 1, column 24: Panicked: boom
     pub async fn main(_) { panic!("boom") }
                            ^
   halted: the budget of 4 instructions was exhausted at that point; --budget N raises it
   ```

   A budget spent inside a nested async function now reports as the halt it
   is, at that function's call site, with the same second line. Telling that
   apart from a failure would need Rune's error kind, which is crate-private.

## Gate 1: synchronous scripts are unchanged

Every suite passes: `cargo test --locked` and `cargo test --features
test-support`. Standard output, standard error and exit status were
compared byte for byte between the two binaries — including for files,
which now all take the driver — for: `eval 7.3*8.75`; an
eval with an iterator chain; `run` of the committed JSON workload and of the
bare file; `run --budget 1000` of the JSON workload (a budget halt); `eval`
of an index error; `eval loop {}` (the default budget's halt); `eval` of a
parse error and of a missing item; a declaration and a call; `run --budget 5`
of the bare file, a budget too small to start it; and a session
of eight inputs — a binding, a declaration, `:vars`, `:debug`, a broken
input and `:debug` again. All identical, the generated wrapper included.

That last pair is what the second review's fix had to preserve: an input
that fails to compile is now compiled twice, once per wrapper, and when both
wrappers are refused in the same words the synchronous attempt is the one
reported and the one `:debug` shows. An input that awaits **and** is wrong
is the only input whose report changes, and it changes from the wrapper's
complaint about awaiting to the input's own error.

## Gates 2 to 7: `tests/async_execution.rs`

Twenty-three gates, Unix and `test-support` only, all passing:

- A file whose `main` awaits the fixture prints its value; a synchronous
  file still runs; `eval` awaits; a session input awaits at the top level
  and the next input reads the binding it made. (Gate 2)
- Three scripts with identical instructions, awaiting at the start, in the
  middle, and at the end, complete under the same smallest budget, found to
  a step of 50 and asserted above 30,000 — three slices' worth — and all
  halt 50 below it. An await in a loop under a budget of 5,000 halts with
  `halted: 5000 instructions exceeded` and nothing else. The review's
  nested function returns 1,999,000 from a file under `--budget 2000000`,
  and from a session where the function was declared by an earlier input.
  (Gate 3)
- `SIGINT` 300 ms into a run pending on a 10 s future: exit 130,
  `interrupted` on standard error, nothing on standard output, done within
  2 s; the same for an `eval`. A session input pending on the fixture is
  interrupted and the session reads its pipe to the end and exits 0; with a
  second input queued behind it, that input runs and answers. A synchronous
  session input in `loop {}` is interrupted and the next input answers. A
  file in `loop {}` under `--budget 100000` halts for budget. (Gate 4)
- An entry point reached through `pub use inner::work as main` runs as what
  it is: the awaiting one prints `5`, the synchronous one beside it prints
  `7`. A failure inside one slice of a synchronous input reports its panic
  and says nothing about the input's budget.
- A string holding `.await` leaves a session input on the synchronous path,
  where Ctrl-C ends its loop and the next input answers, and leaves a file
  synchronous; `select` on a real future reaches the async path and returns
  its value; an awaited `async` block returns its value; an input that only
  declares an awaiting function keeps the synchronous path and its slices.
  (Gate 5)
- An index error on line 4, after an await on line 2, is reported at
  `line 4` with the source line under `run`; in a session it names
  `input 2`. (Gate 6)
- Nothing pending outlives an input: the interrupted session exits as soon
  as its pipe closes, which it could not if the 10 s future were still
  held; the interrupted run exits within the bound. Neither path spawns a
  task. (Gate 7)

## Gate 8: cost

| Command | Mean [µs] | Min [µs] | Max [µs] | Relative |
|:---|---:|---:|---:|---:|
| `version before` | 509.1 ± 21.5 | 481.2 | 589.1 | 1.03 ± 0.06 |
| `version after` | 493.6 ± 18.6 | 474.5 | 587.4 | 1.00 |
| `eval 42 before` | 3948.5 ± 30.8 | 3906.3 | 4143.1 | 8.00 ± 0.31 |
| `eval 42 after` | 3858.9 ± 31.0 | 3825.8 | 4058.0 | 7.82 ± 0.30 |
| `run bare before` | 3617.4 ± 30.9 | 3584.9 | 3759.5 | 7.33 ± 0.28 |
| `run bare after` | 3530.2 ± 56.5 | 3488.8 | 4010.2 | 7.15 ± 0.29 |
| `json loop before` | 11496.8 ± 267.6 | 11249.5 | 13432.7 | 23.29 ± 1.03 |
| `json loop after` | 11426.4 ± 98.7 | 11265.0 | 11778.2 | 23.15 ± 0.89 |

`eval`, `run`, and the JSON workload are unchanged within 0.1 ms. `run` now
builds a Tokio runtime for every file rather than for some, and `run bare`
is 3.6 ms before and 3.5 ms after: the runtime costs less than the run-to-run
drift of this machine. The binary grows from 9.25 to 9.42 MiB. The session's
startup reference point from `:memory`: 1,789,815 bytes before, 1,797,948
after — 8,133 bytes for the runtime the session builds at start, counted
against the ceiling as record 0031 says.

## Dependencies and notices

tokio 1.53.1 joins the tree with `default-features = false` and the `rt`
and `time` features. `scripts/third-party-notices.sh` regenerated
`THIRD-PARTY-NOTICES.md` (54 packages, one new section for tokio under its
MIT license); its `--check` passes and so does the release-metadata gate
that runs it.

## What is open, and not claimed

Record 0031's gate 2 asks for interruption during pending I/O and CPU loops.
Every entry point is interruptible while pending on a future, and every
synchronous script is interruptible in a loop. A script that can await and
then loops without awaiting is bounded by its budget and by nothing else;
that clause of gate 2 is open, and the record says why it cannot be closed
here.

## Not measured

Windows: the paths are platform-neutral and the tests are Unix-only, so
record 0025's gates are what will say whether `SIGINT` semantics carry
over to console events. macOS: type-checks are not measurements. The
async path's own startup — the runtime and the first pending poll — was
not measured apart from the whole; no shipping battery exists yet to
measure it with.
