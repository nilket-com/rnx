# Implementation plan 0253: a final releases the build output it owns

Status: Implemented

Paired ADR: ../adr/0253_finals_release_their_build_output.md.

1. Pin 4e4c980a. Read tools/verify.py and its binding and host-check
   order, paired_build.py and its cleanup journal, miri_exact.py and the
   preparation cache, child_result.run, and accept.py.
2. Implement the ownership record, the ordered removal for the three
   producers in scope, and the duration field in the common stage runner.
   Decide the baseline Clippy exports by the rule in the ADR.
3. Add the controls of the ADR.
4. Run the real cross and Miri checks, the required stages, preflight and
   the commit checker. Bring the packet with the disk use of the final's
   own directories before and after, the per-stage durations, the selected
   stages and the list of what is kept and why. The candidate is frozen
   once the packet is sent.

Results are added here by the implementer.

## Implementation and validation boundary

The three producers use explicit invocation ownership. Cross preserves the
controller result, completes host-check, releases its private Cargo target,
and only then binds the cleanup-aware terminal result. Paired builds retain
verified copied binaries; both failed and successful private caches release.
Exact Miri releases semantic copies while preparation remains hash-bound and
reusable. Baseline Clippy exports remain: their target ownership has not been
resolved. Shared suite/Clippy targets, generated projects, test caches and
external toolchains remain untouched.

The common supervised wait and owned producers share one helper. It forwards
SIGTERM on timeout/interruption, gives the child a bounded drain interval,
then kills remaining process-group members and reaps the child. Signal
handlers are restored on all exits; non-main-thread use refuses explicitly.
Existing common-runner callers are sequential/main-thread callers. Miri's
backend remains unchanged; outer cancellation belongs to its existing cgroup.
The common stage receipt now records elapsed and cleanup seconds; finalization
includes the per-stage duration table. Durations are observations, not gates.

Controls cover direct SIGTERM with child/grandchild, timeout with a failing
terminal receipt, an ignoring child, handler restoration, off-thread refusal,
cleanup pass/failure, evidence preservation, path refusal and cache reuse.
The initial timeout control was rejected by the command classifier before
launch; the synthetic control now declares its leaf contract explicitly.
Controller controls passed (244 tests) before signing. Real producer and final
receipts are reported in the publication packet after signed validation.

Limits: SIGKILL can leave build output; no later sweep is added. Missing retained
evidence, uncertain ownership or removal failure fails closed and can leave
output in place. Direct replay of cross host-check requires rebuilding its
removed compiler artifact; terminal receipt revalidation retains its evidence.

Signed validation exposed a termination race: sending SIGKILL and reaping the
leader did not establish that a grandchild had exited. The wait helper now
uses Linux pidfds to block on exit of remaining killed process-group members
before release. This is exit notification, not progress polling. The corrected
controller controls passed; the failed final remains recorded. An earlier
launch refused before stages because its environment omitted Valgrind; the
subsequent invocation supplies the explicit installation root.
