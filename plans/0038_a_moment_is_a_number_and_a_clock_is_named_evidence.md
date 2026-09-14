# rnx 0038 evidence: a moment is a number, and a clock is named

Implemented and measured by Codex on nano (Linux, Intel i7-14700),
2026-09-14, Rust 1.98.1, Rune 0.14.2, Jiff 0.2.24. Plan: `0c1e4ad`.
Integration evidence is rnx-bench commit `1a36430`, `results/time_0038_*`.
The candidate-crate probes remain a separate, narrower experiment.

## Implementation

`src/time.rs` installs eight functions into the existing shared context:
now_ms, monotonic_ms, format, rfc3339, parse, parts, from_parts, sleep.
No native calendar value escapes. Calendar functions return catchable errors;
clocks return signed integers, and sleep returns an async Result<()>.

The timestamp adapter uses seconds/nanoseconds construction to retain the
last 999 milliseconds of Jiff's range, and signed subsecond floor division
when extracting milliseconds. One unit gate covers both endpoints, adjacent
values, values around zero, out-of-range counts and the one-nanosecond-before-
epoch case where Jiff's as_millisecond returns zero rather than minus one.

Formatting uses the fallible strtime API. RFC 3339 checks the year and offset
before printing three fractional digits. General formatting carries only the
pattern's precision. Fixed offsets use ±HH:MM with hours at most 23.

Parsing retains Jiff's date/calendar validation and checks the raw syntax
that Jiff otherwise discards or broadens: seconds=60, offset spelling, and
non-zone or multiple annotations. Basic and extended time forms are checked
for leap seconds. Named annotations use the Zoned parser, with its documented
Z exception. A further exact numeric-offset check rejects historical offsets
that Jiff would accept after rounding to a minute. The gate includes Paris
in 1900 with a supplied +00:09 versus the actual +00:09:21.

from_parts requires year/month/day and defaults the clock fields to zero.
Extra fields (including parts' informational weekday and zone) are ignored;
the explicit zone argument is authoritative. Gaps refuse before offset
selection. Folds require an offset, and numeric offsets must agree on ordinary
dates too. Local zones without an IANA identifier retain "local" as the
reported label, instead of becoming a misleading fixed-offset name.

Sleep creates its timer only when polled, validates 0..68719476735, and uses
the existing driver. No new runtime, cancellation mechanism or task spawning
is introduced. The process-wide monotonic origin is initialized on first use
and survives session resets.

## Fixture correction discovered during implementation

An empty TZDIR does not simulate an unavailable zone database. Jiff rejects
that directory and tries the normal system locations, so local Paris still
resolved in the first test. The revised fixture is a valid, minimal UTC-only
TZif database with TZ=Europe/Paris in a fresh process. It lacks Paris without
triggering discovery fallback. Local lookup then refuses, while UTC and fixed
offsets continue to work. The record's gate was corrected to state this.
No production override of Jiff's discovery policy was introduced.

## Acceptance

Both complete suites ran sequentially under TERM=xterm-kitty, so feature
builds could not replace the executable during another suite's tests.

| check | result |
| --- | --- |
| cargo test --locked | 287 passed; zero failures |
| cargo test --locked --features test-support | 325 passed; zero failures |
| cargo build --release --locked | passed |
| scripts/third-party-notices.sh --check | passed |
| new source/test rustfmt | passed |
| diff whitespace excluding generated notices | passed |
| isolated Windows time-module and unit-gate type check | passed |

Two unit gates and seven Linux integration gates exercise:

- Exact registration surface; epoch/negative/range-end conversions; both
  out-of-range counts refused without wrapping.
- Wall time bracketed by the test process's SystemTime readings with 100 ms
  tolerance; 10,000 nondecreasing monotonic calls and a near-zero origin.
- Round trips through parts/from_parts in UTC, fixed offsets, Paris, Chicago
  and controlled local time, including both fold occurrences and timestamps
  around the spring transition. RFC 3339 round trips only within its domain;
  its low-year and historical-second-offset refusals are separate assertions.
- January/July offsets in two named zones, ISO weekday, millisecond fields,
  required/default fields, invalid fields, gaps with either offset, folds
  without offsets and with each valid offset, and ordinary-date disagreement.
- Required offsets, exact fixed-offset spelling, negative fractional parsing,
  leap seconds including basic/lowercase/annotated forms, unknown annotations,
  conflicting numeric annotations, the valid Z annotation exception, invalid
  format directives, unavailable local zones and the controlled database.
- A pre-1970 file used directly with now_ms and parts. On Unix a file set to
  one nanosecond before the epoch is read back exactly and metadata.modified_ms
  agrees with parsing that instant as -1. Windows uses the whole-second file
  case; its timestamp precision cannot represent that nanosecond fixture.
- sleep(50) in run, eval and the session, with elapsed time at least 50 ms;
  async wrapper promotion observed through session :debug; an explicit sync
  function containing await refused; future creation for a ten-second sleep
  returning within two seconds; negative/over-limit sleeps refused catchably.
- Unix SIGINT during a ten-second sleep in all three entry points: a ready
  marker precedes the await, then the test waits 20 ms before signalling.
  Each must complete within 500 ms of the signal, far below the requested
  sleep. Run/eval exit 130; the session reports interrupted and runs queued
  input 42. This is a tolerance gate, not a claim of scheduler precision.

All five example sources and complete outputs are in
`results/time_0038_examples.json`, reproducible with
`scripts/run_time_0038_examples.py RELEASE_BINARY`. The log example produced
an RFC 3339 stamp; elapsed reported 51 ms for sleep(50); the age example
reported modified_ms=-1; the zone example produced Chicago -06/-05 and Paris
+01/+02 in January/July; the cancellation example exited 130 about 1.93 ms
after SIGINT. The integration suite, not that single sample, gates all three
entry points against the 500 ms tolerance.

## Features and portability

The saved cargo feature trees show std, tz-system and tzdb-zoneinfo on Linux,
and additionally tzdb-bundle-platform on Windows. Jiff's platform wrapper
selects jiff-tzdb 0.1.8 there. No bundled Unix database, logging, serde or
static-tz feature is enabled. Cargo.lock includes target/optional entries
such as jiff-static; that is not evidence those features are enabled.

rnx-bench's fs-portability probe includes the actual time source and its unit
gates under x86_64-pc-windows-msvc. It isolates these from the previously
recorded MSVC TLS build limitation. This is a type check, not Windows
execution or a whole-application build. Windows execution remains unverified.

Notices now list 124 packages and the same 87 distinct texts, 13 fetched
texts and one pre-existing unresolved entry. Generated notices retain their
generator's existing trailing-space convention; other changed files pass
diff whitespace checks. The generator's --check passes unchanged.

## Integration cost

Before is the default-feature release binary at the plan commit (0037
product code), built and copied before editing. Its SHA-256 matches 0037's
measured after binary. After uses cargo build --release --locked with this
implementation. Versions, hashes, byte sizes, complete output checks and
session memory output are in `results/time_0038_conditions.json`.

Hyperfine 1.20.0, -N, CPU 4, 10 warmups, 50 runs per command, controlled PATH
and TERM=dumb. Elapsed process measurements include startup and destruction.
The measurement driver checks successful exit and equivalent output on the
unchanged workloads. Raw exports are `results/time_0038_startup.json`.

| command | before mean ± standard deviation | after mean ± standard deviation |
| --- | --- | --- |
| version | 0.524 ± 0.017 ms | 0.515 ± 0.021 ms |
| help | 0.513 ± 0.017 ms | 0.511 ± 0.015 ms |
| eval 42 | 4.049 ± 0.101 ms | 3.969 ± 0.013 ms |
| run bare file | 3.699 ± 0.022 ms | 3.683 ± 0.090 ms |
| JSON workload | 11.680 ± 0.090 ms | 11.601 ± 0.375 ms |

No startup increase was detected in this run; small decreases are not
attributed to the change. These do not measure calendar throughput or the
first timezone lookup's I/O cost.

| size/accounting | before | after | change |
| --- | --- | --- | --- |
| release binary bytes | 14,349,432 | 14,865,000 | +515,568 (~0.49 MiB) |
| session startup reference bytes | 1,817,885 | 1,823,031 | +5,146 |
| live request bytes at :memory | 1,826,475 | 1,831,621 | +5,146 |

The integration delta is not the candidate probe's 0.92 MiB standalone
executable size. Memory figures count allocator requests, not resident
memory, and precede lazy timezone-cache population.

Record 0031's async CPU-loop interruption clause remains open. Pending sleep
cancellation does not close it.
