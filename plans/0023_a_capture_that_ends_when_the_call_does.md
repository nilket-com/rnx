# rnx 0023: a capture that ends when the call does

Status: proposed 2026-09-09. The twenty-third record of rnx. A deadline ends
the wait for a child but not the reading of what it said, so a call can report
a timeout correctly and return long afterwards. This record gives the reading
an end too, and says how a caller tells a short capture from a truncated one.

## Context

Record 0020 named this and left it, with a reproducer instead of a test.
Record 0022 fixed the same shape in the other direction — the writing of a
child's input — and inherited this one unchanged. Measured now, with a **200
ms** deadline and a descendant that leaves the process group and lives three
seconds:

| What the descendant keeps | The call returns after |
| --- | --- |
| neither reply pipe | 306 ms |
| standard output | **3,111 ms** |
| standard error | **3,111 ms** |
| both | **3,108 ms** |

`timed_out` is true in every row. The report is right and the return is late
by the descendant's whole life, which is not the deadline's to give away.

Two more things the same code hides:

- **A read failure reads as the end of the stream.** `capture` loops
  `while let Ok(n) = input.read(..)`, so an error ends the capture exactly as
  end-of-file does, and a caller cannot tell a stream that finished from one
  that broke.
- **There is one flag for two different things.** `truncated` means the size
  cap was reached. A capture cut short for any other reason has nothing to
  say, so a script that must know whether it has everything cannot find out.

**What a cleanup allowance can and cannot be justified by.** Killing the
child's process group does not stop a descendant that left it: such a
descendant can go on writing to the pipe it kept, for as long as it lives. So
the data remaining after the wait is **not** bounded by a pipeful, and no
allowance can be justified by the volume left to read.

What bounds a continuous writer is checking the clock **before every read**,
which decision 1 requires and gate 9 exercises with a descendant that writes
without stopping. The allowance is then only a limit on how long rnx keeps
asking, and 100 ms is a **measured starting choice** rather than a derived
one: draining the whole two mebibyte cap takes a few milliseconds —
`host::process_bytes` of three megabytes from `/dev/zero` completes in 11 ms
including rnx's own startup — so it is comfortably more than an ordinary
cleanup needs, and gate 6 is what would fail if it were ever too little.


## Decision

### 1. One deadline, and one cleanup instant shared by both readers

The deadline still governs the wait for the child, unchanged. The moment the
wait ends the call publishes **one instant** into a set-once cell both readers
hold, and it does so **first** — before the writer is stopped, before the
child is reaped, before anything else is collected — because everything after
it is cleanup and the readers should already be winding down while it happens.

One instant, set once, read by both, so two readers cannot spend the allowance
twice. It is published when the wait ends rather than computed at the start,
because a child that ran for a minute should not have spent its readers'
allowance while it was running.

**How far ahead the instant is depends on why the wait ended, and that is a
decision rather than an oversight:**

- The child exited, or the deadline passed: **100 ms**. There may be bytes
  already in the pipes and they are worth having.
- The call was **cancelled**: **none**. Ctrl-C means stop, not tidy up.
  Record 0020 measured interruption latency at about 7 ms and this record
  will not spend a hundred milliseconds of it collecting output nobody asked
  for. Whatever had not been read is reported as `cut_short`.

**A bound on asking is not a wall-clock guarantee.** The instant is checked
before every read attempt, and the pause between attempts is a sleep, which is
a request the scheduler may overrun. So what is promised is that rnx stops
asking for more once the instant has passed — not that the call returns by
any particular time on a loaded machine. Every measurement below reports what
was observed rather than what was promised.


**The readers are made non-blocking and stop by themselves**, the way record
0022's writer does: the instant, and the process's interrupt flag, are read
before every attempt rather than only when a read would block, and the growing
wait between attempts is record 0020's. Nothing depends on a descendant
closing anything, which is the whole point — a descendant outside the process
group cannot be made to.

**An interrupt during the cleanup is still an interrupt.** The wait loop stops
watching for one when it ends, so the readers are the only thing looking and
the call reads the flag again before it reports: without that, a SIGINT
arriving while the allowance ran was simply ignored, and the call waited the
allowance out and reported `cancelled=false` with a capture cut short for no
reason it could name. Gated separately from cancelling while the child is
still being waited for, because it is a different code path with a different
watcher.


**Both readers are always joined and their outcomes always collected — even
when one of them fails.** Both joins are taken before either failure is
propagated, because a `?` on the first would return while the second reader
was still running. That is record 0022's lesson repeated twice over: the same
defect, in the same shape, on the way out of the same function.

### 2. Three ways a capture can fall short, and three ways to say so

Three things can go wrong with a capture, and they are **independent** rather
than four alternative endings: a stream can pass the size cap, and then be
cut short when the instant arrives, and a read of it can fail — all three, in
that order, on the same stream. So each is its own flag, and any combination
is possible:

| Flag | What it says |
| --- | --- |
| `truncated` | the size cap was reached; bytes past it were discarded |
| `cut_short` | rnx stopped reading before the stream ended |
| `unreadable` | a read failed |

An earlier draft of this record presented these as one ending out of four,
which would have made a caller believe that a `truncated` capture had not also
been cut short. They are three questions, each answerable independently.


`truncated` keeps its meaning and its name, so the four ports and anything
else reading it are untouched. The two new flags are additions, and each names
a cause rather than a symptom: `cut_short` says rnx stopped reading,
`unreadable` says the stream could not be read. A caller that needs to know it
has everything checks all three; one that only ever cared about the size cap
carries on as before.

Each is tracked **per stream** and reported as the union, which is how
`truncated` already behaves. The per-stream detail is not thrown away, because
record 0016 lets a stream's own shortfall excuse its own partial last
character when decoding, and the three flags do not excuse alike:

- `truncated` excuses a partial last character, as it does today.
- `cut_short` excuses one for the same reason: the bytes stopped arriving
  because rnx stopped reading, not because the child sent something invalid.
- `unreadable` **excuses nothing, and overrides both**. A stream whose read
  failed is not known to have been cut at a boundary or anywhere else, so a
  stream that is both truncated and unreadable is refused rather than
  excused. The exemption is a statement about where the bytes stopped, and a
  failed read is the case where that is unknown.

The exemptions are per stream and must stay that way: a truncated standard
output has never excused a standard error the child ended mid-character, and
an unreadable standard error must not refuse a standard output that was read
whole. The crossed cases are gated rather than reasoned about.


### 3. Every caller that needs complete output is corrected, not just left alone

Keeping `truncated` unchanged keeps the ports **compiling**; it does not keep
them **right**. A child can exit 0 while a descendant holds a reply pipe with
bytes still in it, and this record makes that return a prefix with `cut_short`
set — which a check written against `truncated` alone will wave through as
success. So all four are audited here rather than assumed:

| Port | Children | What it checked | What it must check |
| --- | --- | --- | --- |
| `classify_fold_perf.rn` | 1 | **nothing** | all three |
| `summarize_runtime_filter_perft.rn` | 0 | not applicable | not applicable |
| `check_blake3_confinement.rn` | 1 | `truncated`, `timed_out`, `cancelled` | and the two new flags |
| `verify_nilket_graft.rn` | 3 | `truncated`, `timed_out`, `cancelled` at each | and the two new flags |

`classify_fold_perf.rn` is the one worth naming: it reads `perf` output and
has **never** checked whether rnx captured all of it. That is a gap this
record found rather than made — record 0015 learned the same lesson from a
different script, that an ignored truncated capture reports a clean check —
and it is closed here because this record is what makes partial output
likely enough to matter.

A completeness check now reads `truncated || cut_short || unreadable`. The
wording of each refusal stays the port's own, because each is compared against
its original byte for byte and a diagnostic is part of that.

### 4. What a caller is told, in the description it reads

The three flags are in the text `:help` shows for `process`, `process_bytes`
and `process_bytes_input`, along with what to do about them: check `timed_out`
and `cancelled` first, then `truncated`, `cut_short` and `unreadable`. An
interface that hides the flags a caller now has to inspect is not an interface
that added them.

### 5. What this record does not decide

Whether a script should be able to choose the allowance. It is a cleanup
window, not a budget, and nothing measured wants it adjustable; record 0021's
flag exists for a limit a workload can outgrow, and this is not one.

Raising the two mebibyte cap, which predates all of this.

A persistent child, which record 0022 deferred and nothing here needs.

## Acceptance gates

1. **A held reply pipe no longer holds the call.** The Context's table,
   re-measured with a 200 ms deadline and a descendant living three seconds:

   | What the descendant keeps | Before | After |
   | --- | --- | --- |
   | neither reply pipe | 306 ms | 306 ms |
   | standard output | 3,111 ms | **406 ms** |
   | standard error | 3,111 ms | **405 ms** |
   | both | 3,108 ms | **405 ms** |

   The deadline plus the allowance plus rnx's own startup, instead of the
   descendant's whole life.

2. **The readers are gone, not detached.** While the descendant still holds
   the pipes, the process has no reader threads left: counted from `/proc`
   after the call returns, with the handshake harness proving the descendant
   still holds them at that moment. The harness's release-and-acknowledge is
   what makes "still holds" a fact, and closing **every** duplicate
   descriptor is what makes its acknowledgment true.
3. **A child that exits with its streams ended spends none of the allowance.**
   Clock-controlled rather than timed against a long deadline: the allowance
   is set to **three seconds** through the same `test-support` feature record
   0022 uses, and the call still returns in **8 ms**. An unnecessary hundred
   millisecond sleep would be invisible against a thirty second deadline and
   is impossible to miss against a three second allowance.
4. **A child that exits while a descendant keeps a pipe returns after the
   cleanup, not after the deadline.** Deadline of thirty seconds, child gone
   in milliseconds, descendant holding standard output for six: measured
   **306 ms**, with `timed_out` false and `cut_short` true. This is the case
   that distinguishes "the wait ended" from "the reading ended".

5. **The three shortfalls are distinguishable, and they overlap.** A capture
   over the size cap reports `truncated`; one ended by the allowance reports
   `cut_short`; one whose read failed reports `unreadable`. A stream that
   passes the cap **and** is then cut short reports **both**, which is the
   case an exclusive ending would have got wrong. The read failure is
   injected behind the same `test-support` feature record 0022 uses, because a
   read error on a pipe cannot be provoked from a script.

   And the decoding rules are gated across streams: `unreadable` refuses a
   stream that `truncated` would have excused, a truncated standard output
   still does not excuse a standard error the child ended mid-character, and
   an unreadable standard error does not refuse a standard output that was
   read whole.

6. **A cancellation stops the readers at once.** Interrupted with a descendant
   holding both pipes, the call returns with `cancelled` and `cut_short`,
   spending no allowance — decision 1 gives a cancellation none.

   Latency, measured on the same script before and after this cut: **102 ms
   before, 107 ms after**, so nothing regressed. That figure is not record
   0020's 7 ms and is not meant to be: 0020 measured a different script, and
   the ~100 ms here is present in both binaries and belongs to something
   else. Setting the allowance to 0, 100 and 1,000 ms changes it by nothing,
   which is how it was established that the allowance is not part of it.

7. **What was captured is still captured.** A child that writes a mebibyte to
   each stream and exits has both captured whole, with no flag set: the
   allowance must not cost bytes that were there to be read.
8. **The old meanings hold.** `truncated` still means the size cap, still
   excuses a partial last character on its own stream, and the gates of
   records 0013, 0016, 0020 and 0022 pass unchanged.
9. **A descendant that never stops writing does not hold the call.** One that
   leaves the process group and writes continuously to the pipe it kept — the
   case the group kill cannot stop and a pipeful's worth of reasoning does not
   cover — is read until the instant passes and no further. Measured with a
   200 ms deadline: the call returns in **406 ms** with `truncated` and
   `cut_short` both set, having filled the cap and then stopped.
10. **A completeness check catches partial output.** The state the ports could
    not see: a child exits 0, a descendant keeps the pipe with bytes in it,
    and the call returns `printf 'partial'`'s seven bytes with **`truncated`
    false and `cut_short` true**. Demonstrated both ways — the old check
    `truncated || timed_out || cancelled` prints `ACCEPTED AS COMPLETE`, and
    the corrected check refuses — which is what makes decision 3 a correction
    rather than a precaution.

    **The order of the checks matters, and the fixtures found it.** A deadline
    and a cancellation both leave `cut_short` set, so a port that tests the
    shortfall first reports a symptom where it should report a cause: the
    blake3 comparison failed on exactly that, its cancellation case printing
    "rnx did not read all of grep's output" instead of "interrupted while grep
    was running". Every port now tests `timed_out` and `cancelled` before the
    three capture flags, and the flags catch what is left — a capture cut
    short with neither a deadline nor an interrupt.

11. **An interrupt during the cleanup is reported and obeyed.** With the
    allowance set to a second and the child already gone, a SIGINT arriving
    inside it returns in **53 ms** with `cancelled=true` and `cut_short=true`.
    Against the same binary without the two flag reads it waits the second out
    and reports `cancelled=false`, which is what makes this a gate.
12. **A reader that panics does not strand the other.** The standard output
    reader is made to panic while a descendant outside the process group holds
    standard error. The call fails and says which reader panicked, and the
    process has **one** thread afterwards — counted from `/proc` while the
    descendant still holds the pipe. Failing is not enough to observe here,
    because the call fails either way; the stranded thread is the difference,
    and the sequential joins show two.
13. **Nothing regresses.** The gates of records 0002 through 0022 pass, all
    four ports match their originals, and the graft verifier still agrees with
    the python original on the full 1,346-commit lineage.


## Guardrails and stop conditions

1. One clock. If the two readers ever get an allowance each, that is the
   defect this record exists to prevent.
2. Nothing waits on a descendant. If any part of the cleanup can be extended
   by a process outside the group, it is not done.
3. `truncated` does not change meaning. A new cause gets a new name, because a
   caller that reads the old one must keep being right.
4. The allowance is not a budget. If a workload wants a longer one, the reason
   is recorded before a knob is added.

## Risks

- **A hundred milliseconds is a guess with three orders of magnitude of
  headroom.** It covers a pipe's worth of bytes, and draining a full cap takes
  a few milliseconds; if a machine ever needs more, gate 6 is what fails
  first, and the number moves with a measurement rather than a hunch.
- **A cut-short capture is a new thing for a caller to check.** A script that
  only reads `truncated` keeps working, and is now wrong in a case it could
  not previously detect at all — which is an improvement in what can be known
  rather than a new hazard.
- **`unreadable` may never be seen in practice.** It exists because a read
  error currently reads as the end of the stream, which is a lie that costs
  nothing to stop telling.

## Forward

With this, the process functions' lifecycle is closed: a bounded wait, a
bounded delivery, and a bounded capture, each stopping when the call does and
each reporting what it could not finish. What remains unclaimed is a second
script that reads bytes, which the repository had no need of when it was last
checked, and the two upstream Rune drafts, which are written and unfiled.
