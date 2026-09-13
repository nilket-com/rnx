# rnx 0032 evidence: one way to run a script

Measured on Linux on 2026-09-13, on `nano` (Intel i7-14700, kernel
7.0.0-31), Rust 1.98.1 (the machine's default toolchain; the manifest records
1.95 as the tested one), Rune 0.14.2, tokio 1.53.1 with `rt` and `time`.
"Before" is the tree at the plan commit (0e718ae), built into a worktree of
its own with `cargo build --release --locked`; "after" is the implementation
commit. Timings are `hyperfine 1.20.0 -N`, pinned with `taskset -c 4`.

## The probe the record rests on

A scratch crate against Rune 0.14.2 and tokio, before any rnx code changed,
drove `pub async fn main` through `Vm::execute` and `async_resume` in
slices under `budget::with`, selected against a flag-watching future on a
5 ms cadence. Its source is nine functions long and is reproduced in the
driver's own comments where it matters; the outcomes:

| case | outcome | slices | wall |
| --- | --- | --- | --- |
| A: async main, CPU loop of ~130k instructions, slice 10k | complete | 13 | 2.7 ms |
| B: the same loop with a 30 ms await in the middle | complete | 13 | 34 ms |
| B2: awaits inside a loop, slice 500 | complete | 73 | 19 ms |
| C: infinite await loop, budget 20k | budget | 20 | — |
| D: 5 s pending sleep, flag set at 50 ms | interrupted | 1 | 51.8 ms |
| E: infinite CPU loop, flag set at 50 ms | interrupted | 2191 | 50.3 ms |
| F: synchronous main through the same driver | complete | 13 | 1.1 ms |
| G: index error after an await | error, with its text | 1 | — |

A and B taking the same 13 slices is the fact the record's guardrail 2
rests on. D is the pending-interrupt latency: under 2 ms past the flag.

## Gate 1: synchronous scripts are unchanged

Every suite passes: `cargo test --locked` (default features) and
`cargo test --features test-support`. Standard output, standard error and
exit status were compared byte for byte between the two binaries for:
`eval 7.3*8.75`; an eval with an iterator chain; `run` of the committed JSON
workload and of the bare file; `run --budget 1000` of the JSON workload (a
budget halt); `eval` of an index error; and a five-input session with a
binding, a declaration, and `:vars`. All identical.

## Gates 2 to 6: `tests/async_execution.rs`

Thirteen gates, Unix and `test-support` only, all passing:

- A file whose `main` awaits the fixture prints its value; a synchronous
  file still runs; `eval` awaits; a session input awaits at the top level
  and the next input reads the binding it made. (Gate 2)
- Three scripts with identical instructions, awaiting at the start, in the
  middle, and at the end, complete under the same smallest budget (found to
  a step of 50, and above 1,000 so the scripts span slices) and all halt 50
  below it. An await in a loop under a budget of 5,000 halts with
  `halted: 5000 instructions exceeded` and nothing else. (Gate 3)
- `SIGINT` 300 ms into a run pending on a 10 s future: exit 130,
  `interrupted` on standard error, nothing on standard output, done within
  2 s. The same for a run in `loop {}` under the largest budget, and for an
  `eval` pending on the fixture. A session input pending on the fixture is
  interrupted and the session goes on to read its pipe to the end and exit
  0; with a second input queued behind the interrupted one, it runs and
  answers. (Gate 4)
- An index error on line 4, after an await on line 2, is reported at
  `line 4` with the source line under `run`; in a session it names
  `input 2`. (Gate 5)
- Nothing pending outlives an input: the interrupted session exits as soon
  as its pipe closes, which it could not if the 10 s future were still
  held; the interrupted run exits within the bound. The driver spawns no
  task, so there is nothing else to check. (Gate 6)

## Gate 7: cost

| Command | Mean [µs] | Min [µs] | Max [µs] | Relative |
|:---|---:|---:|---:|---:|
| `version before` | 527.5 ± 91.5 | 482.2 | 1086.2 | 1.07 ± 0.19 |
| `version after` | 716.2 ± 250.2 | 482.1 | 2877.4 | 1.45 ± 0.51 |
| `help before` | 501.4 ± 15.0 | 485.7 | 567.8 | 1.01 ± 0.04 |
| `help after` | 495.1 ± 15.5 | 473.9 | 567.0 | 1.00 |
| `eval 42 before` | 3950.9 ± 51.2 | 3909.9 | 4290.7 | 7.98 ± 0.27 |
| `eval 42 after` | 3875.2 ± 14.3 | 3847.8 | 3920.5 | 7.83 ± 0.25 |
| `run bare before` | 3605.6 ± 14.9 | 3581.5 | 3651.8 | 7.28 ± 0.23 |
| `run bare after` | 3539.9 ± 24.0 | 3508.7 | 3643.4 | 7.15 ± 0.23 |
| `json loop before` | 11491.5 ± 266.1 | 11282.8 | 13853.3 | 23.21 ± 0.91 |
| `json loop after` | 11540.7 ± 120.1 | 11397.0 | 12191.7 | 23.31 ± 0.77 |

Re-measured with 200 runs, `version` is 506 µs before and 497 µs after; the
100-run figure above for `version after` has a σ of 250 µs and is noise.
`eval`, `run`, and the JSON workload are unchanged within 0.1 ms. The
binary grows from 9.25 to 9.41 MiB.

The session's startup reference point from `:memory`: 1,787,061 bytes
before, 1,795,205 after — 8,144 bytes for the runtime, counted against the
ceiling as record 0031 says, and not this record's question to exclude.

## Dependencies and notices

tokio 1.53.1 joins the tree with `default-features = false` and the `rt`
and `time` features. `scripts/third-party-notices.sh` regenerated
`THIRD-PARTY-NOTICES.md` (54 packages, one new section, for tokio under its
MIT license) and its `--check` passes; the release-metadata gate that runs
it passes.

## Not measured

Windows: the driver is platform-neutral and the tests are Unix-only, so
record 0025's gates are what will say whether `SIGINT` semantics carry
over to console events. macOS: type-checks are not measurements.
