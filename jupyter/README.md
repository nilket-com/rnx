# rnx Jupyter kernel

Implementation of record 0047 is in progress. This independent package currently
contains the owned transport and message layer. It does **not yet install or run
a notebook kernel**: worker supervision, execution scheduling, kernelspec
installation and JupyterLab acceptance are the remaining implementation.

Ordinary `cargo build` in the parent directory does not build this package or
resolve its dependencies. This package has its own workspace boundary, lockfile
and notices, and remains unpublished at version 0.0.0.

The first implementation contains:

- `transport`: the accepted 0048 server-side ZMTP component. Five loopback TCP
  endpoints; 3.0 framing with bounded PING/PONG, not full 3.1. Receive and send
  credits include unfinished messages and writes. Each application callback
  borrows its message, so this layer has no added inbound queue. An application
  must reserve its own bytes before copying a request into an execution queue.
- `wire`: Jupyter 5.4 JSON framing and HMAC-SHA256. Authentication precedes JSON
  parsing. Replies retain the request's routing and original header bytes;
  no mutable "latest request" determines attribution. Serialization has a byte
  cap before allocation grows beyond the encoded budget. The caller supplies
  the message timestamp; the fixture's fixed timestamp is not a kernel clock.
- `connection`: the local connection-file subset from 0047. A regular file of
  at most 64 KiB, numeric loopback TCP address, five distinct nonzero ports,
  and HMAC-SHA256 (an empty key disables authentication). Unix opens use
  `O_NONBLOCK` so the subsequent handle check refuses a FIFO without waiting.

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
