# Implementation plan 0288: Admission waits are recorded

Status: Implemented

Paired ADR: ../adr/0288_admission_waits_are_recorded.md.

1. Pin 8092c4b5. Split `scheduler::ready` into the gate it fails
   (producers, lanes with the stage, alone), without changing its
   verdict, and name the gate each admission branch returns on.
2. A schedule recorder in the controller: per stage, a reason written
   only on change, its start and end; `SCHEDULE.json` written with the
   other terminal records and bound into the verdict.
3. Revalidation and comparison: the binding, the stage set and the
   order checks; times run-specific in the comparison.
4. The planted-schedule controls through the controller's existing test
   seams, and the tamper controls.
5. Run the affected checks in a final-like environment, preflight and
   the commit checker; sign; source review; one complete Rust final.
   Results state what the code does, the final's duration, and what its
   schedule record shows of its critical path.

## Results

`scheduler::gate` names the first readiness check a stage fails, in
`ready`'s order, and `ready` is defined through it; a control holds it
equal to the earlier rule over every producer, lane and alone
combination of four small plans. `schedule::Recorder` is fed from the
existing admission branches only: the gate refused (with the unpassed
producers, the earlier stage sharing a lane, the running stages for
`alone`, free and needed bytes for `disk`, the weight for `tokens`),
`queued` for each stage a refusal or an alone start left unevaluated in
that pass, readiness when first established, start, end, and the
controller's block and what woke it. A refusal is written only when its
gate or the stages it waited on change. Admission order, verdicts, loop
breaks and wakeups are unchanged; nothing is re-read for the record.

`SCHEDULE.json` is written with `ENVIRONMENTS.json`, on failure paths as
well, and bound into `RESULT.records`; `ACCEPTANCE.schedule_sha256`
declares it. Revalidation and comparison require a declared schedule to
be the bound one and validate it against `SELECTION.json`: the exact
stage set, known gates and states, finite nonnegative ordered times, and
a state agreeing with its start and end. Finals from before this record
declare and bind none and still revalidate; the published 0287 receipt
revalidates PASS. The check verifies the bound digest against the
record's bytes, and a declaration without a binding, or a binding
without a declaration, is refused; comparison runs it for failed finals,
which it does not revalidate. A final that reached its stages owes the
record: a missing or unreadable one fails it, keeping an earlier cause.
A started stage must have been ready first, and no wait follows its
start; a wakeup names a selected stage or a signal. A disk refusal is
recorded before admission chooses between waiting and failing, so a
refusal before the first stage keeps its numbers. Each of these rules
has a control that fails when the rule is removed.

The complete Rust final passed in 48.2 minutes, with 1834 memory.high
events and no memory.max or OOM events. Its schedule record shows
rune-suite ready at 6.4 minutes and refused tokens at 6.4 and 10.8,
starting at 17.4 and ending at 36.6. From 26.5 to 36.6 minutes the
controller was blocked with only rune-suite running, while package-hegel
and final-controls, last refused tokens at 24.4 and 24.5, and the stages
queued behind them waited. Sixteen stages started within a minute of
rune-suite's end, including fixed-route, which ran alone from 37.6 to
47.2. The record shows refusals at those decision times, not whether
tokens stayed unavailable between them.
