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

   | input | wrapper |
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
compared byte for byte between the two binaries for: `eval 7.3*8.75`; an
eval with an iterator chain; `run` of the committed JSON workload and of the
bare file; `run --budget 1000` of the JSON workload (a budget halt); `eval`
of an index error; `eval loop {}` (the default budget's halt); `eval` of a
parse error and of a missing item; a declaration and a call; and a session
of eight inputs — a binding, a declaration, `:vars`, `:debug`, a broken
input and `:debug` again. All identical, the generated wrapper included.

That last pair is what the second review's fix had to preserve: an input
that fails to compile is now compiled twice, once per wrapper, and when both
wrappers are refused in the same words the synchronous attempt is the one
reported and the one `:debug` shows. An input that awaits **and** is wrong
is the only input whose report changes, and it changes from the wrapper's
complaint about awaiting to the input's own error.

## Gates 2 to 7: `tests/async_execution.rs`

Twenty-one gates, Unix and `test-support` only, all passing:

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
| `version before` | 515.6 ± 23.8 | 479.7 | 610.9 | 1.04 ± 0.06 |
| `version after` | 494.1 ± 16.0 | 475.5 | 552.8 | 1.00 |
| `eval 42 before` | 3967.6 ± 37.3 | 3918.8 | 4122.2 | 8.03 ± 0.27 |
| `eval 42 after` | 3891.1 ± 54.3 | 3842.6 | 4307.3 | 7.87 ± 0.28 |
| `run bare before` | 3619.4 ± 21.3 | 3582.1 | 3754.3 | 7.32 ± 0.24 |
| `run bare after` | 3535.8 ± 13.9 | 3512.5 | 3586.7 | 7.16 ± 0.23 |
| `json loop before` | 11502.8 ± 206.1 | 11325.2 | 12920.3 | 23.28 ± 0.86 |
| `json loop after` | 11553.6 ± 108.5 | 11396.5 | 12220.2 | 23.38 ± 0.79 |

`eval`, `run`, and the JSON workload are unchanged within 0.1 ms; those are
synchronous and take the paths they always took. The binary grows from
9.25 to 9.42 MiB. The session's startup reference point from `:memory`:
1,789,815 bytes before, 1,797,948 after — 8,133 bytes for the runtime the
session builds at start, counted against the ceiling as record 0031 says.

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
