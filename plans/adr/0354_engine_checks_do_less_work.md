# ADR 0354: Engine checks do less work

Status: Proposed

Base: published 13c39806 (ADR 0353).

## Problem

1. The rune-suite stage is the longest stage of the complete
   final. In the final of ADR 0352 it ran 2,058.7 of 3,731.6
   seconds, with 921.2 seconds in the 50 contracts. In the final
   of ADR 0353 it ran 2,221.4 of 3,942.8 seconds, with 1,012.3
   in the contracts. Eleven of its twelve checks, and the
   contracts, build Rune with the engine-dev profile. That
   profile compiles the Rune crates and every dependency,
   Cranelift among them, at optimization level 0.
2. More parallelism doesn't help. A withdrawn record of
   2026-11-27 freed a build lane in the tail of the final. Two
   stages started earlier, both ran longer, and the final took
   68.5 seconds more. The tail is bound by processors and job
   tokens. Time comes back only from less work.
3. The level of the Rune crates is part of what is tested. Since
   3e4844ea (2026-11-22), check 0 runs the Rune library tests at
   level 2, because a NaN result differed only in optimized
   code. Checks 1 to 11 and the contracts are the only runs of
   the Rune library at level 0. Raising Rune's level in
   engine-dev would change what they test. The dependencies are
   not what these checks test, so their level changes only the
   time. A search of the 50 contract cases and the Rune test
   files found no assertion on instruction counts, timings or
   generated code. Some tests set wall-clock ceilings, which a
   faster build only makes easier to meet.
4. The final's entry script removes `~/.cargo/bin` from PATH,
   but the removal hasn't worked since ADR 0338.
   tools/final/clean/entry.sh sets `PATH="$run/tools"` before
   the loop that runs `realpath` on each directory of the
   caller's PATH. `realpath` is not found until `/usr/bin` is
   appended. Each directory before it compares as an empty
   string and is kept. On this host `~/.cargo/bin` is entry 10
   and `/usr/bin` entry 15, so every final since ADR 0338 has
   had `~/.cargo/bin` on its PATH.
5. The final's ledger writer and reader are not checked against
   each other. In the withdrawn record, the controller wrote a
   debug lane release for generated-projects that the reader
   (tools/final/src/schedule.rs) refuses. Only the docs
   successor found it, when it revalidated the final's ledger.
   No control passes what the writer can write through the
   reader.

## Decision

1. In engine-dev, the dependencies are built optimized. The Rune
   crates stay at level 0 with debug = 2, debug assertions and
   overflow checks. The stage keeps its build routing:
   incremental compilation off, and each check's target reused
   by the next check and the contracts. The implementation
   chooses the dependency level by timing the whole rune-suite
   stage command, compilation included, with every routed target
   empty and the production job budget. If the best level saves
   less than 60 seconds, confirmed by a fresh matched pair of
   base and chosen level, the profile stays as it is and the
   record carries only Decisions 2 and 3. Changing the Rune
   crates' level would need its own record, because it changes
   what the checks test.
2. The entry script resolves each directory with the caller's
   PATH and only then replaces PATH. The result is `$run/tools`,
   then the caller's directories in their order, without any
   directory that resolves to `*/.cargo/bin`. If a directory
   cannot be resolved, the script stops with an error instead of
   keeping it.
3. The ledger reader accepts every record the writer can
   produce, and a control in the code proves it. A record the
   writer can produce and the reader refuses is fixed in the
   reader or the writer, whichever is wrong. Which stages may
   release the debug lane does not change: the present writer
   and reader agree on rune-suite.
4. Scope: the tools, the engine-dev profile and their controls.
   No engine source changes. The dev, release, engine-release
   and engine-bench profiles stay as they are. The source
   guard's 40 items stay unchanged.

## Controls

1. Profile. Cargo's unit graph for engine-dev shows the Rune
   crates at level 0 with debug assertions, overflow checks and
   full debug information, and the dependencies at the chosen
   level. The four other profiles are unchanged. The control
   runs in final-controls.
2. PATH. The entry script's filter runs on a PATH with a
   `.cargo/bin` directory, and a link to it, placed before
   `/usr/bin`. The result begins with `$run/tools`, keeps the
   other directories in their order and contains neither the
   directory nor the link. With the old order of the assignment
   restored, or with a failing `realpath`, the control fails.
3. Ledger. A final-controls test drives the controller's writer
   through every kind of record it can write, through the real
   release path, checks that a debug lane release is present,
   and the reader accepts the ledger. A writer-only change,
   planted, makes the test fail, and so does a release outside
   the stage's lifetime. Before signing, `final.sh --revalidate`
   passes on a ledger that the candidate's own controller wrote.
4. Counts. The implementation verifies that every rune-suite
   check and contract passes with the base's test and row
   counts, and that the Miri stages pass. Optimized dependencies
   can change behavior at run time, so this is checked, not
   assumed.
5. Stages. The whole rune-suite, root-tests, cross and
   final-controls stages, and every catalogue stage whose
   command reads a changed file, pass by their stage commands.
   Then one complete final. Its Results give its wall time and
   rune-suite's, against the 3,942.8 and 2,221.4 seconds of ADR
   0353's final.
