# rnx 0030 evidence: answering before building the context

Measured on Linux on 2026-09-13, on `nano` (Intel i7-14700, kernel
7.0.0-31), Rust 1.95, Rune 0.14.2. `hyperfine 1.20.0` with `-N` (no shell),
pinned to one core with `taskset -c 4`, 10 warmups, 100 runs. "Before" is
the tree at the plan commit, built with `cargo build --release --locked`
into a separate target directory; "after" is the implementation commit.

## Gates 1 and 2: the outputs are unchanged

Each command was run under both binaries with standard output captured and
compared with `cmp`:

| command | exit before | exit after | stdout |
| --- | --- | --- | --- |
| `rnx version` | 0 | 0 | identical, 2 lines |
| `rnx help` | 0 | 0 | identical, 11 lines |

The gates in `tests/commands.rs` and `tests/release_metadata.rs` pass
unchanged; `cargo test --locked` is green across every suite.

## Gate 3: measured

| Command | Mean [µs] | Min [µs] | Max [µs] | Relative |
|:---|---:|---:|---:|---:|
| `true` | 299.1 ± 128.4 | 228.9 | 875.7 | 1.00 |
| `version before` | 3567.7 ± 20.0 | 3533.9 | 3615.7 | 11.93 ± 5.12 |
| `version after` | 503.2 ± 21.7 | 477.4 | 609.3 | 1.68 ± 0.73 |
| `help before` | 3570.1 ± 24.9 | 3533.7 | 3716.1 | 11.94 ± 5.13 |
| `help after` | 497.3 ± 18.7 | 472.5 | 584.7 | 1.66 ± 0.72 |
| `eval 42 before` | 3905.9 ± 20.3 | 3871.8 | 3965.5 | 13.06 ± 5.61 |
| `eval 42 after` | 3882.7 ± 64.4 | 3840.6 | 4473.3 | 12.98 ± 5.58 |

`version` and `help` each fall from 3.6 ms to about 0.5 ms, which is within
0.2 ms of `/bin/true` on the same core. `eval 42` is 3.9 ms before and
after: the reorder gives it nothing, as the record said it would not.

## Where the 3.1 ms was

The phase measurement the record rests on, so it is beside the numbers it
explains. A scratch crate against Rune 0.14.2, one binary that returns after
the named phase, same hyperfine conditions:

| after | mean | delta |
| --- | --- | --- |
| exit at once | 0.56 ms | the process floor |
| `Context::with_default_modules()` | 3.7 ms | +3.1 ms |
| `context.runtime()` | 3.7 ms | ~0 |
| compiling `pub fn main() { 42 }` | 3.7 ms | ~0 |
| running it | 3.7 ms | ~0 |

The scratch crate is not in the repository; its whole source is nine lines
of phase gating around the four calls named in the table.

## The wider baseline

The same machine and conditions were used for a cross-runtime baseline kept
in its own repository, `rnx-bench`, at commit 1e4c09b. That is the commit a
future comparison should cite; the numbers above are rnx's rows from it.

## What was not measured

The cost of the context itself was not profiled: `perf` is refused on this
machine (`perf_event_paranoid=4`). An in-process `Instant` around the same
phases read 7–9 ms for the context, against the 3.1 ms above; the two were
not taken under the same conditions and the difference is unexplained.
