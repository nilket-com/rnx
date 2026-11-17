# Implementation plan 0290: Returned tokens wake admission

Status: Implemented

Paired ADR: ../adr/0290_returned_tokens_wake_admission.md.

1. Pin bf321072. In job-budget's `fifo`: the pool's byte count, and a
   watch (inotify `IN_MODIFY` and a stop pipe) whose waiter reports
   writes, overflows and failures; unit controls for own, raw and
   other-process writes, reads raising nothing, synthesized overflow and
   removal records, and stop.
2. In the controller: the `Tokens` event, its single-outstanding flag
   cleared before the recheck, the count check before taking, the
   waiting-on-tokens mark, the named watcher-failure event, and the
   watcher guard started before any stage and stopped and joined before
   the pool closes; the schedule's `tokens` wake.
3. The concurrency controls of the ADR, including the mutation that
   removes the wake failing the first.
4. The affected checks, preflight and the commit checker; sign; source
   review; one complete Rust final; Results from its receipts, stating
   what its schedule shows, not a promised saving.

## Results

`job_budget::fifo` gains `Pool::available`, the fifo's byte count by
`FIONREAD`, and `Watch`, an inotify `IN_MODIFY` watch with a stop pipe
whose `wait` reports `Modified` (a write or `IN_Q_OVERFLOW`) or
`Stopped`. A readable stop wins over pending writes, and each poll is
followed by at most one bounded batch, so a writer that never pauses
cannot postpone a stop. `IN_IGNORED`, `IN_DELETE_SELF` and `IN_UNMOUNT`,
and a record whose declared name the batch does not hold, are errors; a
batch with no write keeps waiting. On this
host's kernel, every write is notified, from this process or another,
while the fifo is readable or not; reads are not; and unlinking a fifo
that still has an open descriptor raises nothing, so a final's own pool
descriptor keeps its watch valid.

The controller starts a `Watcher` from `Hooks::jobserver_watch` right
after the pool, before any stage: a thread that sends `Event::Tokens`
only when its flag was clear, and `Event::WatchFailed` on an error. A
failed start is an error before any stage. Admission compares the count
with a stage's weight before taking; too few is a refusal that reads and
writes nothing, and marks the pass as waiting on tokens. At the block, a
noted `Tokens` event clears the flag and then, while a stage waits on
tokens, runs one ordinary pass, recorded as `woken` by `tokens`; with
none waiting it runs no pass. A `Tokens` event taken in the middle of a
pass is only noted, so the stages that pass already refused are
rechecked by the next one. `WatchFailed` is the named controller error
`jobserver watch failed`, and the stages drain as on any controller
error. Dropping the `Watcher`, on every return and unwinding, stops and
joins it, before the pool closes.

Controls: a running stage's raw returns start the waiting stage before
the returner ends; too few returned tokens start nothing, read nothing
(an `IN_ACCESS` count of zero from the returns to the controller's next
block) and wake at most once per write, the controller's own lease
return included; a competing client delays the waiter until it gives its
token back; a burst of single writes, two writes, and a return written
from `before_block`, between a refusal and the block, each start the
waiter; two handoffs are forced through the controller's seams, a write
while it holds a taken `Tokens` event with its flag still set (the
watcher coalesces it, and the recheck finds the whole capacity) and a
write after the flag is cleared and the recheck has refused the waiter
(the watcher sends a new event, and the write is not lost); a readiness
wait that runs out fails its control; a failure while a stage waits stops admission; a watch the kernel
removes (`IN_IGNORED` from a decoy fifo's unlink) and a watch that cannot
start are named errors. Removing the wake, never clearing the flag,
clearing it after the recheck, taking without the count, ignoring a
watch failure, a stop without priority, or an unchecked record length
each makes one of these fail.

The complete Rust final passed in 44.8 minutes, with 2024 memory.high
events and no memory.max or OOM events. Its schedule record shows 3120
wakes by returned tokens, 28 of whose passes started stages while others
ran: polars-suite at 7.4 minutes, production-cli-build at 11.6 and
rune-suite at 15.4 each started after a tokens wake, and budget-controls
and suite-adapter-candle started at 22.6 and 18.6, where ADR 0289's
final started both at 33.3. The longest block, 24.1 to 34.2 minutes with
only rune-suite running, held no stage refused tokens: rune-clippy waited
on its lane and fixed-route, which runs alone, on rune-suite; fixed-route
then ran from 34.2 to 43.9. The elapsed time is one run's and is not
attributed to the wake alone.
