# ADR 0290: Returned tokens wake admission

Status: Proposed

Base: published bf3210726c19e0e24e376fcb4d49a0d9889f1b9c (ADR 0289).

## Problem

Between admission passes the controller blocks on its event channel,
which carries only a stage's end and a signal. A stage refused its
tokens is retried only when some stage ends: tokens a running stage's
Cargo returns to the jobserver mid-stage wake nothing. The schedule
records of ADRs 0288 and 0289 each show the controller blocked for about
ten minutes after token refusals, with one stage running and eligible
stages waiting. That is a measured blocked interval, not a measured
amount of unused capacity: the records show refusals at decision times,
not whether tokens were free between them. This record adds the missing
wake and lets the controls and the next final show what it changes.

## Decision

1. A write to the jobserver fifo, by any process, is a new reason to
   re-evaluate admission, beside a stage's end and a signal. Nothing
   else about admission changes: the sole pool, Cargo's protocol, stage
   weights, whole-or-none acquisition with no partial lease held, the
   selection order, prerequisites, resource lanes, the alone and first
   rules, memory and disk admission, and the fixed-route isolation.
   Start times may move earlier; who is eligible, and in what order, may
   not.
2. The wake is the kernel's. An inotify watch for `IN_MODIFY` on the
   fifo's inode, armed when the pool is created and before any stage
   starts, is notified of writes: the controller's own returns, a stage's
   Rust client, and Cargo's and rustc's raw writes, which no Rust return
   hook would see. Reads raise no `IN_MODIFY`, so taking tokens wakes
   nothing. Notifications are capacity hints and may coalesce (the
   kernel merges identical unread events, and reports a full queue as
   `IN_Q_OVERFLOW`): nothing counts them, no verdict depends on their
   number matching writes or tokens, and one notification suffices,
   because the recheck it causes reads the whole current count. An
   overflow is a notification like any other: capacity may have changed,
   so recheck.
3. Capacity is read without consuming it. Before taking a stage's
   weight, admission reads the fifo's byte count (`FIONREAD`); fewer
   tokens than the weight is a refusal with nothing read and nothing
   written back. Only when the count suffices does it take the tokens,
   whole or none, as now.
4. The wait boundary, with no lost wake and no busy retry:
   - A watcher thread waits on the inotify descriptor and a stop pipe,
     both by `poll`. It reads the whole queued batch first, then sends
     one `Tokens` event to the controller's channel only if none is
     outstanding: a flag it sets before sending.
   - The controller, taking a `Tokens` event, clears the flag first and
     only then rechecks capacity; it never clears it after the recheck.
     A write already coalesced under the old flag is covered by that
     recheck, and a write after the clear sends a new event, so no
     write can fall between the two.
   - A pass that refuses a stage's tokens marks the controller as
     waiting on tokens. Any write after the watch was armed, including
     one between the refusal and the controller's next block, has
     queued an event, so the block returns: no write is lost between
     failed acquisition and arming, because arming precedes everything.
   - A `Tokens` event while no refused stage waits is still taken, and
     its flag cleared, but runs no pass. While one waits, it runs one
     ordinary pass.
   - Unchanged insufficient capacity cannot loop. A wake comes only from
     a write, and a pass that finds too few tokens reads and writes
     nothing, so it raises no new event. The one remaining self-wake is
     a race: the count sufficed, a competing client took tokens first,
     and the partial read went back. That write wakes one more pass,
     which finds the count short and stops; it repeats only if another
     process keeps racing it.
   - A watcher failure reaches the controller: a failed initialization
     is an explicit error before any stage; the watch removed or
     invalidated (`IN_IGNORED`, `IN_DELETE_SELF`, `IN_UNMOUNT`), or a
     failed poll or read, ends the watcher with a named error event, on
     which the controller stops admission and drains its running stages
     as on any controller error, never sleeping on a dead source.
   - The watcher is owned by a guard: on every return, an error's and an
     unwinding's included, it is stopped through its pipe and joined,
     before the pool is closed and its fifo removed.
5. `SCHEDULE.json` records a `Tokens` wake as `woken` by `tokens`,
   distinct from a stage's end and a signal; validation accepts it.

## Constraints

- No new crate (inotify and `FIONREAD` through `libc`); no new Python
  file; the budget, coverage and 60-minute limit are unchanged.
- No other admission policy change.

## Controls

Readiness-driven, through the controller's test seams and a real pool:

- A running stage returns enough tokens by raw fifo writes and stays
  alive until the waiting stage has started; the waiting stage must
  start before the running one ends. The controller without the wake
  fails this control.
- Returns that leave too few tokens start nothing, and wake at most once
  per write.
- A competing client holding tokens, then returning them, starts the
  waiting stage only when the count suffices.
- Several returns, and a burst of raw returns before the controller
  takes its single pending event; a return while an event is
  outstanding; a return after the flag is cleared and an insufficient
  check is made, before the controller blocks.
- Injected notifications: an overflow rechecks; a removed watch and a
  watcher error are named controller errors, with the stages drained.
- Token conservation at the pool's close, and stop or failure while a
  stage waits on tokens (no hang; the watcher joins).
- The existing exclusive-case and alone-stage controls unchanged.
- One complete Rust final, whose schedule record shows the token wakes.
