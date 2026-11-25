# ADR 0336: Result callbacks run on the new engine

Status: Proposed

Base: published 6cefc3496411e92d0516a4e60517a23694bdabf8 (ADR 0335).

## Problem

The fallback-deletion gate on aa5af565 stopped on 118 stock routing
cases that succeed today only through the legacy route. ADR 0334
restored 28 of them. Apart from async functions, generators and byte
strings, 26 cases remain. Replayed on the published base with
fallback disabled, all 26 refuse before any output, at the same
category and span in both native modes.

| Cases | First refusal | Blocker | Record |
|---:|---|---|---|
| 6 | Method | `Result::map` and `and_then` with a closure | this one |
| 6 | Match | matching a range value against its type | later |
| 3 | Method | `push_str` with checked borrows | later |
| 3 | Method | slice iterator `collect` and `sort` | later |
| 2 | ConstCall | const function call in an expression | later |
| 1 | FrontendDiagnostic | const block as a format string | later |
| 1 | Method | `next` on a script-defined iterator | later |
| 1 | Native | `is_readable` on a dropped alias | later |
| 3 | Native | `with_capacity`, then `into_bytes` | byte strings |

The groups need different work, so each gets its own record. The
last group needs byte strings and moves to that record whole.

The six cases of this record are l1-00406 to l1-00411. They are six
entry points into one module. The module calls `Ok(1).map(...)`
once and `Ok(1).and_then(...)` twice, with a closure that returns
`Ok` and one that returns `Err`. Its `expect`, `unwrap` and
`unwrap_or` calls already run.

## Decision

1. Admit `Result::map` and `Result::and_then` with a callback on the
   new engine with the old core's behavior: the same results, output,
   errors and effects, in the same order.
2. The receiver and the callback expression are each evaluated once,
   in source order, whether the receiver is `Ok` or `Err`. The
   callback runs only for `Ok`. An `Err` result carries the receiver's
   payload, the same value and not a copy.
3. The old core holds a shared borrow of the receiver while the
   callback runs, takes the callback by value, and takes the `Result`
   that an `and_then` callback returns. The new engine has no borrow
   that lasts across a script call. Admit a call only where no
   program can observe the difference:
   - the receiver is a standard `Ok` or `Err` built in the call
     expression, so nothing else refers to it;
   - the callback is a closure written in the call expression, with
     any captures and effects;
   - for `and_then`, every normal exit of the callback returns a
     standard `Result` built there.
   A callback may fail or never return.
4. Every other call keeps a compile-time refusal that names the form
   and its span. That includes a receiver held in a variable or
   returned from a call, a callback passed by name, a callback that
   isn't a function, and an `and_then` callback that returns a
   `Result` it didn't build.
5. Admission is by identity. A `map` or `and_then` that is missing,
   overridden or installed by someone else keeps its refusal. A
   source-defined method with the same name keeps its own behavior.
6. The calls refused under Decision 4 need a borrow that lasts
   across a script call and ends on every exit, and a way to take a
   callback and its returned `Result`. That is a later record. Return
   to review if the admitted calls need a new opcode, a dispatcher
   change or a host ABI change.
7. The live memory control establishes its own precondition. The
   fixture in `tools/column/memory_live.py` writes a clean file and
   assumes the kernel keeps that cache charged to its unit. Before
   scoring, it now requires from the kernel's own fields that raw
   headroom under the unit's limit is below the refusal line and that
   enough clean file cache is charged. If short, it appends and syncs
   a 512 MiB extent and observes again, at most twice. If the
   precondition still isn't met, it fails as an unmet precondition
   and reports every observation. Once met, the accounting is scored
   once on that observation, and a wrong answer fails at once. The
   accounting, its thresholds, the schedule and the timeouts don't
   change.
8. The cancellation test for quiet capture asserts what the helper
   controls. `cancellation_with_full_pipes_is_prompt_and_reaped` in
   `tools/project` bounds all of `Capture::finish` by five seconds
   of wall time. That span includes the log's `fsync`, whose duration
   belongs to the host: under the final's write load one `fsync` on
   ext4 took 4.6 s after a drain that succeeded. The test drops that
   bound. It keeps the checks that the drain reached the end of the
   output, that the output is in the report, and that the child's
   group is gone, and it now compares the standard descriptors'
   identities before capture and after `finish`. A hang is still
   caught by the isolated child's 60-second deadline. `finish` itself
   doesn't change: its five-second drain bound and the log sync stay.
9. Scope: these two methods, that fixture and that test. No change
   to the legacy engine and no change to programs admitted today.
   The two control repairs ride in this record because each stopped
   a complete final of it. That is a deliberate exception to one
   subject per record.

## Controls

1. Replay the six case IDs with the same harness entry, argv and
   both native modes, with fallback disabled, against the published
   runner for status, stdout, stderr and file effects. Count a case
   restored only when the full source runs and agrees. Report reduced
   probes separately from full-source replays.
2. A parity table against the old core for both methods, within the
   admitted calls: `Ok` and `Err` receivers, captured values, effects
   in the receiver, the callback expression and the callback body, a
   callback that fails, nested `Result` values, and identity of the
   `Err` payload.
3. Each of the 14 ownership observations taken on the old core
   either agrees on the new engine or is refused at its span. None
   runs with a different outcome.
4. Each shape named in Decision 4 has a test for its refusal message
   and span. The missing, overridden and foreign-installed cases of
   each method keep their refusals.
5. The existing contract rows that record these forms as refused are
   replaced by measured outcomes. Unaffected dispositions stay.
6. The other 20 cases keep the first refusal recorded at the
   boundary, by category and span.
7. The file gate and both project screens, image to image against
   the published base. No allowance.
8. The fixture: a first observation that qualifies writes nothing
   more. A planted shortfall is admitted only on a later observation
   that qualifies. A lasting shortfall fails as an unmet precondition
   at the byte bound without asking for a budget. A qualifying
   observation with a wrong budget fails as accounting without a
   further write. Evicting the file's cache under a running compiler
   reproduces the first final's failure on the old fixture and passes
   on the new one.
9. The cancellation test: it passes with the log's `fsync` delayed
   past five seconds. A writer held open past the drain bound still
   fails it, and so does a standard stream left unrestored. The
   lingering-writer test stays as it is. The project suite and the
   Python controller and deadline controls pass repeated runs under
   combined CPU and file-write load.
10. Finals: targeted pre-sign checks in separate state, with every
    measured delta checked against its band, then one complete final
    on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- Programs admitted today keep the same results and effects.
