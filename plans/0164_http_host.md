# rnx 0164: a product HTTP host

**Status:** plan. **Direction:** a generic `rnx serve PROGRAM.rn`, using the reusable slots proved in 0163. A real site in its own repository is the first user. Nothing in this repository names that project or contains its content. Codex plans/implements; Claude reviews before implementation and before push.

## 1. Cut and user contract

This record delivers the host, not a framework. The program is a normal Rune file with `pub fn main(request)` or `pub async fn main(request)`. It can import other Rune files. The request is `#{method, path, query, headers, body}`; the response is exactly `#{status, headers, body}`. No Rust routing or host code is needed in an application.

**Route tables and web helpers are deferred.** The measured skeleton already computes its home page, dynamic greeting, CSS, contact form, HEAD/405/allow and 404 through this contract. Porting it unchanged tests the product host independently of new routing/escaping semantics. Removing the per-application Rust host is the first usability gain. The remaining manual routing and helpers, and their line counts, stay explicit follow-up findings; this record does not claim the final Flask-level terseness target is met.

The request method is uppercase; path and optional query are raw URI text, without decoding. Missing query is Rune unit. Header keys are lowercase, each value is a vector of Bytes, retaining duplicates; the body is Bytes. The handler controls routing, decoding and content types. No header, query or body is trusted HTML. Response status is an integer 200..599; body is String or Bytes; header values are vectors of String or Bytes. All response data is converted to owned Rust data and Rune values dropped before closing the invocation. Transport strips a HEAD body while retaining the equivalent response length. No automatic GET dispatch, 405, redirect or content-type inference is added.

## 2. Packaging and CLI

An optional `http-server` feature enables the host and `server-runtime`. It is included in the stock default build, but remains absent from no-default generated/embedding builds unless requested. The existing `server-runtime` feature alone still adds no HTTP server dependencies. Axum 0.8.9 is pinned, with HTTP/1-only hyper/hyper-util support and explicit Tokio features. Existing libraries are reused; no compiler or upstream fork.

Dispatch serve before constructing the ordinary CLI context or installing its signal handler. `rnx serve --help` and invalid options require no compilation. A build without the feature refuses serve by name. Existing commands and embedding API stay unchanged.

```
rnx serve [--bind IP:PORT] [--workers N] [--budget N]
          [--request-timeout-ms N] [--grace-ms N] [--log requests|off] PROGRAM.rn
```

Options precede the one program path; duplicates, unknown options, missing values and extra paths are errors before opening the program. Defaults: 127.0.0.1:3000; 2 workers; the runner's existing 2,000,000-instruction budget; request timeout 30,000 ms; shutdown grace 5,000 ms; request logging on. Workers 1..16, budgets 1..LARGEST_BUDGET, request/grace durations 1..300,000 ms. SocketAddr parsing accepts IP literals only, IPv4/IPv6, including port 0 for controls. Non-loopback binding requires an explicit --bind and is documented for a reverse proxy; no TLS or proxy-header trust is added.

Startup compiles once, constructs all worker slots successfully, then announces the actual bound address. A failure closes constructed owners, joins workers and exits nonzero with a source-located diagnostic where available. Startup readiness is not inferred from a TCP port opening before workers are ready.

## 3. HTTP and bounds

Axum handles requests through a generic fallback. The HTTP/1 connection host uses Hyper's public server builder to set explicit header limits and read timeout, rather than relying on the unconfigurable axum::serve defaults. Keep-alive is on. HTTP/2, TLS and upgrades are not enabled.

Limits are frozen for this cut: 256 accepted connections; 64 request headers and 16 KiB parsed-header buffer; raw target at most 8 KiB; request and response bodies at most 1 MiB; response headers at most 64 values and 16 KiB total. Active body readers have a bounded admission permit (workers × 20); no proportional body allocation before admission. A known excessive content-length is refused before reading; streamed/chunked bodies use limit+1 refusal. Body reading has a 5-second timeout. Connection admission bounds pre-request memory; excess connections wait at the listener/backlog, while a full request/worker queue gets 503.

Malformed requests are rejected by the parser; body too large is 413, body timeout 408, full admission/queue 503, request deadline 504, handler/response/cleanup error 500. Invalid response schema/types, header syntax, reserved hop-by-hop or transport framing headers, excessive headers/body, and a nonempty 204/304 body are named internal failures, never panics or partial replies. Host computes content-length; script cannot supply content-length, transfer-encoding, connection, keep-alive, upgrade, trailer, TE or proxy connection headers. Known transport-generated fields are handled consistently with the 0162 wire specification.

Request timeout starts before body reading and includes queueing. An expired queued job is discarded before VM execution. The worker races execution against the same deadline and the response receiver closing; cancellation drops the run future, drops values, closes the invocation and returns or replaces its slot. A deadline bounds cooperative async work and client waiting, not arbitrary blocking trusted native code. Filesystem/process/stdin/printing calls remain synchronous as in 0163; this is not a sandbox or hard native-code preemption promise.

## 4. Workers, retirement and shutdown

W OS threads each own a current-thread Tokio runtime and one LocalSet, four slots and a queue of sixteen owned request jobs. Only Rust-owned request/response data crosses threads; Rune Values are built, used and destroyed on their worker. Exclusive slot ownership prevents shared active generations. The acceptor distributes to an available bounded queue, otherwise 503; no unbounded retry queue or request spawn.

Close runs on success, ordinary error, invalid conversion, cancellation and timeout. A cleanup failure returns no slot: record/log retirement, send that request 500 where still possible, and construct a fresh Program::slot with identical trusted extension registrations before admitting another request on that slot. Other active slots keep running. Failed replacement is a host failure, not silent pool shrinkage or an infinite retry loop: stop admission and shut down nonzero. Native Rust unwinds are caught at the worker job boundary with AssertUnwindSafe around the polled local execution future. This is a disposal boundary, not permission to resume poisoned state: the future, invocation and its slot are dropped/retired, no VM or request-owned state from that slot is resumed/reused, and replacement constructs a new owner. Other slots have independent owners; intentional shared external/native extension state remains outside the isolation guarantee, as in 0163. The same replacement rule applies. Abort-on-panic or double-panic builds cannot make that guarantee.

SIGINT/SIGTERM (and Ctrl-C on supported platforms) stop acceptance and new queue admission; queued jobs receive 503. Connections get graceful shutdown (no new keep-alive requests). Already-active jobs drain within the grace period; on expiry their futures are cancelled and all available owners explicitly closed. Workers and connection tasks are joined; the separately bounded logging thread has the explicit detach exception below. Tests distinguish this cooperative guarantee from synchronous trusted/native calls, which can delay a join; no finite wall-clock promise is made for an uninterruptible native poll. A second signal is an explicitly reported immediate process termination, with no claim that destructors ran.

Request logging is bounded, terminal-safe JSON to stderr (method, clipped raw path without query, response status/outcome and duration; no bodies/headers/secrets). Retirement and replacement counters/events are separate. Logging cannot block the acceptor or a Rune worker behind an undrained stderr pipe: one dedicated logger thread performs ordinary blocking writes to a duplicated stderr handle; producers try_send into a channel bounded to 256 records, each at most 4 KiB, and a full/disconnected channel increments the dropped-log counter. Do not change fd 2 flags, including O_NONBLOCK: its open file description can be shared with a parent or supervisor. The logger owns only Rust log data, never Rune Values, slots or request futures. Shutdown disconnects producers and waits at most 100 ms for the logger, then detaches a still-blocked thread. No unbounded queue or blocked logger join. The logger emits the dropped counter in a best-effort shutdown summary after draining; a permanently blocked sink can prevent delivery, which must not be described as complete logging. Controls assert accounting independently of that delivery. Native script output retains its ordinary synchronous I/O caveat; --log off suppresses request logs but still enqueues bounded failure diagnostics. Benchmarks report the log policy rather than silently disabling a default.

## 5. First application, outside this repository

Prepare the existing skeleton in its own empty repository with only Rune source, README and a check script. No Cargo launcher or per-application Rust code. The README pins installation with cargo install --git <rnx repository> --rev <accepted implementation revision> --locked, then rnx serve site.rn. Validate all 0162 fixtures against this fetched, installed stock binary. Its first local commit is made reviewable, then ask the user before the first push to that public repository. A revision that cannot be fetched is a named blocker, not replaced with an undisclosed local path. No deployment is included.

## 6. Gates and measurements

Controls exercise the actual command/host, not only a copied prototype:

- option/feature/bind errors before source opening; compile diagnostics with original path/line; bind collision; ready only after all slots; IPv4 and IPv6 loopback;
- all 26 frozen 0162 wire fixtures through the old Rune program, and three complete responses on one keep-alive socket;
- strict body/header/response limits, streamed and declared oversize, malformed responses, HEAD/204/304, full queues/admission, expired queued work and client disconnect;
- instruction exhaustion and async timeout followed by reuse; an injected tracked destructor failure under concurrent load retires exactly one slot, replacement occurs, the worker survives and other slots complete; replacement-construction failure shuts down visibly;
- graceful signal drain, queued refusal, deadline cancellation, joins/owner cleanup and idle keep-alive shutdown; no-background-task/lifecycle contamination on reuse;
- default logging, hostile path/diagnostic text, a deliberately undrained pipe (bounded memory, service progress, dropped-log accounting, shutdown), parent pipe flags unchanged (fcntl F_GETFL before/after), bounded logger detach and internal drop accounting, and disabled request logs retaining bounded failure diagnostics;
- unchanged 0163 and existing server-runtime suites; root/test-support, feature-off builds, CLI help/version/run/worker/session gates, formatting and diff checks; signed subject-only commits.

A retained rnx-bench driver runs a **current matched** axum-only, product rnx serve and Flask/gunicorn comparison, using the unchanged 0162 content/fixtures and fail-closed load gates. Same CPUs, two workers, three interleaved repetitions, three-second warm-up, ten-second samples, / and /hello/world at c=1/32 keep-alive, plus hello c=1 without keep-alive. Retain complete oha JSON, exit status, connection samples, source/binary/tool/lockfile hashes and response fixture results. All accepted timed responses are 200 with no errors, finite positive fields and expected keep-alive occupancy. Harness failure/corruption controls remain fatal.

Report request logging configuration and cost explicitly: the throughput comparison disables per-request logs for all three systems; a separate matched product sample measures default logging to a drained file. Also measure ready-to-serve startup (excluding fixture round trips), slot pool cost, process-tree RSS and stock binary size. The default HTTP feature is gated by 0068's stock-startup budget: matched release baseline at f86c714 versus product, one pinned core, five warm-ups and 100 randomized interleaved samples per command per binary in two repeats for version, eval 42, run of the same JSON script and PTY create-to-first-prompt; 200 persistent prompt cells per binary per repeat. A median regression above 5% that reproduces in both repeats fails the gate; report medians and p10-p90 spreads, retain sample journals. A failure stops and comes back to review; the feature is not silently removed from defaults and the budget is not loosened. No Flask/axum wording until these contemporaneous results exist; no performance number is promised. Instruction budgets, admission bounds, logging and ownership differences are disclosed beside results. The product default is 2,000,000 instructions versus the 10,000,000 used in 0162/0163; the comparison runs the product default and says so.

## 7. Out of scope

Route tables/path parameters, web helpers and string API additions, disk static-file serving, templates, cookies/sessions, databases, reload-on-edit, deployment, TLS/HTTP2/proxy trust, and tuning or changing the skeleton workload. Follow-up ranking is based on the first application's observed friction, not additional framework features built speculatively.

## 8. Reviewed follow-up path

After this host and the unchanged site validate, a Rune route table with parameters and explicit HEAD/405/allow rules is the next usability cut, then small web helpers for escaping and URL/form decoding (and measured string gaps). These are separate reviewed records, ranked again if the real application exposes a different blocker. They are the path toward Flask-level terseness, not functionality claimed by 0164.

**Plan review amendments:** R1 replaces nonblocking stderr mutation with a bounded logger thread and bounded detach; R2 turns stock launch into the existing 5% regression gate; R3 removes the application Cargo launcher. No implementation or site changes preceded these amendments.

**Implementation boundary clarification (reviewed during implementation):** `Extensions`
contains non-Send, one-shot builders, not repeatable registrations. Stock serve
reproduces `Extensions::none()` for compilation, workers and replacements. A
`main_with` executable supplying named extensions is refused before source
opening, naming the extensions, the one-shot reason and stock rnx as the way
forward. The private worker factory permits native failure controls, but adds no
public API. Adapter-enabled serving requires a separately reviewed repeatable,
thread-safe factory contract, ranked by the application's needs.
