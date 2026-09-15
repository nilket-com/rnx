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


## Replacement probes, 2026-09-15

Accepted revision `123bfd0` preceded this work. Sources, lockfile, two transport
runs, initial/expanded containment traces and source audit are in rnx-bench
**0637193**, `results/jupyter-0047-replacement/README.md` and the probe directories
it names. Kernel implementation has not started. The earlier sections retain
the initial candidates’ evidence; the decisions in the revised plan supersede
their pending-choice wording.

The zmq 0.10.0 / zmq-sys 0.12.0 graph builds bundled libzmq 4.3.4 through cc,
without CMake. It does not select the installed 4.3.5. An offline clean-target
release build took 7.02 seconds. Build logs, native version, linked libraries,
executable hashes and licence declarations are preserved; no finished kernel
notices, Windows build or kernel-size measurement is claimed.

With MAXMSGSIZE=1 MiB, both high-water marks=64 and linger=0, an oversized
64 MiB length header is refused by connection close with no sampled RSS growth.
But 16 MiB of legal incomplete parts grows RSS by 17,010,688 bytes, and 32 MiB
grows it by 34,025,472 bytes, in both runs. The application receives **zero parts**
until a final part arrives, then receives 4098 including the routing identity and
empty final part and rejects the multipart. The receive loop already reads one
part at a time and limits retained application data. This does not limit native
buffering before that loop. ypipe’s flush boundary and pipe’s message-count
updates explain the observation; source hashes/line references are preserved.
RSS was sampled externally at page resolution in a server limited to 512 MiB
address space. The fixture sends only 32 MiB; no exhaustion test is needed to
refute the proposed bound. No specific individual native allocation is inferred.

Other shell requests answered in under 1.5 ms in these observations, alongside
responsive control and heartbeat. An unfinished multipart did not lock the
application’s fair queue in this probe. Signed/empty-key exchanges and malformed
message refusals passed. A reading subscriber received output while another
stopped reading, with 20,000 successful PUB sends and no send errors. Successful
sends remain no proof of delivery; the receiving counts are observations only.
Joining socket-owner threads and dropping the context ended the server cleanly
in about 33–36 ms. The outstanding memory stop blocks further integration.

The Linux replacement discovers adopted descendants through all task child lists,
uses pidfds for signalling and a single reaper, and repeats until empty. It does
not consume fixture-supplied PIDs for cleanup. Cooperative worker shutdown, hard
worker death and stopped-worker termination all reaped the double-forked,
new-session fixture, in about 10–11 ms here. An initial confirmation attempt
exposed a partial readiness-file read; atomic publication corrected that fixture
before the saved confirmation. No descendants remained after the runs.

The C parent-death probe confirms direct-child death when armed and catches
parent-process death before setup by rechecking the pre-fork parent PID. It also
confirms a child dies when its spawning thread exits while the process lives.
If that thread exits before prctl, the PID check still matches and the child
survives: a concrete negative case for the separate shared-rnx plan. The harness
then kills/reaps it. This is not a production implementation, credential-change
gate or Windows result.

Rust formatting and Python syntax/undefined-name checks pass. Both probes ran
again; production rnx source and dependency graph are untouched, so its full
suites were not rerun. No replacement notebook adapter or browser capture exists.
