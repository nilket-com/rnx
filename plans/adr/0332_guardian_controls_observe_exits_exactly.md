# ADR 0332: Guardian controls observe exits exactly

Status: Proposed

Base: published 8e2370be5fb3e579a95e435eabcd03ac82dbcdda (ADR 0331).

## Problem

The `dbg!` candidate's complete final failed one final-controls test,
`an_externally_stopped_guardian_that_hangs_is_killed_within_its_deadline`
(`tools/final/tests/guardian.rs:455`), with "the hung guardian's own
child survived". `tools/final` was unchanged from the published base.

A bounded fixture-only capture reproduced it on attempt 203 of the
guardian test binary, under eight CPU load workers. The product had done
its job. `drain_group` selected the exact child, sent `SIGKILL`
successfully and waited on its pidfd until `POLLIN`. An independently
pinned pidfd was already ready before the assertion. The test's
`exited()` reads `/proc` twice: the first read saw the child as a zombie
(`Z`), the second as dead (`X`), and the zombie-only check returned
false. Reading `/proc` state is not a reliable way to observe exit.

The same capture hit a second failure, on attempt 100: `ETXTBSY` while
executing the fixture shell script the test had just written. Another
thread's fork can briefly inherit the write descriptor.

Review of the drain also found silent paths that the captured failure
did not exercise. `drain_group` skips failed pidfd and stat reads and
ignores the signal result, and `wait_exit` treats any poll event as
exit.

## Decision

1. Guardian controls observe exit through a pidfd pinned at fixture
   readiness, checked for exit readiness with a zero timeout as soon as
   the wait returns. `/proc` state reads are not used to assert exit.
   For the hung guardian and its child, the independent descriptors
   must first show both processes alive. Preserve the deadlines and the
   stronger reaping assertions wherever the guardian owns and reaps
   descendants.
   For a recorded stage that can exit before the test opens its pidfd,
   accept proven disappearance or wait on the verified recorded identity.
2. The drain reports real failures. A member that can't be identified
   for a reason other than proven disappearance, a signal failure other
   than the target being gone, and a wait failure each surface as an
   error identifying the operation, requested group and observed identity
   fields. Include errno for a failed syscall and revents for a returned
   event. Mark unavailable fields as such. Preserve the group and start
   checks and signal through the pinned descriptor. Finish cleanup of
   other identifiable members and both recorded groups, and reap the
   guardian before returning a drain error. Never wait for exit without a
   deadline after a failed kill of a still-live member.
   Both `wait_exit` and the pidfd branch of `wait_either` require
   `POLLIN` for exit. `POLLIN|POLLHUP` counts as exit. A timeout and an
   unexpected event remain distinct. A proven disappearance stays normal.
3. Run the hung-guardian fixture from an existing executable, with the
   per-test child marker relative to the private working directory.
   The fixture must still ignore the stop signals and leave a live group
   member for the controller to drain.
4. Scope: `tools/final` only. No engine, product or gate-band change.

## Controls

1. As one-time acceptance evidence, run the candidate's complete guardian
   test binary at least 1,000 times with the capture's test concurrency
   and eight CPU workers. Stop on the first failure and retain its output.
2. The live-child negative control still fails when a member is
   deliberately left alive.
3. Injected identification, signal and wait failures each surface with
   the available identity. A second eligible member is still drained
   after a member error. A disappearance during the drain stays normal.
   Cover a live pidfd, exit with HUP, and an event lacking POLLIN.
4. Exercise the actual hung-guardian fixture from a private working
   directory with spaces. Establish that the executable exists before
   the test and is never opened for writing by the fixture.
5. Finals: targeted pre-sign checks under the final's real recipe,
   including the full final-controls command, then one complete final
   on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
