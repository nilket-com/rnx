# rnx HTTP/PostgreSQL example server

A standalone Linux application assembled through `rnx::server`, behind rnx's
`server-runtime` feature. This is its own Cargo workspace, lockfile and executable;
stock rnx does not acquire a listener, PostgreSQL driver or server command.
Record 0056 gate 4 extracts the measured server from record 0054. The real example
journey and stock-rnx comparison are still gates 5 and 6, not claimed here.

Build from the repository root:

```sh
cargo build --locked --release --manifest-path servers/http-postgres/Cargo.toml
```

In a database you have chosen for this example, create `audit(tag text NOT NULL)`.
Supply its connection string through `RNX_POOL_URL`. For example, with a local
PostgreSQL Unix socket and database `example` already set up:

```sh
export RNX_POOL_URL='host=/var/run/postgresql dbname=example'
servers/http-postgres/target/release/rnx-http-postgres \
  --program servers/http-postgres/examples/app.rn --bind 127.0.0.1:8080
curl --data 'hello' http://127.0.0.1:8080/healthy
curl --data '' 'http://127.0.0.1:8080/db?ok'
curl --data '' 'http://127.0.0.1:8080/db?fail'
```

`--program` is required; its entry file anchors module loading as in `rnx run`.
`--bind` defaults to `127.0.0.1:0` and accepts numeric loopback addresses only.
Without `--events FILE`, the bound address is printed at startup. An optional
regular-file event log receives structured ownership and timing events instead.
SIGINT or SIGTERM requests shutdown. Startup and shutdown failures exit 1.
The pool currently uses `NoTls`; this example is a local boundary, not a public
HTTP/TLS deployment or a configurable web framework.

## Request and response

The Rust application routes exactly `POST /healthy`, `/await`, `/cpu`, `/fail`
and `/db` to Rune's `main(request)`. Only `/db` obtains a transaction lease.
Paths are neither decoded nor normalised. Unknown paths return 404; a different
method on a known path returns 405. Request targets must be origin-form and
HTTP/1.1. Hyper tolerates bare-LF request lines and headers. There is no proxy
interpretation, keep-alive, HTTP/2, compression, upgrade or streaming response.

The argument is an object containing `method`, `path`, `query` (raw text, or
unit when absent), `headers` (lowercase names to vectors of Bytes), and `body`
(Bytes). Return exactly `#{status, headers, body}`: status is 200 through 599,
headers maps names to vectors of strings or Bytes, and body is a string or
Bytes. The host validates the shape, sizes and header syntax and owns framing
headers. Bodies on 204 and 304 refuse. Handler or response failure returns a
plain HTTP 500; the optional event log carries the diagnostic with source data.

`app::record(text).await?` inserts one bound text parameter into `audit(tag)`.
The database route needs a query string for the supplied example. `?fail`
inserts, then deliberately fails so the transaction rolls back. These are
bounded example operations, not unrestricted pooled SQL or transaction control.
`app::phase(text)` writes a phase event when event logging is enabled. The
existing one-query PostgreSQL adapter is a separate package and is unchanged.

## Ownership and bounds

One schema context compiles the program. Each handler preparation constructs a
fresh battery/extension context, invocation, lifecycle and HTTP owner on its
worker. Request construction and response reading use public Rune APIs only.
The immutable program is shared; concrete contexts, values and scope captures
are not. One whole instruction budget runs without resumption after exhaustion.
The Rune result becomes owned host data before explicit invocation close; close
precedes transaction completion. Cleanup failure forbids COMMIT and fails the
server. An active credit lasts through lease completion and retirement.

| Resource or clock | Limit |
| --- | --- |
| Executor workers / active handlers per worker | 2 / 4 |
| Central queued requests / connections | 16 / 32 |
| Request target | 8 KiB |
| Request and response headers | 16 KiB, 64 fields including host response fields |
| Request body / response body | 1 MiB each |
| Header / body read clock | 5 s each |
| Admitted request clock, including queue | 2 s |
| Response write clock | 1 s |
| Handler instruction budget | 10,000,000 |
| PostgreSQL connections per worker | 2 |
| Per-command PostgreSQL statement timeout | 800 ms |
| COMMIT/ROLLBACK acknowledgement / driver retirement | 1.2 s each |
| Whole-runtime drain after owners end | 1 s |
| Overall shutdown | 5 s |

The coordinator owns sockets and clocks on its own runtime. Awaiting work can
share a worker; synchronous CPU or native work occupies it. A timeout response
does not mean its handler stopped. Cancellation drops a run future between
polls; a CPU loop is bounded by its VM budget, not the network clock. Pool
drivers outlive handlers and are joined separately. No handler cleanup waits
for the whole runtime's task count.

There is exactly one COMMIT or ROLLBACK attempt and no retry. An ERROR response
to COMMIT is classified as rejected only for SQLSTATE class 23, `40001`, or
`40P01`. Other errors, lost replies and acknowledgement deadlines remain
ambiguous. Even a rejected lease is retired: the driver's error alone does not
certify consumption of ReadyForQuery. Retirement drops the client, awaits its
driver and replenishes with a new backend. A failed rollback also retires.

Shutdown stops admission, disposes queued requests without constructing
contexts, cancels executions between polls, finishes leases, closes pools and
joins workers. A clean event and exit zero require zero owned sockets, credits
and connection permits, with context builds equal retirements. Inherited
socket descriptors are identified and preserved separately. At the deadline,
remaining owners are reported and this executable hard-exits 1. The final
stderr write is bounded and nonblocking, so a full pipe cannot delay it. No
clean event is emitted on failure. The execution library never makes this
process-termination decision for an embedding host.

These clocks do not make trusted native code, event-file I/O, schema builders
or OS calls bounded. The standalone exit is containment, not an assertion
that PostgreSQL already observed a disconnect; the fixtures measure backend
closure independently. General child-process containment is not added here.

## Allocation accounting and reproducibility

This application disables rnx's optional `count-allocations` feature and
installs its own private allocator with the same live-request-byte accounting.
That preserves the prototype's memory measurements without exposing private
rnx counters. It is process-wide measurement, not RSS or a memory ceiling.
An application must not enable rnx's allocator alongside another one.

`test-support` is off by default and is local to this package. It adds the
blocking native and driver-retirement injections, long SQL and wire-response
fixtures. `examples/fixture.rn` deliberately needs that feature;
`examples/app.rn` does not. The ordinary build ignores the driver-stall and
small-send-buffer test environment variables. `RNX_HTTP_CPUS` optionally pins
the two workers and coordinator to three comma-separated Linux CPU indices
for measurement.

The acceptance drivers live in rnx-bench under `probes/server-extraction` and
reuse the existing wire, scheduling, transaction and shutdown assertions.
They start private clusters, never the system database. Both build modes are
formatted and clippy-checked. Only Linux execution is claimed.

`dependency-graph.json` records the resolved Cargo graph. Regenerate and check
licence texts with `python3 scripts/third-party-notices.py [--check]` from this
package. The notices include normal/build/procedural-macro dependencies on
Linux; `syntree 0.18.0`'s already-recorded missing text remains explicit.
