# ADR 0283: Python tools become Rust xtasks

Status: Implemented

Base: published 515e5211ba38ceb7506c82bd844df3e80e686a8b (ADR 0282).

## Problem

rnx tracks 494 Python files, 48,102 lines. The gate that decides whether
a commit may be published is among them: the complete final's controller
is 26 modules and 4,110 lines under `tools/column`, with 65 test modules
of 4,667 lines. Its rules hold only by convention and by the tests that
happen to exercise them: stage records, receipts and resource
declarations are dictionaries whose shape no compiler checks, and a
misspelled key or a missing field fails at run time, sometimes an hour
into a final. Python also has no rule against growth: any record can add
another script.

This record moves the final's controller to Rust first, since every
other check runs under it. The stage programs it runs (the suite runner,
the Clippy and inventory stages, the fixed-route screen, the Jupyter
controls, commit discipline) stay Python here and move in later records,
one area at a time.

## Decision

1. A Rust command at the rnx root runs the final: the complete,
   required, explicit and docs scopes, revalidation and the docs
   successor, with the same arguments as `tools/column/final.py`. It is
   a workspace member with its own name, run through a Cargo alias, and
   uses no crate that `Cargo.lock` does not already hold.
2. Its stage records, selection, receipts and resource declarations are
   declared types. It reads and writes the receipts `final.py` does,
   field for field. Each controller revalidates the other's final as it
   stands: the other controller's tools identity, the original artifact
   bytes and digests and the released evidence, with no field erased or
   relabeled to make the two agree. A docs successor may follow a final
   from either.
3. It keeps every rule ADR 0282 and earlier records put in the Python
   controller, with the same meaning:
   - the stage list and selection, with no stage dropped, sampled or
     narrowed, and the source, candidate and selection checks
     throughout the run;
   - concurrent stages ordered by shared target directories and
     declared producers, commit discipline and the fetches first, the
     fixed-route screen alone, the first failure stopping admission and
     staying the cause, a signal draining every running stage;
   - supervision: each stage's exit status and its child's own receipt
     both validated; the controller blocks on exits and never polls;
   - failure diagnostics bounded, keeping the original cause and never
     exposing an environment value;
   - receipts written atomically and durably; exact named-root
     ownership, crash recovery and release on success and on failure,
     and revalidation after release;
   - admission of Valgrind and the Jupyter client only when a selected
     stage uses them, and of disk, counting target-directory growth,
     before each stage;
   - the docs successor, under which a plans-only amend runs only the
     existing documentation and commit checks and keeps the passed
     code's evidence;
   - the 60-minute wall error for a complete final, measured through
     release.
4. One budget covers the final, shared by the controller, the unchanged
   Python stage clients, Cargo, rustc and their nested children: the
   named-fifo jobserver of ADR 0282 (`MAKEFLAGS` with `-jN` and
   `--jobserver-auth=fifo:PATH`, and `RNX_FINAL_STAGE_TOKENS` naming the
   tokens a stage holds). The budget code of `rune/tools/xtask`'s
   scheduler moves into a library crate both commands use, extended to
   that protocol, rather than a second implementation: a stage's tokens
   are taken whole or not at all, a stage wider than the budget is
   refused before any stage starts, a stage's idle tokens serve its own
   cases, an `exclusive` case holds every token, cancellation returns
   what was taken, and no token is ever made. The memory policy moves
   with it: the headroom under every ancestor control group's high and
   max limits, the final's 25% reserve, the stage weights, and the
   memory events in the acceptance receipt. The fixed-route screen keeps
   its pinned recipe outside the pool.
5. The final's clock starts before the controller is compiled. A
   committed shell entry takes the start time and the candidate's
   identity (its commit, a clean tree, the lock file and the toolchain),
   refuses to build unless the free disk covers the final's floor and a
   cold Cargo target directory and the memory under every enclosing
   control group's limits covers one build job, builds the controller
   with the pinned toolchain and one job, and hands the controller a
   single-use record of all of it with the digest of the binary it
   built. Before anything else the controller checks that its running
   binary has that digest and that the candidate, the lock file and the
   toolchain are unchanged, and it refuses a complete final not started
   this way. The controller's target directory is a persistent cache
   like every other Cargo target directory, never released; the stages
   that build the controller declare it as their resource. No stage
   starts until the build ends, so no build overlaps the fixed-route
   screen.
6. Each stage runs under a guardian process outside the stage's own
   process group. When the stage ends, is stopped, or the controller
   dies, the guardian terminates every descendant of the stage, detached
   ones included, and records that it did. A complete final runs in its
   own systemd unit, which the controller checks: if a guardian is
   killed outright together with the controller, a detached descendant
   is contained only by that unit. Recovery releases a final's roots
   only after each guardian's completion record or the end of that
   final's unit, and otherwise refuses.
7. Both controllers gain the same stages. The budget library and the
   Rust controller are registered packages, so their tests
   (`budget-controls`, `final-controls`), formats and lints run in every
   complete final and whenever their sources change; `python-list`, the
   check of decision 9, runs in every required and complete selection.
   The Python controller runs them as it runs any stage, so it checks the
   Rust controller independently rather than the Rust controller
   accepting itself. These registrations are the only edits to the
   Python controller; its selection, verdict and scheduling logic stay as
   published, so it remains the witness.
8. The stage programs are unchanged, and the Rust controller runs them
   with the same commands and environment rules. The Python controller
   stays for this record only; the next record deletes it and ports the
   modules only it uses.
9. Python stops growing. A committed list names every tracked `.py`
   file with its disposition (port, delete, or stays because real
   Jupyter is the system under test). The first list holds exactly the
   tracked `.py` paths of 515e5211; after it, a list may only drop
   entries its parent had. A tracked `.py` missing from the list fails,
   a listed path that is not tracked fails, and a renamed file counts as
   an addition. The check is Rust, run by the `python-list` stage.
10. Parity is a committed comparison in the Rust command, not receipt
   equality. It names the fields that differ between any two runs
   (owned roots, invocation identifiers, times, resource observations,
   controller identity and binary digest) and compares everything else:
   the selected stages, their commands with owned roots substituted,
   their declared environments, every deterministic row and pin
   exactly, and each property verdict for observations that vary by
   run. A missing or extra stage or row, a changed command, or a changed
   verdict fails it; raw clock values are not compared and no other
   field is ignored.

## Constraints

- No test, check, contract row or stage is dropped or narrowed. The
  Python controller's tests stay and run.
- No new third-party crate. No new Python. No real environment or argv
  in a committed fixture.

## Controls

- A committed table pairs each control of the Python controller with
  its Rust test, and a Rust test fails when a Python control has no
  pair. Each paired test uses the same fixture and assertion, and each
  planted negative fails under Rust as under Python.
- The budget, by process controls with the unchanged Python client and
  Cargo under the Rust controller's pool: whole-width acquisition and
  refusal, a stage's own idle tokens, an `exclusive` case alone, a
  one-token budget that finishes, cancellation that returns every
  token, and the token count on a unit with 48 GiB of headroom equal to
  the Python controller's.
- Revalidation across controllers on fixture finals, in both
  directions: a passed final, a failed one, one with a tampered
  artifact, and one with an incomplete release; the last two are
  refused.
- Bootstrap: a candidate whose controller does not build fails with no
  output directory and no stage; a source change between the entry's
  capture and the controller's check, an invalid or reused handoff
  record, out-of-order or future times, a binary that is not the
  candidate's build, and too little memory for the build are each
  refused; the build's time counts toward the wall budget; a change to
  the controller's source selects `final-controls`.
- Guardians: a stage does not start before its guardian is recorded; a
  controller that dies before releasing a guardian leaves the stage
  unrun; a detached grandchild is gone when its stage ends, and when the
  controller is killed outright and recovery runs, before any root is
  released; a guardian killed without its completion record makes
  recovery refuse release outside a finished unit; a reused process
  identity is never signaled.
- The Python list: a tracked `.py` missing from the list, a listed path
  not tracked, a list that gains a path its parent lacked, and a rename
  each fail (committed controls, with a planted negative for each).
- The parity comparison: a planted missing stage, extra row, changed
  command and changed verdict each fail it (committed controls).
- On the signed candidate: one complete final under each controller,
  one after the other, never together. The parity comparison passes on
  the two; each controller revalidates the other's final; the Rust final
  passes under 60 minutes; the packet shows both finals' stage
  durations.

## Out of scope

Porting the stage programs, `ci/`, the measurement tools, `rune/tools`
and the probes; each area follows in its own record, in the order the
committed list sets. Also not here: caching the baseline lint builds
across finals, the control group's memory-high events, and the three
interrupt tests that still signal after a fixed delay, which remain
open follow-ups from ADR 0282.
