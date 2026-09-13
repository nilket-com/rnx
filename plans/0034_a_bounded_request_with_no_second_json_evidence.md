# rnx 0034 implementation evidence

Measured on nano, Linux, Intel i7-14700, 2026-09-13, Rust 1.98.1,
Rune 0.14.2, reqwest 0.12.28, Tokio 1.53.1. Measurements precede the implementation commit.
The initial DNS shutdown stop condition is resolved under the revised
decision 3: shutdown does not wait for native DNS, and does not cancel it.
No Windows execution is claimed.

## Initial stop condition: started blocking DNS outlives the deadline

Hyper-util 0.1.20 `src/client/legacy/connect/dns.rs`, `GaiResolver::call`,
uses `tokio::task::spawn_blocking` around system address resolution.
`GaiFuture::drop` aborts the join handle. Tokio 1.53.1
`src/task/blocking.rs` documents that started blocking work cannot be
aborted, and runtime shutdown waits for it. The blocking pool creates
unowned tasks, outside the async scheduler's `num_alive_tasks` count.

The standalone rnx-bench probe `probes/http-dns-shutdown` uses that same
blocking-task and abort-on-drop mechanism, substituting a controlled
two-second sleep for system DNS. It does not issue any DNS or public-network
request. Results in `results/http_0034_dns_shutdown.txt`:

| observation | elapsed |
| --- | --- |
| 50 ms request timeout returns, `is_timeout() == true` | 51.547 ms |
| client dropped, runtime driven; async task count is zero | 62.872 ms |
| runtime destruction completes | 2000.809 ms |

This proves a mechanism defect in the proposed bound, not that real DNS
took two seconds on nano. Zero async tasks and socket EOF cover the
connection work but do not cover started system resolver work. The
implementation initially stopped under guardrail 1. The revised decision
keeps system name-service compatibility and explicitly permits a started
lookup to linger. The runtime owner now calls `shutdown_background` on
drop at every entry point, including error paths. The runner no longer
turns a completed script into a failure because of a post-run drain.
Sessions still abort HTTP work, discard the client, and drain async tasks
after cancellation or reset; blocking resolver work is the documented
exception. It can retain native resources and more than one lookup can
linger. No universal native-resolver deadline is promised.

The final test-support resolver uses a two-second blocking sleep for one
fixture name, without real DNS. With a 100 ms request deadline, the gate
requires completion within 355 ms (deadline plus 255 ms scheduling
allowance). Raw output: rnx-bench `results/http_0034_final_gates.txt`.

| observation | elapsed |
| --- | --- |
| `run` reports deadline and process exits | 115.876 ms |
| `eval` reports deadline and process exits | 121.562 ms |
| session reports deadline, runs next input, resets and exits | 108.635 ms |

These are debug test-build observations of non-waiting shutdown, not proof
of DNS cancellation. The session measurement starts after its first prompt.

## Implemented surfaces and loopback evidence

`src/http.rs` registers exactly `http::get`, `get_bytes`, `request`, and
`request_bytes`, gated by a unit test at registration. Reqwest is used
directly with default features off and only rustls-tls and gzip enabled.
JSON parsing and writing remain the existing host functions; the serializer
and JSON reader are unchanged.

HTTP requests run in tracked Tokio tasks. A guard aborts work when an
unretained Rune future drops. Cancellation/reset abort all tracked work,
discard the client, and drain the runtime, including when a prior input's
binding retains the interrupted Rune future. This is additional ownership
for HTTP work; VM budget behavior is unchanged. An edit interrupted at the
prompt clears HTTP state too.

Nine `tests/http.rs` tests exercise loopback fixtures with all uppercase
and lowercase proxy variables removed, except the explicit proxy gate.
The command helper sets `TERM=dumb` for deterministic piped prompts. Both
full suites were invoked with `TERM=xterm-kitty` to exercise that isolation:

- GET text and bytes, generic POST/PUT/PATCH/DELETE bodies, HEAD, lowercase
  header names and repeated Set-Cookie values, ordinary 404/500 responses.
- The u64 JSON reader path, malformed/deep JSON refusal, strict UTF-8 and
  bytes-preserving behavior. No companion JSON reader is registered.
- Delayed headers, partial body, and a declared-but-unsent body time out.
  These use 100 ms deadlines and a 355 ms scheduling allowance bound,
  against a fixture read timeout of five seconds. Chunked and decoded gzip
  bodies obey the byte cap; the exact chunked-body boundary is accepted.
- A declared 1 GiB followed by ten bytes and EOF reports an incomplete
  body. A test-support allocator peak probe stays below 32 MiB, rather
  than allocating the declared size. The peak hooks are absent from an
  ordinary build. Invalid ranges, unknown options and a malformed header
  produce no accepted server connection.
- Redirect completion, excess redirects, credential stripping between
  fixture ports, deterministic injected DNS failure, malformed URL,
  refused connection, and explicit proxy routing. The stalled-DNS exit gate is described above.
- A self-signed certificate is refused; TLS to a plaintext peer also
  fails. The test certificate/key are intentionally public fixture data.
- File and REPL entry points execute GET; named help describes all four
  functions. Other method/response cases run through eval.
- Two successful session requests reuse one socket. A hanging request to
  a different fixture is saved as a future, then awaited and interrupted.
  Both healthy and cancelled sockets report clean `Ok(0)` EOF, distinct
  from read errors. The next request succeeds on a fresh connection;
  reset closes it and the following request builds another.

## Suite results

```text
TERM=xterm-kitty cargo test --locked                          253 passed, 0 failed
TERM=xterm-kitty cargo test --locked --features test-support  290 passed, 0 failed
cargo build --release --locked                passed
scripts/third-party-notices.sh --check         passed
```

Notices were regenerated: 121 packages, 87 texts, 13 fetched, one unresolved
item, the same unresolved item as before. New dependency licence texts are
included. Generated notices preserve upstream whitespace; diff whitespace
checking excludes that generated file. Product-source whitespace checks
pass.

## Startup, allocation and size

The before artifact is the release executable preserved from record 0033
before implementation; the after artifact is the locked release build of
this worktree. Their SHA-256 hashes are respectively
`d8d78d10954493a451fc90132e44d9430398c5c006a061585167ac2c839a4faf`
and `7165be726c82f00870d968c976897e8b1cdbe14547e5b5055f0a4a34959da5dc`.
Bench scripts remain `scripts/bare.rn` and `scripts/json.rn`
in rnx-bench. The JSON workload stdout is byte-identical, with exit 0 before
and after. No serializer code changed.

Raw exports: rnx-bench `results/http_0034_startup.json`, taskset core 4,
hyperfine `-N`, ten warmups, fifty runs per command. Milliseconds,
mean ± standard deviation:

| command | before | after |
| --- | --- | --- |
| version | 0.553 ± 0.084 | 0.523 ± 0.019 |
| help | 0.531 ± 0.068 | 0.524 ± 0.023 |
| eval 42 | 3.957 ± 0.067 | 3.943 ± 0.018 |
| bare run | 3.616 ± 0.016 | 3.615 ± 0.029 |
| JSON loop | 11.632 ± 0.191 | 11.619 ± 0.084 |

The binary grows from 9,880,504 to 14,142,008 bytes, about 4.06 MiB. Linux
dynamic dependencies remain libc, libm and libgcc plus the loader; there is
no system TLS link. This is not a cross-platform linking measurement.

Before the DNS-shutdown revision, fresh piped-session allocation startup
references were 1,798,786 and
1,813,884 bytes, an increase of 15,098 bytes. Separately, in the HTTP reset
probe, live allocation was 1,822,330 bytes before HTTP, 1,822,588 after the
first request/reset, and 1,822,583 after the second. These are allocator
request counts, not RSS or exact equality to startup. No progressive pool
retention is evident across those two cycles; the direct socket-closure
gates are stronger evidence. Raw output is `results/http_0034_memory.txt`.

The registration-only probe `probes/http-registration` imports this
worktree's actual HTTP module. Across 100 release in-process constructions,
with destruction excluded and core 4 pinned, it measured 2.164 ms default
context construction and 0.006 ms for the four-function HTTP installation.
It has its own lockfile; it is not the matched whole-rnx measurement above.
No client is constructed during registration. This is much smaller than
the earlier roughly 0.4 ms companion-module installation report, but the
probes differ and the numbers are not an isolated comparison of identical
builds. Output is `results/http_0034_registration.txt`.

Request reuse is socket-counted. The final debug fixture run measured
2.951 ms for the first request through the next prompt and 42.111 ms for
the reused request, with exactly one accepted socket. This single sample
includes compilation, prompt handling and fixture behavior; it establishes
no latency advantage. Raw output is `results/http_0034_final_gates.txt`.

The HTTP gates are complete under the revised native-DNS exception.
Record 0031's separate CPU-interruption clause remains open; this HTTP
work does not change the VM budget or make running async Rune code
preemptible. No outward-facing report was filed, no signing configuration
was changed, and nothing was pushed. The record and implementation are committed separately, plan then
implementation, using the configured SSH signing key.
