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
