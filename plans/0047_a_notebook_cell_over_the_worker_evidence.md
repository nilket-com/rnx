# rnx 0047 evidence: both pre-kernel probes stop

Measured by Codex on nano/Linux on 2026-09-14. Plan commit 5d17a64 preceded the
probes. Sources/results/environment pins are in rnx-bench commit **1a1fc7a**:
`probes/jupyter-transport`, `probes/jupyter-containment`, and
`results/jupyter-0047/README.md`. Production rnx source remains the accepted
0046 implementation at 60e79fe. No kernel or user kernelspec was installed.

## Transport outcome

Signed exchanges with Python's actual jupyter_client/pyzmq pass, including
routing identities, extra fields and empty-key operation. Wrong signatures
receive no reply or dispatch. Malformed multipart and extra buffers are refused
by the fixture's application parser. A stalled subscriber hits the probe's
250 ms send timeout while independent control and heartbeat respond.

The bound fails before that parser: a 4 MiB message causes a 4,194,313-byte
allocation before application refusal. A nine-byte ZMTP long-frame header
announcing 64 MiB causes a **67,108,967-byte allocation request with zero body
bytes sent**. Both runs reproduce these allocation sizes. These are allocator
request sizes, not RSS or total memory. Tests are bounded and target only the
probe's loopback listener; they do not attempt exhaustion.

The locked zeromq codec takes its reserve size from the peer's length before
returning a message. It also buffers multipart frames internally. SocketOptions
exposes identity and connect timeout, not frame/multipart bounds. Application
size checks and signature verification happen too late to impose the planned
receive limit. The source audit records exact file hashes and paths.

Fifty PUB sends succeed without a subscriber. That supports the record's
qualification that local publishing success is not browser receipt. Cancellation-
safe reuse of a timed-out publisher and full kernel semantics remain unverified.

## Containment outcome

All child requests use timeout_ms=2000, not 90000. Normal deadline expiry ends
the ordinary child group; cooperative interruption does too (about 3 ms after
the signal in the confirmation run).

After a hard worker kill, the child is alive 2.5 seconds later. Sending SIGINT
to a stopped worker before killing it has the same result. A setsid descendant
also survives past the requested deadline after the worker cooperatively handles
an interrupt. The harness uses Linux subreaper mode to adopt and kill/reap every
owned survivor; both traces record completed cleanup. No Windows result is
inferred from these Linux cases.

host.rs enforces its Instant deadline in the worker's run_child_with loop.
There is no inherited OS timer continuing after that supervisor dies. The
90-second maximum argument therefore does not bound these survivors. The
fixture's own 30-second sleep is its own lifetime, not an rnx guarantee.
This is a stop condition, not grounds to relabel interrupt-first as containment.

## Environment, scope and next decisions

A dedicated gitignored Python 3.14 environment was created. The complete freeze
is committed, including jupyter_client 8.10.0, pyzmq 27.2.0, nbclient 0.11.0,
JupyterLab 4.6.3 and Playwright 1.62.0. Rust dependencies are locked, with
resolved features/declared licences captured. The clean-target release build,
with downloaded sources cached, took 8.12 s. Binary size and hash are preserved;
this is a transport fixture, not a measured kernel.

The screen gate is planned as headless Chromium via Playwright. Its pinned
browser revision is recorded but the browser has not been downloaded or run.
No notebook or screenshot acceptance is claimed. The notebook adapter will
ignore render_bounded rather than attach an always-true metadata flag; this
changes no worker protocol. The installer deliberately requires the jupyter CLI.

Both probes were rerun, formatting and Python undefined-name/unused-import checks
pass, and all fixture servers/workers/descendants were cleaned up. The only later
Python cleanup edit catches BrokenPipeError before reaping an already-dead test
server; the recorded successful shutdown path is unchanged. No production code
changed, so rnx's full suites were not rerun for this probe-only work.

Kernel implementation waits for review of two decisions: enforce receive bounds
below application deserialization, and define containment which survives hard
worker death and accounts for descendants leaving a process group (or explicitly
revise that promise). Neither change has been selected or implemented here.
