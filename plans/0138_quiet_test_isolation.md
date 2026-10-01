# rnx 0138: isolate the quiet capture tests from the test harness

Status: plan, a small maintenance record agreed with Codex after 0137 (which found the flake). Production code is unchanged.

**The flaw:** `tools/project/src/workflow/quiet.rs`'s capture tests call `Capture::begin()`, which redirects **the test process's own** fds 1 and 2 into a pipe drained into `dep.log`. Six tests do: the flooding child, parsed stdout, the mid-stream write failure, finalization, cancellation, and the lingering writer.
- **What protects them now:** they're serialized among themselves (`SERIAL`).
- **What doesn't:** libtest, and any other test running in parallel, still writes to fds 1 and 2. A line such as `test tests::project_run_budget_is_checked_before_any_launch` lands in the capture.
- **The effect:** `a_flooding_child_is_drained_and_reported` asserts the log's exact prefix, and failed in 3 of 8 full default-parallel runs, also with 0135's tests skipped (0137's evidence).
- **No lock can fix it:** the writer is the harness itself.

## 1. The boundary: one child process per capture test

**A helper, `isolated(name, body)`, used by all six tests:**
- **In the parent** (the selector environment variable `RNX_QUIET_CHILD` is unset): it re-executes the test binary (`std::env::current_exe()`) with `--exact <the test's path> --test-threads=1 --nocapture` and `RNX_QUIET_CHILD=<name>`, so the child runs that one test alone and nothing else in its process writes to its fds during the body.
- **In the child** (the variable equals the name): it runs the body in-process. A child never spawns: the variable is set, so `isolated` runs the body, and **recursive re-execution is impossible**.
- **A different value of the variable** (another test's name) means a misrouted child: it fails by name instead of running.

**Status propagation, a timeout and cleanup:**
- **Assertions:** a child's failed assertion exits non-zero, and the parent fails with the child's exit status and its captured stdout and stderr (bounded to the last 64 KiB).
- **The timeout:** 60 s per child, polled. On expiry the parent kills the child, reaps it, and fails by name ("quiet test child … timed out").
- **No orphans:** the parent always waits for the child, and the child's temporary directory is removed by the child, as now.

**Bounded output and a whole-helper deadline (amended after review):**
- **Draining:** the parent drains the child's stdout and stderr **continuously, while it runs,** each into a bounded tail (64 KiB per stream, the oldest bytes dropped). A flooding child never blocks on pipe capacity, and output never accumulates without bound.
- **The deadline:** the 60 s timeout covers the whole helper, draining included. After the child exits, the parent waits for both streams' end only until the deadline. If a descendant still holds a pipe, the helper doesn't join it indefinitely: it reports by name ("a descendant kept the child's output open").

**Parent-owned cleanup (amended after review):**
- **The child's group:** the child runs in **its own process group.**
- **Fixture groups:** the capture fixtures that start their own process groups (cancellation, the lingering writer) **register** each group's id in a parent-owned directory, passed to the child as `RNX_QUIET_DIR`. Their temporary directories live there too.
- **On every outcome** (success, a failed assertion, a panic, a timeout), the parent kills every registered group and the child's own group, reaps the child, and removes the parent-owned directory. A SIGKILLed child can't clean up after itself, so the parent does.
- **The report:** the helper returns the groups it killed, so a control can prove them gone.

**Production behaviour is unchanged:** `Capture`, `Sinks` and the drain aren't touched. Only `#[cfg(test)]` code changes.

**Audit:** every test in `quiet.rs` that calls `Capture::begin()` uses `isolated`. Tests that don't redirect (the tail bounds, the private log open) stay in-process. `SERIAL` is removed once no in-process capture remains, and the evidence lists each test and its form.

## 2. Controls

- **The old form leaks, shown deterministically** in an isolated reproducer: a child begins a capture in-process and attaches a log. A second thread in the same child writes a foreign marker to fd 1, synchronized by a channel, while the capture is open, as libtest would. The child then finishes and asserts that **the marker is in `dep.log`.** This is the mechanism the flake exhibits, without relying on a lucky failure.
- **The new form doesn't, shown deterministically:**
  - **The handshake:** the parent starts an isolated capture child, which signals "capturing" by creating a file in a shared temporary directory, then waits for a "written" file.
  - **The foreign write:** the parent, seeing "capturing", writes a foreign marker to its own fd 1, then creates "written".
  - **The assertion:** the child finishes, and the parent asserts that the child's `dep.log` **doesn't** contain the marker.
  - **A bound:** the handshake has a timeout.
- **The helper's lifecycle controls (from review):**
  - a child flooding both streams is drained into bounded tails without blocking;
  - a stray descendant in its own unregistered group, holding the child's stdout after the child exits, makes the helper report by name at a short deadline instead of hanging;
  - a child that starts a lingering fixture (registered), then panics, leaves no running fixture group and no open pipe: the helper returns promptly, and each killed group is shown gone (`kill(-pgid, 0)` is ESRCH).
- **The helper's own controls:**
  - a child whose body panics fails the parent with the child's status and output;
  - a child that hangs is killed at the timeout and reported by name (with a short timeout in the control);
  - a mismatched selector is refused by name;
  - a child never re-executes (an assertion in the helper).
- **Stability:** 20 consecutive full default-parallel runs of the project tool's suite, all passing, reported with the count.

## Gates

- **The project tool's suite** passes, with the six capture tests isolated and every control above.
- **20 consecutive default-parallel full runs** pass, with zero failures.
- **The other suites** (core, Polars, Candle) are unaffected; they're rerun for completeness.
- **`git diff`** touches only `quiet.rs`'s test module, plus the plan and evidence.

## Out of scope

- Changing `Capture`'s production behaviour.
- Other crates' tests. A similar audit elsewhere is reported if found, not done here.
