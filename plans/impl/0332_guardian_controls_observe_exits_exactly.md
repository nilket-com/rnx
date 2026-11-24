# Implementation plan 0332: Guardian controls observe exits exactly

Status: Implemented

Paired ADR: ../adr/0332_guardian_controls_observe_exits_exactly.md.

1. Use published 8e2370be and the guardian capture receipts to
   distinguish exit assertions from reaping assertions in the guardian
   suite and identify the write-then-exec path from attempt 100.
2. Pin the hung guardian and child before raising stop. Check both are
   live and then check immediate exit readiness independently of the
   production wait helper. Preserve explicit reaping checks.
3. Surface the drain's identification, signal and wait failures with
   available member identity. Keep PID reuse checks, finish cleanup on
   error, and apply the exit-event rule in both pidfd wait paths.
4. Replace the generated hung-guardian executable with an existing
   executable fixture and preserve the ready-child handshake.
5. Add the negative, event-mask, injected-failure and fixture controls.
   Run the one-time soak against the candidate binary and retain its
   receipt.
6. fmt, Clippy, preflight, the full final-controls command and the
   targeted checks under the final's real recipe. Sign. Source review,
   one complete final, Results.

## Results

The hung-guardian test pins both processes before raising stop. Both
descriptors were live at readiness and readable at return in 1,000
consecutive runs of the full nine-test guardian binary with eight CPU
load workers. A separate control moved the child out of the drained
group. The same assertion detected the surviving child at return.

The fixture ran from a working directory containing a space. The test
executes a tracked shell script and writes only the child marker there.
Controls covered a failed identification, a refused signal, a failed
wait, and a vanished target. They verified that the drain still signaled
and waited for the remaining eligible members. A hangup alone produced
an error and a readable descriptor with a hangup counted as exited.

The format and Clippy recipes passed. The first pre-sign invocation of
final-controls omitted the Valgrind installation path and stopped at
the image-observer test. On the same source with the path set, the full
final-controls recipe passed with 308 tests passed and the existing
receipt round-trip test ignored.

The first complete final on 6aca3ace passed all 107 stages and both
fixed-route bands but exceeded the wall budget, with 60.3 minutes of
accepted run and 61.0 minutes end to end. The previous final in the
same cache had built a Rune change from a branch outside this
candidate's history. Cargo build time across stages rose from 2,682
to 3,863 seconds against the 0331 final, while the fixed-route stage
took 674 seconds against 677. The second complete final on the same
commit, with the cache matching its source, passed all 107 stages,
with 36.1 minutes of accepted run and 36.4 minutes end to end. All 32
per-record comparisons matched the base exactly and all 32 cumulative
comparisons passed. There were no memory.high, memory.max or OOM
events, and the host was in the performance profile. Neither final
added owned entries to /tmp or /var/tmp.
