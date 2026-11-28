# Implementation plan 0354: Engine checks do less work

Status: Proposed

Paired ADR: ../adr/0354_engine_checks_do_less_work.md.

1. Boundary. Report, before any change: how the rune-suite
   stage's time divides into building, each check and the
   contracts, and within the contract runs, the share spent in
   dependency code against Rune code. Also report which of the
   stage's tests, if any, depend on the optimization level.
2. The profile of Decision 1, chosen by timing the whole stage
   command at the base and at each candidate level, as Decision
   1 sets out, then a fresh matched pair of base and chosen
   level. Controls 1 and 4. If the pair shows less than 60
   seconds saved, stop and send the numbers; steps 3 and 4 go
   on.
3. The entry script of Decision 2 with Control 2.
4. The ledger control of Decision 3. Send any record the reader
   refuses, with the proposed fix, before fixing it.
5. fmt, Clippy, preflight and the stages of Control 5 by their
   stage commands, with the revalidation of Control 3. Commit
   unsigned. Source review, one complete final, Results.

## Results

The dependency profile was not changed. Timed from empty routed targets,
with incremental builds off and the production budget of four tokens,
the whole rune-suite stage took:

| Dependency level | Stage | Checks 1 to 11 build | Contracts |
|---|---:|---:|---:|
| 0 (base) | 1,089.6 s | 196.3 s | 403.1 s |
| 1 | 1,102.6 s | 258.4 s | 380.4 s |
| 2 | 1,129.2 s | 288.4 s | 380.6 s |

Both levels were slower than the base, so no matched pair was run. In
two sampled contract runs, third-party dependency code took 3.7 and 5.4
percent of CPU time, against 52 to 60 percent in Rune crates. No
contract case runs Cranelift. The same checks and contract counts passed
at all three tested dependency levels.

The clean-run entry script now resolves `PATH` entries through
`filter_path` before it replaces `PATH`. It keeps their order, drops
entries that resolve to `*/.cargo/bin`, and stops when `realpath` fails.
Its control fails with the old in-loop assignment restored. The filter
returned an error when `realpath` exited nonzero.

The ledger control drives the real controller through the rune-suite
debug-lane release, checks that the release is recorded, and validates
the written ledger with the reader. It also passes every wait gate,
terminal state and controller wakeup source through `Recorder` and the
reader, and the reader rejects a release on another stage or outside the
stage's lifetime. The reader accepted every record the writer produced.
Changing only the writer turned the control red: omitting the release
time failed the recorded-release check, and recording it a million
seconds late failed validation. The control covers today's rune-suite
release, not stages that may release the debug lane in future. Its
wakeup check does not show that the release woke the controller; the
recorded release and the validation are the evidence.

Before signing, fmt, Clippy, final-controls, root-tests, cross,
rune-suite and preflight passed, and `final.sh --revalidate` passed on
the 0353 final's ledger. One complete final of signed `02abfe06` passed
all 107 stages in 3,746.2 s, against 3,942.8 s for 0353. Its rune-suite
stage took 2,024.3 s, with contracts at 902.2 s, against 2,221.4 s for
0353. The stage's build profile was unchanged; these two finals do not
isolate the cause of the timing difference. `final.sh --revalidate`
passed on this final's ledger, which the candidate's controller wrote.
`RESULT.json` sha256:

572ee59f68fb7a97217e32faf9f3af5342774da7d1b82aa657eeaea4572979e4
