# rnx 0142 evidence: two project-suite flakes, and an ungated Polars test

**The result:** both project-suite flakes have a **demonstrated** cause, one shared mechanism: a concurrent test's spawn forks every open descriptor and holds the copies until its `exec`. Each test is fixed, with a deterministic control that reproduces its mechanism on demand. `tests/dense.rs` is gated where it uses generated bindings. **No production code changed.**

## 1. The shared mechanism, and its helper

**`tools/project/src/fork_hold.rs`** (test-only, Linux) holds this to the contract from Codex's review of the plan:
- **`Held::fork()`** forks a child that, using only async-signal-safe raw calls, closes its copy of a pipe's write end, blocks on `read` of the read end, then calls `_exit`. It runs no Rust code after the fork.
- **The parent decides when it exits** by writing a **release byte**, not by closing the pipe (review round 1, below). `release()` then reaps with a non-blocking `waitpid` for up to 5 s, falls back to `SIGKILL` and a reap, and returns whether the child exited on the byte. Every caller asserts that it did.
- **The guard:** if the parent never releases it (an assertion failed first), `Drop` kills it with `SIGKILL` and reaps it.
- **Review round 1 (Codex, reproduced deterministically):**
  - **The flaw:** the first version released the child by closing the pipe's write end, then waited for it without bound. A second held child, forked while the first was alive, inherits the first's write end, so the first child never saw EOF until the second exited. The helper recreated the very hazard it demonstrates.
  - **The fix:** the explicit release byte (only the child reads its own pipe, so a stray copy of the writer can't hold it) and the bounded reap.
  - **The control, `a_release_does_not_wait_for_another_held_child`,** is Codex's reproducer: with two held children, `first.release()` must return on its byte within 2 s while the second is still alive, then the second is released.

## 2. `capability_refuses_bad_replies_and_retires_timed_out_child`

**Reproduced first (this corrects 0138):**
- **Baseline:** the whole suite failed 1 in 30 runs at default parallelism.
- **With the must-succeed assertions temporarily printing their error** (since reverted): **3 in 80, every one `Text file busy (os error 26)`**, never the 1 s deadline 0138 suspected.
- **The script was written by the test process.** A spawn forked by any concurrent test held a copy of the writer until its `exec`, and Linux refuses to `exec` a file open for writing.

**The fix:** `install()` writes the body to a staging file, and `/bin/cp`, another process, writes the executable. This process never holds a writer on anything it executes. The handshake is unchanged.

**The control (`an_inherited_writer_makes_the_script_busy_and_cp_installation_does_not`),** with separate paths for each arm:
- **Unfixed:** this process writes the executable, a held child inherits the writer, and the parent closes its own. The handshake fails with "Text file busy".
- **Fixed:** the held child inherits a writer on the staging file only, and `cp` installs the executable. The handshake succeeds **while the child is alive**.

## 3. `builders_share_the_coordination_lock_and_exclude_removal`

**0138's hypothesis, now demonstrated** (`an_inherited_description_keeps_the_shared_lock_until_the_child_ends`):
- the test takes a shared lock, and a held child inherits its description;
- dropping every handle in this process still leaves the exclusive `try_lock` at `WouldBlock`, because the child is alive;
- once the child is released and reaped, the exclusive lock is acquired.

It didn't reproduce in this record's 30 baseline runs (it failed once in 0138's), so the control is the evidence.

**The fix (`freed_within`):**
- **Only the final acquisition waits:** it polls every 10 ms for up to 5 s, as production's `lock()` does (it retries `WouldBlock` every 10 ms), and any other error fails at once.
- **The control's own final acquisition uses it too:** another test's spawn may also have forked while the shared handle was open.
- **The earlier `WouldBlock` assertions stay instant:** an inherited copy can only lengthen a hold.
- **Production is unchanged:** it already waits through a transient hold.

**Scope:** the controls and the helper are compiled on Linux only, where `ETXTBSY` on `exec` and descriptor-owned `flock` locks are verified. Installing by `cp` and the bounded final wait apply on every Unix.

## 4. `tests/dense.rs`

- **Gated:** `round_trips_and_every_binding_stays_usable` and `strings_are_checked_before_copying_and_the_frame_stays_usable` call the generated `select_`, so both are gated on `generated`.
- **Without `generated`:** the `dense` binary runs **exactly 4** tests, all passing (6 by default).
- **The whole `--no-default-features` suite:** 24 passed, 0 failed (it was 2 failed before).

## Gates

- **The project suite, on the final helper (after review round 1): 100 whole runs at default parallelism, all 100 green,** with 105 tests passing each run: 0141's 102 plus the three controls (`out/gate-100.txt`). The first version's gate was also 100 of 100 (104 tests), but it predates the helper fix, so it's superseded.
  - **Before:** 1 failure in 30 (`out/baseline-30.txt`), and 3 in 80 in the diagnostic runs (`out/diagnosis-80.txt`, every one `Text file busy`).
- **Core,** with `server-runtime`: 408 passed.
- **Polars:** default 43; `test-support` 273; `--no-default-features` 24, 0 failed.
  - **`oracle-results.json`,** rewritten by release runs, is restored to the committed copy.
- **Checks:** `cargo fmt` (project and Polars), `git diff --check`, and clippy on the touched files, all clean.
