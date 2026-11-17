# Implementation plan 0290: Returned tokens wake admission

Status: Proposed

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
