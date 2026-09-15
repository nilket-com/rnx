# rnx Jupyter kernel

Implementation of record 0047 now installs and runs a Linux notebook kernel over
the accepted 0046 worker. The Linux JupyterLab acceptance path is implemented;
non-Linux supervision remains unfinished. This package is not a claim that the
record's Windows and macOS/BSD gates are complete.

Ordinary `cargo build` in the parent directory does not build this package or
resolve its dependencies. This package has its own workspace boundary, lockfile
and notices, and remains unpublished at version 0.0.0.

The transport and message layer contains:

- `transport`: the accepted 0048 server-side ZMTP component. Five loopback TCP
  endpoints; 3.0 framing with bounded PING/PONG, not full 3.1. Receive and send
  credits include unfinished messages and writes. Each application callback
  borrows its message, so this layer has no added inbound queue. An application
  must reserve its own bytes before copying a request into an execution queue.
- `wire`: Jupyter 5.4 JSON framing and HMAC-SHA256. Authentication precedes JSON
  parsing. Replies retain the request's routing and original header bytes;
  no mutable "latest request" determines attribution. Serialization has a byte
  cap before allocation grows beyond the encoded budget. The kernel supplies the current UTC timestamp through jiff; the acceptance
  fixture alone uses a fixed timestamp.
- `connection`: the local connection-file subset from 0047. A regular file of
  at most 64 KiB, numeric loopback TCP address, five distinct nonzero ports,
  and HMAC-SHA256 (an empty key disables authentication). Unix opens use
  `O_NONBLOCK` so the subsequent handle check refuses a FIFO without waiting.

On Linux, build and install into the Jupyter CLI's user registry:

```sh
cargo build --release --locked
./target/release/rnx-jupyter install --rnx /absolute/path/to/rnx
```

Then choose **Rune (rnx)** in JupyterLab. The kernelspec records absolute executable
paths, so paths containing spaces need no shell wrapper. Keep both binaries at
those paths, or install again with `--replace`. An existing rnx spec is refused
without that flag. Jupyter's CLI currently always replaces, so rnx preflights both
its registry listing and its reported user destination, including malformed specs.
This check is not atomic against another concurrent installer. The Jupyter CLI
must be on PATH; a Desktop-only setup without it gets installation guidance.
Installation is explicit and launches trusted Jupyter commands synchronously;
it does not inherit the serving kernel's five-second shutdown budget.

A Jupyter manager launches the serving command directly:

```sh
./target/release/rnx-jupyter --connection-file /absolute/connection.json --rnx /absolute/rnx
```

The launcher inherits the notebook working directory and environment. It starts
one worker with null stdin and private control pipes. The server requires Linux
subreaper, `/proc` child discovery and pidfd signalling support; it refuses
startup when these are unavailable. Other platforms explicitly refuse this
interim executable. Windows type checking below covers the portable component
and that refusal, not Windows worker containment.

Execution admits 64 queued requests and 4 MiB of serialized payload, with the
active request retaining its byte credit. History is bounded at 16 MiB/10,000
entries; source-origin mappings at 10,000. The transport's receive credits stay
held until admission finishes. One owner publishes output under the immutable
request header. Control and heartbeat remain independent of the worker.

Shell kernel-info waits behind active execution while retaining the transport's
receive credit. Its busy/idle pair cannot leave a running notebook falsely idle.
Control kernel-info stays immediate and suppresses that pair during execution.
At startup, shell info waits up to two seconds for an observed status subscriber;
this narrows the PUB startup race but does not guarantee frontend delivery.
A shell-only caller still receives its reply after the bounded wait. The input
channel is deliberate: execute on control and interrupt on shell are ignored.
IOPub errors carry only notebook error fields, separate from execute replies.

After restart, the next execution confirms fresh state; an unsolicited idle is
not a readiness requirement. Reloading while a cell runs does not recreate the
old page's output future. Interrupt remains available, and subsequent cells use
the same live kernel. Completion, inspection, history queries and rich output
remain deferred; unsupported optional requests may be ignored.

Each stream retains at most 2 MiB per operation while continuing to drain. UTF-8
replacement is marked in message metadata. Short flushed lines can arrive during
execution; unflushed partial lines wait for the worker's barrier. Late output has
an empty parent header and a separate 2 MiB lifetime allowance per stream. The
worker is acknowledged only after both barriers, its settlement and all retained
output/result/error have reached bounded publication admission (or intentional
silent suppression). This is not a browser delivery receipt.

Interruption preserves rnx's async CPU-loop limitation. Shutdown uses one
five-second budget, interrupts active execution, then kills/reaps the worker and
adopted descendants. No shared rnx spawn behavior changes. A hard-killed kernel
cannot run Linux cleanup; escaped descendants can survive. OS-uninterruptible
work may outlast the cleanup deadline, which is reported as failure.

`bash probes/jupyter-supervision/run.sh` from rnx-bench runs real-client, queue,
byte-stream, blocked-handoff, Linux descendant and private nbclient notebook
fixtures twice, plus both original transport fixtures. It uses the existing
pinned Python environment and installs no user kernelspec. Review notes are not
part of either repository's history.

To check this component:

```sh
cargo test --locked
cargo test --locked --features transport-probe
cargo fmt --check
python3 scripts/third-party-notices.py --check
```

The feature-gated `transport-probe` example is an acceptance adapter, **not a
kernel executable**. It carries the test key, fixed test timestamp, publication
commands and allocation telemetry needed to reuse 0048's raw wire fixtures.
None is installed as a notebook command or enabled by default. From the adjacent
rnx-bench repository, `bash probes/jupyter-integration/transport.sh` rebuilds it
and runs both fixtures twice using the pinned Python environment.

The limits permit refusal, not fairness. Eight local connectors can occupy an
endpoint and exclude another client. A subscriber exceeding its budget is
closed; PUB acceptance is not a frontend delivery receipt. Payload credit is
not total process RSS. Windows type checking does not establish Windows
execution or worker job containment. These are carried into the kernel record,
not changed by moving the transport into this package.

The transport came from rnx-bench `00fc828`, `probes/jupyter-zmtp`, measured and
accepted under record 0048. The original failed candidates and all prototype
measurements remain there. Changes at extraction are described in record 0047's
evidence; review notes remain gitignored.

The browser fixture is `probes/jupyter-notebook/run.sh` in rnx-bench. It installs
only into temporary directories and records real JupyterLab screenshots and
saved notebooks. Its separate Python environment pins Tornado 6.5.8: the original
6.5.9/Jupyter Server 2.21.0 combination failed serving static assets before the
notebook loaded. This is a test-environment compatibility pin, not a Python or
Tornado dependency of rnx-jupyter. See the evidence for versions and limits.
