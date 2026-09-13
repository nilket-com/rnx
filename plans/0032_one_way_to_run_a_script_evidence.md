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

## Gate 1: synchronous scripts are unchanged

Every suite passes: `cargo test --locked` and `cargo test --features
test-support`. Standard output, standard error and exit status were
compared byte for byte between the two binaries for: `eval 7.3*8.75`; an
eval with an iterator chain; `run` of the committed JSON workload and of the
bare file; `run --budget 1000` of the JSON workload (a budget halt); `eval`
of an index error; `eval loop {}` (the default budget's halt); and a session
of six inputs — a binding, a declaration, `:vars`, and `:debug`, whose
generated wrapper is the same text. All identical.

## Gates 2 to 6: `tests/async_execution.rs`

Fifteen gates, Unix and `test-support` only, all passing:

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
- An index error on line 4, after an await on line 2, is reported at
  `line 4` with the source line under `run`; in a session it names
  `input 2`. (Gate 5)
- Nothing pending outlives an input: the interrupted session exits as soon
  as its pipe closes, which it could not if the 10 s future were still
  held; the interrupted run exits within the bound. Neither path spawns a
  task. (Gate 6)

## Gate 7: cost

| Command | Mean [µs] | Min [µs] | Max [µs] | Relative |
|:---|---:|---:|---:|---:|
| `version before` | 508.1 ± 19.9 | 484.5 | 597.7 | 1.03 ± 0.05 |
| `version after` | 493.6 ± 15.4 | 472.4 | 563.8 | 1.00 |
| `eval 42 before` | 3949.4 ± 62.7 | 3907.1 | 4492.2 | 8.00 ± 0.28 |
| `eval 42 after` | 3884.5 ± 16.3 | 3853.6 | 3943.4 | 7.87 ± 0.25 |
| `run bare before` | 3607.1 ± 19.9 | 3576.6 | 3713.3 | 7.31 ± 0.23 |
| `run bare after` | 3558.4 ± 49.6 | 3512.8 | 3914.0 | 7.21 ± 0.25 |
| `json loop before` | 11503.8 ± 295.1 | 11276.3 | 14082.8 | 23.30 ± 0.94 |
| `json loop after` | 11597.6 ± 96.8 | 11469.5 | 11940.9 | 23.49 ± 0.76 |

`eval`, `run`, and the JSON workload are unchanged within 0.1 ms; those are
synchronous and take the paths they always took. The binary grows from
9.25 to 9.41 MiB. The session's startup reference point from `:memory`:
1,789,936 bytes before, 1,798,072 after — 8,136 bytes for the runtime the
session builds at start, counted against the ceiling as record 0031 says.

## Dependencies and notices

tokio 1.53.1 joins the tree with `default-features = false` and the `rt`
and `time` features. `scripts/third-party-notices.sh` regenerated
`THIRD-PARTY-NOTICES.md` (54 packages, one new section for tokio under its
MIT license); its `--check` passes and so does the release-metadata gate
that runs it.

## Not measured

Windows: the paths are platform-neutral and the tests are Unix-only, so
record 0025's gates are what will say whether `SIGINT` semantics carry
over to console events. macOS: type-checks are not measurements. The
async path's own startup — the runtime and the first pending poll — was
not measured apart from the whole; no shipping battery exists yet to
measure it with.
