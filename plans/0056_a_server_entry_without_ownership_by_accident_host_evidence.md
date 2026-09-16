# rnx 0056: gate 2, the embedding host keeps control

Status: implemented, pending review, 2026-09-16. Gate 1 is accepted and pushed
at rnx `abc9bdd` and rnx-bench `99f5830`. Gates 3–6 remain open. This step adds
only tests, documentation and evidence. No production execution path, manifest,
lockfile, notices, kernel or adapter changed.

## Reproduce

On Linux, run `python3 probes/server-entry/host_boundary.py` in rnx-bench.
It runs the root test-only subprocess fixture, the feature's documentation tests
and a generated-doc surface check. Captures and source hash/toolchain conditions
are in `results/server-host-0056/`. The runner bounds each command to 300 seconds
and kills its process group on timeout. The first capture predates this step's
commit; the server source hash identifies the measured test implementation.

The test is in `src/server.rs`, behind test + test-support + server-runtime +
Linux. Private test controls are necessary to set the real existing CLI script
flag and report the existing config-open counter. No production accessor or
additional public API was added. Gate 1's separate external crate continues to
establish assembly without private access.

## Host policy child

The child installs its own SIGINT and SIGTERM handlers, then raises both signals
before compilation and after schema compilation, successful execution/close,
failed execution/close, and the remaining diagnostic/config checks. Ten actual
deliveries reach the caller's handler; this is not merely a comparison of handler
addresses. Installing rnx's CLI SIGINT handler would fail the next observation.

It sets `host::running_a_script()` before compiling. The server's process exit
still returns exactly `cannot exit: this is a server/embedding context` and the
child stays alive. A registration that reused the old global-flag exit would
instead terminate this child with status 7.

A valid installed config is supplied through RNX_CONFIG. After server compilation,
execution, failure and close, the actual-open counter reports zero. The child
then explicitly calls the config loader as a positive control and observes one.
An absent counter file or a config that happens not to run is not used as proof.

Runtime and compile failures return owned diagnostics. The parent requires empty
child stderr and permits only the Rust test-harness lines and the fixture's
explicit HOST status lines on stdout. No library diagnostic or prompt is allowed.
Temporary source, config and report files are removed by the parent guard.

## Native blocking child

A separate child compiles a handler that calls a blocking native function. The
schema must not execute the function. The worker constructs its own extensions,
invocation, values and current-thread runtime; its native call reports that it
has started and waits on a condition variable owned by the host.

After observing that start, the host waits 5.25 seconds, longer than the separate
standalone-server policy's five seconds. The worker is still unfinished and the
host is alive. The host then releases the condition variable, receives the
handler's original value 73 and joins the worker after explicit close.

This establishes a real still-owned native poll with host-controlled release.
It does not claim safe native cancellation, prove the absence of every possible
timer by finite waiting, or implement the standalone hard-exit policy. Source
review establishes that the execution API creates no threads or kill timer and
has no process-exit call. The binary's deadline policy remains a later gate.

## Public surface and validation

Generated `rnx::server` documentation contains exactly Program, Invocation and
Failure, with no public free function, enum, trait, alias, constant or static.
Four compile-fail documentation tests refuse the private context constructor and
the ownership fields of all three types. The existing fifth rejects sending an
Invocation across threads. All five pass.

The focused subprocess fixture passes with three expected HOST lines and no
child diagnostics. Formatting and diff checks pass. The full suite with
`server-runtime,test-support` passes 424 tests, zero failures, including the
five compile-fail checks. Its raw log is `suite.txt` beside the bench captures.
The production server implementation is byte-identical to gate 1 after removing
the added documentation; all new executable code is behind the test cfg. Linux only: no Windows signal
or native-blocking execution is claimed. The bench README now explicitly forbids
running default and test-support configurations concurrently in one target dir.
