# rnx 0142: two project-suite flakes, and an ungated Polars test

Status: plan, the third of the order agreed with Codex after 0139 (0140, 0141, then 0142). Maintenance only: no product behaviour changes.

## 0. The fork controls' lifecycle (Codex's review of this plan)

Both mechanism controls fork a child from a multithreaded Rust test process. That child is held to strict rules:
- **Only async-signal-safe raw calls until `_exit`:**
  - it closes its copy of the pipe's write end;
  - it blocks on `read` of the pipe's read end, which keeps the inherited descriptor alive;
  - it calls `libc::_exit`.
  - **Nothing else:** no Rust sleep, allocation, assertion, logging, unwinding or destructors.
- **The parent controls the release; there's no fixed sleep:**
  - the child can't exit before the parent writes to (or closes) the pipe, so it's known to be alive while the refusal is asserted;
  - the parent then releases it and reaps it with `waitpid`, and only then asserts success.
- **Cleanup always happens:** a guard on the parent's side kills (`SIGKILL`) and reaps the child if it hasn't been released, so an assertion failure can't leak it.
- **Scoped to Linux,** where `ETXTBSY` on `exec` and the descriptor-owned `flock` behaviour are verified. Elsewhere the controls aren't compiled. The two test fixes themselves (installing by `cp`, the bounded final wait) are portable and apply on every Unix.

## 1. `capability_refuses_bad_replies_and_retires_timed_out_child`: reproduced, and it isn't the deadline

**The cause 0138 guessed was the 1 s handshake deadline under host load. That was wrong.** Measured before planning:
- **The baseline:** the project suite's release test binary, run whole at default parallelism, failed this test **1 time in 30** (the other 29 runs were fully green).
- **What failed:** the first handshake, at `src/tests.rs:325`, which must succeed. The assertion (`is_ok()`) hid the error.
- **With the error printed** (a temporary `unwrap()` on the two must-succeed handshakes, since reverted): **3 failures in 80 runs.** All three were `executable …/executable: Text file busy (os error 26)`: two at line 325 and one at 327, **never the deadline.**

**The mechanism (`ETXTBSY`):**
- `set()` writes the script with `std::fs::write` inside the test process, then the handshake executes it.
- Any concurrent test that spawns a process forks the whole fd table. The child holds a copy of the script's writable descriptor until it calls `exec`, even though the descriptor is close-on-exec.
- If the handshake's `exec` lands in that window, Linux refuses it, because a writer is still open.

**The fix (test only):**
- The script is never opened for writing by the test process. `set()` writes the body to a staging file, then has `/bin/cp` (a separate process) produce the executable.
- When `cp` exits, no process holds a write descriptor on the executable. A concurrent fork can only inherit the staging file's descriptor, and the staging file is never executed.
- The production handshake is unchanged: it executes binaries other processes wrote.

**The deterministic control** (`tests.rs`, Linux), with separate fixtures and paths for each arm:
- **The unfixed arm:** write the executable through a descriptor this process holds open, fork the child of section 0 (it inherits that writer), close the parent's descriptor, then run `handshake::check`. It must fail with `Text file busy`.
- **The fixed arm:** write a staging file through a held descriptor, and fork the child (it inherits the **staging** writer, never one on the executable). Install the executable from the staging file with `cp`, close the parent's descriptor, then run the handshake. It must succeed while the child is alive.
- **After either arm,** the child is released and reaped.

**The measure:** the whole suite, 100 runs at default parallelism, against the 3 in 80 above.

## 2. `builders_share_the_coordination_lock_and_exclude_removal`: the hypothesis, tested

**0138's hypothesis:** a concurrent fork briefly holds a copy of the lock's open file description, so the exclusive taker sees `WouldBlock` just after the last holder drops. It was unverified, and this record's 30 baseline runs didn't reproduce it (it failed once in 0138's measurements).

**The section 1 finding makes it plausible:** the same inherited-descriptor window demonstrably occurs in this suite. `flock` locks belong to the open file description, so an inherited copy keeps a shared lock alive until the child execs or exits.

**The deterministic control, required before any fix** (Linux, the section 0 child): take the shared lock, fork the child (it inherits the lock's description), then drop every parent handle. Then:
- `try_lock` must be `WouldBlock` while the child is known to be alive (unreleased);
- after the child is released and reaped, `try_lock` must be `Ok`.

(A local prototype, since removed, already showed both outcomes. The control is written to the section 0 rules.)

**If the control fails, the hypothesis is wrong:** this record then states that, and leaves the test unchanged.

**The fix, only if the control demonstrates it:**
- **Only the final assertion waits:** the exclusive acquisition after both drops polls every 10 ms for up to 5 s, as production's `lock()` does (it retries `WouldBlock` every 10 ms). Any other error fails at once.
- The two `WouldBlock` assertions stay instant: an inherited copy can only lengthen a hold, so they can't be broken by it.
- **Production is unchanged:** it already waits through a transient hold.

**The measure:** it reproduced too rarely to measure a rate. The control is the evidence, and the 100 runs of section 1 report this test too.

## 3. `tests/dense.rs` without `generated`

`round_trips_and_every_binding_stays_usable` and `strings_are_checked_before_copying_and_the_frame_stays_usable` call the generated `select_`. They fail under `--no-default-features`, on 0140's tree as well (0141's evidence).

**The fix:** gate those two on `generated`, as the other tests that use generated bindings are gated. The file's other four tests use only hand-written bindings and stay ungated. **The proof:** under `--no-default-features`, the `dense` test binary reports exactly 4 tests run and passed.

## Gates

- **The project suite:** 100 whole runs with no failure of either test. Any other failure is reported as it is.
- **Polars:** `--no-default-features` all green; default and `test-support` unchanged.
- **Core:** unchanged.
- `cargo fmt`, `git diff --check`, and clippy on the touched files.

## Out of scope

- Changing the production handshake or lock code.
- Other flakes, which are reported if seen.
