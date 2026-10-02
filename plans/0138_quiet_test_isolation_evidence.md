# rnx 0138 evidence: the quiet capture tests run in isolated children

**The result:** every test in `tools/project/src/workflow/quiet.rs` that redirects the test process's own fds 1 and 2 now runs alone in a re-executed child, so nothing else writes to its descriptors.
- **The flake is fixed:** 0137 measured `a_flooding_child_is_drained_and_reported` failing in 3 of 8 runs, and 5 of 20 at 0137's commit.
  - **Since:** across this record's final two 20-run measurements, **no `quiet` test failed.**
  - **The full suite still isn't always green:** two separate flakes elsewhere in the suite, outside this record, failed in some runs. The capability test's was measured at baseline; the cache-lock test's has no measured baseline. Both are disclosed in section 4, with that uncertainty.
- **Production code is unchanged:** `Capture`, `Sinks` and the drain aren't touched. Only `#[cfg(test)]` code changed.

## 1. The audit (each test and its form)

| test | `Capture::begin` | form |
|---|---|---|
| `a_flooding_child_is_drained_and_reported` | yes | **isolated** |
| `parsed_stdout_is_untouched_while_stderr_is_captured` | yes | **isolated** |
| `a_mid_stream_write_failure_keeps_the_tail_and_reports_no_usable_log` | yes | **isolated** |
| `finalization_never_follows_a_replaced_path` | yes | **isolated** |
| `cancellation_with_full_pipes_is_prompt_and_reaped` | yes | **isolated**; its fixture group is registered |
| `a_lingering_writer_cannot_hang_finalization` | yes | **isolated**; its fixture group is registered |
| `the_tail_keeps_the_last_lines_within_both_bounds` | no | in-process |
| `the_log_is_opened_privately_and_planted_paths_refuse` | no | in-process |

`SERIAL`, the in-process lock, is removed: no capture runs in-process any more.

## 2. The mechanism (`quiet.rs`, test module)

- **Selection:** `isolated(name, body)`.
  - **In the parent** (`RNX_QUIET_CHILD` unset), it calls `run_isolated(name, name, 60 s)` and panics with the report on failure.
  - **In the child** (the variable equals the name), it runs the body.
  - **A different value** panics as "a misrouted quiet test child".
  - The helper's own fixtures use `child_only(name, body)`, which is a no-op in an ordinary run.
- **The child:** `run_isolated` re-executes `current_exe()` with `--exact workflow::quiet::tests::<name> --test-threads=1 --nocapture` and `RNX_QUIET_CHILD=<selector>`.
  - **Its environment:** `RNX_QUIET_DIR` names a parent-owned directory; stdin is null and stdout and stderr are piped.
  - **Its process group:** its own (`process_group(0)`).
  - **No recursion:** `run_isolated` asserts that it isn't itself running in a child.
- **Bounded output:** two threads drain the child's stdout and stderr continuously while it runs, each into a tail of at most 64 KiB (the oldest bytes dropped).
- **One deadline for the whole helper:**
  - the child's exit is polled every 20 ms until the deadline (60 s);
  - after the exit, both streams' end is awaited only for the deadline's remainder;
  - a stream still open then (a descendant holding the pipe) is reported by name and not joined.
- **Parent-owned cleanup, on every outcome:**
  - **Registration:** fixtures that start their own process group register the group's id in `RNX_QUIET_DIR/groups` (`register(pgid)`); their temporary directories live in `RNX_QUIET_DIR`, through `temp`.
  - **After the child exits or times out,** the parent `SIGKILL`s every registered group and the child's own group, reaps the child, and removes the directory.
  - **The report** lists the killed groups.
- **One child at a time:** a parent-side lock (`ONE_CHILD`) runs one isolated child at a time. It protects nothing about descriptors (each child has its own). It bounds the suite's extra load: an earlier version ran up to 15 re-executed test binaries at once, and that load showed up in a timing-sensitive test elsewhere (section 4).
- **The report:** `Report { result, elapsed, killed }`.
  - `result` is the two tails on success, or a named failure: the child's status with both tails, a timeout, or a held pipe.
  - `elapsed` is measured from after the lock is held, so waiting behind other children isn't counted as the child's time.
- **An admission hook:** `run_isolated_with(…, admitted, collect)` runs `admitted` once the caller holds the slot and the parent-owned directory exists, just before the spawn. Anything that observes the child, and its own deadline, starts there (review round 1).
- **A collection hook:** `collect` reads what the child left in its parent-owned directory after it exits, still under the slot and before the directory is removed, and returns it by value. Nothing a child leaves is shared between invocations or read after the slot is released (review round 2).

## 3. Controls (all in the project suite)

| control | what it proves |
|---|---|
| `the_in_process_form_captures_foreign_output` | **the old form leaks, deterministically:** inside an isolated child, a capture is open in-process while a second thread writes a marker to fd 1 (channel-synchronized), and the marker is asserted **in** `dep.log` |
| `an_isolated_capture_does_not_capture_the_parents_output` | **the new form doesn't:** an isolated child signals "capturing" (a file in the parent-owned directory) and waits; the parent writes a marker to its own fd 1 and signals "written"; the child's `dep.log`, read by the collection hook from that invocation's own directory, starts with its header and **doesn't** contain the marker |
| `queued_admission_does_not_start_the_handshake_clock` | another thread holds the slot for 3 s while the handshake's bound is 2 s: it passes and did queue (review round 1) |
| `overlapping_consumers_each_keep_their_own_log` | one invocation pauses 3 s after its run returns while a second runs entirely within that pause; the first's log is still its own (review round 2) |
| `the_helper_drains_a_flooding_child_into_bounded_tails` | a child writing 2 MiB to each stream (past pipe capacity and the tail) finishes without blocking, and each tail is at most 64 KiB |
| `a_failing_child_fails_the_parent_with_its_output` | a panicking child fails the parent, with its status and its panic text in the report |
| `a_hanging_child_is_killed_at_the_deadline` | a 30 s sleeper under a 2 s deadline is reported as timed out within 10 s, and its group is gone |
| `a_descendant_holding_the_output_cannot_hang_the_helper` | a stray `sleep` in its own **unregistered** group holds the child's stdout after the child exits 0: the helper reports "a descendant kept the child's output open" at a 3 s deadline, within 10 s; the control then kills the stray (whose pid it learned out of band) and shows it gone |
| `a_failed_child_leaves_no_fixture_group_and_no_open_pipe` | a child starts a lingering fixture, registers it, and panics: the helper returns promptly (within 10 s; killing the fixture closes its pipe); both killed groups (fixture and child) are gone (`kill(-pgid, 0)` is ESRCH); the parent-owned directory is removed |
| `a_misrouted_child_is_refused` | a child selected for another name fails as "misrouted" |
| `a_child_never_re_executes` | calling the helper inside a child fails as "never re-executes" |

## 4. Stability, as measured (each line is 20 consecutive full default-parallel runs of the project suite)

| state | runs fully passing | the flood test | the capability handshake test |
|---|---:|---:|---:|
| **0137's commit, before this record** (`4a364a8`) | (not counted) | **failed 5** | failed 1 |
| first version: children concurrent, 8 MiB flood | 19 | 0 | failed 1 |
| children serialized, controls timed from before the lock | 14 | 0 | failed 2 |
| children serialized, controls timed from after the lock, 2 MiB flood | 20 | 0 | 0 |
| after review round 1: the non-leak handshake starts at admission, plus the queued-admission control | 18 | 0 | failed 2 |
| **after review round 2:** the child's log is collected under the slot, plus the overlap control | 18 | **0** | failed 2 (and a third, separate test failed once, below) |

- **The quiet tests:** the flood test fails 5 in 20 before this record. **No quiet test failed in the final two measurements (40 runs).** The earlier rows' failures were my own controls (next bullet), not the isolated capture tests.
- **The intermediate failures were my own controls:** the hang, stray-descendant and lingering-fixture controls timed themselves from before `run_isolated`. Under the one-child lock that included queueing behind other children. They now use the helper's `elapsed`.
- **Review round 2 found a shared file:** two callers of the non-leak check had their child save its log to the same parent-pid-keyed file, read after the slot was released. Codex reproduced an ENOENT deterministically by pausing one caller. The fix is the collection hook above, with no shared file, and the new overlap control.
- **Review round 1 found one more clock outside the slot,** so the earlier claim that every control was timed after the lock was wrong. The non-leak control's writer started its 20 s handshake timer before the slot was held. Codex reproduced the failure deterministically: holding the slot for 21 s made the test fail after 41 s.
  - **The fix:** the writer now starts in the admission hook.
  - **The new control, `queued_admission_does_not_start_the_handshake_clock`:** another thread holds the slot for 3 s while the handshake's bound is 2 s. It passes (and waited at least 3 s, so it did queue). Under the old structure it fails by construction.
  - **The audit:** every timed control now times from admission. The hang, stray and lingering controls use `elapsed`; the non-leak handshake uses the hook.
- **`capability_refuses_bad_replies_and_retires_timed_out_child` (`src/tests.rs`)** is a separate, pre-existing flake. It asserts that a subprocess handshake succeeds within the production 1 s deadline, and its own comment notes host load. It failed 1 in 20 at 0137's commit, then 1, 2, 0 and 2 in this record's runs.
  - **Inconclusive:** samples of 20 can't separate those rates. Nor do they show whether one isolated child at a time adds measurable load to it.
  - **What this record did:** bounded its own load (the one-child lock and the smaller flood), and it doesn't change that test.
  - **The follow-up:** the test's fix, a deadline that doesn't depend on host load (or a refusal assertion that accepts either outcome, as its own comment already does for the 4,085-byte case), is reported as a separate maintenance follow-up.
- **`cache_entry::shared_build_tests::builders_share_the_coordination_lock_and_exclude_removal`** failed once, in the last measurement. It takes `flock` locks, then asserts that a third handle is excluded until both are dropped.
  - **A hypothesis, unverified:** between a fork and its exec, a child spawned by any concurrent test briefly holds copies of every open file description, close-on-exec ones included. If one is spawned just as the locks are dropped, the exclusion check sees `WouldBlock`.
  - **What's known:** many tests in this suite spawn processes, so this would be pre-existing. The baseline row only counted the two tests named above, so whether it failed before this record is unknown, as is whether 0138's isolated children make it likelier.
  - **The follow-up:** reported for the same separate maintenance follow-up, not addressed here.

## Gates

- **The project suite:** 102 passed and 2 ignored. That includes the `quiet` module's **27 tests:** the six isolated capture tests, the two in-process ones, and 19 helper fixtures and controls. A filter on "quiet" also matches one test outside the module, `transition`'s `…never_quiet_consent`.
- **20 consecutive default-parallel full runs:** no `quiet` test failed (section 4). The failures were the separate flakes disclosed there.
- **Clippy** on the project tool's tests is clean. The two fixtures that leave a process for the parent to kill carry `#[allow(clippy::zombie_processes)]` with the reason.
- **Core:** 392 passed. Polars and Candle have no changes and aren't rerun.
- **The diff** touches only `quiet.rs`'s test module, plus the plan and this evidence.
